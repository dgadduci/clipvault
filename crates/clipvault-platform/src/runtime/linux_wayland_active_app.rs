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
//! with third-party tools expose a small set of public protocols.
//! The two this probe speaks are:
//!
//! 1. [`ext-foreign-toplevel-list-v1`](https://wayland.app/protocols/ext-foreign-toplevel-list-v1)
//!    — modern KDE / wlroots preference. Lists toplevels and their
//!    `app_id` / `identifier` metadata, but **does NOT report window
//!    activation / focus**. The probe treats ext as metadata-only
//!    and refuses to invent "active" ordering from it.
//! 2. [`zwlr_foreign_toplevel_management_unstable_v1`](https://wayland.app/protocols/wlr-foreign-toplevel-management-unstable-v1)
//!    — wlroots-only protocol. Reports an explicit
//!    `state[activated]` bit per toplevel, so the probe uses this as
//!    the **focus source of truth**.
//!
//! On a session where both protocols are announced the probe binds
//! both. The wlroots binding is the authoritative one for
//! activation; the ext binding supplies a back-up `app_id` should
//! the wlroots handle publish a `title` but not an `app_id`. When
//! only ext is announced the probe returns
//! [`ActiveAppError::Unavailable`]: ext alone cannot answer
//! "which toplevel is focused" and the contract forbids inferring
//! focus from handle ordering, creation order, titles, PID or any
//! other compositor-supplied signal.
//!
//! The `app_id` has the same trust model as `WM_CLASS`: it is
//! metadata the application itself published through the toolkit,
//! it is not a proof of process identity, and the rest of the
//! pipeline treats it as a stable identifier only. The blacklist and
//! the metadata provider (`LinuxApplicationMetadataProvider`)
//! consume it the same way they consume `WM_CLASS`.
//!
//! ## Wire protocol
//!
//! The marshal / unmarshal helpers in this module hand-encode the
//! minimum surface the probe needs and are pinned by byte-level
//! tests in `crates/clipvault-platform/tests/linux_wayland_active_app.rs`.
//! The object-id accounting matches the canonical Wayland rules:
//!
//! - `wl_display` is id `1` for the lifetime of the connection;
//! - `wl_registry` is allocated by the client through
//!   `wl_display.get_registry` (opcode 1, single `new_id` arg);
//! - every other object (`wl_callback`, the bound protocol,
//!   per-handle instances) is allocated by the client through a
//!   `new_id` argument.
//!
//! The probe does NOT read `WAYLAND_SOCKET`: that environment
//! variable holds an integer file descriptor handed over by the
//! compositor via `SCM_RIGHTS` / `execve` inheritance, not a socket
//! path. The pre-fix implementation rewrote it as
//! `/proc/self/fd/<fd>` and called `UnixStream::connect`, which
//! works only by accident and risks closing a file descriptor the
//! compositor expects us to leave alone. The `systemd` activation
//! path is dropped from this probe.
//!
//! ## Why hand-written marshal and not `wayland-client`
//!
//! Using `wayland-client` would add a `build.rs`-time dependency on
//! `wayland-scanner` and bind a tree of protocols the probe does
//! not need. The two protocols this probe speaks are small enough
//! that the hand-written marshal / unmarshal stays under a few
//! hundred lines, every opcode and payload is covered by an
//! integration test that compares the emitted / received bytes
//! against the spec, and the dependency graph stays minimal. The
//! decision is documented in `design.md`.
//!
//! ## What this module does NOT do
//!
//! - **Title-based identity.** The probe never reads, logs or uses a
//!   window title. The metadata enrichment path stays free of title
//!   data so the privacy contract the rest of ClipVault honours is
//!   not weakened.
//! - **GNOME private extensions.** No `org.gnome.Shell.Eval`, no
//!   `gdbus` introspection, no GNOME Shell evaluation. A GNOME
//!   session without one of the supported public protocols returns
//!   `Unavailable` — a different change would be required to wire a
//!   private protocol.
//! - **Inferring focus from ext.** `ext-foreign-toplevel-list-v1`
//!   enumerates toplevels and their metadata but does not expose an
//!   activation / focused state. The probe never picks the lowest
//!   handle, the latest handle, the handle whose `app_id` parsed
//!   or any other heuristic as a stand-in for focus.
//! - **Forced Wayland session.** The probe is opt-in: the bootstrap
//!   only installs it when `$WAYLAND_DISPLAY` is set, the session
//!   is classified as Wayland and the registry actually publishes
//!   one of the two protocols. Otherwise the probe is not even
//!   constructed.

#![cfg(all(target_os = "linux", feature = "linux-wayland-active-app"))]
#![allow(dead_code)]

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

// =====================================================================
// Backend names
// =====================================================================

/// Backend identifier when the wlroots protocol is the focus
/// source of truth. The diagnostics card consumes the value
/// verbatim, so renaming it is a breaking change for the UI.
pub const BACKEND_NAME_WLR: &str = "wayland_wlr_foreign_toplevel";

/// Backend identifier when both protocols are bound but the
/// wlroots one provides focus. Kept as a separate identifier so
/// the diagnostics card can tell apart "wlroots resolved focus"
/// from "wlroots alone resolved focus".
pub const BACKEND_NAME_WLR_COMBINED: &str = "wayland_wlr_foreign_toplevel_with_ext";

/// Backend identifier when only ext is bound and the probe
/// therefore cannot determine focus. Kept as a separate identifier
/// so the diagnostics card can distinguish "metadata only, focus
/// unknown" from "fully operational".
pub const BACKEND_NAME_EXT_FALLBACK: &str = "wayland_foreign_toplevel_ext_no_focus";

/// Backwards-compatible alias used by callers that consume the
/// pre-fix names. The original change shipped
/// `BACKEND_NAME="wayland_foreign_toplevel"` for the ext-preferred
/// path; the corrected protocol now associates that string with
/// the authoritative wlroots outcome (which is the only path that
/// actually resolves a focus state). The alias keeps the old name
/// pointing at the same string so existing consumers and tests do
/// not need to learn a new identifier to keep their assertions
/// valid.
pub const BACKEND_NAME: &str = BACKEND_NAME_WLR;

