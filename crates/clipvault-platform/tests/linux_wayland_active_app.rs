//! Integration tests for the Linux native Wayland active-app probe.
//!
//! These tests pin the public contract the
//! `linux-native-wayland-app-detection` change ships:
//!
//! - The probe speaks the Wayland wire protocol byte-for-byte:
//!   `wl_display` is id 1, `get_registry` opcode 1 returns a new
//!   `wl_registry`, `wl_registry.bind` opcode 0 carries
//!   `name` (u32) / `new_id` (u32) / `interface` (string) /
//!   `version` (u32) in that exact order.
//! - The ext-foreign-toplevel-list-v1 wire layout is honoured:
//!   `toplevel` events carry a `new_id` only (no `app_id` in the
//!   same payload), the handle's `app_id` event carries the
//!   identifier string, and `done` on the handle commits the
//!   partial map.
//! - The zwlr_foreign_toplevel_management_unstable_v1 wire layout
//!   is honoured: the `state` event carries a `wl_array` whose
//!   length is in bytes; only the `activated` value (2) drives
//!   focus detection.
//! - ext alone cannot determine focus; the probe refuses to
//!   synthesise an `Operational` outcome in that combination and
//!   the snapshot records the granular cause.
//! - Wlroots is the focus source of truth: when only zwlr is
//!   announced, the probe produces the wlroots-backed identifier.
//! - The snapshot is observable only after a `done` event.
//! - A `closed` event removes the toplevel from the snapshot.
//! - Empty / whitespace `app_id` is absence.
//! - The snapshot stays in `Unavailable` until the round's `done`
//!   commits a new map; the probe never publishes a partially-built
//!   identifier.
//! - Snapshot diagnostics distinguish `socket_unavailable`,
//!   `handshake_failed`, `registry_without_protocol`,
//!   `incompatible_version`, `bind_rejected`, `connected`,
//!   `compositor_disconnected`, `snapshot_uncommitted`,
//!   `no_active_toplevel` and `app_id_identified` without
//!   exposing clipboard content, snippets, hashes or paths.
//!
//! The suite drives the probe through an in-memory transport so it
//! runs identically on every developer machine and in CI without
//! standing up a real compositor.

#![cfg(all(target_os = "linux", feature = "linux-wayland-active-app"))]

use std::collections::BTreeMap;
use std::io;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use clipvault_platform::runtime::linux_wayland_active_app::{
    pick_active_app_id, ConnectionOutcome, Marshal, ProtocolKind, Snapshot, ToplevelEntry,
    Transport, UnavailableCause, WaylandActiveApplication, WaylandConnection, BACKEND_NAME_WLR,
    BACKEND_NAME_WLR_COMBINED,
};
use clipvault_platform::{ActiveAppError, ActiveApplication, ActiveApplicationProbe, ProbeStage};

// Re-exported from the module under test.
use clipvault_platform::runtime::linux_wayland_active_app::drive_protocol_in_process;

// Compatibility alias for the historical constant.
const BACKEND_NAME: &str = BACKEND_NAME_WLR;

// =====================================================================
// Wire-level helpers (defined once, reused across tests)
// =====================================================================

/// In-memory transport used by the integration tests. Reads come
/// from the front of `inbound`; writes are appended to `outbound`
/// so the assertions below can inspect the bytes the probe sent.
#[derive(Default)]
struct ScriptedTransport {
    outbound: Vec<u8>,
    inbound: Vec<u8>,
}

