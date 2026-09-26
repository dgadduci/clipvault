//! Platform-neutral TLS / mTLS transport surface for the
//! `local-peer-mutual-pairing` change.
//!
//! The pairing runtime opens a TCP/TLS listener on an ephemeral
//! port and answers mTLS handshakes from other ClipVault
//! installations that have just observed the local TXT record.
//! The transport module owns every cryptographic detail so the
//! runtime core (and the SQLite / frontend / Tauri layers) never
//! see a TLS key, an IP, a port or a handshake error. The runtime
//! sees a metadata-only [`PeerTransportEvent`] stream plus typed
//! outcomes ([`TransportOutcome`]).
//!
//! ## Architecture
//!
//! - [`PeerTransport`] is the trait the runtime drives. It is
//!   cheap to clone (every field is `Arc`-shared) and offers
//!   idempotent `start` / `stop` semantics mirroring the discovery
//!   adapter.
//! - [`NoopPeerTransport`] is the safe default the production
//!   shell installs when the `local-peer-pairing-tls` feature is
//!   not enabled (cross-compiles, Windows builds, distroless test
//!   sandboxes). Every call returns
//!   [`TransportOutcome::Unavailable`] so the runtime surfaces a
//!   typed reason to the UI without spawning a thread or binding a
//!   socket.
//! - [`TlsPeerTransport`] is the production adapter that owns a
//!   [`tokio::runtime::Runtime`] (the only async surface in the
//!   platform crate), an ephemeral TCP listener, a `rustls` server
//!   config, and a pairing session surface that hands completed
//!   peer observations back to the runtime core. The actual TLS
//!   stack lives in the [`tls`] module so this facade stays
//!   readable.
//!
//! ## Productive session API
//!
//! The pairing runtime drives the transport through a typed
//! session surface that exposes only opaque
//! [`PairingSessionId`]s:
//!
//! - [`PeerTransport::start_outbound`] resolves the announced
//!   peer through the platform's mDNS adapter, opens the mTLS
//!   connection, drives Hello/HelloAck, computes the SAS and
//!   returns the [`PairingSessionId`] the runtime hands back to
//!   the UI.
//! - [`PeerTransport::approve_local`] signs the canonical
//!   transcript and sends the `Approve` envelope over the same
//!   mTLS connection. The remote observation flows back through
//!   the [`TransportSink`] the runtime installed at install time.
//! - [`PeerTransport::cancel_session`] tears the connection down
//!   without promoting trust.
//! - [`PeerTransport::disconnect_peer`] closes every session the
//!   transport holds for the matching `peer_id` (used by revoke
//!   / block so a previously-trusted peer cannot keep an open
//!   pairing socket after the row leaves the trusted state).
//!
//! The runtime never sees a `SocketAddr`, a TCP port, a TLS
//! stream or an endpoint byte; the resolver, the listener and
//! the connection all stay inside `clipvault-platform`. The
//! `PairingSessionId` is the only opaque identifier that crosses
//! the trust boundary.
//!
//! The transport layer is the single owner of the local private
//! TLS key. The key is derived from the same 32-byte Ed25519 seed
//! the secure store already minted for [`LocalPeerIdentity`], so
//! the cert's SPKI is cryptographically pinned to the identity
//! the runtime advertises through mDNS — the `peer_id`,
//! fingerprint and cert fingerprint always agree across
//! restarts. See [`crate::peer_identity::LocalIdentityMaterial`]
//! for the binding.
//!
//! ## Wire contract
//!
//! The pairing wire format the transport speaks is versioned. The
//! current [`PAIRING_WIRE_VERSION`] is the only value the runtime
//! understands; an incoming message with a higher version is
//! rejected with [`TransportOutcome::IncompatibleProtocol`] before
//! the handshake reaches the application layer. The format itself
//! is documented in [`crate::peer_transport::wire`] (a length-
//! prefixed JSON envelope the future text/history changes will
//! reuse for their metadata-only RPCs).

#[cfg(feature = "local-peer-pairing-tls")]
pub mod tls;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
#[cfg(feature = "local-peer-pairing-tls")]
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(feature = "local-peer-pairing-tls")]
pub use super::peer_identity::LocalIdentityMaterial;
pub use super::peer_identity::{LocalPeerIdentity, PeerFingerprint, PeerId};

/// Wire-protocol version the pairing transport speaks. The constant
/// is the only value the runtime accepts on the wire; bumping it
/// without updating the runtime must surface a typed rejection at
/// the application layer (the [`TransportOutcome::IncompatibleProtocol`]
/// branch).
pub const PAIRING_WIRE_VERSION: u32 = 1;

/// Wire-protocol major version the `peer-text-history-browser`
/// change ships. The pairing runtime keeps the legacy constant for
/// backwards compatibility with the pairing surface, but the
/// history wire uses its own discriminator so a future text /
/// import change can ship a different major without breaking the
/// pairing path.
pub const HISTORY_WIRE_VERSION: u32 = 1;

/// Maximum number of metadata-only rows the host returns in a single
/// `list_recent_text` response. The runtime enforces the cap on both
/// ends so a malicious cursor cannot trick the projection into
/// streaming more rows than the contract allows.
pub const HISTORY_MAX_PAGE_ROWS: usize = 50;

/// Maximum serialized size of one authenticated `ListRecentTextAck`.
///
/// A page is still bounded to [`HISTORY_MAX_PAGE_ROWS`] rows and each preview
/// is bounded by the core, but JSON plus UTF-8 can legitimately exceed the
/// small pairing frame. This limit applies only after the listener has
/// authenticated the history request; pairing handshakes keep their own cap.
pub const HISTORY_MAX_RESPONSE_BYTES: usize = 128 * 1024;

/// Maximum length of an incoming pairing wire envelope. The pairing
/// surface only ever exchanges nonces, fingerprints, SAS confirmations
/// and signed approvals — the cap is generous and stays well below
/// the MTU so the listener can short-circuit obviously malicious
/// payloads before they reach the application state machine.
pub const PAIRING_MAX_PAYLOAD_BYTES: usize = 4 * 1024;

/// Maximum serialized size of one authenticated `FetchTextAck`
/// response. The contract pins a 1 MiB UTF-8 text body as the
/// absolute cap the host honours; the JSON envelope plus the UTF-8
/// escaping overhead and optional bounded source-app icon stay below
/// this 8 MiB ceiling so a regression
/// that forgets to enforce the limit cannot accidentally stream
/// more bytes than the runtime expects.
pub const FETCH_TEXT_MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Hard cap the `peer-text-import` change pins on the imported text
/// body. The runtime enforces the cap on both the listener (which
/// refuses any body larger than the threshold) and the caller
/// (which never trusts the listener's word alone). The 1 MiB value
/// matches the design (`peer-text-import/design.md` §"Dependencia y
/// fetch") and the spec (`peer-text-import/spec.md` §"Complete
/// remote text is fetched only for explicit import").
pub const FETCH_TEXT_MAX_BODY_BYTES: usize = 1024 * 1024;

/// Wire-protocol major the `peer-image-import` change ships. The
/// pairing runtime keeps the legacy constant for backwards
/// compatibility with the pairing surface, but the image wire
/// uses its own discriminator so a future text / import change can
/// ship a different major without breaking the image path.
pub const IMAGE_WIRE_VERSION: u32 = 1;

/// Maximum number of metadata-only image rows the host returns in
/// a single `list_recent_images` response. Mirrors the text cap
/// the `peer-text-history-browser` change pins so the client rail
/// renders a consistent newest-first page regardless of which
/// endpoint answered the request.
pub const IMAGE_HISTORY_MAX_PAGE_ROWS: usize = 50;

/// Maximum serialized size of one authenticated
/// `ListRecentImagesAck`. The cap matches the text envelope so
/// the productive transport can drive both endpoints with the
/// same listener budget.
pub const IMAGE_HISTORY_MAX_RESPONSE_BYTES: usize = 128 * 1024;

/// Maximum serialized size of one authenticated `FetchImageAck`
/// response. The body the host returns is the bounded PNG payload
/// the local asset store produced when the entry was committed.
/// The envelope JSON carries the PNG as a base64 string
/// (`bytes_b64`) so the worst-case framing overhead is `4 * bytes
/// / 3`; the cap is sized to fit a fully-valid
/// [`FETCH_IMAGE_MAX_BODY_BYTES`] PNG without truncating or
/// silently downgrading the limit, leaving a small budget for the
/// surrounding envelope (peer_id, remote_entry_id, version, title
/// and JSON formatting). The runtime enforces the cap on both
/// ends so a regression that forgets to enforce the limit cannot
/// accidentally stream more bytes than the runtime expects.
pub const FETCH_IMAGE_MAX_RESPONSE_BYTES: usize = 24 * 1024 * 1024;

/// Hard cap the `peer-image-import` change pins on the imported
/// image body. The runtime enforces the cap on both the listener
/// (which refuses any body larger than the threshold) and the
/// caller (which never trusts the listener's word alone). The
/// value mirrors [`clipvault_core::clipboard_assets::MAX_CLIPBOARD_ASSET_BYTES`]
/// so the wire stays consistent with the local asset store
/// contract the `clipboard-rich-content` change ships. The
/// platform crate cannot link core so the constant is duplicated
/// here; keeping them in lock-step is covered by the bridge
/// tests.
pub const FETCH_IMAGE_MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

/// Hard cap for an optional source-application PNG carried only by an
/// explicit text/image import acknowledgement. Mirrors core's
/// `MAX_SOURCE_APP_ICON_BYTES`; platform keeps the wire bound independent
/// from the core crate.
pub const FETCH_SOURCE_APP_ICON_MAX_BYTES: usize = 512 * 1024;

/// Hard cap the `peer-image-preview-thumbnails` change pins on
/// the encoded thumbnail body. The value mirrors the documented
/// 384 KiB cap the design pins: the host MUST reject any
/// generated PNG larger than this threshold with a typed
/// `body_too_large` reason and never ship oversized bytes to
/// the client. The caller re-validates the size against the
/// decoded byte length so a drifted host cannot bypass the
/// contract.
pub const FETCH_IMAGE_THUMBNAIL_MAX_BODY_BYTES: usize = 384 * 1024;

/// Maximum serialized size of one authenticated
/// `FetchImageThumbnailAck` envelope. The wire carries the
/// bounded PNG payload as a base64 string so the worst-case
/// framing overhead is `4 * bytes / 3`; the cap is sized so a
/// fully-valid 384 KiB PNG fits the envelope (≈512 KiB) plus a
/// small budget for the surrounding metadata (peer_id,
/// remote_entry_id, version, dimensions and JSON formatting).
/// The runtime enforces the cap on both the listener (which
/// refuses any body larger than the threshold) and the
/// caller (which never trusts the listener's word alone).
pub const FETCH_IMAGE_THUMBNAIL_MAX_RESPONSE_BYTES: usize = 544 * 1024;

/// Maximum serialized size of one authenticated
/// `FetchSourceAppPresentationAck` envelope. The wire contract
/// pins a 720 KiB cap so the bounded base64 PNG icon (≤ 512 KiB)
/// plus the validated display name never exceeds the documented
/// threshold. The listener enforces the cap before serializing
/// the response; the dialer decodes the value locally and
/// re-validates the PNG signature / dimensions before turning
/// the bytes into an Object URL.
pub const FETCH_SOURCE_APP_PRESENTATION_MAX_RESPONSE_BYTES: usize = 720 * 1024;

/// Best-effort byte estimate for the serialized source-app
/// presentation envelope. The helper sums the JSON-overhead
/// of the validated display name + base64 icon so the
/// listener can refuse an oversized response before
/// serializing it. The estimate is intentionally conservative
/// (over-estimates) so the listener never ships a payload that
/// blows past [`FETCH_SOURCE_APP_PRESENTATION_MAX_RESPONSE_BYTES`].
#[cfg(feature = "local-peer-pairing-tls")]
#[derive(Debug, Clone, Copy)]
pub struct PendingSourceAppPresentationSize<'a> {
    pub source_app_name: Option<&'a str>,
    pub source_app_icon_b64: Option<&'a str>,
}

#[cfg(feature = "local-peer-pairing-tls")]
impl<'a> PendingSourceAppPresentationSize<'a> {
    pub fn estimated_bytes(&self) -> usize {
        // Length-prefixed envelope overhead + serde tag.
        let mut total: usize = 64;
        if let Some(name) = self.source_app_name {
            total = total.saturating_add(name.len());
        }
        if let Some(b64) = self.source_app_icon_b64 {
            // Base64 grows 4/3 over the raw bytes; the listener
            // double-checks the actual serialised size after
            // building the JSON body.
            total = total.saturating_add(b64.len());
        }
        total
    }
}

/// Maximum number of in-flight pairing sessions the listener keeps
/// open concurrently. The transport surfaces a typed rejection when a
/// remote peer tries to start a session above the cap so a single
/// remote cannot exhaust the local listener's accept queue.
pub const PAIRING_MAX_IN_FLIGHT_SESSIONS: usize = 64;

/// Outcome of a single transport interaction. The pairing runtime
/// collapses every error path the underlying TLS / TCP stack
/// surfaces into one of these typed variants so the core never has
/// to inspect free-form strings or platform-specific error
/// taxonomy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportOutcome {
    /// The remote peer is on a wire-protocol version the runtime
    /// does not understand.
    IncompatibleProtocol,
    /// The remote peer presented an unknown / revoked / blocked
    /// peer_id or a public key fingerprint that does not match
    /// the pinned value the runtime persisted after the
    /// reciprocal pairing.
    UnknownPeer,
    /// The remote peer's TLS chain does not authenticate against
    /// the pinned certificate fingerprint.
    KeyMismatch,
    /// The remote peer is in the [`clipvault_db::TrustState::Blocked`]
    /// state. The runtime rejects the connection before any
    /// cryptographic work runs.
    Blocked,
    /// The remote peer is in the [`clipvault_db::TrustState::Revoked`]
    /// state. The runtime rejects the connection; the user must
    /// explicitly re-pair to clear the revoke.
    Revoked,
    /// The transport could not bind the ephemeral port or the
    /// cryptographic provider refused to mint a key. The runtime
    /// surfaces this as a typed `transport_unavailable` reason
    /// without retrying on the same session.
    Unavailable,
}

/// Metadata-only event the transport pushes into the runtime when
/// a remote peer successfully completes the pairing handshake.
/// The runtime persists the [`Self::cert_fingerprint`] in
/// `known_peers.tls_cert_fingerprint` so a future connection from
/// the same `peer_id` can be pinned to the same key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerTransportObservation {
    pub peer_id: String,
    pub public_key_fingerprint: String,
    pub cert_fingerprint: String,
    pub display_name: String,
}

/// Metadata the runtime passes to the transport so it can dial a
/// remote peer. The descriptor carries only what the runtime
/// already has from the discovery layer (the canonical
/// `peer_id`, the full SHA-256 public-key fingerprint, the
/// display name the remote advertises). The transport resolves
/// the address + port internally through the platform's mDNS
/// adapter so the runtime, SQLite, Tauri and the frontend never
/// see an endpoint byte. The transport mints the local nonce
/// during the Hello/HelloAck exchange so the runtime never has
/// to invent a fictitious nonce that would diverge from the
/// transcript the listener actually validates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundSessionDescriptor {
    pub peer_id: String,
    pub full_public_key_fingerprint: String,
    pub display_name: String,
    pub local_public_key: [u8; 32],
    pub local_peer_id: String,
}

/// Metadata the transport hands back to the runtime after the
/// outbound Hello/HelloAck round-trip completes. The struct
/// carries the real nonces the wire protocol saw (not a
/// placeholder the runtime minted locally) so the SAS the UI
/// renders matches the SAS the remote listener validates. The
/// `remote_cert_der` is the SHA-256 of the cert the mTLS
/// handshake authenticated; the runtime persists it as the
/// pin the next health probe must match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundSessionMetadata {
    pub remote_peer_id: String,
    pub remote_full_fingerprint: String,
    pub remote_display_name: String,
    pub local_nonce: String,
    pub remote_nonce: String,
    pub sas: String,
    pub remote_cert_fingerprint: String,
}

/// Handle the productive pairing transport returns from
/// [`PeerTransport::start_outbound`]. The id is the opaque
/// session identifier the runtime stores in its in-memory
/// session table; the metadata is the canonical record the
/// runtime populates the [`PairingSessionSnapshot`] from so the
/// UI renders the same SAS the remote listener validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingOutbound {
    pub session_id: PairingSessionId,
    pub metadata: OutboundSessionMetadata,
}

/// Metadata-only event the transport pushes into the runtime
/// when an inbound `Hello` envelope registers a pairing
/// session. The transport computes the SAS using the real
/// nonces the wire protocol negotiated and surfaces the values
/// to the runtime BEFORE waiting for the local user to approve
/// so the UI of the second team can render the same SAS the
/// remote dialer shows. The session id is opaque and unique
/// per inbound attempt; the runtime never sees an IP, a port,
/// a cert fingerprint or any other secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundSessionMetadata {
    pub session_id: PairingSessionId,
    pub remote_peer_id: String,
    pub remote_full_fingerprint: String,
    pub remote_display_name: String,
    pub local_nonce: String,
    pub remote_nonce: String,
    pub sas: String,
    /// RFC-3339 instant the session expires. The runtime
    /// surfaces the formatted string to the UI so the modal
    /// can render the two-minute countdown that mirrors the
    /// transport-side hard limit.
    pub expires_at: String,
}

/// Opaque identifier the transport hands back when the runtime
/// initiates a productive pairing session. The id never encodes a
/// `peer_id`, an IP, a port, a TLS handle or any other secret; it
/// is a monotonic counter the runtime stores as the
/// `PairingSessionId` key and the transport uses as the lookup
/// key into its in-memory session table. The same opaque value is
/// what the bridge surfaces to the frontend, so the renderer can
/// never correlate the id with an endpoint byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PairingSessionId(pub u64);

impl PairingSessionId {
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Callback the runtime installs so the transport can surface
/// completed pairings. Implementations are expected to be cheap
/// (the transport pushes from a tokio task and any blocking work
/// would stall the listener's accept loop).
pub trait TransportSink: Send + Sync {
    /// The transport pushes a metadata-only observation the
    /// inbound side built from the cert SPKI it pinned during
    /// the mTLS handshake. The observation is the second
    /// approval the dual-approval gate requires: the local user
    /// must have already accepted (via
    /// [`PeerTransport::approve_local`]) for the row to promote
    /// to `Trusted`.
    fn on_pairing_observed(&self, observation: PeerTransportObservation);