// =====================================================================
// Protocol kinds
// =====================================================================

/// Well-known protocol interfaces the registry may advertise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolKind {
    /// `ext-foreign-toplevel-list-v1`. Modern, preferred for
    /// metadata; cannot answer focus on its own.
    Ext,
    /// `zwlr_foreign_toplevel_management_unstable_v1`. Wlroots-only;
    /// the only one of the two that exposes `state[activated]`.
    Zwlr,
}

impl ProtocolKind {
    /// Human-readable interface name used in bind requests.
    pub fn interface_name(self) -> &'static str {
        match self {
            ProtocolKind::Ext => "ext_foreign_toplevel_list_v1",
            ProtocolKind::Zwlr => "zwlr_foreign_toplevel_manager_v1",
        }
    }

    /// Highest version of the interface this probe speaks.
    pub fn max_version(self) -> u32 {
        match self {
            ProtocolKind::Ext => 1,
            ProtocolKind::Zwlr => 3,
        }
    }

    /// Minimum version we are willing to speak. A compositor that
    /// announces the interface at a version below this number is
    /// considered incompatible and the probe refuses the bind
    /// rather than talking a divergent dialect.
    pub fn min_version(self) -> u32 {
        match self {
            ProtocolKind::Ext => 1,
            ProtocolKind::Zwlr => 1,
        }
    }
}

// =====================================================================
// Toplevel state
// =====================================================================

/// Per-toplevel state the adapter maintains between events. Both
/// protocols populate the same struct (the union is the union of
/// the two protocols' metadata shapes), so the active-app
/// extraction reuses the same logic regardless of which protocol
/// was the source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToplevelEntry {
    /// `app_id` event seen for the toplevel. Used as the
    /// blacklist-compatible identifier.
    pub app_id: Option<String>,
    /// `state[activated]` set for the toplevel on the most recent
    /// round. Set only by the zwlr protocol; ext can never set it.
    pub activated: bool,
}

// =====================================================================
// Snapshot
// =====================================================================

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
    /// What caused the most recent binding / handshake transition.
    /// Mirrors the granular taxonomy the design documents.
    cause: ConnectionCause,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ConnectionCause {
    /// Default — no connection has been attempted yet, or the
    /// previous attempt never produced a committed snapshot.
    #[default]
    Unset,
    /// The Wayland socket could not be opened.
    SocketUnavailable,
    /// The handshake (`get_registry`) failed.
    HandshakeFailed,
    /// The registry announced neither of the two protocols.
    RegistryWithoutProtocol,
    /// The announced protocol version is below the minimum we
    /// speak.
    IncompatibleVersion,
    /// The compositor refused the bind request.
    BindRejected,
    /// A supported protocol is now bound and the I/O thread is
    /// consuming its events.
    Connected,
    /// The compositor closed the socket mid-flight.
    CompositorDisconnected,
    /// The bound protocol never produced a `done` round.
    SnapshotUncommitted,
    /// A `done` round was committed but no toplevel reported
    /// `state[activated]`.
    NoActiveToplevel,
    /// A `done` round resolved an `app_id` for the active toplevel.
    AppIdIdentified,
}

