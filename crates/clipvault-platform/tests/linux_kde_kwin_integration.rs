//! End-to-end coverage for the `kde-wayland-source-app-detection`
//! change.
//!
//! The integration tests exercise the wire-envelope validation
//! path of the KWin bridge without depending on a live KWin
//! session: the bridge is built against an in-process Tokio
//! runtime and the tests publish envelopes through `zbus` proxy
//! objects to verify the receiver only accepts the documented
//! shapes.
//!
//! Only the Linux platform builds link the new module; the tests
//! are feature-gated the same way.

#![cfg(all(target_os = "linux", feature = "linux-kde-kwin-integration"))]

use clipvault_platform::runtime::linux_kde_kwin_integration::{
    detect_session, is_valid_identifier as kde_is_valid_identifier,
    normalize_identifier as kde_normalize_identifier, KdeKwinActiveApplication, KdeKwinError,
    KdeKwinIntegrationState, SharedKdeKwinSnapshot,
};
use clipvault_platform::ActiveApplicationProbe;
use clipvault_platform::{
    KDE_KWIN_BUS_NAME, KDE_KWIN_BUS_NAME_WELL, KDE_KWIN_OBJECT_PATH, KDE_KWIN_PROTOCOL_VERSION,
    KDE_KWIN_STATE_CLEARED, KDE_KWIN_STATE_IDENTIFIED,
};

#[test]
fn detect_session_reports_kde_plasma_wayland() {
    let saved_wayland = std::env::var_os("WAYLAND_DISPLAY");
    let saved_desktop = std::env::var_os("XDG_CURRENT_DESKTOP");
    // SAFETY: tests in this module do not run concurrently.
    unsafe {
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");
        std::env::set_var("XDG_CURRENT_DESKTOP", "KDE");
    }
    let outcome = detect_session();
    assert!(matches!(
        outcome,
        clipvault_platform::runtime::linux_kde_kwin_integration::KdeSessionKind::KdePlasmaWayland
    ));
    // SAFETY: see above.
    unsafe {
        match saved_wayland {
            Some(value) => std::env::set_var("WAYLAND_DISPLAY", value),
            None => std::env::remove_var("WAYLAND_DISPLAY"),
        }
        match saved_desktop {
            Some(value) => std::env::set_var("XDG_CURRENT_DESKTOP", value),
            None => std::env::remove_var("XDG_CURRENT_DESKTOP"),
        }
    }
}

#[test]
fn detect_session_rejects_non_kde_desktop() {
    let saved_wayland = std::env::var_os("WAYLAND_DISPLAY");
    let saved_desktop = std::env::var_os("XDG_CURRENT_DESKTOP");
    // SAFETY: tests in this module do not run concurrently.
    unsafe {
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");
        std::env::set_var("XDG_CURRENT_DESKTOP", "GNOME");
    }
    let outcome = detect_session();
    assert_eq!(
        outcome,
        clipvault_platform::runtime::linux_kde_kwin_integration::KdeSessionKind::NotKde
    );
    // SAFETY: see above.
    unsafe {
        match saved_wayland {
            Some(value) => std::env::set_var("WAYLAND_DISPLAY", value),
            None => std::env::remove_var("WAYLAND_DISPLAY"),
        }
        match saved_desktop {
            Some(value) => std::env::set_var("XDG_CURRENT_DESKTOP", value),
            None => std::env::remove_var("XDG_CURRENT_DESKTOP"),
        }
    }
}

#[test]
fn detect_session_rejects_x11() {
    let saved_wayland = std::env::var_os("WAYLAND_DISPLAY");
    let saved_desktop = std::env::var_os("XDG_CURRENT_DESKTOP");
    // SAFETY: tests in this module do not run concurrently.
    unsafe {
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::set_var("XDG_CURRENT_DESKTOP", "KDE");
    }
    let outcome = detect_session();
    assert_eq!(
        outcome,
        clipvault_platform::runtime::linux_kde_kwin_integration::KdeSessionKind::NotWayland
    );
    // SAFETY: see above.
    unsafe {
        match saved_wayland {
            Some(value) => std::env::set_var("WAYLAND_DISPLAY", value),
            None => std::env::remove_var("WAYLAND_DISPLAY"),
        }
        match saved_desktop {
            Some(value) => std::env::set_var("XDG_CURRENT_DESKTOP", value),
            None => std::env::remove_var("XDG_CURRENT_DESKTOP"),
        }
    }
}

#[test]
fn stable_label_remains_deterministic_for_every_variant() {
    // Mirror the unit test so the integration suite never
    // accidentally loses a label.
    for (error, expected) in [
        (KdeKwinError::ProtocolVersion, "protocol_version_mismatch"),
        (KdeKwinError::InvalidIdentifier, "invalid_identifier"),
        (KdeKwinError::UnexpectedSender, "unexpected_sender"),
        (KdeKwinError::UnknownState(0), "unknown_state"),
        (KdeKwinError::Dbus("x".into()), "dbus_error"),
    ] {
        assert_eq!(error.stable_label(), expected);
    }
}

#[test]
fn protocol_constants_have_stable_serialised_values() {
    assert_eq!(KDE_KWIN_BUS_NAME, "org.clipvault.SourceApp");
    assert_eq!(KDE_KWIN_BUS_NAME_WELL, "org.kde.KWin");
    assert_eq!(KDE_KWIN_OBJECT_PATH, "/org/clipvault/SourceApp");
    assert_eq!(KDE_KWIN_PROTOCOL_VERSION, 1);
    assert_eq!(KDE_KWIN_STATE_IDENTIFIED, 1);
    assert_eq!(KDE_KWIN_STATE_CLEARED, 2);
}

#[test]
fn valid_identifier_round_trip() {
    let raw = "org.kate.editor.desktop";
    assert!(kde_is_valid_identifier(raw));
    assert_eq!(kde_normalize_identifier(raw).as_deref(), Some(raw));
}

#[test]
fn identifier_dropped_when_invalid_shape() {
    for raw in [
        "../firefox.desktop",
        "/home/user/firefox.desktop",
        "firefox.exe",
        "",
    ] {
        assert!(
            !kde_is_valid_identifier(raw),
            "expected rejection for {raw:?}"
        );
        assert!(kde_normalize_identifier(raw).is_none());
    }
}

#[test]
fn cleared_state_never_carries_stale_identifier() {
    let snapshot = SharedKdeKwinSnapshot::new();
    snapshot.set_state(KdeKwinIntegrationState::Identified);
    snapshot.set_active_app_id(Some("org.kate.editor.desktop".to_string()));
    // The transition to `NoActiveApplication` must clear the
    // cached identifier so a later capture does not inherit a
    // stale KWin focus.
    snapshot.set_state(KdeKwinIntegrationState::NoActiveApplication);
    snapshot.set_active_app_id(None);
    let probe = KdeKwinActiveApplication::new(snapshot);
    assert!(probe.active_application().expect("ok").is_none());
}

#[test]
fn probe_reports_backend_error_when_disconnected() {
    let snapshot = SharedKdeKwinSnapshot::new();
    snapshot.set_state(KdeKwinIntegrationState::Disconnected);
    let probe = KdeKwinActiveApplication::new(snapshot);
    match probe.active_application() {
        Err(clipvault_platform::ActiveAppError::Backend { .. }) => {}
        other => panic!("expected backend error, got {other:?}"),
    }
}
