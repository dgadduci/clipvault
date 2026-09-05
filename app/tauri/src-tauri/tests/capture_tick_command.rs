//! Integration tests for the `clipvault_capture_tick` and
//! `clipvault_capture_text` Tauri commands.
//!
//! The user-reported bug was that the manual **Tick capture** button
//! could attribute a capture to ClipVault (whose window is focused
//! when the user presses the button) because the frontend passed
//! `sourceApp: null` and the backend trusted the argument. The same
//! shape of bug applied to the `clipvault_capture_text` command. The
//! shell now centralises the resolution on the cached active-app
//! probe and ignores whatever the frontend supplies.
//!
//! The tests below pin the contract:
//!
//! - The manual tick resolves the identifier from the cached probe;
//! - When the cache holds a non-empty identifier the new row carries
//!   that identifier and never falls back to a synthetic ClipVault
//!   label;
//! - An empty cache preserves the existing
//!   "unknown source → Allow" path and the row keeps `source_app = NULL`;
//! - Frontend-supplied identifiers are deliberately ignored so a
//!   malicious frontend cannot smuggle a blacklisted identifier or
//!   substitute ClipVault for the real source.
//!
//! The `SharedState` constructor is reached through the helper that
//! also exercises the persistence layer so the tests cover both the
//! shell wrapper and the underlying core behaviour.

use std::path::PathBuf;
use std::sync::Arc;

use clipvault_app::state::SharedState;
use clipvault_core::{
    ActiveApplication, ActiveApplicationProbe, AppBootstrap, Clipboard, Clock, FakeClipboard,
    HistoryOutcome, PlatformAdapters, SystemClock, WatchTickOutcome,
};
use clipvault_db::{builtin_migrations, IgnoredAppRepository};
use clipvault_platform::{
    Capabilities, ClipboardBackend, DisplayServer, HotkeyManager, OsFamily, PasteController,
    PlatformInfo, SettingsNavigator, TrayController,
};

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        time::OffsetDateTime::now_utc()
    }
}

struct ScriptedProbe(&'static str);

impl ActiveApplicationProbe for ScriptedProbe {
    fn active_application(
        &self,
    ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
    {
        Ok(Some(ActiveApplication::new("App", self.0)))
    }
    fn name(&self) -> &'static str {
        "shell-tests-scripted"
    }
}

struct ScriptedClipboard(Vec<String>);

impl Clipboard for ScriptedClipboard {
    fn read_text(&self) -> Result<Option<String>, clipvault_core::ClipboardError> {
        let mut iter = self.0.iter();
        Ok(iter.next().cloned())
    }
}

fn build_harness_with_identifier(identifier: &'static str) -> (tempfile::TempDir, SharedState) {
    use clipvault_core::{FakeHotkeyManager, FakePasteController, FakeSettingsNavigator};
    let dir = tempfile::tempdir().expect("tempdir");
    {
        let mut db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");
    }
    let info = PlatformInfo {
        home_dir: PathBuf::from("/tmp"),
        data_dir: dir.path().to_path_buf(),
        os_family: OsFamily::Macos,
        display_server: DisplayServer::Unknown,
    };
    let probe: Arc<dyn ActiveApplicationProbe> = Arc::new(ScriptedProbe(identifier));
    let fake_clipboard = Arc::new(clipvault_core::FakeClipboardBackend::new());
    // Queue a single read so the watcher has content to persist on
    // the first tick. Without this the harness reads `None` from the
    // fake backend and returns `WatchTickOutcome::Ignored`.
    fake_clipboard.push_read(Ok(Some("cv-shell-tick-payload".into())));
    let platform_adapters = PlatformAdapters::new(
        fake_clipboard.clone() as Arc<dyn ClipboardBackend>,
        Arc::new(FakeHotkeyManager::new()) as Arc<dyn HotkeyManager>,
        probe,
        Arc::new(FakePasteController::new()) as Arc<dyn PasteController>,
        Arc::new(clipvault_core::FakeTrayController::new()) as Arc<dyn TrayController>,
        Arc::new(FakeSettingsNavigator::new()) as Arc<dyn SettingsNavigator>,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        Capabilities::ALL_AVAILABLE,
        info,
    );
    let context = AppBootstrap::new()
        .with_clock(Arc::new(FixedClock) as Arc<dyn Clock>)
        .with_clipboard(Arc::new(FakeClipboard::new()) as Arc<dyn Clipboard>)
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    context
        .cached_active_app_probe()
        .refresh_with(Some(ActiveApplication::new("App", identifier)));

    let watcher = clipvault_core::CaptureWatcher::new(
        context.platform_adapters().clipboard(),
        std::time::Duration::from_millis(10),
    );
    let app_state = clipvault_app::bootstrap::AppState {
        context: context.clone(),
        watcher: Arc::new(watcher),
        adapters: context.platform_adapters().clone(),
        cancel_capture: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        active_app_refresher: None,
        metadata_scheduler: Arc::new(
            clipvault_app::metadata_scheduler::MetadataEnrichmentScheduler::new(),
        ),
    };
    (dir, SharedState::new(app_state))
}

/// Manual tick routes the identifier through the cached probe even
/// when the frontend tries to override it with `ClipVault`. The
/// backend MUST ignore the frontend-supplied argument and use the
/// cached identifier instead.
#[test]
fn capture_tick_uses_cached_probe_identifier_and_ignores_frontend_argument() {
    let (_dir, state) = build_harness_with_identifier("com.apple.Terminal");

    // Frontend claims the capture came from ClipVault; the backend
    // must ignore the lie and use the cached identifier.
    let outcome = state.tick(Some("ClipVault"));
    assert!(matches!(
        outcome,
        WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
    ));

    let recent = state
        .context()
        .history()
        .recent_entries(state.context(), 10)
        .unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].source_app.as_deref(), Some("com.apple.Terminal"));
    assert_ne!(recent[0].source_app.as_deref(), Some("ClipVault"));
}