impl ConnectionCause {
    fn as_str(self) -> &'static str {
        match self {
            ConnectionCause::Unset => "unset",
            ConnectionCause::SocketUnavailable => "socket_unavailable",
            ConnectionCause::HandshakeFailed => "handshake_failed",
            ConnectionCause::RegistryWithoutProtocol => "registry_without_protocol",
            ConnectionCause::IncompatibleVersion => "incompatible_version",
            ConnectionCause::BindRejected => "bind_rejected",
            ConnectionCause::Connected => "connected",
            ConnectionCause::CompositorDisconnected => "compositor_disconnected",
            ConnectionCause::SnapshotUncommitted => "snapshot_uncommitted",
            ConnectionCause::NoActiveToplevel => "no_active_toplevel",
            ConnectionCause::AppIdIdentified => "app_id_identified",
        }
    }
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

    fn set_cause(&self, cause: ConnectionCause) {
        self.inner.write().cause = cause;
    }

    /// Granular `ConnectionCause` the most recent transition left
    /// behind. Re-exported for the integration tests so they can
    /// pin the failure-mode taxonomy the diagnostics card
    /// consumes.
    pub fn cause(&self) -> &'static str {
        self.inner.read().cause.as_str()
    }

    /// Commit a new round of toplevel state. The I/O thread calls
    /// this once per `done` event from the bound protocol; partial
    /// state between `done` events stays internal.
    pub fn commit(&self, toplevels: BTreeMap<u32, ToplevelEntry>) {
        let active = pick_active_app_id(&toplevels);
        let cause = if active.is_some() {
            ConnectionCause::AppIdIdentified
        } else {
            ConnectionCause::NoActiveToplevel
        };
        {
            let mut guard = self.inner.write();
            guard.toplevels = toplevels;
            guard.active_app_id = active;
            guard.committed = true;
            guard.cause = cause;
        }
        let stage = match cause {
            ConnectionCause::AppIdIdentified => ProbeStage::Identified,
            _ => ProbeStage::ActiveWindowEmpty,
        };
        self.set_stage(stage);
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
/// across runs: smallest compositor handle whose `activated` flag
/// is set wins. The rule is only meaningful for the zwlr protocol
/// (the only one of the two that exposes `state[activated]`); the
/// ext path never carries the flag so the function returns
/// `None`. The matcher ignores the toplevel's window title and
/// `app_id`-matching rules — `activated` is the only focus
/// indicator either protocol publishes.
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

// =====================================================================
// Wire-protocol helpers
// =====================================================================

/// Marshal-side helpers. Pure byte builders — no I/O.
pub struct Marshal;

impl Marshal {
    /// Build a Wayland request header. The header carries the
    /// fixed Wayland layout: `sender_id` (u32), `opcode` (u16),
    /// `size` (u16).
    pub fn request_header(sender_id: u32, opcode: u16, args_byte_count: usize) -> [u8; 8] {
        let size = (8 + args_byte_count) as u16;
        let mut header = [0u8; 8];
        header[0..4].copy_from_slice(&sender_id.to_ne_bytes());
        header[4..6].copy_from_slice(&opcode.to_ne_bytes());
        header[6..8].copy_from_slice(&size.to_ne_bytes());
        header
    }

    pub fn u32(value: u32) -> [u8; 4] {
        value.to_ne_bytes()
    }

    /// Encode a Wayland string: `[length: u32][bytes + NUL][pad]`.
    /// The `length` field counts the NUL terminator but not the
    /// trailing padding that aligns the next argument to a
    /// 4-byte boundary.
    pub fn string(value: &str) -> Vec<u8> {
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

/// Unmarshal-side helpers. Pure byte readers — no I/O.
#[derive(Debug)]
pub struct Unmarshal {
    payload: Vec<u8>,
    cursor: usize,
}

impl Unmarshal {
    pub fn new(payload: Vec<u8>) -> Self {
        Self { payload, cursor: 0 }
    }

    pub fn parse_event_header(header: &[u8; 8]) -> io::Result<(u32, u16, u16)> {
        let sender_id = u32::from_ne_bytes([header[0], header[1], header[2], header[3]]);
        let opcode = u16::from_ne_bytes([header[4], header[5]]);
        let size = u16::from_ne_bytes([header[6], header[7]]);
        Ok((sender_id, opcode, size))
    }

    pub fn read_u32(&mut self) -> io::Result<u32> {
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

    pub fn read_string(&mut self) -> io::Result<String> {
        let length = self.read_u32()? as usize;
        if length == 0 {
            return Ok(String::new());
        }
        let padded = (length + 3) & !3;
        if self.cursor + padded > self.payload.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Wayland: short string read",
            ));
        }
        let end = self.cursor + length;
        let bytes = &self.payload[self.cursor..end];
        self.cursor += padded;
        let trimmed = bytes.strip_suffix(&[0]).unwrap_or(bytes);
        Ok(String::from_utf8_lossy(trimmed).into_owned())
    }

    /// Decode a Wayland array whose first field is the **byte**
    /// length, not the element count. The Wayland core protocol
    /// defines <arg type="array"/> as a count followed by `count`
    /// bytes of arbitrary payload; both the count and the payload
    /// are padded to a 4-byte boundary so the next argument falls
    /// on a natural boundary.
    ///
    /// `zwlr_foreign_toplevel_handle_v1.state` is the canonical
    /// example this probe reads. The handler refines the bytes
    /// into `u32` elements and validates that the size matches
    /// the expected element width.
    pub fn read_byte_length_array_u32(&mut self) -> io::Result<Vec<u32>> {
        let byte_length = self.read_u32()? as usize;
        if byte_length % 4 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Wayland: wl_array byte length is not u32-aligned",
            ));
        }
        let padded = (byte_length + 3) & !3;
        if self.cursor + padded > self.payload.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Wayland: short wl_array read",
            ));
        }
        let mut out = Vec::with_capacity(byte_length / 4);
        for chunk in self.payload[self.cursor..self.cursor + byte_length].chunks_exact(4) {
            let bytes: [u8; 4] = [chunk[0], chunk[1], chunk[2], chunk[3]];
            out.push(u32::from_ne_bytes(bytes));
        }
        self.cursor += padded;
        Ok(out)
    }

    pub fn remaining(&self) -> usize {
        self.payload.len().saturating_sub(self.cursor)
    }
}

/// In-process event representation the dispatcher and the
/// integration tests share.
#[derive(Debug)]
pub struct WaylandEvent {
    sender_id: u32,
    opcode: u16,
    unmarshal: Unmarshal,
}

impl WaylandEvent {
    pub fn new(sender_id: u32, opcode: u16, payload: Vec<u8>) -> Self {
        Self {
            sender_id,
            opcode,
            unmarshal: Unmarshal::new(payload),
        }
    }

    pub fn read_u32(&mut self) -> io::Result<u32> {
        self.unmarshal.read_u32()
    }

    pub fn read_string(&mut self) -> io::Result<String> {
        self.unmarshal.read_string()
    }

    pub fn read_byte_length_array(&mut self) -> io::Result<Vec<u32>> {
        self.unmarshal.read_byte_length_array_u32()
    }

    /// Construct an event straight from already-decoded bytes.
    /// The integration suite uses this helper to build events
    /// without going through the in-memory transport; the dispatch
    /// loop reads events the same way and the helper stays
    /// available for the `Drive` test that walks the dispatcher
    /// byte by byte.
    pub fn for_test(sender_id: u32, opcode: u16, payload: Vec<u8>) -> Self {
        Self::new(sender_id, opcode, payload)
    }
}

// =====================================================================
// Protocol state machine
// =====================================================================

/// Per-protocol state the I/O thread keeps while it dispatches
/// events. The struct carries both the bindings (which objects are
/// our `wl_display` children) and the partial toplevel map that
/// builds up between `done` rounds.
#[derive(Debug, Default)]
pub struct ProtocolState {
    /// Object id the compositor reserved for the bound ext
    /// interface, if any.
    pub ext_list_id: Option<u32>,
    /// Maps every per-toplevel ext handle id (the value the
    /// compositor allocated) to the local toplevel id the probe
    /// synthesised. The local id is monotonically increasing so
    /// the matcher's deterministic selection stays stable across
    /// rounds.
    pub ext_handles: BTreeMap<u32, u32>,
    /// Object id the compositor reserved for the bound zwlr
    /// interface, if any.
    pub zwlr_manager_id: Option<u32>,
    /// Per-handle partial state for zwlr.
    pub zwlr_handles: BTreeMap<u32, u32>,
    /// Per-toplevel partial state keyed by the local toplevel id.
    pub pending_toplevels: BTreeMap<u32, ToplevelEntry>,
    /// Stable counter used to assign monotonically-increasing
    /// local toplevel ids.
    pub next_toplevel_id: u32,
}

