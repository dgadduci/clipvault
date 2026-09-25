//! Platform-neutral trait and types the local peer discovery
//! runtime uses to receive metadata-only browse / removal events.
//!
//! The contract is intentionally narrow:
//!
//! - [`DiscoveryAdvertisement`] carries the local metadata the
//!   runtime publishes in the DNS-SD TXT record. It MUST NOT carry
//!   endpoints, raw public key bytes or content; the runtime only
//!   uses it to populate the announcement and to self-filter
//!   incoming browse results.
//! - [`DiscoverySink`] is the platform-neutral callback the
//!   adapter pushes events into. The runtime owns the
//!   implementation that drains the sink and routes the events
//!   through the validation / persistence pipeline.
//! - [`PeerDiscoveryAdapter::start`] is idempotent and receives
//!   both the local advertisement and the sink so the adapter
//!   can wire its background thread once without holding onto
//!   references the runtime cannot guarantee otherwise.
//!
//! The default adapter installed by [`crate::stub`] for
//! unsupported targets is [`NoopPeerDiscoveryAdapter`], which
//! reports [`AdapterError::MulticastUnavailable`] on `start` so
//! the runtime surfaces a typed `runtime_stopped` reason without
//! touching the network. Production shells on macOS / Linux use
//! the `mdns-sd`-backed adapter behind the
//! `local-peer-discovery-mdns` feature.

use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

#[cfg(feature = "local-peer-discovery-mdns")]
pub mod mdns;

/// Metadata-only description of the local peer the adapter must
/// publish. The struct mirrors what the runtime validates /
/// persists on the receiving side: the `peer_id`, the public key
/// fingerprint, the validated visible name, the protocol major
/// version and the capability advertised by this build.
///
/// The adapter is free to use the values internally (e.g. to
/// build the TXT record) but MUST never propagate them as
/// identity alongside an IP / port tuple. The discovery contract
/// guarantees the runtime only sees [`crate::peer_identity`]
/// metadata — endpoints are scoped to the mDNS implementation
/// and never reach the core, the SQLite persistence layer, the
/// frontend, the logs or the bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryAdvertisement {
    pub peer_id: String,
    /// Short public-key fingerprint (16 hex chars). The mDNS TXT
    /// record always carries this projection so a discovery-only
    /// browser can render the same UI badge the pairing surface
    /// exposes. The pairing change additionally writes the full
    /// SHA-256 (64 hex chars) digest in [`Self::pairing_fingerprint`]
    /// so the listener can pin the canonical peer identity the
    /// pairing transcript signs over; the two values are derivable
    /// from each other through truncation.
    pub public_key_fingerprint: String,
    /// Full public-key fingerprint (64 hex chars). The pairing
    /// change populates the field ONLY when `capability =
    /// "pairing"`, never for the discovery-only record, so a
    /// discovery-only browser that does not understand pairing
    /// still rejects the record at the existing
    /// [`TxtRecord::public_key_fingerprint`] length check.
    pub pairing_fingerprint: Option<String>,
    pub display_name: String,
    pub protocol_major: i64,
    pub capability: String,
    /// Additive capability tokens the host advertises through a
    /// separate TXT field (`caps_extra`). The field is optional
    /// so a legacy client / browser can ignore it without
    /// breaking the record layout; the discovery validator
    /// combines the canonical `capability` token with the
    /// additive `caps_extra` tokens so a build that only knows
    /// about `pairing` keeps pairing while a newer build
    /// additionally opts into `image_import`.
    pub caps_extra: Vec<String>,
}

impl DiscoveryAdvertisement {
    /// Build an advertisement from the local identity the
    /// identity foundation mints. The display name is trimmed but
    /// NOT validated here — the runtime validates the value
    /// before persisting any peer observation, and the adapter
    /// only mirrors the runtime's contract.
    pub fn new(
        peer_id: impl Into<String>,
        public_key_fingerprint: impl Into<String>,
        display_name: impl Into<String>,
        protocol_major: i64,
        capability: impl Into<String>,
    ) -> Self {
        Self {
            peer_id: peer_id.into(),
            public_key_fingerprint: public_key_fingerprint.into(),
            pairing_fingerprint: None,
            display_name: display_name.into(),
            protocol_major,
            capability: capability.into(),
            caps_extra: Vec::new(),
        }
    }

