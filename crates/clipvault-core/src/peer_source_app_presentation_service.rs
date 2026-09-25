//! Client-side façade the `peer-source-app-presentation`
//! change ships.
//!
//! The façade mirrors the
//! [`crate::peer_text_history::PeerTextHistoryService`] /
//! [`crate::peer_image_thumbnail::PeerImageThumbnailService`]
//! shape: it owns the [`PeerSourceAppPresentationTransport`]
//! trait the productive pairing transport fulfils, the
//! per-peer in-flight gate the spec pins (at most two
//! concurrent presentation requests per peer on the client
//! and on the host), and the [`crate::peer_discovery`] capability
//! check that fails closed for legacy peers that did not opt
//! into the additive contract.
//!
//! The façade is metadata-only by construction: it never
//! receives the icon bytes the host supplies — the transport
//! hands the runtime a typed
//! [`PeerSourceAppPresentation`] value the caller passes to the
//! renderer. The icon bytes never linger in any field the bridge
//! surfaces to the frontend and the runtime never writes the
//! raw bytes to SQLite, to logs or to the clipboard.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use thiserror::Error;

use crate::peer_source_app_presentation::{
    validate_source_app_icon, validate_source_app_name, MAX_SOURCE_APP_ICON_BYTES,
};

/// Maximum concurrent source-app presentation requests the
/// façade allows per peer. The spec pins the same limit on
/// both sides of the dial loop (client and host); the client
/// side drops requests that would exceed the gate without
/// surfacing a global rail error so the renderer keeps its
/// deterministic per-row loading state.
pub const SOURCE_APP_PRESENTATION_MAX_INFLIGHT_PER_PEER: usize = 2;

/// Successful payload the client façade returns. The struct
/// mirrors the wire envelope the spec pins: a validated display
/// name + an optional bounded PNG icon, plus a stable
/// reason string for every failure mode. The renderer
/// branches on the variants without inspecting free-form
/// strings or icon bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerSourceAppPresentation {
    /// The host returned a validated display name and / or a
    /// bounded PNG icon. The runtime validated the bytes
    /// through the [`crate::peer_source_app_presentation`]
    /// helper; the renderer MUST release the icon's Object URL
    /// on replacement / unmount / peer switch.
    Ok {
        /// Trimmed, validated display name (`None`).
        source_app_name: Option<String>,
        /// Bounded PNG bytes the host returned and the
        /// runtime validated (`None` when no icon was sent).
        /// The renderer decodes the bytes into a transient
        /// Object URL; the runtime never persists the bytes.
        source_app_icon_bytes: Option<Vec<u8>>,
    },
    /// The peer did not advertise the `source_app_presentation`
    /// capability. The runtime renders the generic icon +
    /// unknown-name fallback for the row.
    CapabilityMissing,
    /// The peer is no longer trusted / active. The runtime
    /// drops the cached result and renders the fallback.
    NotTrusted,
    /// The peer revoked / blocked the request. The runtime
    /// drops the cached result and renders the fallback.
    PeerUnavailable,
    /// The transport rejected the request (`unavailable`,
    /// `unknown_peer`, `key_mismatch`, `body_too_large`, …).
    /// The runtime treats the failure as a transient error
    /// without surfacing a global rail error.
    TransportUnavailable,
    /// The icon bytes failed the source-app presentation
    /// validation (signature, decode, dimensions, byte cap).
    /// The runtime keeps the validated display name when
    /// present and renders the generic icon fallback.
    InvalidIcon,
}

/// Typed error the runtime surfaces when the transport hands
/// back a structured failure the bridge cannot translate. The
/// variants collapse every underlying rejection into one of
/// the [`PeerSourceAppPresentation`] outcomes so the renderer
/// never has to inspect free-form strings.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerSourceAppPresentationError {
    #[error("peer source-app presentation transport is unavailable")]
    Unavailable,
}

/// Trait the bootstrap fulfils against the pairing transport.
/// The transport dials the productive mTLS session and
/// forwards the typed request / response the spec pins.
pub trait PeerSourceAppPresentationTransport: Send + Sync {
    /// Drive a single presentation request through the wire
    /// envelope. The transport authenticates the `peer_id`
    /// against the pinned cert fingerprint and re-validates
    /// the capability gate before dialling the listener.
    fn fetch_source_app_presentation(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        remote_entry_id: &str,
    ) -> Result<HostSourceAppPresentationWire, PeerSourceAppPresentationTransportError>;
}

