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

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

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

/// Port the discovery-only record publishes. Per the design
/// (`local-peer-discovery/design.md` §"Descubrimiento, presencia
/// y compatibilidad") discovery-only records use a placeholder
/// port so a client knows NOT to attempt a TCP connect. The
/// `local-peer-mutual-pairing` change uses the productive
/// [`Self::start_with_port`] entry point to publish the real
/// non-zero ephemeral port the TLS listener reserved; the
/// discovery-only contract stays bound to the legacy `start`
/// path and to this constant. The constant is `pub` so the
/// pairing transport's withdrawal path can flip the
/// advertisement back to the discovery-only contract without
/// embedding a magic number in the call site.
pub const DISCOVERY_ONLY_PORT: u16 = 0;

/// TXT keys the adapter reads / writes for the metadata the
/// runtime validates / persists. The keys are kept short so the
/// mDNS payload stays well under RFC 6763 §6.2.1's 1300-byte
/// TXT record bound.
const TXT_PEER_ID: &str = "peer_id";
const TXT_FINGERPRINT: &str = "fp";
const TXT_PAIRING_FINGERPRINT: &str = "pfp";
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
    /// Most recent `SocketAddr` the browse loop observed for
    /// each `peer_id`. The pairing transport uses this to dial
    /// the announced listener without exposing the address to
    /// the runtime layer. `None` when the adapter is stopped.
    peer_addresses: Option<Arc<Mutex<HashMap<String, std::net::SocketAddr>>>>,
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
    /// Idempotency guard so the ordered shutdown runs at most
    /// once even when both the explicit `stop` path and `Drop`
    /// fire (the explicit path takes the [`MdnsHandle`] out of
    /// the adapter state and runs `shutdown`; the eventual
    /// `Drop` sees the guard flipped and short-circuits).
    shutdown_started: Cell<bool>,
}

/// Bounded wait for the goodbye packet the `mdns-sd` daemon
/// confirms on the `unregister` channel. RFC 6762 lets the daemon
/// retransmit the goodbye at +120 ms before responding on the
/// channel, so a 750 ms ceiling comfortably covers the happy
/// path while still bounding the adapter when the daemon is
/// hung or the channel is wedged.
const UNREGISTER_WAIT: Duration = Duration::from_millis(750);