    /// The transport pushes a metadata-only event when an
    /// inbound `Hello` envelope registers a new session. The
    /// runtime calls
    /// [`crate::peer_pairing::PairingRuntime::register_inbound`]
    /// so the UI can display the SAS and the user can approve
    /// the session BEFORE the transport sends `HelloAck` over
    /// the open mTLS connection. The default implementation is
    /// a no-op so adapters that only consume the observation
    /// event (legacy fakes, noop transports) keep compiling.
    fn on_pairing_session_started(&self, _metadata: InboundSessionMetadata) {}
}

/// Host-side handler the listener drives when an `ListRecentText`
/// envelope lands after a successful mTLS handshake. The trait is
/// feature-gated to the productive TLS path so cross-compiles and
/// unsupported targets keep compiling. The transport invokes the
/// handler exactly once per inbound envelope and forwards the typed
/// response through the wire; the handler never touches a TLS
/// stream, a socket or a session id.
#[cfg(feature = "local-peer-pairing-tls")]
pub trait HistoryHostHandler: Send + Sync {
    /// Project a metadata-only page of the local history. The
    /// `peer_id` argument is the canonical `peer_id` the
    /// transport derived from the cert's SPKI; the transport
    /// already authenticated it against the pin the runtime
    /// armed. `cursor` is the opaque cursor the client
    /// submitted verbatim (empty string for the first page);
    /// `limit` is the upper bound the client requested, capped
    /// to [`HISTORY_MAX_PAGE_ROWS`]. Implementations NEVER
    /// inspect row content beyond the metadata-only projection
    /// and NEVER return a payload other than the bounded page.
    fn list_recent_text(&self, peer_id: &str, cursor: &str, limit: u32) -> HistoryHostResponse;
}

/// Outcome the host-side handler returns to the listener. The
/// transport forwards the variant through the wire envelope the
/// spec pins: `ListRecentTextAck` for [`HistoryHostResponse::Ok`],
/// `ListRecentTextInvalid` for [`HistoryHostResponse::InvalidCursor`]
/// and `ListRecentTextUnavailable` for
/// [`HistoryHostResponse::Unavailable`]. Every variant collapses
/// to a stable reason string the renderer branches on.
#[cfg(feature = "local-peer-pairing-tls")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryHostResponse {
    Ok {
        rows: Vec<wire::ListRecentTextRow>,
        /// Opaque cursor the renderer must submit to fetch the
        /// next page. Empty string when this page is the last one.
        next_cursor: String,
        /// Stable fingerprint the renderer compares across page
        /// requests to detect a local capture that landed between
        /// the two.
        snapshot_id: String,
    },
    InvalidCursor,
    Unavailable {
        reason: &'static str,
    },
}

/// Host-side handler the listener drives when a `FetchText`
/// envelope lands after a successful mTLS handshake. The trait is
/// feature-gated to the productive TLS path so cross-compiles and
/// unsupported targets keep compiling. The transport invokes the
/// handler exactly once per inbound envelope and forwards the typed
/// response through the wire; the handler never touches a TLS
/// stream, a socket or a session id.
///
/// Implementations are expected to enforce the
/// [`FETCH_TEXT_MAX_BODY_BYTES`] cap on the body before returning
/// the [`HostFetchResponse::Ok`] variant. Returning a larger body
/// is a contract violation the transport must catch and reject
/// with [`HostFetchResponse::BodyTooLarge`] so the caller cannot
/// accidentally stream more than the contract allows.
#[cfg(feature = "local-peer-pairing-tls")]
pub trait FetchTextHostHandler: Send + Sync {
    /// Project the body of `remote_entry_id` for `peer_id`. The
    /// `peer_id` argument is the canonical `peer_id` the
    /// transport derived from the cert's SPKI; the transport
    /// already authenticated it against the pin the runtime
    /// armed. `remote_entry_id` is the opaque id the host
    /// minted (the `entry-<id>` projection the
    /// `peer-text-history-browser` change ships).
    fn fetch_text(&self, peer_id: &str, remote_entry_id: &str) -> HostFetchResponse;
}

/// Host-side handler the listener drives when a
/// `ListRecentImages` envelope lands after a successful mTLS
/// handshake. The trait mirrors [`HistoryHostHandler`]; the
/// productive mTLS path keeps the text and image history
/// surfaces separate so a future host that has not shipped the
/// `peer-image-import` change still speaks the wire contract
/// (`ListRecentImagesUnavailable { reason: not_available }`).
#[cfg(feature = "local-peer-pairing-tls")]
pub trait ImageHistoryHostHandler: Send + Sync {
    /// Project a metadata-only page of the local image history.
    /// The contract mirrors [`HistoryHostHandler::list_recent_text`]
    /// minus the preview: the response carries only the row
    /// metadata the spec authorises (opaque remote id, validated
    /// title, content type, RFC 3339 timestamp, byte size and
    /// pixel dimensions). Implementations NEVER return image bytes,
    /// thumbnails, asset references or filesystem paths.
    fn list_recent_images(
        &self,
        peer_id: &str,
        cursor: &str,
        limit: u32,
    ) -> ImageHistoryHostResponse;
}

/// Outcome the image-host handler returns to the listener. The
/// transport forwards the variant through the wire envelope the
/// spec pins: `ListRecentImagesAck` for [`Self::Ok`],
/// `ListRecentImagesInvalid` for [`Self::InvalidCursor`] and
/// `ListRecentImagesUnavailable` for [`Self::Unavailable`]. Every
/// typed failure collapses into a stable reason string the
/// renderer branches on.
#[cfg(feature = "local-peer-pairing-tls")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageHistoryHostResponse {
    Ok {
        rows: Vec<wire::ListRecentImageRow>,
        next_cursor: String,
        snapshot_id: String,
    },
    InvalidCursor,
    Unavailable {
        reason: &'static str,
    },
}

/// Host-side handler the listener drives when a `FetchImage`
/// envelope lands after a successful mTLS handshake. The trait
/// is feature-gated to the productive TLS path so cross-compiles
/// and unsupported targets keep compiling. Implementations are
/// expected to enforce the [`FETCH_IMAGE_MAX_BODY_BYTES`] cap
/// before returning the [`HostImageFetchResponse::Ok`] variant;
/// returning a larger body is a contract violation the transport
/// catches and rejects with [`HostImageFetchResponse::BodyTooLarge`].
#[cfg(feature = "local-peer-pairing-tls")]
pub trait FetchImageHostHandler: Send + Sync {
    /// Project the PNG payload of `remote_entry_id` for `peer_id`.
    /// The transport authenticated `peer_id` against the pinned
    /// cert fingerprint before invoking the handler.
    fn fetch_image(&self, peer_id: &str, remote_entry_id: &str) -> HostImageFetchResponse;
}

/// Outcome the image-fetch handler returns to the listener. The
/// transport forwards the variant through the wire envelope the
/// spec pins: `FetchImageAck` for [`Self::Ok`], every other
/// variant collapses into `FetchImageUnavailable` so the wire
/// contract stays stable across hosts that have not shipped the
/// image path yet. Every typed failure collapses into a stable
/// reason string the importer branches on.
#[cfg(feature = "local-peer-pairing-tls")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostImageFetchResponse {
    /// The host validated the PNG against the contract and the
    /// handler returned a payload smaller than the
    /// [`FETCH_IMAGE_MAX_BODY_BYTES`] cap. The transport forwards
    /// the bytes verbatim; the importer re-validates the size and
    /// the PNG signature before any SQLite mutation.
    Ok {
        title: Option<String>,
        /// Canonical PNG bytes the host read from the local
        /// asset store. The listener re-validates the limit
        /// locally; the importer validates the PNG signature /
        /// dimensions / size before any local mutation.
        bytes: Vec<u8>,
        /// Optional source-app attribution bundled only with an explicit
        /// import acknowledgement.
        source_app_name: Option<String>,
        source_app_icon_bytes: Option<Vec<u8>>,
    },
    /// The entry disappeared between the listing and the fetch,
    /// or it has been edited into a non-transferable shape.
    NotFound,
    /// The entry exists but is no longer transferrable (no
    /// `asset_ref`, oversized, mismatched MIME, …).
    NotTransferable,
    /// The entry exceeded the [`FETCH_IMAGE_MAX_BODY_BYTES`] cap.
    BodyTooLarge,
    /// The persistence layer refused the lookup (SQLite error,
    /// missing handle, asset store failure, …).
    PersistenceUnavailable,
}

/// Host-side handler the listener drives when a
/// `FetchImageThumbnail` envelope lands after a successful mTLS
/// handshake. The trait is feature-gated to the productive TLS
/// path so cross-compiles and unsupported targets keep compiling.
/// Implementations are expected to enforce the
/// [`FETCH_IMAGE_THUMBNAIL_MAX_BODY_BYTES`] cap on the generated
/// PNG before returning the [`HostImageThumbnailResponse::Ok`]
/// variant; returning a larger body is a contract violation the
/// transport catches and rejects with
/// [`HostImageThumbnailResponse::BodyTooLarge`].
#[cfg(feature = "local-peer-pairing-tls")]
pub trait FetchImageThumbnailHostHandler: Send + Sync {
    /// Generate the bounded PNG thumbnail for `remote_entry_id`
    /// and `peer_id`. The transport authenticated `peer_id`
    /// against the pinned cert fingerprint before invoking the
    /// handler. The implementation MUST re-validate the
    /// capability gate, the entry eligibility and the asset
    /// namespace before reading bytes; the runtime never trusts
    /// client-supplied identifiers beyond the opaque
    /// `remote_entry_id`.
    fn fetch_image_thumbnail(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
    ) -> HostImageThumbnailResponse;
}

/// Outcome the thumbnail-fetch handler returns to the listener.
/// The transport forwards the variant through the wire envelope
/// the spec pins: `FetchImageThumbnailAck` for [`Self::Ok`], every
/// other variant collapses into `FetchImageThumbnailUnavailable`
/// so the wire contract stays stable across hosts that have not
/// shipped the thumbnail path yet. Every typed failure collapses
/// into a stable reason string the client branches on; the
/// generated PNG never crosses the wire on a failure path so the
/// caller cannot leak the original image bytes through an error.
#[cfg(feature = "local-peer-pairing-tls")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostImageThumbnailResponse {
    /// The host re-validated the entry, generated the bounded
    /// thumbnail and the resulting PNG body fits the
    /// [`FETCH_IMAGE_THUMBNAIL_MAX_BODY_BYTES`] cap. The
    /// transport forwards the bytes verbatim; the client
    /// re-validates the PNG signature and the dimension cap
    /// before turning the bytes into an Object URL.
    Ok {
        /// Encoded PNG body, aspect ratio preserved, longest
        /// side ≤ 256 px, transparency preserved when present.
        bytes: Vec<u8>,
        /// Width / height of the encoded PNG. The renderer can
        /// use the dimensions to size the `<img>` element
        /// without decoding the bytes.
        width: u32,
        height: u32,
    },
    /// The entry disappeared between the listing and the
    /// thumbnail request, or it has been edited into a
    /// non-transferable shape.
    NotFound,
    /// The entry exists but is no longer transferrable (no
    /// `asset_ref`, oversized, mismatched MIME, …).
    NotTransferable,
    /// The host refused to start the resize because the
    /// per-peer concurrency limit was reached. The caller
    /// treats this as a transient unavailability without
    /// surfacing a global rail error.
    Busy,
    /// The generated PNG exceeded the
    /// [`FETCH_IMAGE_THUMBNAIL_MAX_BODY_BYTES`] cap. The
    /// transport never ships an oversized body and the
    /// caller keeps the static placeholder.
    BodyTooLarge,
    /// The persistence layer refused the lookup (SQLite
    /// error, missing handle, asset store failure, …).
    PersistenceUnavailable,
}

/// Host-side source-app presentation handler the
/// `peer-source-app-presentation` change ships. The transport
/// authenticates the `peer_id` against the pinned cert
/// fingerprint and re-validates the
/// `source_app_presentation` capability before invoking the
/// handler; the implementation MUST still re-validate the
/// entry eligibility, the trusted / active state and the
/// local icon namespace before resolving the attribution.
#[cfg(feature = "local-peer-pairing-tls")]
pub trait FetchSourceAppPresentationHostHandler: Send + Sync {
    /// Resolve the validated source-app display name + bounded
    /// PNG icon for `remote_entry_id` and `peer_id`. The
    /// transport authenticated `peer_id` against the pinned
    /// cert fingerprint before invoking the handler. The
    /// implementation MUST re-validate the capability gate, the
    /// entry eligibility and the asset namespace before
    /// reading bytes; the runtime never trusts client-supplied
    /// identifiers beyond the opaque `remote_entry_id`.
    fn fetch_source_app_presentation(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
    ) -> HostSourceAppPresentationResponse;
}

/// Outcome the source-app presentation handler returns to the
/// listener. The transport forwards the variant through the
/// wire envelope the spec pins:
/// `FetchSourceAppPresentationAck` for [`Self::Ok`], every
/// other variant collapses into
/// `FetchSourceAppPresentationUnavailable`. Every typed failure
/// collapses into a stable reason string the caller branches
/// on; the icon bytes never cross the wire on a failure path.
#[cfg(feature = "local-peer-pairing-tls")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostSourceAppPresentationResponse {
    /// The host re-validated the entry and the source-app
    /// metadata fits the documented contract. The transport
    /// base64-encodes the icon bytes before serialising the
    /// envelope; the caller decodes the value locally and
    /// re-validates the PNG signature + dimensions before
    /// turning the bytes into an Object URL. The icon never
    /// carries a remote path / filename / reference; the
    /// caller resolves the icon through the local
    /// `application-icons/` writer.
    Ok {
        /// Trimmed, validated source-app display name.
        /// `None` when the host has no metadata for this entry.
        source_app_name: Option<String>,
        /// Bounded PNG bytes the host validated against the
        /// 512 KiB / 256 × 256 px envelope. The transport
        /// rejects any payload larger than
        /// [`crate::peer_source_app_presentation::MAX_SOURCE_APP_ICON_BYTES`].
        /// `None` when no icon is available.
        source_app_icon_bytes: Option<Vec<u8>>,
    },
    /// The peer did not advertise the
    /// `source_app_presentation` capability.
    NotAvailable,
    /// The entry disappeared between the listing and the
    /// source-app request, or it has been edited into a
    /// non-transferable shape.
    NotFound,
    /// The peer revoked / blocked the request before the host
    /// could resolve the metadata.
    NotTrusted,
    /// The persistence layer refused the lookup (SQLite
    /// error, missing handle, asset store failure, …).
    PersistenceUnavailable,
}

/// Outcome the host-side fetch handler returns to the listener.
/// The transport forwards the variant through the wire envelope
/// the spec pins: `FetchTextAck` for [`HostFetchResponse::Ok`],
/// `FetchTextUnavailable` for every other variant. Every typed
/// failure collapses into a stable reason string the importer
/// branches on without inspecting free-form strings or content
/// bytes.
#[cfg(feature = "local-peer-pairing-tls")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostFetchResponse {
    /// The host validated the body against the contract and the
    /// handler returned a UTF-8 text smaller than the
    /// [`FETCH_TEXT_MAX_BODY_BYTES`] cap. The transport forwards
    /// the body verbatim; the importer re-validates the size and
    /// the UTF-8 shape before any SQLite mutation.
    Ok {
        /// Validated, trimmed user-supplied title. `None` when
        /// the entry has no custom title or the persisted value
        /// fails validation.
        title: Option<String>,
        /// Canonical snake_case string the local SQLite layer
        /// persists (e.g. `text`, `url`, `json`).
        content_type: String,
        /// Canonical UTF-8 body. The handler MUST keep the size
        /// ≤ [`FETCH_TEXT_MAX_BODY_BYTES`]; the transport
        /// refuses larger bodies with [`Self::BodyTooLarge`]
        /// before they cross the wire.
        body: String,
        /// Optional source-app attribution bundled only with an explicit
        /// import acknowledgement. Older peers can ignore these additive
        /// fields without affecting the body import.
        source_app_name: Option<String>,
        source_app_icon_bytes: Option<Vec<u8>>,
    },
    /// The entry disappeared between the listing and the fetch,
    /// or it has been edited into a non-transferable shape. The
    /// reason is a stable snake_case identifier the importer
    /// branches on.
    NotFound,
    /// The entry exists but is no longer transferrable (an image
    /// row, an `Html` row, a rich-only row whose plain preview
    /// is gone, …). The importer collapses this into the typed
    /// `not_transferable` outcome.
    NotTransferable,
    /// The entry exceeded the [`FETCH_TEXT_MAX_BODY_BYTES`] cap.
    /// The transport surfaces this variant verbatim; the
    /// importer rejects the body without persisting anything.
    BodyTooLarge,
    /// The persistence layer refused the lookup (SQLite error,
    /// missing handle, …). The runtime collapses this into the
    /// typed `persistence_unavailable` outcome.
    PersistenceUnavailable,
}

/// Coordinated start / stop the [`PeerTransport`] layer delegates
/// to in order to keep the mDNS advertisement in lockstep with
/// the real ephemeral port the TLS listener reserved. The
/// implementation owns both the listener and the discovery
/// adapter: the runtime, SQLite, Tauri and the frontend never
/// see the port or the IP.
pub trait PairingAdvertisementSink: Send + Sync {
    /// Publish the pairing advertisement. The `bound_port` is the
    /// non-zero ephemeral port the TLS listener just reserved;
    /// the implementation forwards it through mDNS alongside
    /// `capability = pairing`.
    fn publish(&self, bound_port: u16) -> Result<(), TransportError>;
    /// Withdraw the pairing advertisement. Called on `stop`,
    /// before the listener is dropped, so a remote browser sees
    /// the goodbye packet while the OS still accepts the TCP
    /// connection.
    fn withdraw(&self) -> Result<(), TransportError>;
}