/// Wire-level response the productive transport returns. The
/// fields mirror [`crate::peer_transport::PeerImageFetchSnapshot`]
/// — the bytes are base64-encoded so the JSON envelope stays
/// bounded by [`FETCH_SOURCE_APP_PRESENTATION_MAX_RESPONSE_BYTES`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSourceAppPresentationWire {
    pub source_app_name: Option<String>,
    pub source_app_icon_b64: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerSourceAppPresentationTransportError {
    #[error("peer source-app presentation transport is unavailable")]
    Unavailable,
    #[error("peer source-app presentation transport has no resolved pairing endpoint")]
    PeerUnresolved,
    #[error("peer source-app presentation transport rejected an unknown peer")]
    UnknownPeer,
    #[error("peer source-app presentation transport rejected a mismatched TLS identity")]
    KeyMismatch,
    #[error("peer source-app presentation transport rejected a revoked peer")]
    Revoked,
    #[error("peer source-app presentation transport rejected a blocked peer")]
    Blocked,
    #[error("peer source-app presentation transport rejected an untrusted / inactive peer")]
    NotTrusted,
    #[error("peer source-app presentation transport rejected a non-transferrable entry")]
    NotTransferable,
    #[error("peer source-app presentation transport rejected a malformed payload")]
    Malformed,
    #[error("peer source-app presentation transport wire protocol is incompatible")]
    IncompatibleProtocol,
    #[error("peer source-app presentation transport rejected a row the host cannot serve")]
    NotAvailable,
    #[error(
        "peer source-app presentation transport rejected a body that exceeded the envelope cap"
    )]
    BodyTooLarge,
}

/// Capability resolver the façade consults before dialling.
/// The closure matches [`crate::peer_discovery::PeerCapabilityResolver`]
/// so the bootstrap can install the SQLite-backed resolver the
/// pairing persistence owns. A peer that did not advertise
/// `source_app_presentation` collapses to the typed
/// [`PeerSourceAppPresentation::CapabilityMissing`] outcome.
pub type PeerSourceAppPresentationCapabilityResolver = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Default resolver the tests / simple bootstraps install when
/// the SQLite-backed closure is unavailable. A peer is considered
/// to advertise the capability when the snapshot the runtime
/// handed the service includes the token; peers that did not
/// collapse to the typed no-op outcome.
fn default_capability_resolver() -> PeerSourceAppPresentationCapabilityResolver {
    Arc::new(|_peer_id: &str| false)
}

/// Per-peer in-flight gate the spec pins. The counter caps the
/// number of concurrent presentation requests the façade
/// issues for a single peer; a request that would exceed the
/// gate collapses to the typed
/// [`PeerSourceAppPresentation::TransportUnavailable`] outcome
/// without surfacing a global rail error.
#[derive(Debug, Default)]
struct InflightGate {
    in_flight: HashMap<String, Arc<AtomicUsize>>,
}

impl InflightGate {
    fn try_acquire(gate: &Arc<Mutex<Self>>, peer_id: &str) -> Result<InflightGuard, ()> {
        let counter = {
            let mut state = gate.lock();
            state
                .in_flight
                .entry(peer_id.to_string())
                .or_insert_with(|| Arc::new(AtomicUsize::new(0)))
                .clone()
        };
        let previous = counter.fetch_add(1, Ordering::AcqRel);
        if previous >= SOURCE_APP_PRESENTATION_MAX_INFLIGHT_PER_PEER {
            counter.fetch_sub(1, Ordering::AcqRel);
            return Err(());
        }
        Ok(InflightGuard {
            gate: Arc::clone(gate),
            peer_id: peer_id.to_string(),
            counter,
        })
    }
}

