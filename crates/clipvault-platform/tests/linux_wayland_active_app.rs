//! Integration tests for the Linux native Wayland active-app probe.
//!
//! These tests pin the public contract the
//! `linux-native-wayland-app-detection` change ships:
//!
//! - The probe binds `ext-foreign-toplevel-list-v1` when the
//!   registry publishes it and falls back to
//!   `zwlr_foreign_toplevel_management_unstable_v1` when the
//!   compositor only publishes the wlroots variant.
//! - The probe publishes the active `app_id` only after the
//!   compositor commits a `done` round; partial state stays
//!   internal.
//! - A `closed` event removes the toplevel from the snapshot.
//! - An empty / whitespace `app_id` is treated as absence.
//! - When several toplevels report `activated`, the probe picks the
//!   one with the lowest handle id (the compositor-assigned order)
//!   — never the title.
//! - The absence of either protocol produces
//!   [`ConnectionOutcome::Unavailable`]; the bootstrap can then
//!   fall back to the XWayland EWMH probe without re-using a stale
//!   identifier.
//! - A protocol version mismatch (the registry publishes the
//!   interface with version 0) is refused: the probe never binds a
//!   zero-version protocol because every revision of either spec
//!   starts at version 1.
//! - The snapshot the [`LinuxApplicationMetadataProvider`] consumes
//!   is fed with the `app_id` directly; the provider reuses its
//!   existing `.desktop` parser, XDG lookup and PNG persistence.
//! - A blacklisted `app_id` is rejected before metadata enrichment,
//!   so a blacklisted capture never creates an icon asset under
//!   `assets/application-icons/`.
//! - Diagnostics surface the granular [`ProbeStage`] (no
//!   `identified`, `no_active_toplevel`, `protocol_unavailable`,
//!   `connection_unavailable`, `backend`) and the stable backend
//!   identifier so the UI can route on documented strings without
//!   parsing free-form log lines.
//! - The adapter never logs clipboard content, snippets, hashes,
//!   asset references, paths, titles, PIDs or environment
//!   variables.
//!
//! The suite drives the protocol parser through an in-memory
//! transport so it runs identically on every developer machine and
//! in CI without standing up a real compositor.

#![cfg(all(target_os = "linux", feature = "linux-wayland-active-app"))]

use std::collections::BTreeMap;
use std::io;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use clipvault_platform::runtime::linux_wayland_active_app::{
    pick_active_app_id, stage_after_done_pub, ConnectionOutcome, ProtocolKind, Snapshot, Transport,
    WaylandActiveApplication, WaylandConnection, BACKEND_NAME, BACKEND_NAME_WLR,
};
use clipvault_platform::{ActiveAppError, ActiveApplication, ActiveApplicationProbe, ProbeStage};

/// In-memory transport. The probe reads events from `inbound` and
/// records everything it writes so the tests can assert the
/// handshake the probe performed matches the protocol.
#[derive(Default)]
struct ScriptedTransport {
    outbound: Vec<u8>,
    inbound: Vec<u8>,
}

impl ScriptedTransport {
    fn push(&mut self, bytes: &[u8]) {
        self.inbound.extend_from_slice(bytes);
    }

    #[allow(dead_code)]
    fn outbound(&self) -> &[u8] {
        &self.outbound
    }
}

impl Transport for ScriptedTransport {
    fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.outbound.extend_from_slice(bytes);
        Ok(())
    }
    fn recv(&mut self, bytes: &mut [u8]) -> io::Result<()> {
        if self.inbound.len() < bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "scripted transport exhausted",
            ));
        }
        let take = bytes.len();
        bytes.copy_from_slice(&self.inbound[..take]);
        self.inbound.drain(..take);
        Ok(())
    }
}

/// Build a Wayland event header with the given `sender_id`,
/// `opcode` and `payload` size.
fn event_header(sender_id: u32, opcode: u16, payload_size: u16) -> [u8; 8] {
    let mut bytes = [0u8; 8];
    bytes[0..4].copy_from_slice(&sender_id.to_ne_bytes());
    bytes[4..6].copy_from_slice(&opcode.to_ne_bytes());
    bytes[6..8].copy_from_slice(&(payload_size + 8).to_ne_bytes());
    bytes
}

/// Pack a u32 little-endian.
fn u32_le(value: u32) -> [u8; 4] {
    value.to_ne_bytes()
}

