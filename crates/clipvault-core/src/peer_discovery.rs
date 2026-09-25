//! Local peer discovery: opt-in mDNS browse + metadata-only merge.
//!
//! The `local-peer-discovery` change introduces a continuous
//! `_clipvault._tcp.local` browser that records every other
//! ClipVault installation visible on the same link. The discovery
//! surface is metadata-only by construction:
//!
//! - TXT records carry `peer_id`, public key fingerprint, display
//!   name, protocol major version and capability. They NEVER
//!   carry clipboard content, previews, hashes, source
//!   identifiers, file paths or secrets.
//! - SQLite persists the validated metadata in `known_peers`. IP
//!   addresses and ports are deliberately absent: an endpoint is a
//!   runtime detail the discovery event surface carries for the
//!   current browser session and never promotes to identity.
//! - The `Equipos` view renders the persisted metadata plus the
//!   presence state the runtime derives from events. Pairing /
//!   trust / history remain in later changes.
//!
//! ## Architecture
//!
//! - [`PeerDiscoveryAdapter`] is the platform-neutral trait the core
//!   uses to receive events. The production adapter is the
//!   `mdns-sd`-backed implementation in `clipvault-platform`
//!   (`mdns::MdnsPeerDiscoveryAdapter`); tests inject an in-process
//!   fake.
//! - [`PeerDiscoveryRuntime`] is the in-process state machine the
//!   shell drives: it owns the `PeerDiscoveryAdapter`, deduplicates
//!   events, validates identity records, applies self-filtering and
//!   persists validated observations through
//!   [`crate::peer_discovery::core`].
//! - [`PeerObservationValidator`] is the single point that
//!   sanitises an event payload and surfaces the typed
//!   [`PersistenceError`] variants. Validation MUST be deterministic
//!   and conservative — the spec scenario "Malformed or conflicting
//!   announcement" pins the ignore-safely contract.
//! - A single worker thread (spawned on `start`, joined on `stop`)
//!   drains the event queue and persists observations through the
//!   `apply_observation` closure the bootstrap installs. The worker
//!   is the only consumer of the queue; the production adapter
//!   pushes metadata-only events through [`DiscoverySink`].
//!
//! The shell holds the `PeerDiscoveryRuntime` behind an `Arc`, calls
//! `start` / `stop` idempotently and surfaces a metadata-only
//! snapshot through the Tauri commands the `clipvault-peer-sharing-*`
//! bridge exposes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{debug, warn};

use clipvault_db::{KnownPeer, PeerObservation, UpsertObservationOutcome};
pub use clipvault_platform::peer_discovery::{
    AdapterError, DiscoveryAdvertisement, DiscoveryEvent, DiscoverySink, PeerDiscoveryAdapter,
};
use clipvault_platform::peer_identity::{PeerFingerprint, PeerId};

/// Maximum length of the validated visible name the TXT record
/// publishes. Mirrors `MAX_PEER_DISPLAY_NAME_LENGTH` from the
/// settings layer so the value cannot blow up the mDNS payload.
pub const MAX_PEER_DISPLAY_NAME_LENGTH: usize = 64;

/// Current wire-protocol major version. The runtime only persists
/// observations whose `protocol_major` matches `PROTOCOL_MAJOR` so
/// the `Equipos` view never surfaces an incompatible peer.
pub const PROTOCOL_MAJOR: i64 = 1;

/// Capability advertised in every TXT record this change owns.
/// The `local-peer-mutual-pairing` change ships its own
/// `pairing` capability for the productive path; the
/// discovery-only capability stays valid for every peer that has
/// not yet trusted the remote listener. The runtime rejects
/// every other capability at validation time.
pub const DISCOVERY_ONLY_CAPABILITY: &str = "discovery_only";

/// Capability the productive pairing change advertises when the
/// local peer has a real ephemeral port bound and the mDNS TXT
/// record additionally carries the full public-key fingerprint
/// the pairing listener pins during the mTLS handshake. The
/// constant lives in the discovery module so the validation path
/// and the bridge can share a single string.
pub const PAIRING_CAPABILITY: &str = "pairing";

/// Capability the `peer-image-import` change ships. A peer that
/// advertises `image_import` accepts the productive
/// `list_recent_images` and `fetch_image` envelopes the
/// `peer-image-import` bridge uses; a peer that does not
/// announce the capability continues to expose only the text
/// endpoints so the gating UI surfaces the absence. The
/// capability lives in the discovery module so the validation
/// path and the bridge can share a single string.
///
/// The capability is announced alongside `pairing` (the wire
/// preserves a single `capability` field) by separating the
/// values with a comma: `pairing,image_import`. The discovery
/// validator splits the field, recognises each token
/// independently and rejects unknown tokens so a future
/// capability addition can opt in without breaking the
/// existing surface.
pub const IMAGE_IMPORT_CAPABILITY: &str = "image_import";

/// Decode the comma-separated `capability` field into a
/// normalised `Vec<String>`. Empty / whitespace-only tokens
/// are dropped so a TXT record with `pairing,` decodes the
/// same way as one with `pairing`. The order is preserved so
/// tests can pin the value verbatim when they need to.
pub fn decode_capabilities(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

/// Service type the runtime browses / registers. The trailing dot
/// is intentional: `mdns-sd` treats it as a fully-qualified name
/// (RFC 6762 §3).
pub const SERVICE_TYPE: &str = "_clipvault._tcp.local.";

/// Number of lowercase hex characters the `peer_id` MUST carry.
/// Mirrors the private `PEER_ID_HEX_CHARS` constant the platform
/// identity module uses; the discovery runtime duplicates the
/// value here so it can validate TXT records without exposing the
/// platform-internal constant.
pub const PEER_ID_HEX_CHARS: usize = 32;

/// Number of lowercase hex characters the full public-key
/// fingerprint MUST carry. The pairing advertisement carries the
/// full SHA-256 of the Ed25519 public key in this projection so
/// the pairing runtime never has to derive it from the 16-char
/// short fingerprint the discovery-only advertisement advertises.
pub const FULL_FINGERPRINT_HEX_CHARS: usize = 64;

/// Identifier of the local peer the runtime must filter out before
/// persisting. The runtime compares the TXT `peer_id` against this
/// value and silently ignores a self-match so the local row never
/// appears in its own `Equipos` snapshot.
///
/// The snapshot also carries the trimmed visible name the runtime
/// publishes in its own TXT record so the discovery adapter does
/// not have to re-resolve the identity foundation at `start` time.
/// The runtime rejects an empty / over-long name at construction
/// time so the adapter cannot accidentally publish an invalid
/// record.
#[derive(Debug, Clone)]
pub struct LocalPeerIdentitySnapshot {
    pub peer_id: PeerId,
    pub fingerprint: PeerFingerprint,
    pub display_name: String,
}

impl LocalPeerIdentitySnapshot {
    /// Build a snapshot from the platform-neutral identity the
    /// identity foundation mints. Tests construct one directly
    /// when they bypass the secure store.
    pub fn new(peer_id: PeerId, fingerprint: PeerFingerprint, display_name: String) -> Self {
        Self {
            peer_id,
            fingerprint,
            display_name,
        }
    }
}

impl LocalPeerIdentitySnapshot {
    /// Returns `true` when the supplied observation matches the
    /// local identity. The runtime uses the helper as a
    /// `self-filter`: a service record that claims our own
    /// `peer_id` (a) is the local announcement and (b) is ignored
    /// before reaching persistence. The comparison is exact on
    /// `peer_id` and `fingerprint` so a peer reusing an existing
    /// `peer_id` with a different fingerprint still reaches the
    /// conflict-detection branch.
    pub fn matches(&self, observation: &PeerObservationRecord) -> bool {
        observation.peer_id == self.peer_id.as_str()
            && observation.public_key_fingerprint == self.fingerprint.as_str()
    }
}

/// Validated, runtime-shaped observation the [`PeerDiscoveryRuntime`]
/// builds from a raw event before persisting. The struct mirrors
/// [`PeerObservation`] but stays typed at the `peer_id` /
/// `fingerprint` level and carries the observation timestamp so the
/// presence TTL can be evaluated without a second database read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerObservationRecord {
    pub peer_id: String,
    pub public_key_fingerprint: String,
    /// Full SHA-256 of the peer's Ed25519 public key (64 hex
    /// chars). The pairing change populates the field when the
    /// incoming advertisement declares `capability = pairing`;
    /// the discovery-only path leaves it `None`. The runtime
    /// never synthesizes the value from the short fingerprint —
    /// the full digest is only ever set by the productive
    /// advertisement so the pairing runtime can build the
    /// canonical `OutboundSessionDescriptor` from a row that was
    /// only ever seen over pairing-capable mDNS.
    pub full_public_key_fingerprint: Option<String>,
    pub display_name: String,
    pub protocol_major: i64,
    /// Canonical `capability` token the host advertises. Stays
    /// at `pairing` / `discovery_only` exactly so a legacy
    /// client that only accepts the exact literal keeps
    /// recognising the record; the runtime persists the value
    /// verbatim in `known_peers.capability` so the bootstrap
    /// resolver can combine it with the additive
    /// [`Self::caps_extra`] tokens when resolving the peer's
    /// capabilities.
    pub capability: String,
    /// Additive capability tokens the host advertises through
    /// the separate `caps_extra` TXT field. The field is
    /// `Vec<String>` so a host that supports several additive
    /// surfaces (image_import, future rich_text_share, …) can
    /// publish them in a single TXT key without breaking the
    /// legacy contract.
    pub caps_extra: Vec<String>,
    pub observed_at: OffsetDateTime,
}