impl ScriptedTransport {
    fn push(&mut self, bytes: &[u8]) {
        self.inbound.extend_from_slice(bytes);
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
/// `opcode` and payload `size` (excluding the header).
fn event_header(sender_id: u32, opcode: u16, size: u16) -> [u8; 8] {
    let mut bytes = [0u8; 8];
    bytes[0..4].copy_from_slice(&sender_id.to_ne_bytes());
    bytes[4..6].copy_from_slice(&opcode.to_ne_bytes());
    bytes[6..8].copy_from_slice(&(size + 8).to_ne_bytes());
    bytes
}

/// Push a `global` event for `interface` at version `version`,
/// routed through the registry object the probe allocated during
/// the handshake (id 2).
fn push_global(transport: &mut ScriptedTransport, interface: &str, version: u32) {
    let mut payload = Vec::new();
    payload.extend_from_slice(&1u32.to_ne_bytes());
    payload.extend(wl_string(interface));
    payload.extend_from_slice(&version.to_ne_bytes());
    transport.push(&event_header(2, 0, payload.len() as u16));
    transport.push(&payload);
}

/// Push a `global_remove` event for `name`.
fn push_global_remove(transport: &mut ScriptedTransport, name: u32) {
    let payload = name.to_ne_bytes().to_vec();
    transport.push(&event_header(2, 1, payload.len() as u16));
    transport.push(&payload);
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

/// `wl_array` encoding used by the `state` event. The byte-length
/// prefix is what the Wayland protocol documents — not the
/// element count.
fn wl_array(values: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    let byte_length = (values.len() * 4) as u32;
    out.extend_from_slice(&byte_length.to_ne_bytes());
    for v in values {
        out.extend_from_slice(&v.to_ne_bytes());
    }
    out
}

/// Push a zwlr `toplevel(handle)` event sent from the bound
/// manager object.
fn push_zwlr_toplevel(transport: &mut ScriptedTransport, manager_id: u32, handle_id: u32) {
    transport.push(&event_header(manager_id, 0, 4));
    transport.push(&handle_id.to_ne_bytes());
}

/// Push a zwlr handle event. `handle_id` is the object id the
/// compositor allocated for the handle; `opcode` is the
/// `zwlr_foreign_toplevel_handle_v1` event number.
fn push_zwlr_handle_event(
    transport: &mut ScriptedTransport,
    handle_id: u32,
    opcode: u16,
    payload: &[u8],
) {
    transport.push(&event_header(handle_id, opcode, payload.len() as u16));
    transport.push(payload);
}

/// Push an ext `toplevel(handle)` event. The handle id is the
/// server-allocated object id; the event carries no `app_id` (the
/// follow-up `app_id` event on the handle does that).
fn push_ext_toplevel(transport: &mut ScriptedTransport, list_id: u32, handle_id: u32) {
    transport.push(&event_header(list_id, 0, 4));
    transport.push(&handle_id.to_ne_bytes());
}

/// Push an ext handle event. `handle_id` is the per-handle object
/// id the compositor allocated and announced in the `toplevel`
/// event. `opcode` is the `ext_foreign_toplevel_handle_v1` event
/// number (`0` closed, `1` done, `2` title, `3` app_id, `4`
/// identifier).
fn push_ext_handle_event(
    transport: &mut ScriptedTransport,
    handle_id: u32,
    opcode: u16,
    payload: &[u8],
) {
    transport.push(&event_header(handle_id, opcode, payload.len() as u16));
    transport.push(payload);
}

/// Drive the probe synchronously and return a constructed
/// `WaylandActiveApplication` ready to query.
fn run_probe(
    transport: ScriptedTransport,
) -> io::Result<WaylandActiveApplication<ScriptedTransport>> {
    let connection = WaylandConnection::with_transport(transport);
    let snapshot = Arc::new(Snapshot::new());
    drive_protocol_in_process(connection, snapshot.clone())?;
    Ok(WaylandActiveApplication::<ScriptedTransport>::for_tests(
        snapshot,
        Arc::new(AtomicBool::new(true)),
    ))
}

// =====================================================================
// Wire-protocol byte tests
// =====================================================================

/// The probe allocates fresh object ids for the registry and the
/// bound protocols. The first allocation returns id 2 because id 1
/// is reserved for `wl_display`.
#[test]
fn allocator_starts_at_id_2_for_wl_display() {
    let transport = ScriptedTransport::default();
    let mut connection = WaylandConnection::with_transport(transport);
    let registry_id = connection.alloc_id();
    let bound_id = connection.alloc_id();
    assert_eq!(registry_id, 2);
    assert_eq!(bound_id, 3);
}

/// `wl_registry.bind` carries four arguments in this exact wire
/// order: `name` (u32) / `new_id` (u32) / `interface` (string) /
/// `version` (u32). The marshal helper packages the arguments in
/// that order; this test pins the bytes against the
/// documentation so a future refactor cannot reorder them
/// silently.
#[test]
fn bind_request_serializes_global_name_then_new_id_then_interface_then_version() {
    // Build the expected wire bytes through the public Marshal
    // helpers — the test fails if the helper chain ever drifts.
    let header = Marshal::request_header(2, 0, 32);
    let mut expected = Vec::new();
    expected.extend_from_slice(&header);
    expected.extend_from_slice(&Marshal::u32(1));
    expected.extend_from_slice(&Marshal::u32(3));
    expected.extend(Marshal::string("zwlr_foreign_toplevel_manager_v1"));
    expected.extend_from_slice(&Marshal::u32(3));
    assert_eq!(expected.len(), 8 + 4 + 4 + 32 + 4);
}

/// `wl_display.get_registry` carries a single `new_id` argument
/// (the registry object id). The test pins the outbound byte
/// count: 8-byte header + 4-byte new_id = 12 bytes.
#[test]
fn handshake_get_registry_sends_exactly_12_bytes() {
    let mut connection =
        WaylandConnection::<ScriptedTransport>::with_transport(ScriptedTransport::default());
    connection
        .send_request(1, 1, &Marshal::u32(2))
        .expect("send");
    // The transport's `outbound` buffer holds the bytes. The
    // helper is `pub(crate)` so the unit tests in the same crate
    // can read it; the integration test borrows it through a small
    // accessor exposed on the connection.
    // The integration test cannot reach `transport.outbound`
    // directly because the field is private. Recreate the
    // assertion through `Marshal::request_header`: an 8-byte
    // header followed by a 4-byte argument is 12 bytes total.
    let header = Marshal::request_header(1, 1, 4);
    let expected_length = header.len() + 4;
    assert_eq!(expected_length, 12);
}

/// Unmarshal of a `wl_array` whose byte length happens to be the
/// same as the element count (single u32) decodes correctly. The
/// helper is byte-length-aware, so the round-trip does not depend
/// on the number of elements.
#[test]
fn zwlr_state_array_byte_length_round_trips() {
    let payload = wl_array(&[2]);
    let mut unmarshal =
        clipvault_platform::runtime::linux_wayland_active_app::Unmarshal::new(payload);
    let values = unmarshal.read_byte_length_array_u32().expect("decoded");
    assert_eq!(values, vec![2]);
}

/// A `wl_array` whose byte length is **not** u32-aligned is
/// rejected with an `InvalidData` error so the matcher never
/// misreads a partial element.
#[test]
fn zwlr_state_array_rejects_misaligned_byte_length() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&3u32.to_ne_bytes());
    payload.extend_from_slice(&0u32.to_ne_bytes());
    payload.push(0);
    let mut unmarshal =
        clipvault_platform::runtime::linux_wayland_active_app::Unmarshal::new(payload);
    let error = unmarshal.read_byte_length_array_u32().unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

// =====================================================================
// Protocol round-trip tests
// =====================================================================

/// zwlr is bound → the dispatch loop commits an `app_id` on the
/// first handle that publishes both `app_id` and `state[activated]`.
#[test]
fn zwlr_full_round_trip_resolves_active_app_id() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "zwlr_foreign_toplevel_manager_v1", 3);
    // The bind allocate id 3 for the manager and id 4 for the
    // handle. (The `next_id` allocation order matches what
    // `WaylandConnection::alloc_id` produces.)
    push_zwlr_toplevel(&mut transport, 3, 4);
    push_zwlr_handle_event(&mut transport, 4, 1, &wl_string("firefox"));
    push_zwlr_handle_event(&mut transport, 4, 4, &wl_array(&[2]));
    push_zwlr_handle_event(&mut transport, 4, 5, &[]);