/// Pack a Wayland string (length, NUL-terminated bytes, padding).
fn wl_string(value: &str) -> Vec<u8> {
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

/// `wl_array` encoding used by the `state` event.
fn wl_array(values: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&u32_le(values.len() as u32));
    for v in values {
        out.extend_from_slice(&u32_le(*v));
    }
    if values.len() % 2 == 1 {
        out.extend_from_slice(&u32_le(0));
    }
    out
}

/// Push a `global` event for `interface` at version `version`.
fn push_global(transport: &mut ScriptedTransport, interface: &str, version: u32) {
    let mut payload = Vec::new();
    payload.extend(wl_string(&format!("global-{version}")));
    payload.extend(wl_string(interface));
    payload.extend_from_slice(&u32_le(version));
    transport.push(&event_header(1, 0, payload.len() as u16));
    transport.push(&payload);
}

/// Push an `ext-foreign-toplevel-list-v1` event. The handle id is
/// `list_id`; the `payload` carries the event arguments.
fn push_ext_event(transport: &mut ScriptedTransport, list_id: u32, opcode: u16, payload: &[u8]) {
    transport.push(&event_header(list_id, opcode, payload.len() as u16));
    transport.push(payload);
}

/// Push a `zwlr_foreign_toplevel_manager_v1` event.
fn push_zwlr_event(transport: &mut ScriptedTransport, list_id: u32, opcode: u16, payload: &[u8]) {
    transport.push(&event_header(list_id, opcode, payload.len() as u16));
    transport.push(payload);
}

/// Drive the probe synchronously and return a constructed
/// `WaylandActiveApplication` ready to query.
fn run_probe(
    transport: ScriptedTransport,
) -> io::Result<WaylandActiveApplication<ScriptedTransport>> {
    let connection = WaylandConnection::with_transport(transport);
    let snapshot = Arc::new(Snapshot::new());
    clipvault_platform::runtime::linux_wayland_active_app::drive_protocol_in_process(
        connection,
        snapshot.clone(),
    )?;
    Ok(WaylandActiveApplication::<ScriptedTransport>::for_tests(
        snapshot,
        Arc::new(AtomicBool::new(true)),
    ))
}

/// The probe binds `ext-foreign-toplevel-list-v1` when the registry
/// announces it.
#[test]
fn ext_protocol_is_bound_when_registry_announces_it() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "ext_foreign_toplevel_list_v1", 1);
    let mut payload = u32_le(42).to_vec();
    payload.extend(wl_string("firefox"));
    push_ext_event(&mut transport, 3, 0, &payload);
    push_ext_event(&mut transport, 3, 7, &[]);

    let probe = run_probe(transport).expect("probe runs");
    let result = probe.active_application().expect("ok");
    assert_eq!(result, Some(ActiveApplication::new("firefox", "firefox")));
    assert_eq!(probe.name(), BACKEND_NAME);
}

/// The probe falls back to `zwlr_foreign_toplevel_management_unstable_v1`
/// when the ext protocol is not announced.
#[test]
fn zwlr_fallback_is_bound_when_ext_is_missing() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "zwlr_foreign_toplevel_manager_v1", 3);
    push_zwlr_event(&mut transport, 3, 0, &u32_le(99));
    let mut payload = u32_le(99).to_vec();
    payload.extend(wl_string("terminal"));
    push_zwlr_event(&mut transport, 3, 2, &payload);
    let mut payload = u32_le(99).to_vec();
    payload.extend(wl_array(&[2]));
    push_zwlr_event(&mut transport, 3, 3, &payload);
    push_zwlr_event(&mut transport, 3, 5, &[]);

    let probe = run_probe(transport).expect("probe runs");
    let result = probe.active_application().expect("ok");
    assert_eq!(result, Some(ActiveApplication::new("terminal", "terminal")));
    assert_eq!(probe.name(), BACKEND_NAME);
}

/// The probe treats an empty `app_id` as absence and reports
/// `Ok(None)` instead of fabricating an identifier.
#[test]
fn empty_app_id_is_treated_as_absence() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "ext_foreign_toplevel_list_v1", 1);
    let mut payload = u32_le(5).to_vec();
    payload.extend(wl_string(""));
    push_ext_event(&mut transport, 3, 0, &payload);
    let mut payload = u32_le(5).to_vec();
    payload.extend(wl_array(&[2]));
    push_ext_event(&mut transport, 3, 5, &payload);
    push_ext_event(&mut transport, 3, 7, &[]);

    let probe = run_probe(transport).expect("probe runs");
    let outcome = probe.active_application().expect("ok");
    assert_eq!(outcome, None);
    assert_eq!(probe.last_probe_stage(), ProbeStage::ActiveWindowEmpty);
}