impl ProtocolState {
    fn new() -> Self {
        Self {
            next_toplevel_id: 1,
            ..Self::default()
        }
    }

    fn allocate_toplevel_id(&mut self) -> u32 {
        let id = self.next_toplevel_id;
        self.next_toplevel_id = self.next_toplevel_id.wrapping_add(1);
        id
    }
}

#[derive(Debug)]
enum DriveOutcome {
    Operational {
        backend: &'static str,
    },
    Unavailable {
        cause: UnavailableCause,
        connection_cause: ConnectionCause,
    },
    BackendError,
}

/// Drive the handshake and the registry round, then return the
/// verdict the handshake waiter was waiting for.
fn drive_protocol<T: Transport>(
    connection: &mut WaylandConnection<T>,
    snapshot: &Snapshot,
    state: &mut ProtocolState,
    alive: &Arc<AtomicBool>,
) -> DriveOutcome {
    snapshot.set_cause(ConnectionCause::HandshakeFailed);
    // -----------------------------------------------------------------
    // Handshake: wl_display is id 1, get_registry opcode 1 returns a
    // new wl_registry. We allocate the registry id (id == 2) here.
    // -----------------------------------------------------------------
    let registry_id = connection.alloc_id();
    if connection
        .send_request(1, 1, &Marshal::u32(registry_id))
        .is_err()
    {
        return DriveOutcome::Unavailable {
            cause: UnavailableCause::HandshakeFailed,
            connection_cause: ConnectionCause::HandshakeFailed,
        };
    }

    let mut registry_seen = false;
    let mut ext_global_seen = false;
    let mut ext_global_low_version = false;
    let mut zwlr_global_seen = false;
    let mut zwlr_global_low_version = false;

    // Drain the registry round: read at least one `global` event
    // and the `global_remove` events that follow before we accept
    // that the registry has finished. The registry stream is
    // capped to a small fixed number of events so the I/O thread
    // cannot spin forever on a misbehaving compositor.
    const REGISTRY_EVENT_LIMIT: usize = 64;
    for _ in 0..REGISTRY_EVENT_LIMIT {
        if !alive.load(Ordering::Acquire) {
            return DriveOutcome::BackendError;
        }
        let mut event = match connection.read_event() {
            Ok(event) => event,
            Err(_) => return DriveOutcome::BackendError,
        };
        if event.sender_id != registry_id {
            continue;
        }
        registry_seen = true;
        match event.opcode {
            0 => {
                let name = event.read_u32();
                let interface = event.read_string();
                let version = event.read_u32();
                let (name, interface, version) = match (name, interface, version) {
                    (Ok(n), Ok(i), Ok(v)) => (n, i, v),
                    _ => {
                        return DriveOutcome::Unavailable {
                            cause: UnavailableCause::HandshakeFailed,
                            connection_cause: ConnectionCause::HandshakeFailed,
                        };
                    }
                };
                if let Err(reason) =
                    try_bind_protocol(connection, registry_id, state, &interface, name, version)
                {
                    return reason;
                }
                if interface == ProtocolKind::Ext.interface_name() {
                    ext_global_seen = true;
                    if version < ProtocolKind::Ext.min_version() {
                        ext_global_low_version = true;
                    }
                }
                if interface == ProtocolKind::Zwlr.interface_name() {
                    zwlr_global_seen = true;
                    if version < ProtocolKind::Zwlr.min_version() {
                        zwlr_global_low_version = true;
                    }
                }
            }
            1 => {
                // `global_remove`. The probe keeps no global-name
                // shadow map (the bindings table is the source of
                // truth), so removal triggers no further action.
                let _ = event.read_u32();
            }
            _ => {
                // Unknown registry event (future-revision opcode).
                let _ = event.read_u32();
            }
        }
        if state.zwlr_manager_id.is_some() {
            // The wlroots protocol is the focus source of truth;
            // bind succeeded → declare startup complete.
            snapshot.set_cause(ConnectionCause::Connected);
            return DriveOutcome::Operational {
                backend: pick_backend(state),
            };
        }
    }

    if !registry_seen {
        return DriveOutcome::Unavailable {
            cause: UnavailableCause::HandshakeFailed,
            connection_cause: ConnectionCause::HandshakeFailed,
        };
    }
    if !ext_global_seen && !zwlr_global_seen {
        return DriveOutcome::Unavailable {
            cause: UnavailableCause::RegistryWithoutProtocol,
            connection_cause: ConnectionCause::RegistryWithoutProtocol,
        };
    }
    if ext_global_seen && !zwlr_global_seen {
        if ext_global_low_version {
            return DriveOutcome::Unavailable {
                cause: UnavailableCause::RegistryWithoutProtocol,
                connection_cause: ConnectionCause::IncompatibleVersion,
            };
        }
        return DriveOutcome::Unavailable {
            cause: UnavailableCause::RegistryWithoutProtocol,
            connection_cause: ConnectionCause::RegistryWithoutProtocol,
        };
    }
    if zwlr_global_seen && zwlr_global_low_version {
        return DriveOutcome::Unavailable {
            cause: UnavailableCause::RegistryWithoutProtocol,
            connection_cause: ConnectionCause::IncompatibleVersion,
        };
    }
    DriveOutcome::Unavailable {
        cause: UnavailableCause::RegistryWithoutProtocol,
        connection_cause: ConnectionCause::RegistryWithoutProtocol,
    }
}

fn pick_backend(state: &ProtocolState) -> &'static str {
    if state.ext_list_id.is_some() && state.zwlr_manager_id.is_some() {
        BACKEND_NAME_WLR_COMBINED
    } else {
        BACKEND_NAME_WLR
    }
}