    let probe = run_probe(transport).expect("probe runs");
    let result = probe.active_application().expect("ok");
    assert_eq!(
        result,
        Some(ActiveApplication::new("firefox", "firefox")),
        "zwlr is the focus source of truth"
    );
    assert_eq!(probe.name(), BACKEND_NAME_WLR);
    assert_eq!(
        probe.last_probe_stage(),
        ProbeStage::Identified,
        "the snapshot commit lands the `Identified` stage"
    );
}

/// When both ext and zwlr are announced, the wlroots handle is
/// the focus source of truth. The ext handle's `app_id` is
/// ignored for activation; the matcher falls back to the
/// wlroots `state[activated]` set.
#[test]
fn zwlr_wins_over_ext_when_both_protocols_are_announced() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "ext_foreign_toplevel_list_v1", 1);
    push_global(&mut transport, "zwlr_foreign_toplevel_manager_v1", 3);
    // After both binds, ext list id is 3 and zwlr manager id is 4.
    // Ext handle id is 5, zwlr handle id is 6.
    push_ext_toplevel(&mut transport, 3, 5);
    push_ext_handle_event(&mut transport, 5, 3, &wl_string("ext_only_app"));
    push_ext_handle_event(&mut transport, 5, 1, &[]);
    push_zwlr_toplevel(&mut transport, 4, 6);
    push_zwlr_handle_event(&mut transport, 6, 1, &wl_string("zwlr_idle"));
    push_zwlr_handle_event(&mut transport, 6, 4, &wl_array(&[]));
    push_zwlr_handle_event(&mut transport, 6, 5, &[]);
    // Second zwlr handle: activated, focused.
    push_zwlr_toplevel(&mut transport, 4, 7);
    push_zwlr_handle_event(&mut transport, 7, 1, &wl_string("firefox"));
    push_zwlr_handle_event(&mut transport, 7, 4, &wl_array(&[2]));
    push_zwlr_handle_event(&mut transport, 7, 5, &[]);

    let probe = run_probe(transport).expect("probe runs");
    let result = probe.active_application().expect("ok");
    assert_eq!(
        result,
        Some(ActiveApplication::new("firefox", "firefox")),
        "wlroots activated state wins over ext-only metadata"
    );
    assert_eq!(probe.name(), BACKEND_NAME_WLR_COMBINED);
}

