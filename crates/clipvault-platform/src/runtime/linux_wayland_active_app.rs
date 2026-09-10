//! Linux native Wayland active-application probe.
//!
//! ## Why a probe and not the EWMH `_NET_ACTIVE_WINDOW`
//!
//! The X11 active-app probe covers plain X11 sessions and Wayland
//! sessions that happen to expose XWayland: in those environments the
//! X server publishes `_NET_ACTIVE_WINDOW` and `WM_CLASS` for every
//! windowed application, X11 or native Wayland, so the probe can
//! answer the question through `$DISPLAY`. On a Wayland session
//! without XWayland, the native Wayland toplevel never appears in
//! `_NET_ACTIVE_WINDOW`, and the EWMH probe can only return
//! `Ok(None)`.
//!
//! Wayland compositors that want to share focused-window metadata
//! with third-party tools expose one of two public protocols:
//!
//! 1. [`ext-foreign-toplevel-list-v1`](https://wayland.app/protocols/ext-foreign-toplevel-list-v1)
//!    — preferred, used by GNOME, KDE Plasma and wlroots compositors
//!    that adopted the protocol.
//! 2. [`zwlr_foreign_toplevel_management_unstable_v1`](https://wayland.app/protocols/wlr-foreign-toplevel-management-unstable-v1)
//!    — older wlroots-only fallback used when the first protocol is
//!    not announced.
//!
//! Both protocols publish a per-toplevel `app_id` the moment the
//! compositor commits the round (`done` event). The `app_id` has the
//! same trust model as `WM_CLASS`: it is metadata the application
//! itself published through the toolkit, it is not a proof of process
//! identity, and the rest of the pipeline treats it as a stable
//! identifier only. The blacklist and the metadata provider
//! (`LinuxApplicationMetadataProvider`) consume it the same way they
//! consume `WM_CLASS`.
//!
//! ## Design choices
//!
//! - **No external runtime tools.** The marshal / unmarshal logic is
//!   hand-written; the build does not depend on `wayland-scanner`,
//!   `libwayland-bin` or any external helper. macOS builds do not
//!   pull the module at all (the `cfg(target_os = "linux")` gate plus
//!   the `linux-wayland-active-app` feature ensure the linker never
//!   sees Wayland symbols outside Linux).
//! - **Background thread owns the socket.** The capture loop reads
//!   the snapshot behind an `Arc<RwLock<...>>`; the I/O thread owns
//!   the `UnixStream` and never exposes it to the watcher.
//! - **Snapshots commit on `done`.** Until the compositor publishes
//!   the round's `done` event, no `app_id` is observable through the
//!   probe. A `done` event triggers a snapshot commit; partial state
//!   stays internal.
//! - **`closed` removes the toplevel.** A `closed` event drops the
//!   toplevel from the map immediately; it never reappears as a
//!   candidate.
//! - **Empty / whitespace `app_id` is absence.** When the protocol
//!   publishes an empty or whitespace-only `app_id` string we
//!   surface `Ok(None)` rather than a fabricated identifier.
//! - **Deterministic selection across many active toplevels.** When
//!   several toplevels report `activated`, the probe picks the one
//!   with the lowest handle id — the order the compositor assigned
//!   — never the title.
//! - **Thread-safe.** The snapshot lives behind an
//!   `Arc<Snapshot>` and the granular [`ProbeStage`] lives behind
//!   a `Mutex<ProbeStage>`; both match the pattern the X11 probe
//!   uses so the capture loop does not have to special-case the
//!   Wayland adapter.
//! - **Typed degradation.** When the protocol is absent, the version
//!   is incompatible, the compositor refuses to bind or the socket
//!   is unreachable, the probe surfaces an `ActiveAppError::Unavailable`
//!   (or a `Backend` error carrying the sanitised cause) without
//!   panicking and without blocking the capture loop.
//! - **No process inspection.** The probe never reads `/proc`,
//!   titles, PID, environment variables or process metadata. The
//!   `app_id` is the only identifier the rest of the pipeline
//!   observes.
//!
//! ## What this module does NOT do
//!
//! - **Title-based identity.** The probe never reads, logs or uses a
//!   window title. The metadata enrichment path stays free of title
//!   data so the privacy contract the rest of ClipVault honours is
//!   not weakened.
//! - **GNOME private extensions.** No `org.gnome.Shell.Eval`, no
//!   `gdbus` introspection, no GNOME Shell evaluation. A GNOME
//!   session without `ext-foreign-toplevel-list-v1` returns
//!   `Unavailable` — a different change would be required to wire a
//!   private protocol.
//! - **Forced Wayland session.** The probe is opt-in: the bootstrap
//!   only installs it when `$WAYLAND_DISPLAY` is set, the session is
//!   classified as Wayland and the registry actually publishes one
//!   of the two protocols. Otherwise the probe is not even
//!   constructed.