impl PeerObservationRecord {
    /// Validate a raw TXT-record payload against the contract the
    /// design pins. Returns either the canonicalised observation or
    /// a typed [`PeerRecordValidationError`] so the runtime can
    /// branch on the reason without inspecting the rejected bytes.
    pub fn from_txt_record(raw: &TxtRecord) -> Result<Self, PeerRecordValidationError> {
        if raw.peer_id.is_empty() {
            return Err(PeerRecordValidationError::MissingPeerId);
        }
        if raw.peer_id.len() != PEER_ID_HEX_CHARS {
            return Err(PeerRecordValidationError::MalformedPeerId);
        }
        if !raw
            .peer_id
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(PeerRecordValidationError::MalformedPeerId);
        }
        if raw.public_key_fingerprint.is_empty() {
            return Err(PeerRecordValidationError::MissingFingerprint);
        }
        if !raw
            .public_key_fingerprint
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(PeerRecordValidationError::MalformedFingerprint);
        }
        let display_name = raw.display_name.trim().to_string();
        if display_name.is_empty() {
            return Err(PeerRecordValidationError::MissingDisplayName);
        }
        if display_name.chars().any(is_invalid_display_char) {
            return Err(PeerRecordValidationError::InvalidDisplayName);
        }
        if display_name.chars().count() > MAX_PEER_DISPLAY_NAME_LENGTH {
            return Err(PeerRecordValidationError::DisplayNameTooLong);
        }
        if raw.protocol_major != PROTOCOL_MAJOR {
            return Err(PeerRecordValidationError::IncompatibleProtocol {
                observed: raw.protocol_major,
            });
        }
        // Decode the canonical `capability` field. The legacy
        // contract pins the value to a single token
        // (`discovery_only` or `pairing`) so a strict parser
        // that does not understand the additive surface keeps
        // recognising the record. The runtime rejects any
        // comma-separated additional token in the legacy field
        // so a host that accidentally publishes the additive
        // capability in the wrong field is rejected as
        // `UnsupportedCapability` instead of silently bypassing
        // the capability gate.
        let tokens = decode_capabilities(&raw.capability);
        if tokens.is_empty() {
            return Err(PeerRecordValidationError::UnsupportedCapability {
                capability: raw.capability.clone(),
            });
        }
        let primary = tokens[0].as_str();
        if tokens.len() > 1 {
            // The legacy `capability` field is reserved for a
            // single token (`pairing` / `discovery_only`). A
            // host that publishes additive tokens in the wrong
            // field is rejected so the strict-legacy-parser
            // contract stays consistent across the runtime.
            return Err(PeerRecordValidationError::UnsupportedCapability {
                capability: raw.capability.clone(),
            });
        }
        match primary {
            DISCOVERY_ONLY_CAPABILITY => {}
            PAIRING_CAPABILITY => {
                let full = raw.pairing_fingerprint.as_deref().unwrap_or("");
                if full.len() != FULL_FINGERPRINT_HEX_CHARS
                    || !full
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                {
                    return Err(PeerRecordValidationError::MissingPairingFingerprint);
                }
            }
            _ => {
                return Err(PeerRecordValidationError::UnsupportedCapability {
                    capability: raw.capability.clone(),
                });
            }
        }
        // Validate the additive `caps_extra` surface. Only the
        // tokens the runtime currently understands are
        // accepted; unknown tokens surface as
        // `UnsupportedCapability` so a future capability
        // addition cannot silently bypass the capability gate
        // when the host upgrades before the runtime does.
        for token in &raw.caps_extra {
            if !matches!(token.as_str(), IMAGE_IMPORT_CAPABILITY) {
                return Err(PeerRecordValidationError::UnsupportedCapability {
                    capability: format!("caps_extra:{}", token),
                });
            }
        }
        let full_public_key_fingerprint = if primary == PAIRING_CAPABILITY {
            raw.pairing_fingerprint.clone()
        } else {
            None
        };
        Ok(Self {
            peer_id: raw.peer_id.clone(),
            public_key_fingerprint: raw.public_key_fingerprint.clone(),
            full_public_key_fingerprint,
            display_name,
            protocol_major: raw.protocol_major,
            capability: raw.capability.clone(),
            caps_extra: raw.caps_extra.clone(),
            observed_at: raw.observed_at,
        })
    }

    /// `true` when the record was published by a peer that
    /// opts into the `peer-image-import` capability. The check
    /// runs against the additive `caps_extra` surface the
    /// runtime reads from the dedicated TXT field; the legacy
    /// `capability` field never carries the token so a strict
    /// parser that only accepts the canonical value keeps
    /// pairing without seeing the additive surface. The
    /// persisted `caps_extra` column stays empty for legacy
    /// rows so no schema migration is required.
    pub fn has_image_import_capability(&self) -> bool {
        self.caps_extra
            .iter()
            .any(|token| token == IMAGE_IMPORT_CAPABILITY)
    }

    /// Convert into the database-shaped [`PeerObservation`]. The
    /// runtime calls this immediately before persistence so the
    /// repository never sees unvalidated bytes. The additive
    /// `caps_extra` surface travels through verbatim so the
    /// bootstrap resolver can combine it with the canonical
    /// `capability` value when it gates the productive image
    /// routes.
    pub fn into_persistence(self) -> PeerObservation {
        PeerObservation {
            peer_id: self.peer_id,
            public_key_fingerprint: self.public_key_fingerprint,
            full_public_key_fingerprint: self.full_public_key_fingerprint,
            display_name: self.display_name,
            protocol_major: self.protocol_major,
            capability: self.capability,
            caps_extra: self.caps_extra.join(","),
            observed_at: self.observed_at,
        }
    }
}

/// Raw TXT record payload the runtime receives from the platform
/// adapter. Re-exported from [`clipvault_platform::peer_discovery::TxtRecord`]
/// so the core can rely on the metadata-only contract without
/// duplicating the shape — the adapter pushes these records into
/// the runtime through a [`DiscoverySink`].
pub type TxtRecord = clipvault_platform::peer_discovery::TxtRecord;

/// Typed validation errors the runtime classifies before
/// discarding a malformed / conflicting announcement. The variants
/// are stable identifiers the bridge / frontend surfaces; the
/// messages never carry clipboard content, hashes or other
/// payload-shaped data.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerRecordValidationError {
    #[error("TXT record is missing peer_id")]
    MissingPeerId,
    #[error("TXT record peer_id is malformed")]
    MalformedPeerId,
    #[error("TXT record is missing fingerprint")]
    MissingFingerprint,
    #[error("TXT record fingerprint is malformed")]
    MalformedFingerprint,
    #[error("TXT record is missing display_name")]
    MissingDisplayName,
    #[error("TXT record display_name contains invalid characters")]
    InvalidDisplayName,
    #[error("TXT record display_name exceeds the published length")]
    DisplayNameTooLong,
    #[error("TXT record protocol major {observed} is incompatible with PROTOCOL_MAJOR")]
    IncompatibleProtocol { observed: i64 },
    #[error("TXT record capability {capability:?} is not supported by this build")]
    UnsupportedCapability { capability: String },
    #[error("TXT record pairing advertisement is missing the full public-key fingerprint")]
    MissingPairingFingerprint,
}

fn is_invalid_display_char(c: char) -> bool {
    c.is_control() || matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}')
}

/// Outcome of a single observation event. The runtime returns one
/// of these variants to the platform adapter / the test suite so
/// callers can branch on the reason without inspecting free-form
/// strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationOutcome {
    /// The observation was self-filtered because the TXT record
    /// matched the local identity. Persisted state is unchanged.
    SelfFiltered,
    /// The observation passed validation but the repository
    /// rejected it as a conflict (different fingerprint / display
    /// name / protocol / capability reusing the same `peer_id`).
    /// The persisted row stays untouched.
    Conflict(KnownPeer),
    /// The observation was persisted (new row or merge).
    Stored(KnownPeer),
    /// The TXT record failed validation. Persisted state is
    /// unchanged. The variant carries the typed reason the
    /// runtime / diagnostics surface.
    Rejected(PeerRecordValidationError),
}