    /// Build a pairing advertisement that carries both the short
    /// fingerprint (for the UI badge the discovery-side browsers
    /// also render) and the full SHA-256 (for the TLS listener).
    /// The helper exists so the productive pairing sink does not
    /// have to reach into struct fields directly.
    ///
    /// The advertised capability stays `pairing` exactly so a
    /// legacy client that only accepts the canonical value keeps
    /// recognising the record; the additive `image_import`
    /// capability travels through the separate `caps_extra`
    /// field instead. The legacy field never carries the new
    /// token because some parsers reject unknown tokens outright
    /// and would silently drop the host instead of pairing.
    pub fn new_pairing(
        peer_id: impl Into<String>,
        short_fingerprint: impl Into<String>,
        pairing_fingerprint: impl Into<String>,
        display_name: impl Into<String>,
        protocol_major: i64,
    ) -> Self {
        Self {
            peer_id: peer_id.into(),
            public_key_fingerprint: short_fingerprint.into(),
            pairing_fingerprint: Some(pairing_fingerprint.into()),
            display_name: display_name.into(),
            protocol_major,
            capability: PAIRING_CAPABILITY.to_string(),
            caps_extra: vec![IMAGE_IMPORT_CAPABILITY.to_string()],
        }
    }
}

/// Canonical `capability` value the productive pairing
/// advertisement publishes. Mirrored from
/// [`clipvault_core::peer_discovery::PAIRING_CAPABILITY`] so the
/// platform crate can write the canonical string without taking a
/// dependency on the core crate.
pub const PAIRING_CAPABILITY: &str = "pairing";

/// Additive capability the `peer-image-import` change ships. The
/// token travels through the `caps_extra` TXT field rather than
/// the canonical `capability` field so legacy clients that only
/// accept `pairing` keep pairing. Mirrored from
/// [`clipvault_core::peer_discovery::IMAGE_IMPORT_CAPABILITY`].
pub const IMAGE_IMPORT_CAPABILITY: &str = "image_import";

/// Platform-neutral callback the production adapter uses to
/// surface browse / removal events to the runtime.
///
/// Implementations are expected to be cheap (the production
/// adapter pushes events into an `mpsc`-backed channel and
/// returns without blocking on validation or persistence) and to
/// outlive the adapter's `start` / `stop` cycle so the runtime
/// can collect every event the adapter emits during its
/// lifetime.
///
/// `push` MUST NEVER block on long-running work: the production
/// adapter pushes from a background mDNS thread and any block
/// would stall the OS daemon. The runtime drains the queue
/// asynchronously, validates and persists on its own worker
/// thread.
pub trait DiscoverySink: Send + Sync {
    /// Forward a metadata-only event the adapter observed. The
    /// runtime owns the [`TxtRecord`] / removal payload contract;
    /// adapters that need to translate from a backend-specific
    /// representation (e.g. `mdns_sd::ServiceEvent`) MUST do that
    /// mapping before calling this method.
    fn push(&self, event: DiscoveryEvent);
}

/// Metadata-only event the platform adapter pushes into the
/// runtime's sink. Mirrors the shape [`crate::peer_discovery::DiscoveryEvent`]
/// from the core crate but stays in `clipvault-platform` so the
/// adapter does not depend on `clipvault-core`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryEvent {
    /// A peer appeared or refreshed its announcement. The TXT
    /// record is already in the canonical shape; the runtime
    /// still runs it through validation before persisting.
    Observed(TxtRecord),
    /// A peer went away (TTL expired, removal event, shutdown).
    /// The runtime uses the `peer_id` to mark the row as
    /// `NotAvailable` without persisting a new observation.
    Removed { peer_id: String },
}

/// Canonical TXT record payload the platform adapter hands to
/// the runtime. The struct is intentionally identical to the
/// core's shape so the core can convert it without losing
/// information; the platform-level mirror avoids a hard
/// `clipvault-core` dependency from the adapter.
///
/// `observed_at` is stamped by the runtime when the event
/// reaches the sink (the adapter never invents a timestamp)
/// so every persisted row carries the wall-clock instant the
/// local session first noticed the peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxtRecord {
    pub peer_id: String,
    /// Short public-key fingerprint (16 hex chars) the
    /// discovery-only mDNS TXT carries. Always populated by both
    /// `discovery_only` and `pairing` advertisements so a legacy
    /// browser that does not understand pairing still renders a
    /// usable badge.
    pub public_key_fingerprint: String,
    /// Full SHA-256 of the public key (64 hex chars). The
    /// productive pairing advertisement sets this field; the
    /// discovery-only advertisement leaves it `None`. The runtime
    /// persists the value in `known_peers.public_key_fingerprint_full`
    /// so the pairing runtime can build the canonical
    /// `OutboundSessionDescriptor` without re-resolving through
    /// mDNS.
    pub pairing_fingerprint: Option<String>,
    pub display_name: String,
    pub protocol_major: i64,
    /// Canonical `capability` token the host advertises. Stays
    /// at `pairing` (or `discovery_only`) exactly so a legacy
    /// parser that only accepts those literal values keeps
    /// recognising the record.
    pub capability: String,
    /// Additive capability tokens the host publishes through
    /// the separate `caps_extra` TXT field. The field is optional
    /// (empty when the host advertises no additive capabilities)
    /// and the discovery validator combines it with
    /// [`Self::capability`] when it decides which routes the
    /// peer supports.
    pub caps_extra: Vec<String>,
    pub observed_at: time::OffsetDateTime,
}

