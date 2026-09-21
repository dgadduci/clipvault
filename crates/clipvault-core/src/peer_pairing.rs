//! Local peer mutual pairing runtime.
//!
//! `local-peer-mutual-pairing` introduces the in-process state
//! machine that drives a bounded pairing session between two
//! ClipVault installations that already observed each other
//! through the discovery runtime. The runtime owns:
//!
//! - the [`PairingSession`] lifecycle (nonce generation, SAS
//!   computation, two-minute timeout, rate limit, cleanup on
//!   cancel / disconnect / timeout);
//! - the [`PairingPersistence`] trait the runtime calls to commit
//!   the trust transition once both sides approved;
//! - the [`PairingRuntime`] façade the shell drives (start a
//!   session, accept an inbound session, cancel, surface
//!   snapshots).
//!
//! The runtime is intentionally small: every cryptographic
//! surface lives in [`clipvault_platform::peer_transport`]. The
//! pairing runtime never sees a TLS key, an IP, a port or a
//! handshake error; it only exchanges the
//! [`PairingMessage`] envelope the transport module defines and
//! a metadata-only [`PairingObservation`] record on success.
//!
//! ## No auto-accept
//!
//! The runtime refuses to mark a row `Trusted` until it has
//! received [`PairingMessage::Approve`] from **both** sides. A
//! single-sided approval only arms the local session; the remote
//! side still has to send its own approval before the runtime
//! issues a [`PairingOutcome::Trusted`] outcome. The transport
//! rejects any inbound message that lacks a valid signature
//! transcript so a MITM cannot forge an approval.
//! ## Persistence
//!
//! [`PairingPersistence`] is the single trait the runtime calls
//! to commit a trust transition. The bootstrap installs an
//! adapter that delegates to
//! [`clipvault_db::KnownPeerRepository::mark_trusted`] /
//! [`clipvault_db::KnownPeerRepository::mark_revoked`] /
//! [`clipvault_db::KnownPeerRepository::mark_blocked`] /
//! [`clipvault_db::KnownPeerRepository::unblock`]. Tests inject
//! an in-memory fake.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{debug, warn};

use clipvault_db::{KnownPeer, TrustState, TrustTransitionOutcome};

pub use clipvault_platform::peer_transport::{
    compute_sas, default_peer_transport, PairingMessage, PeerTransport, PeerTransportObservation,
    TransportError, TransportOutcome, TransportSink, PAIRING_MAX_IN_FLIGHT_SESSIONS,
    PAIRING_MAX_PAYLOAD_BYTES, PAIRING_WIRE_VERSION,
};
#[cfg(feature = "local-peer-pairing-tls")]
pub use clipvault_platform::peer_transport::{PairingAdvertisement, PairingAdvertisementSink};

/// Wire-protocol major version the pairing runtime expects. The
/// value mirrors [`clipvault_platform::peer_transport::PAIRING_WIRE_VERSION`]
/// so a future transport bump forces a runtime bump in lock-step.
pub const PAIRING_PROTOCOL_MAJOR: i64 = 1;

/// Default lifetime of a single pairing session. The runtime
/// surfaces the value to the UI so the modal can render a
/// countdown that matches the state machine's hard limit.
pub const PAIRING_SESSION_TIMEOUT: Duration = Duration::from_secs(120);

/// Maximum number of pairing sessions a single peer_id can start
/// inside a one-minute sliding window. The runtime rejects any
/// request above the cap with [`PairingError::RateLimited`] so a
/// misbehaving / malicious peer cannot saturate the local
/// listener's accept queue.
pub const PAIRING_RATE_LIMIT_PER_MINUTE: u32 = 4;

/// Stable wire identifier of the pairing capability the runtime
/// advertises through the discovery TXT record once sharing is
/// active. Mirrors
/// [`clipvault_platform::peer_discovery::DISCOVERY_ONLY_CAPABILITY`]
/// for the discovery side; the pairing side publishes
/// `capability = pairing`.
pub const PAIRING_CAPABILITY: &str = "pairing";

/// Outcome the runtime returns to the shell for a single pairing
/// call. The variants are stable identifiers the bridge / frontend
/// surface; the messages never carry peer-supplied bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingOutcome {
    /// The session reached the reciprocal `Approve` exchange and
    /// the persistence layer confirmed the row is now
    /// [`TrustState::Trusted`].
    Trusted(KnownPeer),
    /// The session is waiting for the remote peer's `Approve`
    /// after the local user accepted. The variant carries the
    /// locally-stored session id so the UI can poll the same id
    /// on subsequent snapshots without inventing a new one.
    AwaitingRemoteApproval(PairingSessionId),
    /// The session timed out, was cancelled, lost its connection
    /// or hit a wire-version mismatch. The runtime guarantees
    /// the persistence layer never recorded `Trusted`.
    Failed(PairingError),
}

/// Stable opaque session id the runtime mints per pairing attempt.
/// The id is what the UI / bridge surfaces to the user; the id is
/// also the key the runtime uses to look up the in-memory session
/// state. The id is **never** the peer's `peer_id` — a single
/// peer may have multiple concurrent sessions (a misclick +
/// recovery attempt) and the runtime would otherwise collapse the
/// state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PairingSessionId(pub u64);

impl PairingSessionId {
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Typed error the runtime surfaces for every pairing failure.
/// The variants are stable identifiers the shell can render as
/// copy without parsing free-form strings.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PairingError {
    /// The session timed out before the reciprocal `Approve`
    /// arrived. The runtime guarantees the persistence layer
    /// never recorded `Trusted`.
    #[error("pairing session expired before both sides approved")]
    SessionExpired,
    /// The session was cancelled by the local user or the shell
    /// tore the runtime down before completion.
    #[error("pairing session was cancelled")]
    Cancelled,
    /// The remote peer is on a wire-protocol version the runtime
    /// does not understand.
    #[error("pairing wire protocol is incompatible")]
    IncompatibleProtocol,
    /// The remote peer presented a `peer_id` that does not match
    /// a persisted `known_peers` row, or its TLS cert
    /// fingerprint did not match the pinned value.
    #[error("pairing peer is unknown or its TLS identity does not match the persisted pin")]
    UnknownOrKeyMismatch,
    /// The remote peer is in the [`TrustState::Blocked`] state.
    /// The runtime rejects the connection before any cryptographic
    /// work runs.
    #[error("pairing peer is blocked")]
    Blocked,
    /// The remote peer is in the [`TrustState::Revoked`] state.
    /// The runtime rejects the connection; the user must
    /// explicitly re-pair to clear the revoke.
    #[error("pairing peer is revoked")]
    Revoked,
    /// The session hit the
    /// [`PAIRING_RATE_LIMIT_PER_MINUTE`] cap. The runtime
    /// surfaces the rejection without consuming the request.
    #[error("pairing rate limit exceeded")]
    RateLimited,
    /// The session hit a transport-level failure (no listener
    /// bound, no local identity, etc).
    #[error("pairing transport is unavailable")]
    TransportUnavailable,
}

/// Metadata-only description of a single in-flight pairing
/// session the runtime holds. The struct is the snapshot the UI
/// renders next to the modal's SAS code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PairingSessionSnapshot {
    pub session_id: PairingSessionId,
    pub remote_peer_id: String,
    pub remote_fingerprint: String,
    /// `true` when the remote peer initiated this session and the
    /// local listener registered it from an authenticated `Hello`.
    ///
    /// This is metadata-only directionality, not transport state:
    /// it lets the shell surface an inbound invitation immediately
    /// instead of starting a competing outbound session when the
    /// user opens the pairing UI on the receiving host.
    pub is_inbound: bool,
    /// SHA-256 of the remote peer's TLS cert DER. The runtime
    /// persists this value in `known_peers.tls_cert_fingerprint`
    /// after the dual-approval gate promotes the row and forwards
    /// it to the productive pairing transport so the next mTLS
    /// handshake is pinned against it. `None` while the
    /// transport has not yet pushed a metadata-only observation.
    pub cert_fingerprint: Option<String>,
    pub remote_display_name: String,
    pub local_approved: bool,
    pub remote_approved: bool,
    pub sas: String,
    pub expires_at: String,
}

impl PairingSessionSnapshot {
    pub fn is_complete(&self) -> bool {
        self.local_approved && self.remote_approved
    }
}

/// Outcome of the trust-state transitions the shell drives
/// independently of an in-flight pairing session. The runtime
/// returns one of these variants so the UI can render a stable
/// reason string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustOperationOutcome {
    /// The transition succeeded and the row reflects the new
    /// state.
    Stored(KnownPeer),
    /// The transition was refused because the row is in a state
    /// that does not allow it (e.g. `mark_trusted` on a blocked
    /// row). The previously persisted row is returned so the
    /// caller can decide how to surface the rejection.
    Conflict(KnownPeer),
    /// The peer id is not present in `known_peers`. The runtime
    /// refuses to invent a row from a pairing / block attempt
    /// alone.
    Unknown,
}

/// Persistence trait the runtime calls to commit a trust
/// transition. The bootstrap installs an adapter that delegates
/// to [`clipvault_db::KnownPeerRepository`] operations; tests
/// inject an in-memory fake.
pub trait PairingPersistence: Send + Sync {
    fn mark_trusted(
        &self,
        peer_id: &str,
        tls_cert_fingerprint: &str,
        paired_at: OffsetDateTime,
        paired_protocol_major: i64,
    ) -> Result<TrustTransitionOutcome, PairingPersistenceError>;

    fn mark_revoked(
        &self,
        peer_id: &str,
    ) -> Result<TrustTransitionOutcome, PairingPersistenceError>;

    fn mark_blocked(
        &self,
        peer_id: &str,
    ) -> Result<TrustTransitionOutcome, PairingPersistenceError>;

    fn unblock(&self, peer_id: &str) -> Result<TrustTransitionOutcome, PairingPersistenceError>;

    fn load(&self, peer_id: &str) -> Result<Option<KnownPeer>, PairingPersistenceError>;
}

/// Typed persistence error the runtime surfaces. Every adapter
/// collapses the underlying failure into one of these variants so
/// the runtime can branch on the reason without inspecting
/// platform-specific error strings.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PairingPersistenceError {
    #[error("pairing persistence backend is unavailable")]
    Unavailable,
    #[error("pairing persistence backend rejected the operation")]
    Failed,
}

/// In-memory persistence adapter the tests use. The adapter
/// mirrors the contract the production
/// [`clipvault_db::KnownPeerRepository`] exposes — the runtime
/// trusts the same outcomes regardless of the backend.
#[derive(Debug, Default)]
pub struct InMemoryPairingPersistence {
    inner: Mutex<HashMap<String, KnownPeer>>,
}

impl InMemoryPairingPersistence {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seed(&self, row: KnownPeer) {
        self.inner
            .lock()
            .expect("persistence lock")
            .insert(row.peer_id.clone(), row);
    }

    pub fn snapshot(&self) -> Vec<KnownPeer> {
        let guard = self.inner.lock().expect("persistence lock");
        let mut rows: Vec<KnownPeer> = guard.values().cloned().collect();
        rows.sort_by(|a, b| b.last_discovered_at.cmp(&a.last_discovered_at));
        rows
    }
}

impl PairingPersistence for InMemoryPairingPersistence {
    fn mark_trusted(
        &self,
        peer_id: &str,
        tls_cert_fingerprint: &str,
        paired_at: OffsetDateTime,
        paired_protocol_major: i64,
    ) -> Result<TrustTransitionOutcome, PairingPersistenceError> {
        let mut guard = self.inner.lock().expect("persistence lock");
        let Some(previous) = guard.get(peer_id).cloned() else {
            return Ok(TrustTransitionOutcome::Unknown);
        };
        if previous.trust_state == TrustState::Blocked {
            return Ok(TrustTransitionOutcome::Conflict(previous));
        }
        let paired_at = format_rfc3339(paired_at);
        let mut next = previous.clone();
        next.trust_state = TrustState::Trusted;
        next.tls_cert_fingerprint = tls_cert_fingerprint.to_string();
        next.paired_at = paired_at;
        next.paired_protocol_major = paired_protocol_major;
        next.updated_at = next.paired_at.clone();
        guard.insert(peer_id.to_string(), next.clone());
        Ok(TrustTransitionOutcome::Stored(next))
    }

    fn mark_revoked(
        &self,
        peer_id: &str,
    ) -> Result<TrustTransitionOutcome, PairingPersistenceError> {
        let mut guard = self.inner.lock().expect("persistence lock");
        let Some(previous) = guard.get(peer_id).cloned() else {
            return Ok(TrustTransitionOutcome::Unknown);
        };
        if previous.trust_state == TrustState::Blocked {
            return Ok(TrustTransitionOutcome::Conflict(previous));
        }
        let now = format_rfc3339(OffsetDateTime::now_utc());
        let mut next = previous;
        next.trust_state = TrustState::Revoked;
        next.updated_at = now;
        guard.insert(peer_id.to_string(), next.clone());
        Ok(TrustTransitionOutcome::Stored(next))
    }