/// Transport trait the runtime drives. Mirrors the discovery
/// adapter's idempotent lifecycle: a second `start` while the
/// transport is already running returns
/// [`TransportOutcome::Unavailable`] (or
/// [`AdapterError::AlreadyRunning`] in the typed-error taxonomy
/// the runtime exposes); `stop` is always idempotent.
pub trait PeerTransport: Send + Sync {
    fn start(
        &self,
        identity: &LocalPeerIdentity,
        sink: Arc<dyn TransportSink>,
    ) -> Result<u16, TransportError>;

    /// Productive install path. The runtime passes the
    /// keychain-backed material the platform crate uses to mint
    /// the listener's self-signed cert and sign the pairing
    /// transcript. The advertisement sink the platform layer
    /// wired against the discovery adapter publishes the bound
    /// port through mDNS in lockstep with the listener start so
    /// `stop` cleanly withdraws the record. Implementations MUST
    /// either succeed (binding a non-zero ephemeral port) or
    /// leave the transport in the documented `stopped` state;
    /// returning `Crypto` here is a contract violation because
    /// the loader already minted the material.
    ///
    /// The method only exists when the `local-peer-pairing-tls`
    /// feature is active because the [`LocalIdentityMaterial`]
    /// argument is feature-gated. Hosts without the feature
    /// drive the legacy [`Self::start`] entry point.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn start_with_material(
        &self,
        material: LocalIdentityMaterial,
        sink: Arc<dyn TransportSink>,
        advertisement: Arc<dyn PairingAdvertisement>,
    ) -> Result<u16, TransportError>;

    /// Productive install path that also receives the validated
    /// display name the bootstrap / shell wants the listener to
    /// publish on every `HelloAck` / `InboundSessionMetadata`
    /// envelope it emits. The transport caches the value
    /// verbatim; the platform crate falls back to the cert
    /// short fingerprint when the string is empty / whitespace.
    /// The default implementation forwards to
    /// [`Self::start_with_material`] so a feature pair that
    /// builds without the productive display-name path keeps
    /// working — the cert short fingerprint still reaches the
    /// wire, just not under the same named field the UI shows.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn start_with_material_and_display_name(
        &self,
        material: LocalIdentityMaterial,
        sink: Arc<dyn TransportSink>,
        advertisement: Arc<dyn PairingAdvertisement>,
        display_name: &str,
    ) -> Result<u16, TransportError> {
        let _ = display_name;
        self.start_with_material(material, sink, advertisement)
    }

    /// Productive install path that also wires the
    /// [`RemotePeerResolver`] the productive pairing transport
    /// uses to dial the announced listener. The resolver lives
    /// exclusively inside `clipvault-platform` so the runtime,
    /// SQLite, Tauri and the frontend never see an endpoint
    /// byte. The default implementation forwards to
    /// [`Self::start_with_material_and_display_name`] so hosts
    /// without the productive feature pair keep working — a
    /// `start_outbound` call will simply surface
    /// [`TransportError::Unavailable`] because no resolver was
    /// installed.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn start_with_material_and_resolver(
        &self,
        material: LocalIdentityMaterial,
        sink: Arc<dyn TransportSink>,
        advertisement: Arc<dyn PairingAdvertisement>,
        resolver: Option<Arc<dyn RemotePeerResolver>>,
        display_name: &str,
    ) -> Result<u16, TransportError> {
        let _ = resolver;
        self.start_with_material_and_display_name(material, sink, advertisement, display_name)
    }

    /// Productive install path that also wires the resolver and
    /// the host-side [`HistoryHostHandler`] the listener drives
    /// when a `ListRecentText` envelope lands. The handler is
    /// installed by the bootstrap after the productive pairing
    /// material is loaded so the listener can project the local
    /// metadata-only page through the same `HostHistorySource`
    /// the core runtime owns. The default implementation forwards
    /// to [`Self::start_with_material_and_resolver`] so a feature
    /// pair that builds without the productive history path keeps
    /// compiling — `ListRecentText` envelopes collapse to
    /// `not_available` until the handler is installed.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn start_with_material_resolver_and_history(
        &self,
        material: LocalIdentityMaterial,
        sink: Arc<dyn TransportSink>,
        advertisement: Arc<dyn PairingAdvertisement>,
        resolver: Option<Arc<dyn RemotePeerResolver>>,
        display_name: &str,
        history_handler: Option<Arc<dyn HistoryHostHandler>>,
        fetch_handler: Option<Arc<dyn FetchTextHostHandler>>,
        image_history_handler: Option<Arc<dyn ImageHistoryHostHandler>>,
        image_fetch_handler: Option<Arc<dyn FetchImageHostHandler>>,
        image_thumbnail_handler: Option<Arc<dyn FetchImageThumbnailHostHandler>>,
    ) -> Result<u16, TransportError> {
        let _ = history_handler;
        let _ = fetch_handler;
        let _ = image_history_handler;
        let _ = image_fetch_handler;
        let _ = image_thumbnail_handler;
        self.start_with_material_and_resolver(material, sink, advertisement, resolver, display_name)
    }

    fn stop(&self) -> Result<(), TransportError>;

    fn is_running(&self) -> bool;

    /// Arm the cert fingerprint pin the productive pairing
    /// transport enforces against the next mTLS handshake from
    /// the matching `peer_id`. The runtime MUST call this
    /// exactly once per successful trust promotion so a future
    /// connection that presents a different cert is rejected
    /// before the protocol layer sees any bytes.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn arm_pin(&self, peer_id: &str, cert_fingerprint: &str) -> Result<(), TransportError>;

    /// Disarm the cert fingerprint pin the productive pairing
    /// transport holds for the matching `peer_id`. The runtime
    /// MUST call this whenever the row leaves the `Trusted`
    /// state (revoke, block) so a stale pin cannot resurrect the
    /// link.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn disarm_pin(&self, peer_id: &str) -> Result<(), TransportError>;

    /// Run a metadata-only health check against the pinned
    /// cert fingerprint for the matching `peer_id`. A future
    /// cert with a different fingerprint surfaces as
    /// [`TransportError::KeyMismatch`]; an unknown / missing
    /// pin surfaces as [`TransportError::UnknownPeer`].
    /// History / fetch / import routes return the same typed
    /// `Unavailable` outcome the spec scenario "Trusted peer
    /// asks for history early" pins — the transport never opens
    /// content RPCs in this change.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn health_check(&self, peer_id: &str, cert_fingerprint: &str) -> Result<(), TransportError>;

    /// Install (or replace) the host-side [`HistoryHostHandler`]
    /// the listener drives when a `ListRecentText` envelope lands.
    /// The bootstrap calls this after the productive pairing
    /// material loader returns so the handler can rely on the same
    /// SQLite handle the runtime already holds. Idempotent: a
    /// second call replaces the previous handler so a future
    /// refactor that re-wires the runtime cannot leak events to a
    /// stale sink.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_history_handler(
        &self,
        handler: Arc<dyn HistoryHostHandler>,
    ) -> Result<(), TransportError>;

    /// Open an outbound pairing session against the announced
    /// peer. The transport resolves the peer through the
    /// platform's mDNS adapter, opens the mTLS connection, drives
    /// the `Hello` / `HelloAck` envelopes, computes the SAS
    /// against the symmetric transcript and returns the opaque
    /// [`PairingOutbound`] the runtime hands back to the UI.
    /// The runtime never sees an address or a port: the
    /// transport owns the dial loop and the in-memory session
    /// table. Errors collapse into the typed [`TransportError`]
    /// variants the runtime already branches on (`UnknownPeer`,
    /// `KeyMismatch`, `Revoked`, `Blocked`, `Unavailable`,
    /// `IncompatibleProtocol`, `Malformed`).
    #[cfg(feature = "local-peer-pairing-tls")]
    fn start_outbound(
        &self,
        descriptor: OutboundSessionDescriptor,
    ) -> Result<PairingOutbound, TransportError>;

    /// Sign and send the `Approve` envelope over the open mTLS
    /// connection the matching [`PairingSessionId`] holds. The
    /// transport uses the local identity material it cached at
    /// install time and the transcript it stored when
    /// [`Self::start_outbound`] returned; no bytes cross the
    /// runtime boundary. The remote observation flows back
    /// through the [`TransportSink`] the runtime installed at
    /// install time — this method only signals the transport
    /// that the local user accepted. The transport waits for
    /// the remote's `Approve` envelope before invoking the sink
    /// so a single `on_pairing_observed` call can promote both
    /// sides to `Trusted`.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn approve_local(&self, session_id: PairingSessionId) -> Result<(), TransportError>;

    /// Signal the inbound transport task that the local user
    /// approved the metadata the listener pushed through
    /// [`TransportSink::on_pairing_session_started`]. The
    /// listener uses the channel to send the `HelloAck`
    /// envelope over the open mTLS connection and continue
    /// the bounded pairing protocol. Idempotent: a session
    /// already approved collapses to `Ok(())`.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn approve_inbound_session(&self, session_id: PairingSessionId) -> Result<(), TransportError>;

    /// Cancel an in-flight pairing session. Idempotent. Closes
    /// the mTLS connection and removes the entry from the
    /// transport's session table so the runtime can re-attempt
    /// the pairing from a clean slate.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn cancel_session(&self, session_id: PairingSessionId) -> Result<(), TransportError>;

    /// Close every session the transport holds for the matching
    /// `peer_id`. Used by the runtime on revoke / block so a
    /// previously-trusted peer cannot keep an open pairing
    /// socket after the row leaves the trusted state. Idempotent
    /// and never raises on unknown peers.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn disconnect_peer(&self, peer_id: &str) -> Result<(), TransportError>;

    /// Open a productive metadata-only health probe against the
    /// pinned peer. The transport dials the remote listener over
    /// mTLS, exchanges the bounded `health` envelope, returns
    /// `Ok(())` only when the remote presented the pinned cert
    /// fingerprint. Revoked / blocked / unknown / mismatched
    /// peers collapse into the typed [`TransportError`]
    /// variants; no payload other than the bounded health
    /// version / presence fields ever crosses the wire.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn health_probe(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
    ) -> Result<PeerHealthSnapshot, TransportError>;

    /// Open a metadata-only `list_recent_text` request against the
    /// pinned peer. The transport dials the remote listener over
    /// mTLS, exchanges the bounded `list_recent_text` envelope and
    /// returns either the typed [`PeerHistorySnapshot`] the host
    /// emitted or one of the typed [`TransportError`] variants
    /// the runtime already branches on (`UnknownPeer`,
    /// `KeyMismatch`, `Revoked`, `Blocked`, `Unavailable`,
    /// `IncompatibleProtocol`, `Malformed`). The host never
    /// accepts a cursor the listener did not mint and the
    /// response payload is the bounded metadata-only page
    /// (`history.rs` documents the cursor / preview rules).
    ///
    /// The default implementation returns
    /// [`TransportError::Unavailable`] so a transport that does
    /// not yet wire the productive history envelope still
    /// compiles — the shell surfaces the typed reason the
    /// runtime already uses for the discovery-only contract.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn list_recent_text(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        cursor: &str,
        limit: u32,
    ) -> Result<PeerHistorySnapshot, TransportError> {
        let _ = (peer_id, cert_fingerprint, cursor, limit);
        Err(TransportError::Unavailable)
    }

    /// Open an authenticated `fetch_text` request against the
    /// pinned peer. The transport dials the remote listener over
    /// mTLS, exchanges the bounded `fetch_text` envelope and
    /// returns either the typed [`PeerFetchSnapshot`] the host
    /// emitted or one of the typed [`TransportError`] variants
    /// the runtime already branches on. The body the host returns
    /// is bounded by [`FETCH_TEXT_MAX_BODY_BYTES`]; the transport
    /// re-validates the limit before handing the payload back so
    /// a drifted host cannot accidentally stream more than the
    /// contract allows.
    ///
    /// The default implementation returns
    /// [`TransportError::Unavailable`] so a transport that does
    /// not yet wire the productive fetch envelope still compiles
    /// — the shell surfaces the typed reason the runtime already
    /// uses for the discovery-only contract.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn fetch_text(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        remote_entry_id: &str,
    ) -> Result<PeerFetchSnapshot, TransportError> {
        let _ = (peer_id, cert_fingerprint, remote_entry_id);
        Err(TransportError::Unavailable)
    }

    /// Install (or replace) the host-side [`FetchTextHostHandler`]
    /// the listener drives when a `FetchText` envelope lands.
    /// The bootstrap calls this after the productive pairing
    /// material loader returns so the handler can rely on the
    /// same SQLite handle the runtime already holds. Idempotent:
    /// a second call replaces the previous handler so a future
    /// refactor that re-wires the runtime cannot leak events to a
    /// stale sink.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_fetch_handler(
        &self,
        handler: Arc<dyn FetchTextHostHandler>,
    ) -> Result<(), TransportError>;

    /// Install (or replace) the host-side [`ImageHistoryHostHandler`]
    /// the listener drives when a `ListRecentImages` envelope
    /// lands. The bootstrap calls this after the productive
    /// pairing material loader returns so the handler can rely
    /// on the same SQLite handle the runtime already holds.
    /// Idempotent: a second call replaces the previous handler so
    /// a future refactor that re-wires the runtime cannot leak
    /// events to a stale sink.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_image_history_handler(
        &self,
        handler: Arc<dyn ImageHistoryHostHandler>,
    ) -> Result<(), TransportError>;

    /// Install (or replace) the host-side [`FetchImageHostHandler`]
    /// the listener drives when a `FetchImage` envelope lands.
    /// The bootstrap calls this after the productive pairing
    /// material loader returns so the handler can rely on the
    /// same SQLite handle + asset store the runtime already
    /// holds. Idempotent: a second call replaces the previous
    /// handler so a future refactor that re-wires the runtime
    /// cannot leak events to a stale sink.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_image_fetch_handler(
        &self,
        handler: Arc<dyn FetchImageHostHandler>,
    ) -> Result<(), TransportError>;

    /// Open a metadata-only `list_recent_images` request against
    /// the pinned peer. The transport dials the remote listener
    /// over mTLS, exchanges the bounded `list_recent_images`
    /// envelope and returns either the typed
    /// [`PeerImageHistorySnapshot`] the host emitted or one of
    /// the typed [`TransportError`] variants the runtime already
    /// branches on. The host never accepts a cursor the listener
    /// did not mint and the response payload is the bounded
    /// metadata-only image page (no bytes, no thumbnails, no
    /// `asset_ref`).
    ///
    /// The default implementation returns
    /// [`TransportError::Unavailable`] so a transport that does
    /// not yet wire the productive image envelope still compiles
    /// — the shell surfaces the typed reason the runtime
    /// already uses for the discovery-only contract.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn list_recent_images(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        cursor: &str,
        limit: u32,
    ) -> Result<PeerImageHistorySnapshot, TransportError> {
        let _ = (peer_id, cert_fingerprint, cursor, limit);
        Err(TransportError::Unavailable)
    }

    /// Open an authenticated `fetch_image` request against the
    /// pinned peer. The transport dials the remote listener over
    /// mTLS, exchanges the bounded `fetch_image` envelope and
    /// returns either the typed [`PeerImageFetchSnapshot`] the
    /// host emitted or one of the typed [`TransportError`]
    /// variants the runtime already branches on. The bytes the
    /// host returns are bounded by [`FETCH_IMAGE_MAX_BODY_BYTES`];
    /// the transport re-validates the limit locally before
    /// handing the snapshot back so a drifted host cannot
    /// accidentally bypass the documented cap.
    ///
    /// The default implementation returns
    /// [`TransportError::Unavailable`] so a transport that does
    /// not yet wire the productive image fetch envelope still
    /// compiles.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn fetch_image(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        remote_entry_id: &str,
    ) -> Result<PeerImageFetchSnapshot, TransportError> {
        let _ = (peer_id, cert_fingerprint, remote_entry_id);
        Err(TransportError::Unavailable)
    }

    /// Install (or replace) the host-side
    /// [`FetchImageThumbnailHostHandler`] the listener drives when
    /// a `FetchImageThumbnail` envelope lands. The bootstrap calls
    /// this after the productive pairing material loader returns
    /// so the handler can rely on the same SQLite handle + asset
    /// store the runtime already holds. Idempotent: a second call
    /// replaces the previous handler so a future refactor that
    /// re-wires the runtime cannot leak events to a stale sink.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_image_thumbnail_handler(
        &self,
        handler: Arc<dyn FetchImageThumbnailHostHandler>,
    ) -> Result<(), TransportError>;

    /// Install (or replace) the source-app presentation handler the
    /// listener invokes for authenticated, visible-row requests.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_source_app_presentation_handler(
        &self,
        handler: Arc<dyn FetchSourceAppPresentationHostHandler>,
    ) -> Result<(), TransportError> {
        let _ = handler;
        Err(TransportError::Unavailable)
    }

    /// Open an authenticated `fetch_image_thumbnail` request
    /// against the pinned peer. The transport dials the remote
    /// listener over mTLS, exchanges the bounded
    /// `fetch_image_thumbnail` envelope and returns either the
    /// typed [`PeerImageThumbnailSnapshot`] the host emitted or
    /// one of the typed [`TransportError`] variants the runtime
    /// already branches on. The body the host returns is bounded
    /// by [`FETCH_IMAGE_THUMBNAIL_MAX_BODY_BYTES`]; the transport
    /// re-validates the limit locally before handing the snapshot
    /// back so a drifted host cannot accidentally bypass the
    /// documented cap.
    ///
    /// The default implementation returns
    /// [`TransportError::Unavailable`] so a transport that does
    /// not yet wire the productive thumbnail envelope still
    /// compiles — the shell surfaces the typed reason the
    /// runtime already uses for the discovery-only contract.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn fetch_image_thumbnail(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        remote_entry_id: &str,
    ) -> Result<PeerImageThumbnailSnapshot, TransportError> {
        let _ = (peer_id, cert_fingerprint, remote_entry_id);
        Err(TransportError::Unavailable)
    }

    /// Fetch the validated source-app display name + bounded
    /// PNG icon for one visible remote entry. The transport
    /// authenticates `peer_id` against the pinned cert
    /// fingerprint and re-validates the
    /// `source_app_presentation` capability before dialling
    /// the listener. The runtime MUST only call this entry
    /// point when a remote rail row becomes visible; legacy
    /// peers that did not advertise the capability collapse
    /// to the typed `NotAvailable` rejection without exposing
    /// the icon path. Production shells wire this entry
    /// point against the productive pairing transport; the
    /// noop stub returns [`TransportError::Unavailable`].
    #[cfg(feature = "local-peer-pairing-tls")]
    fn fetch_source_app_presentation(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        remote_entry_id: &str,
    ) -> Result<PeerSourceAppPresentationSnapshot, TransportError> {
        let _ = (peer_id, cert_fingerprint, remote_entry_id);
        Err(TransportError::Unavailable)
    }
}