impl TxtRecord {
    /// Build a TXT record from the parts the adapter extracted
    /// from the underlying mDNS response. Used by tests and by
    /// adapters that pre-validate the bytes before pushing.
    /// `observed_at` is filled in by the runtime's sink — pass
    /// `time::OffsetDateTime::now_utc()` (or a deterministic
    /// timestamp in tests).
    pub fn new(
        peer_id: impl Into<String>,
        public_key_fingerprint: impl Into<String>,
        display_name: impl Into<String>,
        protocol_major: i64,
        capability: impl Into<String>,
        observed_at: time::OffsetDateTime,
    ) -> Self {
        Self {
            peer_id: peer_id.into(),
            public_key_fingerprint: public_key_fingerprint.into(),
            pairing_fingerprint: None,
            display_name: display_name.into(),
            protocol_major,
            capability: capability.into(),
            caps_extra: Vec::new(),
            observed_at,
        }
    }
}

/// Typed error the adapter surfaces on `start` / `stop`. The
/// variants mirror the failure modes the design documents:
/// multicast is unavailable on the current session, the secure
/// store cannot mint an identity, or the adapter is already in
/// the requested state.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AdapterError {
    /// The platform does not expose a multicast / mDNS path on
    /// the current session (firewall, router, …). The runtime
    /// reports this as `runtime_stopped` and never retries.
    #[error("mDNS multicast is unavailable on this session")]
    MulticastUnavailable,
    /// The runtime could not mint an identity because the secure
    /// store is unavailable. The runtime reports this as
    /// `identity_unavailable`.
    #[error("secure identity store is unavailable on this session")]
    IdentityUnavailable,
    /// The adapter is already running (`start`) or already stopped
    /// (`stop`). Implementations MUST collapse these into a no-op
    /// rather than raising; the variant exists for the strict
    /// surfaces the spec pins ("start/stop idempotente").
    #[error("discovery adapter is already in the requested state")]
    AlreadyRunning,
    /// The adapter received a payload that does not match the
    /// metadata-only contract. Implementations MUST surface this
    /// as a typed error instead of panicking.
    #[error("discovery adapter received malformed advertisement")]
    MalformedAdvertisement,
}

/// Platform-neutral trait the runtime uses to drive the adapter.
pub trait PeerDiscoveryAdapter: Send + Sync {
    /// Start the browser / registrant. Idempotent: a second call
    /// while the adapter is already running MUST be a no-op so the
    /// shell can call `start` from every bootstrap path
    /// without coordinating state. The advertisement is the
    /// local metadata the adapter publishes; the sink is where
    /// the adapter pushes observed / removed events. The sink is
    /// shared by [`Arc`] so the adapter can spawn its background
    /// browse thread with a `'static` reference to the sink.
    fn start(
        &self,
        advertisement: &DiscoveryAdvertisement,
        sink: std::sync::Arc<dyn DiscoverySink>,
    ) -> Result<(), AdapterError>;

    /// Productive install path the pairing change uses when the
    /// local TLS listener has reserved a real, non-zero ephemeral
    /// port. The adapter MUST publish the supplied `port` in the
    /// mDNS record so a remote browser knows where to dial. A
    /// `port` of `0` (the discovery-only placeholder) is rejected
    /// here so the productive path never silently downgrades to
    /// a discovery-only record — the `start` entry point keeps
    /// the discovery-only contract. The default implementation
    /// refuses the call so adapters that do not yet publish a
    /// real port never silently fall back to discovery-only.
    fn start_with_port(
        &self,
        advertisement: &DiscoveryAdvertisement,
        sink: std::sync::Arc<dyn DiscoverySink>,
        port: u16,
    ) -> Result<(), AdapterError> {
        let _ = (advertisement, sink, port);
        Err(AdapterError::MalformedAdvertisement)
    }