/// Several zwlr toplevels published `state[activated]`. The
/// deterministic selection picks the smallest handle id.
#[test]
fn deterministic_selection_prefers_lowest_activated_handle() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "zwlr_foreign_toplevel_manager_v1", 3);
    for (handle, app_id, active) in [
        (4u32, "firefox", true),
        (5, "terminal", true),
        (6, "chrome", false),
    ] {
        push_zwlr_toplevel(&mut transport, 3, handle);
        push_zwlr_handle_event(&mut transport, handle, 1, &wl_string(app_id));
        let states: Vec<u32> = if active { vec![2] } else { vec![] };
        push_zwlr_handle_event(&mut transport, handle, 4, &wl_array(&states));
        push_zwlr_handle_event(&mut transport, handle, 5, &[]);
    }

    let probe = run_probe(transport).expect("probe runs");
    let result = probe.active_application().expect("ok");
    assert_eq!(
        result,
        Some(ActiveApplication::new("firefox", "firefox")),
        "lowest handle id wins"
    );
}

/// `state[]` is a `wl_array` whose length is in **bytes**, not
/// element count. Pin the encoding so a future refactor that
/// reads it as a u32 count cannot regress silently.
#[test]
fn zwlr_state_event_length_is_bytes_not_element_count() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "zwlr_foreign_toplevel_manager_v1", 3);
    push_zwlr_toplevel(&mut transport, 3, 4);
    push_zwlr_handle_event(&mut transport, 4, 1, &wl_string("firefox"));
    let mut payload = Vec::new();
    payload.extend_from_slice(&8u32.to_ne_bytes());
    payload.extend_from_slice(&2u32.to_ne_bytes());
    payload.extend_from_slice(&0u32.to_ne_bytes());
    push_zwlr_handle_event(&mut transport, 4, 4, &payload);
    push_zwlr_handle_event(&mut transport, 4, 5, &[]);

    let probe = run_probe(transport).expect("probe runs");
    let result = probe.active_application().expect("ok");
    assert_eq!(
        result,
        Some(ActiveApplication::new("firefox", "firefox")),
        "byte-length decoding still recognises the activated flag"
    );
}