/// When the cache is empty the manual tick preserves the
/// "unknown source → Allow" contract: the capture is stored, the row
/// keeps `source_app = NULL` and the frontend can render the
/// fallback accessible label.
#[test]
fn capture_tick_with_empty_cache_persists_unknown_source() {
    use clipvault_core::{FakeHotkeyManager, FakePasteController, FakeSettingsNavigator};
    use clipvault_platform::NoopActiveApplicationProbe;

    let dir = tempfile::tempdir().expect("tempdir");
    {
        let mut db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");
    }
    let info = PlatformInfo {
        home_dir: PathBuf::from("/tmp"),
        data_dir: dir.path().to_path_buf(),
        os_family: OsFamily::Macos,
        display_server: DisplayServer::Unknown,
    };
    let fake_clipboard = Arc::new(clipvault_core::FakeClipboardBackend::new());
    fake_clipboard.push_read(Ok(Some("cv-shell-empty-cache".into())));
    let platform_adapters = PlatformAdapters::new(
        fake_clipboard as Arc<dyn ClipboardBackend>,
        Arc::new(FakeHotkeyManager::new()) as Arc<dyn HotkeyManager>,
        Arc::new(NoopActiveApplicationProbe) as Arc<dyn ActiveApplicationProbe>,
        Arc::new(FakePasteController::new()) as Arc<dyn PasteController>,
        Arc::new(clipvault_core::FakeTrayController::new()) as Arc<dyn TrayController>,
        Arc::new(FakeSettingsNavigator::new()) as Arc<dyn SettingsNavigator>,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        Capabilities::ALL_AVAILABLE,
        info,
    );
    let context = AppBootstrap::new()
        .with_clock(Arc::new(FixedClock) as Arc<dyn Clock>)
        .with_clipboard(Arc::new(FakeClipboard::new()) as Arc<dyn Clipboard>)
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    assert!(context.cached_active_application().is_none());

    let watcher = clipvault_core::CaptureWatcher::new(
        context.platform_adapters().clipboard(),
        std::time::Duration::from_millis(10),
    );
    let app_state = clipvault_app::bootstrap::AppState {
        context: context.clone(),
        watcher: Arc::new(watcher),
        adapters: context.platform_adapters().clone(),
        cancel_capture: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        active_app_refresher: None,
        metadata_scheduler: Arc::new(
            clipvault_app::metadata_scheduler::MetadataEnrichmentScheduler::new(),
        ),
    };
    let state = SharedState::new(app_state);

    let outcome = state.tick(Some("anything-the-frontend-tries"));
    assert!(matches!(
        outcome,
        WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
    ));

    let recent = state
        .context()
        .history()
        .recent_entries(state.context(), 10)
        .unwrap();
    assert_eq!(recent.len(), 1);
    assert!(recent[0].source_app.is_none());
}

/// Captura manual con aplicación activa TextEdit: el backend
/// resuelve TextEdit a través del probe cacheado, no del frontend.
#[test]
fn capture_tick_persists_textedit_identifier() {
    let (_dir, state) = build_harness_with_identifier("com.apple.TextEdit");
    let outcome = state.tick(None);
    assert!(matches!(
        outcome,
        WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
    ));
    let recent = state
        .context()
        .history()
        .recent_entries(state.context(), 10)
        .unwrap();
    assert_eq!(recent[0].source_app.as_deref(), Some("com.apple.TextEdit"));
}