    /// Update the published mDNS record (port + properties) on an
    /// adapter that is already running. The browse loop and the
    /// sink the adapter installed at `start` time MUST stay alive
    /// so a discovery update never interrupts presence delivery
    /// to the runtime. The pairing transport calls this after
    /// binding the ephemeral TLS listener so the same
    /// [`crate::peer_discovery::mdns::MdnsPeerDiscoveryAdapter`]
    /// instance the [`crate::peer_discovery::PeerDiscoveryRuntime`]
    /// already started keeps emitting browse events to the
    /// runtime while the published record flips to
    /// `capability = pairing` with the real port. Callers MUST
    /// refuse to [`Self::reconfigure`] an adapter that is not
    /// running so the productive install path never silently
    /// downgrades to discovery-only. The default implementation
    /// returns [`AdapterError::AlreadyRunning`] so an adapter
    /// without a productive reconfigure path surfaces the same
    /// typed reason every other stateful call already documents.
    fn reconfigure(
        &self,
        advertisement: &DiscoveryAdvertisement,
        port: u16,
    ) -> Result<(), AdapterError> {
        let _ = (advertisement, port);
        Err(AdapterError::AlreadyRunning)
    }

    /// Stop the browser / registrant. Idempotent: a second call
    /// after a previous `stop` MUST be a no-op.
    fn stop(&self) -> Result<(), AdapterError>;

    /// Whether the adapter currently considers itself running. The
    /// shell uses the flag to render the `active` /
    /// `stopped` indicator the `Equipos` view exposes.
    fn is_running(&self) -> bool;
}

/// Static adapter the production shell installs when no real
/// `mdns-sd`-backed adapter is wired. Every `start` returns
/// [`AdapterError::MulticastUnavailable`] so the runtime reports
/// `runtime_stopped` to the frontend without a network round-trip;
/// `stop` is a no-op and `is_running` reports the cached state
/// the adapter observed on the most recent `start` call.
///
/// The noop is also the safe fallback the production shell
/// installs on targets without the
/// `local-peer-discovery-mdns` feature (Windows builds,
/// cross-compiles, hosts the runtime cannot link the daemon
/// against). Tests reuse it to exercise the typed-error
/// contract without standing up an mDNS backend.
#[derive(Debug, Default)]
pub struct NoopPeerDiscoveryAdapter {
    running: AtomicBool,
}

impl NoopPeerDiscoveryAdapter {
    /// Build a fresh adapter. The default state is `stopped` so
    /// the runtime's snapshot surfaces the documented `runtime_stopped`
    /// reason until the shell wires a real adapter.
    pub fn new() -> Self {
        Self::default()
    }
}

impl PeerDiscoveryAdapter for NoopPeerDiscoveryAdapter {
    fn start(
        &self,
        _advertisement: &DiscoveryAdvertisement,
        _sink: std::sync::Arc<dyn DiscoverySink>,
    ) -> Result<(), AdapterError> {
        // The noop path is the production default for hosts that
        // do not (yet) link the `mdns-sd` adapter. The `start`
        // call intentionally fails with a typed error so the
        // runtime reports `runtime_stopped` and the UI shows the
        // toggle in the off state.
        self.running.store(false, Ordering::Release);
        Err(AdapterError::MulticastUnavailable)
    }

    fn start_with_port(
        &self,
        _advertisement: &DiscoveryAdvertisement,
        _sink: std::sync::Arc<dyn DiscoverySink>,
        _port: u16,
    ) -> Result<(), AdapterError> {
        // The noop path does not link `mdns-sd`; refuse the
        // productive pairing call with the same typed error the
        // discovery-only `start` surfaces so the runtime reports
        // a stable reason without a network round-trip.
        self.running.store(false, Ordering::Release);
        Err(AdapterError::MulticastUnavailable)
    }

    fn reconfigure(
        &self,
        _advertisement: &DiscoveryAdvertisement,
        _port: u16,
    ) -> Result<(), AdapterError> {
        // The noop adapter never started. A reconfigure attempt
        // collapses to the typed `AlreadyRunning` reason the
        // default impl surfaces; the runtime can branch on the
        // typed error instead of a generic panic.
        Err(AdapterError::AlreadyRunning)
    }

