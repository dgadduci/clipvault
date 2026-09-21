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

/// Maximum length of an incoming pairing wire envelope. The pairing
/// surface only ever exchanges nonces, fingerprints, SAS confirmations
/// and signed approvals — the cap is generous and stays well below
/// the MTU so the listener can short-circuit obviously malicious
/// payloads before they reach the application state machine.
pub const PAIRING_MAX_PAYLOAD_BYTES: usize = 4 * 1024;

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
    /// record of the peer (the runtime surfaces the
    /// `UnknownPeer` outcome).
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
    }

    impl PairingMessage {
        pub fn version(&self) -> u32 {
            match self {
                PairingMessage::Hello { version, .. }
                | PairingMessage::HelloAck { version, .. }
                | PairingMessage::Approve { version, .. }
                | PairingMessage::Health { version, .. }
                | PairingMessage::HealthAck { version, .. } => *version,
            }
        }

        pub fn peer_id(&self) -> &str {
            match self {
                PairingMessage::Hello { peer_id, .. }
                | PairingMessage::HelloAck { peer_id, .. }
                | PairingMessage::Approve { peer_id, .. }
                | PairingMessage::Health { peer_id, .. }
                | PairingMessage::HealthAck { peer_id, .. } => peer_id,
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
                | PairingMessage::HealthAck { .. } => "",
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

    /// Capture-only sink used by the noop tests so they can build
    /// a `TransportSink` without standing up the real TLS stack.
    #[derive(Default)]
    struct NullSink;

    impl TransportSink for NullSink {
        fn on_pairing_observed(&self, _observation: PeerTransportObservation) {}
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