/// Snapshot of the persisted peer list merged with the in-memory
/// presence state the runtime derives from mDNS events. The struct
/// is the metadata-only DTO the `Equipos` view renders; it never
/// carries the TXT payload, the runtime identity or the local
/// identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PeerSnapshotEntry {
    pub peer_id: String,
    pub public_key_fingerprint: String,
    pub display_name: String,
    pub protocol_major: i64,
    pub capability: String,
    /// Additive capability tokens observed in the `caps_extra` TXT
    /// field. The renderer needs these independently of the legacy
    /// `capability` token to gate image import correctly.
    pub caps_extra: String,
    /// Persisted trust state, projected as its stable wire string.
    /// Discovery owns presence; pairing owns this independent
    /// relationship state. Including it in the same metadata-only
    /// snapshot lets the shell reconcile an asynchronously completed
    /// mutual approval without receiving a raw transport event.
    pub trust_state: String,
    /// RFC 3339 timestamp of a successful reciprocal approval, if
    /// the peer has been paired. Empty / absent pairing state stays
    /// `None` instead of inventing a timestamp for unverified peers.
    pub paired_at: Option<String>,
    pub first_seen_at: String,
    pub last_discovered_at: String,
    /// `true` when the platform adapter has reported a
    /// `ServiceResolved` for this `peer_id` and has not yet
    /// received a `ServiceRemoved` (or a bounded DNS-SD
    /// verify that flipped the cache to removed). `false`
    /// otherwise. The flag no longer depends on a wall-clock
    /// TTL — the platform adapter owns liveness.
    pub is_present: bool,
    /// Stable discriminator the UI branches on. Mirrors
    /// [`PeerPresence::as_str`] so the frontend can render the
    /// matching copy without parsing free-form strings.
    pub presence: PeerPresence,
}

impl PeerSnapshotEntry {
    fn from_row(row: KnownPeer, is_present: bool) -> Self {
        let presence = if is_present {
            PeerPresence::Detected
        } else {
            PeerPresence::NotAvailable
        };
        Self {
            peer_id: row.peer_id,
            public_key_fingerprint: row.public_key_fingerprint,
            display_name: row.display_name,
            protocol_major: row.protocol_major,
            capability: row.capability,
            caps_extra: row.caps_extra,
            trust_state: row.trust_state.as_str().to_string(),
            paired_at: (!row.paired_at.is_empty()).then_some(row.paired_at),
            first_seen_at: row.first_seen_at,
            last_discovered_at: row.last_discovered_at,
            is_present,
            presence,
        }
    }
}

/// Presence discriminator the UI renders in the `Equipos` view.
/// `Unverified` is reserved for future pairing states; the
/// discovery surface only mints `Detected` and `NotAvailable`
/// today, but the enum stays open so the next change does not have
/// to refactor the wire contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerPresence {
    /// The platform adapter reported a `ServiceResolved` for this
    /// peer and the bounded DNS-SD liveness confirmation has not
    /// yet pushed it back to the removed state. The frontend MUST
    /// offer only "Vincular" — never "Ver historial" until the
    /// pairing change lands.
    Detected,
    /// Persisted historically but the adapter has not seen this
    /// peer in the current browser session (or the bounded DNS-SD
    /// verify confirmed it is gone). The UI renders the peer but
    /// disables every action: there is no reachable identity to
    /// verify.
    NotAvailable,
    /// Reserved for the future pairing change. Today the runtime
    /// never emits it; the variant exists so the wire contract
    /// stays stable across changes.
    Unverified,
}

impl PeerPresence {
    /// Stable snake_case string the bridge / frontend uses as a
    /// discriminator. The value MUST stay in sync with
    /// `frontend/src/types.ts`.
    pub fn as_str(self) -> &'static str {
        match self {
            PeerPresence::Detected => "detected",
            PeerPresence::NotAvailable => "not_available",
            PeerPresence::Unverified => "unverified",
        }
    }
}

/// Outcome of [`PeerDiscoveryRuntime::snapshot`]. The variant is
/// the wire contract the `clipvault_peer_snapshot` command
/// returns: the frontend never has to inspect the inner list
/// without first confirming the typed state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PeerSnapshot {
    pub entries: Vec<PeerSnapshotEntry>,
    /// Whether the runtime is currently browsing the LAN. The
    /// frontend reads the flag to decide whether the toggle is
    /// visually "active".
    pub sharing_active: bool,
    /// Free-form reason the runtime is not browsing. The string
    /// never carries platform detail (no keychain / D-Bus
    /// messages); only the documented `identity_unavailable`,
    /// `runtime_stopped`, `disabled` values the bridge can branch
    /// on.
    pub sharing_inactive_reason: Option<String>,
}

impl PeerSnapshot {
    /// Convenience used by tests to build an `active` snapshot.
    pub fn active(entries: Vec<PeerSnapshotEntry>) -> Self {
        Self {
            entries,
            sharing_active: true,
            sharing_inactive_reason: None,
        }
    }

    /// Convenience used by tests to build an inactive snapshot.
    pub fn inactive(reason: impl Into<String>) -> Self {
        Self {
            entries: Vec::new(),
            sharing_active: false,
            sharing_inactive_reason: Some(reason.into()),
        }
    }
}

/// Reason the runtime is currently inactive. The string is part of
/// the wire contract the `Equipos` view branches on.
pub const RUNTIME_INACTIVE_REASON_IDENTITY_UNAVAILABLE: &str = "identity_unavailable";
pub const RUNTIME_INACTIVE_REASON_RUNTIME_STOPPED: &str = "runtime_stopped";
pub const RUNTIME_INACTIVE_REASON_DISABLED: &str = "disabled";

/// Outcome of a single browse iteration the adapter delivers. The
/// runtime receives both shapes through the same callback so the
/// browser can stay in one place. The shape is re-exported from
/// `clipvault_platform::peer_discovery::DiscoveryEvent` (see the
/// file-level `pub use` above).
///
/// Snapshot of the in-memory presence the runtime maintains per
/// peer. The runtime owns this table behind an `RwLock` so the
/// `Equipos` snapshot can read it without serialising against the
/// event pump.
///
/// Presence is now adapter-authoritative: the table simply
/// records every `DiscoveryEvent::Observed` and clears the row
/// when a matching `DiscoveryEvent::Removed` arrives. The previous
/// `PRESENCE_TTL = 120 s` heuristic is gone — flipping a peer to
/// `NotAvailable` because the core had not seen a fresh
/// resolution in 120 s produced the false "No disponible" that
/// the presence-stability change fixes (RFC 6762 browsers can
/// stay silent for a full TTL even when the responder is healthy).
/// The runtime reflects only the adapter's actual browser events.
#[derive(Debug, Clone, Default)]
struct PresenceTable {
    inner: HashMap<String, Instant>,
}

impl PresenceTable {
    fn record(&mut self, peer_id: &str, at: Instant) {
        self.inner.insert(peer_id.to_string(), at);
    }

    fn remove(&mut self, peer_id: &str) {
        self.inner.remove(peer_id);
    }

    fn is_present(&self, peer_id: &str, _now: Instant) -> bool {
        self.inner.contains_key(peer_id)
    }

    fn clear(&mut self) {
        self.inner.clear();
    }
}

/// `DiscoverySink` the runtime exposes to the platform adapter.
/// The adapter pushes events from a background thread; the sink
/// simply forwards them into the queue the worker drains.
struct RuntimeSink {
    queue: Arc<Mutex<Vec<DiscoveryEvent>>>,
}

impl DiscoverySink for RuntimeSink {
    fn push(&self, event: DiscoveryEvent) {
        self.queue.lock().expect("queue lock").push(event);
    }
}

/// Cadence the worker drains the event queue. The value is short
/// enough to feel responsive in the UI but generous enough that
/// a slow persistence call (e.g. the first `INSERT` after a
/// migration) does not starve the loop.
const WORKER_TICK: Duration = Duration::from_millis(250);

/// Persistence closure the bootstrap installs before `start`.
/// The worker passes every validated observation through it;
/// the closure is the only place where the runtime touches the
/// `KnownPeerRepository`, keeping the persistence call site in
/// one place.
type PersistenceFn = dyn Fn(&PeerObservation) -> UpsertObservationOutcome + Send + Sync;

/// Inner fields the worker thread reaches through an `Arc` so
/// the runtime can drop its own handles on `stop` without
/// invalidating the in-flight `drain` call.
#[derive(Clone)]
struct WorkerHandles {
    queue: Arc<Mutex<Vec<DiscoveryEvent>>>,
    local_identity: Arc<RwLock<Option<LocalPeerIdentitySnapshot>>>,
    presence: Arc<RwLock<PresenceTable>>,
    cancel: Arc<AtomicBool>,
    persist: Arc<PersistenceFn>,
}