    fn stop(&self) -> Result<(), AdapterError> {
        self.running.store(false, Ordering::Release);
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct RecordingSink {
        events: std::sync::Mutex<Vec<DiscoveryEvent>>,
    }

    impl RecordingSink {
        fn new() -> Self {
            Self {
                events: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn events(&self) -> Vec<DiscoveryEvent> {
            self.events.lock().expect("events").clone()
        }
    }

    impl DiscoverySink for RecordingSink {
        fn push(&self, event: DiscoveryEvent) {
            self.events.lock().expect("events").push(event);
        }
    }

    #[test]
    fn noop_adapter_reports_multicast_unavailable_on_start() {
        let adapter = NoopPeerDiscoveryAdapter::new();
        let recorder = std::sync::Arc::new(RecordingSink::new());
        let sink: std::sync::Arc<dyn DiscoverySink> = recorder.clone();
        let ad = DiscoveryAdvertisement::new(
            "00000000000000000000000000000000",
            "ffffffffffffffff",
            "Noop",
            1,
            "discovery_only",
        );
        let err = adapter
            .start(&ad, sink)
            .expect_err("noop must refuse to start");
        assert!(matches!(err, AdapterError::MulticastUnavailable));
        assert!(!adapter.is_running());
        assert!(recorder.events().is_empty());
    }

    #[test]
    fn noop_adapter_stop_is_a_no_op() {
        let adapter = NoopPeerDiscoveryAdapter::new();
        adapter.stop().expect("noop stop is a no-op");
        assert!(!adapter.is_running());
    }

    /// The noop adapter must refuse a reconfigure with the same
    /// typed `AlreadyRunning` reason the default impl surfaces
    /// so the runtime can distinguish "adapter never started"
    /// from a productive error on hosts that do not link
    /// `mdns-sd`.
    #[test]
    fn noop_adapter_reconfigure_collapses_to_already_running() {
        let adapter = NoopPeerDiscoveryAdapter::new();
        let ad = DiscoveryAdvertisement::new(
            "0123456789abcdef0123456789abcdef",
            "0123456789abcdef",
            "Studio",
            1,
            "pairing",
        );
        let err = adapter
            .reconfigure(&ad, 65111)
            .expect_err("noop must refuse reconfigure");
        assert!(matches!(err, AdapterError::AlreadyRunning));
    }

    #[test]
    fn advertisement_round_trips_through_clone() {
        let ad = DiscoveryAdvertisement::new(
            "0123456789abcdef0123456789abcdef",
            "0123456789abcdef",
            "Studio",
            1,
            "discovery_only",
        );
        let cloned = ad.clone();
        assert_eq!(cloned.peer_id, ad.peer_id);
        assert_eq!(cloned.public_key_fingerprint, ad.public_key_fingerprint);
        assert_eq!(cloned.display_name, ad.display_name);
        assert_eq!(cloned.protocol_major, ad.protocol_major);
        assert_eq!(cloned.capability, ad.capability);
    }

    #[test]
    fn new_pairing_advertises_image_import_capability() {
        // The productive pairing advertisement MUST publish the
        // canonical `pairing` capability through the legacy
        // field (no comma-separated `image_import` suffix, so a
        // strict legacy parser keeps recognising the record)
        // AND publish the additive `image_import` capability
        // through the dedicated `caps_extra` field so a
        // newer build opts into the image surface. A regression
        // that drops either the canonical capability or the
        // additive surface would break either the legacy
        // compatibility or the end-to-end image routes the
        // spec scenario pins.
        let ad = DiscoveryAdvertisement::new_pairing(
            "0123456789abcdef0123456789abcdef",
            "0123456789abcdef",
            "f".repeat(64),
            "Studio",
            1,
        );
        assert_eq!(ad.capability, PAIRING_CAPABILITY);
        assert_eq!(ad.caps_extra, vec![IMAGE_IMPORT_CAPABILITY.to_string()]);
    }

    #[test]
    fn new_discovery_advertisement_has_empty_caps_extra() {
        // The discovery-only helper builds an advertisement with
        // an empty additive surface so a legacy browser does
        // not see the `caps_extra` key on the wire.
        let ad = DiscoveryAdvertisement::new(
            "0123456789abcdef0123456789abcdef",
            "0123456789abcdef",
            "Studio",
            1,
            "discovery_only",
        );
        assert!(ad.caps_extra.is_empty());
    }

    #[test]
    fn recording_sink_collects_pushed_events() {
        let sink = RecordingSink::new();
        sink.push(DiscoveryEvent::Removed {
            peer_id: "peer-aaaa".to_string(),
        });
        sink.push(DiscoveryEvent::Observed(TxtRecord::new(
            "peer-bbbb",
            "0123456789abcdef",
            "Studio",
            1,
            "discovery_only",
            time::OffsetDateTime::now_utc(),
        )));
        let events = sink.events();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], DiscoveryEvent::Removed { .. }));
        match &events[1] {
            DiscoveryEvent::Observed(record) => {
                assert_eq!(record.peer_id, "peer-bbbb");
                assert_eq!(record.display_name, "Studio");
            }
            _ => panic!("expected Observed"),
        }
    }
}