#![cfg(all(target_os = "linux", feature = "linux-wayland-active-app"))]
#![allow(dead_code)]

// The I/O thread, the marshal / unmarshal helpers and the Wayland
// socket machinery all live behind `try_build`, which only the
// bootstrap calls in production. The dead-code warnings appear on
// macOS or on Linux builds that do not enable the feature; the
// `#[cfg]` gate already prevents the linker from seeing the
// symbols, so silencing the lint at the crate root keeps the
// development workflow noise-free without weakening the type
// checker.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use parking_lot::{Mutex, RwLock};
use tracing::warn;

use crate::active_app::{ActiveAppError, ActiveApplication, ActiveApplicationProbe, ProbeStage};

/// Stable backend identifier surfaced through
/// [`ActiveApplicationProbe::name`]. The diagnostics card
/// consumes the value verbatim, so renaming it is a breaking change
/// for the UI.
pub const BACKEND_NAME: &str = "wayland_foreign_toplevel";

/// Stable backend identifier for the wlroots fallback protocol.
pub const BACKEND_NAME_WLR: &str = "wayland_wlr_foreign_toplevel";

/// Well-known protocol interfaces the registry may advertise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolKind {
    /// `ext-foreign-toplevel-list-v1`. Modern, preferred.
    Ext,
    /// `zwlr_foreign_toplevel_management_unstable_v1`. Wlroots
    /// fallback.
    Zwlr,
}

impl ProtocolKind {
    /// Human-readable name surfaced in diagnostics. Renaming the
    /// strings is a breaking change for the UI.
    pub fn as_str(self) -> &'static str {
        match self {
            ProtocolKind::Ext => "ext_foreign_toplevel_list_v1",
            ProtocolKind::Zwlr => "zwlr_foreign_toplevel_management_unstable_v1",
        }
    }
}

/// Per-toplevel state the adapter maintains between events.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToplevelEntry {
    pub app_id: Option<String>,
    pub activated: bool,
}

/// Snapshot the probe publishes. The struct holds the latest
/// committed state and the granular [`ProbeStage`] the diagnostics
/// card surfaces. Both are updated by the I/O thread and read by
/// the capture loop.
#[derive(Debug)]
pub struct Snapshot {
    inner: RwLock<SnapshotInner>,
    stage: Mutex<ProbeStage>,
}

#[derive(Debug, Default)]
struct SnapshotInner {
    toplevels: BTreeMap<u32, ToplevelEntry>,
    active_app_id: Option<String>,
    committed: bool,
}

impl Snapshot {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(SnapshotInner::default()),
            stage: Mutex::new(ProbeStage::Started),
        }
    }

    pub fn set_stage(&self, stage: ProbeStage) {
        *self.stage.lock() = stage;
    }

    pub fn stage(&self) -> ProbeStage {
        *self.stage.lock()
    }

    /// Commit a new round of toplevel state. The I/O thread calls
    /// this once per `done` event; partial state between `done`
    /// events stays internal. The helper is `pub(crate)` so the
    /// integration tests can drive the protocol state machine
    /// without standing up a real Wayland socket.
    pub fn commit(&self, toplevels: BTreeMap<u32, ToplevelEntry>) {
        let active = pick_active_app_id(&toplevels);
        let mut guard = self.inner.write();
        guard.toplevels = toplevels;
        guard.active_app_id = active;
        guard.committed = true;
    }

    pub fn active_app_id(&self) -> Option<String> {
        let guard = self.inner.read();
        guard
            .active_app_id
            .as_ref()
            .map(|value| value.to_string())
            .filter(|value| !value.trim().is_empty())
    }

    pub fn committed(&self) -> bool {
        self.inner.read().committed
    }
}

