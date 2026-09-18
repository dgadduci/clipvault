//! `mdns-sd`-backed production adapter for the local peer
//! discovery runtime.
//!
//! The adapter registers and browses `_clipvault._tcp.local.`
//! continuously whenever the runtime calls `start`. The wire
//! record carries the validated TXT metadata the design pins
//! (`peer_id`, `public_key_fingerprint`, `display_name`,
//! `protocol_major`, `capability = discovery_only`) and nothing
//! else: the `port = 0` placeholder tells the OS we are
//! discovery-only — clients MUST NOT attempt a TCP connect.
//!
//! The adapter is the single owner of the [`mdns_sd::ServiceDaemon`]
//! it creates at `start`. The background browse loop runs on a
//! dedicated thread spawned by `start`; `stop` shuts the daemon
//! down, which closes the receiver and makes the loop exit.
//! `start` is idempotent so the shell can call it from every
//! bootstrap path without coordinating state.
//!
//! ## Endpoints stay inside the adapter
//!
//! The adapter MUST NOT surface IP addresses, host names or
//! port numbers to the runtime. The [`ServiceInfo`] payload the
//! mDNS daemon returns carries that data, but we translate it
//! into a [`TxtRecord`] (or a [`DiscoveryEvent::Removed`] for
//! disappearance events) before pushing into the sink so the
//! core, SQLite, frontend and bridge never see an endpoint.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use thiserror::Error;
use tracing::{debug, warn};

use super::{
    AdapterError, DiscoveryAdvertisement, DiscoveryEvent, DiscoverySink, PeerDiscoveryAdapter,
    TxtRecord,
};

/// Service type the adapter registers / browses. Mirrors
/// `crate::peer_discovery::SERVICE_TYPE` from the core crate but
/// stays local so the adapter is testable without depending on
/// `clipvault-core`.
const SERVICE_TYPE: &str = "_clipvault._tcp.local.";

/// Port the adapter publishes. Per the design
/// (`local-peer-discovery/design.md` §"Descubrimiento, presencia
/// y compatibilidad") discovery-only records use a placeholder
/// port so a client knows NOT to attempt a TCP connect — the
/// `local-peer-mutual-pairing` change will replace the
/// registration with a real TLS listener and a non-zero port,
/// but until then the contract is metadata-only.
const DISCOVERY_ONLY_PORT: u16 = 0;

/// TXT keys the adapter reads / writes for the metadata the
/// runtime validates / persists. The keys are kept short so the
/// mDNS payload stays well under RFC 6763 §6.2.1's 1300-byte
/// TXT record bound.
const TXT_PEER_ID: &str = "peer_id";
const TXT_FINGERPRINT: &str = "fp";
const TXT_DISPLAY_NAME: &str = "name";
const TXT_PROTOCOL_MAJOR: &str = "pmajor";
const TXT_CAPABILITY: &str = "cap";

/// Background adapter the production shell wires into the
/// runtime on macOS / Linux. The adapter is owned by the
/// runtime via [`std::sync::Arc`]; it owns its own
/// [`mdns_sd::ServiceDaemon`] and a single background thread
/// that drains browse events into the supplied sink.
///
/// `start` is idempotent — a second call while the adapter is
/// already running is a no-op (returns
/// [`AdapterError::AlreadyRunning`] so the runtime can branch on
/// the typed error). `stop` is also idempotent and keeps the
/// previously persisted metadata intact (the design pins that
/// the runtime MUST NOT delete `known_peers` on shutdown).
pub struct MdnsPeerDiscoveryAdapter {
    /// Cached running state. The runtime keeps its own copy;
    /// this atomic is the single source of truth inside the
    /// adapter so `is_running` reflects the current background
    /// thread state without locking.
    running: AtomicBool,
    /// Lifecycle mutex. `start` and `stop` hold it briefly so
    /// the two threads cannot race on `start` / `stop` /
    /// `stop`-then-`start` sequences. The runtime itself is
    /// idempotent (a second `start` short-circuits before
    /// touching the adapter) but the adapter enforces the same
    /// contract from inside.
    state: Mutex<AdapterState>,
}

#[derive(Default)]
struct AdapterState {
    daemon: Option<MdnsHandle>,
    peer_registry: Option<Arc<Mutex<HashMap<String, String>>>>,
}

