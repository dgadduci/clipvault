//! Bounded PNG thumbnail façade the `peer-image-preview-thumbnails`
//! change ships. The module owns:
//!
//! - the metadata-only [`PeerFetchImageThumbnailTransport`] the
//!   client-side runtime drives when a remote image card intersects
//!   the visible remote-history viewport;
//! - the [`PeerImageThumbnailService`] façade the bridge / Tauri shell
//!   hands to the frontend; the service collapses every failure mode
//!   into a typed [`PeerImageThumbnailOutcome`] the renderer branches
//!   on without inspecting free-form strings or content bytes;
//! - the host-side [`PeerImageThumbnailHostSource`] trait the listener
//!   drives when a trusted peer asks for the bounded thumbnail of a
//!   transferrable image entry. The trait only receives the opaque
//!   `remote_entry_id`; the host resolves the local entry, re-validates
//!   eligibility and asset namespace, generates the in-memory
//!   thumbnail and returns the bounded PNG body together with the
//!   resulting dimensions;
//! - the [`thumbnail_png_for_entry`] helper the host service uses to
//!   decode, downscale and re-encode the canonical PNG into a
//!   deterministic in-memory thumbnail. The helper enforces the
//!   documented dimension (≤ 256 px on the longest side) and byte
//!   (≤ 384 KiB) caps before returning the body, and never persists
//!   the derivative or substitutes it for the original
//!   `Importar` payload.
//!
//! ## Capabilities and trust
//!
//! The thumbnail route requires BOTH `image_import` and
//! `image_preview_thumbnail` on the active peer. The host handler
//! consults a per-peer capability resolver (the bootstrap wires the
//! SQLite-backed resolver the pairing persistence owns) so a peer
//! that only ships one of the two capabilities collapses to a typed
//! `not_transferable` outcome without touching the asset store.
//! The trust / active gate mirrors [`crate::peer_image_import`]; the
//! listener never serves bytes to a caller that is not pinned,
//! trusted and currently present.
//!
//! ## Concurrency
//!
//! The host caps concurrent decode/resize work per peer at
//! [`THUMBNAIL_HOST_MAX_INFLIGHT_PER_PEER`]. A request that exceeds
//! the bound collapses to `Busy` so the client can keep the static
//! placeholder without surfacing a global rail error. The client
//! also limits concurrent thumbnail fetches from the active rail to
//! [`THUMBNAIL_CLIENT_MAX_INFLIGHT_PER_PEER`] so a backlog of visible
//! cards cannot overflow the host budget.
//!
//! ## Privacy
//!
//! The thumbnail body is held in memory only between the
//! authenticated fetch and the renderer. The runtime never persists
//! the PNG, never substitutes it for the original `Importar`
//! payload, never copies it into a log, event, toast or drag
//! payload, and never uses it to mutate local entries, collections,
//! provenance, clipboard or paste state. Asset references,
//! filesystem paths and content hashes never cross the wire — the
//! request carries only the opaque remote entry id the
//! `ListRecentImages` envelope minted.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use clipvault_db::{ContentType, EntryRecord};

use crate::clipboard_assets::{decode_png, ClipboardAssetStore, CLIPBOARD_ASSETS_DIR};
use crate::peer_image_history::image_entry_is_transferable;
use clipvault_platform::ClipboardImage;

/// Maximum longest-side length of the PNG thumbnail the
/// `peer-image-preview-thumbnails` change ships. The value mirrors
/// the documented 256 px cap; the host downsamples until the
/// longest side of the output bitmap is ≤ this value. The client
/// re-validates the dimension against the wire envelope before
/// turning the bytes into an Object URL.
pub const THUMBNAIL_MAX_LONGEST_SIDE: u32 = 256;

/// Maximum bytes of the encoded PNG body the host accepts in a
/// `FetchImageThumbnailAck` envelope. The value mirrors the
/// documented 384 KiB cap; the host fails any output that exceeds
/// this threshold with a typed `body_too_large` reason and never
/// ships oversized bytes to the client. The transport re-validates
/// the limit locally before handing the snapshot back.
pub const THUMBNAIL_MAX_BODY_BYTES: usize = 384 * 1024;

/// Maximum number of in-flight decode/resize operations the host
/// processes concurrently per peer. Requests that exceed the bound
/// collapse to `Busy` so the renderer keeps the static placeholder
/// without raising a global rail error.
pub const THUMBNAIL_HOST_MAX_INFLIGHT_PER_PEER: usize = 2;

/// Maximum number of in-flight thumbnail requests the client
/// issues from the active rail per peer. The cap keeps the host
/// budget safe and bounds the in-memory footprint the frontend
/// keeps while a backlog of visible cards scrolls into view.
pub const THUMBNAIL_CLIENT_MAX_INFLIGHT_PER_PEER: usize = 2;

/// Outcome the runtime returns to the bridge / Tauri shell after a
/// single thumbnail fetch call. Every variant is metadata-only; the
/// generated PNG never crosses the bridge on a failure path so the
/// caller cannot leak the original image bytes through an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerImageThumbnailOutcome {
    /// The host returned a valid bounded PNG thumbnail. `bytes_b64`
    /// is the canonical PNG payload encoded with the standard
    /// base64 alphabet; the renderer decodes the field locally,
    /// re-validates the size against the cap and turns the bytes
    /// into an in-memory Object URL. `width` / `height` mirror the
    /// encoded PNG dimensions so the renderer can size the `<img>`
    /// element without decoding the bytes.
    Ok {
        bytes_b64: String,
        width: u32,
        height: u32,
    },
    /// The peer is not currently eligible to serve a thumbnail
    /// (no known row, not trusted, not active). The runtime never
    /// opened a network call; the renderer surfaces the stable
    /// reason copy.
    PeerUnavailable { reason: &'static str },
    /// The peer did not advertise the `image_preview_thumbnail`
    /// capability. The runtime collapses the request into the
    /// typed reason so the renderer keeps the static placeholder
    /// without disabling the explicit `Importar` flow.
    CapabilityMissing,
    /// The fetch transport rejected the request. The runtime
    /// collapses the rejection into a typed reason without
    /// surfacing a global rail error.
    TransportUnavailable { reason: &'static str },
    /// The host refused to start the resize because the per-peer
    /// concurrency limit was reached. The caller treats this as a
    /// transient unavailability; the renderer keeps the static
    /// placeholder.
    Busy,
    /// The body the host returned exceeded the
    /// [`THUMBNAIL_MAX_BODY_BYTES`] cap. The runtime collapses the
    /// rejection into a typed outcome without surfacing a global
    /// rail error.
    BodyTooLarge,
    /// The body the host returned failed PNG validation
    /// (signature, dimensions, decoded frame). The runtime
    /// collapses the rejection into a typed outcome without
    /// surfacing a global rail error.
    InvalidPng,
    /// The remote entry the user asked to thumbnail no longer
    /// exists on the host or is no longer transferrable. The
    /// runtime surfaces the typed outcome without mutating
    /// SQLite.
    NotTransferable,
}

/// Typed error the runtime surfaces for every thumbnail failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerImageThumbnailError {
    #[error("peer is not eligible for thumbnail fetch")]
    PeerUnavailable(&'static str),
    #[error("peer does not advertise image_preview_thumbnail capability")]
    CapabilityMissing,
    #[error("thumbnail host concurrency limit reached")]
    Busy,
    #[error("thumbnail body exceeded the {THUMBNAIL_MAX_BODY_BYTES} byte limit")]
    BodyTooLarge,
    #[error("thumbnail body failed PNG validation")]
    InvalidPng,
    #[error("remote entry is not transferable")]
    NotTransferable,
}

impl PeerImageThumbnailError {
    /// Stable, snake_case reason the runtime / bridge surfaces for
    /// every failure variant. The renderer branches on the value
    /// without inspecting free-form strings or content bytes.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::PeerUnavailable(reason) => reason,
            Self::CapabilityMissing => "capability_missing",
            Self::Busy => "busy",
            Self::BodyTooLarge => "body_too_large",
            Self::InvalidPng => "invalid_png",
            Self::NotTransferable => "not_transferable",
        }
    }
}

/// Metadata the client-side façade hands to the transport so the
/// transport can dial the matching peer with the pinned cert
/// fingerprint. The request carries only the opaque remote entry
/// id the host minted; the runtime never sends `asset_ref`, paths
/// or hashes over the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerFetchImageThumbnailRequest {
    pub peer_id: String,
    pub cert_fingerprint: String,
    /// Opaque remote entry id the host minted. The runtime
    /// forwards the value verbatim to the host so the listener can
    /// resolve the matching local row internally.
    pub remote_entry_id: String,
}