fn try_bind_protocol<T: Transport>(
    connection: &mut WaylandConnection<T>,
    registry_id: u32,
    state: &mut ProtocolState,
    interface: &str,
    global_name: u32,
    announced_version: u32,
) -> Result<(), DriveOutcome> {
    let kind = match interface {
        "ext_foreign_toplevel_list_v1" => ProtocolKind::Ext,
        "zwlr_foreign_toplevel_manager_v1" => ProtocolKind::Zwlr,
        _ => return Ok(()),
    };
    if announced_version < kind.min_version() {
        return Err(DriveOutcome::Unavailable {
            cause: UnavailableCause::RegistryWithoutProtocol,
            connection_cause: ConnectionCause::IncompatibleVersion,
        });
    }
    let requested = announced_version.min(kind.max_version());
    let new_id = connection.alloc_id();
    let mut args = Vec::new();
    args.extend_from_slice(&Marshal::u32(global_name));
    args.extend_from_slice(&Marshal::u32(new_id));
    args.extend(Marshal::string(kind.interface_name()));
    args.extend_from_slice(&Marshal::u32(requested));
    if connection.send_request(registry_id, 0, &args).is_err() {
        return Err(DriveOutcome::BackendError);
    }
    match kind {
        ProtocolKind::Ext => state.ext_list_id = Some(new_id),
        ProtocolKind::Zwlr => state.zwlr_manager_id = Some(new_id),
    }
    Ok(())
}

/// Long-running dispatch loop. Called after the handshake has
/// succeeded; reads events from the socket and routes them to the
/// per-protocol handlers. Exits when the connection closes or the
/// alive flag flips.
fn dispatch_loop<T: Transport>(
    connection: &mut WaylandConnection<T>,
    snapshot: &Snapshot,
    mut state: ProtocolState,
    alive: &Arc<AtomicBool>,
) {
    while alive.load(Ordering::Acquire) {
        let mut event = match connection.read_event() {
            Ok(event) => event,
            Err(_) => {
                snapshot.set_cause(ConnectionCause::CompositorDisconnected);
                return;
            }
        };
        route_event(connection, snapshot, &mut state, &mut event, alive.as_ref());
    }
    snapshot.set_cause(ConnectionCause::CompositorDisconnected);
}

/// Route an incoming event to the matching protocol handler. The
/// dispatcher threads the snapshot through every handler so the
/// per-handle `done` event can commit a fresh toplevel map without
/// a thread-local handshake.
fn route_event<T: Transport>(
    connection: &mut WaylandConnection<T>,
    snapshot: &Snapshot,
    state: &mut ProtocolState,
    event: &mut WaylandEvent,
    _alive: &AtomicBool,
) {
    if let Some(manager_id) = state.zwlr_manager_id {
        if event.sender_id == manager_id {
            dispatch_zwlr_list_event(state, event);
            return;
        }
    }
    if let Some(list_id) = state.ext_list_id {
        if event.sender_id == list_id {
            dispatch_ext_list_event(state, event);
            return;
        }
    }
    if let Some(&local_id) = state.zwlr_handles.get(&event.sender_id) {
        dispatch_zwlr_handle_event(snapshot, state, event, local_id);
        return;
    }
    if let Some(&local_id) = state.ext_handles.get(&event.sender_id) {
        dispatch_ext_handle_event(snapshot, state, event, local_id);
        return;
    }
    // The `connection` parameter is reserved for protocol-level
    // requests the dispatcher may need to send (none today). The
    // unused-binding silences the compiler without affecting
    // runtime behaviour.
    let _ = connection;
}

/// ext-foreign-toplevel-list-v1 manager events.
fn dispatch_ext_list_event(state: &mut ProtocolState, event: &mut WaylandEvent) {
    match event.opcode {
        0 => {
            // `toplevel` (new_id). The wire format hands us a u32
            // the compositor allocated; we adopt that handle id as
            // the foreign key into the partial state.
            let handle_id = match event.read_u32() {
                Ok(id) => id,
                Err(_) => return,
            };
            let local_id = state.allocate_toplevel_id();
            state.ext_handles.insert(handle_id, local_id);
            state.pending_toplevels.entry(local_id).or_default();
        }
        1 => {
            // `finished` — compositor is shutting the list down.
            state.pending_toplevels.clear();
            state.ext_handles.clear();
        }
        4 => {
            // `update_info` — batched event follows; wait for the
            // per-handle events and the round's `done`.
        }
        _ => {
            // Unknown opcode — drop.
        }
    }
}

/// ext-foreign-toplevel-handle-v1 events.
fn dispatch_ext_handle_event(
    snapshot: &Snapshot,
    state: &mut ProtocolState,
    event: &mut WaylandEvent,
    local_id: u32,
) {
    match event.opcode {
        0 => {
            // `closed` — drop the toplevel from the pending map.
            state.pending_toplevels.remove(&local_id);
            state.ext_handles.retain(|_, v| *v != local_id);
        }
        1 => {
            // `done` — commit the pending map onto the snapshot.
            // ext's `done` is per-handle; each handle publishes its
            // own done, and we commit on every one so the matcher
            // sees the freshest state.
            snapshot.commit(state.pending_toplevels.clone());
        }
        2 => {
            // `title` — currently discarded; titles are never used
            // as identity and the privacy contract forbids logging
            // them.
            let _ = event.read_string();
        }
        3 => {
            // `app_id` — promote into the pending entry.
            if let Ok(app_id) = event.read_string() {
                let entry = state.pending_toplevels.entry(local_id).or_default();
                entry.app_id = Some(app_id);
            }
        }
        4 => {
            // `identifier` — currently discarded; the metadata
            // pipeline keys off `app_id`.
            let _ = event.read_string();
        }
        _ => {
            // Unknown opcode — drop.
        }
    }
}

/// zwlr_foreign_toplevel-manager events.
fn dispatch_zwlr_list_event(state: &mut ProtocolState, event: &mut WaylandEvent) {
    match event.opcode {
        0 => {
            // `toplevel` (new_id).
            let handle_id = match event.read_u32() {
                Ok(id) => id,
                Err(_) => return,
            };
            let local_id = state.allocate_toplevel_id();
            state.zwlr_handles.insert(handle_id, local_id);
            state.pending_toplevels.entry(local_id).or_default();
        }
        1 => {
            // `finished` — compositor is shutting the list down.
            state.pending_toplevels.clear();
            state.zwlr_handles.clear();
        }
        _ => {
            // Unknown opcode — drop.
        }
    }
}