/// Central state machine the shell drives. The runtime owns the
/// adapter, the presence table, the in-memory event queue and
/// the worker thread the lifecycle spins up on `start`. The
/// production adapter pushes events through the [`RuntimeSink`]
/// the runtime hands it at `start` time; the worker drains the
/// queue and persists observations through the closure the
/// bootstrap installs via [`Self::set_persistence`].
///
/// `PeerDiscoveryRuntime` is cheap to clone: every field is
/// either `Arc` or already shareable across threads.
#[derive(Clone)]
pub struct PeerDiscoveryRuntime {
    adapter: Arc<dyn PeerDiscoveryAdapter>,
    presence: Arc<RwLock<PresenceTable>>,
    events: Arc<Mutex<Vec<DiscoveryEvent>>>,
    local_identity: Arc<RwLock<Option<LocalPeerIdentitySnapshot>>>,
    /// Tracks the running state the runtime last observed so the
    /// shell can render the `active` indicator without polling the
    /// adapter on every snapshot.
    running: Arc<RwLock<bool>>,
    /// Handle to the worker thread the lifecycle spawns on `start`
    /// and joins on `stop`. The handle stays `None` while the
    /// runtime is stopped.
    worker: Arc<RwLock<Option<WorkerHandle>>>,
    /// Persistence closure the bootstrap installs. The runtime
    /// holds it behind an `Option` so tests can exercise the
    /// adapter / sink contract without standing up a database.
    persistence: Arc<RwLock<Option<Arc<PersistenceFn>>>>,
}