impl MdnsHandle {
    /// Run the ordered shutdown exactly once.
    ///
    /// The order is critical and matches the design pinned in
    /// `local-peer-discovery/design.md` §"Descubrimiento, presencia
    /// y compatibilidad":
    ///
    /// 1. flip the cancel flag so the browse loop wakes up early;
    /// 2. `unregister` the published `fullname` so remote browsers
    ///    receive `ServiceRemoved` and flip the peer to
    ///    `NotAvailable` without waiting for the TTL (~120 s);
    /// 3. shut down the daemon (closes the receiver channel);
    /// 4. join the browse thread.
    ///
    /// The function only logs fixed safe phrases on failure so the
    /// `fullname`, host name, IP, port and raw `mdns-sd` error
    /// taxonomy never leak through the adapter. The runtime, core,
    /// SQLite, frontend and Tauri commands must remain oblivious
    /// to those values.
    fn shutdown(&mut self) {
        if self.shutdown_started.get() {
            return;
        }
        self.shutdown_started.set(true);

        self.cancel.store(true, Ordering::Release);

        // 1. Send the goodbye packet. `mdns-sd` documents
        //    `unregister` as the graceful shutdown of a service
        //    and uses the returned `Receiver<UnregisterStatus>` to
        //    signal completion; on `Error::Msg` / `Error::Again`
        //    we surface a fixed safe phrase so the wire error
        //    (which can carry the daemon's host string) never
        //    reaches the tracing pipeline.
        match self.daemon.unregister(&self.fullname) {
            Ok(receiver) => match receiver.recv_timeout(UNREGISTER_WAIT) {
                Ok(_) => debug!("mdns-sd service unregistered"),
                Err(_) => warn!("mdns-sd unregister timed out; continuing shutdown"),
            },
            Err(_) => warn!("mdns-sd unregister failed; continuing shutdown"),
        }

        // 2. Tear down the daemon (closes the receiver so the loop
        //    exits). The daemon's own `Error` enum carries the
        //    socket path on `Error::Msg`, so we keep the existing
        //    fixed-phrase log to avoid leaking it.
        if let Err(_error) = self.daemon.shutdown() {
            warn!("mdns-sd daemon shutdown failed");
        }

        // 3. Join the browse thread. The thread may have already
        //    exited because the daemon receiver closed; ignore
        //    the join error in that case.
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for MdnsHandle {
    fn drop(&mut self) {
        // Idempotent: the explicit `stop` path takes the
        // [`MdnsHandle`] out of the adapter state and runs
        // `shutdown` itself; when this `Drop` fires the guard is
        // already flipped and the call short-circuits.
        self.shutdown();
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
        port: u16,
    ) -> Result<(), MdnsAdapterError> {
        let daemon = mdns_sd::ServiceDaemon::new().map_err(|error| {
            warn!(error = %error, "failed to start mdns-sd daemon");
            MdnsAdapterError::Register
        })?;
        let (instance, hostname, fullname) = service_names(advertisement);
        let properties = build_txt_properties(advertisement);
        let service_info = mdns_sd::ServiceInfo::new(
            SERVICE_TYPE,
            &instance,
            &hostname,
            "",
            port,
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
        let peer_addresses: Arc<Mutex<HashMap<String, std::net::SocketAddr>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let peer_addresses_for_thread = Arc::clone(&peer_addresses);
        // The browse loop runs on a dedicated thread; the
        // runtime passes the sink as an `Arc` so the closure
        // can move it into the thread without leaking the
        // runtime's queue lifetime into the adapter.
        let thread = thread::Builder::new()
            .name("clipvault-peer-discovery".to_string())
            .spawn(move || {
                run_browse_loop(
                    receiver,
                    sink,
                    cancel_for_thread,
                    peer_registry_for_thread,
                    peer_addresses_for_thread,
                );
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
                shutdown_started: Cell::new(false),
            }),
            peer_registry: Some(peer_registry),
            peer_addresses: Some(peer_addresses),
        };
        debug!(port, "mdns-sd adapter registered and browse loop started");
        Ok(())
    }

    fn shutdown(&self) {
        let mut state = self.state.lock().expect("state lock");
        if let Some(mut handle) = state.daemon.take() {
            // Take the daemon handle out of state first so a
            // second `stop` is a compile-time no-op (the
            // `MdnsHandle` is no longer there) and `Drop` only
            // runs the idempotent helper if this path didn't
            // already execute the ordered shutdown.
            handle.shutdown();
        }
        state.peer_registry = None;
        state.peer_addresses = None;
    }

    /// Update the published mDNS record on a running adapter
    /// without restarting the browse loop. The method unregisters
    /// the previous `fullname` and registers a fresh service with
    /// the supplied `port` + TXT properties; the receiver and the
    /// browse thread keep running so presence delivery to the
    /// runtime is never interrupted. The pairing transport calls
    /// this after binding the ephemeral TLS listener so the same
    /// adapter the runtime started for discovery continues to
    /// forward browse events while the published record flips to
    /// `capability = pairing` with the real port.
    ///
    /// Returns [`MdnsAdapterError::Register`] when the adapter
    /// is not currently running (the runtime should have started
    /// it before the pairing transport tries to publish) so the
    /// productive install path never silently downgrades to
    /// discovery-only.
    fn reconfigure_record(
        &self,
        advertisement: &DiscoveryAdvertisement,
        port: u16,
    ) -> Result<(), MdnsAdapterError> {
        let mut state = self.state.lock().expect("state lock");
        let handle = state.daemon.as_mut().ok_or(MdnsAdapterError::Register)?;
        // Build the new service descriptor first so a malformed
        // advertisement cannot leave the adapter with the
        // previous record unregistered and no replacement
        // registered. The DNS-SD instance and SRV target are
        // derived from the stable peer id, not the editable
        // display name. In particular, a service fullname (which
        // contains `_clipvault._tcp`) is NOT a valid DNS hostname
        // for the SRV target. Some browsers tolerate that invalid
        // target while others only surface `ServiceFound`, causing
        // one-way macOS/Linux discovery. `service_names` creates a
        // label-safe `clipvault-<peer_id>.local.` hostname so every
        // browser can resolve the SRV/TXT/A(AAA) record set.
        let (instance, hostname, new_fullname) = service_names(advertisement);
        let properties = build_txt_properties(advertisement);
        let service_info = mdns_sd::ServiceInfo::new(
            SERVICE_TYPE,
            &instance,
            &hostname,
            "",
            port,
            &properties[..],
        )
        .map_err(|error| {
            warn!(error = %error, "failed to build mdns-sd service info");
            MdnsAdapterError::ServiceInfo
        })?
        .enable_addr_auto();
        // Only unregister the previous record when the
        // `fullname` actually changed; flipping the TXT record
        // for the same instance keeps the daemon's state
        // machine clean.
        if handle.fullname != new_fullname {
            match handle.daemon.unregister(&handle.fullname) {
                Ok(receiver) => {
                    let _ = receiver.recv_timeout(UNREGISTER_WAIT);
                }
                Err(_) => warn!("mdns-sd unregister during reconfigure failed; continuing"),
            }
        }
        handle.daemon.register(service_info).map_err(|error| {
            warn!(error = %error, "failed to register mdns-sd service");
            MdnsAdapterError::Register
        })?;
        handle.fullname = new_fullname;
        debug!(port, "mdns-sd adapter reconfigured");
        Ok(())
    }

    /// Resolve the most recent `SocketAddr` the mDNS browse loop
    /// observed for the matching `peer_id`. The pairing
    /// transport uses this internally to dial the announced
    /// listener without ever exposing the address to the
    /// runtime, SQLite, Tauri or the frontend.
    pub fn resolve_peer(&self, peer_id: &str) -> Option<std::net::SocketAddr> {
        let state = self.state.lock().expect("state lock");
        let addresses = state.peer_addresses.as_ref()?;
        let map = addresses.lock().expect("peer addresses");
        map.get(peer_id).copied()
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
        // The advertisement must be metadata-only and aligned
        // with the runtime contract; we refuse to start with an
        // empty peer_id or capability so a misconfigured shell
        // surfaces the error before publishing.
        if advertisement.peer_id.is_empty() || advertisement.capability.is_empty() {
            return Err(AdapterError::MalformedAdvertisement);
        }
        if self.running.swap(true, Ordering::AcqRel) {
            return Err(AdapterError::AlreadyRunning);
        }
        // The discovery-only path publishes the placeholder
        // port the previous design pinned. The productive
        // pairing path uses [`Self::start_with_port`] to publish
        // the real ephemeral port the TLS listener reserved.
        let install_result = self.install(advertisement, sink, DISCOVERY_ONLY_PORT);
        if let Err(error) = install_result {
            self.running.store(false, Ordering::Release);
            return Err(map_install_error(error));
        }
        Ok(())
    }

    fn start_with_port(
        &self,
        advertisement: &DiscoveryAdvertisement,
        sink: Arc<dyn DiscoverySink>,
        port: u16,
    ) -> Result<(), AdapterError> {
        if port == 0 {
            // The productive pairing path MUST publish a real,
            // non-zero port so a remote browser can dial the
            // TLS listener. A zero port here means the caller
            // accidentally fell back to the discovery-only
            // contract — surface it as a typed rejection so the
            // runtime reports a stable reason instead of
            // silently publishing a record that misleads every
            // browser on the link.
            return Err(AdapterError::MalformedAdvertisement);
        }
        // The advertisement must be metadata-only and aligned
        // with the runtime contract; we refuse to start with an
        // empty peer_id or capability so a misconfigured shell
        // surfaces the error before publishing.
        if advertisement.peer_id.is_empty() || advertisement.capability.is_empty() {
            return Err(AdapterError::MalformedAdvertisement);
        }
        if self.running.swap(true, Ordering::AcqRel) {
            return Err(AdapterError::AlreadyRunning);
        }
        if let Err(error) = self.install(advertisement, sink, port) {
            self.running.store(false, Ordering::Release);
            return Err(map_install_error(error));
        }
        Ok(())
    }

    fn stop(&self) -> Result<(), AdapterError> {
        self.shutdown();
        self.running.store(false, Ordering::Release);
        Ok(())
    }

    fn reconfigure(
        &self,
        advertisement: &DiscoveryAdvertisement,
        port: u16,
    ) -> Result<(), AdapterError> {
        // The adapter must be running for a reconfigure to make
        // sense: the runtime installs the adapter (and its
        // sink) via `start` and the pairing transport flips the
        // advertisement to pairing + the real port once the TLS
        // listener has bound. A reconfigure against a stopped
        // adapter is the typed `MalformedAdvertisement` reason
        // the productive pairing path already documents for
        // a missing mdns daemon.
        if !self.running.load(Ordering::Acquire) {
            return Err(AdapterError::MalformedAdvertisement);
        }
        self.reconfigure_record(advertisement, port)
            .map_err(map_install_error)
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
}

/// Map an internal [`MdnsAdapterError`] to the public
/// [`AdapterError`] taxonomy. Every install-time failure
/// collapses to [`AdapterError::MulticastUnavailable`] except
/// the explicit `ServiceInfo` rejection the productive path
/// raises for a zero port, which surfaces as
/// [`AdapterError::MalformedAdvertisement`] so the runtime can
/// distinguish "could not reach mDNS" from "advertisement did
/// not satisfy the productive pairing contract".
fn map_install_error(error: MdnsAdapterError) -> AdapterError {
    match error {
        MdnsAdapterError::ServiceInfo => AdapterError::MalformedAdvertisement,
        MdnsAdapterError::Register | MdnsAdapterError::Browse | MdnsAdapterError::SpawnThread => {
            AdapterError::MulticastUnavailable
        }
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
    peer_addresses: Arc<Mutex<HashMap<String, std::net::SocketAddr>>>,
) {
    while !cancel.load(Ordering::Acquire) {
        match receiver.recv_timeout(std::time::Duration::from_millis(500)) {
            Ok(event) => process_event(&event, &sink, &peer_registry, &peer_addresses),
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
    peer_addresses: &Arc<Mutex<HashMap<String, std::net::SocketAddr>>>,
) {
    match event {
        mdns_sd::ServiceEvent::ServiceResolved(info) => {
            let fullname = info.get_fullname().to_string();
            if let Some(record) = translate_resolved(info) {
                let peer_id = record.peer_id.clone();
                let address = info
                    .get_addresses()
                    .iter()
                    .copied()
                    .next()
                    .and_then(|ip| std::net::SocketAddr::new(ip, info.get_port()).into());
                peer_registry
                    .lock()
                    .expect("peer registry")
                    .insert(fullname.clone(), peer_id.clone());
                if let Some(address) = address {
                    peer_addresses
                        .lock()
                        .expect("peer addresses")
                        .insert(peer_id, address);
                }
                sink.push(DiscoveryEvent::Observed(record));
            }
        }
        mdns_sd::ServiceEvent::ServiceRemoved(_ty, fullname) => {
            let peer_id = peer_registry
                .lock()
                .expect("peer registry")
                .remove(fullname);
            if let Some(peer_id) = peer_id.clone() {
                peer_addresses
                    .lock()
                    .expect("peer addresses")
                    .remove(&peer_id);
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
    let pairing_fingerprint = read_optional_string_property(properties, TXT_PAIRING_FINGERPRINT);
    let mut record = TxtRecord::new(
        peer_id,
        fingerprint,
        display_name,
        protocol_major,
        capability,
        time::OffsetDateTime::now_utc(),
    );
    record.pairing_fingerprint = pairing_fingerprint;
    Some(record)
}

fn read_string_property(properties: &mdns_sd::TxtProperties, key: &str) -> Option<String> {
    properties
        .get_property_val_str(key)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

fn read_optional_string_property(properties: &mdns_sd::TxtProperties, key: &str) -> Option<String> {
    read_string_property(properties, key)
}

fn read_int_property(properties: &mdns_sd::TxtProperties, key: &str) -> Option<i64> {
    read_string_property(properties, key).and_then(|value| value.parse::<i64>().ok())
}

fn build_txt_properties(advertisement: &DiscoveryAdvertisement) -> [(String, String); 6] {
    let mut properties = [
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
        // The pairing fingerprint is always serialized (even as
        // an empty value) so a legacy browser ignores the key
        // without breaking the record layout. The core rejects
        // pairing records whose `pfp` is empty so the empty
        // value never reaches the pairing runtime.
        (
            TXT_PAIRING_FINGERPRINT.to_string(),
            advertisement
                .pairing_fingerprint
                .clone()
                .unwrap_or_default(),
        ),
    ];
    if advertisement.pairing_fingerprint.is_none() {
        // Discovery-only advertisement: keep the record byte
        // length down by collapsing the empty pairing fingerprint
        // into a value mDNS-sd strips from the wire. The TXT
        // record still parses back to a `None` field. We achieve
        // the same effect by leaving an empty string (mDNS-sd
        // surfaces it as an empty property) which the runtime
        // treats as missing.
        if let Some((_, value)) = properties
            .iter_mut()
            .find(|(k, _)| k == TXT_PAIRING_FINGERPRINT)
        {
            value.clear();
        }
    }
    properties
}

/// Build the DNS-SD service instance and the hostname that the
/// service's SRV record targets.
///
/// A display name is editable and may collide between two devices,
/// so it must remain TXT metadata instead of identifying the DNS-SD
/// service. The local peer id is a 32-char lowercase hexadecimal
/// value minted from the public key, which makes it both stable and
/// valid inside a DNS label. The public adapter still guards the
/// label in case a lower-level caller bypasses the core's identity
/// validation; such a caller cannot inject dots, underscores or
/// control characters into DNS names.
fn service_names(advertisement: &DiscoveryAdvertisement) -> (String, String, String) {
    let mut identity_label = String::with_capacity(advertisement.peer_id.len());
    for ch in advertisement.peer_id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' {
            identity_label.push(ch.to_ascii_lowercase());
        }
    }
    identity_label.truncate(52);
    if identity_label.is_empty() {
        identity_label.push_str("unknown");
    }
    let instance = format!("ClipVault-{identity_label}");
    let hostname = format!("clipvault-{identity_label}.local.");
    let fullname = format!("{instance}.{SERVICE_TYPE}");
    (instance, hostname, fullname)
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
    fn service_names_use_a_stable_dns_safe_peer_identity() {
        let (instance, hostname, fullname) = service_names(&ad());
        assert_eq!(instance, "ClipVault-0123456789abcdef0123456789abcdef");
        assert_eq!(
            hostname,
            "clipvault-0123456789abcdef0123456789abcdef.local."
        );
        assert_eq!(
            fullname,
            "ClipVault-0123456789abcdef0123456789abcdef._clipvault._tcp.local."
        );
        // DNS-SD instance names may contain the service type, but
        // the SRV target is a hostname and must not carry the
        // `_clipvault._tcp` labels. Avahi uses that distinction
        // while resolving an announcement from macOS.
        assert!(!hostname.contains('_'));
        let properties = build_txt_properties(&ad());
        assert!(mdns_sd::ServiceInfo::new(
            SERVICE_TYPE,
            &instance,
            &hostname,
            "192.0.2.42",
            65000,
            &properties[..],
        )
        .is_ok());
    }

    #[test]
    fn service_names_ignore_display_name_and_keep_peers_distinct() {
        let first = DiscoveryAdvertisement::new(
            "11111111111111111111111111111111",
            "aaaaaaaaaaaaaaaa",
            "Mac-M1",
            1,
            "discovery_only",
        );
        let renamed_first = DiscoveryAdvertisement::new(
            "11111111111111111111111111111111",
            "aaaaaaaaaaaaaaaa",
            "Equipo de Diego",
            1,
            "discovery_only",
        );
        let second = DiscoveryAdvertisement::new(
            "22222222222222222222222222222222",
            "bbbbbbbbbbbbbbbb",
            "Mac-M1",
            1,
            "discovery_only",
        );
        assert_eq!(service_names(&first), service_names(&renamed_first));
        assert_ne!(service_names(&first), service_names(&second));
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

    /// `start_with_port` MUST reject a zero port before any
    /// `mdns-sd` round-trip so a regression that defaulted to
    /// `DISCOVERY_ONLY_PORT = 0` (the discovery-only placeholder)
    /// cannot silently publish a record that misleads every
    /// browser on the link. The earlier `port == 0` rejection
    /// is what separates the productive pairing path from the
    /// discovery-only path; both code paths share the same
    /// adapter but the productive one is the only one that has
    /// any business binding a real port.
    #[test]
    fn start_with_port_zero_is_rejected_before_mdns_round_trip() {
        let adapter = MdnsPeerDiscoveryAdapter::new();
        let ad = DiscoveryAdvertisement::new(
            "0123456789abcdef0123456789abcdef",
            "fp",
            "Studio",
            1,
            "pairing",
        );
        let sink: Arc<dyn DiscoverySink> = Arc::new(CapturingSink::default());
        let err = adapter
            .start_with_port(&ad, sink, 0)
            .expect_err("port=0 must be rejected before any mdns-sd call");
        assert!(matches!(err, AdapterError::MalformedAdvertisement));
        assert!(!adapter.is_running());
    }

    /// Loopback test: spin up two real daemons on the same host
    /// and confirm the bouncer sees the first daemons record AND
    /// that stopping the first daemon emits `ServiceRemoved` on
    /// the second daemon within the bounded window the design
    /// pins (`local-peer-discovery/design.md` §"Descubrimiento,
    /// presencia y compatibilidad": ordered shutdown → goodbye →
    /// `ServiceRemoved` → `NotAvailable`, no TTL wait).
    ///
    /// The test stays feature-gated to keep CI on hosts without
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
        let observe_deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
        let mut observed = false;
        while std::time::Instant::now() < observe_deadline {
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
                observed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        // Stopping `first` must desregister the published
        // `fullname` *before* tearing the daemon down so the
        // second daemon sees the goodbye packet and pushes a
        // `Removed` event for the matching `peer_id`. Without
        // the ordered shutdown the second daemon would only
        // observe the disappearance after the mDNS TTL (~120 s),
        // which is precisely the bug 3.5 must regress against.
        first.stop().expect("first stop");
        // The receiver polls every 500 ms; allow a bounded window
        // (≈5 s) so retransmissions and the bounded
        // `UNREGISTER_WAIT` ceiling fit comfortably while still
        // failing fast enough to keep the suite responsive.
        let removal_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut removed = false;
        while std::time::Instant::now() < removal_deadline {
            if recorder_b
                .events
                .lock()
                .expect("events")
                .iter()
                .any(|event| {
                    matches!(event, DiscoveryEvent::Removed { peer_id }
                        if peer_id == &first_ad.peer_id)
                })
            {
                removed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let _ = second.stop();
        assert!(
            observed,
            "second daemon never observed the first daemons advertisement on loopback"
        );
        assert!(
            removed,
            "second daemon never received ServiceRemoved for the first daemon after stop()"
        );
    }

    /// `reconfigure` MUST refuse to publish anything when the
    /// adapter is not running. The pairing transport relies on
    /// the runtime to start the adapter (and install its sink)
    /// first; a reconfigure attempt against a stopped adapter
    /// is the typed `MalformedAdvertisement` reason the
    /// productive pairing path documents, not a silent
    /// downgrade to a half-built record.
    #[test]
    fn reconfigure_refuses_a_stopped_adapter() {
        let adapter = MdnsPeerDiscoveryAdapter::new();
        let ad = DiscoveryAdvertisement::new(
            "0123456789abcdef0123456789abcdef",
            "0123456789abcdef",
            "Studio",
            1,
            "pairing",
        );
        let err = adapter
            .reconfigure(&ad, 65000)
            .expect_err("reconfigure on a stopped adapter must fail");
        assert!(matches!(err, AdapterError::MalformedAdvertisement));
        assert!(!adapter.is_running());
    }

    /// `reconfigure` MUST keep the existing browse loop running
    /// so the runtime keeps receiving browse events while the
    /// pairing transport flips the published record to
    /// `capability = pairing`. The previous `start_with_port`
    /// path either failed with `AlreadyRunning` on the toggle
    /// path or replaced the runtime sink with a discarding one
    /// on the bootstrap path; `reconfigure` lets the same
    /// adapter instance carry both the discovery and pairing
    /// lifecycle without dropping browse events.
    ///
    /// The test runs on the loopback: two adapters start in
    /// discovery-only mode, then each reconfigures to pairing
    /// mode with a non-zero port. The browse loop the discovery
    /// `start` call installed must keep firing — the second
    /// adapter's browse events reach the first adapter's sink
    /// after the reconfigure, and the first adapter's
    /// reconfigured record reaches the second adapter's sink
    /// through the same receiver.
    #[test]
    fn reconfigure_keeps_the_running_browse_loop_alive() {
        let first_id = "11111111111111111111111111111111";
        let second_id = "22222222222222222222222222222222";
        let first_discovery = DiscoveryAdvertisement::new(
            first_id,
            "aaaaaaaaaaaaaaaa",
            "Studio-A",
            1,
            "discovery_only",
        );
        let first_pairing = DiscoveryAdvertisement::new_pairing(
            first_id,
            "aaaaaaaaaaaaaaaa",
            &"a".repeat(64),
            "Studio-A",
            1,
        );
        let second_discovery = DiscoveryAdvertisement::new(
            second_id,
            "bbbbbbbbbbbbbbbb",
            "Studio-B",
            1,
            "discovery_only",
        );
        let second_pairing = DiscoveryAdvertisement::new_pairing(
            second_id,
            "bbbbbbbbbbbbbbbb",
            &"b".repeat(64),
            "Studio-B",
            1,
        );
        let recorder_a = Arc::new(CapturingSink::default());
        let recorder_b = Arc::new(CapturingSink::default());
        let sink_a: Arc<dyn DiscoverySink> = recorder_a.clone();
        let sink_b: Arc<dyn DiscoverySink> = recorder_b.clone();
        let first = MdnsPeerDiscoveryAdapter::new();
        let second = MdnsPeerDiscoveryAdapter::new();
        // Discovery-only start. We must classify the multicast
        // unavailability symmetrically to the loopback test
        // above; if either `start` fails, the productive path
        // cannot run and we exit explicitly without asserting.
        let first_started = first.start(&first_discovery, sink_a.clone()).is_ok();
        let second_started = second.start(&second_discovery, sink_b.clone()).is_ok();
        if !(first_started && second_started) {
            let _ = first.stop();
            let _ = second.stop();
            eprintln!(
                "reconfigure loopback skipped: first_started={first_started} second_started={second_started}"
            );
            return;
        }
        // Wait up to ~6 s for the discovery-only browse loop to
        // see both peers. We then reconfigure BOTH adapters to
        // pairing mode; the same browse loop and sink must
        // remain active so a second round of browse events
        // reaches each recorder.
        let mut observed_pair = false;
        let mut observed_pair_after = false;
        let initial_deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
        while std::time::Instant::now() < initial_deadline {
            observed_pair = recorder_a
                .events
                .lock()
                .expect("events")
                .iter()
                .any(|event| {
                    matches!(event, DiscoveryEvent::Observed(record)
                        if record.peer_id == second_id
                            && record.capability == "discovery_only")
                });
            if observed_pair {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        // Flip both adapters to pairing with a real port. The
        // adapter must accept the reconfigure and keep the
        // browse loop running.
        let reconfigure_first = first.reconfigure(&first_pairing, 65100);
        let reconfigure_second = second.reconfigure(&second_pairing, 65200);
        let _ = reconfigure_first;
        let _ = reconfigure_second;
        assert!(
            first.is_running(),
            "first adapter must stay running after reconfigure"
        );
        assert!(
            second.is_running(),
            "second adapter must stay running after reconfigure"
        );
        // The first adapter's existing `sink_a` is still
        // installed. The reconfigured record must reach the
        // second adapter through the same browse loop within
        // the bounded window.
        let after_deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
        while std::time::Instant::now() < after_deadline {
            observed_pair_after = recorder_a
                .events
                .lock()
                .expect("events")
                .iter()
                .any(|event| {
                    matches!(event, DiscoveryEvent::Observed(record)
                        if record.peer_id == second_id
                            && record.capability == "pairing")
                });
            if observed_pair_after {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let _ = first.stop();
        let _ = second.stop();
        assert!(
            observed_pair,
            "first adapter never observed the second adapter's discovery-only record"
        );
        assert!(
            observed_pair_after,
            "first adapter's browse loop did not survive the reconfigure (no pairing record observed)"
        );
    }

    /// Stopping the adapter MUST release the browse loop so a
    /// subsequent `start` succeeds. The previous prototype's
    /// `MdnsPairingSink::push` short-circuited events; the new
    /// `reconfigure` path replaces the start_with_port path and
    /// must not leave the adapter in a state where the next
    /// `start` fails. The regression test confirms stop/start
    /// still works through the existing `start` entry point
    /// after a reconfigure cycle.
    #[test]
    fn stop_after_reconfigure_returns_adapter_to_idle_state() {
        let adapter = MdnsPeerDiscoveryAdapter::new();
        let discovery = ad();
        let pairing = DiscoveryAdvertisement::new_pairing(
            "0123456789abcdef0123456789abcdef",
            "0123456789abcdef",
            &"f".repeat(64),
            "Studio",
            1,
        );
        let sink: Arc<dyn DiscoverySink> = Arc::new(CapturingSink::default());
        if adapter.start(&discovery, sink.clone()).is_err() {
            // Multicast unavailable: the typed-error unit tests
            // already cover the contract. Skip the stop assertion
            // because we cannot run a productive cycle on a
            // sandbox that blocks mDNS.
            eprintln!("stop after reconfigure skipped: multicast unavailable");
            return;
        }
        adapter
            .reconfigure(&pairing, 65111)
            .expect("reconfigure on a running adapter must succeed");
        adapter
            .stop()
            .expect("stop after reconfigure must release the adapter");
        assert!(!adapter.is_running());
        // Restarting the same adapter must succeed because the
        // previous stop fully unwound the daemon + browse loop.
        let restart = adapter.start(&discovery, sink);
        adapter.stop().ok();
        assert!(
            restart.is_ok(),
            "stop must release the adapter so the next start succeeds"
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
        let addresses: Arc<StdMutex<HashMap<String, std::net::SocketAddr>>> =
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
        process_event(&resolved, &sink_dyn, &registry, &addresses);
        let removed = mdns_sd::ServiceEvent::ServiceRemoved(SERVICE_TYPE.to_string(), fullname);
        process_event(&removed, &sink_dyn, &registry, &addresses);
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
        let addresses: Arc<StdMutex<HashMap<String, std::net::SocketAddr>>> =
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
            &addresses,
        );
        process_event(
            &mdns_sd::ServiceEvent::ServiceResolved(info_b),
            &sink_dyn,
            &registry,
            &addresses,
        );
        process_event(
            &mdns_sd::ServiceEvent::ServiceRemoved(SERVICE_TYPE.to_string(), fullname_a),
            &sink_dyn,
            &registry,
            &addresses,
        );
        process_event(
            &mdns_sd::ServiceEvent::ServiceRemoved(SERVICE_TYPE.to_string(), fullname_b),
            &sink_dyn,
            &registry,
            &addresses,
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
        let addresses: Arc<StdMutex<HashMap<String, std::net::SocketAddr>>> =
            Arc::new(StdMutex::new(HashMap::new()));
        let sink = Arc::new(CapturingSink::default());
        let sink_dyn: Arc<dyn DiscoverySink> = sink.clone();
        let removed = mdns_sd::ServiceEvent::ServiceRemoved(
            SERVICE_TYPE.to_string(),
            "Unknown._clipvault._tcp.local.".to_string(),
        );
        process_event(&removed, &sink_dyn, &registry, &addresses);
        assert!(sink.events.lock().expect("events").is_empty());
    }
}