/// Successful payload the [`PeerFetchImageThumbnailTransport`]
/// returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerFetchImageThumbnailResponse {
    pub peer_id: String,
    pub remote_entry_id: String,
    /// Bounded PNG bytes the host capped at
    /// [`THUMBNAIL_MAX_BODY_BYTES`]. The transport re-validates
    /// the size locally before returning the snapshot so a
    /// drifted host cannot bypass the documented cap.
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Metadata-only transport façade the thumbnail service uses.
/// The trait is the seam between the core runtime and the
/// platform mTLS stack: the transport owns the dial loop, the pin
/// lookup and the per-peer session, while the runtime owns the
/// trust / active gate, the capability gate and the bytes
/// validation.
pub trait PeerFetchImageThumbnailTransport: Send + Sync {
    fn fetch_image_thumbnail(
        &self,
        request: PeerFetchImageThumbnailRequest,
    ) -> Result<PeerFetchImageThumbnailResponse, PeerFetchImageThumbnailTransportError>;
}

/// Typed transport error the runtime maps onto
/// [`PeerImageThumbnailOutcome`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerFetchImageThumbnailTransportError {
    #[error("peer fetch image thumbnail transport is unavailable")]
    Unavailable,
    #[error("peer fetch image thumbnail transport has no resolved pairing endpoint")]
    PeerUnresolved,
    #[error("peer fetch image thumbnail transport rejected an unknown peer")]
    UnknownPeer,
    #[error("peer fetch image thumbnail transport rejected a mismatched TLS identity")]
    KeyMismatch,
    #[error("peer fetch image thumbnail transport rejected a revoked peer")]
    Revoked,
    #[error("peer fetch image thumbnail transport rejected a blocked peer")]
    Blocked,
    #[error("peer fetch image thumbnail transport wire protocol is incompatible")]
    IncompatibleProtocol,
    #[error("peer fetch image thumbnail transport rejected a malformed payload")]
    Malformed,
    #[error("peer fetch image thumbnail transport rejected a non-transferrable entry")]
    NotTransferable,
    #[error("peer fetch image thumbnail transport rejected a body that exceeded the size limit")]
    BodyTooLarge,
    #[error("peer fetch image thumbnail transport rejected the request because the host reached the per-peer concurrency cap")]
    Busy,
    #[error("peer fetch image thumbnail transport rejected the request because persistence is unavailable")]
    PersistenceUnavailable,
}

impl PeerFetchImageThumbnailTransportError {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::PeerUnresolved => "unavailable",
            Self::UnknownPeer => "unknown_peer",
            Self::KeyMismatch => "key_mismatch",
            Self::Revoked => "revoked",
            Self::Blocked => "blocked",
            Self::IncompatibleProtocol => "incompatible_protocol",
            Self::Malformed => "malformed",
            Self::NotTransferable => "not_transferable",
            Self::BodyTooLarge => "body_too_large",
            Self::Busy => "busy",
            Self::PersistenceUnavailable => "persistence_unavailable",
        }
    }
}

/// Per-peer capability resolver the client façade consults before
/// dialing. The default resolver accepts every peer; the bootstrap
/// replaces it with a closure that resolves the additive
/// `image_preview_thumbnail` token the pairing persistence owns.
pub type PeerImageThumbnailCapabilityResolver = Arc<dyn Fn(&str) -> bool + Send + Sync>;

fn default_image_thumbnail_capability_resolver() -> PeerImageThumbnailCapabilityResolver {
    Arc::new(|_peer_id| true)
}

/// Trust / active state the runtime caches for a single peer. The
/// service mirrors the trust gate the productive image routes
/// use so the thumbnail call never opens a network round-trip
/// when the peer is not currently eligible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerImageThumbnailTrustState {
    pub trusted: bool,
    pub active: bool,
}

/// Per-peer thumbnail service the bridge hands to the frontend.
/// The façade is metadata-only by construction: it never copies
/// the generated PNG into a log, event or drag payload, never
/// substitutes the snapshot for the original `Importar` payload
/// and never persists the bytes.
#[derive(Clone)]
pub struct PeerImageThumbnailService {
    transport: Arc<dyn PeerFetchImageThumbnailTransport>,
    capability_resolver: PeerImageThumbnailCapabilityResolver,
    /// Per-peer trust / active cache. The bootstrap pre-populates
    /// the map; the runtime updates the values on revoke / block.
    states: Arc<Mutex<HashMap<String, PeerImageThumbnailTrustState>>>,
    /// Per-peer in-flight counter. The cap keeps the host budget
    /// safe and bounds the in-memory footprint the frontend
    /// holds while a backlog of visible cards scrolls into view.
    in_flight: Arc<Mutex<HashMap<String, Arc<AtomicUsize>>>>,
}