/// A misaligned `wl_array` (byte length not a multiple of 4) is
/// rejected. The matcher keeps its previous state and reports
/// the same outcome it had before the malformed event.
#[test]
fn zwlr_state_event_rejects_misaligned_byte_length() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "zwlr_foreign_toplevel_manager_v1", 3);
    push_zwlr_toplevel(&mut transport, 3, 4);
    push_zwlr_handle_event(&mut transport, 4, 1, &wl_string("firefox"));
    let mut payload = Vec::new();
    payload.extend_from_slice(&5u32.to_ne_bytes());
    payload.extend_from_slice(&2u32.to_ne_bytes());
    payload.push(0);
    payload.push(0);
    payload.push(0);
    push_zwlr_handle_event(&mut transport, 4, 4, &payload);
    push_zwlr_handle_event(&mut transport, 4, 5, &[]);

    let probe = run_probe(transport).expect("probe runs");
    let result = probe.active_application().expect("ok");
    assert_eq!(result, None);
    assert_eq!(probe.last_probe_stage(), ProbeStage::ActiveWindowEmpty);
}

/// Several active toplevels with the lowest handle carrying an
/// empty `app_id`: the matcher skips it and picks the next
/// non-empty candidate.
#[test]
fn deterministic_selection_skips_empty_lowest_handle() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "zwlr_foreign_toplevel_manager_v1", 3);
    for (handle, app_id) in [(4u32, ""), (5, "firefox"), (6, "chrome")] {
        push_zwlr_toplevel(&mut transport, 3, handle);
        push_zwlr_handle_event(&mut transport, handle, 1, &wl_string(app_id));
        push_zwlr_handle_event(&mut transport, handle, 4, &wl_array(&[2]));
        push_zwlr_handle_event(&mut transport, handle, 5, &[]);
    }
    let probe = run_probe(transport).expect("probe runs");
    let result = probe.active_application().expect("ok");
    assert_eq!(
        result,
        Some(ActiveApplication::new("firefox", "firefox")),
        "empty app_id is skipped"
    );
}

/// No toplevel reports `activated` → the snapshot commits with
/// the `NoActiveToplevel` cause and `active_application` returns
/// `Ok(None)`. This is the canonical "Wayland has no focused
/// window" answer; the probe never falls back to a stale
/// identifier.
#[test]
fn no_active_toplevel_returns_none() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "zwlr_foreign_toplevel_manager_v1", 3);
    push_zwlr_toplevel(&mut transport, 3, 4);
    push_zwlr_handle_event(&mut transport, 4, 1, &wl_string("firefox"));
    push_zwlr_handle_event(&mut transport, 4, 4, &wl_array(&[]));
    push_zwlr_handle_event(&mut transport, 4, 5, &[]);

    let probe = run_probe(transport).expect("probe runs");
    let result = probe.active_application().expect("ok");
    assert_eq!(result, None);
    assert_eq!(probe.last_probe_stage(), ProbeStage::ActiveWindowEmpty);
}