struct WorkerHandle {
    cancel: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl PeerDiscoveryRuntime {
    /// Build a runtime around an arbitrary adapter. The local
    /// identity is set later via [`Self::set_local_identity`] so
    /// the shell can wire the identity foundation independently
    /// from the discovery bootstrap. The persistence closure is
    /// installed via [`Self::set_persistence`] before the first
    /// `start` call.
    pub fn new(adapter: Arc<dyn PeerDiscoveryAdapter>) -> Self {
        Self {
            adapter,
            presence: Arc::new(RwLock::new(PresenceTable::default())),
            events: Arc::new(Mutex::new(Vec::new())),
            local_identity: Arc::new(RwLock::new(None)),
            running: Arc::new(RwLock::new(false)),
            worker: Arc::new(RwLock::new(None)),
            persistence: Arc::new(RwLock::new(None)),
        }
    }

    /// Inject the local identity the runtime must self-filter
    /// against. A `None` value disables sharing: the runtime
    /// refuses to start until the foundation is reachable.
    pub fn set_local_identity(&self, identity: Option<LocalPeerIdentitySnapshot>) {
        *self.local_identity.write() = identity;
    }

    /// Install the persistence closure the worker thread calls
    /// on every drained observation. The bootstrap wires the
    /// `KnownPeerRepository::upsert_observation` call here; tests
    /// install an in-memory closure that mirrors the upsert
    /// contract without standing up a database.
    pub fn set_persistence(&self, closure: Arc<PersistenceFn>) {
        *self.persistence.write() = Some(closure);
    }

    /// Start the adapter. Returns the typed reason the runtime
    /// refused to start when an identity is not yet available, and
    /// propagates the adapter error otherwise. Calling `start`
    /// while the runtime is already running is a no-op: the
    /// adapter's own idempotency guarantee plus the running flag
    /// keep the call site contract symmetric.
    pub fn start(&self) -> Result<(), StartError> {
        let identity = self.local_identity.read().clone();
        let Some(identity) = identity else {
            return Err(StartError::IdentityUnavailable);
        };
        let persistence = self.persistence.read().clone();
        let Some(persistence) = persistence else {
            return Err(StartError::PersistenceMissing);
        };
        if *self.running.read() {
            return Ok(());
        }
        let advertisement = DiscoveryAdvertisement::new(
            identity.peer_id.as_str(),
            identity.fingerprint.as_str(),
            identity.display_name.as_str(),
            PROTOCOL_MAJOR,
            DISCOVERY_ONLY_CAPABILITY,
        );
        let sink: Arc<dyn DiscoverySink> = Arc::new(RuntimeSink {
            queue: Arc::clone(&self.events),
        });
        self.adapter
            .start(&advertisement, sink)
            .map_err(StartError::Adapter)?;
        *self.running.write() = true;
        // Spawn the worker thread last so a failed `adapter.start`
        // does not leak a half-started background task.
        let cancel = Arc::new(AtomicBool::new(false));
        let worker = spawn_worker(WorkerHandles {
            queue: Arc::clone(&self.events),
            local_identity: Arc::clone(&self.local_identity),
            presence: Arc::clone(&self.presence),
            cancel: Arc::clone(&cancel),
            persist: persistence,
        });
        *self.worker.write() = Some(WorkerHandle {
            cancel,
            join: Some(worker),
        });
        Ok(())
    }

    /// Stop the adapter. Idempotent: a second call after the
    /// adapter has already been stopped is a no-op. The runtime
    /// keeps the persisted `known_peers` rows across stop / start
    /// so a transient shutdown does not erase the known peers,
    /// but the in-memory presence table is cleared on every
    /// `stop` so a fresh `start` re-derives presence from
    /// authoritative mDNS events (the previous 120 s TTL
    /// heuristic is gone — see [`PresenceTable`]).
    pub fn stop(&self) -> Result<(), AdapterError> {
        if !*self.running.read() {
            return Ok(());
        }
        self.adapter.stop()?;
        *self.running.write() = false;
        // Signal the worker to exit, then drain any leftover
        // events so a subsequent snapshot does not surface a
        // stale "Detected" state for a peer that has already
        // gone away.
        if let Some(mut handle) = self.worker.write().take() {
            handle.cancel.store(true, Ordering::Release);
            if let Some(join) = handle.join.take() {
                if let Err(error) = join.join() {
                    warn!("peer discovery worker thread panicked");
                    let _ = error;
                }
            }
        }
        self.events.lock().expect("events lock").clear();
        // The platform adapter tears down its own liveness
        // scheduler before `stop` returns so no callback can
        // reinsert presence after this point; mirroring that on
        // the core side keeps the snapshot in sync with the
        // adapter's authoritative state.
        self.presence.write().clear();
        Ok(())
    }

    /// Whether the runtime is currently browsing the LAN.
    pub fn is_running(&self) -> bool {
        *self.running.read()
    }

    /// Push an event the adapter emitted into the queue. Tests use
    /// this to feed scripted events; the production adapter has
    /// its own queue that the runtime drains through this method.
    pub fn enqueue(&self, event: DiscoveryEvent) {
        self.events.lock().expect("events lock").push(event);
    }

    /// Drain every queued event, returning the outcomes the runtime
    /// computed (self-filtered, conflict, stored, rejected). The
    /// runtime also updates the presence table on the way out so
    /// the snapshot is always in sync with the database.
    ///
    /// `apply_observation` is the closure the runtime uses to
    /// persist a validated observation. Tests pass a closure that
    /// delegates to a [`clipvault_db::KnownPeerRepository`];
    /// production wires the repository through the [`AppContext`].
    pub fn drain<F>(&self, now: Instant, mut apply_observation: F) -> Vec<ObservationOutcome>
    where
        F: FnMut(&PeerObservation) -> UpsertObservationOutcome,
    {
        let drained: Vec<DiscoveryEvent> = {
            let mut queue = self.events.lock().expect("events lock");
            std::mem::take(&mut *queue)
        };
        let mut outcomes = Vec::with_capacity(drained.len());
        let local = self.local_identity.read().clone();
        for event in drained {
            match event {
                DiscoveryEvent::Observed(record) => {
                    let validated = PeerObservationRecord::from_txt_record(&record);
                    let outcome = match validated {
                        Ok(observation) => {
                            if let Some(local) = local.as_ref() {
                                if local.matches(&observation) {
                                    self.presence.write().record(&observation.peer_id, now);
                                    ObservationOutcome::SelfFiltered
                                } else {
                                    self.persist_and_record(
                                        observation,
                                        now,
                                        &mut apply_observation,
                                    )
                                }
                            } else {
                                ObservationOutcome::Rejected(
                                    PeerRecordValidationError::MissingPeerId,
                                )
                            }
                        }
                        Err(error) => ObservationOutcome::Rejected(error),
                    };
                    outcomes.push(outcome);
                }
                DiscoveryEvent::Removed { peer_id } => {
                    self.presence.write().remove(&peer_id);
                    outcomes.push(ObservationOutcome::SelfFiltered);
                }
            }
        }
        outcomes
    }

    fn persist_and_record<F>(
        &self,
        observation: PeerObservationRecord,
        now: Instant,
        apply_observation: &mut F,
    ) -> ObservationOutcome
    where
        F: FnMut(&PeerObservation) -> UpsertObservationOutcome,
    {
        let persisted = apply_observation(&PeerObservation {
            peer_id: observation.peer_id.clone(),
            public_key_fingerprint: observation.public_key_fingerprint.clone(),
            full_public_key_fingerprint: observation.full_public_key_fingerprint.clone(),
            display_name: observation.display_name.clone(),
            protocol_major: observation.protocol_major,
            capability: observation.capability.clone(),
            caps_extra: observation.caps_extra.join(","),
            observed_at: observation.observed_at,
        });
        self.presence.write().record(&observation.peer_id, now);
        match persisted {
            UpsertObservationOutcome::Stored(row) => ObservationOutcome::Stored(row),
            UpsertObservationOutcome::Conflict(row) => ObservationOutcome::Conflict(row),
        }
    }

    /// Build a metadata-only snapshot the bridge surfaces. The
    /// runtime reads every persisted peer through `read_persisted`
    /// so the snapshot is in sync with the database and the
    /// presence table.
    pub fn snapshot<F>(&self, read_persisted: F, now: Instant) -> PeerSnapshot
    where
        F: FnOnce() -> Vec<KnownPeer>,
    {
        if !self.is_running() {
            // The runtime refuses to start without an identity;
            // when the runtime is not running, the snapshot reports
            // `runtime_stopped` so the UI can render the toggle in
            // the off state. The runtime can also be running while
            // the local identity has not been wired yet; that arm
            // is reported as `identity_unavailable` separately.
            let reason = match self.local_identity.read().clone() {
                Some(_) => RUNTIME_INACTIVE_REASON_RUNTIME_STOPPED,
                None => RUNTIME_INACTIVE_REASON_IDENTITY_UNAVAILABLE,
            };
            return PeerSnapshot::inactive(reason);
        }
        let rows = read_persisted();
        let presence = self.presence.read();
        let entries: Vec<PeerSnapshotEntry> = rows
            .into_iter()
            .map(|row| {
                let is_present = presence.is_present(&row.peer_id, now);
                PeerSnapshotEntry::from_row(row, is_present)
            })
            .collect();
        PeerSnapshot::active(entries)
    }
}

/// Spawn the single worker thread that drains the event queue
/// and persists observations through the closure the bootstrap
/// installs. The worker exits when `cancel` flips to `true`,
/// which the runtime's `stop` does after `adapter.stop`. The
/// thread is the only consumer of the queue; nothing else
/// reaches into `WorkerHandles::queue` for draining.
fn spawn_worker(handles: WorkerHandles) -> JoinHandle<()> {
    thread::Builder::new()
        .name("clipvault-peer-discovery-worker".to_string())
        .spawn(move || {
            while !handles.cancel.load(Ordering::Acquire) {
                let drained = {
                    let mut queue = handles.queue.lock().expect("queue lock");
                    std::mem::take(&mut *queue)
                };
                if drained.is_empty() {
                    // No events to process: sleep until the next
                    // tick or the cancel flag flips.
                    thread::park_timeout(WORKER_TICK);
                    continue;
                }
                // Process each event inline; the persistence
                // closure is allowed to take its time (a slow
                // SQLite write should not block the queue) but
                // we do not spawn an unbounded number of helper
                // threads. The next tick drains whatever the
                // adapter pushed while we were persisting.
                for event in drained {
                    apply_event(&handles, event);
                }
            }
        })
        .expect("spawn peer discovery worker")
}

/// Apply a single event the adapter pushed: validates an
/// observation, self-filters, persists through the closure and
/// updates the presence table. The function lives outside the
/// runtime so the worker's loop body stays short and the
/// per-event error handling stays in one place.
fn apply_event(handles: &WorkerHandles, event: DiscoveryEvent) {
    let now = Instant::now();
    match event {
        DiscoveryEvent::Observed(record) => {
            let validated = PeerObservationRecord::from_txt_record(&record);
            let local = handles.local_identity.read().clone();
            match validated {
                Ok(observation) => {
                    let is_self = local
                        .as_ref()
                        .map(|local| local.matches(&observation))
                        .unwrap_or(false);
                    if is_self {
                        // Self-observation: record presence so the
                        // UI reflects "we are here" but skip
                        // persistence.
                        handles.presence.write().record(&observation.peer_id, now);
                    } else {
                        let persisted = (handles.persist)(&PeerObservation {
                            peer_id: observation.peer_id.clone(),
                            public_key_fingerprint: observation.public_key_fingerprint.clone(),
                            full_public_key_fingerprint: observation
                                .full_public_key_fingerprint
                                .clone(),
                            display_name: observation.display_name.clone(),
                            protocol_major: observation.protocol_major,
                            capability: observation.capability.clone(),
                            caps_extra: observation.caps_extra.join(","),
                            observed_at: observation.observed_at,
                        });
                        handles.presence.write().record(&observation.peer_id, now);
                        match persisted {
                            UpsertObservationOutcome::Stored(_) => {
                                debug!(peer_id = %observation.peer_id, "peer stored");
                            }
                            UpsertObservationOutcome::Conflict(_) => {
                                debug!(
                                    peer_id = %observation.peer_id,
                                    "peer announcement conflicted with persisted row"
                                );
                            }
                        }
                    }
                }
                Err(error) => {
                    debug!(?error, "rejected malformed peer announcement");
                }
            }
        }
        DiscoveryEvent::Removed { peer_id } => {
            handles.presence.write().remove(&peer_id);
        }
    }
}

/// Typed error the runtime returns from `start`. The shell maps
/// this to a stable `sharing_inactive_reason` value so the
/// frontend can render the matching copy.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StartError {
    #[error("local peer identity is not available on this session")]
    IdentityUnavailable,
    #[error("peer discovery persistence closure has not been installed")]
    PersistenceMissing,
    #[error("discovery adapter refused to start: {0}")]
    Adapter(AdapterError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipvault_db::TrustState;
    use clipvault_platform::peer_identity::{PeerFingerprint, PeerId};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use time::macros::datetime;

    fn local_identity(peer_id: &str, fingerprint: &str) -> LocalPeerIdentitySnapshot {
        let pid = PeerId::from_public_key(peer_id.as_bytes());
        let fp = PeerFingerprint::from_public_key(fingerprint.as_bytes());
        LocalPeerIdentitySnapshot::new(pid, fp, "Test".to_string())
    }

    fn txt(peer_id: &str, fingerprint: &str, name: &str) -> TxtRecord {
        TxtRecord::new(
            peer_id,
            fingerprint,
            name,
            PROTOCOL_MAJOR,
            DISCOVERY_ONLY_CAPABILITY,
            datetime!(2026-01-02 03:04:05 UTC),
        )
    }

    /// Test-only adapter that records every `start` / `stop` call
    /// and lets the test queue events the runtime drains.
    #[derive(Debug)]
    struct ScriptedAdapter {
        running: AtomicUsize,
        start_calls: AtomicUsize,
        stop_calls: AtomicUsize,
        start_outcome: Mutex<Result<(), AdapterError>>,
    }

    impl ScriptedAdapter {
        fn new() -> Self {
            Self {
                running: AtomicUsize::new(0),
                start_calls: AtomicUsize::new(0),
                stop_calls: AtomicUsize::new(0),
                start_outcome: Mutex::new(Ok(())),
            }
        }

        fn start_calls(&self) -> usize {
            self.start_calls.load(Ordering::Acquire)
        }

        fn stop_calls(&self) -> usize {
            self.stop_calls.load(Ordering::Acquire)
        }

        fn running_calls(&self) -> usize {
            self.running.load(Ordering::Acquire)
        }
    }

    impl PeerDiscoveryAdapter for ScriptedAdapter {
        fn start(
            &self,
            _advertisement: &DiscoveryAdvertisement,
            _sink: Arc<dyn DiscoverySink>,
        ) -> Result<(), AdapterError> {
            self.start_calls.fetch_add(1, Ordering::AcqRel);
            if self.running_calls() > 0 {
                return Err(AdapterError::AlreadyRunning);
            }
            let outcome = self.start_outcome.lock().expect("start outcome").clone();
            if outcome.is_ok() {
                self.running.fetch_add(1, Ordering::AcqRel);
            }
            outcome
        }

        fn stop(&self) -> Result<(), AdapterError> {
            self.stop_calls.fetch_add(1, Ordering::AcqRel);
            if self.running_calls() == 0 {
                return Ok(());
            }
            self.running.fetch_sub(1, Ordering::AcqRel);
            Ok(())
        }

        fn is_running(&self) -> bool {
            self.running_calls() > 0
        }
    }

    fn apply_in_memory(
        storage: std::sync::Arc<std::sync::Mutex<Vec<KnownPeer>>>,
    ) -> Arc<PersistenceFn> {
        Arc::new(move |observation: &PeerObservation| {
            let mut guard = storage.lock().expect("storage");
            let stamp = observation
                .observed_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into());
            let idx = guard
                .iter()
                .position(|existing| existing.peer_id == observation.peer_id);
            match idx {
                Some(idx) => {
                    let existing = guard.remove(idx);
                    if existing.public_key_fingerprint == observation.public_key_fingerprint
                        && existing.display_name == observation.display_name
                        && existing.protocol_major == observation.protocol_major
                    {
                        // Merge: upgrade the full fingerprint when
                        // the new observation carries one and the
                        // previous row did not. A peer first seen
                        // via discovery_only is promoted the
                        // moment a pairing advertisement upgrades
                        // the column.
                        let merged_full = observation
                            .full_public_key_fingerprint
                            .clone()
                            .filter(|value| !value.is_empty())
                            .unwrap_or_else(|| existing.full_public_key_fingerprint.clone());
                        let refreshed = KnownPeer {
                            peer_id: observation.peer_id.clone(),
                            public_key_fingerprint: observation.public_key_fingerprint.clone(),
                            full_public_key_fingerprint: merged_full,
                            display_name: observation.display_name.clone(),
                            protocol_major: observation.protocol_major,
                            capability: observation.capability.clone(),
                            first_seen_at: existing.first_seen_at.clone(),
                            last_discovered_at: stamp.clone(),
                            updated_at: stamp.clone(),
                            trust_state: existing.trust_state,
                            tls_cert_fingerprint: existing.tls_cert_fingerprint.clone(),
                            paired_at: existing.paired_at.clone(),
                            paired_protocol_major: existing.paired_protocol_major,
                            cursor_secret: existing.cursor_secret.clone(),
                            caps_extra: observation.caps_extra.clone(),
                        };
                        guard.push(refreshed.clone());
                        UpsertObservationOutcome::Stored(refreshed)
                    } else {
                        guard.push(existing.clone());
                        UpsertObservationOutcome::Conflict(existing)
                    }
                }
                None => {
                    let row = KnownPeer {
                        peer_id: observation.peer_id.clone(),
                        public_key_fingerprint: observation.public_key_fingerprint.clone(),
                        full_public_key_fingerprint: observation
                            .full_public_key_fingerprint
                            .clone()
                            .unwrap_or_default(),
                        display_name: observation.display_name.clone(),
                        protocol_major: observation.protocol_major,
                        capability: observation.capability.clone(),
                        first_seen_at: stamp.clone(),
                        last_discovered_at: stamp.clone(),
                        updated_at: stamp,
                        trust_state: clipvault_db::TrustState::Unverified,
                        tls_cert_fingerprint: String::new(),
                        paired_at: String::new(),
                        paired_protocol_major: 0,
                        cursor_secret: String::new(),
                        caps_extra: observation.caps_extra.clone(),
                    };
                    guard.push(row.clone());
                    UpsertObservationOutcome::Stored(row)
                }
            }
        })
    }

