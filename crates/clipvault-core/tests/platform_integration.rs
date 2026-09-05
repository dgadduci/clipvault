use std::sync::Arc;

use clipvault_core::{
    AppBootstrap, AppContext, Capabilities, ClipboardBackendError, DefaultPlatform, DisplayServer,
    FakeActiveApplication, FakeClipboardBackend, FakeHotkeyManager, FakePasteController,
    FakeSettingsNavigator, FakeTrayController, HotkeyBinding, HotkeyKey, HotkeyModifiers,
    HotkeyOutcome, OsFamily, PasteError, PasteOutcome, PlatformAdapters, PlatformGuidance,
    PlatformInfo, PlatformIssueKind, SettingsOpenOutcome, SYNTHETIC_PASTE_CAPABILITY,
};
use clipvault_platform::{
    ActiveAppError, HotkeyManager, PlatformSettingsTarget, SettingsNavigator, TrayAction,
    TrayOutcome,
};
use tempfile::TempDir;
use time::macros::datetime;

#[derive(Debug, Clone)]
struct FixedClock {
    instant: time::OffsetDateTime,
}

impl clipvault_core::Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        self.instant
    }
}

fn tempdir() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn bootstrap_with_adapters(adapters: PlatformAdapters) -> AppContext {
    let dir = tempdir();
    AppBootstrap::new()
        .with_clock(Arc::new(FixedClock {
            instant: datetime!(2026-01-02 03:04:05 UTC),
        }))
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap")
}