/// zwlr_foreign_toplevel_handle events.
fn dispatch_zwlr_handle_event(
    snapshot: &Snapshot,
    state: &mut ProtocolState,
    event: &mut WaylandEvent,
    local_id: u32,
) {
    match event.opcode {
        0 => {
            // `title` — currently discarded.
            let _ = event.read_string();
        }
        1 => {
            // `app_id` — promote into the pending entry.
            if let Ok(app_id) = event.read_string() {
                let entry = state.pending_toplevels.entry(local_id).or_default();
                entry.app_id = Some(app_id);
            }
        }
        2 => {
            // `output_enter` — currently discarded.
            let _ = event.read_u32();
        }
        3 => {
            // `output_leave` — currently discarded.
            let _ = event.read_u32();
        }
        4 => {
            // `state` — `wl_array` of u32 states. The byte-length
            // prefix is what the protocol specifies; reject any
            // encoding where the byte length is not u32-aligned.
            match event.read_byte_length_array() {
                Ok(states) => {
                    let entry = state.pending_toplevels.entry(local_id).or_default();
                    entry.activated = states.contains(&2);
                }
                Err(error) => {
                    warn!(error = %error, "wayland zwlr state array failed to parse");
                }
            }
        }
        5 => {
            // `done` — round boundary. We commit immediately so
            // the matcher picks up the latest `state[activated]`
            // before any other handle's events arrive.
            snapshot.commit(state.pending_toplevels.clone());
        }
        6 => {
            // `closed` — drop the toplevel from the pending map.
            state.pending_toplevels.remove(&local_id);
            state.zwlr_handles.retain(|_, v| *v != local_id);
        }
        _ => {
            // Unknown opcode — drop.
        }
    }
}

// =====================================================================
// Adapter
// =====================================================================

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

    /// Test-only constructor with an explicit backend name. Used by
    /// tests that need to assert the diagnostics card receives the
    /// `wayland_wlr_foreign_toplevel` identifier when the wlroots
    /// path is the one that landed the activation state.
    pub fn for_tests_with_backend(
        snapshot: Arc<Snapshot>,
        alive: Arc<AtomicBool>,
        backend: &'static str,
    ) -> Self {
        Self {
            snapshot,
            backend,
            alive,
            _io: None,
        }
    }

    /// Attach an I/O thread to the probe. The probe takes
    /// ownership of the thread; dropping the probe stops the
    /// alive flag, which makes the thread exit on its next
    /// round-trip. The helper exists so [`try_build`] can return
    /// a probe that owns its I/O thread without exposing the
    /// `IoThread` type to the public API.
    pub fn attach_io(mut self, io: IoThread<T>) -> Self {
        self._io = Some(io);
        self
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

impl<T: Transport> ActiveApplicationProbe for WaylandActiveApplication<T> {
    fn active_application(&self) -> Result<Option<ActiveApplication>, ActiveAppError> {
        if !self.alive.load(Ordering::Acquire) {
            self.snapshot.set_stage(ProbeStage::Backend);
            return Err(ActiveAppError::Unavailable);
        }
        match self.snapshot.active_app_id() {
            Some(app_id) => Ok(Some(ActiveApplication::new(app_id.clone(), app_id))),
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

    fn name(&self) -> &'static str {
        self.backend
    }

    fn last_probe_stage(&self) -> ProbeStage {
        self.snapshot.stage()
    }
}

/// Reason a `try_build` call returned [`ConnectionOutcome::Unavailable`].
/// The bootstrap can map the cause onto a diagnostics card
/// category without parsing free-form log lines. The values are
/// stable identifiers — renaming any variant is a breaking change
/// for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableCause {
    /// The `$WAYLAND_DISPLAY` / `$XDG_RUNTIME_DIR` lookup failed or
    /// `connect(2)` returned an error before the handshake.
    SocketUnavailable,
    /// The connection opened but the registry never replied (or
    /// the reply was malformed enough to refuse the handshake).
    HandshakeFailed,
    /// The registry announced neither of the supported protocols
    /// at a compatible version.
    RegistryWithoutProtocol,
    /// The compositor refused a bind request.
    BindRejected,
    /// The I/O thread reported a transport / backend failure that
    /// is recoverable on the next `try_build` but fatal for the
    /// current session.
    Backend,
}

impl UnavailableCause {
    pub fn as_str(self) -> &'static str {
        match self {
            UnavailableCause::SocketUnavailable => "socket_unavailable",
            UnavailableCause::HandshakeFailed => "handshake_failed",
            UnavailableCause::RegistryWithoutProtocol => "registry_without_protocol",
            UnavailableCause::BindRejected => "bind_rejected",
            UnavailableCause::Backend => "backend",
        }
    }
}

/// Outcome of [`try_build`]. The constructor distinguishes "no
/// Wayland session" / "protocol absent" from "connection died" so
/// the bootstrap can keep the existing X11 fallback honest.
#[derive(Debug)]
#[allow(dead_code)]
pub enum ConnectionOutcome {
    /// The probe connected to the Wayland socket and bound a
    /// protocol that can resolve focus.
    Operational {
        probe: WaylandActiveApplication<UnixStreamTransport>,
        backend: &'static str,
    },
    /// The Wayland socket could not be opened, the registry did not
    /// publish a usable protocol or the I/O thread reported a
    /// transport error during startup.
    Unavailable { cause: UnavailableCause },
}