    /// Seed the in-memory storage with a single `KnownPeer` row.
    /// The drain test helper treats every `peer_id` collision as
    /// a merge / conflict branch, so a test that wants to exercise
    /// the conflict path primes the storage with a row first.
    fn seed_in_memory(storage: &std::sync::Arc<std::sync::Mutex<Vec<KnownPeer>>>, row: KnownPeer) {
        storage.lock().expect("storage").push(row);
    }

    #[test]
    fn validation_accepts_a_well_formed_txt_record() {
        let raw = txt(
            "0123456789abcdef0123456789abcdef",
            "0123456789abcdef",
            "Studio",
        );
        let validated = PeerObservationRecord::from_txt_record(&raw).expect("valid");
        assert_eq!(validated.peer_id, raw.peer_id);
        assert_eq!(validated.display_name, "Studio");
        assert_eq!(validated.protocol_major, PROTOCOL_MAJOR);
        assert_eq!(validated.capability, DISCOVERY_ONLY_CAPABILITY);
    }

    #[test]
    fn validation_rejects_malformed_peer_id() {
        for bad in [
            "",
            "deadbeef",
            "DEADBEEFDEADBEEFDEADBEEFDEADBEEF",
            "0123456789abcdef0123456789abcde!",
            "0123456789abcdef0123456789abcdeg",
        ] {
            let raw = TxtRecord {
                peer_id: bad.to_string(),
                ..txt("0123456789abcdef", "0123456789abcdef", "Studio")
            };
            let err = PeerObservationRecord::from_txt_record(&raw)
                .expect_err("malformed peer_id must fail");
            assert!(matches!(
                err,
                PeerRecordValidationError::MissingPeerId
                    | PeerRecordValidationError::MalformedPeerId
            ));
        }
    }

    #[test]
    fn validation_rejects_invalid_display_name() {
        for bad in [
            "",
            "   ",
            "with\u{200B}zwsp",
            "\u{0001}ctrl",
            &"a".repeat(MAX_PEER_DISPLAY_NAME_LENGTH + 1),
        ] {
            let raw = TxtRecord {
                display_name: bad.to_string(),
                ..txt(
                    "0123456789abcdef0123456789abcdef",
                    "0123456789abcdef",
                    "placeholder",
                )
            };
            let err =
                PeerObservationRecord::from_txt_record(&raw).expect_err("invalid name must fail");
            assert!(matches!(
                err,
                PeerRecordValidationError::MissingDisplayName
                    | PeerRecordValidationError::InvalidDisplayName
                    | PeerRecordValidationError::DisplayNameTooLong
            ));
        }
    }

    #[test]
    fn validation_rejects_incompatible_protocol_major() {
        let raw = TxtRecord {
            protocol_major: PROTOCOL_MAJOR + 1,
            ..txt(
                "0123456789abcdef0123456789abcdef",
                "0123456789abcdef",
                "Studio",
            )
        };
        let err = PeerObservationRecord::from_txt_record(&raw)
            .expect_err("incompatible protocol must fail");
        assert!(matches!(
            err,
            PeerRecordValidationError::IncompatibleProtocol { observed } if observed == PROTOCOL_MAJOR + 1
        ));
    }

    #[test]
    fn validation_rejects_unsupported_capability() {
        let raw = TxtRecord {
            capability: "totally_unknown".into(),
            ..txt(
                "0123456789abcdef0123456789abcdef",
                "0123456789abcdef",
                "Studio",
            )
        };
        let err = PeerObservationRecord::from_txt_record(&raw)
            .expect_err("unsupported capability must fail");
        assert!(matches!(
            err,
            PeerRecordValidationError::UnsupportedCapability { ref capability } if capability == "totally_unknown"
        ));
    }

    #[test]
    fn validation_accepts_a_pairing_advertisement_with_full_fingerprint() {
        let raw = TxtRecord {
            capability: PAIRING_CAPABILITY.into(),
            pairing_fingerprint: Some("f".repeat(64)),
            ..txt(
                "0123456789abcdef0123456789abcdef",
                "0123456789abcdef",
                "Studio",
            )
        };
        let validated =
            PeerObservationRecord::from_txt_record(&raw).expect("pairing must validate");
        assert_eq!(validated.capability, PAIRING_CAPABILITY);
        assert_eq!(
            validated.full_public_key_fingerprint.as_deref(),
            Some("f".repeat(64).as_str())
        );
    }

    #[test]
    fn validation_accepts_pairing_image_import_capability() {
        // The `peer-image-import` change ships the
        // `image_import` capability alongside `pairing`. The
        // validator must accept the additive form (the legacy
        // `capability` field stays at `pairing` exactly so a
        // strict parser keeps recognising the record; the new
        // `caps_extra` field carries `image_import`) and
        // expose the helper the runtime / UI consults to gate
        // the image surface.
        let raw = TxtRecord {
            capability: PAIRING_CAPABILITY.to_string(),
            caps_extra: vec![IMAGE_IMPORT_CAPABILITY.to_string()],
            pairing_fingerprint: Some("f".repeat(64)),
            ..txt(
                "0123456789abcdef0123456789abcdef",
                "0123456789abcdef",
                "Studio",
            )
        };
        let validated = PeerObservationRecord::from_txt_record(&raw)
            .expect("pairing+image_import must validate");
        assert_eq!(validated.capability, PAIRING_CAPABILITY);
        assert_eq!(
            validated.caps_extra,
            vec![IMAGE_IMPORT_CAPABILITY.to_string()]
        );
        assert!(validated.has_image_import_capability());
        // The full pairing fingerprint is preserved.
        assert_eq!(
            validated.full_public_key_fingerprint.as_deref(),
            Some("f".repeat(64).as_str())
        );
    }

    #[test]
    fn validation_rejects_legacy_comma_separated_capability() {
        // Regression: a host that publishes `pairing,image_import`
        // in the legacy `capability` field MUST be rejected.
        // The strict legacy parser would refuse the record
        // because it does not match `pairing` exactly; the
        // runtime surfaces the rejection as
        // `UnsupportedCapability` so a host that accidentally
        // publishes the additive capability in the wrong field
        // does not silently opt into the image surface.
        let raw = TxtRecord {
            capability: format!("{},{}", PAIRING_CAPABILITY, IMAGE_IMPORT_CAPABILITY),
            pairing_fingerprint: Some("f".repeat(64)),
            ..txt(
                "0123456789abcdef0123456789abcdef",
                "0123456789abcdef",
                "Studio",
            )
        };
        let err = PeerObservationRecord::from_txt_record(&raw)
            .expect_err("comma-separated capability must fail");
        assert!(matches!(
            err,
            PeerRecordValidationError::UnsupportedCapability { .. }
        ));
    }

    #[test]
    fn validation_rejects_unknown_caps_extra_token() {
        // Future-proofing: a token outside the documented
        // additive capability set is rejected so a typo or a
        // future capability addition cannot silently bypass
        // the capability gate when the host upgrades before
        // the runtime does.
        let raw = TxtRecord {
            capability: PAIRING_CAPABILITY.to_string(),
            caps_extra: vec!["future_token".to_string()],
            pairing_fingerprint: Some("f".repeat(64)),
            ..txt(
                "0123456789abcdef0123456789abcdef",
                "0123456789abcdef",
                "Studio",
            )
        };
        let err = PeerObservationRecord::from_txt_record(&raw)
            .expect_err("unknown caps_extra token must fail");
        assert!(matches!(
            err,
            PeerRecordValidationError::UnsupportedCapability { .. }
        ));
    }