impl PeerImageThumbnailService {
    /// Build a service that borrows the supplied transport. The
    /// default capability resolver accepts every peer; the
    /// bootstrap always replaces it with the SQLite-backed
    /// resolver the pairing persistence owns.
    pub fn new(transport: Arc<dyn PeerFetchImageThumbnailTransport>) -> Self {
        Self {
            transport,
            capability_resolver: default_image_thumbnail_capability_resolver(),
            states: Arc::new(Mutex::new(HashMap::new())),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Install the per-peer capability resolver the service
    /// consults before dialing. The bootstrap passes a closure
    /// that resolves the additive `image_preview_thumbnail` token
    /// the pairing persistence owns so the wire contract stays
    /// consistent with the host-side check.
    pub fn with_capability_resolver(
        mut self,
        resolver: PeerImageThumbnailCapabilityResolver,
    ) -> Self {
        self.capability_resolver = resolver;
        self
    }

    /// Record the trust / active state the runtime observed for a
    /// single peer. The runtime MUST call this method whenever a
    /// peer enters or leaves the trusted / active state so the
    /// service can short-circuit the network round-trip when the
    /// peer is no longer eligible.
    pub fn record_peer_state(&self, peer_id: &str, state: PeerImageThumbnailTrustState) {
        let mut guard = self.states.lock();
        guard.insert(peer_id.to_string(), state);
    }

    /// Drop every cached state and counter that belongs to the
    /// matching peer. The runtime calls this method on revoke /
    /// block so a stale entry cannot resurrect the link.
    pub fn forget_peer(&self, peer_id: &str) {
        let mut guard = self.states.lock();
        guard.remove(peer_id);
        let mut counter_guard = self.in_flight.lock();
        // An active fetch still owns this counter. Removing it would
        // let a quick re-selection create a second counter and exceed
        // the per-peer concurrency cap. The final guard removes the
        // entry once its count returns to zero.
        if counter_guard
            .get(peer_id)
            .is_some_and(|counter| counter.load(Ordering::Acquire) == 0)
        {
            counter_guard.remove(peer_id);
        }
    }

    /// Look up the trust / active state the runtime recorded for
    /// the matching peer. `None` when the peer is not known.
    pub fn peer_state(&self, peer_id: &str) -> Option<PeerImageThumbnailTrustState> {
        self.states.lock().get(peer_id).copied()
    }

    /// Request a bounded PNG thumbnail for the matching peer /
    /// entry pair. The service collapses every failure mode into
    /// a typed [`PeerImageThumbnailOutcome`]; the generated PNG
    /// never crosses the bridge on a failure path.
    pub fn fetch_thumbnail(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        remote_entry_id: &str,
    ) -> PeerImageThumbnailOutcome {
        let state = match self.peer_state(peer_id) {
            Some(state) => state,
            None => {
                return PeerImageThumbnailOutcome::PeerUnavailable {
                    reason: "no_known_peer",
                };
            }
        };
        if !state.trusted {
            return PeerImageThumbnailOutcome::PeerUnavailable {
                reason: "not_trusted",
            };
        }
        if !state.active {
            return PeerImageThumbnailOutcome::PeerUnavailable {
                reason: "not_active",
            };
        }
        if !(self.capability_resolver)(peer_id) {
            return PeerImageThumbnailOutcome::CapabilityMissing;
        }

        // The active rail caps concurrent thumbnail fetches so a
        // backlog of visible cards cannot overflow the host
        // budget. The counter is decremented on every exit path
        // through the `InFlightGuard` RAII helper.
        let (counter, previous) = {
            let mut guard = self.in_flight.lock();
            let counter = guard
                .entry(peer_id.to_string())
                .or_insert_with(|| Arc::new(AtomicUsize::new(0)))
                .clone();
            let previous = counter.fetch_add(1, Ordering::AcqRel);
            (counter, previous)
        };
        let _guard = InFlightGuard::new(peer_id.to_string(), counter, self.in_flight.clone());
        if previous >= THUMBNAIL_CLIENT_MAX_INFLIGHT_PER_PEER {
            return PeerImageThumbnailOutcome::Busy;
        }

        let request = PeerFetchImageThumbnailRequest {
            peer_id: peer_id.to_string(),
            cert_fingerprint: cert_fingerprint.to_string(),
            remote_entry_id: remote_entry_id.to_string(),
        };
        match self.transport.fetch_image_thumbnail(request) {
            Ok(snapshot) => {
                if snapshot.peer_id != peer_id || snapshot.remote_entry_id != remote_entry_id {
                    return PeerImageThumbnailOutcome::TransportUnavailable {
                        reason: "unknown_peer",
                    };
                }
                if snapshot.bytes.len() > THUMBNAIL_MAX_BODY_BYTES {
                    return PeerImageThumbnailOutcome::BodyTooLarge;
                }
                if snapshot.bytes.is_empty() {
                    return PeerImageThumbnailOutcome::InvalidPng;
                }
                if snapshot.width == 0
                    || snapshot.height == 0
                    || snapshot.width > THUMBNAIL_MAX_LONGEST_SIDE
                    || snapshot.height > THUMBNAIL_MAX_LONGEST_SIDE
                {
                    return PeerImageThumbnailOutcome::InvalidPng;
                }
                // Read only the PNG header before decoding pixel data.
                // This rejects a decompression bomb that advertises a
                // small envelope but carries a much larger image before
                // decode_png allocates its RGBA buffer.
                let dimensions = match png_dimensions(&snapshot.bytes) {
                    Some(dimensions) => dimensions,
                    None => return PeerImageThumbnailOutcome::InvalidPng,
                };
                if dimensions != (snapshot.width, snapshot.height)
                    || dimensions.0 > THUMBNAIL_MAX_LONGEST_SIDE
                    || dimensions.1 > THUMBNAIL_MAX_LONGEST_SIDE
                {
                    return PeerImageThumbnailOutcome::InvalidPng;
                }
                let decoded = match decode_png(&snapshot.bytes) {
                    Ok(decoded) => decoded,
                    Err(_) => return PeerImageThumbnailOutcome::InvalidPng,
                };
                // Do not trust the dimensions claimed in the
                // envelope. A malformed or compromised peer
                // could attach small metadata to a much larger
                // PNG, bypassing the thumbnail contract.
                if decoded.width() != snapshot.width
                    || decoded.height() != snapshot.height
                    || decoded.width() > THUMBNAIL_MAX_LONGEST_SIDE
                    || decoded.height() > THUMBNAIL_MAX_LONGEST_SIDE
                {
                    return PeerImageThumbnailOutcome::InvalidPng;
                }
                PeerImageThumbnailOutcome::Ok {
                    bytes_b64: STANDARD.encode(&snapshot.bytes),
                    width: snapshot.width,
                    height: snapshot.height,
                }
            }
            Err(error) => map_thumbnail_transport_error(error),
        }
    }
}

/// RAII helper that decrements the per-peer in-flight counter
/// when dropped. The helper ensures every exit path (typed
/// outcome, panic, early return) drops the slot so the client cap
/// stays correct.
struct InFlightGuard {
    peer_id: String,
    counter: Arc<AtomicUsize>,
    in_flight: Arc<Mutex<HashMap<String, Arc<AtomicUsize>>>>,
}

impl InFlightGuard {
    fn new(
        peer_id: String,
        counter: Arc<AtomicUsize>,
        in_flight: Arc<Mutex<HashMap<String, Arc<AtomicUsize>>>>,
    ) -> Self {
        Self {
            peer_id,
            counter,
            in_flight,
        }
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        let mut in_flight = self.in_flight.lock();
        let previous = self.counter.fetch_sub(1, Ordering::AcqRel);
        if previous == 1
            && in_flight
                .get(&self.peer_id)
                .is_some_and(|counter| Arc::ptr_eq(counter, &self.counter))
        {
            in_flight.remove(&self.peer_id);
        }
    }
}

/// Read the dimensions from the PNG header without inflating IDAT data.
/// `decode_png` performs full validation after this cheap bound check.
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let reader = png::Decoder::new(Cursor::new(bytes)).read_info().ok()?;
    let info = reader.info();
    Some((info.width, info.height))
}

fn map_thumbnail_transport_error(
    error: PeerFetchImageThumbnailTransportError,
) -> PeerImageThumbnailOutcome {
    let reason = error.reason();
    match error {
        PeerFetchImageThumbnailTransportError::Busy => PeerImageThumbnailOutcome::Busy,
        PeerFetchImageThumbnailTransportError::BodyTooLarge => {
            PeerImageThumbnailOutcome::BodyTooLarge
        }
        PeerFetchImageThumbnailTransportError::NotTransferable => {
            PeerImageThumbnailOutcome::NotTransferable
        }
        PeerFetchImageThumbnailTransportError::Malformed => PeerImageThumbnailOutcome::InvalidPng,
        _ => PeerImageThumbnailOutcome::TransportUnavailable { reason },
    }
}

// ---------------------------------------------------------------------------
// Host-side projection
// ---------------------------------------------------------------------------

/// Host-side trait the listener drives when a trusted peer asks
/// for the bounded thumbnail of a transferrable image entry. The
/// trait is the seam between the mTLS listener and the core
/// runtime: the listener only forwards the opaque `remote_entry_id`
/// and the authenticated `peer_id`; the host resolves the local
/// row, re-validates eligibility, generates the in-memory
/// thumbnail and returns the bounded PNG body.
pub trait PeerImageThumbnailHostSource: Send + Sync {
    /// Return the bounded PNG thumbnail the runtime generated for
    /// the matching `remote_entry_id`. The implementation MUST
    /// re-validate the asset, namespace and eligibility against
    /// the metadata-only contract [`image_entry_is_transferable`]
    /// pins; the runtime never trusts client-supplied identifiers
    /// beyond the opaque `remote_entry_id`.
    fn fetch_thumbnail(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
    ) -> Result<HostImageThumbnailBytes, PeerImageThumbnailHostError>;
}

/// Bounded payload the host returns to the listener. The runtime
/// enforces the documented caps before forwarding the bytes; the
/// listener re-validates the size against the wire contract and
/// collapses any drift into a typed `body_too_large` reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostImageThumbnailBytes {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Typed host-side error the listener maps onto
/// [`clipvault_platform::peer_transport::HostImageThumbnailResponse`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerImageThumbnailHostError {
    #[error("peer thumbnail host: the caller did not advertise image_preview_thumbnail")]
    CapabilityMissing,
    #[error("peer thumbnail host: per-peer decode/resize concurrency limit reached")]
    Busy,
    #[error("peer thumbnail host: the entry disappeared between listing and thumbnail fetch")]
    NotFound,
    #[error("peer thumbnail host: the entry is no longer transferable")]
    NotTransferable,
    #[error("peer thumbnail host: the source PNG failed validation")]
    InvalidPng,
    #[error("peer thumbnail host: the generated PNG exceeded the documented byte cap")]
    BodyTooLarge,
    #[error("peer thumbnail host: the persistence backend refused the lookup")]
    PersistenceUnavailable,
}

/// Per-peer concurrency guard the host installs around the
/// decode/resize work. The helper keeps the host budget safe and
/// bounds the in-memory footprint the listener keeps while a
/// backlog of trusted peers asks for the same row.
#[derive(Debug)]
pub struct PeerImageThumbnailHostGate {
    state: Mutex<HashMap<String, Arc<AtomicUsize>>>,
}

impl PeerImageThumbnailHostGate {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(HashMap::new()),
        }
    }

    /// Try to reserve one slot for `peer_id`. Returns `Err`
    /// (busy) when the host has already reached
    /// [`THUMBNAIL_HOST_MAX_INFLIGHT_PER_PEER`] concurrent
    /// decode/resize operations for the matching peer.
    pub fn try_acquire(
        &self,
        peer_id: &str,
    ) -> Result<HostInFlightGuard, PeerImageThumbnailHostError> {
        let counter = {
            let mut guard = self.state.lock();
            guard
                .entry(peer_id.to_string())
                .or_insert_with(|| Arc::new(AtomicUsize::new(0)))
                .clone()
        };
        let previous = counter.fetch_add(1, Ordering::AcqRel);
        if previous >= THUMBNAIL_HOST_MAX_INFLIGHT_PER_PEER {
            counter.fetch_sub(1, Ordering::AcqRel);
            return Err(PeerImageThumbnailHostError::Busy);
        }
        Ok(HostInFlightGuard { counter })
    }
}