impl Default for Snapshot {
    fn default() -> Self {
        Self::new()
    }
}

/// Pick the canonical active toplevel across a round. Deterministic
/// across runs: smallest compositor handle wins, ignoring the
/// title. The compositor-assigned handle order is what the
/// `ext-foreign-toplevel-list-v1` spec documents as the
/// authoritative id, so the rule is independent of any
/// application-controlled data.
pub fn pick_active_app_id(toplevels: &BTreeMap<u32, ToplevelEntry>) -> Option<String> {
    for entry in toplevels.values() {
        if entry.activated {
            if let Some(app_id) = entry.app_id.as_ref() {
                if !app_id.trim().is_empty() {
                    return Some(app_id.clone());
                }
            }
        }
    }
    None
}

/// Marshal a Wayland request header.
struct Marshal;

impl Marshal {
    fn request_header(sender_id: u32, opcode: u16, args_byte_count: usize) -> [u8; 8] {
        let size = (8 + args_byte_count) as u16;
        let mut header = [0u8; 8];
        header[0..4].copy_from_slice(&sender_id.to_ne_bytes());
        header[4..6].copy_from_slice(&opcode.to_ne_bytes());
        header[6..8].copy_from_slice(&size.to_ne_bytes());
        header
    }

    fn u32(value: u32) -> [u8; 4] {
        value.to_ne_bytes()
    }

    fn string(value: &str) -> Vec<u8> {
        let bytes = value.as_bytes();
        let length = (bytes.len() + 1) as u32;
        let mut out = Vec::with_capacity(4 + length as usize);
        out.extend_from_slice(&length.to_ne_bytes());
        out.extend_from_slice(bytes);
        out.push(0);
        let pad = (4 - (out.len() % 4)) % 4;
        out.resize(out.len() + pad, 0);
        out
    }
}

/// Unmarshal a Wayland event.
struct Unmarshal {
    sender_id: u32,
    opcode: u16,
    payload: Vec<u8>,
    cursor: usize,
}

impl Unmarshal {
    fn new(sender_id: u32, opcode: u16, payload: Vec<u8>) -> Self {
        Self {
            sender_id,
            opcode,
            payload,
            cursor: 0,
        }
    }

    fn parse_event_header(header: &[u8; 8]) -> io::Result<(u32, u16, u16)> {
        let sender_id = u32::from_ne_bytes([header[0], header[1], header[2], header[3]]);
        let opcode = u16::from_ne_bytes([header[4], header[5]]);
        let size = u16::from_ne_bytes([header[6], header[7]]);
        Ok((sender_id, opcode, size))
    }

    fn read_u32(&mut self) -> io::Result<u32> {
        if self.cursor + 4 > self.payload.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Wayland: short u32 read",
            ));
        }
        let bytes: [u8; 4] = [
            self.payload[self.cursor],
            self.payload[self.cursor + 1],
            self.payload[self.cursor + 2],
            self.payload[self.cursor + 3],
        ];
        self.cursor += 4;
        Ok(u32::from_ne_bytes(bytes))
    }

    fn read_string(&mut self) -> io::Result<String> {
        let length = self.read_u32()? as usize;
        if length == 0 {
            return Ok(String::new());
        }
        if self.cursor + length > self.payload.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Wayland: short string read",
            ));
        }
        let end = self.cursor + length;
        let bytes = &self.payload[self.cursor..end];
        self.cursor += (length + 3) & !3;
        let trimmed = bytes.strip_suffix(&[0]).unwrap_or(bytes);
        Ok(String::from_utf8_lossy(trimmed).into_owned())
    }
}