    #[test]
    fn validation_rejects_unknown_capability_token() {
        // Future-proofing: a token outside the documented
        // capability set is rejected so a typo cannot silently
        // sneak past the discovery validator.
        let raw = TxtRecord {
            capability: format!("{},bogus", PAIRING_CAPABILITY),
            pairing_fingerprint: Some("f".repeat(64)),
            ..txt(
                "0123456789abcdef0123456789abcdef",
                "0123456789abcdef",
                "Studio",
            )
        };
        let err =
            PeerObservationRecord::from_txt_record(&raw).expect_err("unknown capability must fail");
        assert!(matches!(
            err,
            PeerRecordValidationError::UnsupportedCapability { .. }
        ));
    }

    #[test]
    fn decode_capabilities_handles_csv_whitespace_and_dedup() {
        // Helper sanity check: a TXT record that ships
        // `pairing, image_import,` (trailing comma + extra
        // whitespace) decodes into the two canonical tokens.
        assert_eq!(
            decode_capabilities("pairing, image_import,"),
            vec![
                PAIRING_CAPABILITY.to_string(),
                IMAGE_IMPORT_CAPABILITY.to_string(),
            ]
        );
    }

    #[test]
    fn pair_only_record_lacks_image_import_capability() {
        // Regression: a `pairing` record that does not advertise
        // `image_import` must keep returning `false` from the
        // helper so the UI can gate the surface.
        let raw = TxtRecord {
            capability: PAIRING_CAPABILITY.into(),
            pairing_fingerprint: Some("f".repeat(64)),
            ..txt(
                "0123456789abcdef0123456789abcdef",
                "0123456789abcdef",
                "Studio",
            )
        };
        let validated =
            PeerObservationRecord::from_txt_record(&raw).expect("pairing must validate");
        assert!(!validated.has_image_import_capability());
    }

    #[test]
    fn validation_rejects_pairing_advertisement_missing_full_fingerprint() {
        let raw = TxtRecord {
            capability: PAIRING_CAPABILITY.into(),
            pairing_fingerprint: None,
            ..txt(
                "0123456789abcdef0123456789abcdef",
                "0123456789abcdef",
                "Studio",
            )
        };
        let err = PeerObservationRecord::from_txt_record(&raw)
            .expect_err("pairing without full fingerprint must fail");
        assert!(matches!(
            err,
            PeerRecordValidationError::MissingPairingFingerprint
        ));
    }

    #[test]
    fn start_requires_local_identity() {
        let adapter = Arc::new(ScriptedAdapter::new());
        let runtime = PeerDiscoveryRuntime::new(adapter.clone());
        let err = runtime.start().expect_err("identity unavailable");
        assert!(matches!(err, StartError::IdentityUnavailable));
        // The runtime short-circuits before touching the adapter
        // when the identity foundation is missing — no spurious
        // start request reaches the platform layer.
        assert_eq!(adapter.start_calls(), 0);
        assert!(!runtime.is_running());
    }

    /// Spawn a runtime wired to an in-memory storage that mirrors
    /// the production `KnownPeerRepository::upsert_observation`
    /// contract. The runtime's worker thread is started so the
    /// tests observe the real persistence call site, not the
    /// private `drain` helper.
    fn runtime_with_storage(
        adapter: Arc<ScriptedAdapter>,
        storage: std::sync::Arc<std::sync::Mutex<Vec<KnownPeer>>>,
    ) -> PeerDiscoveryRuntime {
        let runtime = PeerDiscoveryRuntime::new(adapter);
        runtime.set_persistence(apply_in_memory(std::sync::Arc::clone(&storage)));
        runtime
    }

    /// Wait until the worker has drained every queued event the
    /// test pushed. The runtime's worker polls every
    /// [`WORKER_TICK`] ms; the helper bounds the wait so a hung
    /// worker fails the test fast instead of stalling forever.
    fn wait_for_drain(runtime: &PeerDiscoveryRuntime, timeout_ms: u64) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        while std::time::Instant::now() < deadline {
            if runtime.events.lock().expect("events lock").is_empty() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn start_is_idempotent_while_running() {
        let adapter = Arc::new(ScriptedAdapter::new());
        let storage = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let runtime = runtime_with_storage(adapter.clone(), storage);
        runtime.set_local_identity(Some(local_identity(
            "00000000000000000000000000000000",
            "ffffffffffffffff",
        )));
        runtime.set_persistence(apply_in_memory(std::sync::Arc::new(std::sync::Mutex::new(
            Vec::new(),
        ))));
        runtime.start().expect("first start");
        runtime.start().expect("second start is a no-op");
        // The adapter sees exactly one start call so the runtime
        // short-circuits without forwarding a redundant call.
        assert_eq!(adapter.start_calls(), 1);
        // The runtime marks itself running after the first call;
        // the second call is a no-op.
        assert!(runtime.is_running());
        runtime.stop().expect("stop");
    }

    #[test]
    fn stop_is_idempotent_when_not_running() {
        let adapter = Arc::new(ScriptedAdapter::new());
        let storage = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let runtime = runtime_with_storage(adapter.clone(), storage);
        runtime.stop().expect("first stop is a no-op");
        runtime.stop().expect("second stop is a no-op");
        // The runtime short-circuits before touching the adapter
        // when it is already stopped.
        assert_eq!(adapter.stop_calls(), 0);
    }

    #[test]
    fn drain_self_filters_the_local_identity() {
        let adapter = Arc::new(ScriptedAdapter::new());
        let storage = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let runtime = runtime_with_storage(adapter, std::sync::Arc::clone(&storage));
        let local = local_identity("0123456789abcdef0123456789abcdef", "0123456789abcdef");
        runtime.set_local_identity(Some(local.clone()));
        runtime.start().expect("start");
        runtime.enqueue(DiscoveryEvent::Observed(txt(
            local.peer_id.as_str(),
            local.fingerprint.as_str(),
            "Myself",
        )));
        wait_for_drain(&runtime, 2_000);
        assert!(storage.lock().expect("storage").is_empty());
        runtime.stop().expect("stop");
    }

    #[test]
    fn drain_persists_a_new_observation() {
        let adapter = Arc::new(ScriptedAdapter::new());
        let storage = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let runtime = runtime_with_storage(adapter, std::sync::Arc::clone(&storage));
        runtime.set_local_identity(Some(local_identity(
            "00000000000000000000000000000000",
            "ffffffffffffffff",
        )));
        runtime.start().expect("start");
        runtime.enqueue(DiscoveryEvent::Observed(txt(
            "0123456789abcdef0123456789abcdef",
            "0123456789abcdef",
            "Studio",
        )));
        wait_for_drain(&runtime, 2_000);
        let rows = storage.lock().expect("storage");
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.peer_id, "0123456789abcdef0123456789abcdef");
        assert_eq!(row.display_name, "Studio");
        drop(rows);
        runtime.stop().expect("stop");
    }

    #[test]
    fn snapshot_projects_persisted_pairing_trust_without_transport_data() {
        let adapter = Arc::new(ScriptedAdapter::new());
        let storage = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let runtime = runtime_with_storage(adapter, std::sync::Arc::clone(&storage));
        runtime.set_local_identity(Some(local_identity(
            "00000000000000000000000000000000",
            "ffffffffffffffff",
        )));
        runtime.start().expect("start");
        runtime.enqueue(DiscoveryEvent::Observed(txt(
            "0123456789abcdef0123456789abcdef",
            "0123456789abcdef",
            "Studio",
        )));
        wait_for_drain(&runtime, 2_000);
        {
            let mut rows = storage.lock().expect("storage");
            rows[0].trust_state = TrustState::Trusted;
            rows[0].paired_at = "2026-09-21T00:00:00Z".to_string();
            rows[0].caps_extra = IMAGE_IMPORT_CAPABILITY.to_string();
        }

        let snapshot =
            runtime.snapshot(|| storage.lock().expect("storage").clone(), Instant::now());
        let entry = snapshot.entries.first().expect("trusted peer");
        assert_eq!(entry.trust_state, "trusted");
        assert_eq!(entry.paired_at.as_deref(), Some("2026-09-21T00:00:00Z"));
        assert_eq!(entry.capability, DISCOVERY_ONLY_CAPABILITY);
        assert_eq!(entry.caps_extra, IMAGE_IMPORT_CAPABILITY);
        runtime.stop().expect("stop");
    }