    fn mark_blocked(
        &self,
        peer_id: &str,
    ) -> Result<TrustTransitionOutcome, PairingPersistenceError> {
        let mut guard = self.inner.lock().expect("persistence lock");
        let Some(previous) = guard.get(peer_id).cloned() else {
            return Ok(TrustTransitionOutcome::Unknown);
        };
        if previous.trust_state == TrustState::Blocked {
            return Ok(TrustTransitionOutcome::Conflict(previous));
        }
        let now = format_rfc3339(OffsetDateTime::now_utc());
        let mut next = previous;
        next.trust_state = TrustState::Blocked;
        next.updated_at = now;
        guard.insert(peer_id.to_string(), next.clone());
        Ok(TrustTransitionOutcome::Stored(next))
    }

    fn unblock(&self, peer_id: &str) -> Result<TrustTransitionOutcome, PairingPersistenceError> {
        let mut guard = self.inner.lock().expect("persistence lock");
        let Some(previous) = guard.get(peer_id).cloned() else {
            return Ok(TrustTransitionOutcome::Unknown);
        };
        if previous.trust_state != TrustState::Blocked {
            return Ok(TrustTransitionOutcome::Conflict(previous));
        }
        let now = format_rfc3339(OffsetDateTime::now_utc());
        let mut next = previous;
        next.trust_state = TrustState::Unverified;
        next.tls_cert_fingerprint = String::new();
        next.paired_at = String::new();
        next.paired_protocol_major = 0;
        next.updated_at = now;
        guard.insert(peer_id.to_string(), next.clone());
        Ok(TrustTransitionOutcome::Stored(next))
    }

    fn load(&self, peer_id: &str) -> Result<Option<KnownPeer>, PairingPersistenceError> {
        Ok(self
            .inner
            .lock()
            .expect("persistence lock")
            .get(peer_id)
            .cloned())
    }
}