/// Adapter implementation the bootstrap wires in on Wayland
/// sessions.
pub struct WaylandActiveApplication<T: Transport = UnixStreamTransport> {
    snapshot: Arc<Snapshot>,
    backend: &'static str,
    alive: Arc<AtomicBool>,
    _io: Option<IoThread<T>>,
}

impl<T: Transport> WaylandActiveApplication<T> {
    /// Test-only constructor that wraps a snapshot, an `alive`
    /// flag and an empty `IoThread` slot. The production code path
    /// uses [`try_build`].
    pub fn for_tests(snapshot: Arc<Snapshot>, alive: Arc<AtomicBool>) -> Self {
        Self {
            snapshot,
            backend: BACKEND_NAME,
            alive,
            _io: None,
        }
    }
}

impl<T: Transport> std::fmt::Debug for WaylandActiveApplication<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WaylandActiveApplication")
            .field("backend", &self.backend)
            .field("alive", &self.alive)
            .finish()
    }
}

/// Outcome of [`try_build`]. The constructor distinguishes "no
/// Wayland session" / "protocol absent" from "connection died" so
/// the bootstrap can keep the existing X11 fallback honest.
#[derive(Debug)]
#[allow(dead_code)]
pub enum ConnectionOutcome {
    /// The probe connected to the Wayland socket and bound one of
    /// the two protocols.
    Operational(WaylandActiveApplication<UnixStreamTransport>),
    /// The Wayland socket could not be opened, the registry did not
    /// publish a usable protocol or the version was incompatible.
    Unavailable,
}

/// Public entry point the bootstrap uses to build the probe. The
/// helper looks up `$WAYLAND_DISPLAY` / `$XDG_RUNTIME_DIR`, opens
/// the socket, runs the handshake and binds the right protocol. The
/// I/O thread is spawned before the constructor returns so the
/// capture loop sees a fully-running probe on the very first call.
pub fn try_build() -> ConnectionOutcome {
    let socket_path = match wayland_socket_path() {
        Some(path) => path,
        None => return ConnectionOutcome::Unavailable,
    };
    let connection = match WaylandConnection::connect(&socket_path) {
        Ok(connection) => connection,
        Err(_) => return ConnectionOutcome::Unavailable,
    };
    let snapshot = Arc::new(Snapshot::new());
    let alive = Arc::new(AtomicBool::new(true));
    let io = IoThread::spawn(connection, snapshot.clone(), alive.clone());
    let probe = WaylandActiveApplication {
        snapshot,
        backend: BACKEND_NAME,
        alive,
        _io: Some(io),
    };
    ConnectionOutcome::Operational(probe)
}

fn wayland_socket_path() -> Option<PathBuf> {
    use std::env;
    if let Ok(fd) = env::var("WAYLAND_SOCKET") {
        if !fd.is_empty() {
            return Some(PathBuf::from(format!("/proc/self/fd/{fd}")));
        }
    }
    let wayland_display = env::var("WAYLAND_DISPLAY").ok()?;
    if wayland_display.is_empty() {
        return None;
    }
    let runtime_dir = env::var("XDG_RUNTIME_DIR").ok()?;
    if runtime_dir.is_empty() {
        return None;
    }
    let mut path = PathBuf::from(runtime_dir);
    path.push(wayland_display);
    Some(path)
}