/// Public entry point the bootstrap uses to build the probe. The
/// helper looks up `$WAYLAND_DISPLAY` / `$XDG_RUNTIME_DIR`, opens
/// the socket, runs the handshake and binds the right protocol. The
/// I/O thread is spawned before the constructor returns so the
/// capture loop sees a fully-running probe on the very first call.
///
/// `try_build` only returns `Operational` once the handshake
/// succeeded **and** the I/O thread confirmed a registry round-trip
/// that bound a supported protocol that can resolve focus. A
/// failure at any of those stages yields
/// [`ConnectionOutcome::Unavailable`] with the granular cause the
/// diagnostics card consumes.
pub fn try_build() -> ConnectionOutcome {
    let socket_path = match wayland_socket_path() {
        Some(path) => path,
        None => {
            return ConnectionOutcome::Unavailable {
                cause: UnavailableCause::SocketUnavailable,
            }
        }
    };
    let connection = match WaylandConnection::connect(&socket_path) {
        Ok(connection) => connection,
        Err(_) => {
            return ConnectionOutcome::Unavailable {
                cause: UnavailableCause::SocketUnavailable,
            }
        }
    };
    let snapshot = Arc::new(Snapshot::new());
    let alive = Arc::new(AtomicBool::new(true));
    let initial_state = Arc::new(parking_lot::Mutex::new(StartupReport::Pending));
    let (handshake_tx, handshake_rx) = std::sync::mpsc::channel::<StartupReport>();
    let io = IoThread::spawn(
        connection,
        snapshot.clone(),
        alive.clone(),
        initial_state.clone(),
        handshake_tx,
    );
    // Block on the I/O thread reporting either an Operational
    // outcome or a typed Unavailable cause. The block is bounded
    // by a short deadline so a misbehaving compositor can't stall
    // the bootstrap.
    let handshake_done = wait_for_handshake(&handshake_rx, &alive);
    let io_alive = io.alive.clone();
    match handshake_done {
        StartupReport::Operational { backend } => {
            // Drop the io thread without joining yet — the
            // dispatch loop is still running in the background and
            // the bootstrap needs the probe `Arc` it carries. The
            // helper drops the handle but keeps the I/O thread
            // alive (the alive flag is still true). The bootstrap
            // consumes the probe `Arc` and the I/O thread drains
            // events until either the compositor disconnects or
            // the probe is dropped (which sets `alive = false`
            // from `WaylandActiveApplication::drop`).
            let probe = WaylandActiveApplication::for_tests_with_backend(snapshot, alive, backend);
            // Suppress the unused warning on `io_alive`; the
            // helper only exists to keep the `Arc` reference
            // readable in the failure arms below.
            let _ = io_alive;
            // Stash the io thread on the probe's `_io` slot so the
            // dispatch loop stays alive for the probe's lifetime.
            // The bootstrap does not need to expose the I/O
            // thread directly — the alive flag and the snapshot are
            // the only state the capture loop reads.
            let probe_with_io = WaylandActiveApplication::attach_io(probe, io);
            ConnectionOutcome::Operational {
                probe: probe_with_io,
                backend,
            }
        }
        StartupReport::Unavailable { cause } => {
            io.shutdown();
            ConnectionOutcome::Unavailable { cause }
        }
        StartupReport::Pending => {
            io.shutdown();
            ConnectionOutcome::Unavailable {
                cause: UnavailableCause::HandshakeFailed,
            }
        }
    }
}

fn wait_for_handshake(
    rx: &std::sync::mpsc::Receiver<StartupReport>,
    _alive: &Arc<AtomicBool>,
) -> StartupReport {
    const HANDSHAKE_DEADLINE: std::time::Duration = std::time::Duration::from_millis(500);
    let start = std::time::Instant::now();
    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(report) => return report,
            Err(_) => {
                if start.elapsed() >= HANDSHAKE_DEADLINE {
                    return StartupReport::Pending;
                }
            }
        }
    }
}