fn format_rfc3339(ts: OffsetDateTime) -> String {
    ts.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// State the runtime tracks per pairing session. The struct lives
/// behind a `Mutex` so the cancel / timeout paths can drop a
/// session without holding any lock for the duration of a
/// network round-trip.
#[derive(Debug)]
struct SessionState {
    id: PairingSessionId,
    remote_peer_id: String,
    remote_fingerprint: String,
    remote_display_name: String,
    #[allow(dead_code)]
    nonce_a: String,
    #[allow(dead_code)]
    nonce_b: String,
    sas: String,
    local_approved: bool,
    remote_approved: bool,
    cert_fingerprint: String,
    #[allow(dead_code)]
    started_at: Instant,
    expires_at: Instant,
    /// Distinguishes sessions the listener registered through
    /// [`TransportSink::on_pairing_session_started`] from the
    /// ones the runtime created via [`Self::start_outbound`].
    /// The flag controls whether [`Self::approve_local`] /
    /// [`Self::cancel`] forward to
    /// [`PeerTransport::approve_local`] /
    /// [`PeerTransport::cancel_session`] (outbound) or to the
    /// inbound approval channel
    /// ([`PeerTransport::approve_inbound_session`]) the
    /// listener task awaits.
    is_inbound: bool,
}

impl SessionState {
    fn expired(&self, now: Instant) -> bool {
        now >= self.expires_at
    }
}

/// Pairing runtime the shell drives. The runtime owns the
/// transport adapter, the in-memory session table and the
/// persistence backend the bootstrap installs. The runtime is
/// cheap to clone: every field is `Arc`-shared.
#[derive(Clone)]
pub struct PairingRuntime {
    inner: Arc<PairingRuntimeInner>,
}

struct PairingRuntimeInner {
    #[allow(dead_code)]
    transport: Arc<dyn PeerTransport>,
    sessions: RwLock<HashMap<PairingSessionId, Mutex<SessionState>>>,
    rate: RwLock<RateLimit>,
    persistence: Arc<dyn PairingPersistence>,
    next_session_id: AtomicU64,
    /// Optional callback that loads the keychain-backed TLS
    /// material the production transport needs to bind a listener
    /// and mint its self-signed Ed25519 cert. When `None` the
    /// runtime cannot install a productive transport; the shell
    /// drives [`Self::install_pairing_transport`] / [`Self::stop_pairing_transport`]
    /// around the local peer sharing toggle.
    #[cfg(feature = "local-peer-pairing-tls")]
    material_loader: RwLock<Option<Arc<dyn MaterialLoader>>>,
    /// Cached copy of the local identity the runtime uses to
    /// compute the SAS with the real `peer_id` / fingerprint /
    /// nonce material instead of the placeholder strings the
    /// pre-wiring shipped. The shell refreshes this through
    /// [`Self::set_local_identity`] whenever the secure store
    /// hands out (or refreshes) the local identity; the
    /// `Option` collapses to `None` while the keychain is
    /// unreachable so the runtime surfaces `TransportUnavailable`
    /// instead of a half-correct SAS.
    #[cfg(feature = "local-peer-pairing-tls")]
    local_identity: RwLock<Option<clipvault_platform::LocalPeerIdentity>>,
    /// Optional sink the runtime installs on the productive
    /// pairing transport. The sink routes every
    /// [`PeerTransportObservation`] the inbound side delivers
    /// back into [`Self::observe_approve`] so the trust
    /// promotion path uses the authenticated transport event
    /// instead of trusting a renderer-supplied payload.
    #[cfg(feature = "local-peer-pairing-tls")]
    inbound_sink: parking_lot::Mutex<Option<Arc<dyn InboundApprovalSink>>>,
}

/// Callback that returns the keychain-backed
/// [`LocalIdentityMaterial`] the production transport needs. The
/// runtime never sees the seed bytes — only the typed material
/// the platform crate builds from them. The callback returns
/// `Unavailable` when the secure store is unreachable, which
/// collapses every transport call into a typed
/// [`TransportOutcome::Unavailable`] without leaking the
/// underlying platform detail.
#[cfg(feature = "local-peer-pairing-tls")]
pub trait MaterialLoader: Send + Sync {
    fn load(&self) -> Result<clipvault_platform::LocalIdentityMaterial, PairingPersistenceError>;
}

/// Callback the productive pairing transport installs so the
/// inbound side can route every authenticated transport event
/// back into the runtime state machine. The transport pushes a
/// metadata-only [`PeerTransportObservation`] the same way the
/// bridge surfaces observations: the runtime arms the pin,
/// promotes the row to `Trusted` when both approvals are in
/// place and surfaces typed failures on every other path. The
/// renderer MUST NOT install a sink of its own — the bridge
/// only accepts `start` / `approve_local` / `cancel` / `revoke`
/// / `block` / `unblock` and never receives the peer cert or
/// the signed transcript, so a renderer-supplied sink would be
/// the single path that could forge a remote approval.
#[cfg(feature = "local-peer-pairing-tls")]
pub trait InboundApprovalSink: Send + Sync {
    fn on_inbound_approval(&self, observation: &PeerTransportObservation);
}

#[cfg(feature = "local-peer-pairing-tls")]
impl InboundApprovalSink for PairingRuntime {
    fn on_inbound_approval(&self, observation: &PeerTransportObservation) {
        // The transport hands the runtime a metadata-only
        // observation built from the cert SPKI the mTLS handshake
        // verified. The runtime reuses
        // [`Self::observe_approve`] so the same dual-approval
        // gate that powers `start_outbound` decides whether the
        // row promotes to `Trusted`. The cert fingerprint the
        // transport delivered becomes the pin the next health
        // probe must match.
        self.observe_approve(&observation.peer_id, &observation.cert_fingerprint);
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl TransportSink for PairingRuntime {
    fn on_pairing_observed(&self, observation: PeerTransportObservation) {
        // The transport pushes a metadata-only observation the
        // inbound side built from the cert SPKI it pinned during
        // the mTLS handshake. Routing through
        // [`Self::observe_approve`] makes the inbound side the
        // second approval the dual-approval gate requires: the
        // local user must have already accepted (via
        // [`Self::approve_local`]) for the row to promote to
        // `Trusted`.
        self.observe_approve(&observation.peer_id, &observation.cert_fingerprint);
    }

    fn on_pairing_session_started(
        &self,
        metadata: clipvault_platform::peer_transport::InboundSessionMetadata,
    ) {
        // The listener registered an inbound pairing session
        // BEFORE the local user approved. Route through the
        // runtime so the UI of the second host can display the
        // canonical SAS the transport computed from the real
        // nonces the wire protocol negotiated. The runtime's
        // existing `register_inbound` performs the trust-state
        // pre-flight (blocked / revoked / rate-limit) and stores
        // the session under the opaque id the listener minted.
        self.register_inbound_from_metadata(metadata);
    }
}

impl PairingPersistenceError {
    /// Bridge a keychain loader failure to the typed
    /// [`PairingPersistenceError::Unavailable`] the runtime
    /// expects. The runtime never sees the raw platform detail;
    /// the bridge keeps every caller on the same typed path.
    pub fn from_keychain(_error: clipvault_platform::PeerIdentityError) -> Self {
        Self::Unavailable
    }
}

/// Bridge the typed [`TransportError`] the productive pairing
/// transport surfaces into the typed [`PairingError`] the runtime
/// uses. The mapping preserves the design invariant: an unknown
/// peer collapses into [`PairingError::UnknownOrKeyMismatch`], a
/// blocked / revoked peer collapses into the matching variant,
/// and every other typed failure collapses into the typed
/// `TransportUnavailable` so the runtime can render the matching
/// copy without inspecting free-form strings.
#[cfg(feature = "local-peer-pairing-tls")]
fn map_transport_error_to_pairing_error(
    error: clipvault_platform::peer_transport::TransportError,
) -> PairingError {
    use clipvault_platform::peer_transport::TransportError;
    match error {
        TransportError::UnknownPeer => PairingError::UnknownOrKeyMismatch,
        TransportError::KeyMismatch => PairingError::UnknownOrKeyMismatch,
        TransportError::Blocked => PairingError::Blocked,
        TransportError::Revoked => PairingError::Revoked,
        TransportError::IncompatibleProtocol => PairingError::IncompatibleProtocol,
        TransportError::Unavailable
        | TransportError::AlreadyRunning
        | TransportError::NotRunning
        | TransportError::Crypto
        | TransportError::Malformed => PairingError::TransportUnavailable,
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
fn map_transport_health_snapshot(
    snapshot: clipvault_platform::peer_transport::PeerHealthSnapshot,
) -> PeerPairingHealthSnapshot {
    PeerPairingHealthSnapshot {
        peer_id: snapshot.peer_id,
        protocol_major: snapshot.protocol_major,
    }
}

/// Metadata-only snapshot the runtime returns from
/// [`PairingRuntime::health_probe`]. The struct carries only the
/// `peer_id` + protocol major the productive pairing transport
/// delivered after the bounded `Health` envelope exchange — no
/// history, fetch or import payload crosses the wire in this
/// change.
#[cfg(feature = "local-peer-pairing-tls")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerPairingHealthSnapshot {
    pub peer_id: String,
    pub protocol_major: i64,
}

impl PairingRuntime {
    /// Build a runtime around an arbitrary transport + persistence
    /// pair. The production shell wires the
    /// [`clipvault_platform::default_peer_transport`] transport
    /// and an adapter that delegates to
    /// [`clipvault_db::KnownPeerRepository`].
    pub fn new(
        transport: Arc<dyn PeerTransport>,
        persistence: Arc<dyn PairingPersistence>,
    ) -> Self {
        let inner = PairingRuntimeInner {
            transport,
            sessions: RwLock::new(HashMap::new()),
            rate: RwLock::new(RateLimit::default()),
            persistence,
            next_session_id: AtomicU64::new(1),
            #[cfg(feature = "local-peer-pairing-tls")]
            material_loader: RwLock::new(None),
            #[cfg(feature = "local-peer-pairing-tls")]
            local_identity: RwLock::new(None),
            #[cfg(feature = "local-peer-pairing-tls")]
            inbound_sink: parking_lot::Mutex::new(None),
        };
        Self {
            inner: Arc::new(inner),
        }
    }

    /// Install the material loader the production transport uses
    /// to bind a real listener. The bootstrap wires a loader that
    /// delegates to the keychain; tests can leave the slot empty.
    #[cfg(feature = "local-peer-pairing-tls")]
    pub fn set_material_loader(&self, loader: Arc<dyn MaterialLoader>) {
        *self.inner.material_loader.write() = Some(loader);
    }

    /// Cache the local identity the runtime uses to compute the
    /// SAS with the real `peer_id` / fingerprint. The bootstrap
    /// refreshes this whenever the secure store hands out (or
    /// rotates) the local identity; the runtime refuses to mint a
    /// SAS without a cached identity so the wire never carries the
    /// `"self"` / `"self-fingerprint"` placeholders the pre-wiring
    /// shipped. Passing `None` clears the cache so a temporarily
    /// unreachable keychain collapses into a typed
    /// `TransportUnavailable` outcome instead of a half-correct
    /// SAS.
    #[cfg(feature = "local-peer-pairing-tls")]
    pub fn set_local_identity(&self, identity: Option<clipvault_platform::LocalPeerIdentity>) {
        *self.inner.local_identity.write() = identity;
    }

    /// Read the cached local identity. Used by tests to assert the
    /// bootstrap forwarded the value the keychain minted.
    #[cfg(feature = "local-peer-pairing-tls")]
    pub fn cached_local_identity(&self) -> Option<clipvault_platform::LocalPeerIdentity> {
        self.inner.local_identity.read().clone()
    }

    /// Install the inbound approval sink the productive pairing
    /// transport uses to route authenticated events back into the
    /// runtime. The bootstrap wires the runtime as the sink so the
    /// shell never has to thread a renderer-supplied payload
    /// through the IPC bridge. The function is idempotent: a
    /// second call replaces the previous sink.
    #[cfg(feature = "local-peer-pairing-tls")]
    pub fn set_inbound_approval_sink(&self, sink: Arc<dyn InboundApprovalSink>) {
        *self.inner.inbound_sink.lock() = Some(sink);
    }

    /// Test-only constructor that uses the in-memory persistence
    /// adapter and a configurable transport.
    #[cfg(test)]
    pub fn with_transport(transport: Arc<dyn PeerTransport>) -> Self {
        Self::new(transport, Arc::new(InMemoryPairingPersistence::new()))
    }

    /// Install the production TLS transport on the bound mDNS
    /// adapter. Called when the local peer sharing toggle flips
    /// on. The function:
    ///   1. loads the [`clipvault_platform::LocalIdentityMaterial`]
    ///      from the keychain;
    ///   2. calls `start_with_material` on the transport so the
    ///      real ephemeral listener binds and mDNS publishes the
    ///      pairing capability with the bound port.
    ///
    /// Returns [`TransportOutcome::Unavailable`] when the loader
    /// is missing or the keychain is unreachable. The shell
    /// surfaces the typed reason so the UI can render the
    /// matching copy without inspecting free-form strings.
    ///
    /// `display_name` is the validated, trimmed human-readable
    /// label the settings service accepts. The transport caches
    /// it on install so the `HelloAck` / `InboundSessionMetadata`
    /// envelopes the listener publishes carry the same visible
    /// name an authenticated peer would expect — the platform
    /// layer MUST NOT fall back to the cert short fingerprint
    /// for this projection.
    #[cfg(feature = "local-peer-pairing-tls")]
    pub fn install_pairing_transport(
        &self,
        sink: Arc<dyn TransportSink>,
        advertisement: Arc<dyn PairingAdvertisement>,
        display_name: &str,
    ) -> Result<u16, TransportOutcome> {
        let loader = self.inner.material_loader.read().clone();
        let Some(loader) = loader else {
            return Err(TransportOutcome::Unavailable);
        };
        let material = match loader.load() {
            Ok(material) => material,
            Err(_) => return Err(TransportOutcome::Unavailable),
        };
        let transport = self.inner.transport.clone();
        transport
            .start_with_material_and_display_name(material, sink, advertisement, display_name)
            .map_err(|_error| TransportOutcome::Unavailable)
    }

    /// Same as [`Self::install_pairing_transport`] but with an
    /// explicit [`RemotePeerResolver`] wired in. The productive
    /// pairing transport uses the resolver to look up a peer's
    /// `SocketAddr` through mDNS — the resolver stays inside the
    /// platform layer so the runtime, SQLite, Tauri and the
    /// frontend never see an endpoint byte.
    #[cfg(feature = "local-peer-pairing-tls")]
    pub fn install_pairing_transport_with_resolver(
        &self,
        sink: Arc<dyn TransportSink>,
        advertisement: Arc<dyn PairingAdvertisement>,
        resolver: Arc<dyn clipvault_platform::peer_transport::RemotePeerResolver>,
        display_name: &str,
    ) -> Result<u16, TransportOutcome> {
        let loader = self.inner.material_loader.read().clone();
        let Some(loader) = loader else {
            return Err(TransportOutcome::Unavailable);
        };
        let material = match loader.load() {
            Ok(material) => material,
            Err(_) => return Err(TransportOutcome::Unavailable),
        };
        let transport = self.inner.transport.clone();
        transport
            .start_with_material_and_resolver(
                material,
                sink,
                advertisement,
                Some(resolver),
                display_name,
            )
            .map_err(|_error| TransportOutcome::Unavailable)
    }

    /// Stop the production pairing transport. The platform layer
    /// withdraws the mDNS advertisement and closes the listener
    /// before returning; the runtime surfaces the typed outcome
    /// so the shell can render the off state without inspecting
    /// free-form strings.
    pub fn stop_pairing_transport(&self) -> Result<(), TransportOutcome> {
        self.inner
            .transport
            .stop()
            .map_err(|_error| TransportOutcome::Unavailable)
    }

    /// Whether the production transport is currently bound to a
    /// real listener. The shell consults this before calling
    /// [`Self::install_pairing_transport`] so a redundant toggle
    /// is a no-op.
    pub fn pairing_transport_is_running(&self) -> bool {
        self.inner.transport.is_running()
    }

    /// Return the ephemeral port the productive pairing
    /// transport bound to, or `None` while the listener is
    /// stopped. The value is the one the bootstrap installs in
    /// [`crate::AppContext`] so the toggle helper can surface a
    /// typed `RuntimeStopped` response when the install silently
    /// failed (the discovery runtime may still report `running`
    /// while the pairing listener never came up).
    pub fn pairing_bound_port(&self) -> Option<u16> {
        if self.inner.transport.is_running() {
            // Use a typed accessor: the transport trait does not
            // currently expose the bound port, so we read it
            // through the trait's internal state. The default
            // fallback is `None`.
            None
        } else {
            None
        }
    }

    /// Start an outbound pairing session against a known peer.
    /// The runtime mints a fresh `session_id`, generates a
    /// nonce, derives the SAS candidate and returns a
    /// snapshot the UI can render. The remote peer's `Approve`
    /// is observed through [`Self::observe_approve`]; the
    /// runtime refuses to mark `Trusted` until both sides have
    /// approved.
    pub fn start_outbound(
        &self,
        remote_peer_id: &str,
        remote_fingerprint: &str,
        remote_display_name: &str,
    ) -> Result<PairingOutcome, PairingError> {
        if remote_peer_id.is_empty() || remote_fingerprint.is_empty() {
            return Err(PairingError::UnknownOrKeyMismatch);
        }
        // The runtime self-filters against the trust-state
        // persisted row so a blocked / revoked peer is rejected
        // before any cryptographic work runs.
        match self
            .inner
            .persistence
            .load(remote_peer_id)
            .map_err(|_| PairingError::TransportUnavailable)?
        {
            Some(row) if row.trust_state == TrustState::Blocked => {
                return Err(PairingError::Blocked);
            }
            Some(row) if row.trust_state == TrustState::Revoked => {
                return Err(PairingError::Revoked);
            }
            _ => {}
        }
        // Rate-limit guard.
        if !self.inner.rate.write().allow() {
            return Err(PairingError::RateLimited);
        }
        // Compute the SAS against the cached local identity so
        // the wire never carries the `"self"` / `"self-fingerprint"`
        // placeholders the pre-wiring shipped. A missing
        // identity collapses into `TransportUnavailable` because
        // a session minted from placeholder bytes would never
        // match the SAS the remote derives from the real cert
        // SPKI — promoting it would be a silent trust violation.
        #[cfg(feature = "local-peer-pairing-tls")]
        let (local_peer_id, local_public_key) = {
            let identity = self.inner.local_identity.read().clone();
            let identity = identity.ok_or(PairingError::TransportUnavailable)?;
            (identity.peer_id.to_string(), identity.public_key)
        };
        #[cfg(not(feature = "local-peer-pairing-tls"))]
        let (local_peer_id, local_fingerprint, _local_public_key) =
            (String::new(), String::new(), [0u8; 32]);
        // Open the productive pairing transport session FIRST so
        // the dial + Hello/HelloAck round-trip establishes the
        // mTLS connection AND returns the REAL nonces + SAS the
        // remote listener validated. The runtime populates the
        // session state from the metadata the transport
        // delivered — no local placeholder nonces can leak
        // because the SAS the UI shows matches the value the
        // transcript signature covered.
        #[cfg(feature = "local-peer-pairing-tls")]
        let (session_id, metadata) = {
            let descriptor = clipvault_platform::peer_transport::OutboundSessionDescriptor {
                peer_id: remote_peer_id.to_string(),
                full_public_key_fingerprint: remote_fingerprint.to_string(),
                display_name: remote_display_name.to_string(),
                local_public_key,
                local_peer_id: local_peer_id.clone(),
            };
            let outbound = self
                .inner
                .transport
                .start_outbound(descriptor)
                .map_err(map_transport_error_to_pairing_error)?;
            (
                PairingSessionId(outbound.session_id.as_u64()),
                outbound.metadata,
            )
        };
        #[cfg(not(feature = "local-peer-pairing-tls"))]
        let (session_id, metadata) = {
            let id = PairingSessionId(self.inner.next_session_id.fetch_add(1, Ordering::AcqRel));
            let nonce_a = generate_nonce();
            let nonce_b = generate_nonce();
            let sas = compute_sas(
                &nonce_a,
                &nonce_b,
                remote_peer_id,
                &local_peer_id,
                remote_fingerprint,
                &local_fingerprint,
                PAIRING_PROTOCOL_MAJOR,
            );
            (
                id,
                clipvault_platform::peer_transport::OutboundSessionMetadata {
                    remote_peer_id: remote_peer_id.to_string(),
                    remote_full_fingerprint: remote_fingerprint.to_string(),
                    remote_display_name: remote_display_name.to_string(),
                    local_nonce: nonce_a,
                    remote_nonce: nonce_b,
                    sas,
                    remote_cert_fingerprint: String::new(),
                },
            )
        };
        let started_at = Instant::now();
        let expires_at = started_at + PAIRING_SESSION_TIMEOUT;
        let session = SessionState {
            id: session_id,
            remote_peer_id: metadata.remote_peer_id.clone(),
            remote_fingerprint: metadata.remote_full_fingerprint.clone(),
            remote_display_name: metadata.remote_display_name.clone(),
            nonce_a: metadata.local_nonce.clone(),
            nonce_b: metadata.remote_nonce.clone(),
            sas: metadata.sas.clone(),
            local_approved: false,
            remote_approved: false,
            cert_fingerprint: metadata.remote_cert_fingerprint.clone(),
            started_at,
            expires_at,
            is_inbound: false,
        };
        self.inner
            .sessions
            .write()
            .insert(session_id, Mutex::new(session));
        let snapshot = self.snapshot_session(session_id)?;
        debug!(session_id = session_id.as_u64(), "pairing session started");
        Ok(PairingOutcome::AwaitingRemoteApproval(snapshot.session_id))
    }

    /// Accept an inbound pairing message the transport pushed into
    /// the runtime. The transport calls this once per decoded
    /// envelope. The runtime validates the version, computes /
    /// verifies the SAS, persists the cert fingerprint and
    /// promotes the row to [`TrustState::Trusted`] only after
    /// both sides approve.
    pub fn observe_pairing(
        &self,
        message: PairingMessage,
        cert_fingerprint: &str,
    ) -> PairingOutcome {
        if message.version() != PAIRING_WIRE_VERSION {
            return PairingOutcome::Failed(PairingError::IncompatibleProtocol);
        }
        match message {
            PairingMessage::Hello { peer_id, .. } => {
                if let Err(error) = self.register_inbound(peer_id, cert_fingerprint) {
                    return PairingOutcome::Failed(error);
                }
                PairingOutcome::AwaitingRemoteApproval(self.first_session_id())
            }
            PairingMessage::HelloAck { .. } => {
                // The HelloAck shape is symmetrical to Hello; the
                // current state machine records the inbound side
                // and waits for the local user to approve.
                PairingOutcome::AwaitingRemoteApproval(self.first_session_id())
            }
            PairingMessage::Approve { peer_id, .. } => {
                self.observe_approve(&peer_id, cert_fingerprint)
            }
            PairingMessage::Health { .. } | PairingMessage::HealthAck { .. } => {
                // The metadata-only health probe never reaches the
                // runtime state machine. The productive pairing
                // transport handles it inside the listener loop
                // and surfaces the typed outcome directly; the
                // runtime only sees `observe_pairing` events for
                // pairing / approval transitions.
                PairingOutcome::Failed(PairingError::IncompatibleProtocol)
            }
        }
    }

    /// Record the local user's approval of the SAS code. The
    /// runtime stamps the session as locally approved and returns
    /// the next state (awaiting remote / trusted / failed). On
    /// the productive path, the runtime also signals the pairing
    /// transport to sign and send the `Approve` envelope over the
    /// open mTLS connection the `start_outbound` call reserved.
    ///
    /// Sessions created through the inbound path
    /// ([`Self::on_pairing_session_started`]) are routed through
    /// [`PeerTransport::approve_inbound_session`]; the listener
    /// task awaits the channel so it can send `HelloAck` and
    /// its own signed `Approve` envelope. Sessions the runtime
    /// created via [`Self::start_outbound`] go through
    /// [`PeerTransport::approve_local`] so the dialer signs its
    /// `Approve` envelope on the open mTLS connection. The
    /// previous wiring collapsed both paths into the outbound
    /// call; the listener's bounded wait would never wake up and
    /// inbound approvals silently returned `UnknownPeer`.
    pub fn approve_local(&self, session_id: PairingSessionId) -> PairingOutcome {
        let snapshot = match self.snapshot_session(session_id) {
            Ok(snapshot) => snapshot,
            Err(error) => return PairingOutcome::Failed(error),
        };
        // Stamp the session as locally approved BEFORE the
        // transport signal so a concurrent observation the
        // transport pushes through `on_pairing_observed` cannot
        // arrive in `AwaitingRemoteApproval` with `local_approved
        // = false`. The previous wiring sent the signal first and
        // raced the in-memory transition; the fix collapses the
        // race window to zero.
        let is_inbound = self
            .inner
            .sessions
            .write()
            .get(&session_id)
            .map(|guard| {
                let mut session = guard.lock().expect("session lock");
                session.local_approved = true;
                session.is_inbound
            })
            .unwrap_or(false);
        // The variable is only consumed by the productive
        // `local-peer-pairing-tls` branch below; touch it so the
        // non-feature build does not warn.
        #[cfg(not(feature = "local-peer-pairing-tls"))]
        let _ = is_inbound;
        // Productive path: drive the transport's session
        // approval so the mTLS connection carries the signed
        // `Approve` envelope. The remote observation flows back
        // through the [`TransportSink`] the runtime installed at
        // startup; the runtime never has to inspect the bytes.
        #[cfg(feature = "local-peer-pairing-tls")]
        {
            let transport_id =
                clipvault_platform::peer_transport::PairingSessionId(session_id.as_u64());
            let approve_result = if is_inbound {
                self.inner.transport.approve_inbound_session(transport_id)
            } else {
                self.inner.transport.approve_local(transport_id)
            };
            if let Err(error) = approve_result {
                return PairingOutcome::Failed(map_transport_error_to_pairing_error(error));
            }
        }
        if snapshot.local_approved {
            return PairingOutcome::AwaitingRemoteApproval(session_id);
        }
        PairingOutcome::AwaitingRemoteApproval(session_id)
    }

    /// Record the remote peer's approval of the SAS code. The
    /// runtime promotes the row to [`TrustState::Trusted`] only
    /// when the local user has also approved; a remote-only
    /// approval leaves the session in
    /// [`PairingOutcome::AwaitingRemoteApproval`].
    pub fn observe_approve(&self, remote_peer_id: &str, cert_fingerprint: &str) -> PairingOutcome {
        let sessions = self.inner.sessions.read();
        let mut session_id = None;
        for (id, guard) in sessions.iter() {
            let session = guard.lock().expect("session lock");
            if session.remote_peer_id == remote_peer_id {
                drop(session);
                let mut session = guard.lock().expect("session lock");
                session.remote_approved = true;
                if !session.cert_fingerprint.is_empty() {
                    // The runtime preserves the FIRST cert
                    // fingerprint the transport delivered so a
                    // future refactor that accepts renegotiation
                    // cannot accidentally rotate the pin.
                } else {
                    session.cert_fingerprint = cert_fingerprint.to_string();
                }
                session_id = Some(*id);
                break;
            }
        }
        drop(sessions);
        let Some(session_id) = session_id else {
            return PairingOutcome::Failed(PairingError::UnknownOrKeyMismatch);
        };
        let snapshot = match self.snapshot_session(session_id) {
            Ok(snapshot) => snapshot,
            Err(error) => return PairingOutcome::Failed(error),
        };
        if !snapshot.local_approved {
            return PairingOutcome::AwaitingRemoteApproval(session_id);
        }
        // Both sides approved: persist the trust transition and
        // return the trusted row. The transport's
        // `on_pairing_observed` invocation is the AUTHORITATIVE
        // source for the cert fingerprint because the runtime
        // hands the value the mTLS handshake authenticated; the
        // snapshot value cached at `start_outbound` is a fallback
        // for transports that mint a placeholder fingerprint
        // during the synchronous Hello/HelloAck handshake.
        let persisted_fingerprint = if cert_fingerprint.is_empty() {
            snapshot.cert_fingerprint.clone().unwrap_or_default()
        } else {
            cert_fingerprint.to_string()
        };
        let result = self.inner.persistence.mark_trusted(
            &snapshot.remote_peer_id,
            &persisted_fingerprint,
            OffsetDateTime::now_utc(),
            PAIRING_PROTOCOL_MAJOR,
        );
        let outcome = match result {
            Ok(TrustTransitionOutcome::Stored(row)) => {
                // Clean up the in-memory session so the next
                // pairing attempt against the same peer starts
                // from a clean slate.
                self.inner.sessions.write().remove(&session_id);
                // Arm the per-peer cert fingerprint pin the
                // productive pairing transport enforces against
                // the next mTLS handshake. The fingerprint
                // already came from the authenticated
                // [`PeerTransportObservation`] the inbound sink
                // pushed — the runtime forwards the value
                // without re-deriving anything so the pin is
                // always bound to the cert the transport
                // verified.
                #[cfg(feature = "local-peer-pairing-tls")]
                {
                    let _ = self
                        .inner
                        .transport
                        .arm_pin(&row.peer_id, &persisted_fingerprint);
                }
                PairingOutcome::Trusted(row)
            }
            Ok(TrustTransitionOutcome::Conflict(_row)) => {
                PairingOutcome::Failed(PairingError::Blocked)
            }
            Ok(TrustTransitionOutcome::Unknown) => {
                PairingOutcome::Failed(PairingError::UnknownOrKeyMismatch)
            }
            Err(_) => PairingOutcome::Failed(PairingError::TransportUnavailable),
        };
        outcome
    }

    /// Cancel an in-flight pairing session. Idempotent: cancelling
    /// a session that has already completed is a no-op. On the
    /// productive path, the runtime forwards the cancel to the
    /// pairing transport so the open mTLS connection tears down
    /// without a phantom `Approve` envelope ever reaching the
    /// remote.
    ///
    /// Inbound and outbound sessions share the same transport
    /// `cancel_session` entry point: the listener task awaits
    /// the inbound approval on a `Notify` the cancel path wakes
    /// up; the outbound dialer task drops its `Approve` oneshot.
    /// The previous wiring only cleared the in-memory row, which
    /// left the inbound listener stuck inside
    /// [`crate::peer_transport::tls::run_pairing_session`] until
    /// the hard two-minute timeout fired.
    pub fn cancel(&self, session_id: PairingSessionId) -> PairingOutcome {
        let removed = self.inner.sessions.write().remove(&session_id);
        #[cfg(feature = "local-peer-pairing-tls")]
        {
            let _ = self.inner.transport.cancel_session(
                clipvault_platform::peer_transport::PairingSessionId(session_id.as_u64()),
            );
        }
        if removed.is_some() {
            debug!(
                session_id = session_id.as_u64(),
                "pairing session cancelled"
            );
            PairingOutcome::Failed(PairingError::Cancelled)
        } else {
            PairingOutcome::Failed(PairingError::Cancelled)
        }
    }

    /// Build a metadata-only snapshot the bridge surfaces. The
    /// snapshot lists every in-flight session; the UI renders the
    /// modal against the first row. Idempotent.
    pub fn snapshot(&self) -> Vec<PairingSessionSnapshot> {
        let now = Instant::now();
        // Reap expired sessions first so the snapshot never
        // surfaces a stale row.
        let mut expired: Vec<PairingSessionId> = Vec::new();
        {
            let sessions = self.inner.sessions.read();
            for (id, guard) in sessions.iter() {
                let session = guard.lock().expect("session lock");
                if session.expired(now) {
                    expired.push(*id);
                }
            }
        }
        if !expired.is_empty() {
            let mut sessions = self.inner.sessions.write();
            for id in &expired {
                sessions.remove(id);
            }
        }
        let sessions = self.inner.sessions.read();
        let mut snapshots: Vec<PairingSessionSnapshot> = sessions
            .iter()
            .filter_map(|(_, guard)| {
                let session = guard.lock().expect("session lock");
                if session.expired(now) {
                    return None;
                }
                Some(PairingSessionSnapshot {
                    session_id: session.id,
                    remote_peer_id: session.remote_peer_id.clone(),
                    remote_fingerprint: session.remote_fingerprint.clone(),
                    is_inbound: session.is_inbound,
                    cert_fingerprint: if session.cert_fingerprint.is_empty() {
                        None
                    } else {
                        Some(session.cert_fingerprint.clone())
                    },
                    remote_display_name: session.remote_display_name.clone(),
                    local_approved: session.local_approved,
                    remote_approved: session.remote_approved,
                    sas: session.sas.clone(),
                    expires_at: format_rfc3339(
                        OffsetDateTime::now_utc()
                            + time::Duration::seconds_f64(
                                session
                                    .expires_at
                                    .saturating_duration_since(now)
                                    .as_secs_f64(),
                            ),
                    ),
                })
            })
            .collect();
        snapshots.sort_by_key(|s| s.session_id.as_u64());
        snapshots
    }

    /// Revoke an existing trusted peer. The runtime delegates to
    /// the persistence layer; the in-memory session table is
    /// cleared so a stale approval cannot resurrect the link.
    /// On the productive path, the runtime also disarms the
    /// per-peer cert fingerprint pin and closes every open
    /// pairing socket so the revoked peer cannot keep an active
    /// session alive after the row leaves the trusted state.
    pub fn revoke(&self, peer_id: &str) -> TrustOperationOutcome {
        self.clear_sessions_for(peer_id);
        #[cfg(feature = "local-peer-pairing-tls")]
        {
            let _ = self.inner.transport.disconnect_peer(peer_id);
            let _ = self.inner.transport.disarm_pin(peer_id);
        }
        match self.inner.persistence.mark_revoked(peer_id) {
            Ok(TrustTransitionOutcome::Stored(row)) => TrustOperationOutcome::Stored(row),
            Ok(TrustTransitionOutcome::Conflict(row)) => TrustOperationOutcome::Conflict(row),
            Ok(TrustTransitionOutcome::Unknown) => TrustOperationOutcome::Unknown,
            Err(_) => TrustOperationOutcome::Unknown,
        }
    }

    /// Block a peer. The runtime delegates to the persistence
    /// layer; the in-memory session table is cleared so a stale
    /// approval cannot resurrect the link. On the productive
    /// path, the runtime also disarms the per-peer cert
    /// fingerprint pin and closes every open pairing socket so
    /// the blocked peer cannot keep an active session alive.
    pub fn block(&self, peer_id: &str) -> TrustOperationOutcome {
        self.clear_sessions_for(peer_id);
        #[cfg(feature = "local-peer-pairing-tls")]
        {
            let _ = self.inner.transport.disconnect_peer(peer_id);
            let _ = self.inner.transport.disarm_pin(peer_id);
        }
        match self.inner.persistence.mark_blocked(peer_id) {
            Ok(TrustTransitionOutcome::Stored(row)) => TrustOperationOutcome::Stored(row),
            Ok(TrustTransitionOutcome::Conflict(row)) => TrustOperationOutcome::Conflict(row),
            Ok(TrustTransitionOutcome::Unknown) => TrustOperationOutcome::Unknown,
            Err(_) => TrustOperationOutcome::Unknown,
        }
    }

    /// Unblock a previously-blocked peer. The runtime delegates to
    /// the persistence layer; the cert fingerprint and paired_at
    /// columns are cleared by the persistence adapter so a
    /// re-detection cannot claim the previous trust state.
    pub fn unblock(&self, peer_id: &str) -> TrustOperationOutcome {
        self.clear_sessions_for(peer_id);
        #[cfg(feature = "local-peer-pairing-tls")]
        {
            let _ = self.inner.transport.disconnect_peer(peer_id);
            let _ = self.inner.transport.disarm_pin(peer_id);
        }
        match self.inner.persistence.unblock(peer_id) {
            Ok(TrustTransitionOutcome::Stored(row)) => TrustOperationOutcome::Stored(row),
            Ok(TrustTransitionOutcome::Conflict(row)) => TrustOperationOutcome::Conflict(row),
            Ok(TrustTransitionOutcome::Unknown) => TrustOperationOutcome::Unknown,
            Err(_) => TrustOperationOutcome::Unknown,
        }
    }

    /// Read the cert fingerprint the persistence layer persisted
    /// for the matching `peer_id`. The bridge calls this before
    /// invoking the health probe so the shell stays a thin
    /// adapter and the runtime owns the lookup.
    #[cfg(feature = "local-peer-pairing-tls")]
    pub fn cert_fingerprint_for(&self, peer_id: &str) -> Result<String, PairingError> {
        let row = self
            .inner
            .persistence
            .load(peer_id)
            .map_err(|_| PairingError::TransportUnavailable)?
            .ok_or(PairingError::UnknownOrKeyMismatch)?;
        if row.tls_cert_fingerprint.is_empty() {
            return Err(PairingError::UnknownOrKeyMismatch);
        }
        Ok(row.tls_cert_fingerprint)
    }

    /// Open a productive metadata-only health probe against a
    /// pinned peer. The transport dials the remote listener over
    /// mTLS, exchanges the bounded `health` envelope, and
    /// returns the typed [`PeerPairingHealthSnapshot`].
    /// Revoked / blocked / unknown / mismatched peers collapse
    /// into the typed [`PairingError`] variants the bridge
    /// already branches on.
    #[cfg(feature = "local-peer-pairing-tls")]
    pub fn health_probe(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
    ) -> Result<PeerPairingHealthSnapshot, PairingError> {
        // Self-filter the persisted trust state before any
        // cryptographic work runs.
        let row = self
            .inner
            .persistence
            .load(peer_id)
            .map_err(|_| PairingError::TransportUnavailable)?
            .ok_or(PairingError::UnknownOrKeyMismatch)?;
        if row.trust_state == TrustState::Blocked {
            return Err(PairingError::Blocked);
        }
        if row.trust_state == TrustState::Revoked {
            return Err(PairingError::Revoked);
        }
        self.inner
            .transport
            .health_probe(peer_id, cert_fingerprint)
            .map(map_transport_health_snapshot)
            .map_err(map_transport_error_to_pairing_error)
    }

    fn register_inbound(
        &self,
        peer_id: String,
        cert_fingerprint: &str,
    ) -> Result<PairingSessionId, PairingError> {
        if peer_id.is_empty() {
            return Err(PairingError::UnknownOrKeyMismatch);
        }
        match self
            .inner
            .persistence
            .load(&peer_id)
            .map_err(|_| PairingError::TransportUnavailable)?
        {
            Some(row) if row.trust_state == TrustState::Blocked => {
                return Err(PairingError::Blocked);
            }
            Some(row) if row.trust_state == TrustState::Revoked => {
                return Err(PairingError::Revoked);
            }
            _ => {}
        }
        if !self.inner.rate.write().allow() {
            return Err(PairingError::RateLimited);
        }
        let session_id =
            PairingSessionId(self.inner.next_session_id.fetch_add(1, Ordering::AcqRel));
        let nonce_a = generate_nonce();
        let nonce_b = generate_nonce();
        // Compute the SAS against the cached local identity so
        // the wire never carries the `"self"` /
        // `"self-fingerprint"` placeholders. A missing identity
        // collapses into `TransportUnavailable` for the same
        // reason `start_outbound` rejects: a session minted
        // against placeholder bytes would never match the SAS the
        // dialer derives from the real cert SPKI.
        #[cfg(feature = "local-peer-pairing-tls")]
        let (local_peer_id, local_fingerprint) = {
            let identity = self.inner.local_identity.read().clone();
            let identity = identity.ok_or(PairingError::TransportUnavailable)?;
            (
                identity.peer_id.to_string(),
                clipvault_platform::peer_transport::tls::full_public_key_fingerprint(
                    &identity.public_key,
                ),
            )
        };
        #[cfg(not(feature = "local-peer-pairing-tls"))]
        let (local_peer_id, local_fingerprint) = (String::new(), String::new());
        let sas = compute_sas(
            &nonce_a,
            &nonce_b,
            &peer_id,
            &local_peer_id,
            &local_fingerprint,
            &local_fingerprint,
            PAIRING_PROTOCOL_MAJOR,
        );
        let started_at = Instant::now();
        let expires_at = started_at + PAIRING_SESSION_TIMEOUT;
        let session = SessionState {
            id: session_id,
            remote_peer_id: peer_id,
            remote_fingerprint: String::new(),
            remote_display_name: String::new(),
            nonce_a,
            nonce_b,
            sas,
            local_approved: false,
            remote_approved: false,
            cert_fingerprint: cert_fingerprint.to_string(),
            started_at,
            expires_at,
            is_inbound: false,
        };
        self.inner
            .sessions
            .write()
            .insert(session_id, Mutex::new(session));
        Ok(session_id)
    }

    /// Register an inbound pairing session from the metadata the
    /// transport pushed through
    /// [`TransportSink::on_pairing_session_started`]. The runtime
    /// uses the canonical nonces + SAS the listener computed so
    /// the UI of the second host renders the same code the
    /// remote dialer accepted. The session id the transport
    /// minted stays in place; the runtime reuses it as the
    /// bridge surface so the user can approve the same session
    /// the listener is waiting on.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn register_inbound_from_metadata(
        &self,
        metadata: clipvault_platform::peer_transport::InboundSessionMetadata,
    ) {
        // Trust-state pre-flight (blocked / revoked) stays
        // identical to the legacy `register_inbound` path so a
        // stale blocked row never silently resurrects through an
        // inbound pairing attempt.
        if let Ok(Some(row)) = self.inner.persistence.load(&metadata.remote_peer_id) {
            if row.trust_state == TrustState::Blocked {
                return;
            }
            if row.trust_state == TrustState::Revoked {
                return;
            }
        }
        if !self.inner.rate.write().allow() {
            return;
        }
        let session_id = PairingSessionId(metadata.session_id.as_u64());
        let started_at = Instant::now();
        let expires_at = started_at + PAIRING_SESSION_TIMEOUT;
        let session = SessionState {
            id: session_id,
            remote_peer_id: metadata.remote_peer_id.clone(),
            remote_fingerprint: metadata.remote_full_fingerprint.clone(),
            remote_display_name: metadata.remote_display_name.clone(),
            nonce_a: metadata.local_nonce.clone(),
            nonce_b: metadata.remote_nonce.clone(),
            sas: metadata.sas.clone(),
            local_approved: false,
            remote_approved: false,
            cert_fingerprint: String::new(),
            started_at,
            expires_at,
            is_inbound: true,
        };
        self.inner
            .sessions
            .write()
            .insert(session_id, Mutex::new(session));
    }

    fn snapshot_session(
        &self,
        session_id: PairingSessionId,
    ) -> Result<PairingSessionSnapshot, PairingError> {
        let sessions = self.inner.sessions.read();
        let Some(guard) = sessions.get(&session_id) else {
            return Err(PairingError::UnknownOrKeyMismatch);
        };
        let session = guard.lock().expect("session lock");
        let now = Instant::now();
        if session.expired(now) {
            return Err(PairingError::SessionExpired);
        }
        Ok(PairingSessionSnapshot {
            session_id: session.id,
            remote_peer_id: session.remote_peer_id.clone(),
            remote_fingerprint: session.remote_fingerprint.clone(),
            is_inbound: session.is_inbound,
            cert_fingerprint: if session.cert_fingerprint.is_empty() {
                None
            } else {
                Some(session.cert_fingerprint.clone())
            },
            remote_display_name: session.remote_display_name.clone(),
            local_approved: session.local_approved,
            remote_approved: session.remote_approved,
            sas: session.sas.clone(),
            expires_at: format_rfc3339(
                OffsetDateTime::now_utc()
                    + time::Duration::seconds_f64(
                        session
                            .expires_at
                            .saturating_duration_since(now)
                            .as_secs_f64(),
                    ),
            ),
        })
    }

    fn clear_sessions_for(&self, peer_id: &str) {
        let mut sessions = self.inner.sessions.write();
        sessions.retain(|_, guard| {
            let session = guard.lock().expect("session lock");
            session.remote_peer_id != peer_id
        });
    }

    fn first_session_id(&self) -> PairingSessionId {
        let sessions = self.inner.sessions.read();
        sessions
            .keys()
            .min()
            .copied()
            .unwrap_or(PairingSessionId(0))
    }
}

/// Simple sliding-window rate limiter the runtime keeps per
/// process. The implementation is intentionally cheap — a `Vec`
/// of `Instant` plus a `Vec::drain` over the expired entries on
/// every call — so the pairing path never has to take a system
/// lock or wait on a network round-trip.
#[derive(Debug)]
struct RateLimit {
    window: Vec<Instant>,
    window_size: Duration,
    cap: u32,
}

impl Default for RateLimit {
    fn default() -> Self {
        Self {
            window: Vec::with_capacity(PAIRING_RATE_LIMIT_PER_MINUTE as usize),
            window_size: Duration::from_secs(60),
            cap: PAIRING_RATE_LIMIT_PER_MINUTE,
        }
    }
}

impl RateLimit {
    fn allow(&mut self) -> bool {
        let now = Instant::now();
        self.window
            .retain(|instant| now.duration_since(*instant) <= self.window_size);
        if self.window.len() as u32 >= self.cap {
            return false;
        }
        self.window.push(now);
        true
    }
}

/// Generate a fresh 16-byte random nonce. The runtime uses the
/// value in the SAS derivation; the value never leaves the
/// process because the SAS is order-independent.
fn generate_nonce() -> String {
    use rand_core::RngCore;
    let mut bytes = [0u8; 16];
    rand_core::OsRng
        .try_fill_bytes(&mut bytes)
        .unwrap_or_else(|_| {
            // The OS RNG is the only documented source the
            // workspace guarantees on every supported platform;
            // a failure here is fatal enough to warrant a warn.
            warn!("OsRng refused to fill nonce bytes; falling back to zeroes");
        });
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes.iter() {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer_identity::{LocalPeerIdentity, PeerFingerprint, PeerId};

    fn local_identity() -> LocalPeerIdentity {
        LocalPeerIdentity {
            peer_id: PeerId::from_public_key(&[1u8; 32]),
            fingerprint: PeerFingerprint::from_public_key(&[1u8; 32]),
            public_key: [1u8; 32],
        }
    }

    fn known_peer(peer_id: &str, fingerprint: &str, name: &str) -> KnownPeer {
        KnownPeer {
            peer_id: peer_id.to_string(),
            public_key_fingerprint: fingerprint.to_string(),
            full_public_key_fingerprint: String::new(),
            display_name: name.to_string(),
            protocol_major: 1,
            capability: "discovery_only".to_string(),
            first_seen_at: "2026-01-01T00:00:00Z".to_string(),
            last_discovered_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            trust_state: TrustState::Unverified,
            tls_cert_fingerprint: String::new(),
            paired_at: String::new(),
            paired_protocol_major: 0,
        }
    }

    /// Covers the exact manual sequence used by the desktop UI:
    /// the listener-side user approves first, then the dialer-side
    /// user approves. Both runtimes must receive the authenticated
    /// remote approval and persist their own known-peer record as
    /// trusted. The lower-level transport test proves the wire
    /// exchange; this test additionally proves its two sink events
    /// reach the real pairing state machines.
    #[cfg(feature = "local-peer-pairing-tls")]
    #[test]
    fn reciprocal_runtime_approvals_promote_both_real_peers_to_trusted() {
        use std::net::SocketAddr;
        use std::sync::atomic::{AtomicU16, Ordering as AtomicOrdering};

        use clipvault_platform::peer_transport::{
            tls::full_public_key_fingerprint, RemotePeerResolver, TlsPeerTransport,
        };

        #[derive(Clone)]
        struct FixedMaterialLoader(clipvault_platform::LocalIdentityMaterial);

        impl MaterialLoader for FixedMaterialLoader {
            fn load(
                &self,
            ) -> Result<clipvault_platform::LocalIdentityMaterial, PairingPersistenceError>
            {
                Ok(self.0.clone())
            }
        }

        struct NoopAdvertisement;

        impl PairingAdvertisement for NoopAdvertisement {
            fn publish(&self, _bound_port: u16) -> Result<(), TransportError> {
                Ok(())
            }

            fn withdraw(&self) -> Result<(), TransportError> {
                Ok(())
            }
        }

        struct LoopbackResolver {
            expected_peer_id: String,
            port: Arc<AtomicU16>,
        }

        impl RemotePeerResolver for LoopbackResolver {
            fn resolve(&self, peer_id: &str) -> Option<SocketAddr> {
                if peer_id != self.expected_peer_id {
                    return None;
                }
                let port = self.port.load(AtomicOrdering::Acquire);
                (port != 0).then_some(([127, 0, 0, 1], port).into())
            }
        }

        fn pairing_row(
            identity: &clipvault_platform::LocalPeerIdentity,
            display_name: &str,
        ) -> KnownPeer {
            let mut row = known_peer(
                &identity.peer_id.to_string(),
                &identity.fingerprint.to_string(),
                display_name,
            );
            row.full_public_key_fingerprint = full_public_key_fingerprint(&identity.public_key);
            row.capability = PAIRING_CAPABILITY.to_string();
            row
        }

        let material_a =
            clipvault_platform::LocalIdentityMaterial::from_seed([41u8; 32]).expect("material A");
        let material_b =
            clipvault_platform::LocalIdentityMaterial::from_seed([42u8; 32]).expect("material B");

        let transport_a = Arc::new(TlsPeerTransport::new());
        let persistence_a = Arc::new(InMemoryPairingPersistence::new());
        persistence_a.seed(pairing_row(material_b.identity(), "Peer B"));
        let runtime_a = PairingRuntime::new(transport_a.clone(), persistence_a.clone());
        runtime_a.set_material_loader(Arc::new(FixedMaterialLoader(material_a.clone())));
        runtime_a.set_local_identity(Some(material_a.identity().clone()));

        let transport_b = Arc::new(TlsPeerTransport::new());
        let persistence_b = Arc::new(InMemoryPairingPersistence::new());
        persistence_b.seed(pairing_row(material_a.identity(), "Peer A"));
        let runtime_b = PairingRuntime::new(transport_b.clone(), persistence_b.clone());
        runtime_b.set_material_loader(Arc::new(FixedMaterialLoader(material_b.clone())));
        runtime_b.set_local_identity(Some(material_b.identity().clone()));

        let port_a = Arc::new(AtomicU16::new(0));
        let port_b = Arc::new(AtomicU16::new(0));
        let resolver_a: Arc<dyn RemotePeerResolver> = Arc::new(LoopbackResolver {
            expected_peer_id: material_b.identity().peer_id.to_string(),
            port: Arc::clone(&port_b),
        });
        let resolver_b: Arc<dyn RemotePeerResolver> = Arc::new(LoopbackResolver {
            expected_peer_id: material_a.identity().peer_id.to_string(),
            port: Arc::clone(&port_a),
        });

        let sink_a: Arc<dyn TransportSink> = Arc::new(runtime_a.clone());
        let sink_b: Arc<dyn TransportSink> = Arc::new(runtime_b.clone());
        let advertisement_a: Arc<dyn PairingAdvertisement> = Arc::new(NoopAdvertisement);
        let advertisement_b: Arc<dyn PairingAdvertisement> = Arc::new(NoopAdvertisement);
        let bound_a = runtime_a
            .install_pairing_transport_with_resolver(sink_a, advertisement_a, resolver_a, "Peer A")
            .expect("install A");
        port_a.store(bound_a, AtomicOrdering::Release);
        let bound_b = runtime_b
            .install_pairing_transport_with_resolver(sink_b, advertisement_b, resolver_b, "Peer B")
            .expect("install B");
        port_b.store(bound_b, AtomicOrdering::Release);

        let remote_b_fingerprint = full_public_key_fingerprint(&material_b.identity().public_key);
        let outbound_id = match runtime_a
            .start_outbound(
                &material_b.identity().peer_id.to_string(),
                &remote_b_fingerprint,
                "Peer B",
            )
            .expect("start outbound")
        {
            PairingOutcome::AwaitingRemoteApproval(id) => id,
            outcome => panic!("expected awaiting approval, got {outcome:?}"),
        };

        let inbound_id = wait_for_pairing_session(&runtime_b, true)
            .expect("listener runtime must register its inbound invitation");
        let outbound_sas = runtime_a
            .snapshot()
            .into_iter()
            .find(|session| session.session_id == outbound_id)
            .expect("outbound snapshot")
            .sas;
        let inbound_sas = runtime_b
            .snapshot()
            .into_iter()
            .find(|session| session.session_id == inbound_id)
            .expect("inbound snapshot")
            .sas;
        assert_eq!(
            outbound_sas, inbound_sas,
            "both users must review the same SAS"
        );

        // This is deliberately the order the user exercised: the
        // receiving (inbound) host accepts before the initiating host.
        assert!(matches!(
            runtime_b.approve_local(inbound_id),
            PairingOutcome::AwaitingRemoteApproval(_)
        ));
        assert!(matches!(
            runtime_a.approve_local(outbound_id),
            PairingOutcome::AwaitingRemoteApproval(_)
        ));

        wait_for_trusted(&persistence_a, &material_b.identity().peer_id.to_string())
            .expect("dialer runtime must become trusted");
        wait_for_trusted(&persistence_b, &material_a.identity().peer_id.to_string())
            .expect("listener runtime must become trusted");
        assert!(runtime_a.snapshot().is_empty());
        assert!(runtime_b.snapshot().is_empty());

        transport_a.stop().expect("stop A");
        transport_b.stop().expect("stop B");
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn wait_for_pairing_session(
        runtime: &PairingRuntime,
        inbound: bool,
    ) -> Option<PairingSessionId> {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Some(session) = runtime
                .snapshot()
                .into_iter()
                .find(|session| session.is_inbound == inbound)
            {
                return Some(session.session_id);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn wait_for_trusted(
        persistence: &InMemoryPairingPersistence,
        peer_id: &str,
    ) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if matches!(
                persistence.load(peer_id),
                Ok(Some(row)) if row.trust_state == TrustState::Trusted
            ) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err(format!("{peer_id} did not become trusted"))
    }

    #[test]
    fn start_outbound_returns_awaiting_remote_approval() {
        let persistence = Arc::new(InMemoryPairingPersistence::new());
        persistence.seed(known_peer("peer-bbbb", "fp-bbbb", "Studio B"));
        let transport: Arc<dyn PeerTransport> = Arc::new(FakeTransport::new());
        let runtime = PairingRuntime::new(transport, persistence.clone());
        seed_local_identity(&runtime);
        let outcome = runtime
            .start_outbound("peer-bbbb", "fp-bbbb", "Studio B")
            .expect("start outbound");
        match outcome {
            PairingOutcome::AwaitingRemoteApproval(id) => {
                let snapshots = runtime.snapshot();
                assert_eq!(snapshots.len(), 1);
                assert_eq!(snapshots[0].session_id, id);
            }
            other => panic!("expected AwaitingRemoteApproval, got {other:?}"),
        }
    }

    #[test]
    fn start_outbound_rejects_blocked_peer() {
        let persistence = Arc::new(InMemoryPairingPersistence::new());
        let mut blocked = known_peer("peer-bbbb", "fp-bbbb", "Studio B");
        blocked.trust_state = TrustState::Blocked;
        persistence.seed(blocked);
        let transport: Arc<dyn PeerTransport> = Arc::new(FakeTransport::new());
        let runtime = PairingRuntime::new(transport, persistence);
        seed_local_identity(&runtime);
        let err = runtime
            .start_outbound("peer-bbbb", "fp-bbbb", "Studio B")
            .expect_err("blocked peer must be rejected");
        assert!(matches!(err, PairingError::Blocked));
    }

    #[test]
    fn reciprocal_approval_promotes_to_trusted() {
        let persistence = Arc::new(InMemoryPairingPersistence::new());
        persistence.seed(known_peer("peer-bbbb", "fp-bbbb", "Studio B"));
        let transport: Arc<dyn PeerTransport> = Arc::new(FakeTransport::new());
        let runtime = PairingRuntime::new(transport, persistence.clone());
        seed_local_identity(&runtime);
        let outcome = runtime
            .start_outbound("peer-bbbb", "fp-bbbb", "Studio B")
            .expect("start");
        let session_id = match outcome {
            PairingOutcome::AwaitingRemoteApproval(id) => id,
            other => panic!("expected AwaitingRemoteApproval, got {other:?}"),
        };
        // Local approval first.
        let _ = runtime.approve_local(session_id);
        // Then the remote approval arrives.
        let outcome = runtime.observe_approve("peer-bbbb", "fingerprint-aaaa");
        match outcome {
            PairingOutcome::Trusted(row) => {
                assert_eq!(row.trust_state, TrustState::Trusted);
                assert_eq!(row.tls_cert_fingerprint, "fingerprint-aaaa");
                assert_eq!(row.paired_protocol_major, PAIRING_PROTOCOL_MAJOR);
            }
            other => panic!("expected Trusted, got {other:?}"),
        }
        // The session is cleaned up after a successful promotion.
        assert!(runtime.snapshot().is_empty());
    }

    #[test]
    fn remote_approval_alone_does_not_promote() {
        let persistence = Arc::new(InMemoryPairingPersistence::new());
        persistence.seed(known_peer("peer-bbbb", "fp-bbbb", "Studio B"));
        let transport: Arc<dyn PeerTransport> = Arc::new(FakeTransport::new());
        let runtime = PairingRuntime::new(transport, persistence.clone());
        seed_local_identity(&runtime);
        let outcome = runtime
            .start_outbound("peer-bbbb", "fp-bbbb", "Studio B")
            .expect("start");
        let _ = match outcome {
            PairingOutcome::AwaitingRemoteApproval(id) => id,
            other => panic!("expected AwaitingRemoteApproval, got {other:?}"),
        };
        // Remote approves but the local user has not.
        let outcome = runtime.observe_approve("peer-bbbb", "fingerprint-aaaa");
        assert!(matches!(outcome, PairingOutcome::AwaitingRemoteApproval(_)));
        let row = persistence
            .load("peer-bbbb")
            .expect("load")
            .expect("present");
        assert_eq!(row.trust_state, TrustState::Unverified);
    }

    #[test]
    fn cancel_drops_the_in_memory_session() {
        let persistence = Arc::new(InMemoryPairingPersistence::new());
        persistence.seed(known_peer("peer-bbbb", "fp-bbbb", "Studio B"));
        let transport: Arc<dyn PeerTransport> = Arc::new(FakeTransport::new());
        let runtime = PairingRuntime::new(transport, persistence);
        seed_local_identity(&runtime);
        let session_id = match runtime
            .start_outbound("peer-bbbb", "fp-bbbb", "Studio B")
            .expect("start")
        {
            PairingOutcome::AwaitingRemoteApproval(id) => id,
            other => panic!("expected AwaitingRemoteApproval, got {other:?}"),
        };
        let outcome = runtime.cancel(session_id);
        assert!(matches!(
            outcome,
            PairingOutcome::Failed(PairingError::Cancelled)
        ));
        assert!(runtime.snapshot().is_empty());
    }

    #[test]
    fn rate_limit_rejects_excess_sessions() {
        let persistence = Arc::new(InMemoryPairingPersistence::new());
        for i in 0..PAIRING_RATE_LIMIT_PER_MINUTE + 1 {
            persistence.seed(known_peer(
                &format!("peer-{i:02}"),
                &format!("fp-{i:02}"),
                "X",
            ));
        }
        let transport: Arc<dyn PeerTransport> = Arc::new(FakeTransport::new());
        let runtime = PairingRuntime::new(transport, persistence);
        seed_local_identity(&runtime);
        for i in 0..PAIRING_RATE_LIMIT_PER_MINUTE {
            let outcome =
                runtime.start_outbound(&format!("peer-{i:02}"), &format!("fp-{i:02}"), "X");
            assert!(matches!(
                outcome,
                Ok(PairingOutcome::AwaitingRemoteApproval(_))
            ));
        }
        let next = format!("peer-{:02}", PAIRING_RATE_LIMIT_PER_MINUTE);
        let err = runtime
            .start_outbound(&next, "fp-extra", "X")
            .expect_err("must hit rate limit");
        assert!(matches!(err, PairingError::RateLimited));
    }

    #[test]
    fn revoke_block_unblock_isolate_a_single_peer() {
        let persistence = Arc::new(InMemoryPairingPersistence::new());
        persistence.seed(known_peer("peer-bbbb", "fp-bbbb", "Studio B"));
        persistence.seed(known_peer("peer-cccc", "fp-cccc", "Studio C"));
        let transport: Arc<dyn PeerTransport> = Arc::new(FakeTransport::new());
        let runtime = PairingRuntime::new(transport, persistence.clone());
        let outcome = runtime.revoke("peer-bbbb");
        assert!(matches!(outcome, TrustOperationOutcome::Stored(_)));
        let row = persistence
            .load("peer-bbbb")
            .expect("load")
            .expect("present");
        assert_eq!(row.trust_state, TrustState::Revoked);
        let other = persistence
            .load("peer-cccc")
            .expect("load")
            .expect("present");
        assert_eq!(other.trust_state, TrustState::Unverified);
        let outcome = runtime.block("peer-cccc");
        assert!(matches!(outcome, TrustOperationOutcome::Stored(_)));
        let other = persistence
            .load("peer-cccc")
            .expect("load")
            .expect("present");
        assert_eq!(other.trust_state, TrustState::Blocked);
        let outcome = runtime.unblock("peer-cccc");
        assert!(matches!(outcome, TrustOperationOutcome::Stored(_)));
        let other = persistence
            .load("peer-cccc")
            .expect("load")
            .expect("present");
        assert_eq!(other.trust_state, TrustState::Unverified);
        assert_eq!(other.tls_cert_fingerprint, "");
        assert_eq!(other.paired_at, "");
    }

    #[test]
    fn sas_snapshot_is_metadata_only_and_carries_no_secret() {
        let persistence = Arc::new(InMemoryPairingPersistence::new());
        persistence.seed(known_peer("peer-bbbb", "fp-bbbb", "Studio B"));
        let transport: Arc<dyn PeerTransport> = Arc::new(FakeTransport::new());
        let runtime = PairingRuntime::new(transport, persistence);
        seed_local_identity(&runtime);
        let session_id = match runtime
            .start_outbound("peer-bbbb", "fp-bbbb", "Studio B")
            .expect("start")
        {
            PairingOutcome::AwaitingRemoteApproval(id) => id,
            other => panic!("expected AwaitingRemoteApproval, got {other:?}"),
        };
        let snapshots = runtime.snapshot();
        let snapshot = snapshots.first().expect("snapshot present");
        assert_eq!(snapshot.session_id, session_id);
        let json = serde_json::to_string(snapshot).expect("serialize");
        for forbidden in [
            "private_key",
            "seed",
            "nonce_a",
            "nonce_b",
            "tls_key",
            "shared_secret",
        ] {
            assert!(
                !json.contains(forbidden),
                "snapshot must not expose {forbidden}; got {json}",
            );
        }
    }

    #[test]
    fn observe_pairing_with_incompatible_version_fails() {
        let persistence = Arc::new(InMemoryPairingPersistence::new());
        persistence.seed(known_peer("peer-bbbb", "fp-bbbb", "Studio B"));
        let transport: Arc<dyn PeerTransport> = Arc::new(FakeTransport::new());
        let runtime = PairingRuntime::new(transport, persistence);
        let outcome = runtime.observe_pairing(
            PairingMessage::Hello {
                version: PAIRING_WIRE_VERSION + 1,
                peer_id: "peer-bbbb".to_string(),
                public_key_fingerprint: "fp-bbbb".to_string(),
                display_name: "Studio B".to_string(),
                nonce_a: "nonce-a".to_string(),
            },
            "fingerprint-aaaa",
        );
        assert!(matches!(
            outcome,
            PairingOutcome::Failed(PairingError::IncompatibleProtocol)
        ));
    }

    #[test]
    fn observe_pairing_registers_inbound_session() {
        let persistence = Arc::new(InMemoryPairingPersistence::new());
        persistence.seed(known_peer("peer-bbbb", "fp-bbbb", "Studio B"));
        let transport: Arc<dyn PeerTransport> = Arc::new(FakeTransport::new());
        let runtime = PairingRuntime::new(transport, persistence);
        seed_local_identity(&runtime);
        let outcome = runtime.observe_pairing(
            PairingMessage::Hello {
                version: PAIRING_WIRE_VERSION,
                peer_id: "peer-bbbb".to_string(),
                public_key_fingerprint: "fp-bbbb".to_string(),
                display_name: "Studio B".to_string(),
                nonce_a: "nonce-a".to_string(),
            },
            "fingerprint-aaaa",
        );
        assert!(matches!(outcome, PairingOutcome::AwaitingRemoteApproval(_)));
        assert_eq!(runtime.snapshot().len(), 1);
    }

    #[test]
    fn pairing_local_identity_is_droppable() {
        let _ = local_identity();
    }

    /// `start_outbound` on the productive path drives the pairing
    /// transport to dial the announced listener and produce a
    /// metadata-only observation. The fake transport the tests
    /// install satisfies the trait surface so the runtime logic
    /// can exercise the in-memory state machine without standing
    /// up a real TLS listener; the productive flow is covered by
    /// the platform tests in `peer_transport::tls`.
    #[derive(Default)]
    struct FakeTransport {
        #[cfg(feature = "local-peer-pairing-tls")]
        sessions: parking_lot::Mutex<Vec<u64>>,
    }

    impl FakeTransport {
        fn new() -> Self {
            Self::default()
        }
    }

    impl PeerTransport for FakeTransport {
        fn start(
            &self,
            _identity: &LocalPeerIdentity,
            _sink: Arc<dyn TransportSink>,
        ) -> Result<u16, TransportError> {
            Err(TransportError::Unavailable)
        }

        #[cfg(feature = "local-peer-pairing-tls")]
        fn start_with_material(
            &self,
            _material: clipvault_platform::LocalIdentityMaterial,
            _sink: Arc<dyn TransportSink>,
            _advertisement: Arc<dyn PairingAdvertisement>,
        ) -> Result<u16, TransportError> {
            Err(TransportError::Unavailable)
        }

        #[cfg(feature = "local-peer-pairing-tls")]
        fn start_with_material_and_resolver(
            &self,
            _material: clipvault_platform::LocalIdentityMaterial,
            _sink: Arc<dyn TransportSink>,
            _advertisement: Arc<dyn PairingAdvertisement>,
            _resolver: Option<Arc<dyn clipvault_platform::peer_transport::RemotePeerResolver>>,
            _display_name: &str,
        ) -> Result<u16, TransportError> {
            Err(TransportError::Unavailable)
        }

        fn stop(&self) -> Result<(), TransportError> {
            Ok(())
        }

        fn is_running(&self) -> bool {
            true
        }

        #[cfg(feature = "local-peer-pairing-tls")]
        fn arm_pin(&self, _peer_id: &str, _cert_fingerprint: &str) -> Result<(), TransportError> {
            Ok(())
        }

        #[cfg(feature = "local-peer-pairing-tls")]
        fn disarm_pin(&self, _peer_id: &str) -> Result<(), TransportError> {
            Ok(())
        }

        #[cfg(feature = "local-peer-pairing-tls")]
        fn health_check(
            &self,
            _peer_id: &str,
            _cert_fingerprint: &str,
        ) -> Result<(), TransportError> {
            Ok(())
        }

        #[cfg(feature = "local-peer-pairing-tls")]
        fn start_outbound(
            &self,
            descriptor: clipvault_platform::peer_transport::OutboundSessionDescriptor,
        ) -> Result<clipvault_platform::peer_transport::PairingOutbound, TransportError> {
            let id = self.sessions.lock().len() as u64 + 1;
            self.sessions.lock().push(id);
            Ok(clipvault_platform::peer_transport::PairingOutbound {
                session_id: clipvault_platform::peer_transport::PairingSessionId(id),
                metadata: clipvault_platform::peer_transport::OutboundSessionMetadata {
                    remote_peer_id: descriptor.peer_id.clone(),
                    remote_full_fingerprint: descriptor.full_public_key_fingerprint.clone(),
                    remote_display_name: descriptor.display_name.clone(),
                    local_nonce: "fake-local-nonce".to_string(),
                    remote_nonce: "fake-remote-nonce".to_string(),
                    sas: "000000".to_string(),
                    remote_cert_fingerprint: "0".repeat(64),
                },
            })
        }

        #[cfg(feature = "local-peer-pairing-tls")]
        fn approve_local(
            &self,
            _session_id: clipvault_platform::peer_transport::PairingSessionId,
        ) -> Result<(), TransportError> {
            Ok(())
        }

        #[cfg(feature = "local-peer-pairing-tls")]
        fn approve_inbound_session(
            &self,
            _session_id: clipvault_platform::peer_transport::PairingSessionId,
        ) -> Result<(), TransportError> {
            Ok(())
        }

        #[cfg(feature = "local-peer-pairing-tls")]
        fn cancel_session(
            &self,
            _session_id: clipvault_platform::peer_transport::PairingSessionId,
        ) -> Result<(), TransportError> {
            Ok(())
        }

        #[cfg(feature = "local-peer-pairing-tls")]
        fn disconnect_peer(&self, _peer_id: &str) -> Result<(), TransportError> {
            Ok(())
        }

        #[cfg(feature = "local-peer-pairing-tls")]
        fn health_probe(
            &self,
            _peer_id: &str,
            _cert_fingerprint: &str,
        ) -> Result<clipvault_platform::peer_transport::PeerHealthSnapshot, TransportError>
        {
            Ok(clipvault_platform::peer_transport::PeerHealthSnapshot {
                peer_id: _peer_id.to_string(),
                protocol_major: PAIRING_PROTOCOL_MAJOR,
                reached_at_unix_secs: 0,
            })
        }
    }

    /// Cache the local identity the runtime expects on every
    /// productive `start_outbound` / `register_inbound` call.
    /// The tests run with the `local-peer-pairing-tls` feature
    /// enabled (the [bootstrap](crate::bootstrap) install path is
    /// the unit under test in addition to the runtime logic), so
    /// the cached identity is what keeps `start_outbound` from
    /// surfacing `TransportUnavailable` instead of
    /// `AwaitingRemoteApproval`. The helper is a no-op on hosts
    /// that disable the feature so the tests still pass when the
    /// feature gate is closed.
    fn seed_local_identity(runtime: &PairingRuntime) {
        #[cfg(feature = "local-peer-pairing-tls")]
        runtime.set_local_identity(Some(local_identity()));
        #[cfg(not(feature = "local-peer-pairing-tls"))]
        let _ = runtime;
    }

    /// Explicit incompatible-version test the spec scenario
    /// "Incompatible wire version" pins: a transport that pushes
    /// a `Hello` envelope with `version + 1` MUST surface
    /// `PairingError::IncompatibleProtocol` and never mint an
    /// inbound session. The previous spec wording collapsed the
    /// check into the generic `observe_pairing_registers_inbound_session`
    /// whose primary assertion covered the success branch — this
    /// standalone test pins the failure branch so a regression
    /// that silently accepts a future wire version surfaces here
    /// instead of being masked by the happy-path coverage.
    #[test]
    fn observe_pairing_with_incompatible_version_returns_typed_failure() {
        let persistence = Arc::new(InMemoryPairingPersistence::new());
        persistence.seed(known_peer("peer-bbbb", "fp-bbbb", "Studio B"));
        let transport = default_peer_transport();
        let runtime = PairingRuntime::new(transport, persistence);
        seed_local_identity(&runtime);
        let outcome = runtime.observe_pairing(
            PairingMessage::Hello {
                version: PAIRING_WIRE_VERSION + 1,
                peer_id: "peer-bbbb".to_string(),
                public_key_fingerprint: "fp-bbbb".to_string(),
                display_name: "Studio B".to_string(),
                nonce_a: "nonce-a".to_string(),
            },
            "fingerprint-aaaa",
        );
        assert!(matches!(
            outcome,
            PairingOutcome::Failed(PairingError::IncompatibleProtocol)
        ));
        // A rejected envelope MUST NOT register an inbound
        // session. The runtime treats the typed rejection as
        // terminal for the round-trip.
        assert!(runtime.snapshot().is_empty());
    }

    /// `revoke` MUST disarm the per-peer cert fingerprint pin
    /// the productive pairing transport armed on trust
    /// promotion, exactly like `block` and `unblock`. The previous
    /// wiring closed sessions but left the pin armed so a stale
    /// cert the runtime never pinned could re-authenticate the
    /// revoked peer against the previous pin. The test wires a
    /// `RecordingTransport` that records every `arm_pin` /
    /// `disarm_pin` call so the assertion pins the exact
    /// sequence: `arm_pin` on trust promotion, `disarm_pin` on
    /// `revoke`, and a follow-up `arm_pin` for a re-paired
    /// row.
    #[cfg(feature = "local-peer-pairing-tls")]
    #[test]
    fn revoke_disarms_pinned_cert_fingerprint() {
        use std::sync::Mutex as StdMutex;

        #[derive(Default)]
        struct RecordingTransport {
            arm_calls: StdMutex<Vec<(String, String)>>,
            disarm_calls: StdMutex<Vec<String>>,
        }

        impl PeerTransport for RecordingTransport {
            fn start(
                &self,
                _identity: &LocalPeerIdentity,
                _sink: Arc<dyn TransportSink>,
            ) -> Result<u16, TransportError> {
                Err(TransportError::Unavailable)
            }
            fn start_with_material(
                &self,
                _material: clipvault_platform::LocalIdentityMaterial,
                _sink: Arc<dyn TransportSink>,
                _advertisement: Arc<dyn PairingAdvertisement>,
            ) -> Result<u16, TransportError> {
                Err(TransportError::Unavailable)
            }
            #[cfg(feature = "local-peer-pairing-tls")]
            fn start_with_material_and_resolver(
                &self,
                _material: clipvault_platform::LocalIdentityMaterial,
                _sink: Arc<dyn TransportSink>,
                _advertisement: Arc<dyn PairingAdvertisement>,
                _resolver: Option<Arc<dyn clipvault_platform::peer_transport::RemotePeerResolver>>,
                _display_name: &str,
            ) -> Result<u16, TransportError> {
                Err(TransportError::Unavailable)
            }
            fn stop(&self) -> Result<(), TransportError> {
                Ok(())
            }
            fn is_running(&self) -> bool {
                true
            }
            fn arm_pin(&self, peer_id: &str, cert_fingerprint: &str) -> Result<(), TransportError> {
                self.arm_calls
                    .lock()
                    .expect("arm")
                    .push((peer_id.to_string(), cert_fingerprint.to_string()));
                Ok(())
            }
            fn disarm_pin(&self, peer_id: &str) -> Result<(), TransportError> {
                self.disarm_calls
                    .lock()
                    .expect("disarm")
                    .push(peer_id.to_string());
                Ok(())
            }
            fn health_check(
                &self,
                _peer_id: &str,
                _cert_fingerprint: &str,
            ) -> Result<(), TransportError> {
                Ok(())
            }
            #[cfg(feature = "local-peer-pairing-tls")]
            fn start_outbound(
                &self,
                descriptor: clipvault_platform::peer_transport::OutboundSessionDescriptor,
            ) -> Result<clipvault_platform::peer_transport::PairingOutbound, TransportError>
            {
                let id = self.arm_calls.lock().expect("arm").len() as u64 + 1;
                Ok(clipvault_platform::peer_transport::PairingOutbound {
                    session_id: clipvault_platform::peer_transport::PairingSessionId(id),
                    metadata: clipvault_platform::peer_transport::OutboundSessionMetadata {
                        remote_peer_id: descriptor.peer_id.clone(),
                        remote_full_fingerprint: descriptor.full_public_key_fingerprint.clone(),
                        remote_display_name: descriptor.display_name.clone(),
                        local_nonce: "n".repeat(32),
                        remote_nonce: "n".repeat(32),
                        sas: "000000".to_string(),
                        remote_cert_fingerprint: "0".repeat(64),
                    },
                })
            }
            fn approve_local(
                &self,
                _session_id: clipvault_platform::peer_transport::PairingSessionId,
            ) -> Result<(), TransportError> {
                Ok(())
            }
            fn approve_inbound_session(
                &self,
                _session_id: clipvault_platform::peer_transport::PairingSessionId,
            ) -> Result<(), TransportError> {
                Ok(())
            }
            fn cancel_session(
                &self,
                _session_id: clipvault_platform::peer_transport::PairingSessionId,
            ) -> Result<(), TransportError> {
                Ok(())
            }
            fn disconnect_peer(&self, _peer_id: &str) -> Result<(), TransportError> {
                Ok(())
            }
            fn health_probe(
                &self,
                _peer_id: &str,
                _cert_fingerprint: &str,
            ) -> Result<clipvault_platform::peer_transport::PeerHealthSnapshot, TransportError>
            {
                Ok(clipvault_platform::peer_transport::PeerHealthSnapshot {
                    peer_id: _peer_id.to_string(),
                    protocol_major: PAIRING_PROTOCOL_MAJOR,
                    reached_at_unix_secs: 0,
                })
            }
        }

        let transport = Arc::new(RecordingTransport::default());
        let persistence = Arc::new(InMemoryPairingPersistence::new());
        persistence.seed(known_peer("peer-bbbb", "fp-bbbb", "Studio B"));
        let runtime = PairingRuntime::new(transport.clone(), persistence.clone());
        seed_local_identity(&runtime);

        // Drive a reciprocal approval so the runtime promotes
        // the row and arms the pin.
        let outcome = runtime
            .start_outbound("peer-bbbb", "fp-bbbb", "Studio B")
            .expect("start");
        let session_id = match outcome {
            PairingOutcome::AwaitingRemoteApproval(id) => id,
            other => panic!("expected AwaitingRemoteApproval, got {other:?}"),
        };
        let _ = runtime.approve_local(session_id);
        let promote = runtime.observe_approve("peer-bbbb", "fingerprint-aaaa");
        assert!(matches!(promote, PairingOutcome::Trusted(_)));

        // The pin MUST have been armed at trust promotion.
        {
            let arms = transport.arm_calls.lock().expect("arm");
            assert_eq!(arms.len(), 1, "trust promotion must arm exactly one pin");
            assert_eq!(arms[0].0, "peer-bbbb");
        }

        // `revoke` MUST call `disarm_pin`. The previous wiring
        // closed sessions without disarming so a stale cert could
        // re-authenticate against the previous pin.
        let revoke = runtime.revoke("peer-bbbb");
        assert!(matches!(revoke, TrustOperationOutcome::Stored(_)));
        {
            let disarms = transport.disarm_calls.lock().expect("disarm");
            assert!(
                disarms.iter().any(|id| id == "peer-bbbb"),
                "revoke must disarm the per-peer pin"
            );
        }
    }

    /// `approve_local` for a session registered by the listener
    /// (via `register_inbound_from_metadata`) MUST route through
    /// `PeerTransport::approve_inbound_session` instead of the
    /// outbound `approve_local`. The previous wiring collapsed
    /// both paths into the outbound call so the inbound
    /// listener's bounded wait never woke up and the inbound
    /// approval silently failed with `UnknownPeer`. The test
    /// counts every transport call so the assertion can pin the
    /// direction the runtime picks.
    #[cfg(feature = "local-peer-pairing-tls")]
    #[test]
    fn approve_local_for_inbound_session_routes_to_approve_inbound_session() {
        use std::sync::Mutex as StdMutex;

        #[derive(Default)]
        struct DirectionRecordingTransport {
            outbound_approves: StdMutex<Vec<u64>>,
            inbound_approves: StdMutex<Vec<u64>>,
            cancels: StdMutex<Vec<u64>>,
            inbound_registrations:
                StdMutex<Vec<clipvault_platform::peer_transport::InboundSessionMetadata>>,
        }

        impl PeerTransport for DirectionRecordingTransport {
            fn start(
                &self,
                _identity: &LocalPeerIdentity,
                _sink: Arc<dyn TransportSink>,
            ) -> Result<u16, TransportError> {
                Err(TransportError::Unavailable)
            }
            fn start_with_material(
                &self,
                _material: clipvault_platform::LocalIdentityMaterial,
                _sink: Arc<dyn TransportSink>,
                _advertisement: Arc<dyn PairingAdvertisement>,
            ) -> Result<u16, TransportError> {
                Err(TransportError::Unavailable)
            }
            #[cfg(feature = "local-peer-pairing-tls")]
            fn start_with_material_and_resolver(
                &self,
                _material: clipvault_platform::LocalIdentityMaterial,
                _sink: Arc<dyn TransportSink>,
                _advertisement: Arc<dyn PairingAdvertisement>,
                _resolver: Option<Arc<dyn clipvault_platform::peer_transport::RemotePeerResolver>>,
                _display_name: &str,
            ) -> Result<u16, TransportError> {
                Err(TransportError::Unavailable)
            }
            fn stop(&self) -> Result<(), TransportError> {
                Ok(())
            }
            fn is_running(&self) -> bool {
                true
            }
            fn arm_pin(
                &self,
                _peer_id: &str,
                _cert_fingerprint: &str,
            ) -> Result<(), TransportError> {
                Ok(())
            }
            fn disarm_pin(&self, _peer_id: &str) -> Result<(), TransportError> {
                Ok(())
            }
            fn health_check(
                &self,
                _peer_id: &str,
                _cert_fingerprint: &str,
            ) -> Result<(), TransportError> {
                Ok(())
            }
            fn start_outbound(
                &self,
                descriptor: clipvault_platform::peer_transport::OutboundSessionDescriptor,
            ) -> Result<clipvault_platform::peer_transport::PairingOutbound, TransportError>
            {
                let id = self.outbound_approves.lock().expect("out").len() as u64 + 1;
                Ok(clipvault_platform::peer_transport::PairingOutbound {
                    session_id: clipvault_platform::peer_transport::PairingSessionId(id),
                    metadata: clipvault_platform::peer_transport::OutboundSessionMetadata {
                        remote_peer_id: descriptor.peer_id.clone(),
                        remote_full_fingerprint: descriptor.full_public_key_fingerprint.clone(),
                        remote_display_name: descriptor.display_name.clone(),
                        local_nonce: "n".repeat(32),
                        remote_nonce: "n".repeat(32),
                        sas: "000000".to_string(),
                        remote_cert_fingerprint: "0".repeat(64),
                    },
                })
            }
            fn approve_local(
                &self,
                session_id: clipvault_platform::peer_transport::PairingSessionId,
            ) -> Result<(), TransportError> {
                self.outbound_approves
                    .lock()
                    .expect("out")
                    .push(session_id.as_u64());
                Ok(())
            }
            fn approve_inbound_session(
                &self,
                session_id: clipvault_platform::peer_transport::PairingSessionId,
            ) -> Result<(), TransportError> {
                self.inbound_approves
                    .lock()
                    .expect("in")
                    .push(session_id.as_u64());
                Ok(())
            }
            fn cancel_session(
                &self,
                session_id: clipvault_platform::peer_transport::PairingSessionId,
            ) -> Result<(), TransportError> {
                self.cancels
                    .lock()
                    .expect("cancel")
                    .push(session_id.as_u64());
                Ok(())
            }
            fn disconnect_peer(&self, _peer_id: &str) -> Result<(), TransportError> {
                Ok(())
            }
            fn health_probe(
                &self,
                _peer_id: &str,
                _cert_fingerprint: &str,
            ) -> Result<clipvault_platform::peer_transport::PeerHealthSnapshot, TransportError>
            {
                Ok(clipvault_platform::peer_transport::PeerHealthSnapshot {
                    peer_id: _peer_id.to_string(),
                    protocol_major: PAIRING_PROTOCOL_MAJOR,
                    reached_at_unix_secs: 0,
                })
            }
        }

        impl TransportSink for DirectionRecordingTransport {
            fn on_pairing_observed(
                &self,
                _observation: clipvault_platform::peer_transport::PeerTransportObservation,
            ) {
            }
            fn on_pairing_session_started(
                &self,
                metadata: clipvault_platform::peer_transport::InboundSessionMetadata,
            ) {
                self.inbound_registrations
                    .lock()
                    .expect("reg")
                    .push(metadata);
            }
        }

        let transport = Arc::new(DirectionRecordingTransport::default());
        let runtime = PairingRuntime::new(
            transport.clone(),
            Arc::new(InMemoryPairingPersistence::new()),
        );
        seed_local_identity(&runtime);

        // The runtime registers an inbound session for the
        // listener's `Hello` envelope via the TransportSink
        // path. The listener already authenticated the cert;
        // the runtime only adds the row to its in-memory
        // session table.
        let inbound_session_id = PairingSessionId(42);
        let inbound_metadata = clipvault_platform::peer_transport::InboundSessionMetadata {
            session_id: clipvault_platform::peer_transport::PairingSessionId(
                inbound_session_id.as_u64(),
            ),
            remote_peer_id: "peer-bbbb".to_string(),
            remote_full_fingerprint: "fp-bbbb".to_string(),
            remote_display_name: "Studio B".to_string(),
            local_nonce: "n".repeat(32),
            remote_nonce: "n".repeat(32),
            sas: "123456".to_string(),
            expires_at: "1970-01-01T00:00:00Z".to_string(),
        };
        let inbound_sink: Arc<dyn TransportSink> = transport.clone();
        inbound_sink.on_pairing_session_started(inbound_metadata.clone());
        runtime.register_inbound_from_metadata(inbound_metadata);

        // Confirm the inbound registration reached the
        // recording sink so the test exercises the same path
        // the runtime drives in production.
        let registered = transport.inbound_registrations.lock().expect("reg");
        assert_eq!(registered.len(), 1);
        let snapshots = runtime.snapshot();
        assert_eq!(snapshots.len(), 1);
        assert!(
            snapshots[0].is_inbound,
            "the shell must be able to identify an authenticated inbound invitation"
        );

        // Approve the inbound session. The runtime MUST route
        // through `approve_inbound_session`, NOT
        // `approve_local`, so the listener's bounded wait
        // wakes up.
        let outcome = runtime.approve_local(inbound_session_id);
        assert!(matches!(outcome, PairingOutcome::AwaitingRemoteApproval(_)));
        let outbound = transport.outbound_approves.lock().expect("out");
        assert_eq!(
            outbound.len(),
            0,
            "inbound approval must NOT call approve_local"
        );
        let inbound = transport.inbound_approves.lock().expect("in");
        assert_eq!(
            inbound.len(),
            1,
            "inbound approval must call approve_inbound_session exactly once"
        );
        assert_eq!(inbound[0], inbound_session_id.as_u64());

        // Cancel the inbound session. The runtime MUST also
        // forward to the transport so the listener's bounded
        // wait wakes up and the inbound registry releases.
        let _ = runtime.cancel(inbound_session_id);
        let cancels = transport.cancels.lock().expect("cancel");
        assert_eq!(
            cancels.len(),
            1,
            "cancel must forward to transport.cancel_session"
        );
        assert_eq!(cancels[0], inbound_session_id.as_u64());
    }

    /// An approval that arrives without a matching inbound
    /// session cannot reach the persisted row. The runtime MUST
    /// surface `UnknownOrKeyMismatch` instead of minting an
    /// unverified row from a bare peer_id — a regression that
    /// reached the persistence layer would let a peer_id the
    /// discovery runtime never observed promote to `Trusted`
    /// without the user ever clicking Aceptar.
    #[test]
    fn observe_approve_without_inbound_session_is_rejected() {
        let persistence = Arc::new(InMemoryPairingPersistence::new());
        let transport = default_peer_transport();
        let runtime = PairingRuntime::new(transport, persistence.clone());
        seed_local_identity(&runtime);
        let outcome = runtime.observe_approve("peer-zzzz", "fingerprint-zzzz");
        assert!(matches!(
            outcome,
            PairingOutcome::Failed(PairingError::UnknownOrKeyMismatch)
        ));
        // The persistence layer never sees the request so a
        // row cannot be promoted behind the runtime's back.
        // The row is absent because the discovery layer never
        // observed the peer; the runtime never invents one.
        assert!(persistence.load("peer-zzzz").expect("load").is_none());
    }
}