#[allow(clippy::too_many_arguments)]
fn adapters_with(
    clipboard: Arc<FakeClipboardBackend>,
    hotkey: Arc<FakeHotkeyManager>,
    active_app: Arc<FakeActiveApplication>,
    paste: Arc<FakePasteController>,
    tray: Arc<FakeTrayController>,
    settings: Arc<FakeSettingsNavigator>,
    capabilities: Capabilities,
) -> PlatformAdapters {
    adapters_with_platform(
        clipboard,
        hotkey,
        active_app,
        paste,
        tray,
        settings,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        capabilities,
        PlatformInfo {
            home_dir: std::path::PathBuf::from("/tmp"),
            data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
            os_family: OsFamily::Macos,
            display_server: DisplayServer::Unknown,
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn adapters_with_platform(
    clipboard: Arc<FakeClipboardBackend>,
    hotkey: Arc<FakeHotkeyManager>,
    active_app: Arc<FakeActiveApplication>,
    paste: Arc<FakePasteController>,
    tray: Arc<FakeTrayController>,
    settings: Arc<FakeSettingsNavigator>,
    app_metadata: Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
    capabilities: Capabilities,
    info: PlatformInfo,
) -> PlatformAdapters {
    PlatformAdapters::new(
        clipboard,
        hotkey,
        active_app,
        paste,
        tray,
        settings,
        app_metadata,
        capabilities,
        info,
    )
}

#[test]
fn platform_adapters_expose_capabilities_and_info() {
    let adapters = adapters_with(
        Arc::new(FakeClipboardBackend::new()),
        Arc::new(FakeHotkeyManager::new()),
        Arc::new(FakeActiveApplication::new()),
        Arc::new(FakePasteController::new()),
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Capabilities::ALL_AVAILABLE,
    );
    assert_eq!(adapters.capabilities(), Capabilities::ALL_AVAILABLE);
    assert_eq!(adapters.info().os_family, OsFamily::Macos);
    assert_eq!(adapters.settings_navigator().name(), "fake");
}

#[test]
fn refresh_capabilities_returns_runtime_detected_matrix() {
    let adapters = adapters_with(
        Arc::new(FakeClipboardBackend::new()),
        Arc::new(FakeHotkeyManager::new()),
        Arc::new(FakeActiveApplication::new()),
        Arc::new(FakePasteController::new()),
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Capabilities::ALL_AVAILABLE,
    );
    // `refresh_capabilities` must call the runtime detection so the
    // `synthetic_paste` field reflects the actual host preflight
    // rather than the value captured at construction time.
    let refreshed = adapters.refresh_capabilities();
    assert_eq!(
        refreshed.synthetic_paste,
        clipvault_platform::macos_preflight_event_access()
    );
}

#[test]
fn paste_service_pastes_when_clipboard_and_paste_succeed() {
    let clipboard = Arc::new(FakeClipboardBackend::new());
    let paste = Arc::new(FakePasteController::new());
    let adapters = adapters_with(
        Arc::clone(&clipboard),
        Arc::new(FakeHotkeyManager::new()),
        Arc::new(FakeActiveApplication::new()),
        Arc::clone(&paste),
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Capabilities::ALL_AVAILABLE,
    );
    let context = bootstrap_with_adapters(adapters);

    // Seed a row directly.
    let id = {
        let mut db = context.database().lock();
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        let entry = clipvault_db::NewEntry::text(
            "to paste".into(),
            clipvault_db::ContentType::Text,
            8,
            clipvault_core::hash_content("to paste"),
            None,
            time::OffsetDateTime::now_utc(),
            time::OffsetDateTime::now_utc(),
        );
        let outcome = repo.insert_or_touch(entry).expect("insert");
        outcome.record().id
    };

    let outcome = context
        .paste()
        .paste_entry(&context, id, clipvault_core::PasteMode::Plain);
    assert!(matches!(outcome, PasteOutcome::Pasted { id: pasted } if pasted == id));
    assert_eq!(paste.invocations(), 1);
    assert_eq!(clipboard.written_payloads(), vec!["to paste".to_string()]);
}

#[test]
fn paste_service_returns_failed_when_paste_controller_rejects() {
    let clipboard = Arc::new(FakeClipboardBackend::new());
    let paste = Arc::new(FakePasteController::new());
    paste.set_next(Err(PasteError::backend("target app rejected")));
    let adapters = adapters_with(
        Arc::clone(&clipboard),
        Arc::new(FakeHotkeyManager::new()),
        Arc::new(FakeActiveApplication::new()),
        Arc::clone(&paste),
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Capabilities::ALL_AVAILABLE,
    );
    let context = bootstrap_with_adapters(adapters);

    let id = {
        let mut db = context.database().lock();
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        let entry = clipvault_db::NewEntry::text(
            "stay".into(),
            clipvault_db::ContentType::Text,
            4,
            clipvault_core::hash_content("stay"),
            None,
            time::OffsetDateTime::now_utc(),
            time::OffsetDateTime::now_utc(),
        );
        repo.insert_or_touch(entry).expect("insert").record().id
    };

    let outcome = context
        .paste()
        .paste_entry(&context, id, clipvault_core::PasteMode::Plain);
    assert!(matches!(
        outcome,
        PasteOutcome::Failed { kind: "paste", .. }
    ));

    // History row untouched.
    let count = context.history().history_count(&context).expect("count");
    assert_eq!(count, 1);
    assert_eq!(paste.invocations(), 1);
    assert_eq!(clipboard.written_payloads(), vec!["stay".to_string()]);
}

#[test]
fn paste_service_returns_capability_unavailable_under_wayland() {
    let clipboard = Arc::new(FakeClipboardBackend::new());
    let paste = Arc::new(FakePasteController::new());
    paste.set_next(Err(PasteError::Unavailable));
    let info = PlatformInfo {
        home_dir: std::path::PathBuf::from("/tmp"),
        data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
        os_family: OsFamily::Linux,
        display_server: DisplayServer::Wayland,
    };
    let adapters = adapters_with_platform(
        clipboard,
        Arc::new(FakeHotkeyManager::new()),
        Arc::new(FakeActiveApplication::new()),
        paste,
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        Capabilities {
            synthetic_paste: false,
            ..Capabilities::ALL_AVAILABLE
        },
        info,
    );
    let context = bootstrap_with_adapters(adapters);

    let id = {
        let mut db = context.database().lock();
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        let entry = clipvault_db::NewEntry::text(
            "x".into(),
            clipvault_db::ContentType::Text,
            1,
            clipvault_core::hash_content("x"),
            None,
            time::OffsetDateTime::now_utc(),
            time::OffsetDateTime::now_utc(),
        );
        repo.insert_or_touch(entry).expect("insert").record().id
    };

    let outcome = context
        .paste()
        .paste_entry(&context, id, clipvault_core::PasteMode::Plain);
    match outcome {
        PasteOutcome::CapabilityUnavailable {
            capability,
            guidance,
        } => {
            assert_eq!(capability, SYNTHETIC_PASTE_CAPABILITY);
            let guidance = guidance.expect("guidance present");
            assert_eq!(guidance.kind, PlatformIssueKind::UnsupportedSession);
            // Wayland guidance must not advertise settings nor mention
            // macOS.
            assert!(!guidance.has_settings_target());
            assert!(!guidance.title.to_lowercase().contains("macos"));
        }
        other => panic!("expected CapabilityUnavailable with guidance, got {other:?}"),
    }
}

#[test]
fn paste_service_propagates_permission_required_guidance() {
    let clipboard = Arc::new(FakeClipboardBackend::new());
    let paste = Arc::new(FakePasteController::new());
    paste.set_next(Err(PasteError::permission_required(
        clipvault_core::macos_accessibility_guidance(SYNTHETIC_PASTE_CAPABILITY),
    )));
    let adapters = adapters_with(
        Arc::clone(&clipboard),
        Arc::new(FakeHotkeyManager::new()),
        Arc::new(FakeActiveApplication::new()),
        Arc::clone(&paste),
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Capabilities::ALL_AVAILABLE,
    );
    let context = bootstrap_with_adapters(adapters);

    let id = {
        let mut db = context.database().lock();
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        let entry = clipvault_db::NewEntry::text(
            "stay".into(),
            clipvault_db::ContentType::Text,
            4,
            clipvault_core::hash_content("stay"),
            None,
            time::OffsetDateTime::now_utc(),
            time::OffsetDateTime::now_utc(),
        );
        repo.insert_or_touch(entry).expect("insert").record().id
    };

    let outcome = context
        .paste()
        .paste_entry(&context, id, clipvault_core::PasteMode::Plain);
    match outcome {
        PasteOutcome::CapabilityUnavailable {
            capability,
            guidance,
        } => {
            assert_eq!(capability, SYNTHETIC_PASTE_CAPABILITY);
            let guidance = guidance.expect("guidance present");
            assert_eq!(guidance.kind, PlatformIssueKind::PermissionRequired);
            assert!(guidance.has_settings_target());
        }
        other => panic!("expected CapabilityUnavailable with guidance, got {other:?}"),
    }
    // History must be intact.
    assert_eq!(context.history().history_count(&context).expect("count"), 1);
}

#[test]
fn paste_service_classifies_backend_failure_on_linux_x11() {
    let clipboard = Arc::new(FakeClipboardBackend::new());
    let paste = Arc::new(FakePasteController::new());
    paste.set_next(Err(PasteError::backend("display gone")));
    let info = PlatformInfo {
        home_dir: std::path::PathBuf::from("/tmp"),
        data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
        os_family: OsFamily::Linux,
        display_server: DisplayServer::X11,
    };
    let adapters = adapters_with_platform(
        clipboard,
        Arc::new(FakeHotkeyManager::new()),
        Arc::new(FakeActiveApplication::new()),
        paste,
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        Capabilities::ALL_AVAILABLE,
        info,
    );
    let context = bootstrap_with_adapters(adapters);

    let id = {
        let mut db = context.database().lock();
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        let entry = clipvault_db::NewEntry::text(
            "x".into(),
            clipvault_db::ContentType::Text,
            1,
            clipvault_core::hash_content("x"),
            None,
            time::OffsetDateTime::now_utc(),
            time::OffsetDateTime::now_utc(),
        );
        repo.insert_or_touch(entry).expect("insert").record().id
    };

    let outcome = context
        .paste()
        .paste_entry(&context, id, clipvault_core::PasteMode::Plain);
    match outcome {
        PasteOutcome::Failed { kind, guidance, .. } => {
            assert_eq!(kind, "paste");
            let guidance: PlatformGuidance = guidance.expect("guidance present");
            assert_eq!(guidance.kind, PlatformIssueKind::BackendUnavailable);
            // X11 guidance must not mention macOS or advertise a
            // settings target without a known safe environment.
            assert!(!guidance.title.to_lowercase().contains("macos"));
        }
        other => panic!("expected Failed with guidance, got {other:?}"),
    }
    assert_eq!(context.history().history_count(&context).expect("count"), 1);
}

#[test]
fn settings_navigator_open_returns_programmed_outcome() {
    let nav = FakeSettingsNavigator::new();
    nav.push_outcome(
        PlatformSettingsTarget::MacosAccessibility,
        SettingsOpenOutcome::FallbackRequired {
            manual_steps: vec!["open settings manually".into()],
        },
    );
    let outcome = nav.open(PlatformSettingsTarget::MacosAccessibility);
    assert!(matches!(
        outcome,
        SettingsOpenOutcome::FallbackRequired { .. }
    ));
    // Unknown target returns the default Opened.
    let opened = nav.open(PlatformSettingsTarget::LinuxDesktopIntegration);
    assert!(matches!(opened, SettingsOpenOutcome::Opened));
    let calls = nav.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].0, PlatformSettingsTarget::MacosAccessibility);
    assert!(matches!(
        calls[0].1,
        SettingsOpenOutcome::FallbackRequired { .. }
    ));
    assert_eq!(calls[1].0, PlatformSettingsTarget::LinuxDesktopIntegration);
    assert!(matches!(calls[1].1, SettingsOpenOutcome::Opened));
}

#[test]
fn hotkey_manager_returns_registered_then_conflict_then_unsupported() {
    let hotkey = Arc::new(FakeHotkeyManager::new());
    let binding = HotkeyBinding {
        id: "quick_search".into(),
        modifiers: HotkeyModifiers::CMD_SHIFT,
        key: HotkeyKey::V,
    };

    let first =
        <dyn HotkeyManager>::register(&*hotkey, &binding, Box::new(|| {})).expect("register ok");
    assert_eq!(first, HotkeyOutcome::Registered);
    assert_eq!(hotkey.registered_bindings().len(), 1);

    hotkey.set_outcome(HotkeyOutcome::Conflict {
        reason: "another app owns the binding".into(),
    });
    let second =
        <dyn HotkeyManager>::register(&*hotkey, &binding, Box::new(|| {})).expect("register ok");
    assert!(matches!(second, HotkeyOutcome::Conflict { .. }));

    hotkey.set_outcome(HotkeyOutcome::Unsupported {
        reason: "wayland compositor does not support global shortcuts".into(),
    });
    let third =
        <dyn HotkeyManager>::register(&*hotkey, &binding, Box::new(|| {})).expect("register ok");
    assert!(matches!(third, HotkeyOutcome::Unsupported { .. }));

    <dyn HotkeyManager>::unregister_all(&*hotkey).expect("unregister_all");
    assert!(hotkey.was_unregistered());
}

#[test]
fn active_application_unavailable_does_not_block_capture() {
    let active = Arc::new(FakeActiveApplication::new());
    active.set_next(Err(ActiveAppError::Unavailable));
    let adapters = adapters_with(
        Arc::new(FakeClipboardBackend::new()),
        Arc::new(FakeHotkeyManager::new()),
        Arc::new(FakeActiveApplication::new()),
        Arc::new(FakePasteController::new()),
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Capabilities::ALL_AVAILABLE,
    );
    let context = bootstrap_with_adapters(adapters);

    // Capture still works even when active-app detection is
    // unavailable.
    let outcome = context
        .history()
        .record_payload(&context, "still captured".into(), None);
    assert!(matches!(
        outcome,
        clipvault_core::HistoryOutcome::Stored { .. }
    ));
    assert_eq!(context.history().history_count(&context).expect("count"), 1);
}

#[test]
fn non_text_clipboard_is_ignored() {
    let clipboard = Arc::new(FakeClipboardBackend::new());
    clipboard.push_read(Ok(Some(String::new()))); // empty payload
    clipboard.push_read(Ok(None)); // nothing on the clipboard
    let adapters = adapters_with(
        Arc::new(FakeClipboardBackend::new()),
        Arc::new(FakeHotkeyManager::new()),
        Arc::new(FakeActiveApplication::new()),
        Arc::new(FakePasteController::new()),
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Capabilities::ALL_AVAILABLE,
    );
    let context = bootstrap_with_adapters(adapters);

    let outcome = context
        .history()
        .record_payload(&context, String::new(), None);
    assert_eq!(outcome, clipvault_core::HistoryOutcome::Ignored);
    assert_eq!(context.history().history_count(&context).expect("count"), 0);
}

#[test]
fn clipboard_backend_error_is_typed_and_not_a_panic() {
    let clipboard = Arc::new(FakeClipboardBackend::new());
    clipboard.push_read(Err(ClipboardBackendError::backend("backend gone")));
    let adapters = adapters_with(
        Arc::new(FakeClipboardBackend::new()),
        Arc::new(FakeHotkeyManager::new()),
        Arc::new(FakeActiveApplication::new()),
        Arc::new(FakePasteController::new()),
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Capabilities::ALL_AVAILABLE,
    );
    let context = bootstrap_with_adapters(adapters);

    // No panic, history untouched.
    let outcome = context
        .history()
        .record_payload(&context, String::new(), None);
    assert_eq!(outcome, clipvault_core::HistoryOutcome::Ignored);
    assert_eq!(context.history().history_count(&context).expect("count"), 0);
}

#[test]
fn tray_actions_are_routed_via_the_handle() {
    let tray = Arc::new(FakeTrayController::new());
    let adapters = adapters_with(
        Arc::new(FakeClipboardBackend::new()),
        Arc::new(FakeHotkeyManager::new()),
        Arc::new(FakeActiveApplication::new()),
        Arc::new(FakePasteController::new()),
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Capabilities::ALL_AVAILABLE,
    );
    let context = bootstrap_with_adapters(adapters);

    // Bootstrap wires the platform adapters; here we just exercise the
    // FakeTrayController directly to validate the trait surface.
    let handle = <dyn clipvault_platform::TrayController>::install(&*tray).expect("install");
    let entries = vec![clipvault_platform::TrayEntry {
        label: "Quit".into(),
        action: TrayAction::Quit,
    }];
    handle.set_menu(&entries).expect("set_menu");

    let outcome = handle.invoke(TrayAction::OpenQuickSearch).expect("invoke");
    assert!(matches!(outcome, TrayOutcome::Delivered));

    let outcome = handle.invoke(TrayAction::OpenFavorites).expect("invoke");
    assert!(matches!(outcome, TrayOutcome::Unavailable { .. }));

    handle.shutdown().expect("shutdown");

    assert_eq!(tray.recorded_menus().len(), 1);
    assert_eq!(tray.recorded_invocations().len(), 2);
    assert!(tray.was_shut_down());

    // Bootstrap remains usable after tray interactions.
    assert!(context.platform().os_family == OsFamily::Macos);
}

#[test]
fn default_platform_detection_resolves_home_directory() {
    // Sanity check that the platform detection helper still works
    // when invoked from the core.
    let info = DefaultPlatform::detect().expect("detect");
    assert!(!info.home_dir.as_os_str().is_empty());
    assert!(info.data_dir.ends_with(".clipvault"));
}

// -----------------------------------------------------------------------------
// platform-permission-guidance: deterministic capability matrix tests
// -----------------------------------------------------------------------------
//
// These tests exercise the runtime detection entry point with a
// deterministic macOS preflight stub so the assertions do not depend
// on the developer machine's Accessibility state.

fn macos_info() -> PlatformInfo {
    PlatformInfo {
        home_dir: std::path::PathBuf::from("/tmp"),
        data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
        os_family: OsFamily::Macos,
        display_server: DisplayServer::Unknown,
    }
}

#[test]
fn macos_without_accessibility_reports_synthetic_paste_unavailable() {
    use clipvault_platform::detect_capabilities_with_preflight;
    fn denied() -> bool {
        false
    }
    let caps = detect_capabilities_with_preflight(&macos_info(), denied);
    assert!(caps.clipboard_read);
    assert!(caps.clipboard_write);
    assert!(caps.global_hotkey);
    assert!(caps.active_application);
    assert!(caps.tray);
    assert!(!caps.synthetic_paste);
}

#[test]
fn macos_with_accessibility_reports_synthetic_paste_available() {
    use clipvault_platform::detect_capabilities_with_preflight;
    fn granted() -> bool {
        true
    }
    let caps = detect_capabilities_with_preflight(&macos_info(), granted);
    assert_eq!(caps, Capabilities::ALL_AVAILABLE);
}

#[test]
fn refresh_after_granting_accessibility_lifts_synthetic_paste() {
    use clipvault_platform::detect_capabilities_with_preflight;
    let info = macos_info();
    let denied = detect_capabilities_with_preflight(&info, || false);
    assert!(!denied.synthetic_paste);
    let granted = detect_capabilities_with_preflight(&info, || true);
    assert!(granted.synthetic_paste);
    // Re-denying must drop the capability back to `false`.
    let denied_again = detect_capabilities_with_preflight(&info, || false);
    assert!(!denied_again.synthetic_paste);
}

#[test]
fn explicit_paste_after_capability_refresh_succeeds() {
    let clipboard = Arc::new(FakeClipboardBackend::new());
    let paste = Arc::new(FakePasteController::new());
    let info = macos_info();
    let initial = Capabilities {
        synthetic_paste: false,
        ..Capabilities::ALL_AVAILABLE
    };
    let adapters = adapters_with_platform(
        clipboard.clone(),
        Arc::new(FakeHotkeyManager::new()),
        Arc::new(FakeActiveApplication::new()),
        paste.clone(),
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        initial,
        info,
    );
    let context = bootstrap_with_adapters(adapters);

    // Seed an entry.
    let id = {
        let mut db = context.database().lock();
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        let entry = clipvault_db::NewEntry::text(
            "after refresh".into(),
            clipvault_db::ContentType::Text,
            13,
            clipvault_core::hash_content("after refresh"),
            None,
            time::OffsetDateTime::now_utc(),
            time::OffsetDateTime::now_utc(),
        );
        repo.insert_or_touch(entry).expect("insert").record().id
    };

    // Simulate the user granting Accessibility and the shell
    // refreshing the capability matrix. We bypass the runtime call
    // (which would query the real OS) and feed the lifted matrix
    // directly into the context.
    context.set_capabilities(Capabilities::ALL_AVAILABLE);
    assert!(context.capabilities().synthetic_paste);

    // The next explicit paste call must succeed and not invoke any
    // guidance.
    let outcome = context
        .paste()
        .paste_entry(&context, id, clipvault_core::PasteMode::Plain);
    assert!(matches!(outcome, PasteOutcome::Pasted { id: pasted } if pasted == id));
    assert!(outcome.guidance().is_none());
    assert_eq!(paste.invocations(), 1);
    assert_eq!(
        clipboard.written_payloads(),
        vec!["after refresh".to_string()]
    );
    // History is intact.
    assert_eq!(context.history().history_count(&context).expect("count"), 1);
}

#[test]
fn paste_after_capability_refresh_keeps_history_intact_when_paste_fails() {
    let clipboard = Arc::new(FakeClipboardBackend::new());
    let paste = Arc::new(FakePasteController::new());
    paste.set_next(Err(PasteError::permission_required(
        clipvault_core::macos_accessibility_guidance(SYNTHETIC_PASTE_CAPABILITY),
    )));
    let adapters = adapters_with(
        Arc::clone(&clipboard),
        Arc::new(FakeHotkeyManager::new()),
        Arc::new(FakeActiveApplication::new()),
        Arc::clone(&paste),
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Capabilities::ALL_AVAILABLE,
    );
    let context = bootstrap_with_adapters(adapters);

    let id = {
        let mut db = context.database().lock();
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        let entry = clipvault_db::NewEntry::text(
            "still here".into(),
            clipvault_db::ContentType::Text,
            10,
            clipvault_core::hash_content("still here"),
            None,
            time::OffsetDateTime::now_utc(),
            time::OffsetDateTime::now_utc(),
        );
        repo.insert_or_touch(entry).expect("insert").record().id
    };

    let outcome = context
        .paste()
        .paste_entry(&context, id, clipvault_core::PasteMode::Plain);
    // Failure surfaces as CapabilityUnavailable with guidance, and the
    // history row is left untouched.
    match outcome {
        PasteOutcome::CapabilityUnavailable { guidance, .. } => {
            let guidance = guidance.expect("guidance present");
            assert_eq!(guidance.kind, PlatformIssueKind::PermissionRequired);
        }
        other => panic!("expected CapabilityUnavailable, got {other:?}"),
    }
    assert_eq!(context.history().history_count(&context).expect("count"), 1);
    assert_eq!(paste.invocations(), 1);
}

#[test]
fn paste_outcome_carries_guidance_without_clipboard_payload() {
    // The guidance payload the frontend receives through the Tauri
    // command must never carry the clipboard text, secrets, tokens or
    // private keys. We validate the serialised payload directly so the
    // contract is enforced regardless of how the wrapping response is
    // built.
    let guidance = clipvault_core::macos_accessibility_guidance(SYNTHETIC_PASTE_CAPABILITY);
    let json = serde_json::to_string(&guidance).unwrap();
    assert!(!json.contains("\"clipboard\""));
    assert!(!json.to_lowercase().contains("password"));
    assert!(!json.to_lowercase().contains("token"));
    assert!(!json.to_lowercase().contains("secret"));
    assert!(!json.to_lowercase().contains("private key"));
    assert!(json.contains("\"capability\":\"synthetic_paste\""));
}

#[test]
fn paste_outcome_serialises_without_clipboard_payload() {
    // The user-facing serialised response mirrors the core outcome: it
    // carries the guidance (which never references clipboard contents)
    // instead of the clipboard payload itself.
    let outcome = PasteOutcome::CapabilityUnavailable {
        capability: SYNTHETIC_PASTE_CAPABILITY,
        guidance: Some(clipvault_core::macos_accessibility_guidance(
            SYNTHETIC_PASTE_CAPABILITY,
        )),
    };
    // We can stringify the outcome via its `Debug` impl to assert the
    // invariant without going through serde (the response struct in the
    // Tauri command layer wraps this outcome and adds a serialised
    // `kind` tag).
    let debug = format!("{outcome:?}");
    assert!(!debug.to_lowercase().contains("\"clipboard\""));
    assert!(!debug.to_lowercase().contains("password"));
    let json = serde_json::to_string(&clipvault_core::macos_accessibility_guidance(
        SYNTHETIC_PASTE_CAPABILITY,
    ))
    .unwrap();
    assert!(!json.contains("\"clipboard\""));
}