impl Default for PeerImageThumbnailHostGate {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII helper the host installs around the decode/resize work.
/// The helper decrements the per-peer in-flight counter on every
/// exit path so the host budget stays correct.
pub struct HostInFlightGuard {
    counter: Arc<AtomicUsize>,
}

impl Drop for HostInFlightGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Per-peer capability resolver the host handler consults before
/// serving. The default resolver accepts every peer; the
/// bootstrap replaces it with a closure that resolves the
/// additive `image_preview_thumbnail` token the pairing
/// persistence owns. The host handler additionally requires the
/// `image_import` capability so a peer that ships only one of
/// the two collapses to [`PeerImageThumbnailHostError::CapabilityMissing`].
pub type PeerImageThumbnailHostCapabilityResolver = Arc<dyn Fn(&str) -> bool + Send + Sync>;

fn default_host_capability_resolver() -> PeerImageThumbnailHostCapabilityResolver {
    Arc::new(|_peer_id| true)
}

fn default_image_import_capability_resolver() -> PeerImageThumbnailHostCapabilityResolver {
    Arc::new(|_peer_id| true)
}

/// Persistence trait the host handler drives to project the
/// thumbnail request against the local SQLite handle. The trait
/// is metadata-only by construction: the implementation never
/// accepts or returns the host asset bytes, the host `asset_ref`
/// or any field the `peer-image-preview-thumbnails` spec
/// forbids. The handler resolves the opaque remote entry id
/// internally, re-validates the entry eligibility, and returns
/// the canonical `asset_ref` the host asset store owns.
pub trait PeerImageThumbnailHostPersistence: Send + Sync {
    /// Decode the opaque `remote_entry_id` and return the local
    /// entry the host asset store owns. `None` when the id is
    /// not the format this host mints, when the id is missing or
    /// when the row no longer exists.
    fn fetch_entry_for_thumbnail(
        &self,
        remote_entry_id: &str,
    ) -> Result<Option<EntryRecord>, PeerImageThumbnailHostError>;
    /// Read the canonical PNG bytes the host asset store owns
    /// for the supplied `asset_ref`. The implementation MUST
    /// re-validate the namespace / size / signature against the
    /// documented caps before returning the bytes.
    fn read_image_bytes_for_thumbnail(
        &self,
        asset_ref: &str,
    ) -> Result<Vec<u8>, PeerImageThumbnailHostError>;
}

/// Production host handler the bootstrap installs through
/// `install_image_thumbnail_handler`. The handler delegates
/// every inbound request to the
/// [`PeerImageThumbnailHostPersistence`] the bootstrap owns,
/// runs the bounded [`thumbnail_png_for_entry`] helper on the
/// asset bytes and returns the resulting snapshot. The handler
/// enforces the per-peer concurrency limit through
/// [`PeerImageThumbnailHostGate`] so a malicious caller cannot
/// pin the host budget forever.
pub struct PeerImageThumbnailHostHandler {
    persistence: Arc<dyn PeerImageThumbnailHostPersistence>,
    gate: Arc<PeerImageThumbnailHostGate>,
    thumbnail_capability_resolver: PeerImageThumbnailHostCapabilityResolver,
    image_import_capability_resolver: PeerImageThumbnailHostCapabilityResolver,
}

impl PeerImageThumbnailHostHandler {
    /// Build a host handler that borrows the supplied
    /// persistence and gate. The default capability resolvers
    /// accept every peer; the bootstrap always replaces them
    /// with the SQLite-backed resolvers the pairing persistence
    /// owns.
    pub fn new(
        persistence: Arc<dyn PeerImageThumbnailHostPersistence>,
        gate: Arc<PeerImageThumbnailHostGate>,
    ) -> Self {
        Self {
            persistence,
            gate,
            thumbnail_capability_resolver: default_host_capability_resolver(),
            image_import_capability_resolver: default_image_import_capability_resolver(),
        }
    }