/// El identificador que consume `PrivacyGate` desde el tick manual
/// coincide con el que se persiste: con un identificador en la
/// blacklist la captura queda descartada.
#[test]
fn capture_tick_uses_cached_identifier_for_privacy_gate() {
    let dir = tempfile::tempdir().expect("tempdir");
    {
        let mut db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");
        let conn = db.connection_mut();
        IgnoredAppRepository::new(conn)
            .insert("1password", time::OffsetDateTime::now_utc())
            .expect("insert 1password");
    }
    let info = PlatformInfo {
        home_dir: PathBuf::from("/tmp"),
        data_dir: dir.path().to_path_buf(),
        os_family: OsFamily::Macos,
        display_server: DisplayServer::Unknown,
    };
    let probe: Arc<dyn ActiveApplicationProbe> = Arc::new(ScriptedProbe("1password"));
    let fake_clipboard = Arc::new(clipvault_core::FakeClipboardBackend::new());
    fake_clipboard.push_read(Ok(Some("cv-shell-blacklist-payload".into())));
    let platform_adapters = PlatformAdapters::new(
        fake_clipboard as Arc<dyn ClipboardBackend>,
        Arc::new(clipvault_core::FakeHotkeyManager::new()) as Arc<dyn HotkeyManager>,
        probe,
        Arc::new(clipvault_core::FakePasteController::new()) as Arc<dyn PasteController>,
        Arc::new(clipvault_core::FakeTrayController::new()) as Arc<dyn TrayController>,
        Arc::new(clipvault_core::FakeSettingsNavigator::new()) as Arc<dyn SettingsNavigator>,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        Capabilities::ALL_AVAILABLE,
        info,
    );
    let context = AppBootstrap::new()
        .with_clock(Arc::new(FixedClock) as Arc<dyn Clock>)
        .with_clipboard(Arc::new(FakeClipboard::new()) as Arc<dyn Clipboard>)
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    context
        .cached_active_app_probe()
        .refresh_with(Some(ActiveApplication::new("1Password", "1password")));

    let watcher = clipvault_core::CaptureWatcher::new(
        context.platform_adapters().clipboard(),
        std::time::Duration::from_millis(10),
    );
    let app_state = clipvault_app::bootstrap::AppState {
        context: context.clone(),
        watcher: Arc::new(watcher),
        adapters: context.platform_adapters().clone(),
        cancel_capture: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        active_app_refresher: None,
        metadata_scheduler: Arc::new(
            clipvault_app::metadata_scheduler::MetadataEnrichmentScheduler::new(),
        ),
    };
    let state = SharedState::new(app_state);

    // The frontend cannot bypass the blacklist by claiming the
    // capture came from a benign app — the backend ignores the
    // frontend argument.
    let outcome = state.tick(Some("com.apple.Safari"));
    assert!(
        matches!(outcome, WatchTickOutcome::Captured(HistoryOutcome::Ignored)),
        "blacklisted source must be discarded, got {outcome:?}"
    );
    let recent = state
        .context()
        .history()
        .recent_entries(state.context(), 10)
        .unwrap();
    assert!(recent.is_empty());
}

/// `clipvault_capture_text` debe resolver a través del probe
/// cacheado igual que el tick manual. Centralizar la resolución
/// evita que un caller sustituya el identificador por un literal
/// tipo ClipVault.
#[test]
fn capture_text_command_resolves_identifier_from_cached_probe() {
    use clipvault_core::{FakeHotkeyManager, FakePasteController, FakeSettingsNavigator};
    let dir = tempfile::tempdir().expect("tempdir");
    {
        let mut db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");
    }
    let info = PlatformInfo {
        home_dir: PathBuf::from("/tmp"),
        data_dir: dir.path().to_path_buf(),
        os_family: OsFamily::Macos,
        display_server: DisplayServer::Unknown,
    };
    let probe: Arc<dyn ActiveApplicationProbe> = Arc::new(ScriptedProbe("com.apple.Terminal"));
    let platform_adapters = PlatformAdapters::new(
        Arc::new(clipvault_core::FakeClipboardBackend::new()) as Arc<dyn ClipboardBackend>,
        Arc::new(FakeHotkeyManager::new()) as Arc<dyn HotkeyManager>,
        probe,
        Arc::new(FakePasteController::new()) as Arc<dyn PasteController>,
        Arc::new(clipvault_core::FakeTrayController::new()) as Arc<dyn TrayController>,
        Arc::new(FakeSettingsNavigator::new()) as Arc<dyn SettingsNavigator>,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        Capabilities::ALL_AVAILABLE,
        info,
    );
    let context = AppBootstrap::new()
        .with_clock(Arc::new(FixedClock) as Arc<dyn Clock>)
        .with_clipboard(
            Arc::new(FakeClipboard::with_text("cv-shell-capture-text-payload"))
                as Arc<dyn Clipboard>,
        )
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    context
        .cached_active_app_probe()
        .refresh_with(Some(ActiveApplication::new(
            "Terminal",
            "com.apple.Terminal",
        )));

    // `clipvault_capture_text` ignores the caller-supplied argument
    // and routes through the cached probe instead. Mirroring the
    // command body (which the tests can exercise without standing
    // up a Tauri runtime) ensures we pin the same behaviour.
    let identifier = clipvault_app::bootstrap::resolved_source_identifier(&context);
    let outcome = context
        .history()
        .record_text(&context, identifier.as_deref());
    assert!(matches!(
        outcome,
        clipvault_core::HistoryOutcome::Stored { .. }
    ));
    let recent = context.history().recent_entries(&context, 10).unwrap();
    assert_eq!(recent[0].source_app.as_deref(), Some("com.apple.Terminal"));
}

/// Silences the unused warning for `ScriptedClipboard` and
/// `SystemClock` while keeping the imports tight.
#[allow(dead_code)]
fn _ensure_imports() {
    let _: Option<String> = ScriptedClipboard(Vec::new()).read_text().ok().flatten();
    let _: Arc<dyn Clock> = Arc::new(SystemClock);
}