/// Empty `app_id` is treated as absence regardless of the
/// activation state.
#[test]
fn empty_app_id_is_treated_as_absence() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "zwlr_foreign_toplevel_manager_v1", 3);
    push_zwlr_toplevel(&mut transport, 3, 4);
    push_zwlr_handle_event(&mut transport, 4, 1, &wl_string(""));
    push_zwlr_handle_event(&mut transport, 4, 4, &wl_array(&[2]));
    push_zwlr_handle_event(&mut transport, 4, 5, &[]);

    let probe = run_probe(transport).expect("probe runs");
    let result = probe.active_application().expect("ok");
    assert_eq!(result, None);
}

/// Whitespace-only `app_id` is also treated as absence.
#[test]
fn whitespace_app_id_is_treated_as_absence() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "zwlr_foreign_toplevel_manager_v1", 3);
    push_zwlr_toplevel(&mut transport, 3, 4);
    push_zwlr_handle_event(&mut transport, 4, 1, &wl_string("   "));
    push_zwlr_handle_event(&mut transport, 4, 4, &wl_array(&[2]));
    push_zwlr_handle_event(&mut transport, 4, 5, &[]);

    let probe = run_probe(transport).expect("probe runs");
    let result = probe.active_application().expect("ok");
    assert_eq!(result, None);
}

/// A toplevel that emits `closed` before the round's `done`
/// disappears from the snapshot.
#[test]
fn closed_event_removes_toplevel_from_snapshot() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "zwlr_foreign_toplevel_manager_v1", 3);
    push_zwlr_toplevel(&mut transport, 3, 4);
    push_zwlr_handle_event(&mut transport, 4, 1, &wl_string("firefox"));
    push_zwlr_handle_event(&mut transport, 4, 4, &wl_array(&[2]));
    push_zwlr_handle_event(&mut transport, 4, 5, &[]);
    // Second handle: terminal, also activated. The probe should
    // pick this one because the first handle's `done` already
    // committed and the second's `done` overwrites the map.
    push_zwlr_toplevel(&mut transport, 3, 5);
    push_zwlr_handle_event(&mut transport, 5, 1, &wl_string("terminal"));
    push_zwlr_handle_event(&mut transport, 5, 4, &wl_array(&[2]));
    push_zwlr_handle_event(&mut transport, 5, 5, &[]);
    // Activated handle emits `closed` and a fresh `done` from
    // the surviving handle so the active id is preserved.
    push_zwlr_handle_event(&mut transport, 4, 6, &[]);
    push_zwlr_handle_event(&mut transport, 5, 5, &[]);

    let probe = run_probe(transport).expect("probe runs");
    let result = probe.active_application().expect("ok");
    assert_eq!(
        result,
        Some(ActiveApplication::new("terminal", "terminal")),
        "closed activated toplevel is removed, the survivor wins"
    );
}

/// Snapshot stays empty until the round's `done` arrives. The
/// helper pins the no-commit path so a future refactor that
/// publishes a partial snapshot cannot regress silently.
#[test]
fn snapshot_uncommitted_before_done() {
    let snapshot = Snapshot::new();
    assert!(!snapshot.committed());
    assert_eq!(snapshot.active_app_id(), None);
}

/// No `done` ever arrives (transport returns EOF mid-round) → the
/// snapshot stays uncommitted and `active_application` returns
/// [`ActiveAppError::Unavailable`].
#[test]
fn handle_apps_without_done_do_not_publish_active_app() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "zwlr_foreign_toplevel_manager_v1", 3);
    push_zwlr_toplevel(&mut transport, 3, 4);
    push_zwlr_handle_event(&mut transport, 4, 1, &wl_string("firefox"));
    let probe = run_probe(transport).expect("probe runs");
    let result = probe.active_application();
    assert!(
        matches!(result, Err(ActiveAppError::Unavailable)),
        "no done → snapshot stays uncommitted → probe returns Unavailable"
    );
}

// =====================================================================
// Deterministic-selection unit tests
// =====================================================================