fn wayland_socket_path() -> Option<PathBuf> {
    use std::env;
    // `WAYLAND_SOCKET` is intentionally NOT honoured. The
    // environment variable is the `systemd`-style fd-inheritance
    // shortcut the compositor uses to hand a client a pre-opened
    // socket. The pre-fix implementation rewrote it as
    // `/proc/self/fd/<fd>` and called `UnixStream::connect`, which
    // risks double-closing the descriptor and is brittle on hosts
    // where `/proc/self/fd` is unavailable (sandboxed Flatpak /
    // Snap sessions, certain container runtimes).
    let _ = std::env::var("WAYLAND_SOCKET");
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
    alive: Arc<AtomicBool>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Transport + 'static> IoThread<T> {
    fn spawn(
        connection: WaylandConnection<T>,
        snapshot: Arc<Snapshot>,
        alive: Arc<AtomicBool>,
        _initial_state: Arc<parking_lot::Mutex<StartupReport>>,
        handshake_tx: std::sync::mpsc::Sender<StartupReport>,
    ) -> Self {
        let alive_for_thread = alive.clone();
        let handle = thread::Builder::new()
            .name("clipvault-wayland-toplevel".into())
            .spawn(move || {
                io_thread_main(connection, snapshot, alive_for_thread, handshake_tx);
            })
            .ok();
        Self {
            handle,
            alive,
            _marker: std::marker::PhantomData,
        }
    }

    /// Stop the I/O thread (if still alive) and join it. Returns
    /// the cached handshake verdict in case the caller did not
    /// observe it through the channel. The helper does not block
    /// indefinitely — the `alive` flag flips first and the I/O
    /// thread exits within a single round-trip.
    pub fn shutdown(mut self) {
        self.alive.store(false, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl<T: Transport> Drop for IoThread<T> {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Result the I/O thread hands back to `try_build` once it has
/// either completed the handshake and bound a supported protocol
/// or has hit a startup-time failure that has no recovery.
#[derive(Debug, Clone, Default)]
enum StartupReport {
    /// Default — the I/O thread has not produced a verdict yet.
    #[default]
    Pending,
    /// The handshake reached a [`ConnectionOutcome::Operational`]
    /// outcome.
    Operational { backend: &'static str },
    /// The handshake hit a typed failure before a protocol was
    /// successfully bound.
    Unavailable { cause: UnavailableCause },
}

fn io_thread_main<T: Transport>(
    mut connection: WaylandConnection<T>,
    snapshot: Arc<Snapshot>,
    alive: Arc<AtomicBool>,
    handshake_tx: std::sync::mpsc::Sender<StartupReport>,
) {
    let mut state = ProtocolState::new();
    let outcome = drive_protocol(&mut connection, &snapshot, &mut state, &alive);
    match outcome {
        DriveOutcome::Operational { backend } => {
            snapshot.set_cause(ConnectionCause::Connected);
            let _ = handshake_tx.send(StartupReport::Operational { backend });
            dispatch_loop(&mut connection, &snapshot, state, &alive);
        }
        DriveOutcome::Unavailable {
            cause,
            connection_cause,
        } => {
            snapshot.set_cause(connection_cause);
            let _ = handshake_tx.send(StartupReport::Unavailable { cause });
        }
        DriveOutcome::BackendError => {
            snapshot.set_cause(ConnectionCause::HandshakeFailed);
            let _ = handshake_tx.send(StartupReport::Unavailable {
                cause: UnavailableCause::HandshakeFailed,
            });
        }
    }
    alive.store(false, Ordering::Release);
}

// =====================================================================
// Connection wrapper
// =====================================================================

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
    /// `next_id` starts at 2 because id 1 is the `wl_display` the
    /// compositor hands us at connection time.
    pub fn with_transport(transport: T) -> Self {
        Self {
            transport,
            next_id: 2,
        }
    }

    pub fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        id
    }

    /// Send a fully-marshalled request. The header is built from
    /// `(sender_id, opcode, args.len())` and then the bytes are
    /// written verbatim. The arguments MUST already be in the wire
    /// order the spec mandates — callers go through the `Marshal`
    /// helpers so the byte order matches the documented layout.
    pub fn send_request(&mut self, sender_id: u32, opcode: u16, args: &[u8]) -> io::Result<()> {
        let header = Marshal::request_header(sender_id, opcode, args.len());
        self.transport.send(&header)?;
        if !args.is_empty() {
            self.transport.send(args)?;
        }
        Ok(())
    }

    pub fn read_event(&mut self) -> io::Result<WaylandEvent> {
        let mut header = [0u8; 8];
        self.transport.recv(&mut header)?;
        let (sender_id, opcode, size) = Unmarshal::parse_event_header(&header)?;
        if size < 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Wayland: event size < 8",
            ));
        }
        let mut payload = vec![0u8; (size as usize) - 8];
        if !payload.is_empty() {
            self.transport.recv(&mut payload)?;
        }
        Ok(WaylandEvent {
            sender_id,
            opcode,
            unmarshal: Unmarshal::new(payload),
        })
    }
}

/// In-process variant of `drive_protocol` the integration tests
/// use to drive the protocol state machine synchronously.
#[allow(dead_code)]
pub fn drive_protocol_in_process<T: Transport>(
    connection: WaylandConnection<T>,
    snapshot: Arc<Snapshot>,
) -> io::Result<()> {
    let alive = Arc::new(AtomicBool::new(true));
    let mut connection = connection;
    let mut state = ProtocolState::new();
    let outcome = drive_protocol(&mut connection, &snapshot, &mut state, &alive);
    match outcome {
        DriveOutcome::Operational { .. } => {
            dispatch_loop(&mut connection, &snapshot, state, &alive);
            Ok(())
        }
        DriveOutcome::BackendError => Ok(()),
        DriveOutcome::Unavailable { .. } => Ok(()),
    }
}

// =====================================================================
// Transport abstraction
// =====================================================================

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
        let mut unmarshal = Unmarshal::new(Marshal::string("terminal"));
        assert_eq!(unmarshal.read_string().unwrap(), "terminal");
    }

    #[test]
    fn unmarshal_short_payload_errors() {
        let mut unmarshal = Unmarshal::new(vec![0u8; 3]);
        let error = unmarshal.read_u32().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn unmarshal_array_byte_length_is_decoded_as_bytes() {
        let payload = {
            let mut out = Vec::new();
            out.extend_from_slice(&Marshal::u32(4));
            out.extend_from_slice(&Marshal::u32(2));
            out
        };
        let mut unmarshal = Unmarshal::new(payload);
        let values = unmarshal.read_byte_length_array_u32().unwrap();
        assert_eq!(values, vec![2]);
    }

    #[test]
    fn unmarshal_array_rejects_misaligned_byte_length() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&Marshal::u32(3));
        payload.extend_from_slice(&Marshal::u32(0));
        payload.push(0);
        let mut unmarshal = Unmarshal::new(payload);
        let error = unmarshal.read_byte_length_array_u32().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn allocator_starts_at_id_2_for_wl_display() {
        let transport = MemoryTransport::new();
        let mut connection = WaylandConnection::with_transport(transport);
        let id = connection.alloc_id();
        assert_eq!(id, 2);
    }

    #[test]
    fn allocator_advances_after_each_alloc() {
        let transport = MemoryTransport::new();
        let mut connection = WaylandConnection::with_transport(transport);
        let first = connection.alloc_id();
        let second = connection.alloc_id();
        assert_eq!(first, 2);
        assert_eq!(second, 3);
    }

    #[test]
    fn protocol_kind_interface_names_match_protocol_spec() {
        assert_eq!(
            ProtocolKind::Ext.interface_name(),
            "ext_foreign_toplevel_list_v1"
        );
        assert_eq!(
            ProtocolKind::Zwlr.interface_name(),
            "zwlr_foreign_toplevel_manager_v1"
        );
    }

    #[test]
    fn unavailable_cause_strings_are_stable() {
        assert_eq!(
            UnavailableCause::SocketUnavailable.as_str(),
            "socket_unavailable"
        );
        assert_eq!(
            UnavailableCause::HandshakeFailed.as_str(),
            "handshake_failed"
        );
        assert_eq!(
            UnavailableCause::RegistryWithoutProtocol.as_str(),
            "registry_without_protocol"
        );
        assert_eq!(UnavailableCause::BindRejected.as_str(), "bind_rejected");
        assert_eq!(UnavailableCause::Backend.as_str(), "backend");
    }
}