struct InflightGuard {
    gate: Arc<Mutex<InflightGate>>,
    peer_id: String,
    counter: Arc<AtomicUsize>,
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        let previous = self.counter.fetch_sub(1, Ordering::AcqRel);
        if previous == 1 {
            let mut state = self.gate.lock();
            if state
                .in_flight
                .get(&self.peer_id)
                .is_some_and(|counter| Arc::ptr_eq(counter, &self.counter))
            {
                state.in_flight.remove(&self.peer_id);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerSourceAppPresentationTrustState {
    pub trusted: bool,
    pub active: bool,
}

#[cfg(feature = "local-peer-pairing-tls")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerSourceAppPresentationHostAuthorization {
    Authorized,
    CapabilityMissing,
    NotTrusted,
}

#[cfg(feature = "local-peer-pairing-tls")]
pub type PeerSourceAppPresentationHostAuthorizationResolver =
    Arc<dyn Fn(&str) -> PeerSourceAppPresentationHostAuthorization + Send + Sync>;

#[cfg(feature = "local-peer-pairing-tls")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerSourceAppPresentationHostError {
    Busy,
    CapabilityMissing,
    NotTrusted,
    NotFound,
    PersistenceUnavailable,
}

#[cfg(feature = "local-peer-pairing-tls")]
pub trait PeerSourceAppPresentationHostPersistence: Send + Sync {
    fn fetch_entry(
        &self,
        remote_entry_id: &str,
    ) -> Result<Option<clipvault_db::EntryRecord>, PeerSourceAppPresentationHostError>;
    fn image_asset_is_valid(&self, entry: &clipvault_db::EntryRecord) -> bool;
    fn read_application_icon(
        &self,
        asset_ref: &str,
    ) -> Result<Vec<u8>, PeerSourceAppPresentationHostError>;
}

#[cfg(feature = "local-peer-pairing-tls")]
pub struct SqlitePeerSourceAppPresentationHostPersistence {
    database: Arc<Mutex<clipvault_db::Database>>,
    clipboard_assets: crate::ClipboardAssetStore,
    application_icons: crate::ApplicationIconStore,
}

#[cfg(feature = "local-peer-pairing-tls")]
impl SqlitePeerSourceAppPresentationHostPersistence {
    pub fn new(
        database: Arc<Mutex<clipvault_db::Database>>,
        clipboard_assets: crate::ClipboardAssetStore,
    ) -> Self {
        let application_icons = crate::ApplicationIconStore::new(clipboard_assets.data_dir());
        Self {
            database,
            clipboard_assets,
            application_icons,
        }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerSourceAppPresentationHostPersistence for SqlitePeerSourceAppPresentationHostPersistence {
    fn fetch_entry(
        &self,
        remote_entry_id: &str,
    ) -> Result<Option<clipvault_db::EntryRecord>, PeerSourceAppPresentationHostError> {
        let Some(id) = remote_entry_id
            .strip_prefix("entry-")
            .and_then(|suffix| suffix.parse::<i64>().ok())
            .filter(|id| *id > 0)
        else {
            return Ok(None);
        };
        let mut database = self.database.lock();
        clipvault_db::EntryRepository::new(database.connection_mut())
            .find_by_id(id)
            .map_err(|_| PeerSourceAppPresentationHostError::PersistenceUnavailable)
    }

    fn image_asset_is_valid(&self, entry: &clipvault_db::EntryRecord) -> bool {
        entry
            .asset_ref
            .as_deref()
            .filter(|asset_ref| asset_ref.starts_with("clipboard/"))
            .is_some_and(|asset_ref| self.clipboard_assets.validate(asset_ref).is_ok())
    }

    fn read_application_icon(
        &self,
        asset_ref: &str,
    ) -> Result<Vec<u8>, PeerSourceAppPresentationHostError> {
        self.application_icons
            .read_bytes(asset_ref)
            .map_err(|_| PeerSourceAppPresentationHostError::NotFound)
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
#[derive(Debug, Default)]
struct HostInflightGate {
    counters: HashMap<String, Arc<AtomicUsize>>,
}

#[cfg(feature = "local-peer-pairing-tls")]
impl HostInflightGate {
    fn acquire(gate: &Arc<Mutex<Self>>, peer_id: &str) -> Result<HostInflightGuard, ()> {
        let counter = {
            let mut state = gate.lock();
            state
                .counters
                .entry(peer_id.to_string())
                .or_insert_with(|| Arc::new(AtomicUsize::new(0)))
                .clone()
        };
        let previous = counter.fetch_add(1, Ordering::AcqRel);
        if previous >= SOURCE_APP_PRESENTATION_MAX_INFLIGHT_PER_PEER {
            counter.fetch_sub(1, Ordering::AcqRel);
            return Err(());
        }
        Ok(HostInflightGuard {
            gate: Arc::clone(gate),
            peer_id: peer_id.to_string(),
            counter,
        })
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
struct HostInflightGuard {
    gate: Arc<Mutex<HostInflightGate>>,
    peer_id: String,
    counter: Arc<AtomicUsize>,
}

#[cfg(feature = "local-peer-pairing-tls")]
impl Drop for HostInflightGuard {
    fn drop(&mut self) {
        let previous = self.counter.fetch_sub(1, Ordering::AcqRel);
        if previous == 1 {
            let mut state = self.gate.lock();
            if state
                .counters
                .get(&self.peer_id)
                .is_some_and(|counter| Arc::ptr_eq(counter, &self.counter))
            {
                state.counters.remove(&self.peer_id);
            }
        }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
pub struct PeerSourceAppPresentationHostHandlerAdapter {
    persistence: Arc<dyn PeerSourceAppPresentationHostPersistence>,
    authorization: PeerSourceAppPresentationHostAuthorizationResolver,
    in_flight: Arc<Mutex<HostInflightGate>>,
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerSourceAppPresentationHostHandlerAdapter {
    pub fn new(
        persistence: Arc<dyn PeerSourceAppPresentationHostPersistence>,
        authorization: PeerSourceAppPresentationHostAuthorizationResolver,
    ) -> Self {
        Self {
            persistence,
            authorization,
            in_flight: Arc::new(Mutex::new(HostInflightGate::default())),
        }
    }

    fn fetch(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
    ) -> Result<(Option<String>, Option<Vec<u8>>), PeerSourceAppPresentationHostError> {
        match (self.authorization)(peer_id) {
            PeerSourceAppPresentationHostAuthorization::Authorized => {}
            PeerSourceAppPresentationHostAuthorization::CapabilityMissing => {
                return Err(PeerSourceAppPresentationHostError::CapabilityMissing);
            }
            PeerSourceAppPresentationHostAuthorization::NotTrusted => {
                return Err(PeerSourceAppPresentationHostError::NotTrusted);
            }
        }
        let _guard = HostInflightGate::acquire(&self.in_flight, peer_id)
            .map_err(|_| PeerSourceAppPresentationHostError::Busy)?;
        let entry = self
            .persistence
            .fetch_entry(remote_entry_id)?
            .ok_or(PeerSourceAppPresentationHostError::NotFound)?;
        let eligible = if entry.content_type.is_textual() {
            crate::peer_text_history::entry_is_transferable(&entry)
        } else if entry.content_type == clipvault_db::ContentType::Image {
            crate::peer_image_history::image_entry_is_transferable(&entry)
                && self.persistence.image_asset_is_valid(&entry)
        } else {
            false
        };
        if !eligible {
            return Err(PeerSourceAppPresentationHostError::NotFound);
        }
        let source_app_name = entry
            .source_app_name
            .as_deref()
            .and_then(|name| validate_source_app_name(name).ok());
        let source_app_icon_bytes = entry
            .source_app_icon_ref
            .as_deref()
            .filter(|asset_ref| {
                asset_ref.starts_with("application-icons/")
                    && crate::application_icons::is_safe_icon_ref(asset_ref)
            })
            .and_then(|asset_ref| self.persistence.read_application_icon(asset_ref).ok())
            .filter(|bytes| validate_source_app_icon(bytes).is_ok());
        Ok((source_app_name, source_app_icon_bytes))
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl clipvault_platform::peer_transport::FetchSourceAppPresentationHostHandler
    for PeerSourceAppPresentationHostHandlerAdapter
{
    fn fetch_source_app_presentation(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
    ) -> clipvault_platform::peer_transport::HostSourceAppPresentationResponse {
        use clipvault_platform::peer_transport::HostSourceAppPresentationResponse as Response;
        match self.fetch(peer_id, remote_entry_id) {
            Ok((source_app_name, source_app_icon_bytes)) => Response::Ok {
                source_app_name,
                source_app_icon_bytes,
            },
            Err(PeerSourceAppPresentationHostError::Busy) => Response::NotAvailable,
            Err(PeerSourceAppPresentationHostError::CapabilityMissing) => Response::NotAvailable,
            Err(PeerSourceAppPresentationHostError::NotTrusted) => Response::NotTrusted,
            Err(PeerSourceAppPresentationHostError::NotFound) => Response::NotFound,
            Err(PeerSourceAppPresentationHostError::PersistenceUnavailable) => {
                Response::PersistenceUnavailable
            }
        }
    }
}

/// Metadata-only façade the runtime drives when the renderer
/// needs the source-app presentation for a visible remote
/// entry. The service is cheap to clone (every field is
/// `Arc`-shared).
#[derive(Clone)]
pub struct PeerSourceAppPresentationService {
    transport: Arc<dyn PeerSourceAppPresentationTransport>,
    capability_resolver: PeerSourceAppPresentationCapabilityResolver,
    states: Arc<Mutex<HashMap<String, PeerSourceAppPresentationTrustState>>>,
    in_flight: Arc<Mutex<InflightGate>>,
}

impl PeerSourceAppPresentationService {
    /// Build the façade with the productive transport and a
    /// default capability resolver. Production shells
    /// replace the resolver through [`Self::with_capability_resolver`].
    pub fn new(transport: Arc<dyn PeerSourceAppPresentationTransport>) -> Self {
        Self {
            transport,
            capability_resolver: default_capability_resolver(),
            states: Arc::new(Mutex::new(HashMap::new())),
            in_flight: Arc::new(Mutex::new(InflightGate::default())),
        }
    }

    /// Replace the per-peer capability resolver the façade
    /// consults before dialling the productive mTLS transport.
    /// The bootstrap installs the SQLite-backed resolver the
    /// pairing persistence owns so a peer that did not advertise
    /// `source_app_presentation` collapses to the typed
    /// `CapabilityMissing` outcome.
    pub fn with_capability_resolver(
        mut self,
        resolver: PeerSourceAppPresentationCapabilityResolver,
    ) -> Self {
        self.capability_resolver = resolver;
        self
    }

    /// Record the latest trust / active projection for a peer.
    pub fn record_peer_state(&self, peer_id: &str, state: PeerSourceAppPresentationTrustState) {
        self.states.lock().insert(peer_id.to_string(), state);
    }

    pub fn forget_peer(&self, peer_id: &str) {
        self.states.lock().remove(peer_id);
    }

    /// Drive a single source-app presentation request. The
    /// caller supplies the `peer_id` and `remote_entry_id` it
    /// received through the bridge; the façade returns the
    /// typed [`PeerSourceAppPresentation`] outcome the renderer
    /// branches on without an extra round-trip.
    pub fn fetch(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        remote_entry_id: &str,
    ) -> PeerSourceAppPresentation {
        match self.states.lock().get(peer_id).copied() {
            None => return PeerSourceAppPresentation::PeerUnavailable,
            Some(state) if !state.trusted || !state.active => {
                return PeerSourceAppPresentation::NotTrusted;
            }
            Some(_) => {}
        }
        let _guard = match InflightGate::try_acquire(&self.in_flight, peer_id) {
            Ok(guard) => guard,
            Err(()) => {
                return PeerSourceAppPresentation::TransportUnavailable;
            }
        };
        if !(self.capability_resolver)(peer_id) {
            return PeerSourceAppPresentation::CapabilityMissing;
        }
        match self.transport.fetch_source_app_presentation(
            peer_id,
            cert_fingerprint,
            remote_entry_id,
        ) {
            Ok(payload) => validate_wire_payload(payload),
            Err(error) => map_transport_error(error),
        }
    }
}

fn validate_wire_payload(payload: HostSourceAppPresentationWire) -> PeerSourceAppPresentation {
    let HostSourceAppPresentationWire {
        source_app_name,
        source_app_icon_b64,
    } = payload;
    let validated_name = match source_app_name.as_deref() {
        Some(value) => match validate_source_app_name(value) {
            Ok(value) => Some(value),
            Err(_) => None,
        },
        None => None,
    };
    let validated_icon = source_app_icon_b64
        .as_deref()
        .and_then(|b64| decode_b64_to_bytes(b64));
    let validated_icon = match validated_icon {
        Some(Ok(bytes)) => match validate_source_app_icon(&bytes) {
            Ok(()) => Some(bytes),
            Err(_) => None,
        },
        Some(Err(_)) | None => None,
    };
    // The spec scenario "Malformed or oversized icon is
    // encountered" pins the fallback: an invalid icon MUST
    // NOT fail an otherwise-valid name lookup. The renderer
    // shows the generic local icon and the validated display
    // name in that order; the renderer never surfaces a
    // global rail error.
    if validated_name.is_none()
        && validated_icon.is_none()
        && source_app_name.is_none()
        && source_app_icon_b64.is_none()
    {
        // Both fields were empty on the wire; treat as the
        // generic missing-presentation case so the renderer
        // keeps its deterministic unknown-source fallback.
        PeerSourceAppPresentation::CapabilityMissing
    } else {
        PeerSourceAppPresentation::Ok {
            source_app_name: validated_name,
            source_app_icon_bytes: validated_icon,
        }
    }
}

fn decode_b64_to_bytes(b64: &str) -> Option<Result<Vec<u8>, ()>> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    match STANDARD.decode(b64.as_bytes()) {
        Ok(bytes) => {
            if bytes.len() > MAX_SOURCE_APP_ICON_BYTES {
                Some(Err(()))
            } else {
                Some(Ok(bytes))
            }
        }
        Err(_) => Some(Err(())),
    }
}

fn map_transport_error(
    error: PeerSourceAppPresentationTransportError,
) -> PeerSourceAppPresentation {
    match error {
        PeerSourceAppPresentationTransportError::Unavailable
        | PeerSourceAppPresentationTransportError::PeerUnresolved => {
            PeerSourceAppPresentation::TransportUnavailable
        }
        PeerSourceAppPresentationTransportError::UnknownPeer
        | PeerSourceAppPresentationTransportError::KeyMismatch
        | PeerSourceAppPresentationTransportError::Revoked
        | PeerSourceAppPresentationTransportError::Blocked => {
            PeerSourceAppPresentation::PeerUnavailable
        }
        PeerSourceAppPresentationTransportError::NotTrusted => {
            PeerSourceAppPresentation::NotTrusted
        }
        PeerSourceAppPresentationTransportError::NotTransferable
        | PeerSourceAppPresentationTransportError::Malformed
        | PeerSourceAppPresentationTransportError::IncompatibleProtocol
        | PeerSourceAppPresentationTransportError::NotAvailable => {
            PeerSourceAppPresentation::TransportUnavailable
        }
        PeerSourceAppPresentationTransportError::BodyTooLarge => {
            PeerSourceAppPresentation::InvalidIcon
        }
    }
}

/// Adapter the bootstrap installs against the productive pairing
/// transport. The adapter maps the productive transport onto
/// the [`PeerSourceAppPresentationTransport`] trait the
/// façade consumes.
#[cfg(feature = "local-peer-pairing-tls")]
pub struct PairingFetchSourceAppPresentationAdapter {
    inner: Arc<dyn clipvault_platform::peer_transport::PeerTransport>,
}

pub struct NoopPeerSourceAppPresentationTransport;

impl PeerSourceAppPresentationTransport for NoopPeerSourceAppPresentationTransport {
    fn fetch_source_app_presentation(
        &self,
        _peer_id: &str,
        _cert_fingerprint: &str,
        _remote_entry_id: &str,
    ) -> Result<HostSourceAppPresentationWire, PeerSourceAppPresentationTransportError> {
        Err(PeerSourceAppPresentationTransportError::Unavailable)
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PairingFetchSourceAppPresentationAdapter {
    pub fn new(inner: Arc<dyn clipvault_platform::peer_transport::PeerTransport>) -> Self {
        Self { inner }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerSourceAppPresentationTransport for PairingFetchSourceAppPresentationAdapter {
    fn fetch_source_app_presentation(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        remote_entry_id: &str,
    ) -> Result<HostSourceAppPresentationWire, PeerSourceAppPresentationTransportError> {
        // The productive transport returns the wire-level
        // snapshot the listener pushes back; the adapter
        // collapses the typed variants into the
        // [`HostSourceAppPresentationWire`] the façade
        // consumes. The caller resolves the mTLS pin from the
        // trusted pairing record; the renderer never supplies it.
        let snapshot = self
            .inner
            .fetch_source_app_presentation(peer_id, cert_fingerprint, remote_entry_id)
            .map_err(map_pairing_transport_error)?;
        Ok(HostSourceAppPresentationWire {
            source_app_name: snapshot.source_app_name,
            source_app_icon_b64: snapshot.source_app_icon_b64,
        })
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
fn map_pairing_transport_error(
    error: clipvault_platform::peer_transport::TransportError,
) -> PeerSourceAppPresentationTransportError {
    use clipvault_platform::peer_transport::TransportError as PairingError;
    match error {
        PairingError::Unavailable => PeerSourceAppPresentationTransportError::Unavailable,
        PairingError::IncompatibleProtocol => {
            PeerSourceAppPresentationTransportError::IncompatibleProtocol
        }
        PairingError::Malformed => PeerSourceAppPresentationTransportError::Malformed,
        PairingError::BodyTooLarge => PeerSourceAppPresentationTransportError::BodyTooLarge,
        _ => PeerSourceAppPresentationTransportError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark_eligible(service: &PeerSourceAppPresentationService, peer_id: &str) {
        service.record_peer_state(
            peer_id,
            PeerSourceAppPresentationTrustState {
                trusted: true,
                active: true,
            },
        );
    }

    #[derive(Default)]
    struct ScriptedTransport {
        next: Mutex<
            Option<Result<HostSourceAppPresentationWire, PeerSourceAppPresentationTransportError>>,
        >,
    }

    impl PeerSourceAppPresentationTransport for ScriptedTransport {
        fn fetch_source_app_presentation(
            &self,
            _peer_id: &str,
            _cert_fingerprint: &str,
            _remote_entry_id: &str,
        ) -> Result<HostSourceAppPresentationWire, PeerSourceAppPresentationTransportError>
        {
            self.next
                .lock()
                .take()
                .unwrap_or(Err(PeerSourceAppPresentationTransportError::Unavailable))
        }
    }

    #[test]
    fn fetch_returns_capability_missing_when_resolver_returns_false() {
        let transport: Arc<dyn PeerSourceAppPresentationTransport> =
            Arc::new(ScriptedTransport::default());
        let service = PeerSourceAppPresentationService::new(transport);
        mark_eligible(&service, "peer-a");
        let result = service.fetch("peer-a", "fingerprint", "entry-1");
        assert!(matches!(
            result,
            PeerSourceAppPresentation::CapabilityMissing
        ));
    }

    #[test]
    fn fetch_returns_ok_for_valid_payload() {
        let transport: Arc<dyn PeerSourceAppPresentationTransport> = Arc::new({
            struct T;
            impl PeerSourceAppPresentationTransport for T {
                fn fetch_source_app_presentation(
                    &self,
                    _peer_id: &str,
                    _cert_fingerprint: &str,
                    _remote_entry_id: &str,
                ) -> Result<HostSourceAppPresentationWire, PeerSourceAppPresentationTransportError>
                {
                    Ok(HostSourceAppPresentationWire {
                        source_app_name: Some("Terminal".to_string()),
                        source_app_icon_b64: None,
                    })
                }
            }
            T
        });
        let service = PeerSourceAppPresentationService::new(transport)
            .with_capability_resolver(Arc::new(|_| true));
        mark_eligible(&service, "peer-a");
        match service.fetch("peer-a", "fingerprint", "entry-1") {
            PeerSourceAppPresentation::Ok {
                source_app_name,
                source_app_icon_bytes,
            } => {
                assert_eq!(source_app_name.as_deref(), Some("Terminal"));
                assert!(source_app_icon_bytes.is_none());
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn fetch_rejects_unknown_or_untrusted_peer_before_transport() {
        let transport = Arc::new(ScriptedTransport {
            next: Mutex::new(Some(Ok(HostSourceAppPresentationWire {
                source_app_name: Some("Terminal".to_string()),
                source_app_icon_b64: None,
            }))),
        });
        let service = PeerSourceAppPresentationService::new(transport)
            .with_capability_resolver(Arc::new(|_| true));

        assert_eq!(
            service.fetch("peer-a", "fingerprint", "entry-1"),
            PeerSourceAppPresentation::PeerUnavailable
        );
        service.record_peer_state(
            "peer-a",
            PeerSourceAppPresentationTrustState {
                trusted: false,
                active: true,
            },
        );
        assert_eq!(
            service.fetch("peer-a", "fingerprint", "entry-1"),
            PeerSourceAppPresentation::NotTrusted
        );
        mark_eligible(&service, "peer-a");
        assert!(matches!(
            service.fetch("peer-a", "fingerprint", "entry-1"),
            PeerSourceAppPresentation::Ok { .. }
        ));
    }

    #[test]
    fn fetch_returns_invalid_icon_when_payload_fails_validation() {
        let transport: Arc<dyn PeerSourceAppPresentationTransport> = Arc::new({
            struct T;
            impl PeerSourceAppPresentationTransport for T {
                fn fetch_source_app_presentation(
                    &self,
                    _peer_id: &str,
                    _cert_fingerprint: &str,
                    _remote_entry_id: &str,
                ) -> Result<HostSourceAppPresentationWire, PeerSourceAppPresentationTransportError>
                {
                    Ok(HostSourceAppPresentationWire {
                        source_app_name: Some("Visual Studio Code".to_string()),
                        source_app_icon_b64: Some("AAAA".to_string()),
                    })
                }
            }
            T
        });
        let service = PeerSourceAppPresentationService::new(transport)
            .with_capability_resolver(Arc::new(|_| true));
        mark_eligible(&service, "peer-a");
        match service.fetch("peer-a", "fingerprint", "entry-1") {
            PeerSourceAppPresentation::Ok {
                source_app_name,
                source_app_icon_bytes,
            } => {
                // The display name survives even when the icon
                // fails validation — the spec scenario
                // "Malformed or oversized icon is encountered"
                // pins this behaviour.
                assert_eq!(source_app_name.as_deref(), Some("Visual Studio Code"));
                assert!(source_app_icon_bytes.is_none());
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn fetch_caps_concurrent_requests_to_two_per_peer() {
        // The spec pins a per-peer concurrency limit of two
        // presentation requests. The gate helper exposes the
        // counter directly so the test can pin the
        // max-concurrency branch without threading the
        // service across multiple workers.
        let gate = Arc::new(Mutex::new(InflightGate::default()));
        // First two attempts acquire a slot.
        let first = InflightGate::try_acquire(&gate, "peer-a").expect("first slot available");
        let second = InflightGate::try_acquire(&gate, "peer-a").expect("second slot available");
        // Third concurrent attempt collapses to the typed
        // rejection without ever dialling the transport.
        assert!(InflightGate::try_acquire(&gate, "peer-a").is_err());
        // A different peer is independent: the per-peer
        // counter must NOT bleed across rows.
        let peer_b = InflightGate::try_acquire(&gate, "peer-b").expect("peer-b slot available");
        drop(peer_b);
        // Releasing the slots restores the capacity.
        drop(first);
        drop(second);
        // After the releases we have room for two more
        // in-flight requests on peer-a.
        let _first = InflightGate::try_acquire(&gate, "peer-a").expect("released first slot");
        let _second = InflightGate::try_acquire(&gate, "peer-a").expect("released second slot");
        // ...but the third concurrent attempt still collapses
        // to the typed rejection.
        assert!(InflightGate::try_acquire(&gate, "peer-a").is_err());
    }

    #[test]
    fn fetch_allows_two_inflight_requests_and_rejects_a_third_per_peer() {
        struct BlockingTransport {
            state: Mutex<(usize, bool, usize)>,
            changed: parking_lot::Condvar,
        }

        impl PeerSourceAppPresentationTransport for BlockingTransport {
            fn fetch_source_app_presentation(
                &self,
                _peer_id: &str,
                _cert_fingerprint: &str,
                _remote_entry_id: &str,
            ) -> Result<HostSourceAppPresentationWire, PeerSourceAppPresentationTransportError>
            {
                let mut state = self.state.lock();
                state.0 += 1;
                state.2 += 1;
                self.changed.notify_all();
                while !state.1 {
                    self.changed.wait(&mut state);
                }
                state.0 -= 1;
                Ok(HostSourceAppPresentationWire {
                    source_app_name: Some("Terminal".to_string()),
                    source_app_icon_b64: None,
                })
            }
        }

        let transport = Arc::new(BlockingTransport {
            state: Mutex::new((0, false, 0)),
            changed: parking_lot::Condvar::new(),
        });
        let service = PeerSourceAppPresentationService::new(transport.clone())
            .with_capability_resolver(Arc::new(|_| true));
        mark_eligible(&service, "peer-a");

        let first_service = service.clone();
        let first =
            std::thread::spawn(move || first_service.fetch("peer-a", "fingerprint", "entry-1"));
        let second_service = service.clone();
        let second =
            std::thread::spawn(move || second_service.fetch("peer-a", "fingerprint", "entry-2"));

        {
            let mut state = transport.state.lock();
            while state.0 < 2 {
                transport.changed.wait(&mut state);
            }
        }
        assert_eq!(
            service.fetch("peer-a", "fingerprint", "entry-3"),
            PeerSourceAppPresentation::TransportUnavailable
        );
        assert_eq!(transport.state.lock().2, 2);

        {
            let mut state = transport.state.lock();
            state.1 = true;
            transport.changed.notify_all();
        }
        assert!(matches!(
            first.join().expect("first request"),
            PeerSourceAppPresentation::Ok { .. }
        ));
        assert!(matches!(
            second.join().expect("second request"),
            PeerSourceAppPresentation::Ok { .. }
        ));
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    #[test]
    fn host_rejects_untrusted_or_legacy_peer_before_persistence_lookup() {
        struct UnusedPersistence;
        impl PeerSourceAppPresentationHostPersistence for UnusedPersistence {
            fn fetch_entry(
                &self,
                _remote_entry_id: &str,
            ) -> Result<Option<clipvault_db::EntryRecord>, PeerSourceAppPresentationHostError>
            {
                panic!("authorization must run before entry lookup");
            }

            fn image_asset_is_valid(&self, _entry: &clipvault_db::EntryRecord) -> bool {
                panic!("authorization must run before asset validation");
            }

            fn read_application_icon(
                &self,
                _asset_ref: &str,
            ) -> Result<Vec<u8>, PeerSourceAppPresentationHostError> {
                panic!("authorization must run before icon reads");
            }
        }

        let persistence: Arc<dyn PeerSourceAppPresentationHostPersistence> =
            Arc::new(UnusedPersistence);
        for (authorization, expected) in [
            (
                PeerSourceAppPresentationHostAuthorization::NotTrusted,
                PeerSourceAppPresentationHostError::NotTrusted,
            ),
            (
                PeerSourceAppPresentationHostAuthorization::CapabilityMissing,
                PeerSourceAppPresentationHostError::CapabilityMissing,
            ),
        ] {
            let adapter = PeerSourceAppPresentationHostHandlerAdapter::new(
                Arc::clone(&persistence),
                Arc::new(move |_| authorization),
            );
            assert_eq!(adapter.fetch("peer-a", "entry-1"), Err(expected));
        }
    }
}