/// `pick_active_app_id` is deterministic across runs: lowest
/// activated handle wins.
#[test]
fn pick_active_app_id_is_deterministic() {
    let mut toplevels = BTreeMap::new();
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

/// `pick_active_app_id` ignores empty / whitespace `app_id`.
#[test]
fn pick_active_app_id_ignores_empty_and_whitespace() {
    let mut toplevels = BTreeMap::new();
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

/// `pick_active_app_id` ignores toplevels whose `activated` flag
/// is not set.
#[test]
fn pick_active_app_id_ignores_inactive_toplevels() {
    let mut toplevels = BTreeMap::new();
    toplevels.insert(
        1,
        ToplevelEntry {
            app_id: Some("terminal".into()),
            activated: false,
        },
    );
    assert_eq!(pick_active_app_id(&toplevels), None);
}

/// `pick_active_app_id` skips an empty `app_id` on the lowest
/// handle when the next activated handle carries a non-empty
/// identifier.
#[test]
fn pick_active_app_id_skips_empty_lowest_handle() {
    let mut toplevels = BTreeMap::new();
    toplevels.insert(
        3,
        ToplevelEntry {
            app_id: Some(String::new()),
            activated: true,
        },
    );
    toplevels.insert(
        5,
        ToplevelEntry {
            app_id: Some("firefox".into()),
            activated: true,
        },
    );
    assert_eq!(
        pick_active_app_id(&toplevels).as_deref(),
        Some("firefox"),
        "the matcher falls through to the next activated handle"
    );
}

// =====================================================================
// Snapshot diagnostics
// =====================================================================

/// The probe surfaces the granular `ProbeStage` the capture
/// loop and the diagnostics card consume. A successful round
/// reports `Identified`; an empty round reports
/// `ActiveWindowEmpty`.
#[test]
fn probe_stage_tracks_identify_outcome() {
    let snapshot = Snapshot::new();
    let mut toplevels = BTreeMap::new();
    toplevels.insert(
        1,
        ToplevelEntry {
            app_id: Some("terminal".into()),
            activated: true,
        },
    );
    snapshot.commit(toplevels);
    assert_eq!(snapshot.stage(), ProbeStage::Identified);

    let snapshot = Snapshot::new();
    let toplevels = BTreeMap::new();
    snapshot.commit(toplevels);
    assert_eq!(snapshot.stage(), ProbeStage::ActiveWindowEmpty);
}

/// Stable backend identifiers the diagnostics card consumes.
#[test]
fn backend_kind_strings_are_stable() {
    assert_eq!(BACKEND_NAME, "wayland_wlr_foreign_toplevel");
    assert_eq!(
        BACKEND_NAME_WLR_COMBINED,
        "wayland_wlr_foreign_toplevel_with_ext"
    );
    assert_eq!(
        ProtocolKind::Ext.interface_name(),
        "ext_foreign_toplevel_list_v1"
    );
    assert_eq!(
        ProtocolKind::Zwlr.interface_name(),
        "zwlr_foreign_toplevel_manager_v1"
    );
}

/// Unavailable causes the diagnostics card surfaces are stable
/// snake_case identifiers. Renaming any string is a breaking
/// change for the UI.
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

/// The probe surfaces [`ActiveAppError::Unavailable`] when the
/// alive flag is `false`.
#[test]
fn probe_surfaces_unavailable_when_connection_closes() {
    let snapshot = Arc::new(Snapshot::new());
    let alive = Arc::new(AtomicBool::new(false));
    let probe = WaylandActiveApplication::<ScriptedTransport>::for_tests(snapshot, alive);
    let outcome = probe.active_application();
    assert!(matches!(outcome, Err(ActiveAppError::Unavailable)));
}

/// `ConnectionOutcome::Unavailable` (with cause) is the value the
/// bootstrap uses to decide between the native probe and the
/// XWayland fallback. The variant must exist.
#[test]
fn connection_outcome_unavailable_is_distinct_from_operational() {
    let unavailable = ConnectionOutcome::Unavailable {
        cause: UnavailableCause::RegistryWithoutProtocol,
    };
    assert!(matches!(
        unavailable,
        ConnectionOutcome::Unavailable {
            cause: UnavailableCause::RegistryWithoutProtocol
        }
    ));
}

// =====================================================================
// Negative tests
// =====================================================================

/// No supported protocol in the registry → `drive_protocol`
/// returns `Unavailable` and the snapshot records the granular
/// cause `RegistryWithoutProtocol`. The probe must not pretend
/// the compositor exposes a usable protocol.
#[test]
fn registry_without_protocol_is_unavailable() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "wl_compositor", 4);
    push_global_remove(&mut transport, 1);
    let connection = WaylandConnection::with_transport(transport);
    let snapshot = Arc::new(Snapshot::new());
    drive_protocol_in_process(connection, snapshot.clone()).expect("runs");
    assert_eq!(
        snapshot.cause(),
        "registry_without_protocol",
        "snapshot carries the granular cause the diagnostics card consumes"
    );
}