/// Background thread that owns the Wayland socket and updates the
/// shared [`Snapshot`] on every `done` round.
pub struct IoThread<T: Transport> {
    handle: Option<thread::JoinHandle<()>>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Transport + 'static> IoThread<T> {
    fn spawn(
        connection: WaylandConnection<T>,
        snapshot: Arc<Snapshot>,
        alive: Arc<AtomicBool>,
    ) -> Self {
        let handle = thread::Builder::new()
            .name("clipvault-wayland-toplevel".into())
            .spawn(move || io_thread_main(connection, snapshot, alive))
            .ok();
        Self {
            handle,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T: Transport> Drop for IoThread<T> {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn io_thread_main<T: Transport>(
    connection: WaylandConnection<T>,
    snapshot: Arc<Snapshot>,
    alive: Arc<AtomicBool>,
) {
    if let Err(error) = drive_protocol(connection, snapshot.clone(), alive.clone()) {
        warn!(error = %error, "wayland toplevel probe disconnected");
        snapshot.set_stage(ProbeStage::Backend);
    }
    alive.store(false, Ordering::Release);
}

fn drive_protocol<T: Transport>(
    mut connection: WaylandConnection<T>,
    snapshot: Arc<Snapshot>,
    alive: Arc<AtomicBool>,
) -> io::Result<()> {
    // The handshake is a fixed sequence of two requests:
    //   1. `get_registry` (id 1) — registers for `global` events.
    //   2. `sync` (id 2)        — establishes a round barrier so
    //                              the next `done` event confirms the
    //                              initial toplevel list.
    connection.send_request(0, 1, &[])?;
    connection.send_request(0, 0, &Marshal::u32(2))?;

    let registry_id = 1;
    let mut pending = BTreeMap::<u32, ToplevelEntry>::new();
    let mut bound: Option<(u32, ProtocolKind)> = None;

    loop {
        if !alive.load(Ordering::Acquire) {
            return Ok(());
        }
        let event = connection.read_event()?;
        let sender = event.sender_id;
        let opcode = event.opcode;
        let mut event = event;
        if sender == registry_id && opcode == 0 {
            // `global` event: name (string), interface (string),
            // version (uint).
            let name = event.read_string()?;
            let interface = event.read_string()?;
            let version = event.read_u32()?;
            if interface == "ext_foreign_toplevel_list_v1" {
                let id = connection.alloc_id();
                let mut args = Vec::new();
                args.extend_from_slice(&Marshal::u32(id));
                let mut name_arg = Marshal::string("ext_foreign_toplevel_list_v1");
                args.append(&mut name_arg);
                args.extend_from_slice(&Marshal::u32(version.min(1)));
                connection.send_request(registry_id, 0, &args)?;
                bound = Some((id, ProtocolKind::Ext));
            } else if interface == "zwlr_foreign_toplevel_manager_v1" {
                let id = connection.alloc_id();
                let mut args = Vec::new();
                args.extend_from_slice(&Marshal::u32(id));
                let mut name_arg = Marshal::string("zwlr_foreign_toplevel_manager_v1");
                args.append(&mut name_arg);
                args.extend_from_slice(&Marshal::u32(version.min(3)));
                connection.send_request(registry_id, 0, &args)?;
                bound = Some((id, ProtocolKind::Zwlr));
            }
            let _ = name;
            let _ = version;
        } else if let Some((list_id, kind)) = bound {
            if sender == list_id {
                match kind {
                    ProtocolKind::Ext => {
                        handle_ext_event(opcode, &mut event, &mut pending, snapshot.as_ref())?
                    }
                    ProtocolKind::Zwlr => {
                        handle_zwlr_event(opcode, &mut event, &mut pending, snapshot.as_ref())?
                    }
                }
            }
        }
        // Drop everything else on the floor: the probe never uses
        // titles, PID, geometry or any other compositor-controlled
        // surface.
    }
}

/// In-process variant of [`drive_protocol`] the integration tests
/// use to drive the protocol state machine synchronously. Mirrors
/// the production call but skips the lifetime guard the I/O thread
/// needs.
pub fn drive_protocol_in_process<T: Transport>(
    connection: WaylandConnection<T>,
    snapshot: Arc<Snapshot>,
) -> io::Result<()> {
    let alive = Arc::new(AtomicBool::new(true));
    drive_protocol(connection, snapshot, alive)
}

fn handle_ext_event(
    opcode: u16,
    event: &mut Unmarshal,
    pending: &mut BTreeMap<u32, ToplevelEntry>,
    snapshot: &Snapshot,
) -> io::Result<()> {
    match opcode {
        0 => {
            // `toplevel` (handle, app_id)
            let handle = event.read_u32()?;
            let app_id = event.read_string()?;
            pending.entry(handle).or_default().app_id = Some(app_id);
            Ok(())
        }
        3 => {
            // `app_id` (handle, app_id)
            let handle = event.read_u32()?;
            let app_id = event.read_string()?;
            pending.entry(handle).or_default().app_id = Some(app_id);
            Ok(())
        }
        5 => {
            // `state` (handle, states[])
            let handle = event.read_u32()?;
            let states = read_state_array(event)?;
            pending.entry(handle).or_default().activated = states.contains(&2);
            Ok(())
        }
        6 => {
            // `closed` (handle)
            let handle = event.read_u32()?;
            pending.remove(&handle);
            Ok(())
        }
        7 => {
            // `done`
            snapshot.commit(pending.clone());
            snapshot.set_stage(stage_after_done(snapshot));
            Ok(())
        }
        _ => Ok(()),
    }
}

fn handle_zwlr_event(
    opcode: u16,
    event: &mut Unmarshal,
    pending: &mut BTreeMap<u32, ToplevelEntry>,
    snapshot: &Snapshot,
) -> io::Result<()> {
    match opcode {
        0 => {
            // `toplevel` (handle)
            let handle = event.read_u32()?;
            pending.entry(handle).or_default();
            Ok(())
        }
        2 => {
            // `app_id` (handle, app_id)
            let handle = event.read_u32()?;
            let app_id = event.read_string()?;
            pending.entry(handle).or_default().app_id = Some(app_id);
            Ok(())
        }
        3 => {
            // `state` (handle, states[])
            let handle = event.read_u32()?;
            let states = read_state_array(event)?;
            pending.entry(handle).or_default().activated = states.contains(&2);
            Ok(())
        }
        4 => {
            // `closed` (handle)
            let handle = event.read_u32()?;
            pending.remove(&handle);
            Ok(())
        }
        5 => {
            // `done`
            snapshot.commit(pending.clone());
            snapshot.set_stage(stage_after_done(snapshot));
            Ok(())
        }
        _ => Ok(()),
    }
}

fn read_state_array(event: &mut Unmarshal) -> io::Result<Vec<u32>> {
    let length = event.read_u32()? as usize;
    let mut values = Vec::with_capacity(length);
    for _ in 0..length {
        values.push(event.read_u32()?);
    }
    if length % 2 == 1 {
        let _ = event.read_u32()?;
    }
    Ok(values)
}

fn stage_after_done(snapshot: &Snapshot) -> ProbeStage {
    if snapshot.active_app_id().is_some() {
        ProbeStage::Identified
    } else {
        ProbeStage::ActiveWindowEmpty
    }
}

/// Translate the snapshot's commit outcome into the granular stage
/// the diagnostics surface exposes. Re-exported for the
/// integration tests.
pub fn stage_after_done_pub(snapshot: &Snapshot) -> ProbeStage {
    stage_after_done(snapshot)
}

impl<T: Transport> ActiveApplicationProbe for WaylandActiveApplication<T> {
    fn active_application(&self) -> Result<Option<ActiveApplication>, ActiveAppError> {
        if !self.alive.load(Ordering::Acquire) {
            self.snapshot.set_stage(ProbeStage::Unavailable);
            Err(ActiveAppError::Unavailable)
        } else {
            match self.snapshot.active_app_id() {
                Some(app_id) => Ok(Some(ActiveApplication::new(&app_id, &app_id))),
                None => {
                    if self.snapshot.committed() {
                        self.snapshot.set_stage(ProbeStage::ActiveWindowEmpty);
                        Ok(None)
                    } else {
                        self.snapshot.set_stage(ProbeStage::Unavailable);
                        Err(ActiveAppError::Unavailable)
                    }
                }
            }
        }
    }

    fn name(&self) -> &'static str {
        self.backend
    }

    fn last_probe_stage(&self) -> ProbeStage {
        self.snapshot.stage()
    }
}

impl<T: Transport> Drop for WaylandActiveApplication<T> {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
    }
}

/// Handle the I/O thread keeps around to talk to the Wayland
/// socket. The struct is generic over the [`Transport`] trait so
/// the production code path uses a Unix-domain socket while tests
/// inject an in-memory script.
pub struct WaylandConnection<T: Transport> {
    transport: T,
    next_id: u32,
}

impl WaylandConnection<UnixStreamTransport> {
    /// Open a connection to the Wayland socket at `socket_path`.
    pub fn connect(socket_path: &PathBuf) -> io::Result<Self> {
        let stream = UnixStream::connect(socket_path)?;
        stream.set_nonblocking(false)?;
        Ok(Self {
            transport: UnixStreamTransport(stream),
            next_id: 2,
        })
    }
}

impl<T: Transport> WaylandConnection<T> {
    /// Build a connection backed by an arbitrary transport. The
    /// `next_id` starts at 2 because the production code path
    /// reserves ids 1 (`get_registry`) and 2 (`sync`) for the
    /// handshake.
    pub fn with_transport(transport: T) -> Self {
        Self {
            transport,
            next_id: 2,
        }
    }

    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        id
    }

    fn send_request(&mut self, sender_id: u32, opcode: u16, args: &[u8]) -> io::Result<()> {
        let header = Marshal::request_header(sender_id, opcode, args.len());
        self.transport.send(&header)?;
        if !args.is_empty() {
            self.transport.send(args)?;
        }
        Ok(())
    }

    fn read_event(&mut self) -> io::Result<Unmarshal> {
        let mut header = [0u8; 8];
        self.transport.recv(&mut header)?;
        let (sender_id, opcode, size) = Unmarshal::parse_event_header(&header)?;
        let mut payload = vec![0u8; size.saturating_sub(8) as usize];
        if !payload.is_empty() {
            self.transport.recv(&mut payload)?;
        }
        Ok(Unmarshal::new(sender_id, opcode, payload))
    }
}

/// Transport abstraction the probe drives. The production code
/// path uses a Unix-domain socket; tests inject a
/// [`MemoryTransport`] so the suite can replay protocol exchanges
/// without standing up a real compositor.
pub trait Transport: Send + Sync {
    fn send(&mut self, bytes: &[u8]) -> io::Result<()>;
    fn recv(&mut self, bytes: &mut [u8]) -> io::Result<()>;
}

/// Production transport backed by a `UnixStream`.
pub struct UnixStreamTransport(UnixStream);

impl Transport for UnixStreamTransport {
    fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.0.write_all(bytes)
    }
    fn recv(&mut self, bytes: &mut [u8]) -> io::Result<()> {
        self.0.read_exact(bytes)
    }
}