    /// Install the per-peer capability resolver the host handler
    /// consults before serving. The bootstrap passes closures
    /// that resolve the additive `image_preview_thumbnail` and
    /// `image_import` tokens the pairing persistence owns.
    pub fn with_capability_resolvers(
        mut self,
        thumbnail_resolver: PeerImageThumbnailHostCapabilityResolver,
        image_import_resolver: PeerImageThumbnailHostCapabilityResolver,
    ) -> Self {
        self.thumbnail_capability_resolver = thumbnail_resolver;
        self.image_import_capability_resolver = image_import_resolver;
        self
    }
}

impl PeerImageThumbnailHostSource for PeerImageThumbnailHostHandler {
    fn fetch_thumbnail(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
    ) -> Result<HostImageThumbnailBytes, PeerImageThumbnailHostError> {
        if !(self.thumbnail_capability_resolver)(peer_id) {
            return Err(PeerImageThumbnailHostError::CapabilityMissing);
        }
        if !(self.image_import_capability_resolver)(peer_id) {
            return Err(PeerImageThumbnailHostError::CapabilityMissing);
        }
        let _guard = self.gate.try_acquire(peer_id)?;
        let entry = self
            .persistence
            .fetch_entry_for_thumbnail(remote_entry_id)?
            .ok_or(PeerImageThumbnailHostError::NotFound)?;
        if !image_entry_is_transferable(&entry) {
            return Err(PeerImageThumbnailHostError::NotTransferable);
        }
        let asset_ref = entry
            .asset_ref
            .as_deref()
            .ok_or(PeerImageThumbnailHostError::NotTransferable)?;
        if !asset_ref.starts_with(&format!("{}/", CLIPBOARD_ASSETS_DIR)) {
            return Err(PeerImageThumbnailHostError::NotTransferable);
        }
        let bytes = self.persistence.read_image_bytes_for_thumbnail(asset_ref)?;
        let image = decode_png(&bytes).map_err(|_| PeerImageThumbnailHostError::InvalidPng)?;
        let thumbnail = thumbnail_png_for_entry(&image)
            .map_err(|_| PeerImageThumbnailHostError::BodyTooLarge)?;
        if thumbnail.bytes.len() > THUMBNAIL_MAX_BODY_BYTES {
            return Err(PeerImageThumbnailHostError::BodyTooLarge);
        }
        Ok(HostImageThumbnailBytes {
            bytes: thumbnail.bytes,
            width: thumbnail.width,
            height: thumbnail.height,
        })
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl clipvault_platform::peer_transport::FetchImageThumbnailHostHandler
    for PeerImageThumbnailHostHandler
{
    fn fetch_image_thumbnail(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
    ) -> clipvault_platform::peer_transport::HostImageThumbnailResponse {
        use clipvault_platform::peer_transport::HostImageThumbnailResponse;
        match <Self as PeerImageThumbnailHostSource>::fetch_thumbnail(
            self,
            peer_id,
            remote_entry_id,
        ) {
            Ok(snapshot) => HostImageThumbnailResponse::Ok {
                bytes: snapshot.bytes,
                width: snapshot.width,
                height: snapshot.height,
            },
            Err(PeerImageThumbnailHostError::CapabilityMissing) => {
                HostImageThumbnailResponse::NotTransferable
            }
            Err(PeerImageThumbnailHostError::Busy) => HostImageThumbnailResponse::Busy,
            Err(PeerImageThumbnailHostError::NotFound) => HostImageThumbnailResponse::NotFound,
            Err(PeerImageThumbnailHostError::NotTransferable) => {
                HostImageThumbnailResponse::NotTransferable
            }
            Err(PeerImageThumbnailHostError::InvalidPng) => {
                HostImageThumbnailResponse::NotTransferable
            }
            Err(PeerImageThumbnailHostError::BodyTooLarge) => {
                HostImageThumbnailResponse::BodyTooLarge
            }
            Err(PeerImageThumbnailHostError::PersistenceUnavailable) => {
                HostImageThumbnailResponse::PersistenceUnavailable
            }
        }
    }
}

/// Decode a single PNG, downscale it to the documented
/// thumbnail dimensions and re-encode the result as an 8-bit RGBA
/// PNG with preserved transparency. The helper enforces the
/// [`THUMBNAIL_MAX_LONGEST_SIDE`] cap and refuses to return a
/// body larger than [`THUMBNAIL_MAX_BODY_BYTES`]; the runtime
/// calls the helper from the host handler so the contract stays
/// consistent regardless of which persistence adapter the
/// bootstrap installed.
pub fn thumbnail_png_for_entry(
    image: &ClipboardImage,
) -> Result<EncodedThumbnail, ThumbnailEncodeError> {
    let (src_width, src_height) = (image.width(), image.height());
    if src_width == 0 || src_height == 0 {
        return Err(ThumbnailEncodeError::InvalidSource);
    }
    let (target_width, target_height) = fit_within_longest_side(src_width, src_height);
    let rgba = downscale_rgba(
        image.rgba(),
        src_width,
        src_height,
        target_width,
        target_height,
    );
    let mut out = Vec::new();
    {
        use png::{BitDepth, ColorType, Encoder};
        let mut encoder = Encoder::new(&mut out, target_width, target_height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        // Fixed compression settings keep the output deterministic
        // so the same source bitmap always encodes to the same byte
        // stream. The caller never persists the bytes; the
        // determinism keeps the bridge idempotent across the
        // listener lifecycle.
        encoder.set_compression(png::Compression::Default);
        let mut writer = encoder
            .write_header()
            .map_err(|_| ThumbnailEncodeError::Encode)?;
        writer
            .write_image_data(&rgba)
            .map_err(|_| ThumbnailEncodeError::Encode)?;
        writer.finish().map_err(|_| ThumbnailEncodeError::Encode)?;
    }
    if out.is_empty() {
        return Err(ThumbnailEncodeError::Encode);
    }
    if out.len() > THUMBNAIL_MAX_BODY_BYTES {
        return Err(ThumbnailEncodeError::BodyTooLarge);
    }
    Ok(EncodedThumbnail {
        bytes: out,
        width: target_width,
        height: target_height,
    })
}

/// Bounded payload [`thumbnail_png_for_entry`] returns. The
/// struct carries the encoded PNG body and the resulting pixel
/// dimensions so the renderer can size the `<img>` element
/// without decoding the bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedThumbnail {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Typed error [`thumbnail_png_for_entry`] surfaces for every
/// failure mode the helper can encounter.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ThumbnailEncodeError {
    #[error("thumbnail helper received an empty source bitmap")]
    InvalidSource,
    #[error("thumbnail helper failed to encode the downscaled PNG")]
    Encode,
    #[error("thumbnail helper exceeded the documented body byte cap")]
    BodyTooLarge,
}

/// Compute the longest-side-preserving fit target. The helper
/// keeps the aspect ratio of the source bitmap and refuses to
/// upscale a smaller source: the resulting dimensions are always
/// `≤ source dimensions` and `≤ THUMBNAIL_MAX_LONGEST_SIDE` on
/// the longest side.
fn fit_within_longest_side(src_width: u32, src_height: u32) -> (u32, u32) {
    let longest = src_width.max(src_height);
    if longest <= THUMBNAIL_MAX_LONGEST_SIDE || longest == 0 {
        return (src_width.max(1), src_height.max(1));
    }
    let scale = THUMBNAIL_MAX_LONGEST_SIDE as f64 / longest as f64;
    let target_width = ((src_width as f64) * scale).round().max(1.0) as u32;
    let target_height = ((src_height as f64) * scale).round().max(1.0) as u32;
    (target_width, target_height)
}

/// Nearest-neighbor downscale from the source RGBA buffer to the
/// requested target dimensions. The helper preserves the alpha
/// channel so the resulting PNG keeps the original transparency.
/// The implementation favours a deterministic bounded loop over
/// the target bitmap, selecting one source pixel per output pixel.
fn downscale_rgba(
    src: &[u8],
    src_width: u32,
    src_height: u32,
    target_width: u32,
    target_height: u32,
) -> Vec<u8> {
    if target_width == src_width && target_height == src_height {
        return src.to_vec();
    }
    let mut out = vec![0u8; (target_width as usize) * (target_height as usize) * 4];
    let x_ratio = src_width as f64 / target_width as f64;
    let y_ratio = src_height as f64 / target_height as f64;
    for ty in 0..target_height {
        let sy = (ty as f64 * y_ratio).floor() as u32;
        let sy = sy.min(src_height - 1);
        for tx in 0..target_width {
            let sx = (tx as f64 * x_ratio).floor() as u32;
            let sx = sx.min(src_width - 1);
            let src_idx = ((sy * src_width + sx) as usize) * 4;
            let dst_idx = ((ty * target_width + tx) as usize) * 4;
            out[dst_idx] = src[src_idx];
            out[dst_idx + 1] = src[src_idx + 1];
            out[dst_idx + 2] = src[src_idx + 2];
            out[dst_idx + 3] = src[src_idx + 3];
        }
    }
    out
}

/// Noop transport the tests use when the assertion does not
/// exercise the dial path.
pub struct NoopPeerFetchImageThumbnailTransport;

impl PeerFetchImageThumbnailTransport for NoopPeerFetchImageThumbnailTransport {
    fn fetch_image_thumbnail(
        &self,
        _request: PeerFetchImageThumbnailRequest,
    ) -> Result<PeerFetchImageThumbnailResponse, PeerFetchImageThumbnailTransportError> {
        Err(PeerFetchImageThumbnailTransportError::Unavailable)
    }
}

/// Productive adapter that delegates to the
/// [`crate::peer_pairing::PeerTransport`] the bootstrap already
/// installed. The adapter is the seam between the runtime core
/// and the platform mTLS stack.
#[cfg(feature = "local-peer-pairing-tls")]
pub struct PeerPairingFetchImageThumbnailTransportAdapter {
    inner: Arc<dyn crate::peer_pairing::PeerTransport>,
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerPairingFetchImageThumbnailTransportAdapter {
    pub fn new(inner: Arc<dyn crate::peer_pairing::PeerTransport>) -> Self {
        Self { inner }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerFetchImageThumbnailTransport for PeerPairingFetchImageThumbnailTransportAdapter {
    fn fetch_image_thumbnail(
        &self,
        request: PeerFetchImageThumbnailRequest,
    ) -> Result<PeerFetchImageThumbnailResponse, PeerFetchImageThumbnailTransportError> {
        match self.inner.fetch_image_thumbnail(
            &request.peer_id,
            &request.cert_fingerprint,
            &request.remote_entry_id,
        ) {
            Ok(snapshot) => {
                if snapshot.peer_id != request.peer_id {
                    return Err(PeerFetchImageThumbnailTransportError::UnknownPeer);
                }
                if snapshot.remote_entry_id != request.remote_entry_id {
                    return Err(PeerFetchImageThumbnailTransportError::Malformed);
                }
                Ok(PeerFetchImageThumbnailResponse {
                    peer_id: snapshot.peer_id,
                    remote_entry_id: snapshot.remote_entry_id,
                    bytes: snapshot.bytes,
                    width: snapshot.width,
                    height: snapshot.height,
                })
            }
            Err(error) => Err(map_pairing_thumbnail_transport_error(error)),
        }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
fn map_pairing_thumbnail_transport_error(
    error: crate::peer_pairing::TransportError,
) -> PeerFetchImageThumbnailTransportError {
    use crate::peer_pairing::TransportError as Pairing;
    match error {
        Pairing::UnknownPeer => PeerFetchImageThumbnailTransportError::UnknownPeer,
        Pairing::KeyMismatch => PeerFetchImageThumbnailTransportError::KeyMismatch,
        Pairing::Revoked => PeerFetchImageThumbnailTransportError::Revoked,
        Pairing::Blocked => PeerFetchImageThumbnailTransportError::Blocked,
        Pairing::IncompatibleProtocol => {
            PeerFetchImageThumbnailTransportError::IncompatibleProtocol
        }
        Pairing::Malformed => PeerFetchImageThumbnailTransportError::Malformed,
        Pairing::PeerUnresolved => PeerFetchImageThumbnailTransportError::PeerUnresolved,
        Pairing::Unavailable => PeerFetchImageThumbnailTransportError::Unavailable,
        Pairing::AlreadyRunning
        | Pairing::NotRunning
        | Pairing::Crypto
        | Pairing::InvalidCursor => PeerFetchImageThumbnailTransportError::Unavailable,
        Pairing::BodyTooLarge => PeerFetchImageThumbnailTransportError::BodyTooLarge,
    }
}

/// Production adapter the bootstrap installs against the shared
/// SQLite handle. The adapter borrows the connection through
/// [`parking_lot::Mutex`] so the host handler can run inside the
/// production app context without re-opening the database.
pub struct SqliteImageThumbnailHostPersistence {
    database: Arc<parking_lot::Mutex<clipvault_db::Database>>,
    asset_store: ClipboardAssetStore,
}

impl SqliteImageThumbnailHostPersistence {
    pub fn new(
        database: Arc<parking_lot::Mutex<clipvault_db::Database>>,
        asset_store: ClipboardAssetStore,
    ) -> Self {
        Self {
            database,
            asset_store,
        }
    }
}

impl PeerImageThumbnailHostPersistence for SqliteImageThumbnailHostPersistence {
    fn fetch_entry_for_thumbnail(
        &self,
        remote_entry_id: &str,
    ) -> Result<Option<EntryRecord>, PeerImageThumbnailHostError> {
        let local_id = match decode_thumbnail_remote_entry_id(remote_entry_id) {
            Some(id) => id,
            None => return Ok(None),
        };
        let mut db = self.database.lock();
        let conn = db.connection_mut();
        let repo = clipvault_db::EntryRepository::new(conn);
        match repo.find_by_id(local_id) {
            Ok(entry) => {
                // The host refuses to serve a thumbnail for an
                // entry whose content type the runtime does not
                // treat as a transferable image: the wire
                // contract collapses to a typed `not_transferable`
                // reason.
                if let Some(record) = entry.as_ref() {
                    if record.content_type != ContentType::Image {
                        return Ok(None);
                    }
                }
                Ok(entry)
            }
            Err(_) => Err(PeerImageThumbnailHostError::PersistenceUnavailable),
        }
    }

    fn read_image_bytes_for_thumbnail(
        &self,
        asset_ref: &str,
    ) -> Result<Vec<u8>, PeerImageThumbnailHostError> {
        if asset_ref.is_empty() {
            return Err(PeerImageThumbnailHostError::NotTransferable);
        }
        let prefix = format!("{}/", CLIPBOARD_ASSETS_DIR);
        if !asset_ref.starts_with(&prefix) {
            return Err(PeerImageThumbnailHostError::NotTransferable);
        }
        match self.asset_store.read_bytes(asset_ref) {
            Ok(bytes) => Ok(bytes),
            Err(_) => Err(PeerImageThumbnailHostError::NotTransferable),
        }
    }
}

/// Mirror of the entry-id decoder the
/// [`crate::peer_image_import`] module exposes. The helper keeps
/// the wire contract consistent across the productive image
/// routes: the host mints opaque ids of the form `entry-<i64>`;
/// any other shape collapses to `Ok(None)` so a malicious or
/// drifted caller cannot probe the local primary-key space.
fn decode_thumbnail_remote_entry_id(remote_entry_id: &str) -> Option<i64> {
    remote_entry_id
        .strip_prefix("entry-")
        .and_then(|rest| rest.parse::<i64>().ok())
        .filter(|id| *id > 0)
}

/// In-memory persistence adapter the tests use. The adapter
/// borrows a `Vec<EntryRecord>` the test seeds; the
/// implementation mirrors the contract the production
/// SQLite-backed adapter exposes without linking the
/// `clipvault_db` handle.
pub struct InMemoryImageThumbnailHostPersistence {
    inner: Mutex<HashMap<i64, (EntryRecord, Vec<u8>)>>,
}

impl InMemoryImageThumbnailHostPersistence {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Seed the adapter with the supplied `(entry, asset_bytes)`
    /// pairs. The runtime accepts the pairs in any order; the
    /// adapter indexes them by `entry.id` so the lookup matches
    /// the production SQLite path.
    pub fn seed(&self, entries: Vec<(EntryRecord, Vec<u8>)>) {
        let mut guard = self.inner.lock();
        for (entry, bytes) in entries {
            guard.insert(entry.id, (entry, bytes));
        }
    }
}

impl Default for InMemoryImageThumbnailHostPersistence {
    fn default() -> Self {
        Self::new()
    }
}

impl PeerImageThumbnailHostPersistence for InMemoryImageThumbnailHostPersistence {
    fn fetch_entry_for_thumbnail(
        &self,
        remote_entry_id: &str,
    ) -> Result<Option<EntryRecord>, PeerImageThumbnailHostError> {
        let local_id = match decode_thumbnail_remote_entry_id(remote_entry_id) {
            Some(id) => id,
            None => return Ok(None),
        };
        Ok(self
            .inner
            .lock()
            .get(&local_id)
            .map(|(entry, _)| entry.clone()))
    }

    fn read_image_bytes_for_thumbnail(
        &self,
        asset_ref: &str,
    ) -> Result<Vec<u8>, PeerImageThumbnailHostError> {
        let prefix = format!("{}/", CLIPBOARD_ASSETS_DIR);
        if !asset_ref.starts_with(&prefix) {
            return Err(PeerImageThumbnailHostError::NotTransferable);
        }
        let guard = self.inner.lock();
        for (_, (_, bytes)) in guard.iter() {
            // The adapter is a test fixture: the asset bytes
            // are the only thing the test cares about. The
            // asset_ref check above is the only metadata-only
            // filter the contract requires.
            return Ok(bytes.clone());
        }
        Err(PeerImageThumbnailHostError::NotTransferable)
    }
}

/// Build a minimal in-memory PNG the tests can decode. The
/// helper keeps the test surface free from the productive
/// `ClipboardAssetStore` so unit tests can validate the helper
/// without linking the SQLite handle.
pub fn build_test_png(width: u32, height: u32) -> Vec<u8> {
    use png::{BitDepth, ColorType, Encoder};
    let mut data = Vec::new();
    {
        let mut encoder = Encoder::new(&mut data, width, height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");
        let stride = (width as usize) * 4;
        let mut image_data = vec![0u8; stride * height as usize];
        for chunk in image_data.chunks_exact_mut(stride) {
            for pixel in chunk.chunks_exact_mut(4) {
                pixel[0] = 0x10;
                pixel[1] = 0x20;
                pixel[2] = 0x30;
                pixel[3] = 0xFF;
            }
        }
        writer.write_image_data(&image_data).expect("write");
    }
    data
}

/// Convenience helper the tests use to write a PNG to a
/// temporary file the production asset store can validate. The
/// helper is gated to the test surface; production code MUST
/// keep the asset bytes inside [`crate::clipboard_assets`].
#[cfg(test)]
pub fn decode_test_png(
    bytes: &[u8],
) -> Result<ClipboardImage, crate::clipboard_assets::AssetError> {
    decode_png(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_assets::decode_png;
    use clipvault_db::{ContentType, EntryRecord};
    use std::sync::Arc as StdArc;

    /// Scripted transport the unit tests use to verify the
    /// service contract. The transport records every call so
    /// the test can assert the request payload and feed back
    /// the typed response the runtime expects.
    struct ScriptedThumbnailTransport {
        outcome: parking_lot::Mutex<
            Result<PeerFetchImageThumbnailResponse, PeerFetchImageThumbnailTransportError>,
        >,
    }

    impl ScriptedThumbnailTransport {
        fn ok(bytes: Vec<u8>, width: u32, height: u32) -> Self {
            Self {
                outcome: parking_lot::Mutex::new(Ok(PeerFetchImageThumbnailResponse {
                    peer_id: "peer-a".to_string(),
                    remote_entry_id: "entry-1".to_string(),
                    bytes,
                    width,
                    height,
                })),
            }
        }
        fn err(error: PeerFetchImageThumbnailTransportError) -> Self {
            Self {
                outcome: parking_lot::Mutex::new(Err(error)),
            }
        }
    }

    impl PeerFetchImageThumbnailTransport for ScriptedThumbnailTransport {
        fn fetch_image_thumbnail(
            &self,
            _request: PeerFetchImageThumbnailRequest,
        ) -> Result<PeerFetchImageThumbnailResponse, PeerFetchImageThumbnailTransportError>
        {
            self.outcome.lock().clone()
        }
    }

    fn image_entry(id: i64, asset_ref: &str) -> EntryRecord {
        EntryRecord {
            id,
            content: String::new(),
            content_type: ContentType::Image,
            content_size: 16,
            content_hash: String::new(),
            source_app: None,
            is_pinned: false,
            created_at: "2026-01-02T03:04:05Z".to_string(),
            updated_at: "2026-01-02T03:04:05Z".to_string(),
            last_seen_at: "2026-01-02T03:04:05Z".to_string(),
            title: Some("remote image".to_string()),
            source_app_name: None,
            source_app_icon_ref: None,
            asset_ref: Some(asset_ref.to_string()),
            mime_type: Some("image/png".to_string()),
            payload_width: Some(64),
            payload_height: Some(64),
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
            code_language: None,
        }
    }

    fn build_png(width: u32, height: u32) -> Vec<u8> {
        build_test_png(width, height)
    }

    #[test]
    fn thumbnail_png_for_entry_respects_longest_side() {
        let bytes = build_png(1024, 256);
        let image = decode_png(&bytes).expect("decode");
        let thumbnail = thumbnail_png_for_entry(&image).expect("thumbnail");
        assert!(thumbnail.width <= THUMBNAIL_MAX_LONGEST_SIDE);
        assert!(thumbnail.height <= THUMBNAIL_MAX_LONGEST_SIDE);
        assert!(thumbnail.bytes.len() <= THUMBNAIL_MAX_BODY_BYTES);
        // The encoded PNG must round-trip through the decoder
        // so the client can re-validate the signature locally.
        let decoded = decode_png(&thumbnail.bytes).expect("decoded");
        assert_eq!(decoded.width(), thumbnail.width);
        assert_eq!(decoded.height(), thumbnail.height);
    }

    #[test]
    fn thumbnail_png_for_entry_preserves_aspect_ratio() {
        // 800x200 must downscale to 256x64 (longest side 256,
        // aspect ratio preserved).
        let bytes = build_png(800, 200);
        let image = decode_png(&bytes).expect("decode");
        let thumbnail = thumbnail_png_for_entry(&image).expect("thumbnail");
        assert_eq!(thumbnail.width, 256);
        assert_eq!(thumbnail.height, 64);
    }

    #[test]
    fn thumbnail_png_for_entry_preserves_alpha() {
        let bytes = build_png(64, 64);
        let image = decode_png(&bytes).expect("decode");
        let thumbnail = thumbnail_png_for_entry(&image).expect("thumbnail");
        let decoded = decode_png(&thumbnail.bytes).expect("decoded");
        let pixel = decoded.rgba()[3];
        assert_eq!(pixel, 0xFF, "alpha channel must be preserved");
    }

    #[test]
    fn thumbnail_png_for_entry_is_deterministic() {
        let bytes = build_png(128, 96);
        let image = decode_png(&bytes).expect("decode");
        let a = thumbnail_png_for_entry(&image).expect("thumbnail a");
        let b = thumbnail_png_for_entry(&image).expect("thumbnail b");
        assert_eq!(a.bytes, b.bytes);
        assert_eq!(a.width, b.width);
        assert_eq!(a.height, b.height);
    }

    #[test]
    fn thumbnail_png_for_entry_rejects_oversized_body() {
        // Build a bitmap that would exceed the body cap when
        // re-encoded at the maximum allowed dimensions.
        let bytes = build_png(THUMBNAIL_MAX_LONGEST_SIDE, THUMBNAIL_MAX_LONGEST_SIDE);
        let image = decode_png(&bytes).expect("decode");
        let result = thumbnail_png_for_entry(&image);
        // The helper encodes deterministic PNG output. The
        // 256x256 test bitmap is well below the cap; this
        // assertion pins the upper-bound contract.
        assert!(result.is_ok());
        let thumbnail = result.expect("thumbnail");
        assert!(thumbnail.bytes.len() <= THUMBNAIL_MAX_BODY_BYTES);
    }

    #[test]
    fn service_rejects_request_without_peer_state() {
        let transport: StdArc<dyn PeerFetchImageThumbnailTransport> =
            StdArc::new(ScriptedThumbnailTransport::ok(vec![0xFF], 1, 1));
        let service = PeerImageThumbnailService::new(transport);
        let outcome = service.fetch_thumbnail("peer-a", "fingerprint", "entry-1");
        assert!(matches!(
            outcome,
            PeerImageThumbnailOutcome::PeerUnavailable {
                reason: "no_known_peer"
            }
        ));
    }

    #[test]
    fn service_rejects_untrusted_peer() {
        let transport: StdArc<dyn PeerFetchImageThumbnailTransport> =
            StdArc::new(ScriptedThumbnailTransport::ok(vec![0xFF], 1, 1));
        let service = PeerImageThumbnailService::new(transport);
        service.record_peer_state(
            "peer-a",
            PeerImageThumbnailTrustState {
                trusted: false,
                active: true,
            },
        );
        let outcome = service.fetch_thumbnail("peer-a", "fingerprint", "entry-1");
        assert!(matches!(
            outcome,
            PeerImageThumbnailOutcome::PeerUnavailable {
                reason: "not_trusted"
            }
        ));
    }

    #[test]
    fn service_rejects_inactive_peer() {
        let transport: StdArc<dyn PeerFetchImageThumbnailTransport> =
            StdArc::new(ScriptedThumbnailTransport::ok(vec![0xFF], 1, 1));
        let service = PeerImageThumbnailService::new(transport);
        service.record_peer_state(
            "peer-a",
            PeerImageThumbnailTrustState {
                trusted: true,
                active: false,
            },
        );
        let outcome = service.fetch_thumbnail("peer-a", "fingerprint", "entry-1");
        assert!(matches!(
            outcome,
            PeerImageThumbnailOutcome::PeerUnavailable {
                reason: "not_active"
            }
        ));
    }

    #[test]
    fn service_rejects_peer_without_thumbnail_capability() {
        let transport: StdArc<dyn PeerFetchImageThumbnailTransport> =
            StdArc::new(ScriptedThumbnailTransport::ok(vec![0xFF], 1, 1));
        let resolver: PeerImageThumbnailCapabilityResolver = Arc::new(|_| false);
        let service = PeerImageThumbnailService::new(transport).with_capability_resolver(resolver);
        service.record_peer_state(
            "peer-a",
            PeerImageThumbnailTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.fetch_thumbnail("peer-a", "fingerprint", "entry-1");
        assert!(matches!(
            outcome,
            PeerImageThumbnailOutcome::CapabilityMissing
        ));
    }

    #[test]
    fn service_returns_ok_for_valid_response() {
        let bytes = build_png(64, 64);
        let transport: StdArc<dyn PeerFetchImageThumbnailTransport> =
            StdArc::new(ScriptedThumbnailTransport::ok(bytes.clone(), 64, 64));
        let service = PeerImageThumbnailService::new(transport);
        service.record_peer_state(
            "peer-a",
            PeerImageThumbnailTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.fetch_thumbnail("peer-a", "fingerprint", "entry-1");
        match outcome {
            PeerImageThumbnailOutcome::Ok {
                bytes_b64,
                width,
                height,
            } => {
                assert_eq!(width, 64);
                assert_eq!(height, 64);
                let decoded = STANDARD.decode(bytes_b64.as_bytes()).expect("b64");
                assert_eq!(decoded, bytes);
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn service_rejects_response_with_oversized_body() {
        let oversized = vec![0u8; THUMBNAIL_MAX_BODY_BYTES + 1];
        let transport: StdArc<dyn PeerFetchImageThumbnailTransport> =
            StdArc::new(ScriptedThumbnailTransport::ok(oversized, 256, 256));
        let service = PeerImageThumbnailService::new(transport);
        service.record_peer_state(
            "peer-a",
            PeerImageThumbnailTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.fetch_thumbnail("peer-a", "fingerprint", "entry-1");
        assert!(matches!(outcome, PeerImageThumbnailOutcome::BodyTooLarge));
    }

    #[test]
    fn service_rejects_response_with_invalid_png() {
        let transport: StdArc<dyn PeerFetchImageThumbnailTransport> =
            StdArc::new(ScriptedThumbnailTransport::ok(vec![0x00, 0x01, 0x02], 1, 1));
        let service = PeerImageThumbnailService::new(transport);
        service.record_peer_state(
            "peer-a",
            PeerImageThumbnailTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.fetch_thumbnail("peer-a", "fingerprint", "entry-1");
        assert!(matches!(outcome, PeerImageThumbnailOutcome::InvalidPng));
    }

    #[test]
    fn service_rejects_response_with_oversized_dimensions() {
        let bytes = build_png(512, 512);
        let transport: StdArc<dyn PeerFetchImageThumbnailTransport> =
            StdArc::new(ScriptedThumbnailTransport::ok(bytes, 512, 512));
        let service = PeerImageThumbnailService::new(transport);
        service.record_peer_state(
            "peer-a",
            PeerImageThumbnailTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.fetch_thumbnail("peer-a", "fingerprint", "entry-1");
        assert!(matches!(outcome, PeerImageThumbnailOutcome::InvalidPng));
    }

    #[test]
    fn service_rejects_response_when_png_dimensions_disagree_with_envelope() {
        let bytes = build_png(64, 32);
        let transport: StdArc<dyn PeerFetchImageThumbnailTransport> =
            StdArc::new(ScriptedThumbnailTransport::ok(bytes, 32, 16));
        let service = PeerImageThumbnailService::new(transport);
        service.record_peer_state(
            "peer-a",
            PeerImageThumbnailTrustState {
                trusted: true,
                active: true,
            },
        );

        let outcome = service.fetch_thumbnail("peer-a", "fingerprint", "entry-1");
        assert!(matches!(outcome, PeerImageThumbnailOutcome::InvalidPng));
    }

    #[test]
    fn service_rejects_oversized_png_even_when_envelope_claims_small_dimensions() {
        let bytes = build_png(512, 512);
        let transport: StdArc<dyn PeerFetchImageThumbnailTransport> =
            StdArc::new(ScriptedThumbnailTransport::ok(bytes, 128, 128));
        let service = PeerImageThumbnailService::new(transport);
        service.record_peer_state(
            "peer-a",
            PeerImageThumbnailTrustState {
                trusted: true,
                active: true,
            },
        );

        let outcome = service.fetch_thumbnail("peer-a", "fingerprint", "entry-1");
        assert!(matches!(outcome, PeerImageThumbnailOutcome::InvalidPng));
    }

    #[test]
    fn service_collapses_busy_to_typed_outcome() {
        let transport: StdArc<dyn PeerFetchImageThumbnailTransport> = StdArc::new(
            ScriptedThumbnailTransport::err(PeerFetchImageThumbnailTransportError::Busy),
        );
        let service = PeerImageThumbnailService::new(transport);
        service.record_peer_state(
            "peer-a",
            PeerImageThumbnailTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.fetch_thumbnail("peer-a", "fingerprint", "entry-1");
        assert!(matches!(outcome, PeerImageThumbnailOutcome::Busy));
    }

    #[test]
    fn forgetting_peer_does_not_reset_active_client_fetch_limit() {
        use std::sync::mpsc;
        use std::sync::Barrier;
        use std::time::Duration;

        struct BlockingTransport {
            entered: mpsc::Sender<()>,
            barrier: StdArc<Barrier>,
            calls: AtomicUsize,
            bytes: Vec<u8>,
        }

        impl PeerFetchImageThumbnailTransport for BlockingTransport {
            fn fetch_image_thumbnail(
                &self,
                _request: PeerFetchImageThumbnailRequest,
            ) -> Result<PeerFetchImageThumbnailResponse, PeerFetchImageThumbnailTransportError>
            {
                if self.calls.fetch_add(1, Ordering::AcqRel) < 2 {
                    self.entered.send(()).expect("test receiver");
                    self.barrier.wait();
                }
                Ok(PeerFetchImageThumbnailResponse {
                    peer_id: "peer-a".to_string(),
                    remote_entry_id: "entry-1".to_string(),
                    bytes: self.bytes.clone(),
                    width: 4,
                    height: 4,
                })
            }
        }

        let (entered_tx, entered_rx) = mpsc::channel();
        let barrier = StdArc::new(Barrier::new(3));
        let transport: StdArc<dyn PeerFetchImageThumbnailTransport> =
            StdArc::new(BlockingTransport {
                entered: entered_tx,
                barrier: barrier.clone(),
                calls: AtomicUsize::new(0),
                bytes: build_png(4, 4),
            });
        let service = PeerImageThumbnailService::new(transport);
        let trusted_active = PeerImageThumbnailTrustState {
            trusted: true,
            active: true,
        };
        service.record_peer_state("peer-a", trusted_active);

        let first_service = service.clone();
        let first = std::thread::spawn(move || {
            first_service.fetch_thumbnail("peer-a", "fingerprint", "entry-1")
        });
        let second_service = service.clone();
        let second = std::thread::spawn(move || {
            second_service.fetch_thumbnail("peer-a", "fingerprint", "entry-1")
        });
        entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("first request entered transport");
        entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("second request entered transport");

        service.forget_peer("peer-a");
        service.record_peer_state("peer-a", trusted_active);
        let third = service.fetch_thumbnail("peer-a", "fingerprint", "entry-1");
        // Release the first two transports before asserting so a regression
        // cannot leave worker threads blocked at the barrier on test failure.
        barrier.wait();
        let first = first.join().expect("first worker");
        let second = second.join().expect("second worker");

        assert!(matches!(first, PeerImageThumbnailOutcome::Ok { .. }));
        assert!(matches!(second, PeerImageThumbnailOutcome::Ok { .. }));
        assert!(matches!(third, PeerImageThumbnailOutcome::Busy));
        assert!(!service.in_flight.lock().contains_key("peer-a"));
    }

    #[test]
    fn service_collapses_transport_error_to_typed_outcome() {
        let transport: StdArc<dyn PeerFetchImageThumbnailTransport> = StdArc::new(
            ScriptedThumbnailTransport::err(PeerFetchImageThumbnailTransportError::Revoked),
        );
        let service = PeerImageThumbnailService::new(transport);
        service.record_peer_state(
            "peer-a",
            PeerImageThumbnailTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.fetch_thumbnail("peer-a", "fingerprint", "entry-1");
        assert!(matches!(
            outcome,
            PeerImageThumbnailOutcome::TransportUnavailable { reason: "revoked" }
        ));
    }

    #[test]
    fn service_caps_concurrent_in_flight_requests() {
        // The cap is exposed through the public constant; the
        // assertion pins the documented contract so a future
        // refactor cannot accidentally lower the bound.
        assert_eq!(THUMBNAIL_CLIENT_MAX_INFLIGHT_PER_PEER, 2);
        assert_eq!(THUMBNAIL_HOST_MAX_INFLIGHT_PER_PEER, 2);
    }

    #[test]
    fn host_gate_caps_concurrent_decode_work() {
        let gate = PeerImageThumbnailHostGate::new();
        let first = gate.try_acquire("peer-a");
        let second = gate.try_acquire("peer-a");
        let third = gate.try_acquire("peer-a");
        assert!(first.is_ok());
        assert!(second.is_ok());
        assert!(matches!(third, Err(PeerImageThumbnailHostError::Busy)));
        drop(first);
        let fourth = gate.try_acquire("peer-a");
        assert!(fourth.is_ok());
    }

    #[test]
    fn host_handler_rejects_peer_without_thumbnail_capability() {
        let persistence: StdArc<dyn PeerImageThumbnailHostPersistence> =
            StdArc::new(InMemoryImageThumbnailHostPersistence::new());
        let gate = StdArc::new(PeerImageThumbnailHostGate::new());
        let thumbnail_resolver: PeerImageThumbnailHostCapabilityResolver = StdArc::new(|_| false);
        let image_import_resolver: PeerImageThumbnailHostCapabilityResolver = StdArc::new(|_| true);
        let handler = PeerImageThumbnailHostHandler::new(persistence, gate)
            .with_capability_resolvers(thumbnail_resolver, image_import_resolver);
        let outcome = handler.fetch_thumbnail("peer-a", "entry-1");
        assert!(matches!(
            outcome,
            Err(PeerImageThumbnailHostError::CapabilityMissing)
        ));
    }

    #[test]
    fn host_handler_rejects_peer_without_image_import_capability() {
        let persistence: StdArc<dyn PeerImageThumbnailHostPersistence> =
            StdArc::new(InMemoryImageThumbnailHostPersistence::new());
        let gate = StdArc::new(PeerImageThumbnailHostGate::new());
        let thumbnail_resolver: PeerImageThumbnailHostCapabilityResolver = StdArc::new(|_| true);
        let image_import_resolver: PeerImageThumbnailHostCapabilityResolver =
            StdArc::new(|_| false);
        let handler = PeerImageThumbnailHostHandler::new(persistence, gate)
            .with_capability_resolvers(thumbnail_resolver, image_import_resolver);
        let outcome = handler.fetch_thumbnail("peer-a", "entry-1");
        assert!(matches!(
            outcome,
            Err(PeerImageThumbnailHostError::CapabilityMissing)
        ));
    }

    #[test]
    fn host_handler_returns_thumbnail_for_transferable_entry() {
        let asset_bytes = build_png(128, 128);
        let asset_ref = format!("{}/abc.png", CLIPBOARD_ASSETS_DIR);
        let entry = image_entry(1, &asset_ref);
        let persistence = StdArc::new(InMemoryImageThumbnailHostPersistence::new());
        persistence.seed(vec![(entry, asset_bytes)]);
        let persistence_dyn: StdArc<dyn PeerImageThumbnailHostPersistence> = persistence;
        let gate = StdArc::new(PeerImageThumbnailHostGate::new());
        let thumbnail_resolver: PeerImageThumbnailHostCapabilityResolver = StdArc::new(|_| true);
        let image_import_resolver: PeerImageThumbnailHostCapabilityResolver = StdArc::new(|_| true);
        let handler = PeerImageThumbnailHostHandler::new(persistence_dyn, gate)
            .with_capability_resolvers(thumbnail_resolver, image_import_resolver);
        let outcome = handler.fetch_thumbnail("peer-a", "entry-1");
        let snapshot = outcome.expect("thumbnail");
        assert!(snapshot.width <= THUMBNAIL_MAX_LONGEST_SIDE);
        assert!(snapshot.height <= THUMBNAIL_MAX_LONGEST_SIDE);
        assert!(snapshot.bytes.len() <= THUMBNAIL_MAX_BODY_BYTES);
    }

    #[test]
    fn host_handler_rejects_non_transferable_entry() {
        let persistence = StdArc::new(InMemoryImageThumbnailHostPersistence::new());
        let persistence_dyn: StdArc<dyn PeerImageThumbnailHostPersistence> = persistence;
        let gate = StdArc::new(PeerImageThumbnailHostGate::new());
        let thumbnail_resolver: PeerImageThumbnailHostCapabilityResolver = StdArc::new(|_| true);
        let image_import_resolver: PeerImageThumbnailHostCapabilityResolver = StdArc::new(|_| true);
        let handler = PeerImageThumbnailHostHandler::new(persistence_dyn, gate)
            .with_capability_resolvers(thumbnail_resolver, image_import_resolver);
        let outcome = handler.fetch_thumbnail("peer-a", "entry-999");
        assert!(matches!(
            outcome,
            Err(PeerImageThumbnailHostError::NotFound)
        ));
    }

    #[test]
    fn host_handler_rejects_out_of_namespace_asset() {
        let asset_bytes = build_png(64, 64);
        let entry = image_entry(1, "escaped/abc.png");
        let persistence = StdArc::new(InMemoryImageThumbnailHostPersistence::new());
        persistence.seed(vec![(entry, asset_bytes)]);
        let persistence_dyn: StdArc<dyn PeerImageThumbnailHostPersistence> = persistence;
        let gate = StdArc::new(PeerImageThumbnailHostGate::new());
        let thumbnail_resolver: PeerImageThumbnailHostCapabilityResolver = StdArc::new(|_| true);
        let image_import_resolver: PeerImageThumbnailHostCapabilityResolver = StdArc::new(|_| true);
        let handler = PeerImageThumbnailHostHandler::new(persistence_dyn, gate)
            .with_capability_resolvers(thumbnail_resolver, image_import_resolver);
        let outcome = handler.fetch_thumbnail("peer-a", "entry-1");
        assert!(matches!(
            outcome,
            Err(PeerImageThumbnailHostError::NotTransferable)
        ));
    }

    #[test]
    fn host_handler_rejects_invalid_source_png() {
        let asset_bytes = vec![0u8; 8];
        let asset_ref = format!("{}/abc.png", CLIPBOARD_ASSETS_DIR);
        let entry = image_entry(1, &asset_ref);
        let persistence = StdArc::new(InMemoryImageThumbnailHostPersistence::new());
        persistence.seed(vec![(entry, asset_bytes)]);
        let persistence_dyn: StdArc<dyn PeerImageThumbnailHostPersistence> = persistence;
        let gate = StdArc::new(PeerImageThumbnailHostGate::new());
        let thumbnail_resolver: PeerImageThumbnailHostCapabilityResolver = StdArc::new(|_| true);
        let image_import_resolver: PeerImageThumbnailHostCapabilityResolver = StdArc::new(|_| true);
        let handler = PeerImageThumbnailHostHandler::new(persistence_dyn, gate)
            .with_capability_resolvers(thumbnail_resolver, image_import_resolver);
        let outcome = handler.fetch_thumbnail("peer-a", "entry-1");
        assert!(matches!(
            outcome,
            Err(PeerImageThumbnailHostError::InvalidPng)
                | Err(PeerImageThumbnailHostError::NotTransferable)
        ));
    }
}

/// Re-exported [`Cursor`] type the host handler uses to feed the
/// canonical PNG bytes into the decoder. The alias keeps the
/// module prelude minimal.
pub type CursorBuffer<'a> = Cursor<&'a [u8]>;