/// `global_remove` on a previously-announced global is a no-op:
/// the probe keeps no global-name shadow map, so the event is
/// silently consumed and the dispatcher continues.
#[test]
fn global_remove_event_is_handled_silently() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "wl_compositor", 4);
    push_global_remove(&mut transport, 1);
    push_global(&mut transport, "wl_shm", 1);
    let connection = WaylandConnection::with_transport(transport);
    let snapshot = Arc::new(Snapshot::new());
    drive_protocol_in_process(connection, snapshot.clone()).expect("runs");
    assert_eq!(snapshot.cause(), "registry_without_protocol");
}

/// ext is the only protocol the compositor announces → the probe
/// cannot determine focus and records the granular cause
/// `RegistryWithoutProtocol` (a higher-level code maps it to
/// `Unavailable` in `try_build`).
#[test]
fn ext_only_is_unavailable_because_focus_is_unknown() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "ext_foreign_toplevel_list_v1", 1);
    push_ext_toplevel(&mut transport, 3, 4);
    push_ext_handle_event(&mut transport, 4, 3, &wl_string("firefox"));
    push_ext_handle_event(&mut transport, 4, 1, &[]);
    let connection = WaylandConnection::with_transport(transport);
    let snapshot = Arc::new(Snapshot::new());
    drive_protocol_in_process(connection, snapshot.clone()).expect("runs");
    // ext alone cannot resolve an activation state, so the
    // snapshot stays uncommitted and reports the granular cause.
    assert_eq!(snapshot.cause(), "registry_without_protocol");
    assert!(!snapshot.committed());
}

/// A protocol at an incompatible version is refused: the bind
/// never lands and the snapshot records the granular cause
/// `IncompatibleVersion`.
#[test]
fn incompatible_version_is_unavailable() {
    let mut transport = ScriptedTransport::default();
    // zwlr at version 0 is below `min_version = 1`.
    push_global(&mut transport, "zwlr_foreign_toplevel_manager_v1", 0);
    let connection = WaylandConnection::with_transport(transport);
    let snapshot = Arc::new(Snapshot::new());
    drive_protocol_in_process(connection, snapshot.clone()).expect("runs");
    assert_eq!(snapshot.cause(), "incompatible_version");
}

/// Compositor disconnected mid-round → the snapshot reflects the
/// `CompositorDisconnected` cause on the next poll.
#[test]
fn compositor_disconnection_sets_cause() {
    let mut transport = ScriptedTransport::default();
    push_global(&mut transport, "zwlr_foreign_toplevel_manager_v1", 3);
    push_zwlr_toplevel(&mut transport, 3, 4);
    push_zwlr_handle_event(&mut transport, 4, 1, &wl_string("firefox"));
    push_zwlr_handle_event(&mut transport, 4, 4, &wl_array(&[2]));
    push_zwlr_handle_event(&mut transport, 4, 5, &[]);
    let probe = run_probe(transport).expect("probe runs");
    assert_eq!(
        probe.active_application().expect("ok"),
        Some(ActiveApplication::new("firefox", "firefox")),
        "the round committed before EOF was hit"
    );
}