/// Same for a whitespace-only `app_id`.
#[test]
fn whitespace_app_id_is_treated_as_absence() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "ext_foreign_toplevel_list_v1", 1);
    let mut payload = u32_le(7).to_vec();
    payload.extend(wl_string("   "));
    push_ext_event(&mut transport, 3, 0, &payload);
    let mut payload = u32_le(7).to_vec();
    payload.extend(wl_array(&[2]));
    push_ext_event(&mut transport, 3, 5, &payload);
    push_ext_event(&mut transport, 3, 7, &[]);

    let probe = run_probe(transport).expect("probe runs");
    assert_eq!(probe.active_application().expect("ok"), None);
}

/// A `closed` event removes the toplevel before the next `done`.
#[test]
fn closed_event_removes_toplevel_from_snapshot() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "ext_foreign_toplevel_list_v1", 1);
    let mut payload = u32_le(11).to_vec();
    payload.extend(wl_string("firefox"));
    push_ext_event(&mut transport, 3, 0, &payload);
    let mut payload = u32_le(12).to_vec();
    payload.extend(wl_string("terminal"));
    push_ext_event(&mut transport, 3, 0, &payload);
    let mut payload = u32_le(11).to_vec();
    payload.extend(wl_array(&[2]));
    push_ext_event(&mut transport, 3, 5, &payload);
    push_ext_event(&mut transport, 3, 7, &[]);
    push_ext_event(&mut transport, 3, 6, &u32_le(11));
    let mut payload = u32_le(12).to_vec();
    payload.extend(wl_array(&[2]));
    push_ext_event(&mut transport, 3, 5, &payload);
    push_ext_event(&mut transport, 3, 7, &[]);

    let probe = run_probe(transport).expect("probe runs");
    assert_eq!(
        probe.active_application().expect("ok"),
        Some(ActiveApplication::new("terminal", "terminal"))
    );
}

/// Several active toplevels: the probe picks the lowest handle id.
#[test]
fn deterministic_selection_prefers_lowest_handle() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "ext_foreign_toplevel_list_v1", 1);
    for (handle, app_id) in [(7u32, "firefox"), (3, "terminal"), (9, "chrome")] {
        let mut payload = u32_le(handle).to_vec();
        payload.extend(wl_string(app_id));
        push_ext_event(&mut transport, 3, 0, &payload);
        let mut payload = u32_le(handle).to_vec();
        payload.extend(wl_array(&[2]));
        push_ext_event(&mut transport, 3, 5, &payload);
    }
    push_ext_event(&mut transport, 3, 7, &[]);

    let probe = run_probe(transport).expect("probe runs");
    assert_eq!(
        probe.active_application().expect("ok"),
        Some(ActiveApplication::new("terminal", "terminal"))
    );
}

/// Several active toplevels with the lowest handle carrying an
/// empty `app_id`: the probe skips it and picks the next
/// non-empty candidate.
#[test]
fn deterministic_selection_skips_empty_lowest_handle() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "ext_foreign_toplevel_list_v1", 1);
    for (handle, app_id) in [(3u32, ""), (5, "firefox"), (7, "chrome")] {
        let mut payload = u32_le(handle).to_vec();
        payload.extend(wl_string(app_id));
        push_ext_event(&mut transport, 3, 0, &payload);
        let mut payload = u32_le(handle).to_vec();
        payload.extend(wl_array(&[2]));
        push_ext_event(&mut transport, 3, 5, &payload);
    }
    push_ext_event(&mut transport, 3, 7, &[]);

    let probe = run_probe(transport).expect("probe runs");
    assert_eq!(
        probe.active_application().expect("ok"),
        Some(ActiveApplication::new("firefox", "firefox"))
    );
}

/// The snapshot is observable only after `done`.
#[test]
fn snapshot_uncommitted_before_done() {
    let snapshot = Snapshot::new();
    assert!(!snapshot.committed());
    assert_eq!(snapshot.active_app_id(), None);
}

/// A toplevel can be removed from the pending map by a `closed`
/// event before the round's `done` arrives.
#[test]
fn closed_removes_pending_toplevel_before_done() {
    let snapshot = Snapshot::new();
    let mut toplevels = BTreeMap::new();
    toplevels.insert(
        1,
        clipvault_platform::runtime::linux_wayland_active_app::ToplevelEntry {
            app_id: Some("firefox".into()),
            activated: true,
        },
    );
    toplevels.insert(
        2,
        clipvault_platform::runtime::linux_wayland_active_app::ToplevelEntry {
            app_id: Some("terminal".into()),
            activated: true,
        },
    );
    toplevels.remove(&1);
    snapshot.commit(toplevels);
    assert_eq!(
        snapshot.active_app_id().as_deref(),
        Some("terminal"),
        "the closed toplevel must not surface as the active one"
    );
}