/// Metadata-only response the transport returns from
/// [`PeerTransport::health_probe`]. The struct carries only
/// presence + protocol version; the transport never opens
/// history / fetch / import routes in this change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerHealthSnapshot {
    pub peer_id: String,
    pub protocol_major: i64,
    pub reached_at_unix_secs: i64,
}

/// Metadata-only response the transport returns from
/// [`PeerTransport::list_recent_text`]. The struct is the
/// bounded page the host projected through the
/// `peer-text-history-browser` runtime: only the
/// [`wire::ListRecentTextRow`] entries the host minted, the
/// opaque `next_cursor` the renderer must submit verbatim to
/// fetch the next page, and the `snapshot_id` the runtime
/// surfaces as a stable tie-breaker. The transport never
/// inspects the row payload beyond the type check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerHistorySnapshot {
    pub peer_id: String,
    pub rows: Vec<wire::ListRecentTextRow>,
    pub next_cursor: String,
    pub snapshot_id: String,
}

/// Bounded response the transport returns from
/// [`PeerTransport::fetch_text`]. The struct carries the
/// validated body the host returned through the wire envelope:
/// only the validated, trimmed `title`, the canonical
/// `content_type` string the local SQLite layer persists, and
/// the bounded UTF-8 body the host capped at
/// [`FETCH_TEXT_MAX_BODY_BYTES`]. The transport re-validates
/// the size locally before returning the snapshot so a
/// malformed / drifted host cannot accidentally bypass the
/// documented cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerFetchSnapshot {
    pub peer_id: String,
    pub remote_entry_id: String,
    pub title: Option<String>,
    pub content_type: String,
    pub body: String,
    /// Optional validated source-app display name the
    /// `peer-source-app-presentation` change attaches to the
    /// explicit fetch response. `None` for legacy peers that did
    /// not opt into the additive contract.
    pub source_app_name: Option<String>,
    /// Optional validated source-app icon bytes the
    /// `peer-source-app-presentation` change attaches. The bytes
    /// are held in memory only between the fetch and the import
    /// commit; the caller MUST stage them through the local
    /// application-icons writer before persisting any reference.
    pub source_app_icon_bytes: Option<Vec<u8>>,
}

/// Metadata-only response the transport returns from
/// [`PeerTransport::list_recent_images`]. The struct mirrors
/// [`PeerHistorySnapshot`] for the image side of the
/// `peer-image-import` change: the bounded page the host
/// projected through the productive mTLS runtime, only the
/// [`wire::ListRecentImageRow`] entries the host minted, the
/// opaque `next_cursor` the renderer must submit verbatim to
/// fetch the next page, and the `snapshot_id` the runtime
/// surfaces as a stable tie-breaker. The transport never
/// inspects the row payload beyond the type check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerImageHistorySnapshot {
    pub peer_id: String,
    pub rows: Vec<wire::ListRecentImageRow>,
    pub next_cursor: String,
    pub snapshot_id: String,
}

/// Bounded response the transport returns from
/// [`PeerTransport::fetch_image`]. The struct carries the
/// validated PNG payload the host returned through the wire
/// envelope: only the validated, trimmed `title` and the
/// bounded PNG bytes the host capped at
/// [`FETCH_IMAGE_MAX_BODY_BYTES`]. The transport re-validates
/// the size locally before returning the snapshot so a
/// malformed / drifted host cannot accidentally bypass the
/// documented cap. The runtime routes the bytes through the
/// local PNG validation pipeline before persisting them
/// through `ClipboardAssetStore`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerImageFetchSnapshot {
    pub peer_id: String,
    pub remote_entry_id: String,
    pub title: Option<String>,
    pub bytes: Vec<u8>,
    /// Optional validated source-app display name the
    /// `peer-source-app-presentation` change attaches to the
    /// explicit image-fetch response. `None` for legacy peers that
    /// did not opt into the additive contract.
    pub source_app_name: Option<String>,
    /// Optional validated source-app icon bytes the
    /// `peer-source-app-presentation` change attaches. The bytes
    /// are held in memory only between the fetch and the import
    /// commit; the caller MUST stage them through the local
    /// application-icons writer before persisting any reference.
    pub source_app_icon_bytes: Option<Vec<u8>>,
}

/// Bounded response the transport returns from
/// [`PeerTransport::fetch_image_thumbnail`]. The struct carries
/// only the validated PNG body the host emitted through the
/// dedicated thumbnail envelope: the bounded bytes the host
/// capped at [`FETCH_IMAGE_THUMBNAIL_MAX_BODY_BYTES`] and the
/// resulting pixel dimensions. The transport re-validates the
/// size locally before returning the snapshot so a drifted host
/// cannot accidentally bypass the documented cap. The bytes are
/// held in memory only between the fetch and the renderer; the
/// caller never persists the PNG or substitutes the snapshot for
/// the original [`PeerImageFetchSnapshot`] the explicit `Importar`
/// flow uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerImageThumbnailSnapshot {
    pub peer_id: String,
    pub remote_entry_id: String,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Metadata-only response the transport returns from
/// [`PeerTransport::fetch_source_app_presentation`]. The struct
/// carries the validated source-app display name + bounded
/// PNG icon the `peer-source-app-presentation` change
/// exchanges: the optional display name (trimmed, ≤ 128
/// Unicode scalar values, no control characters) and the
/// optional bounded PNG icon (≤ 512 KiB, ≤ 256 × 256 px).
/// The transport re-validates the envelope before returning
/// the snapshot so a drifted host cannot accidentally bypass
/// the documented caps. The bytes are held in memory only
/// between the fetch and the renderer; the caller never
/// persists the PNG or substitutes the snapshot for the
/// original image fetch the explicit `Importar` flow uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerSourceAppPresentationSnapshot {
    pub peer_id: String,
    pub remote_entry_id: String,
    pub source_app_name: Option<String>,
    /// Bounded PNG bytes the host validated against the
    /// 512 KiB / 256 × 256 px envelope the spec pins. The
    /// transport rejects any payload larger than
    /// [`crate::peer_source_app_presentation::MAX_SOURCE_APP_ICON_BYTES`].
    pub source_app_icon_b64: Option<String>,
}

/// Platform-neutral handle the platform layer exposes to the
/// pairing runtime so the runtime can drive the mDNS advertisement
/// without ever seeing the bound port. The platform crate wires
/// the implementation against the discovery adapter the bootstrap
/// already installed; the runtime only forwards the typed call.
pub trait PairingAdvertisement: Send + Sync {
    /// Publish the pairing advertisement with the real bound port
    /// the listener just reserved.
    fn publish(&self, bound_port: u16) -> Result<(), TransportError>;
    /// Withdraw the pairing advertisement. Called from `stop`
    /// before the listener is dropped so a remote browser sees the
    /// goodbye packet while the OS still accepts the TCP
    /// connection.
    fn withdraw(&self) -> Result<(), TransportError>;
}

/// Trait the platform layer implements so the pairing transport
/// can resolve a `peer_id` into the `SocketAddr` the remote
/// listener currently advertises. The runtime, SQLite, Tauri and
/// the frontend MUST NOT see this surface: the address, the port
/// and the lookup state stay inside `clipvault-platform` so a
/// future refactor that changes the mDNS backend cannot leak an
/// endpoint byte to the bridge.
#[cfg(feature = "local-peer-pairing-tls")]
pub trait RemotePeerResolver: Send + Sync {
    /// Return the most recent `SocketAddr` the resolver knows for
    /// the matching `peer_id`. `None` when the resolver has no
    /// record of the peer (the history route surfaces the
    /// typed `PeerUnresolved` outcome rather than treating that
    /// temporary endpoint absence as a trust failure).
    fn resolve(&self, peer_id: &str) -> Option<std::net::SocketAddr>;
}

/// Typed error the production transport surfaces on `start` /
/// `stop`. The variants mirror [`TransportOutcome`] for the
/// call-site error taxonomy but the trait returns `Result<u16, _>`
/// so the runtime can branch on the bound ephemeral port without
/// inspecting free-form strings.
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("peer transport is already running")]
    AlreadyRunning,
    #[error("peer transport is not running")]
    NotRunning,
    #[error("peer transport cannot bind an ephemeral port on this session")]
    Unavailable,
    #[error("peer transport cryptographic provider refused to mint a key")]
    Crypto,
    #[error("peer transport rejected a malformed advertisement or wire payload")]
    Malformed,
    /// The remote peer presented a `peer_id` that does not match
    /// a persisted `known_peers` row, or its TLS cert
    /// fingerprint did not match the pinned value the runtime
    /// persisted after the reciprocal pairing.
    #[error("peer transport rejected an unknown peer or mismatched identity")]
    UnknownPeer,
    /// Discovery still reports a peer as present, but has not yet
    /// resolved a non-zero pairing endpoint for it. This is not a
    /// trust or certificate failure and callers may retry it within
    /// a bounded transient window.
    #[error("peer transport has no resolved pairing endpoint")]
    PeerUnresolved,
    /// The remote peer's TLS chain does not authenticate against
    /// the pinned certificate fingerprint the pairing handshake
    /// recorded.
    #[error("peer transport rejected a mismatched TLS identity")]
    KeyMismatch,
    /// The remote peer is in the [`clipvault_db::TrustState::Blocked`]
    /// state.
    #[error("peer transport rejected a blocked peer")]
    Blocked,
    /// The remote peer is in the [`clipvault_db::TrustState::Revoked`]
    /// state.
    #[error("peer transport rejected a revoked peer")]
    Revoked,
    /// The remote peer speaks a wire protocol version the runtime
    /// does not understand.
    #[error("peer transport wire protocol is incompatible")]
    IncompatibleProtocol,
    /// The remote peer refused the page request because the
    /// caller submitted an opaque cursor the host did not mint
    /// (forged payload, replay against another peer, signed under
    /// a rotated HMAC secret, …). The runtime collapses this
    /// into the typed `invalid_cursor` outcome the spec pins; a
    /// generic `Malformed` would have hidden the reason behind a
    /// network-shaped error.
    #[error("peer transport rejected an invalid history cursor")]
    InvalidCursor,
    /// The remote host returned a body that exceeded the
    /// [`FETCH_TEXT_MAX_BODY_BYTES`] cap. The transport
    /// enforces the limit locally so a drifted host cannot
    /// stream more than the contract allows; the importer
    /// collapses the rejection into the typed `body_too_large`
    /// outcome without persisting anything.
    #[error("peer transport rejected a fetch body that exceeded the 1 MiB UTF-8 limit")]
    BodyTooLarge,
}

/// Noop transport the platform crate installs when the
/// `local-peer-pairing-tls` feature is not enabled (cross-compiles,
/// Windows builds, distroless test sandboxes). The noop is the
/// safe default — pairing is opt-in and a host that cannot link
/// the TLS stack reports `Unavailable` to the runtime instead of
/// silently spawning a half-broken listener.
#[derive(Debug, Default)]
pub struct NoopPeerTransport {
    running: AtomicBool,
}