/// Owns the [`mdns_sd::ServiceDaemon`] plus the join handle of
/// the background thread the adapter spawns at `start`. Dropping
/// the value without an explicit `stop` shuts the daemon down
/// (which closes the receiver and makes the loop exit), then
/// joins the thread.
struct MdnsHandle {
    daemon: mdns_sd::ServiceDaemon,
    fullname: String,
    thread: Option<JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
}

impl Drop for MdnsHandle {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Err(error) = self.daemon.shutdown() {
            warn!(error = %error, "mdns-sd daemon shutdown failed");
        }
        if let Some(handle) = self.thread.take() {
            // Ignore the join error: the thread may have already
            // exited because the daemon receiver closed.
            let _ = handle.join();
        }
    }
}

/// Failure modes the [`MdnsPeerDiscoveryAdapter::start`] call
/// surfaces internally. The platform layer collapses every
/// transport failure into a typed [`AdapterError`] so the
/// runtime does not depend on the `mdns-sd` error taxonomy.
#[derive(Debug, Error)]
pub enum MdnsAdapterError {
    #[error("mdns-sd daemon refused to register the discovery service")]
    Register,
    #[error("mdns-sd daemon refused to browse the discovery service")]
    Browse,
    #[error("failed to build mdns-sd service descriptor")]
    ServiceInfo,
    #[error("failed to spawn mdns-sd browse thread")]
    SpawnThread,
}

impl std::fmt::Debug for MdnsHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MdnsHandle")
            .field("fullname", &self.fullname)
            .finish()
    }
}

impl MdnsPeerDiscoveryAdapter {
    /// Build a fresh adapter. The adapter is in the `stopped`
    /// state until `start` is called. The production shell wires
    /// this constructor inside the bootstrap behind the
    /// `local-peer-discovery-mdns` feature so non-macOS / Linux
    /// builds never link the adapter.
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            state: Mutex::new(AdapterState::default()),
        }
    }

    fn install(
        &self,
        advertisement: &DiscoveryAdvertisement,
        sink: Arc<dyn DiscoverySink>,
    ) -> Result<(), MdnsAdapterError> {
        let daemon = mdns_sd::ServiceDaemon::new().map_err(|error| {
            warn!(error = %error, "failed to start mdns-sd daemon");
            MdnsAdapterError::Register
        })?;
        let instance = sanitise_instance_name(&advertisement.display_name);
        let fullname = format!("{instance}.{SERVICE_TYPE}");
        let properties = build_txt_properties(advertisement);
        let service_info = mdns_sd::ServiceInfo::new(
            SERVICE_TYPE,
            &instance,
            &fullname,
            "",
            DISCOVERY_ONLY_PORT,
            &properties[..],
        )
        .map_err(|error| {
            warn!(error = %error, "failed to build mdns-sd service info");
            MdnsAdapterError::ServiceInfo
        })?
        .enable_addr_auto();
        daemon.register(service_info).map_err(|error| {
            warn!(error = %error, "failed to register mdns-sd service");
            MdnsAdapterError::Register
        })?;
        let receiver = daemon.browse(SERVICE_TYPE).map_err(|error| {
            warn!(error = %error, "failed to browse mdns-sd service type");
            MdnsAdapterError::Browse
        })?;
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_thread = Arc::clone(&cancel);
        let peer_registry: Arc<Mutex<HashMap<String, String>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let peer_registry_for_thread = Arc::clone(&peer_registry);
        // The browse loop runs on a dedicated thread; the
        // runtime passes the sink as an `Arc` so the closure
        // can move it into the thread without leaking the
        // runtime's queue lifetime into the adapter.
        let thread = thread::Builder::new()
            .name("clipvault-peer-discovery".to_string())
            .spawn(move || {
                run_browse_loop(receiver, sink, cancel_for_thread, peer_registry_for_thread);
            })
            .map_err(|error| {
                warn!(error = %error, "failed to spawn mdns-sd browse thread");
                MdnsAdapterError::SpawnThread
            })?;
        *self.state.lock().expect("state lock") = AdapterState {
            daemon: Some(MdnsHandle {
                daemon,
                fullname,
                thread: Some(thread),
                cancel,
            }),
            peer_registry: Some(peer_registry),
        };
        debug!("mdns-sd adapter registered and browse loop started");
        Ok(())
    }

    fn shutdown(&self) {
        let mut state = self.state.lock().expect("state lock");
        if let Some(mut handle) = state.daemon.take() {
            handle.cancel.store(true, Ordering::Release);
            if let Err(error) = handle.daemon.shutdown() {
                warn!(error = %error, "mdns-sd daemon shutdown failed");
            }
            if let Some(thread) = handle.thread.take() {
                let _ = thread.join();
            }
        }
        state.peer_registry = None;
    }
}

impl Default for MdnsPeerDiscoveryAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PeerDiscoveryAdapter for MdnsPeerDiscoveryAdapter {
    fn start(
        &self,
        advertisement: &DiscoveryAdvertisement,
        sink: Arc<dyn DiscoverySink>,
    ) -> Result<(), AdapterError> {
        if self.running.swap(true, Ordering::AcqRel) {
            return Err(AdapterError::AlreadyRunning);
        }
        // The advertisement must be metadata-only and aligned
        // with the runtime contract; we refuse to start with an
        // empty peer_id or capability so a misconfigured shell
        // surfaces the error before publishing.
        if advertisement.peer_id.is_empty() || advertisement.capability.is_empty() {
            self.running.store(false, Ordering::Release);
            return Err(AdapterError::MalformedAdvertisement);
        }
        if let Err(_error) = self.install(advertisement, sink) {
            self.running.store(false, Ordering::Release);
            // `install` already logged the upstream cause; map
            // every internal failure to the typed multicast
            // unavailable variant so the runtime surfaces a
            // stable `runtime_stopped` reason.
            return Err(AdapterError::MulticastUnavailable);
        }
        Ok(())
    }

    fn stop(&self) -> Result<(), AdapterError> {
        self.shutdown();
        self.running.store(false, Ordering::Release);
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
}

/// Drain the mDNS browse receiver until the daemon shuts down
/// or the cancel flag flips. Every event is translated into a
/// metadata-only [`DiscoveryEvent`] before being pushed into
/// the sink so the runtime never sees IP, host or port.
///
/// `mdns-sd` returns a `RecvError` variant (Timeout /
/// Disconnected) when the daemon closes the receiver (the
/// shutdown path). We treat that as a clean exit and stop
/// without surfacing an error to the sink.
fn run_browse_loop(
    receiver: mdns_sd::Receiver<mdns_sd::ServiceEvent>,
    sink: Arc<dyn DiscoverySink>,
    cancel: Arc<AtomicBool>,
    peer_registry: Arc<Mutex<HashMap<String, String>>>,
) {
    while !cancel.load(Ordering::Acquire) {
        match receiver.recv_timeout(std::time::Duration::from_millis(500)) {
            Ok(event) => process_event(&event, &sink, &peer_registry),
            Err(error) => {
                if is_disconnected(&error) {
                    break;
                }
                // Timeout: continue the loop so the cancel flag
                // can short-circuit a stop request.
            }
        }
    }
}

/// Translate a single `mdns-sd` event into the metadata-only
/// [`DiscoveryEvent`] the runtime consumes. The function owns
/// the `service_fullname -> peer_id` association that lets
/// `ServiceRemoved` emit the real `peer_id` instead of the
/// visible instance name the daemon attached to the record.
///
/// The function is `pub(super)` only so the test suite can drive
/// the translation deterministically with synthetic events; the
/// map and the fullname / host / IP / port it keys on never
/// leave the adapter.
pub(super) fn process_event(
    event: &mdns_sd::ServiceEvent,
    sink: &Arc<dyn DiscoverySink>,
    peer_registry: &Arc<Mutex<HashMap<String, String>>>,
) {
    match event {
        mdns_sd::ServiceEvent::ServiceResolved(info) => {
            let fullname = info.get_fullname().to_string();
            if let Some(record) = translate_resolved(info) {
                let peer_id = record.peer_id.clone();
                peer_registry
                    .lock()
                    .expect("peer registry")
                    .insert(fullname, peer_id);
                sink.push(DiscoveryEvent::Observed(record));
            }
        }
        mdns_sd::ServiceEvent::ServiceRemoved(_ty, fullname) => {
            let peer_id = peer_registry
                .lock()
                .expect("peer registry")
                .remove(fullname);
            if let Some(peer_id) = peer_id {
                sink.push(DiscoveryEvent::Removed { peer_id });
            }
        }
        mdns_sd::ServiceEvent::SearchStopped(_)
        | mdns_sd::ServiceEvent::SearchStarted(_)
        | mdns_sd::ServiceEvent::ServiceFound(_, _) => {
            // Lifecycle events we deliberately ignore — the
            // runtime cares only about resolved and removed
            // events.
        }
    }
}

/// Detect whether the `flume::RecvTimeoutError` reported by the
/// receiver means the channel was closed (`Disconnected`) or the
/// receive timed out. The error type is `Debug`-formatted rather
/// than matched on directly so the platform crate does not have
/// to re-export `flume` as a direct dependency.
fn is_disconnected(error: &(dyn std::error::Error + 'static)) -> bool {
    // `flume::RecvTimeoutError::Disconnected` formats as
    // "sending on a disconnected channel" / the equivalent
    // message in newer releases. Match the more stable
    // `Debug` representation.
    let debug = format!("{error:?}");
    debug.contains("Disconnected")
}

/// Translate a [`mdns_sd::ServiceInfo`] payload into the
/// metadata-only [`TxtRecord`] the runtime persists. The function
/// deliberately ignores the resolved address set, the host name
/// and the port — the runtime MUST NEVER see an endpoint.
///
/// `observed_at` is stamped here (not in the runtime) so the
/// platform adapter produces a self-contained record the runtime
/// can validate without consulting any clock the adapter owns.
/// Using `OffsetDateTime::now_utc()` keeps the timestamp aligned
/// with the wall clock the SQLite repository stamps into
/// `last_discovered_at`.
fn translate_resolved(info: &mdns_sd::ServiceInfo) -> Option<TxtRecord> {
    let properties = info.get_properties();
    let peer_id = read_string_property(properties, TXT_PEER_ID)?;
    let fingerprint = read_string_property(properties, TXT_FINGERPRINT)?;
    let display_name = read_string_property(properties, TXT_DISPLAY_NAME)?;
    let protocol_major = read_int_property(properties, TXT_PROTOCOL_MAJOR)?;
    let capability = read_string_property(properties, TXT_CAPABILITY)?;
    Some(TxtRecord::new(
        peer_id,
        fingerprint,
        display_name,
        protocol_major,
        capability,
        time::OffsetDateTime::now_utc(),
    ))
}

fn read_string_property(properties: &mdns_sd::TxtProperties, key: &str) -> Option<String> {
    properties
        .get_property_val_str(key)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

fn read_int_property(properties: &mdns_sd::TxtProperties, key: &str) -> Option<i64> {
    read_string_property(properties, key).and_then(|value| value.parse::<i64>().ok())
}

fn build_txt_properties(advertisement: &DiscoveryAdvertisement) -> [(String, String); 5] {
    [
        (TXT_PEER_ID.to_string(), advertisement.peer_id.clone()),
        (
            TXT_FINGERPRINT.to_string(),
            advertisement.public_key_fingerprint.clone(),
        ),
        (
            TXT_DISPLAY_NAME.to_string(),
            advertisement.display_name.clone(),
        ),
        (
            TXT_PROTOCOL_MAJOR.to_string(),
            advertisement.protocol_major.to_string(),
        ),
        (TXT_CAPABILITY.to_string(), advertisement.capability.clone()),
    ]
}

/// mDNS instance names are limited to 63 octets per RFC 6763
/// §7.2 / §7.3. We trim the validated visible name to that
/// bound and replace unsafe characters so the service
/// registration does not fail with `ServiceNameTooLong`.
fn sanitise_instance_name(display_name: &str) -> String {
    let trimmed = display_name.trim();
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_control() {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    out.truncate(63);
    if out.is_empty() {
        "ClipVault".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    fn ad() -> DiscoveryAdvertisement {
        DiscoveryAdvertisement::new(
            "0123456789abcdef0123456789abcdef",
            "0123456789abcdef",
            "Studio",
            1,
            "discovery_only",
        )
    }

    #[test]
    fn build_txt_properties_round_trips_the_advertisement() {
        let properties = build_txt_properties(&ad());
        let map: std::collections::HashMap<_, _> = properties
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(map[TXT_PEER_ID], "0123456789abcdef0123456789abcdef");
        assert_eq!(map[TXT_FINGERPRINT], "0123456789abcdef");
        assert_eq!(map[TXT_DISPLAY_NAME], "Studio");
        assert_eq!(map[TXT_PROTOCOL_MAJOR], "1");
        assert_eq!(map[TXT_CAPABILITY], "discovery_only");
    }

    /// translate_resolved must ignore the address set / hostname /
    /// port the daemon attaches to the `ServiceInfo`; the metadata
    /// the runtime persists is metadata-only by construction.
    #[test]
    fn translate_resolved_strips_endpoint_metadata() {
        let mut properties = std::collections::HashMap::new();
        properties.insert(
            "peer_id".to_string(),
            "11111111111111111111111111111111".to_string(),
        );
        properties.insert("fp".to_string(), "aaaaaaaaaaaaaaaa".to_string());
        properties.insert("name".to_string(), "Studio".to_string());
        properties.insert("pmajor".to_string(), "1".to_string());
        properties.insert("cap".to_string(), "discovery_only".to_string());
        let info = mdns_sd::ServiceInfo::new(
            SERVICE_TYPE,
            "Studio",
            "studio.local.",
            "192.0.2.42",
            65000,
            properties,
        )
        .unwrap();
        let translated = translate_resolved(&info).expect("translate");
        assert_eq!(translated.peer_id, "11111111111111111111111111111111");
        assert_eq!(translated.public_key_fingerprint, "aaaaaaaaaaaaaaaa");
        assert_eq!(translated.display_name, "Studio");
        assert_eq!(translated.protocol_major, 1);
        assert_eq!(translated.capability, "discovery_only");
        // The translated record MUST NOT carry host / port /
        // address — the runtime never sees an endpoint.
        let debug = format!("{translated:?}");
        assert!(!debug.contains("192.0.2.42"));
        assert!(!debug.contains("65000"));
        assert!(!debug.contains("studio.local."));
    }

    /// translate_resolved must surface a `None` when the TXT
    /// record is missing one of the required fields so the
    /// runtime can classify the rejection as `Rejected` instead
    /// of silently dropping the bytes.
    #[test]
    fn translate_resolved_returns_none_for_missing_capability() {
        let mut properties = std::collections::HashMap::new();
        properties.insert(
            "peer_id".to_string(),
            "11111111111111111111111111111111".to_string(),
        );
        properties.insert("fp".to_string(), "aaaaaaaaaaaaaaaa".to_string());
        properties.insert("name".to_string(), "Studio".to_string());
        properties.insert("pmajor".to_string(), "1".to_string());
        // capability missing on purpose
        let info =
            mdns_sd::ServiceInfo::new(SERVICE_TYPE, "Studio", "studio.local.", "", 0, properties)
                .unwrap();
        assert!(translate_resolved(&info).is_none());
    }

    #[test]
    fn sanitise_instance_name_truncates_to_sixty_three_chars() {
        let long = "a".repeat(200);
        let sanitised = sanitise_instance_name(&long);
        assert_eq!(sanitised.len(), 63);
    }

    #[test]
    fn sanitise_instance_name_replaces_control_chars_and_falls_back() {
        assert_eq!(sanitise_instance_name("\u{0001}ctrl"), "_ctrl");
        assert_eq!(sanitise_instance_name(""), "ClipVault");
        assert_eq!(sanitise_instance_name("   "), "ClipVault");
    }

    #[derive(Default)]
    struct CapturingSink {
        events: StdMutex<Vec<DiscoveryEvent>>,
    }

    impl DiscoverySink for CapturingSink {
        fn push(&self, event: DiscoveryEvent) {
            self.events.lock().expect("events").push(event);
        }
    }

    #[test]
    fn adapter_is_idempotent_around_start_stop() {
        let adapter = MdnsPeerDiscoveryAdapter::new();
        let sink: Arc<dyn DiscoverySink> = Arc::new(CapturingSink::default());
        // The first call exercises the real daemon; in CI we
        // allow the start to fail (multicast is not always
        // available in the sandbox) but the idempotency branch
        // we care about is the second call. We probe both
        // outcomes explicitly.
        let first = adapter.start(&ad(), sink.clone());
        if first.is_ok() {
            let second = adapter.start(&ad(), sink.clone());
            assert!(matches!(second, Err(AdapterError::AlreadyRunning)));
            adapter.stop().expect("stop");
        } else {
            assert!(matches!(first, Err(AdapterError::MulticastUnavailable)));
        }
        assert!(!adapter.is_running());
    }

    #[test]
    fn adapter_rejects_empty_peer_id_with_typed_error() {
        let adapter = MdnsPeerDiscoveryAdapter::new();
        let sink: Arc<dyn DiscoverySink> = Arc::new(CapturingSink::default());
        let bad = DiscoveryAdvertisement::new("", "fp", "Studio", 1, "discovery_only");
        let err = adapter
            .start(&bad, sink)
            .expect_err("empty peer_id must be rejected");
        assert!(matches!(err, AdapterError::MalformedAdvertisement));
        assert!(!adapter.is_running());
    }

    #[test]
    fn adapter_rejects_empty_capability_with_typed_error() {
        let adapter = MdnsPeerDiscoveryAdapter::new();
        let sink: Arc<dyn DiscoverySink> = Arc::new(CapturingSink::default());
        let bad = DiscoveryAdvertisement::new("peer", "fp", "Studio", 1, "");
        let err = adapter
            .start(&bad, sink)
            .expect_err("empty capability must be rejected");
        assert!(matches!(err, AdapterError::MalformedAdvertisement));
        assert!(!adapter.is_running());
    }

    /// Loopback test: spin up two real daemons on the same host
    /// and confirm the bouncer sees the first daemons record.
    ///
    /// The test is feature-gated to keep CI on hosts without
    /// multicast (sandboxed runners, distroless containers, …)
    /// clean — the design documents that real discovery coverage
    /// requires a stable network surface, and the unit tests above
    /// pin the typed-error contract when multicast is missing.
    ///
    /// We must NOT probe the daemon up front: a sandbox that
    /// blocks multicast surfaces that restriction as
    /// `Operation not permitted` from `ServiceDaemon::new()`
    /// itself, and the test should classify the same restriction
    /// through `MdnsPeerDiscoveryAdapter::start()` instead of
    /// panicking. The two `start` calls below already detect and
    /// typify the unavailability; if either fails we stop the
    /// surviving adapter (if any) and exit the case explicitly
    /// without an assertion so the loopback exchange is only
    /// validated when both sides actually published.
    #[test]
    fn two_daemons_discover_each_other_on_loopback_when_multicast_is_available() {
        let first_ad = DiscoveryAdvertisement::new(
            "11111111111111111111111111111111",
            "aaaaaaaaaaaaaaaa",
            "Studio-A",
            1,
            "discovery_only",
        );
        let second_ad = DiscoveryAdvertisement::new(
            "22222222222222222222222222222222",
            "bbbbbbbbbbbbbbbb",
            "Studio-B",
            1,
            "discovery_only",
        );
        let recorder_a = Arc::new(CapturingSink::default());
        let recorder_b = Arc::new(CapturingSink::default());
        let sink_a: Arc<dyn DiscoverySink> = recorder_a.clone();
        let sink_b: Arc<dyn DiscoverySink> = recorder_b.clone();
        // Install two adapters in parallel. The test only asserts
        // on the loopback exchange when both adapters publish
        // successfully — otherwise the exchange never happens and
        // a one-sided pass would be vacuous. If either `start`
        // surfaces the typed `MulticastUnavailable` error (e.g.
        // the sandbox blocks the socket or the link-local
        // multicast group is denied), we stop the surviving
        // adapter (if any) and exit the case explicitly without
        // an assertion, so the test stays a no-op on hosts where
        // mDNS cannot run.
        let first = MdnsPeerDiscoveryAdapter::new();
        let second = MdnsPeerDiscoveryAdapter::new();
        let first_started = first.start(&first_ad, sink_a.clone()).is_ok();
        let second_started = second.start(&second_ad, sink_b.clone()).is_ok();
        if !(first_started && second_started) {
            // Cleanly stop whichever adapter did start.
            let _ = first.stop();
            let _ = second.stop();
            // Skip the loopback assertion when multicast is
            // unavailable on this host — the typed-error unit
            // tests above already cover the fallback path.
            eprintln!(
                "loopback skipped: first_started={first_started} second_started={second_started}"
            );
            return;
        }
        // Wait up to ~6 seconds for the browser loop to receive
        // the matching record. mDNS announcements typically
        // resolve within a second on loopback but RFC 6762
        // allows for retransmissions.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
        let mut found = false;
        while std::time::Instant::now() < deadline {
            if recorder_b
                .events
                .lock()
                .expect("events")
                .iter()
                .any(|event| {
                    matches!(event, DiscoveryEvent::Observed(record)
                        if record.peer_id == first_ad.peer_id
                            && record.public_key_fingerprint == first_ad.public_key_fingerprint
                            && record.display_name == first_ad.display_name
                            && record.protocol_major == first_ad.protocol_major
                            && record.capability == first_ad.capability)
                })
            {
                found = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let _ = first.stop();
        let _ = second.stop();
        assert!(
            found,
            "second daemon never observed the first daemons advertisement on loopback"
        );
    }

    /// The `peer_id` the adapter emits on `Removed` must match the
    /// one it observed on the matching `ServiceResolved`. This test
    /// pins the regression: the previous implementation derived
    /// `peer_id` from the visible instance portion of the fullname
    /// instead of the TXT record, so a peer whose `display_name`
    /// did not equal its `peer_id` (the typical case) leaked the
    /// wrong identity into the runtime on removal.
    #[test]
    fn resolved_then_removed_emit_same_peer_id() {
        let registry: Arc<StdMutex<HashMap<String, String>>> =
            Arc::new(StdMutex::new(HashMap::new()));
        let sink = Arc::new(CapturingSink::default());
        let sink_dyn: Arc<dyn DiscoverySink> = sink.clone();
        let mut properties = std::collections::HashMap::new();
        properties.insert(
            "peer_id".to_string(),
            "0123456789abcdef0123456789abcdef".to_string(),
        );
        properties.insert("fp".to_string(), "aaaaaaaaaaaaaaaa".to_string());
        properties.insert("name".to_string(), "Studio".to_string());
        properties.insert("pmajor".to_string(), "1".to_string());
        properties.insert("cap".to_string(), "discovery_only".to_string());
        let info =
            mdns_sd::ServiceInfo::new(SERVICE_TYPE, "Studio-A", "host.local.", "", 0, properties)
                .expect("service info");
        let fullname = info.get_fullname().to_string();
        let resolved = mdns_sd::ServiceEvent::ServiceResolved(info);
        process_event(&resolved, &sink_dyn, &registry);
        let removed = mdns_sd::ServiceEvent::ServiceRemoved(SERVICE_TYPE.to_string(), fullname);
        process_event(&removed, &sink_dyn, &registry);
        let events = sink.events.lock().expect("events").clone();
        assert_eq!(events.len(), 2);
        let observed = match &events[0] {
            DiscoveryEvent::Observed(record) => record.peer_id.clone(),
            other => panic!("expected Observed, got {other:?}"),
        };
        let removed_peer_id = match &events[1] {
            DiscoveryEvent::Removed { peer_id } => peer_id.clone(),
            other => panic!("expected Removed, got {other:?}"),
        };
        assert_eq!(observed, "0123456789abcdef0123456789abcdef");
        assert_eq!(removed_peer_id, "0123456789abcdef0123456789abcdef");
        assert_eq!(observed, removed_peer_id);
    }

    /// Two peers advertising the same visible name must still be
    /// tracked independently. mDNS disambiguates the conflicting
    /// instance names on the wire, so the adapter's
    /// `service_fullname -> peer_id` map must key on the fullname
    /// the daemon reported rather than the display_name the adapter
    /// published. This test proves that, with the registry in
    /// place, each `Removed` event surfaces the right peer_id —
    /// neither peer leaks the other's identity and neither collapses
    /// to the visible name.
    #[test]
    fn two_peers_with_same_display_name_dont_interfere() {
        let registry: Arc<StdMutex<HashMap<String, String>>> =
            Arc::new(StdMutex::new(HashMap::new()));
        let sink = Arc::new(CapturingSink::default());
        let sink_dyn: Arc<dyn DiscoverySink> = sink.clone();
        let mut properties_a = std::collections::HashMap::new();
        properties_a.insert(
            "peer_id".to_string(),
            "11111111111111111111111111111111".to_string(),
        );
        properties_a.insert("fp".to_string(), "aaaaaaaaaaaaaaaa".to_string());
        properties_a.insert("name".to_string(), "Studio".to_string());
        properties_a.insert("pmajor".to_string(), "1".to_string());
        properties_a.insert("cap".to_string(), "discovery_only".to_string());
        let info_a =
            mdns_sd::ServiceInfo::new(SERVICE_TYPE, "Studio", "host-a.local.", "", 0, properties_a)
                .expect("service info a");
        let mut properties_b = std::collections::HashMap::new();
        properties_b.insert(
            "peer_id".to_string(),
            "22222222222222222222222222222222".to_string(),
        );
        properties_b.insert("fp".to_string(), "bbbbbbbbbbbbbbbb".to_string());
        properties_b.insert("name".to_string(), "Studio".to_string());
        properties_b.insert("pmajor".to_string(), "1".to_string());
        properties_b.insert("cap".to_string(), "discovery_only".to_string());
        let info_b = mdns_sd::ServiceInfo::new(
            SERVICE_TYPE,
            "Studio (2)",
            "host-b.local.",
            "",
            0,
            properties_b,
        )
        .expect("service info b");
        let fullname_a = info_a.get_fullname().to_string();
        let fullname_b = info_b.get_fullname().to_string();
        assert_ne!(fullname_a, fullname_b);
        process_event(
            &mdns_sd::ServiceEvent::ServiceResolved(info_a),
            &sink_dyn,
            &registry,
        );
        process_event(
            &mdns_sd::ServiceEvent::ServiceResolved(info_b),
            &sink_dyn,
            &registry,
        );
        process_event(
            &mdns_sd::ServiceEvent::ServiceRemoved(SERVICE_TYPE.to_string(), fullname_a),
            &sink_dyn,
            &registry,
        );
        process_event(
            &mdns_sd::ServiceEvent::ServiceRemoved(SERVICE_TYPE.to_string(), fullname_b),
            &sink_dyn,
            &registry,
        );
        let events = sink.events.lock().expect("events").clone();
        assert_eq!(events.len(), 4);
        let observed_a = match &events[0] {
            DiscoveryEvent::Observed(record) => record,
            other => panic!("expected Observed, got {other:?}"),
        };
        let observed_b = match &events[1] {
            DiscoveryEvent::Observed(record) => record,
            other => panic!("expected Observed, got {other:?}"),
        };
        let removed_a = match &events[2] {
            DiscoveryEvent::Removed { peer_id } => peer_id.clone(),
            other => panic!("expected Removed, got {other:?}"),
        };
        let removed_b = match &events[3] {
            DiscoveryEvent::Removed { peer_id } => peer_id.clone(),
            other => panic!("expected Removed, got {other:?}"),
        };
        assert_eq!(observed_a.peer_id, "11111111111111111111111111111111");
        assert_eq!(observed_a.display_name, "Studio");
        assert_eq!(observed_b.peer_id, "22222222222222222222222222222222");
        assert_eq!(observed_b.display_name, "Studio");
        assert_eq!(removed_a, "11111111111111111111111111111111");
        assert_eq!(removed_b, "22222222222222222222222222222222");
        assert_ne!(
            removed_a, removed_b,
            "the registry must not collapse the two peer_ids back to the shared display_name"
        );
        assert!(registry.lock().expect("peer registry").is_empty());
    }

    /// The browse loop's `ServiceRemoved` branch must stay silent
    /// when the registry has no mapping for the fullname the
    /// daemon reported. A spurious removal (e.g. one that arrives
    /// before the matching resolved or that references a foreign
    /// service on the link) must not invent a peer_id; the
    /// runtime already trusts only validated TXT records and
    /// would reject any peer_id we synthesised here.
    #[test]
    fn removed_without_prior_resolved_emits_no_event() {
        let registry: Arc<StdMutex<HashMap<String, String>>> =
            Arc::new(StdMutex::new(HashMap::new()));
        let sink = Arc::new(CapturingSink::default());
        let sink_dyn: Arc<dyn DiscoverySink> = sink.clone();
        let removed = mdns_sd::ServiceEvent::ServiceRemoved(
            SERVICE_TYPE.to_string(),
            "Unknown._clipvault._tcp.local.".to_string(),
        );
        process_event(&removed, &sink_dyn, &registry);
        assert!(sink.events.lock().expect("events").is_empty());
    }
}