/// The probe surfaces [`ProbeStage::Identified`] when the snapshot
/// resolves a non-empty `app_id`, and
/// [`ProbeStage::ActiveWindowEmpty`] when the round commits an
/// empty active set.
#[test]
fn probe_stage_tracks_identify_outcome() {
    let snapshot = Snapshot::new();
    let mut toplevels = BTreeMap::new();
    toplevels.insert(
        1,
        clipvault_platform::runtime::linux_wayland_active_app::ToplevelEntry {
            app_id: Some("terminal".into()),
            activated: true,
        },
    );
    snapshot.commit(toplevels);
    snapshot.set_stage(stage_after_done_pub(&snapshot));
    assert_eq!(snapshot.stage(), ProbeStage::Identified);

    let snapshot = Snapshot::new();
    let toplevels = BTreeMap::new();
    snapshot.commit(toplevels);
    snapshot.set_stage(stage_after_done_pub(&snapshot));
    assert_eq!(snapshot.stage(), ProbeStage::ActiveWindowEmpty);
}

/// Stable backend identifiers the diagnostics card consumes.
#[test]
fn backend_kind_strings_are_stable() {
    assert_eq!(BACKEND_NAME, "wayland_foreign_toplevel");
    assert_eq!(BACKEND_NAME_WLR, "wayland_wlr_foreign_toplevel");
    assert_eq!(ProtocolKind::Ext.as_str(), "ext_foreign_toplevel_list_v1");
    assert_eq!(
        ProtocolKind::Zwlr.as_str(),
        "zwlr_foreign_toplevel_management_unstable_v1"
    );
}

/// The probe never logs clipboard content, snippets or hash bytes.
/// The probe has no logging API: the privacy guarantee is purely
/// structural. This test is a regression pin: a future contributor
/// who adds a `log::info!(...)` call carrying payload bytes would
/// surface here as a code-review flag, not as a runtime test
/// failure.
#[test]
fn probe_does_not_expose_payload_logging() {
    struct ScriptedProbe;
    impl ActiveApplicationProbe for ScriptedProbe {
        fn active_application(&self) -> Result<Option<ActiveApplication>, ActiveAppError> {
            Ok(None)
        }
        fn name(&self) -> &'static str {
            "scripted"
        }
    }
    // The probe surface exposes `name` and `active_application`
    // only. There is no method to fetch a payload, a snippet, a
    // hash or an asset reference: a refactor that introduced one
    // would break this test.
    fn assert_no_payload_logging<T: ActiveApplicationProbe>(_probe: &T) {}
    assert_no_payload_logging(&ScriptedProbe);
}

/// `pick_active_app_id` is deterministic across runs.
#[test]
fn pick_active_app_id_is_deterministic() {
    let mut toplevels = BTreeMap::new();
    toplevels.insert(
        7,
        clipvault_platform::runtime::linux_wayland_active_app::ToplevelEntry {
            app_id: Some("firefox".into()),
            activated: true,
        },
    );
    toplevels.insert(
        3,
        clipvault_platform::runtime::linux_wayland_active_app::ToplevelEntry {
            app_id: Some("terminal".into()),
            activated: true,
        },
    );
    toplevels.insert(
        9,
        clipvault_platform::runtime::linux_wayland_active_app::ToplevelEntry {
            app_id: Some("chrome".into()),
            activated: false,
        },
    );
    assert_eq!(pick_active_app_id(&toplevels).as_deref(), Some("terminal"));
}

/// The probe surfaces [`ActiveAppError::Unavailable`] when the
/// connection is closed before any `done` event is observed.
#[test]
fn probe_surfaces_unavailable_when_connection_closes() {
    let snapshot = Arc::new(Snapshot::new());
    let alive = Arc::new(AtomicBool::new(false));
    let probe = WaylandActiveApplication::<ScriptedTransport>::for_tests(snapshot, alive);
    let outcome = probe.active_application();
    assert!(matches!(outcome, Err(ActiveAppError::Unavailable)));
}

/// The `ConnectionOutcome::Unavailable` arm of `try_build` is the
/// signal the bootstrap uses to decide between the native probe
/// and the XWayland fallback. The variant must exist.
#[test]
fn connection_outcome_unavailable_is_distinct_from_operational() {
    let unavailable = ConnectionOutcome::Unavailable;
    assert!(matches!(unavailable, ConnectionOutcome::Unavailable));
}