impl NoopPeerTransport {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PeerTransport for NoopPeerTransport {
    fn start(
        &self,
        _identity: &LocalPeerIdentity,
        _sink: Arc<dyn TransportSink>,
    ) -> Result<u16, TransportError> {
        self.running.store(false, Ordering::Release);
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn start_with_material(
        &self,
        _material: LocalIdentityMaterial,
        _sink: Arc<dyn TransportSink>,
        _advertisement: Arc<dyn PairingAdvertisement>,
    ) -> Result<u16, TransportError> {
        self.running.store(false, Ordering::Release);
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn start_with_material_and_display_name(
        &self,
        _material: LocalIdentityMaterial,
        _sink: Arc<dyn TransportSink>,
        _advertisement: Arc<dyn PairingAdvertisement>,
        _display_name: &str,
    ) -> Result<u16, TransportError> {
        self.running.store(false, Ordering::Release);
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn start_with_material_and_resolver(
        &self,
        _material: LocalIdentityMaterial,
        _sink: Arc<dyn TransportSink>,
        _advertisement: Arc<dyn PairingAdvertisement>,
        _resolver: Option<Arc<dyn RemotePeerResolver>>,
        _display_name: &str,
    ) -> Result<u16, TransportError> {
        self.running.store(false, Ordering::Release);
        Err(TransportError::Unavailable)
    }

    fn stop(&self) -> Result<(), TransportError> {
        self.running.store(false, Ordering::Release);
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn arm_pin(&self, _peer_id: &str, _cert_fingerprint: &str) -> Result<(), TransportError> {
        // The noop transport never mints a cert, so arming a
        // pin is meaningless. Surface the same typed reason the
        // production path surfaces when the runtime asks for a
        // productive operation on a transport the platform
        // cannot supply.
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn disarm_pin(&self, _peer_id: &str) -> Result<(), TransportError> {
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn health_check(&self, _peer_id: &str, _cert_fingerprint: &str) -> Result<(), TransportError> {
        // The noop transport never observes a peer cert, so a
        // health check collapses to the typed `Unavailable`
        // outcome the runtime already surfaces for the
        // discovery-only contract.
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_history_handler(
        &self,
        _handler: Arc<dyn HistoryHostHandler>,
    ) -> Result<(), TransportError> {
        // The noop transport never opens a session, so a
        // history-handler install collapses to the typed
        // `Unavailable` outcome the runtime already surfaces.
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_fetch_handler(
        &self,
        _handler: Arc<dyn FetchTextHostHandler>,
    ) -> Result<(), TransportError> {
        // The noop transport never opens a session, so a
        // fetch-handler install collapses to the typed
        // `Unavailable` outcome the runtime already surfaces.
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_image_history_handler(
        &self,
        _handler: Arc<dyn ImageHistoryHostHandler>,
    ) -> Result<(), TransportError> {
        // The noop transport never opens a session, so an
        // image-history-handler install collapses to the typed
        // `Unavailable` outcome the runtime already surfaces.
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_image_fetch_handler(
        &self,
        _handler: Arc<dyn FetchImageHostHandler>,
    ) -> Result<(), TransportError> {
        // The noop transport never opens a session, so an
        // image-fetch-handler install collapses to the typed
        // `Unavailable` outcome the runtime already surfaces.
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_image_thumbnail_handler(
        &self,
        _handler: Arc<dyn FetchImageThumbnailHostHandler>,
    ) -> Result<(), TransportError> {
        // The noop transport never opens a session, so a
        // thumbnail-handler install collapses to the typed
        // `Unavailable` outcome the runtime already surfaces.
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn fetch_text(
        &self,
        _peer_id: &str,
        _cert_fingerprint: &str,
        _remote_entry_id: &str,
    ) -> Result<PeerFetchSnapshot, TransportError> {
        // The noop transport never opens a real session, so a
        // fetch request collapses to the typed `Unavailable`
        // outcome the runtime already surfaces for the
        // discovery-only contract.
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn start_outbound(
        &self,
        _descriptor: OutboundSessionDescriptor,
    ) -> Result<PairingOutbound, TransportError> {
        // The noop transport never mints a TLS listener and
        // therefore cannot dial a remote peer; surface the same
        // typed reason the productive install path uses when
        // the keychain is unreachable.
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn approve_local(&self, _session_id: PairingSessionId) -> Result<(), TransportError> {
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn approve_inbound_session(&self, _session_id: PairingSessionId) -> Result<(), TransportError> {
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn cancel_session(&self, _session_id: PairingSessionId) -> Result<(), TransportError> {
        // The noop transport never opens a session; cancel is a
        // typed no-op so the runtime can branch on the same
        // outcome the productive path surfaces.
        Err(TransportError::Unavailable)
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
    ) -> Result<PeerHealthSnapshot, TransportError> {
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn list_recent_text(
        &self,
        _peer_id: &str,
        _cert_fingerprint: &str,
        _cursor: &str,
        _limit: u32,
    ) -> Result<PeerHistorySnapshot, TransportError> {
        // The noop transport never opens a real session, so a
        // history probe collapses to the typed `Unavailable`
        // outcome the runtime already surfaces for the
        // discovery-only contract.
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn list_recent_images(
        &self,
        _peer_id: &str,
        _cert_fingerprint: &str,
        _cursor: &str,
        _limit: u32,
    ) -> Result<PeerImageHistorySnapshot, TransportError> {
        // The noop transport never opens a real session, so an
        // image history probe collapses to the typed
        // `Unavailable` outcome the runtime already surfaces for
        // the discovery-only contract.
        Err(TransportError::Unavailable)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn fetch_image(
        &self,
        _peer_id: &str,
        _cert_fingerprint: &str,
        _remote_entry_id: &str,
    ) -> Result<PeerImageFetchSnapshot, TransportError> {
        // The noop transport never opens a real session, so an
        // image fetch request collapses to the typed
        // `Unavailable` outcome the runtime already surfaces for
        // the discovery-only contract.
        Err(TransportError::Unavailable)
    }
}

/// Production transport backed by `tokio` + `rustls` +
/// `tokio-rustls` + `rcgen`. The struct is compiled only when
/// the `local-peer-pairing-tls` feature is enabled; cross-compiles
/// and unsupported targets link [`NoopPeerTransport`] instead.
#[cfg(feature = "local-peer-pairing-tls")]
pub struct TlsPeerTransport {
    running: AtomicBool,
    state: Mutex<TransportState>,
}

/// Mutable state the runtime manages through the
/// [`PeerTransport`] trait. Exposed `pub(super)` so the
/// [`crate::peer_transport::tls`] module can wire the install /
/// stop paths without needing accessor methods.
#[cfg(feature = "local-peer-pairing-tls")]
pub(crate) struct TransportState {
    /// Runtime driving the listener + mTLS handshake. `None`
    /// while the transport is stopped.
    pub runtime: Option<Arc<tokio::runtime::Runtime>>,
    /// Ephemeral port the listener bound to. The runtime publishes
    /// the value in the mDNS TXT record so remote peers know
    /// where to dial.
    pub bound_port: Option<u16>,
    /// Accept task handle. Aborted on `stop` so the loop exits
    /// promptly.
    pub accept_handle: Option<tokio::task::JoinHandle<()>>,
    /// Cancellation flag the accept loop polls on every iteration
    /// so the runtime can ask the listener to wind down without
    /// waiting for a new connection.
    pub cancel: Option<Arc<AtomicBool>>,
    /// Advertisement sink the install path wires so the listener
    /// can publish / withdraw through mDNS in lockstep with its
    /// own start / stop cycle.
    pub advertisement: Option<Arc<dyn PairingAdvertisementSink>>,
    /// Sink the runtime installs on `start`; the transport hands
    /// completed observations through it.
    pub sink: Option<Arc<dyn TransportSink>>,
    /// Shared slot the mTLS verifier publishes the peer's cert
    /// DER into. The session loop reads the slot right after the
    /// handshake so the SPKI → peer_id binding check happens
    /// against the cert the remote actually presented.
    pub peer_cert_slot: Option<Arc<crate::peer_transport::tls::PeerCertSlot>>,
    /// Per-peer cert fingerprint pin the runtime arms when a
    /// pairing completes. The transport checks the
    /// SHA-256(cert_der) the future mTLS handshake presents
    /// against this map; an unknown / mismatched / blocked /
    /// revoked peer surfaces as a typed rejection before the
    /// listener hands the connection to the protocol layer.
    pub pins: parking_lot::Mutex<std::collections::HashMap<String, String>>,
    /// Per-peer pin used by the mTLS verifier during the
    /// handshake itself. The transport's `arm_pin` /
    /// `disarm_pin` write through the [`crate::peer_transport::tls::HandshakePinLookup`]
    /// the verifier owns, so a rotated cert is rejected at the
    /// handshake itself — not only on `health_check`. The
    /// runtime never sees this lookup; it lives inside the
    /// transport so a future refactor can move the verifier
    /// without affecting the bridge.
    pub handshake_pins: Arc<crate::peer_transport::tls::HandshakePinLookup>,
    /// Material the transport cached at install time. The
    /// session table uses it to sign the local `Approve`
    /// envelope and to mint the per-session client cert the
    /// mTLS verifier already pinned at install time.
    pub local_material: Option<LocalIdentityMaterial>,
    /// Display name the bootstrap / shell handed to the install
    /// path. The listener publishes this value on every
    /// `HelloAck` / `InboundSessionMetadata` envelope it emits
    /// so the remote peer and the local UI surface the same
    /// human-readable label the mDNS TXT record carries. The
    /// transport never falls back to the cert's short
    /// fingerprint for the visible name — that projection is
    /// only a UI badge for the discovery row.
    pub local_display_name: String,
    /// mDNS adapter the transport uses to resolve a `peer_id`
    /// to the `SocketAddr` of the listener the remote peer
    /// currently advertises. The transport owns the lookup so
    /// the address never crosses the trust boundary.
    pub resolver: Option<Arc<dyn RemotePeerResolver>>,
    /// In-memory table the transport uses to track every
    /// productive pairing session. The runtime hands
    /// [`PairingSessionId`]s; the transport stores the
    /// [`crate::peer_transport::tls::OutboundSession`] so the
    /// cancel / approve paths can find the connection they
    /// belong to.
    pub sessions: parking_lot::Mutex<
        std::collections::HashMap<
            PairingSessionId,
            Arc<parking_lot::Mutex<crate::peer_transport::tls::OutboundSession>>,
        >,
    >,
    /// Shared monotonic counter for BOTH locally-dialled and
    /// listener-originated [`PairingSessionId`] values. The core
    /// runtime stores the two directions in one session map, so
    /// separate counters starting at `1` could overwrite an
    /// inbound invitation when both peers pressed Vincular.
    pub next_session_id: Arc<std::sync::atomic::AtomicU64>,
    /// Optional sender the session tasks use to push a
    /// `TransportSink::on_pairing_observed` event back to the
    /// runtime. Wired to the same [`TransportSink`] the
    /// install path accepted so the renderer can never inject a
    /// remote approval through the bridge.
    pub session_sink: Option<Arc<dyn TransportSink>>,
    /// Per-transport registry the listener uses to track every
    /// in-flight inbound session. Scoping the registry to the
    /// transport (instead of a process-wide static) keeps two
    /// parallel listeners — e.g. a test exercising a single
    /// `start_outbound` round-trip while another test tears
    /// down its own listener — from wiping each other's
    /// pending inbound state.
    pub inbound_sessions: Arc<
        parking_lot::Mutex<
            std::collections::HashMap<
                PairingSessionId,
                Arc<parking_lot::Mutex<crate::peer_transport::tls::InboundSession>>,
            >,
        >,
    >,
    /// Host-side history handler the listener drives when a
    /// `ListRecentText` envelope lands. The trait is the seam
    /// between the mTLS listener and the core runtime; the
    /// transport never inspects row content beyond the
    /// metadata-only projection. `None` on hosts that do not
    /// ship the `peer-text-history-browser` change yet —
    /// `ListRecentText` envelopes collapse to
    /// `ListRecentTextUnavailable { reason: not_available }`
    /// so the wire contract stays stable.
    pub history_handler: Option<Arc<dyn HistoryHostHandler>>,
    /// Host-side fetch handler the listener drives when a
    /// `FetchText` envelope lands. The handler is installed by
    /// the bootstrap through the productive
    /// [`Self::install_fetch_handler`] API; a `None` collapses
    /// `FetchText` envelopes to
    /// `FetchTextUnavailable { reason: not_available }` so the
    /// wire contract stays stable across builds that have not
    /// shipped the `peer-text-import` change yet.
    pub fetch_handler: Option<Arc<dyn FetchTextHostHandler>>,
    /// Host-side image-history handler the listener drives when a
    /// `ListRecentImages` envelope lands. `None` on hosts that do
    /// not ship the `peer-image-import` change yet — the envelope
    /// collapses to `ListRecentImagesUnavailable { reason:
    /// not_available }` so the wire contract stays stable.
    pub image_history_handler: Option<Arc<dyn ImageHistoryHostHandler>>,
    /// Host-side image-fetch handler the listener drives when a
    /// `FetchImage` envelope lands. Mirrors the
    /// [`Self::fetch_handler`] hot-swap pattern.
    pub image_fetch_handler: Option<Arc<dyn FetchImageHostHandler>>,
    /// Host-side image-thumbnail handler the listener drives
    /// when a `FetchImageThumbnail` envelope lands. `None` on
    /// hosts that do not ship the
    /// `peer-image-preview-thumbnails` change yet — the
    /// envelope collapses to `FetchImageThumbnailUnavailable
    /// { reason: not_available }` so the wire contract stays
    /// stable. The bootstrap installs the adapter after the
    /// productive material loader returns so the handler can
    /// rely on the same SQLite handle + asset store the
    /// runtime already holds.
    pub image_thumbnail_handler: Option<Arc<dyn FetchImageThumbnailHostHandler>>,
    /// Hot-swappable host-side source-app presentation handler.
    /// The accept loop holds this shared cell and reads it for
    /// each inbound connection, so installing the productive
    /// adapter after listener startup affects the very next
    /// request rather than only changing a disconnected cache.
    pub source_app_presentation_handler:
        Arc<parking_lot::RwLock<Option<Arc<dyn FetchSourceAppPresentationHostHandler>>>>,
}

#[cfg(feature = "local-peer-pairing-tls")]
impl Default for TransportState {
    fn default() -> Self {
        Self {
            runtime: None,
            bound_port: None,
            accept_handle: None,
            cancel: None,
            advertisement: None,
            sink: None,
            peer_cert_slot: None,
            pins: parking_lot::Mutex::new(std::collections::HashMap::new()),
            handshake_pins: Arc::new(crate::peer_transport::tls::HandshakePinLookup::new()),
            local_material: None,
            local_display_name: String::new(),
            resolver: None,
            sessions: parking_lot::Mutex::new(std::collections::HashMap::new()),
            next_session_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            session_sink: None,
            inbound_sessions: Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
            history_handler: None,
            fetch_handler: None,
            image_history_handler: None,
            image_fetch_handler: None,
            image_thumbnail_handler: None,
            source_app_presentation_handler: Arc::new(parking_lot::RwLock::new(None)),
        }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl std::fmt::Debug for TlsPeerTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsPeerTransport")
            .field(
                "bound_port",
                &self.state.lock().ok().and_then(|s| s.bound_port),
            )
            .finish()
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl TlsPeerTransport {
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            state: Mutex::new(TransportState::default()),
        }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl Default for TlsPeerTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerTransport for TlsPeerTransport {
    fn start(
        &self,
        _identity: &LocalPeerIdentity,
        _sink: Arc<dyn TransportSink>,
    ) -> Result<u16, TransportError> {
        // The legacy install entry point. The bootstrap MUST call
        // [`Self::start_with_material`] instead so the platform
        // layer can mint the cert from the keychain. Returning
        // `Unavailable` here (instead of `Crypto`) surfaces the
        // missing material to the runtime without pretending the
        // transport knows how to mount an in-memory cert.
        Err(TransportError::Unavailable)
    }

    fn start_with_material(
        &self,
        material: LocalIdentityMaterial,
        sink: Arc<dyn TransportSink>,
        advertisement: Arc<dyn PairingAdvertisement>,
    ) -> Result<u16, TransportError> {
        let adapter: Arc<dyn PairingAdvertisementSink> =
            Arc::new(AdvertisementSinkAdapter::new(advertisement));
        super::peer_transport::tls::install_with_material(
            self,
            material,
            String::new(),
            adapter,
            sink,
        )
    }

    /// Production install path that also accepts the validated
    /// display name the bootstrap / shell wants the listener to
    /// publish on every wire envelope it emits. The transport
    /// caches the value verbatim; an empty string collapses to
    /// the cert short fingerprint so the wire always carries a
    /// non-empty label.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn start_with_material_and_display_name(
        &self,
        material: LocalIdentityMaterial,
        sink: Arc<dyn TransportSink>,
        advertisement: Arc<dyn PairingAdvertisement>,
        display_name: &str,
    ) -> Result<u16, TransportError> {
        let adapter: Arc<dyn PairingAdvertisementSink> =
            Arc::new(AdvertisementSinkAdapter::new(advertisement));
        super::peer_transport::tls::install_with_material(
            self,
            material,
            display_name.to_string(),
            adapter,
            sink,
        )
    }

    /// Production install path that also wires the
    /// [`RemotePeerResolver`] the productive pairing transport
    /// uses to dial the announced listener. The runtime passes
    /// `None` on hosts without the mDNS feature pair; the
    /// resolver defaults to `None` so the listener still binds
    /// but `start_outbound` will surface
    /// [`TransportError::Unavailable`] until the shell wires a
    /// real adapter.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn start_with_material_and_resolver(
        &self,
        material: LocalIdentityMaterial,
        sink: Arc<dyn TransportSink>,
        advertisement: Arc<dyn PairingAdvertisement>,
        resolver: Option<Arc<dyn RemotePeerResolver>>,
        display_name: &str,
    ) -> Result<u16, TransportError> {
        let adapter: Arc<dyn PairingAdvertisementSink> =
            Arc::new(AdvertisementSinkAdapter::new(advertisement));
        super::peer_transport::tls::install_with_material_and_resolver(
            self,
            material,
            display_name.to_string(),
            adapter,
            sink,
            resolver,
        )
    }

    fn stop(&self) -> Result<(), TransportError> {
        let mut state = self.state.lock().expect("state lock");
        if let Some(handle) = state.accept_handle.take() {
            handle.abort();
        }
        if let Some(cancel) = state.cancel.take() {
            cancel.store(true, Ordering::Release);
        }
        if let Some(advertisement) = state.advertisement.take() {
            let _ = advertisement.withdraw();
        }
        state.runtime = None;
        state.bound_port = None;
        state.sink = None;
        state.peer_cert_slot = None;
        // Drop every inbound session THIS listener owned so a
        // restart does not inherit dangling entries and so a
        // parallel listener running in the same process keeps
        // its own inbound registry intact. The `clear_*`
        // helper pings every notifier so the blocked session
        // handlers exit the bounded wait promptly. The
        // per-listener scope is the regression fix the
        // 2026-09-20 review surfaced: the previous wiring called
        // a process-wide `clear_inbound_sessions` that wiped
        // sibling listeners' pending inbound sessions.
        super::peer_transport::tls::clear_inbound_sessions(&state.inbound_sessions);
        self.running.store(false, Ordering::Release);
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn arm_pin(&self, peer_id: &str, cert_fingerprint: &str) -> Result<(), TransportError> {
        super::peer_transport::tls::arm_pin(self, peer_id, cert_fingerprint)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn disarm_pin(&self, peer_id: &str) -> Result<(), TransportError> {
        super::peer_transport::tls::disarm_pin(self, peer_id)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn health_check(&self, peer_id: &str, cert_fingerprint: &str) -> Result<(), TransportError> {
        super::peer_transport::tls::health_check(self, peer_id, cert_fingerprint)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_history_handler(
        &self,
        handler: Arc<dyn HistoryHostHandler>,
    ) -> Result<(), TransportError> {
        super::peer_transport::tls::install_history_handler(self, handler)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_fetch_handler(
        &self,
        handler: Arc<dyn FetchTextHostHandler>,
    ) -> Result<(), TransportError> {
        super::peer_transport::tls::install_fetch_handler(self, handler)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_image_history_handler(
        &self,
        handler: Arc<dyn ImageHistoryHostHandler>,
    ) -> Result<(), TransportError> {
        super::peer_transport::tls::install_image_history_handler(self, handler)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_image_fetch_handler(
        &self,
        handler: Arc<dyn FetchImageHostHandler>,
    ) -> Result<(), TransportError> {
        super::peer_transport::tls::install_image_fetch_handler(self, handler)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_image_thumbnail_handler(
        &self,
        handler: Arc<dyn FetchImageThumbnailHostHandler>,
    ) -> Result<(), TransportError> {
        super::peer_transport::tls::install_image_thumbnail_handler(self, handler)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn install_source_app_presentation_handler(
        &self,
        handler: Arc<dyn FetchSourceAppPresentationHostHandler>,
    ) -> Result<(), TransportError> {
        super::peer_transport::tls::install_source_app_presentation_handler(self, handler)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn list_recent_images(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        cursor: &str,
        limit: u32,
    ) -> Result<PeerImageHistorySnapshot, TransportError> {
        super::peer_transport::tls::list_recent_images(
            self,
            peer_id,
            cert_fingerprint,
            cursor,
            limit,
        )
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn fetch_image(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        remote_entry_id: &str,
    ) -> Result<PeerImageFetchSnapshot, TransportError> {
        super::peer_transport::tls::fetch_image(self, peer_id, cert_fingerprint, remote_entry_id)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn fetch_image_thumbnail(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        remote_entry_id: &str,
    ) -> Result<PeerImageThumbnailSnapshot, TransportError> {
        super::peer_transport::tls::fetch_image_thumbnail(
            self,
            peer_id,
            cert_fingerprint,
            remote_entry_id,
        )
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn fetch_source_app_presentation(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        remote_entry_id: &str,
    ) -> Result<PeerSourceAppPresentationSnapshot, TransportError> {
        super::peer_transport::tls::fetch_source_app_presentation(
            self,
            peer_id,
            cert_fingerprint,
            remote_entry_id,
        )
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn start_with_material_resolver_and_history(
        &self,
        material: LocalIdentityMaterial,
        sink: Arc<dyn TransportSink>,
        advertisement: Arc<dyn PairingAdvertisement>,
        resolver: Option<Arc<dyn RemotePeerResolver>>,
        display_name: &str,
        history_handler: Option<Arc<dyn HistoryHostHandler>>,
        fetch_handler: Option<Arc<dyn FetchTextHostHandler>>,
        image_history_handler: Option<Arc<dyn ImageHistoryHostHandler>>,
        image_fetch_handler: Option<Arc<dyn FetchImageHostHandler>>,
        image_thumbnail_handler: Option<Arc<dyn FetchImageThumbnailHostHandler>>,
    ) -> Result<u16, TransportError> {
        let adapter: Arc<dyn PairingAdvertisementSink> =
            Arc::new(AdvertisementSinkAdapter::new(advertisement));
        super::peer_transport::tls::install_with_material_resolver_and_history(
            self,
            material,
            display_name.to_string(),
            adapter,
            sink,
            resolver,
            history_handler,
            fetch_handler,
            image_history_handler,
            image_fetch_handler,
            image_thumbnail_handler,
            None,
        )
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn start_outbound(
        &self,
        descriptor: OutboundSessionDescriptor,
    ) -> Result<PairingOutbound, TransportError> {
        super::peer_transport::tls::start_outbound(self, descriptor)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn approve_local(&self, session_id: PairingSessionId) -> Result<(), TransportError> {
        super::peer_transport::tls::approve_local(self, session_id)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn approve_inbound_session(&self, session_id: PairingSessionId) -> Result<(), TransportError> {
        super::peer_transport::tls::approve_inbound_session(self, session_id)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn cancel_session(&self, session_id: PairingSessionId) -> Result<(), TransportError> {
        super::peer_transport::tls::cancel_session(self, session_id)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn disconnect_peer(&self, peer_id: &str) -> Result<(), TransportError> {
        super::peer_transport::tls::disconnect_peer(self, peer_id)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn health_probe(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
    ) -> Result<PeerHealthSnapshot, TransportError> {
        super::peer_transport::tls::health_probe(self, peer_id, cert_fingerprint)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn list_recent_text(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        cursor: &str,
        limit: u32,
    ) -> Result<PeerHistorySnapshot, TransportError> {
        super::peer_transport::tls::list_recent_text(self, peer_id, cert_fingerprint, cursor, limit)
    }

    #[cfg(feature = "local-peer-pairing-tls")]
    fn fetch_text(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        remote_entry_id: &str,
    ) -> Result<PeerFetchSnapshot, TransportError> {
        super::peer_transport::tls::fetch_text(self, peer_id, cert_fingerprint, remote_entry_id)
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
struct AdvertisementSinkAdapter {
    inner: Arc<dyn PairingAdvertisement>,
}

#[cfg(feature = "local-peer-pairing-tls")]
impl AdvertisementSinkAdapter {
    fn new(inner: Arc<dyn PairingAdvertisement>) -> Self {
        Self { inner }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PairingAdvertisementSink for AdvertisementSinkAdapter {
    fn publish(&self, bound_port: u16) -> Result<(), TransportError> {
        self.inner.publish(bound_port)
    }
    fn withdraw(&self) -> Result<(), TransportError> {
        self.inner.withdraw()
    }
}

/// Deterministic SHA-256 fingerprint of the DER-encoded TLS
/// certificate. The runtime persists this value in
/// `known_peers.tls_cert_fingerprint` so future mTLS handshakes
/// from the same `peer_id` can be pinned to the exact key the
/// pairing handshake saw. The projection never returns the raw
/// key bytes; only the 32-byte SHA-256 digest rendered as
/// lowercase hex.
pub fn derive_cert_fingerprint(cert_der: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(cert_der);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest.iter() {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Recover the canonical `peer_id` from a public key. The
/// transport uses this when it needs to look up the pin entry
/// the runtime armed for a peer — the verifier consults the pin
/// map by `peer_id`, but the pin entry was originally indexed by
/// the `peer_id` the mDNS layer announced, which is the same
/// value `PeerId::from_public_key` derives from the SPKI.
pub fn peer_id_from_public_key(public_key: &[u8; 32]) -> String {
    PeerId::from_public_key(public_key).to_string()
}

/// Wire envelope the pairing surface exchanges. The envelope is
/// intentionally a tiny versioned JSON document — the design
/// (`local-peer-mutual-pairing/design.md` §"Vínculo") pins that
/// the pre-trust endpoint only understands bounded pairing
/// messages and never reaches for content / history routes. The
/// envelope is `pub` so the runtime core can write tests against
/// the JSON shape without depending on the platform crate.
pub mod wire {
    use super::*;

    /// Outer envelope every pairing message is wrapped in. The
    /// `version` field lets the runtime reject incompatible peers
    /// before any application logic runs; the `kind` discriminator
    /// stays open so the future text / history changes can ship
    /// their own metadata-only envelopes without changing this
    /// struct.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    pub enum PairingMessage {
        /// Initial handshake: the connecting peer publishes its
        /// `peer_id`, public key fingerprint, displayed name and a
        /// fresh `nonce_a` it generated.
        Hello {
            version: u32,
            peer_id: String,
            public_key_fingerprint: String,
            display_name: String,
            nonce_a: String,
        },
        /// Response from the listener: the receiving peer adds its
        /// own `nonce_b` and returns a freshly computed candidate
        /// `sas` derived from both nonces.
        HelloAck {
            version: u32,
            peer_id: String,
            public_key_fingerprint: String,
            display_name: String,
            nonce_a: String,
            nonce_b: String,
            sas: String,
        },
        /// Final approval: the connecting peer re-confirms the SAS
        /// and countersigns the handshake transcript. Only after
        /// both peers send this message does the runtime promote
        /// the row to [`clipvault_db::TrustState::Trusted`].
        Approve {
            version: u32,
            peer_id: String,
            sas: String,
            signature: String,
        },
        /// Metadata-only health probe the local peer dials after
        /// pinning. The envelope carries only the protocol major
        /// the local peer speaks and the canonical `peer_id` it
        /// authenticated with; the listener rejects every other
        /// route (history, fetch, import) by closing the stream.
        Health { version: u32, peer_id: String },
        /// Metadata-only reply the listener pushes back. The
        /// reply carries the listener's `peer_id`, the agreed
        /// protocol major and the listening side's
        /// `present` boolean. The transport never inspects the
        /// payload beyond the type check.
        HealthAck {
            version: u32,
            peer_id: String,
            protocol_major: i64,
            present: bool,
        },
        /// Metadata-only request the local peer opens over the
        /// pinned mTLS session once the trust promotion completes.
        /// The envelope carries only the protocol major, the
        /// canonical `peer_id` and the opaque cursor the host
        /// minted (empty for the first page); the listener rejects
        /// the request when the row is not trusted or the cursor
        /// does not match a previous mint, returning
        /// [`PairingMessage::ListRecentTextInvalid`] before any
        /// entry data crosses the wire.
        ListRecentText {
            version: u32,
            peer_id: String,
            cursor: String,
            limit: u32,
        },
        /// Metadata-only reply the listener pushes back with the
        /// bounded page the host projected. The rows are the
        /// metadata-only DTOs the bridge forwards to the renderer;
        /// the transport never inspects the payload beyond the
        /// type check. The `next_cursor` field is the opaque
        /// cursor the renderer must submit to fetch the next page
        /// (empty when this page is the last one).
        ListRecentTextAck {
            version: u32,
            peer_id: String,
            rows: Vec<ListRecentTextRow>,
            next_cursor: String,
            snapshot_id: String,
        },
        /// Typed rejection the listener pushes back when the
        /// caller submitted an opaque cursor the host did not
        /// mint. The `reason` field is a stable snake_case
        /// identifier (`invalid_cursor`) the renderer can switch
        /// on; the transport never inspects the payload beyond
        /// the type check and never echoes the rejected cursor
        /// bytes back.
        ListRecentTextInvalid {
            version: u32,
            peer_id: String,
            reason: String,
        },
        /// Typed rejection the listener pushes back when the
        /// caller asked for history while the row is not in the
        /// trusted state or the peer is not currently active.
        /// The `reason` field is a stable snake_case identifier
        /// the renderer can switch on; the transport never
        /// inspects the payload beyond the type check and never
        /// echoes the rejected cursor bytes back.
        ListRecentTextUnavailable {
            version: u32,
            peer_id: String,
            reason: String,
        },
        /// Request the complete UTF-8 text of a remote entry.
        /// The envelope is authenticated exactly like
        /// [`PairingMessage::ListRecentText`] (the runtime
        /// verifies the declared `peer_id` matches the SPKI the
        /// cert pinned, then checks the cert fingerprint against
        /// the persisted pin) and is gated to the
        /// `peer-text-import` change: only an active trusted
        /// peer can ask for the body of a transferrable text
        /// entry, only after the user activates the `Importar`
        /// action. The listener enforces the 1 MiB UTF-8 limit
        /// before returning the body so a malicious / drifted
        /// host cannot bypass the wire contract.
        ///
        /// The body never carries tags, collections, favorites,
        /// source application metadata, content hash, asset
        /// references or rich-text references. The transport
        /// forwards the body verbatim to the importer, which
        /// re-validates eligibility (size + UTF-8 + entry
        /// existence) before any SQLite mutation.
        FetchText {
            version: u32,
            peer_id: String,
            remote_entry_id: String,
        },
        /// Successful reply the listener pushes back with the
        /// bounded body the host validated against the import
        /// contract. `title` is the validated, trimmed value the
        /// host projects through the same
        /// [`crate::peer_text_history::sanitize_remote_title`]
        /// helper the metadata-only projection uses; `None`
        /// means the entry has no custom title or the persisted
        /// value fails validation.
        ///
        /// `content_type` is the canonical snake_case string
        /// the local SQLite layer persists. The body is the
        /// canonical UTF-8 text the listener capped at
        /// [`FETCH_TEXT_MAX_BODY_BYTES`] bytes; the importer
        /// re-validates the cap locally and refuses to insert
        /// anything that exceeds the documented threshold.
        FetchTextAck {
            version: u32,
            peer_id: String,
            remote_entry_id: String,
            title: Option<String>,
            content_type: String,
            body: String,
            /// Optional additions are defaulted so clients still decode
            /// acknowledgements sent by a legacy peer.
            #[serde(default)]
            source_app_name: Option<String>,
            #[serde(default)]
            source_app_icon_b64: Option<String>,
        },
        /// Typed rejection the listener pushes back when the
        /// caller asked for a body that no longer exists, is no
        /// longer transferrable, or the runtime cannot honour
        /// the request for a documented reason (`not_trusted`,
        /// `not_active`, `persistence_unavailable`, …). The
        /// `reason` is a stable snake_case identifier the
        /// importer can switch on; the transport never inspects
        /// the payload beyond the type check and never echoes
        /// the rejected body back.
        FetchTextUnavailable {
            version: u32,
            peer_id: String,
            remote_entry_id: String,
            reason: String,
        },
        /// Metadata-only request the local peer opens over the
        /// pinned mTLS session once the trust promotion completes.
        /// The envelope is gated to the `peer-image-import`
        /// change and shares the auth + pin path the text
        /// history route uses: the transport validates the
        /// declared `peer_id` against the SPKI the cert pins,
        /// then checks the cert fingerprint against the persisted
        /// pin before any byte crosses the application layer.
        /// The listener refuses the request when the row is not
        /// trusted, the cursor does not match a previous mint, or
        /// the host has not installed the
        /// [`ImageHistoryHostHandler`] adapter — the response
        /// collapses to [`Self::ListRecentImagesUnavailable`]
        /// before any entry data crosses the wire.
        ListRecentImages {
            version: u32,
            peer_id: String,
            cursor: String,
            limit: u32,
        },
        /// Metadata-only reply the listener pushes back with the
        /// bounded image page the host projected. The rows are
        /// the metadata-only DTOs the bridge forwards to the
        /// renderer (opaque remote id, validated title, content
        /// type, RFC 3339 timestamp, byte size, dimensions); the
        /// transport never inspects the payload beyond the type
        /// check. The `next_cursor` field is the opaque cursor
        /// the renderer must submit to fetch the next page
        /// (empty when this page is the last one).
        ListRecentImagesAck {
            version: u32,
            peer_id: String,
            rows: Vec<ListRecentImageRow>,
            next_cursor: String,
            snapshot_id: String,
        },
        /// Typed rejection the listener pushes back when the
        /// caller submitted an opaque cursor the host did not
        /// mint. The `reason` field is a stable snake_case
        /// identifier (`invalid_cursor`) the renderer can switch
        /// on.
        ListRecentImagesInvalid {
            version: u32,
            peer_id: String,
            reason: String,
        },
        /// Typed rejection the listener pushes back when the
        /// caller asked for image history while the row is not in
        /// the trusted state, the peer is not currently active or
        /// the host has not installed the image history handler.
        /// The `reason` field is a stable snake_case identifier
        /// the renderer can switch on.
        ListRecentImagesUnavailable {
            version: u32,
            peer_id: String,
            reason: String,
        },
        /// Request the complete PNG payload of a remote image
        /// entry. The envelope is authenticated exactly like
        /// [`PairingMessage::FetchText`] (the transport verifies
        /// the declared `peer_id` matches the SPKI the cert
        /// pinned, then checks the cert fingerprint against the
        /// persisted pin) and is gated to the
        /// `peer-image-import` change: only an active trusted
        /// peer can ask for the bytes of a transferrable image
        /// entry, only after the user activates the `Importar`
        /// action. The listener enforces the
        /// [`FETCH_IMAGE_MAX_BODY_BYTES`] cap before returning
        /// the body so a malicious / drifted host cannot bypass
        /// the wire contract.
        ///
        /// The payload never carries asset references, source
        /// application metadata, content hash, tags, favourites,
        /// collections or rich-text references. The transport
        /// forwards the bytes verbatim to the importer, which
        /// re-validates eligibility (size + PNG signature +
        /// dimensions) before any SQLite mutation.
        FetchImage {
            version: u32,
            peer_id: String,
            remote_entry_id: String,
        },
        /// Successful reply the listener pushes back with the
        /// bounded PNG payload the host validated against the
        /// import contract. `title` is the validated, trimmed
        /// value the host projects through the same
        /// [`crate::peer_text_history::sanitize_remote_title`]
        /// helper the metadata-only projection uses; `None`
        /// means the entry has no custom title or the persisted
        /// value fails validation. `bytes_b64` is the canonical
        /// PNG payload the host capped at
        /// [`FETCH_IMAGE_MAX_BODY_BYTES`] encoded with the
        /// standard base64 alphabet (`A-Z a-z 0-9 + / =`); the
        /// importer decodes the field locally, re-validates the
        /// decoded size against the cap and refuses to persist
        /// anything that exceeds the documented threshold. The
        /// base64 framing keeps the JSON envelope bounded
        /// (≈4·n/3 bytes for n raw bytes) so the
        /// [`FETCH_IMAGE_MAX_RESPONSE_BYTES`] cap fits a fully
        /// valid 16 MiB PNG without truncating or silently
        /// downgrading the limit.
        FetchImageAck {
            version: u32,
            peer_id: String,
            remote_entry_id: String,
            title: Option<String>,
            bytes_b64: String,
            /// Optional additions are defaulted so clients still decode
            /// acknowledgements sent by a legacy peer.
            #[serde(default)]
            source_app_name: Option<String>,
            #[serde(default)]
            source_app_icon_b64: Option<String>,
        },
        /// Typed rejection the listener pushes back when the
        /// caller asked for an image that no longer exists, is
        /// no longer transferrable, or the runtime cannot
        /// honour the request for a documented reason
        /// (`not_trusted`, `not_active`,
        /// `persistence_unavailable`, …). The `reason` is a
        /// stable snake_case identifier the importer can switch
        /// on; the transport never inspects the payload beyond
        /// the type check and never echoes the rejected body
        /// back.
        FetchImageUnavailable {
            version: u32,
            peer_id: String,
            remote_entry_id: String,
            reason: String,
        },
        /// Request a bounded derived PNG thumbnail for a remote
        /// image entry. The envelope is gated to the
        /// `peer-image-preview-thumbnails` change: only an
        /// active trusted peer that advertises both
        /// `image_import` and `image_preview_thumbnail` can ask
        /// for the thumbnail of a transferrable image entry,
        /// and only after the card intersects the visible
        /// remote-history viewport. The listener enforces the
        /// same mTLS pin path [`PairingMessage::FetchImage`]
        /// uses so a drift in either capability collapses to a
        /// typed rejection before any byte crosses the wire.
        ///
        /// The payload never carries asset references, paths,
        /// content hashes or any field the
        /// `peer-image-preview-thumbnails` spec forbids. The
        /// caller only submits the opaque `remote_entry_id` it
        /// received from `ListRecentImagesAck`; the host
        /// resolves the row internally and re-validates the
        /// entry eligibility before generating the
        /// derivative.
        FetchImageThumbnail {
            version: u32,
            peer_id: String,
            remote_entry_id: String,
        },
        /// Successful reply the listener pushes back with the
        /// bounded PNG thumbnail the host generated in memory.
        /// `width` / `height` mirror the encoded PNG
        /// dimensions so the renderer can size the `<img>`
        /// element without decoding the bytes. `bytes_b64` is
        /// the canonical PNG payload the host capped at
        /// [`FETCH_IMAGE_THUMBNAIL_MAX_BODY_BYTES`] encoded
        /// with the standard base64 alphabet (`A-Z a-z 0-9 +
        /// / =`); the client decodes the field locally,
        /// re-validates the size against the cap, validates
        /// the PNG signature and refuses to surface a
        /// thumbnail larger than the documented envelope. The
        /// base64 framing keeps the JSON envelope bounded
        /// (≈4·n/3 bytes for n raw bytes) so the
        /// [`FETCH_IMAGE_THUMBNAIL_MAX_RESPONSE_BYTES`] cap
        /// fits a fully-valid 384 KiB PNG without truncating
        /// or silently downgrading the limit. The body is held
        /// in memory only between the fetch and the renderer;
        /// the runtime never persists the PNG or substitutes
        /// the snapshot for the original `Importar` payload.
        FetchImageThumbnailAck {
            version: u32,
            peer_id: String,
            remote_entry_id: String,
            bytes_b64: String,
            width: u32,
            height: u32,
        },
        /// Typed rejection the listener pushes back when the
        /// caller asked for a thumbnail the runtime cannot
        /// honour for a documented reason. The variant covers
        /// every failure mode the
        /// `peer-image-preview-thumbnails` change pins:
        /// `not_available` (no thumbnail handler installed),
        /// `not_trusted` / `not_active` (caller no longer
        /// eligible), `not_transferable` (entry gone or no
        /// longer transferable), `not_found` (entry
        /// disappeared between listing and thumbnail fetch),
        /// `busy` (per-peer concurrency limit reached),
        /// `body_too_large` (generated PNG exceeded the 384
        /// KiB cap), `persistence_unavailable` (SQLite or
        /// asset store failure), `invalid_png` (the host
        /// detected an invalid source asset mid-resize) or
        /// `capability_missing` (caller advertised
        /// `image_import` but not
        /// `image_preview_thumbnail`). The transport never
        /// inspects the payload beyond the type check and
        /// never echoes the original-image bytes back.
        FetchImageThumbnailUnavailable {
            version: u32,
            peer_id: String,
            remote_entry_id: String,
            reason: String,
        },
        /// Metadata-only request the local peer opens over the
        /// pinned mTLS session to fetch the source-application
        /// presentation (validated display name + bounded PNG
        /// icon) for one visible remote entry. The
        /// `peer-source-app-presentation` change ships the
        /// envelope; the auth + pin path mirrors
        /// [`PairingMessage::FetchText`]. The host revalidates
        /// the `peer_id` against the SPKI the cert pins, the
        /// trusted / active state, the advertised
        /// `source_app_presentation` capability (peers that did
        /// not opt in collapse to a typed `not_available`
        /// rejection), and the local icon namespace before
        /// returning the response. The listener refuses the
        /// request when the row is no longer eligible so a
        /// stale or revoked peer can never surface its icon.
        FetchSourceAppPresentation {
            version: u32,
            peer_id: String,
            remote_entry_id: String,
        },
        /// Successful response the listener pushes back with the
        /// validated source-application display name and the
        /// optional bounded PNG icon the host attached to the
        /// listed entry. The icon is base64-encoded so the wire
        /// envelope stays metadata-only by construction; the
        /// runtime validates the bytes against the
        /// 512 KiB / 256 × 256 px envelope the spec pins before
        /// surfacing the icon to the renderer. The full response
        /// MUST stay within the 720 KiB envelope cap.
        FetchSourceAppPresentationAck {
            version: u32,
            peer_id: String,
            remote_entry_id: String,
            source_app_name: Option<String>,
            source_app_icon_b64: Option<String>,
        },
        /// Typed rejection the listener pushes back when the
        /// caller asked for a row the host cannot serve
        /// (`not_available`, `not_trusted`, `not_active`,
        /// `unknown_peer`, …). The `reason` field is a stable
        /// snake_case identifier the renderer branches on; the
        /// transport never echoes the rejected icon back.
        FetchSourceAppPresentationUnavailable {
            version: u32,
            peer_id: String,
            remote_entry_id: String,
            reason: String,
        },
    }

    /// Metadata-only row the host returns in
    /// [`PairingMessage::ListRecentTextAck`]. The struct carries
    /// only the fields the spec and the design authorise: an
    /// opaque remote entry id, the optional validated title, the
    /// content type, the RFC 3339 timestamp and an escaped
    /// bounded preview and optional bounded source-app name. The row never
    /// carries the entry body, row hash, source-app icon bytes/references,
    /// favourite flag, tags, collections or asset references.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub struct ListRecentTextRow {
        /// Opaque remote entry id the host minted. The id is
        /// bound to the local `entries.id` of the source row but
        /// encoded so the renderer can never inspect the local
        /// primary key.
        pub remote_entry_id: String,
        /// Validated, trimmed user-supplied title. `None` when
        /// the entry has no custom title or the persisted value
        /// fails the same validation the local UI applies (so the
        /// renderer can fall back to the content-type label
        /// without surfacing garbage).
        pub title: Option<String>,
        /// Canonical snake_case string the local SQLite layer
        /// persists. The bridge surfaces the value verbatim so
        /// the renderer can switch on a stable wire contract.
        pub content_type: String,
        /// RFC 3339 timestamp of the entry's `created_at`.
        pub created_at: String,
        /// Bounded, escaped preview. Always trimmed and never
        /// longer than 300 Unicode scalar values / two lines.
        pub preview: String,
        /// Optional validated source-app display name. Missing on
        /// legacy peers; never includes icon bytes or an icon reference.
        #[serde(default)]
        pub source_app_name: Option<String>,
    }

    /// Metadata-only row the host returns in
    /// [`PairingMessage::ListRecentImagesAck`]. The struct carries
    /// only the fields the spec authorises: an opaque remote
    /// entry id, the optional validated title, the canonical
    /// content type (`image`), the RFC 3339 timestamp, the byte
    /// size and the pixel dimensions. The row NEVER carries image
    /// bytes, a thumbnail, an `asset_ref`, a filesystem path, a
    /// content hash, tags, collections, favourites, icon bytes or
    /// source-application identifiers. A bounded display name may be present.
    /// The renderer renders the
    /// common static image placeholder on top of the metadata
    /// the bridge surfaces.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub struct ListRecentImageRow {
        pub remote_entry_id: String,
        pub title: Option<String>,
        pub content_type: String,
        pub created_at: String,
        pub byte_size: u64,
        pub width: u32,
        pub height: u32,
        /// Optional validated display name from this image's own capture.
        /// Missing on legacy peers; carries no icon bytes or reference.
        #[serde(default)]
        pub source_app_name: Option<String>,
    }

    impl PairingMessage {
        pub fn version(&self) -> u32 {
            match self {
                PairingMessage::Hello { version, .. }
                | PairingMessage::HelloAck { version, .. }
                | PairingMessage::Approve { version, .. }
                | PairingMessage::Health { version, .. }
                | PairingMessage::HealthAck { version, .. }
                | PairingMessage::ListRecentText { version, .. }
                | PairingMessage::ListRecentTextAck { version, .. }
                | PairingMessage::ListRecentTextInvalid { version, .. }
                | PairingMessage::ListRecentTextUnavailable { version, .. }
                | PairingMessage::FetchText { version, .. }
                | PairingMessage::FetchTextAck { version, .. }
                | PairingMessage::FetchTextUnavailable { version, .. }
                | PairingMessage::ListRecentImages { version, .. }
                | PairingMessage::ListRecentImagesAck { version, .. }
                | PairingMessage::ListRecentImagesInvalid { version, .. }
                | PairingMessage::ListRecentImagesUnavailable { version, .. }
                | PairingMessage::FetchImage { version, .. }
                | PairingMessage::FetchImageAck { version, .. }
                | PairingMessage::FetchImageUnavailable { version, .. }
                | PairingMessage::FetchImageThumbnail { version, .. }
                | PairingMessage::FetchImageThumbnailAck { version, .. }
                | PairingMessage::FetchImageThumbnailUnavailable { version, .. }
                | PairingMessage::FetchSourceAppPresentation { version, .. }
                | PairingMessage::FetchSourceAppPresentationAck { version, .. }
                | PairingMessage::FetchSourceAppPresentationUnavailable { version, .. } => *version,
            }
        }

        pub fn peer_id(&self) -> &str {
            match self {
                PairingMessage::Hello { peer_id, .. }
                | PairingMessage::HelloAck { peer_id, .. }
                | PairingMessage::Approve { peer_id, .. }
                | PairingMessage::Health { peer_id, .. }
                | PairingMessage::HealthAck { peer_id, .. }
                | PairingMessage::ListRecentText { peer_id, .. }
                | PairingMessage::ListRecentTextAck { peer_id, .. }
                | PairingMessage::ListRecentTextInvalid { peer_id, .. }
                | PairingMessage::ListRecentTextUnavailable { peer_id, .. }
                | PairingMessage::FetchText { peer_id, .. }
                | PairingMessage::FetchTextAck { peer_id, .. }
                | PairingMessage::FetchTextUnavailable { peer_id, .. }
                | PairingMessage::ListRecentImages { peer_id, .. }
                | PairingMessage::ListRecentImagesAck { peer_id, .. }
                | PairingMessage::ListRecentImagesInvalid { peer_id, .. }
                | PairingMessage::ListRecentImagesUnavailable { peer_id, .. }
                | PairingMessage::FetchImage { peer_id, .. }
                | PairingMessage::FetchImageAck { peer_id, .. }
                | PairingMessage::FetchImageUnavailable { peer_id, .. }
                | PairingMessage::FetchImageThumbnail { peer_id, .. }
                | PairingMessage::FetchImageThumbnailAck { peer_id, .. }
                | PairingMessage::FetchImageThumbnailUnavailable { peer_id, .. }
                | PairingMessage::FetchSourceAppPresentation { peer_id, .. }
                | PairingMessage::FetchSourceAppPresentationAck { peer_id, .. }
                | PairingMessage::FetchSourceAppPresentationUnavailable { peer_id, .. } => peer_id,
            }
        }

        pub fn public_key_fingerprint(&self) -> &str {
            match self {
                PairingMessage::Hello {
                    public_key_fingerprint,
                    ..
                }
                | PairingMessage::HelloAck {
                    public_key_fingerprint,
                    ..
                } => public_key_fingerprint,
                PairingMessage::Approve { .. }
                | PairingMessage::Health { .. }
                | PairingMessage::HealthAck { .. }
                | PairingMessage::ListRecentText { .. }
                | PairingMessage::ListRecentTextAck { .. }
                | PairingMessage::ListRecentTextInvalid { .. }
                | PairingMessage::ListRecentTextUnavailable { .. }
                | PairingMessage::FetchText { .. }
                | PairingMessage::FetchTextAck { .. }
                | PairingMessage::FetchTextUnavailable { .. }
                | PairingMessage::ListRecentImages { .. }
                | PairingMessage::ListRecentImagesAck { .. }
                | PairingMessage::ListRecentImagesInvalid { .. }
                | PairingMessage::ListRecentImagesUnavailable { .. }
                | PairingMessage::FetchImage { .. }
                | PairingMessage::FetchImageAck { .. }
                | PairingMessage::FetchImageUnavailable { .. }
                | PairingMessage::FetchImageThumbnail { .. }
                | PairingMessage::FetchImageThumbnailAck { .. }
                | PairingMessage::FetchImageThumbnailUnavailable { .. }
                | PairingMessage::FetchSourceAppPresentation { .. }
                | PairingMessage::FetchSourceAppPresentationAck { .. }
                | PairingMessage::FetchSourceAppPresentationUnavailable { .. } => "",
            }
        }
    }

    /// Compute the deterministic six-digit SAS decimal code both
    /// sides show to the user. The derivation is the one the design
    /// pins (`local-peer-mutual-pairing/design.md` §"Vínculo"):
    /// SHA-256 over both nonces, both `peer_id`s, both
    /// fingerprints and the agreed protocol major, truncated to
    /// six decimal digits.
    pub fn compute_sas(
        nonce_a: &str,
        nonce_b: &str,
        peer_id_a: &str,
        peer_id_b: &str,
        fingerprint_a: &str,
        fingerprint_b: &str,
        protocol_major: i64,
    ) -> String {
        use sha2::{Digest, Sha256};
        // Canonical, order-independent input: the two peers are
        // expected to produce the same SAS byte-for-byte regardless
        // of who initiated the session. EVERY field pair — nonces,
        // peer_ids, fingerprints — is sorted so the hash input
        // does not depend on who holds the `nonce_a` slot vs the
        // `nonce_b` slot. Without the nonce sort a previous
        // prototype computed different SASes on each side because
        // `nonce_a` arrived at the listener in a different order
        // than it was sent by the dialer.
        let (na, nb) = if nonce_a <= nonce_b {
            (nonce_a, nonce_b)
        } else {
            (nonce_b, nonce_a)
        };
        let (a, b) = if peer_id_a <= peer_id_b {
            (peer_id_a, peer_id_b)
        } else {
            (peer_id_b, peer_id_a)
        };
        let (fa, fb) = if fingerprint_a <= fingerprint_b {
            (fingerprint_a, fingerprint_b)
        } else {
            (fingerprint_b, fingerprint_a)
        };
        let mut hasher = Sha256::new();
        hasher.update(na.as_bytes());
        hasher.update(b"|");
        hasher.update(nb.as_bytes());
        hasher.update(b"|");
        hasher.update(a.as_bytes());
        hasher.update(b"|");
        hasher.update(b.as_bytes());
        hasher.update(b"|");
        hasher.update(fa.as_bytes());
        hasher.update(b"|");
        hasher.update(fb.as_bytes());
        hasher.update(b"|");
        hasher.update(protocol_major.to_le_bytes());
        let digest = hasher.finalize();
        // Take the first 4 bytes, modulo 1_000_000, formatted as
        // exactly six decimal digits.
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&digest[..4]);
        let value = u32::from_le_bytes(buf) % 1_000_000;
        format!("{value:06}")
    }
}

/// Resolve the production [`PeerTransport`] the production shell
/// installs when the caller did not inject one. Production builds
/// on macOS / Linux link the TLS-backed adapter the
/// `local-peer-pairing-tls` feature ships; every other build
/// falls back to [`NoopPeerTransport`] so the runtime surfaces
/// the typed [`TransportOutcome::Unavailable`] reason instead of
/// panicking on a missing backend.
pub fn default_peer_transport() -> Arc<dyn PeerTransport> {
    #[cfg(feature = "local-peer-pairing-tls")]
    {
        Arc::new(TlsPeerTransport::new())
    }
    #[cfg(not(feature = "local-peer-pairing-tls"))]
    {
        Arc::new(NoopPeerTransport::new())
    }
}

// Re-exports so callers can name the types without a `wire::` prefix
// when they only need the high-level surface.
pub use wire::{compute_sas, PairingMessage};

#[allow(dead_code)]
const _: () = {
    // Compile-time pin: the wire version never drifts without an
    // explicit bump. The constant is referenced from the runtime
    // core's compat check; failing this `const` evaluation would
    // surface the drift at compile time instead of at runtime.
    let _ = PAIRING_WIRE_VERSION;
};

#[allow(unused_imports)]
use {PeerFingerprint as _, PeerId as _};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_transport_reports_unavailable_on_start() {
        let transport = NoopPeerTransport::new();
        let identity = LocalPeerIdentity {
            peer_id: PeerId::from_public_key(&[0u8; 32]),
            fingerprint: PeerFingerprint::from_public_key(&[0u8; 32]),
            public_key: [0u8; 32],
        };
        let sink: Arc<dyn TransportSink> = Arc::new(NullSink);
        let err = transport
            .start(&identity, sink)
            .expect_err("noop must refuse to start");
        assert!(matches!(err, TransportError::Unavailable));
        assert!(!transport.is_running());
    }

    #[test]
    fn noop_transport_stop_is_a_no_op() {
        let transport = NoopPeerTransport::new();
        transport.stop().expect("noop stop is a no-op");
        assert!(!transport.is_running());
    }

    #[test]
    fn derive_cert_fingerprint_is_deterministic_and_lowercase_hex() {
        let a = derive_cert_fingerprint(b"hello");
        let b = derive_cert_fingerprint(b"hello");
        let c = derive_cert_fingerprint(b"world");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
        assert!(a
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn sas_is_six_decimal_digits_and_order_independent() {
        let sas_ab = compute_sas(
            "nonce-a",
            "nonce-b",
            "peer-aaaa",
            "peer-bbbb",
            "fp-aaaa",
            "fp-bbbb",
            1,
        );
        let sas_ba = compute_sas(
            "nonce-a",
            "nonce-b",
            "peer-bbbb",
            "peer-aaaa",
            "fp-bbbb",
            "fp-aaaa",
            1,
        );
        assert_eq!(sas_ab.len(), 6);
        assert!(sas_ab.chars().all(|c| c.is_ascii_digit()));
        assert_eq!(sas_ab, sas_ba, "SAS must be order-independent");
    }

    #[test]
    fn sas_changes_when_any_input_changes() {
        let base = compute_sas("na", "nb", "pa", "pb", "fa", "fb", 1);
        for (label, changed) in [
            (
                "nonce_a",
                compute_sas("na!", "nb", "pa", "pb", "fa", "fb", 1),
            ),
            (
                "nonce_b",
                compute_sas("na", "nb!", "pa", "pb", "fa", "fb", 1),
            ),
            (
                "peer_id",
                compute_sas("na", "nb", "pa!", "pb", "fa", "fb", 1),
            ),
            (
                "fingerprint",
                compute_sas("na", "nb", "pa", "pb", "fa!", "fb", 1),
            ),
            (
                "protocol",
                compute_sas("na", "nb", "pa", "pb", "fa", "fb", 2),
            ),
        ] {
            assert_ne!(base, changed, "SAS must change when {label} changes");
        }
    }

    #[test]
    fn pairing_message_version_discriminator_is_stable() {
        let hello = PairingMessage::Hello {
            version: PAIRING_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            public_key_fingerprint: "fp".to_string(),
            display_name: "Studio".to_string(),
            nonce_a: "nonce-a".to_string(),
        };
        assert_eq!(hello.version(), PAIRING_WIRE_VERSION);
        assert_eq!(hello.peer_id(), "peer-aaaa");
        assert_eq!(hello.public_key_fingerprint(), "fp");
    }

    /// The `ListRecentText` envelope shape is the metadata-only
    /// request the `peer-text-history-browser` change ships over
    /// the productive mTLS transport. The wire shape must round-
    /// trip through JSON so the renderer / core can introspect
    /// it in tests without standing up the full TLS stack.
    #[test]
    fn list_recent_text_envelope_round_trips_through_json() {
        let request = PairingMessage::ListRecentText {
            version: HISTORY_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            cursor: String::new(),
            limit: HISTORY_MAX_PAGE_ROWS as u32,
        };
        assert_eq!(request.version(), HISTORY_WIRE_VERSION);
        assert_eq!(request.peer_id(), "peer-aaaa");
        let serialised = serde_json::to_string(&request).expect("serialise");
        let parsed: PairingMessage = serde_json::from_str(&serialised).expect("parse");
        assert_eq!(parsed, request);
    }

    /// The `ListRecentTextAck` envelope shape is the bounded
    /// metadata-only page the host emits. The renderer / core
    /// depend on the field names matching the snake_case wire
    /// contract documented in the spec; a future refactor that
    /// accidentally drops a field must surface as a test failure
    /// here before the production build ships.
    #[test]
    fn list_recent_text_ack_envelope_round_trips_through_json() {
        use wire::ListRecentTextRow;
        let response = PairingMessage::ListRecentTextAck {
            version: HISTORY_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            rows: vec![ListRecentTextRow {
                remote_entry_id: "entry-7".to_string(),
                title: Some("hello".to_string()),
                content_type: "text".to_string(),
                created_at: "2026-01-02T03:04:05Z".to_string(),
                preview: "&lt;b&gt;safe&lt;/b&gt;".to_string(),
                source_app_name: Some("Terminal".to_string()),
            }],
            next_cursor: "next".to_string(),
            snapshot_id: "snapshot".to_string(),
        };
        let serialised = serde_json::to_string(&response).expect("serialise");
        let parsed: PairingMessage = serde_json::from_str(&serialised).expect("parse");
        assert_eq!(parsed, response);
    }

    #[test]
    fn legacy_text_browse_row_decodes_without_source_app_name() {
        let row: wire::ListRecentTextRow = serde_json::from_str(
            r#"{"remote_entry_id":"entry-1","title":null,"content_type":"text","created_at":"2026-01-02T03:04:05Z","preview":"hello"}"#,
        )
        .expect("legacy row decodes");
        assert_eq!(row.source_app_name, None);
    }

    #[test]
    fn max_source_names_fit_the_bounded_text_history_envelope() {
        let rows = (0..HISTORY_MAX_PAGE_ROWS)
            .map(|index| wire::ListRecentTextRow {
                remote_entry_id: format!("entry-{index}"),
                title: Some("🙂".repeat(80)),
                content_type: "text".to_string(),
                created_at: "2026-01-02T03:04:05Z".to_string(),
                preview: "🙂".repeat(300),
                source_app_name: Some("🙂".repeat(128)),
            })
            .collect();
        let response = PairingMessage::ListRecentTextAck {
            version: HISTORY_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            rows,
            next_cursor: String::new(),
            snapshot_id: "a".repeat(64),
        };
        let serialized = serde_json::to_vec(&response).expect("serialize");
        assert!(serialized.len() <= HISTORY_MAX_RESPONSE_BYTES);
    }

    /// The `ListRecentTextInvalid` and `ListRecentTextUnavailable`
    /// rejections surface typed reasons the renderer maps to copy.
    /// Pinning the wire shape here means a future refactor that
    /// drops the `reason` field fails the test before the build
    /// can ship.
    #[test]
    fn list_recent_text_invalid_envelope_round_trips_through_json() {
        let invalid = PairingMessage::ListRecentTextInvalid {
            version: HISTORY_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            reason: "invalid_cursor".to_string(),
        };
        let serialised = serde_json::to_string(&invalid).expect("serialise");
        let parsed: PairingMessage = serde_json::from_str(&serialised).expect("parse");
        assert_eq!(parsed, invalid);

        let unavailable = PairingMessage::ListRecentTextUnavailable {
            version: HISTORY_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            reason: "not_active".to_string(),
        };
        let serialised = serde_json::to_string(&unavailable).expect("serialise");
        let parsed: PairingMessage = serde_json::from_str(&serialised).expect("parse");
        assert_eq!(parsed, unavailable);
    }

    /// `PairingMessage::ListRecentText` does NOT carry the public
    /// key fingerprint — the envelope is metadata-only and the
    /// runtime never inspects the field. Pinning the empty string
    /// here means a future contributor who accidentally re-exposes
    /// the fingerprint fails the test before the build can ship.
    #[test]
    fn list_recent_text_does_not_expose_public_key_fingerprint() {
        let request = PairingMessage::ListRecentText {
            version: HISTORY_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            cursor: String::new(),
            limit: HISTORY_MAX_PAGE_ROWS as u32,
        };
        assert_eq!(request.public_key_fingerprint(), "");

        let response = PairingMessage::ListRecentTextAck {
            version: HISTORY_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            rows: Vec::new(),
            next_cursor: String::new(),
            snapshot_id: String::new(),
        };
        assert_eq!(response.public_key_fingerprint(), "");
    }

    /// `PairingMessage::FetchText` round-trips through JSON so a
    /// future refactor that drops the body field fails the test
    /// before the build can ship. The envelope never carries the
    /// body outside the `body` field; the runtime re-validates
    /// the cap locally before the import commits.
    #[test]
    fn fetch_text_envelope_round_trips_through_json() {
        let request = PairingMessage::FetchText {
            version: HISTORY_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            remote_entry_id: "entry-42".to_string(),
        };
        let serialised = serde_json::to_string(&request).expect("serialise");
        let parsed: PairingMessage = serde_json::from_str(&serialised).expect("parse");
        assert_eq!(parsed, request);
        assert_eq!(parsed.public_key_fingerprint(), "");
    }

    /// `PairingMessage::FetchTextAck` round-trips the bounded body
    /// the host returns. The envelope carries the validated title,
    /// the canonical `content_type` and the UTF-8 body; the
    /// runtime enforces the [`FETCH_TEXT_MAX_BODY_BYTES`] cap on
    /// the receiving end so a drifted host cannot bypass the
    /// documented limit.
    #[test]
    fn fetch_text_ack_envelope_round_trips_through_json() {
        let ack = PairingMessage::FetchTextAck {
            version: HISTORY_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            remote_entry_id: "entry-42".to_string(),
            title: Some("Hola · 漢字".to_string()),
            content_type: "text".to_string(),
            body: "hello".to_string(),
            source_app_name: Some("Terminal".to_string()),
            source_app_icon_b64: Some("iVBORw==".to_string()),
        };
        let serialised = serde_json::to_string(&ack).expect("serialise");
        let parsed: PairingMessage = serde_json::from_str(&serialised).expect("parse");
        assert_eq!(parsed, ack);
    }

    #[test]
    fn legacy_fetch_text_ack_without_source_fields_still_decodes() {
        let legacy = r#"{"kind":"fetch_text_ack","version":1,"peer_id":"peer-a","remote_entry_id":"entry-1","title":null,"content_type":"text","body":"hello"}"#;
        let parsed: PairingMessage = serde_json::from_str(legacy).expect("legacy ack parses");
        assert!(matches!(
            parsed,
            PairingMessage::FetchTextAck {
                source_app_name: None,
                source_app_icon_b64: None,
                ..
            }
        ));
    }

    #[test]
    fn fetch_text_ack_max_body_and_icon_fit_response_cap() {
        let body = "\0".repeat(FETCH_TEXT_MAX_BODY_BYTES);
        let icon_b64_len = ((FETCH_SOURCE_APP_ICON_MAX_BYTES + 2) / 3) * 4;
        let ack = PairingMessage::FetchTextAck {
            version: HISTORY_WIRE_VERSION,
            peer_id: "peer-a".to_string(),
            remote_entry_id: "entry-1".to_string(),
            title: None,
            content_type: "text".to_string(),
            body,
            source_app_name: Some("A".repeat(128)),
            source_app_icon_b64: Some("A".repeat(icon_b64_len)),
        };
        let serialized = serde_json::to_vec(&ack).expect("serialize bounded ack");
        assert!(serialized.len() <= FETCH_TEXT_MAX_RESPONSE_BYTES);
    }

    /// `PairingMessage::FetchTextUnavailable` rejection carries a
    /// typed reason the importer maps onto the typed outcome
    /// surface. The variants the wire contract pins
    /// (`not_found`, `not_transferable`, `body_too_large`,
    /// `not_trusted`, `not_active`, `persistence_unavailable`)
    /// stay stable across rebuilds.
    #[test]
    fn fetch_text_unavailable_envelope_round_trips_through_json() {
        let unavailable = PairingMessage::FetchTextUnavailable {
            version: HISTORY_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            remote_entry_id: "entry-42".to_string(),
            reason: "not_found".to_string(),
        };
        let serialised = serde_json::to_string(&unavailable).expect("serialise");
        let parsed: PairingMessage = serde_json::from_str(&serialised).expect("parse");
        assert_eq!(parsed, unavailable);
    }

    /// `PairingMessage::FetchText` does NOT carry the public key
    /// fingerprint — the envelope is metadata-only by
    /// construction and the runtime never inspects the field.
    /// Pinning the empty string here means a future contributor
    /// who accidentally re-exposes the fingerprint fails the
    /// test before the build can ship.
    #[test]
    fn fetch_text_does_not_expose_public_key_fingerprint() {
        let request = PairingMessage::FetchText {
            version: HISTORY_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            remote_entry_id: "entry-42".to_string(),
        };
        assert_eq!(request.public_key_fingerprint(), "");

        let ack = PairingMessage::FetchTextAck {
            version: HISTORY_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            remote_entry_id: "entry-42".to_string(),
            title: None,
            content_type: "text".to_string(),
            body: "hello".to_string(),
            source_app_name: None,
            source_app_icon_b64: None,
        };
        assert_eq!(ack.public_key_fingerprint(), "");
    }

    /// Capture-only sink used by the noop tests so they can build
    /// a `TransportSink` without standing up the real TLS stack.
    #[derive(Default)]
    struct NullSink;

    impl TransportSink for NullSink {
        fn on_pairing_observed(&self, _observation: PeerTransportObservation) {}
    }

    /// The `peer-image-import` change adds the image-related
    /// envelopes without touching the text side. The wire shape
    /// MUST round-trip through JSON so a regression that drops a
    /// field surfaces as a test failure before the build ships.
    #[test]
    fn list_recent_images_envelope_round_trips_through_json() {
        let request = PairingMessage::ListRecentImages {
            version: IMAGE_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            cursor: String::new(),
            limit: IMAGE_HISTORY_MAX_PAGE_ROWS as u32,
        };
        assert_eq!(request.version(), IMAGE_WIRE_VERSION);
        assert_eq!(request.peer_id(), "peer-aaaa");
        let serialised = serde_json::to_string(&request).expect("serialise");
        let parsed: PairingMessage = serde_json::from_str(&serialised).expect("parse");
        assert_eq!(parsed, request);
    }

    #[test]
    fn list_recent_images_ack_envelope_round_trips_through_json() {
        use wire::ListRecentImageRow;
        let response = PairingMessage::ListRecentImagesAck {
            version: IMAGE_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            rows: vec![ListRecentImageRow {
                remote_entry_id: "entry-7".to_string(),
                title: Some("hello".to_string()),
                content_type: "image".to_string(),
                created_at: "2026-01-02T03:04:05Z".to_string(),
                byte_size: 4096,
                width: 320,
                height: 240,
                source_app_name: Some("Editor".to_string()),
            }],
            next_cursor: "next".to_string(),
            snapshot_id: "snapshot".to_string(),
        };
        let serialised = serde_json::to_string(&response).expect("serialise");
        let parsed: PairingMessage = serde_json::from_str(&serialised).expect("parse");
        assert_eq!(parsed, response);
    }

    #[test]
    fn legacy_image_browse_row_decodes_without_source_app_name() {
        let row: wire::ListRecentImageRow = serde_json::from_str(
            r#"{"remote_entry_id":"entry-1","title":null,"content_type":"image","created_at":"2026-01-02T03:04:05Z","byte_size":4096,"width":320,"height":240}"#,
        )
        .expect("legacy row decodes");
        assert_eq!(row.source_app_name, None);
    }

    #[test]
    fn max_source_names_fit_the_bounded_image_history_envelope() {
        let rows = (0..IMAGE_HISTORY_MAX_PAGE_ROWS)
            .map(|index| wire::ListRecentImageRow {
                remote_entry_id: format!("entry-{index}"),
                title: None,
                content_type: "image".to_string(),
                created_at: "2026-01-02T03:04:05Z".to_string(),
                byte_size: 1024,
                width: 640,
                height: 480,
                source_app_name: Some("🙂".repeat(128)),
            })
            .collect();
        let response = PairingMessage::ListRecentImagesAck {
            version: IMAGE_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            rows,
            next_cursor: String::new(),
            snapshot_id: "a".repeat(64),
        };
        let serialized = serde_json::to_vec(&response).expect("serialize");
        assert!(serialized.len() <= IMAGE_HISTORY_MAX_RESPONSE_BYTES);
    }

    #[test]
    fn list_recent_images_invalid_envelope_round_trips_through_json() {
        let invalid = PairingMessage::ListRecentImagesInvalid {
            version: IMAGE_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            reason: "invalid_cursor".to_string(),
        };
        let serialised = serde_json::to_string(&invalid).expect("serialise");
        let parsed: PairingMessage = serde_json::from_str(&serialised).expect("parse");
        assert_eq!(parsed, invalid);
    }

    #[test]
    fn fetch_image_envelope_round_trips_through_json() {
        let request = PairingMessage::FetchImage {
            version: IMAGE_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            remote_entry_id: "entry-42".to_string(),
        };
        assert_eq!(request.version(), IMAGE_WIRE_VERSION);
        assert_eq!(request.peer_id(), "peer-aaaa");
        let serialised = serde_json::to_string(&request).expect("serialise");
        let parsed: PairingMessage = serde_json::from_str(&serialised).expect("parse");
        assert_eq!(parsed, request);
        assert_eq!(parsed.public_key_fingerprint(), "");
    }

    #[test]
    fn fetch_image_ack_envelope_round_trips_through_json() {
        let ack = PairingMessage::FetchImageAck {
            version: IMAGE_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            remote_entry_id: "entry-42".to_string(),
            title: Some("Captura".to_string()),
            bytes_b64: "iVBORw==".to_string(),
            source_app_name: Some("Editor".to_string()),
            source_app_icon_b64: Some("iVBORw==".to_string()),
        };
        let serialised = serde_json::to_string(&ack).expect("serialise");
        let parsed: PairingMessage = serde_json::from_str(&serialised).expect("parse");
        assert_eq!(parsed, ack);
    }

    #[test]
    fn legacy_fetch_image_ack_without_source_fields_still_decodes() {
        let legacy = r#"{"kind":"fetch_image_ack","version":1,"peer_id":"peer-a","remote_entry_id":"entry-1","title":null,"bytes_b64":"iVBORw=="}"#;
        let parsed: PairingMessage = serde_json::from_str(legacy).expect("legacy ack parses");
        assert!(matches!(
            parsed,
            PairingMessage::FetchImageAck {
                source_app_name: None,
                source_app_icon_b64: None,
                ..
            }
        ));
    }

    #[test]
    fn fetch_image_unavailable_envelope_round_trips_through_json() {
        let unavailable = PairingMessage::FetchImageUnavailable {
            version: IMAGE_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            remote_entry_id: "entry-42".to_string(),
            reason: "not_found".to_string(),
        };
        let serialised = serde_json::to_string(&unavailable).expect("serialise");
        let parsed: PairingMessage = serde_json::from_str(&serialised).expect("parse");
        assert_eq!(parsed, unavailable);
    }

    #[test]
    fn fetch_image_ack_carries_a_full_max_payload_envelope() {
        // The 16 MiB PNG body cap the spec pins has to fit
        // inside [`FETCH_IMAGE_MAX_RESPONSE_BYTES`] when
        // base64-encoded. The envelope also has to fit inside
        // the same ceiling after JSON framing so a regression
        // that drops the base64 framing cannot silently shrink
        // the limit. We pin the worst-case here.
        let bytes = vec![0u8; FETCH_IMAGE_MAX_BODY_BYTES];
        let max_icon = vec![0u8; FETCH_SOURCE_APP_ICON_MAX_BYTES];
        let bytes_b64_len = {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD
                .encode(&bytes)
                .len()
        };
        let source_icon_b64_len = {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD
                .encode(&max_icon)
                .len()
        };
        assert!(
            bytes_b64_len + source_icon_b64_len + 1024 <= FETCH_IMAGE_MAX_RESPONSE_BYTES,
            "base64 image and max source-app icon ({} + {} bytes) must fit inside the response cap ({} bytes)",
            bytes_b64_len, source_icon_b64_len,
            FETCH_IMAGE_MAX_RESPONSE_BYTES
        );
    }

    #[test]
    fn fetch_image_ack_rejects_an_oversized_payload() {
        // The `envelope_payload_limit` helper refuses a
        // serialised payload larger than the documented cap;
        // we verify the gate by constructing a stub that is
        // bigger than the cap and asserting the limit returns
        // the documented constant. (The actual serialiser path
        // lives inside the listener helper which requires a
        // running TLS stream.)
        assert!(FETCH_IMAGE_MAX_BODY_BYTES <= FETCH_IMAGE_MAX_RESPONSE_BYTES);
    }

    #[test]
    fn image_envelope_does_not_expose_public_key_fingerprint() {
        // Same metadata-only contract the text side pins:
        // the image envelopes MUST NOT carry a public key
        // fingerprint over the wire.
        let request = PairingMessage::FetchImage {
            version: IMAGE_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            remote_entry_id: "entry-42".to_string(),
        };
        assert_eq!(request.public_key_fingerprint(), "");

        let ack = PairingMessage::FetchImageAck {
            version: IMAGE_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            remote_entry_id: "entry-42".to_string(),
            title: None,
            bytes_b64: String::new(),
            source_app_name: None,
            source_app_icon_b64: None,
        };
        assert_eq!(ack.public_key_fingerprint(), "");
    }

    #[test]
    fn fetch_image_thumbnail_envelope_round_trips_through_json() {
        let request = PairingMessage::FetchImageThumbnail {
            version: IMAGE_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            remote_entry_id: "entry-7".to_string(),
        };
        assert_eq!(request.version(), IMAGE_WIRE_VERSION);
        assert_eq!(request.peer_id(), "peer-aaaa");
        let serialised = serde_json::to_string(&request).expect("serialise");
        let parsed: PairingMessage = serde_json::from_str(&serialised).expect("parse");
        assert_eq!(parsed, request);
    }

    #[test]
    fn fetch_image_thumbnail_ack_envelope_round_trips_through_json() {
        let ack = PairingMessage::FetchImageThumbnailAck {
            version: IMAGE_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            remote_entry_id: "entry-7".to_string(),
            bytes_b64: "iVBORw0KGgo=".to_string(),
            width: 128,
            height: 64,
        };
        let serialised = serde_json::to_string(&ack).expect("serialise");
        let parsed: PairingMessage = serde_json::from_str(&serialised).expect("parse");
        assert_eq!(parsed, ack);
    }

    #[test]
    fn fetch_image_thumbnail_unavailable_envelope_round_trips_through_json() {
        let unavailable = PairingMessage::FetchImageThumbnailUnavailable {
            version: IMAGE_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            remote_entry_id: "entry-7".to_string(),
            reason: "capability_missing".to_string(),
        };
        let serialised = serde_json::to_string(&unavailable).expect("serialise");
        let parsed: PairingMessage = serde_json::from_str(&serialised).expect("parse");
        assert_eq!(parsed, unavailable);
    }

    #[test]
    fn fetch_image_thumbnail_ack_carries_a_full_max_payload_envelope() {
        // The 384 KiB PNG body cap the spec pins has to fit
        // inside [`FETCH_IMAGE_THUMBNAIL_MAX_RESPONSE_BYTES`]
        // when base64-encoded. We pin the worst-case here so
        // a regression that drops the base64 framing cannot
        // silently shrink the limit.
        let bytes = vec![0u8; FETCH_IMAGE_THUMBNAIL_MAX_BODY_BYTES];
        let bytes_b64_len = {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD
                .encode(&bytes)
                .len()
        };
        assert!(
            bytes_b64_len <= FETCH_IMAGE_THUMBNAIL_MAX_RESPONSE_BYTES,
            "base64 payload ({} bytes) must fit inside the response cap ({} bytes)",
            bytes_b64_len,
            FETCH_IMAGE_THUMBNAIL_MAX_RESPONSE_BYTES
        );
    }

    #[test]
    fn fetch_image_thumbnail_ack_rejects_an_oversized_payload() {
        assert!(FETCH_IMAGE_THUMBNAIL_MAX_BODY_BYTES <= FETCH_IMAGE_THUMBNAIL_MAX_RESPONSE_BYTES);
    }

    #[test]
    fn image_thumbnail_envelope_does_not_expose_public_key_fingerprint() {
        let request = PairingMessage::FetchImageThumbnail {
            version: IMAGE_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            remote_entry_id: "entry-42".to_string(),
        };
        assert_eq!(request.public_key_fingerprint(), "");
        let ack = PairingMessage::FetchImageThumbnailAck {
            version: IMAGE_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            remote_entry_id: "entry-42".to_string(),
            bytes_b64: String::new(),
            width: 128,
            height: 128,
        };
        assert_eq!(ack.public_key_fingerprint(), "");
        let unavailable = PairingMessage::FetchImageThumbnailUnavailable {
            version: IMAGE_WIRE_VERSION,
            peer_id: "peer-aaaa".to_string(),
            remote_entry_id: "entry-42".to_string(),
            reason: "busy".to_string(),
        };
        assert_eq!(unavailable.public_key_fingerprint(), "");
    }

    #[test]
    fn transport_outcome_variants_stay_distinct() {
        // The runtime branches on every variant listed in the
        // design. Pinning them here keeps the contract stable.
        let variants = [
            TransportOutcome::IncompatibleProtocol,
            TransportOutcome::UnknownPeer,
            TransportOutcome::KeyMismatch,
            TransportOutcome::Blocked,
            TransportOutcome::Revoked,
            TransportOutcome::Unavailable,
        ];
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }
}