    #[test]
    fn drain_upgrades_a_discovery_record_when_pairing_is_advertised() {
        let adapter = Arc::new(ScriptedAdapter::new());
        let storage = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let runtime = runtime_with_storage(adapter, std::sync::Arc::clone(&storage));
        runtime.set_local_identity(Some(local_identity(
            "00000000000000000000000000000000",
            "ffffffffffffffff",
        )));
        runtime.start().expect("start");

        runtime.enqueue(DiscoveryEvent::Observed(txt(
            "0123456789abcdef0123456789abcdef",
            "0123456789abcdef",
            "Studio",
        )));
        wait_for_drain(&runtime, 2_000);

        let mut pairing = txt(
            "0123456789abcdef0123456789abcdef",
            "0123456789abcdef",
            "Studio",
        );
        pairing.capability = PAIRING_CAPABILITY.to_string();
        pairing.pairing_fingerprint = Some("f".repeat(FULL_FINGERPRINT_HEX_CHARS));
        runtime.enqueue(DiscoveryEvent::Observed(pairing));
        wait_for_drain(&runtime, 2_000);

        let rows = storage.lock().expect("storage");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].capability, PAIRING_CAPABILITY);
        assert_eq!(
            rows[0].full_public_key_fingerprint,
            "f".repeat(FULL_FINGERPRINT_HEX_CHARS)
        );
        drop(rows);
        runtime.stop().expect("stop");
    }

    #[test]
    fn drain_reports_a_conflict_without_overwriting() {
        let adapter = Arc::new(ScriptedAdapter::new());
        let storage = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let runtime = runtime_with_storage(adapter, std::sync::Arc::clone(&storage));
        runtime.set_local_identity(Some(local_identity(
            "00000000000000000000000000000000",
            "ffffffffffffffff",
        )));
        // Seed the in-memory store with a known row.
        seed_in_memory(
            &storage,
            KnownPeer {
                peer_id: "0123456789abcdef0123456789abcdef".to_string(),
                public_key_fingerprint: "aaaaaaaaaaaaaaaa".to_string(),
                full_public_key_fingerprint: String::new(),
                display_name: "Original".to_string(),
                protocol_major: PROTOCOL_MAJOR,
                capability: DISCOVERY_ONLY_CAPABILITY.to_string(),
                first_seen_at: "2026-01-01T00:00:00Z".to_string(),
                last_discovered_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
                trust_state: clipvault_db::TrustState::Unverified,
                tls_cert_fingerprint: String::new(),
                paired_at: String::new(),
                paired_protocol_major: 0,
                cursor_secret: String::new(),
                caps_extra: String::new(),
            },
        );
        runtime.start().expect("start");

        runtime.enqueue(DiscoveryEvent::Observed(TxtRecord {
            peer_id: "0123456789abcdef0123456789abcdef".to_string(),
            public_key_fingerprint: "bbbbbbbbbbbbbbbb".to_string(),
            pairing_fingerprint: None,
            display_name: "Impostor".to_string(),
            protocol_major: PROTOCOL_MAJOR,
            capability: DISCOVERY_ONLY_CAPABILITY.to_string(),
            caps_extra: Vec::new(),
            observed_at: datetime!(2026-01-02 03:04:05 UTC),
        }));
        wait_for_drain(&runtime, 2_000);
        // The persisted row stays untouched.
        let rows = storage.lock().expect("storage");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].public_key_fingerprint, "aaaaaaaaaaaaaaaa");
        drop(rows);
        runtime.stop().expect("stop");
    }

    #[test]
    fn drain_reports_rejected_for_a_malformed_announcement() {
        let adapter = Arc::new(ScriptedAdapter::new());
        let storage = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let runtime = runtime_with_storage(adapter, std::sync::Arc::clone(&storage));
        runtime.set_local_identity(Some(local_identity(
            "00000000000000000000000000000000",
            "ffffffffffffffff",
        )));
        runtime.start().expect("start");
        runtime.enqueue(DiscoveryEvent::Observed(TxtRecord {
            peer_id: "not-a-hex-peer-id".to_string(),
            public_key_fingerprint: "0123456789abcdef".to_string(),
            pairing_fingerprint: None,
            display_name: "Studio".to_string(),
            protocol_major: PROTOCOL_MAJOR,
            capability: DISCOVERY_ONLY_CAPABILITY.to_string(),
            caps_extra: Vec::new(),
            observed_at: datetime!(2026-01-02 03:04:05 UTC),
        }));
        wait_for_drain(&runtime, 2_000);
        assert!(storage.lock().expect("storage").is_empty());
        runtime.stop().expect("stop");
    }

    #[test]
    fn observed_peer_stays_present_past_three_former_presence_windows() {
        // The previous design flipped a peer to `NotAvailable`
        // after a fixed `PRESENCE_TTL = 120 s` wall-clock window
        // because the core and the adapter were using different
        // clocks to express "liveness". That produced the false
        // "No disponible" the `local-peer-presence-liveness`
        // change fixes. The new contract is adapter-authoritative:
        // the presence table only flips when the adapter delivers
        // a matching `DiscoveryEvent::Removed`, so the peer must
        // remain `Detected` well past three of the former
        // 120-second windows without any extra `Observed` event.
        let adapter = Arc::new(ScriptedAdapter::new());
        let storage = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let runtime = runtime_with_storage(adapter, std::sync::Arc::clone(&storage));
        runtime.set_local_identity(Some(local_identity(
            "00000000000000000000000000000000",
            "ffffffffffffffff",
        )));
        runtime.start().expect("start");

        runtime.enqueue(DiscoveryEvent::Observed(txt(
            "0123456789abcdef0123456789abcdef",
            "0123456789abcdef",
            "Studio",
        )));
        wait_for_drain(&runtime, 2_000);
        let now = Instant::now();
        let snapshot = runtime.snapshot(|| storage.lock().expect("storage").clone(), now);
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].presence, PeerPresence::Detected);

        // Three former 120-second windows (360 s + 1 s slack to
        // make the wall-clock arithmetic obvious) — the runtime
        // must still report `Detected` because no `Removed` has
        // arrived. A `reap_expired`-style helper no longer exists.
        let later = now + Duration::from_secs(360) + Duration::from_secs(1);
        let snapshot = runtime.snapshot(|| storage.lock().expect("storage").clone(), later);
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].presence, PeerPresence::Detected);
    }

    /// The runtime must flip a previously-detected peer to
    /// `NotAvailable` as soon as it processes a `Removed` event,
    /// independent of the presence TTL. The adapter now emits the
    /// real `peer_id` on `Removed` (it used to derive it from the
    /// visible instance name), so this test exercises the runtime
    /// side of that contract: the snapshot flips immediately and
    /// the persisted row keeps its first/last-seen metadata.
    #[test]
    fn removed_event_flips_presence_to_not_available_immediately() {
        let adapter = Arc::new(ScriptedAdapter::new());
        let storage = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let runtime = runtime_with_storage(adapter, std::sync::Arc::clone(&storage));
        runtime.set_local_identity(Some(local_identity(
            "00000000000000000000000000000000",
            "ffffffffffffffff",
        )));
        runtime.start().expect("start");

        runtime.enqueue(DiscoveryEvent::Observed(txt(
            "0123456789abcdef0123456789abcdef",
            "0123456789abcdef",
            "Studio",
        )));
        wait_for_drain(&runtime, 2_000);
        let now = Instant::now();
        let snapshot = runtime.snapshot(|| storage.lock().expect("storage").clone(), now);
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].presence, PeerPresence::Detected);

        // Removal happens well before the presence TTL would
        // expire; the snapshot must already report
        // `NotAvailable`.
        runtime.enqueue(DiscoveryEvent::Removed {
            peer_id: "0123456789abcdef0123456789abcdef".to_string(),
        });
        wait_for_drain(&runtime, 2_000);
        let snapshot = runtime.snapshot(|| storage.lock().expect("storage").clone(), now);
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].presence, PeerPresence::NotAvailable);
        // The persisted row stays in the known-peer list so the
        // shell can re-detect the same peer without losing its
        // history.
        let rows = storage.lock().expect("storage");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].peer_id, "0123456789abcdef0123456789abcdef");
        runtime.stop().expect("stop");
    }

    #[test]
    fn snapshot_reports_inactive_when_runtime_is_stopped() {
        let adapter = Arc::new(ScriptedAdapter::new());
        let runtime = PeerDiscoveryRuntime::new(adapter);
        runtime.set_local_identity(Some(local_identity(
            "00000000000000000000000000000000",
            "ffffffffffffffff",
        )));
        let snapshot = runtime.snapshot(Vec::new, Instant::now());
        assert!(!snapshot.sharing_active);
        assert_eq!(
            snapshot.sharing_inactive_reason.as_deref(),
            Some(RUNTIME_INACTIVE_REASON_RUNTIME_STOPPED)
        );
    }

    #[test]
    fn snapshot_reports_identity_unavailable_when_missing() {
        let adapter = Arc::new(ScriptedAdapter::new());
        let runtime = PeerDiscoveryRuntime::new(adapter);
        let snapshot = runtime.snapshot(Vec::new, Instant::now());
        assert!(!snapshot.sharing_active);
        assert_eq!(
            snapshot.sharing_inactive_reason.as_deref(),
            Some(RUNTIME_INACTIVE_REASON_IDENTITY_UNAVAILABLE)
        );
    }
}