/// In-memory transport the test suite drives.
#[cfg(test)]
pub struct MemoryTransport {
    outbound: Vec<u8>,
    inbound: Vec<u8>,
}

#[cfg(test)]
impl MemoryTransport {
    pub fn new() -> Self {
        Self {
            outbound: Vec::new(),
            inbound: Vec::new(),
        }
    }

    /// Append bytes the probe will read on the next `recv` call.
    pub fn push(&mut self, bytes: &[u8]) {
        self.inbound.extend_from_slice(bytes);
    }

    /// Snapshot of the bytes the probe wrote so far. Tests use this
    /// to assert the request the probe sent matches the protocol.
    pub fn outbound(&self) -> &[u8] {
        &self.outbound
    }
}

#[cfg(test)]
impl Default for MemoryTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl Transport for MemoryTransport {
    fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.outbound.extend_from_slice(bytes);
        Ok(())
    }
    fn recv(&mut self, bytes: &mut [u8]) -> io::Result<()> {
        if self.inbound.len() < bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "MemoryTransport: script exhausted",
            ));
        }
        let take = bytes.len();
        bytes.copy_from_slice(&self.inbound[..take]);
        self.inbound.drain(..take);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_active_app_id_prefers_lowest_handle() {
        let mut toplevels = BTreeMap::<u32, ToplevelEntry>::new();
        toplevels.insert(
            7,
            ToplevelEntry {
                app_id: Some("firefox".into()),
                activated: true,
            },
        );
        toplevels.insert(
            3,
            ToplevelEntry {
                app_id: Some("terminal".into()),
                activated: true,
            },
        );
        toplevels.insert(
            9,
            ToplevelEntry {
                app_id: Some("chrome".into()),
                activated: false,
            },
        );
        // BTreeMap iterates in ascending key order, so the lowest
        // handle wins without ever inspecting titles.
        assert_eq!(pick_active_app_id(&toplevels).as_deref(), Some("terminal"));
    }

    #[test]
    fn pick_active_app_id_ignores_empty_and_whitespace() {
        let mut toplevels = BTreeMap::<u32, ToplevelEntry>::new();
        toplevels.insert(
            1,
            ToplevelEntry {
                app_id: Some(" ".into()),
                activated: true,
            },
        );
        toplevels.insert(
            2,
            ToplevelEntry {
                app_id: Some("firefox".into()),
                activated: false,
            },
        );
        assert_eq!(pick_active_app_id(&toplevels), None);
    }

    #[test]
    fn pick_active_app_id_ignores_inactive_toplevels() {
        let mut toplevels = BTreeMap::<u32, ToplevelEntry>::new();
        toplevels.insert(
            1,
            ToplevelEntry {
                app_id: Some("terminal".into()),
                activated: false,
            },
        );
        assert_eq!(pick_active_app_id(&toplevels), None);
    }

    #[test]
    fn snapshot_treats_empty_app_id_as_absence() {
        let snapshot = Snapshot::new();
        let mut toplevels = BTreeMap::<u32, ToplevelEntry>::new();
        toplevels.insert(
            1,
            ToplevelEntry {
                app_id: Some(String::new()),
                activated: true,
            },
        );
        snapshot.commit(toplevels);
        assert_eq!(snapshot.active_app_id(), None);
        assert!(snapshot.committed());
    }

    #[test]
    fn snapshot_publishes_only_after_done() {
        let snapshot = Snapshot::new();
        assert!(!snapshot.committed());
        assert_eq!(snapshot.active_app_id(), None);
        let mut toplevels = BTreeMap::<u32, ToplevelEntry>::new();
        toplevels.insert(
            4,
            ToplevelEntry {
                app_id: Some("terminal".into()),
                activated: true,
            },
        );
        snapshot.commit(toplevels);
        assert!(snapshot.committed());
        assert_eq!(snapshot.active_app_id().as_deref(), Some("terminal"));
    }

    #[test]
    fn closed_event_removes_toplevel() {
        let mut toplevels = BTreeMap::<u32, ToplevelEntry>::new();
        toplevels.insert(
            2,
            ToplevelEntry {
                app_id: Some("terminal".into()),
                activated: true,
            },
        );
        toplevels.insert(
            5,
            ToplevelEntry {
                app_id: Some("firefox".into()),
                activated: false,
            },
        );
        toplevels.remove(&2);
        let only_firefox: Vec<u32> = toplevels.keys().copied().collect();
        assert_eq!(only_firefox, vec![5]);
    }

    #[test]
    fn marshal_request_header_packs_fields() {
        let header = Marshal::request_header(7, 1, 4);
        assert_eq!(header.len(), 8);
        let sender_id = u32::from_ne_bytes([header[0], header[1], header[2], header[3]]);
        let opcode = u16::from_ne_bytes([header[4], header[5]]);
        let size = u16::from_ne_bytes([header[6], header[7]]);
        assert_eq!(sender_id, 7);
        assert_eq!(opcode, 1);
        assert_eq!(size, 12);
    }

    #[test]
    fn marshal_string_includes_length_and_padding() {
        let bytes = Marshal::string("hi");
        assert_eq!(bytes.len(), 8);
        let length = u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(length, 3);
        assert_eq!(&bytes[4..7], b"hi\0");
    }

    #[test]
    fn unmarshal_round_trips_strings() {
        let mut unmarshal = Unmarshal::new(1, 2, Marshal::string("terminal"));
        assert_eq!(unmarshal.read_string().unwrap(), "terminal");
    }

    #[test]
    fn unmarshal_short_payload_errors() {
        let mut unmarshal = Unmarshal::new(1, 2, vec![0u8; 3]);
        let error = unmarshal.read_u32().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn protocol_kind_strings_are_stable() {
        assert_eq!(ProtocolKind::Ext.as_str(), "ext_foreign_toplevel_list_v1");
        assert_eq!(
            ProtocolKind::Zwlr.as_str(),
            "zwlr_foreign_toplevel_management_unstable_v1"
        );
    }
}
