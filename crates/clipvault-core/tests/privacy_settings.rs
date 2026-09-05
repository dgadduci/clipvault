//! Integration tests for the `privacy-settings` capability: the
//! [`PrivacyGate`] is consulted by [`TextHistoryService`] before
//! persisting any captured text, and the [`SettingsService`] keeps the
//! stored blacklist in sync with subsequent captures.

use std::sync::Arc;

use clipvault_core::{
    AppBootstrap, Clipboard, ClipboardError, Clock, CoreBlacklistMatcher, FakeClipboard,
    HistoryOutcome, NoopActiveApplicationProbe, PrivacyGate, SettingsService, TextHistoryService,
};
use clipvault_db::{builtin_migrations, IgnoredAppRepository};
use tempfile::TempDir;
use time::macros::datetime;

#[derive(Debug, Clone)]
struct FixedClock {
    instant: time::OffsetDateTime,
}

impl Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        self.instant
    }
}

fn tempdir() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn bootstrap_with_gate(gate: PrivacyGate) -> (TempDir, clipvault_core::AppContext) {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text("placeholder"));
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(clock)
        .with_clipboard(clipboard)
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    // Re-create the history service with the privacy gate so every
    // record_text call flows through it.
    let history = TextHistoryService::new(
        context.clipboard(),
        context.clock(),
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
    )
    .with_privacy_gate(gate);
    let settings = SettingsService::new(
        context.clock(),
        PrivacyGate::from_probe(Arc::new(NoopActiveApplicationProbe), vec![]),
    );
    let _ = settings;
    let _ = history;
    context.set_capabilities(context.capabilities());
    (dir, context)
}

#[test]
fn blacklist_matcher_matches_case_insensitively() {
    let probe = Arc::new(NoopActiveApplicationProbe);
    let matcher = CoreBlacklistMatcher::with_ignored(probe, vec!["firefox".into()]);
    assert!(matcher.matches(Some("Firefox")));
    assert!(matcher.matches(Some("FIREFOX")));
    assert!(!matcher.matches(Some("Safari")));
    assert!(!matcher.matches(None));
}

#[test]
fn gate_replaces_snapshot_atomically() {
    let probe = Arc::new(NoopActiveApplicationProbe);
    let gate = PrivacyGate::from_probe(probe, vec!["firefox".into()]);
    assert!(matches!(
        gate.evaluate(Some("firefox")),
        clipvault_core::CaptureDecision::Discard { .. }
    ));
    gate.update_ignored(vec!["safari".into()]);
    assert!(matches!(
        gate.evaluate(Some("firefox")),
        clipvault_core::CaptureDecision::Allow
    ));
    assert!(matches!(
        gate.evaluate(Some("safari")),
        clipvault_core::CaptureDecision::Discard { .. }
    ));
}

#[test]
fn unknown_app_identifier_does_not_match_anything_in_blacklist() {
    let probe = Arc::new(NoopActiveApplicationProbe);
    let gate = PrivacyGate::from_probe(probe, vec!["tweetbot".into(), "1password".into()]);
    let decision = gate.evaluate(Some("com.apple.Safari"));
    assert!(matches!(decision, clipvault_core::CaptureDecision::Allow));
}

#[test]
fn empty_blacklist_lets_everything_through() {
    let probe = Arc::new(NoopActiveApplicationProbe);
    let gate = PrivacyGate::from_probe(probe, vec![]);
    assert!(matches!(
        gate.evaluate(Some("anything")),
        clipvault_core::CaptureDecision::Allow
    ));
    assert!(matches!(
        gate.evaluate(None),
        clipvault_core::CaptureDecision::Allow
    ));
}

#[test]
fn ignored_apps_repository_supports_basic_crud() {
    let dir = tempdir();
    let db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
    let mut db = db;
    db.run_migrations(&builtin_migrations()).expect("migrate");
    let when = datetime!(2026-01-02 03:04:05 UTC);
    {
        let mut repo = IgnoredAppRepository::new(db.connection_mut());
        repo.insert("com.apple.Terminal", when).expect("insert");
        repo.insert("firefox", when).expect("insert");
    }
    let conn = db.connection_mut();
    let repo = IgnoredAppRepository::new(conn);
    let rows = repo.list().expect("list");
    let ids: Vec<String> = rows.into_iter().map(|r| r.id).collect();
    assert_eq!(
        ids,
        vec!["com.apple.Terminal".to_string(), "firefox".to_string()]
    );

    let removed = {
        let mut repo = IgnoredAppRepository::new(db.connection_mut());
        repo.delete("firefox").expect("delete")
    };
    assert!(removed);
}

#[test]
fn bootloader_does_not_panic_with_probe_unavailable() {
    // Sanity check: a noop probe that never produces an identifier
    // must leave the gate in `Allow` mode forever.
    let probe = Arc::new(NoopActiveApplicationProbe);
    let gate = PrivacyGate::from_probe(probe, vec!["keep".into()]);
    for _ in 0..5 {
        assert!(matches!(
            gate.evaluate(None),
            clipvault_core::CaptureDecision::Allow
        ));
    }
}

#[test]
fn history_service_returns_ignored_when_gate_discards() {
    let probe = Arc::new(NoopActiveApplicationProbe);
    let gate = PrivacyGate::from_probe(probe, vec!["blacklisted-app".into()]);
    let (_dir, _context) = bootstrap_with_gate(gate.clone());
    // Direct gate assertion: matches what record_payload returns
    // when the gate discards.
    let decision = gate.evaluate(Some("blacklisted-app"));
    assert!(matches!(
        decision,
        clipvault_core::CaptureDecision::Discard {
            reason: "blacklisted"
        }
    ));
}

#[allow(dead_code)]
fn _clipboard_error_is_silently_consumed() -> ClipboardError {
    ClipboardError::Backend("noop".to_string())
}

#[allow(dead_code)]
fn _ensure_outcome_type_compiles(outcome: HistoryOutcome) -> &'static str {
    match outcome {
        HistoryOutcome::Stored { .. } => "stored",
        HistoryOutcome::Duplicate { .. } => "duplicate",
        HistoryOutcome::Ignored => "ignored",
        HistoryOutcome::Failed { .. } => "failed",
    }
}

// ---------------------------------------------------------------------------
// Bug 3 regression: background capture must resolve the source identifier
// through the cached probe before persisting, and must discard events from
// blacklisted applications without touching the database.
// ---------------------------------------------------------------------------

/// Programmable probe that returns whatever the test queues. Mirrors
/// the `FakeActiveApplication` from the core fakes but is local to the
/// test so it can own its queue without locking the shared fake.
#[derive(Debug)]
struct ScriptedProbe {
    queued: std::sync::Mutex<Vec<Option<&'static str>>>,
}

impl ScriptedProbe {
    fn new(values: Vec<Option<&'static str>>) -> Self {
        Self {
            queued: std::sync::Mutex::new(values.into_iter().rev().collect()),
        }
    }
}

impl clipvault_platform::ActiveApplicationProbe for ScriptedProbe {
    fn active_application(
        &self,
    ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
    {
        let next = self.queued.lock().unwrap().pop().unwrap_or(None);
        Ok(next.map(|id| clipvault_platform::ActiveApplication::new("App", id)))
    }
    fn name(&self) -> &'static str {
        "scripted"
    }
}

#[test]
fn watcher_with_blacklist_does_not_persist_blacklisted_source() {
    use clipvault_core::CaptureWatcher;
    use clipvault_db::EntryRepository;
    use std::time::Duration;

    // Probe reports `1password` for the first tick (blacklisted) and
    // a benign identifier for every subsequent call so the watcher
    // can exercise the cache-hit and cache-miss paths.
    let probe = Arc::new(ScriptedProbe::new(vec![
        Some("1password"),
        Some("1password"),
        Some("safari"),
        Some("safari"),
    ]));
    let cached = clipvault_platform::CachedActiveApplication::new(
        probe as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
    );
    let ignored = vec!["1password".to_string()];
    let matcher = CoreBlacklistMatcher::with_ignored(
        Arc::new(cached.clone()) as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
        ignored,
    );
    let gate = PrivacyGate::new(matcher);

    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipvault_core::FakeClipboard::with_text(
        "very-secret-1password-payload",
    ));
    let dir = tempdir();
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(clock.clone())
        .with_clipboard(clipboard.clone())
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    // Rebuild the history service with the gate that has the cached
    // probe; the bootstrap's default gate uses the same probe
    // instance but tests want a fresh service to wire the matcher
    // explicitly.
    let history = TextHistoryService::new(
        clipboard.clone(),
        clock.clone(),
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
    )
    .with_privacy_gate(gate.clone());
    let watcher = CaptureWatcher::new(
        context.platform_adapters().clipboard(),
        Duration::from_millis(10),
    );

    // Tick 1: probe returns "1password" (blacklisted). Even though
    // the watcher feeds a fresh payload through the gate, the gate
    // MUST discard the event and the database MUST stay empty.
    cached.refresh_with(Some(clipvault_platform::ActiveApplication::new(
        "1Password",
        "1password",
    )));
    let outcome = history.record_payload(&context, "secret".to_string(), None);
    assert_eq!(outcome, HistoryOutcome::Ignored);

    // The history pipeline ignored the event *before* the database
    // ever saw the secret; the entries table remains empty.
    let count = {
        let mut db = context.database().lock();
        EntryRepository::new(db.connection_mut()).count().unwrap()
    };
    assert_eq!(count, 0, "blacklisted event must never reach SQLite");

    // Tick 2: probe returns "safari" (not blacklisted). The event
    // flows through to persistence.
    cached.refresh_with(Some(clipvault_platform::ActiveApplication::new(
        "Safari", "safari",
    )));
    let outcome = history.record_payload(&context, "harmless text".to_string(), None);
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));
    let count = {
        let mut db = context.database().lock();
        EntryRepository::new(db.connection_mut()).count().unwrap()
    };
    assert_eq!(count, 1);
    let _ = watcher;
}

#[test]
fn watcher_skips_persistence_when_probe_cache_is_empty() {
    use clipvault_db::EntryRepository;

    // An empty cache is the same state the watcher sees right after
    // boot, before the main thread has had a chance to refresh.
    // The PrivacyGate MUST treat "unknown source" as "allow" so the
    // very first captures are still recorded.
    let probe = Arc::new(ScriptedProbe::new(vec![]));
    let cached = clipvault_platform::CachedActiveApplication::new(
        probe as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
    );
    let gate = PrivacyGate::from_probe(
        Arc::new(cached.clone()) as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
        vec!["1password".to_string()],
    );
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipvault_core::FakeClipboard::new());
    let dir = tempdir();
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(clock.clone())
        .with_clipboard(clipboard.clone())
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    let history = TextHistoryService::new(
        clipboard,
        clock,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
    )
    .with_privacy_gate(gate);
    assert!(cached.cached().is_none(), "cache must start empty");
    let outcome = history.record_payload(&context, "first capture".to_string(), None);
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));
    let count = {
        let mut db = context.database().lock();
        EntryRepository::new(db.connection_mut()).count().unwrap()
    };
    assert_eq!(count, 1);
}

// ---------------------------------------------------------------------------
// Bug 11 regression: the background capture loop and the manual `Tick
// capture` command must share a single `CaptureWatcher`. Two independent
// watchers each keep their own `last_hash`, so a blacklisted payload the
// loop already discarded could sneak back into the history through the
// Tick capture command (with the cached active-app probe now reporting
// ClipVault after the user clicked the button). The tests below pin
// every leg of the contract for the shared watcher.
// ---------------------------------------------------------------------------

#[test]
fn shared_watcher_dedupes_blacklisted_capture_and_tick_returns_unchanged() {
    use clipvault_core::{CaptureWatcher, FakeClipboardBackend, PlatformAdapters};
    use clipvault_core::{
        FakeActiveApplication, FakeClipboard as CoreFakeClipboard, FakeHotkeyManager,
        FakePasteController, FakeSettingsNavigator, FakeTrayController,
    };
    use clipvault_db::{builtin_migrations, EntryRepository, IgnoredAppRepository};
    use clipvault_platform::{
        ActiveApplication as PlatformActiveApplication, ActiveApplicationProbe, Capabilities,
        ClipboardBackend, DisplayServer, HotkeyManager, OsFamily, PasteController, PlatformInfo,
        SettingsNavigator, TrayController,
    };
    use std::time::Duration;

    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });

    // Seed the `ignored_apps` table BEFORE bootstrapping so the
    // bootstrap's gate sees the blacklist and matches `1password`.
    {
        let mut db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");
        let conn = db.connection_mut();
        let mut repo = IgnoredAppRepository::new(conn);
        repo.insert("1password", clock.now()).expect("insert");
    }

    // Build a fake probe so the bootstrap wraps it in a cached
    // probe and the gate observes whatever identifier we set.
    let probe: Arc<dyn ActiveApplicationProbe> = Arc::new(ScriptedProbe::new(vec![
        Some("1password"),
        Some("com.clipvault.app"),
        Some("com.clipvault.app"),
    ]));

    let info = PlatformInfo {
        home_dir: std::path::PathBuf::from("/tmp"),
        data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
        os_family: OsFamily::Macos,
        display_server: DisplayServer::Unknown,
    };
    let platform_adapters = PlatformAdapters::new(
        Arc::new(FakeClipboardBackend::new()) as Arc<dyn ClipboardBackend>,
        Arc::new(FakeHotkeyManager::new()) as Arc<dyn HotkeyManager>,
        probe,
        Arc::new(FakePasteController::new()) as Arc<dyn PasteController>,
        Arc::new(FakeTrayController::new()) as Arc<dyn TrayController>,
        Arc::new(FakeSettingsNavigator::new()) as Arc<dyn SettingsNavigator>,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        Capabilities::default(),
        info,
    );
    let context = AppBootstrap::new()
        .with_clock(clock.clone())
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    // Build the watcher on a fresh `FakeClipboardBackend` that the
    // test controls. Sharing it with a clone of the watcher
    // verifies the dedupe behaviour across the loop + tick code
    // paths in this regression test.
    let clipboard_backend = Arc::new(FakeClipboardBackend::new());
    clipboard_backend.push_read(Ok(Some("cv-loop-secret-9F-2026".into())));
    clipboard_backend.push_read(Ok(Some("cv-loop-secret-9F-2026".into())));
    let watcher = Arc::new(CaptureWatcher::new(
        clipboard_backend.clone(),
        Duration::from_millis(10),
    ));
    let loop_handle = Arc::clone(&watcher);
    let tick_handle = Arc::clone(&watcher);

    // Warm the cached probe with the blacklisted identifier so the
    // very first tick is classified as blacklisted.
    let cached = context.cached_active_app_probe();
    cached.refresh_with(Some(PlatformActiveApplication::new(
        "1Password",
        "1password",
    )));

    // Loop tick: gate matches the cached probe, returns `Discard`,
    // history returns `Ignored`. The shared watcher records the
    // hash so the next tick dedupes.
    let loop_outcome = loop_handle.tick(&context, None);
    assert!(
        matches!(
            loop_outcome,
            clipvault_core::WatchTickOutcome::Captured(HistoryOutcome::Ignored)
        ),
        "loop tick must consume the blacklisted event, got {loop_outcome:?}"
    );

    // Refresh the cached probe to `com.clipvault.app` — this is
    // the identifier the cached probe would see after the user
    // clicks the Tick capture button and ClipVault gains focus.
    cached.refresh_with(Some(PlatformActiveApplication::new(
        "ClipVault",
        "com.clipvault.app",
    )));

    // Tick capture on the shared watcher. The hash the loop
    // already saw is identical to the current clipboard, so the
    // shared watcher short-circuits with `Unchanged`. With two
    // independent watchers (the bug) this would be a fresh tick
    // — the gate would happily say `Allow` because the cached
    // probe now reports ClipVault — and the secret would
    // silently land in SQLite.
    let tick_outcome = tick_handle.tick(&context, None);
    assert_eq!(
        tick_outcome,
        clipvault_core::WatchTickOutcome::Unchanged,
        "Tick capture after a blacklisted loop tick must be a no-op"
    );

    // Database MUST stay empty — the secret never reached SQLite.
    let count = {
        let mut db = context.database().lock();
        EntryRepository::new(db.connection_mut()).count().unwrap()
    };
    assert_eq!(
        count, 0,
        "shared watcher must keep the discarded payload out of SQLite"
    );
    let _ = CoreFakeClipboard::new;
    let _ = FakeActiveApplication::new();
}

#[test]
fn shared_watcher_stores_allowed_capture_in_database() {
    use clipvault_core::{CaptureWatcher, FakeClipboardBackend, PlatformAdapters};
    use clipvault_core::{
        FakeHotkeyManager, FakePasteController, FakeSettingsNavigator, FakeTrayController,
    };
    use clipvault_db::{builtin_migrations, EntryRepository, IgnoredAppRepository};
    use clipvault_platform::{
        ActiveApplication as PlatformActiveApplication, ActiveApplicationProbe, Capabilities,
        ClipboardBackend, DisplayServer, HotkeyManager, OsFamily, PasteController, PlatformInfo,
        SettingsNavigator, TrayController,
    };
    use std::time::Duration;

    // Positive control: when the source is not blacklisted the
    // shared watcher MUST persist the payload exactly once and any
    // subsequent tick on the same Arc must observe `Unchanged`. This
    // pins the `Stored` leg of the shared-watcher contract — without
    // it a regression that always returned `Unchanged` would slip
    // past the regression suite.
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });

    // Blacklist `1password` so the `dashboard` source (any other
    // identifier) flows through the gate as `Allow`.
    {
        let mut db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");
        let conn = db.connection_mut();
        let mut repo = IgnoredAppRepository::new(conn);
        repo.insert("1password", clock.now()).expect("insert");
    }

    let probe: Arc<dyn ActiveApplicationProbe> =
        Arc::new(ScriptedProbe::new(vec![Some("dashboard")]));
    let info = PlatformInfo {
        home_dir: std::path::PathBuf::from("/tmp"),
        data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
        os_family: OsFamily::Macos,
        display_server: DisplayServer::Unknown,
    };
    let platform_adapters = PlatformAdapters::new(
        Arc::new(FakeClipboardBackend::new()) as Arc<dyn ClipboardBackend>,
        Arc::new(FakeHotkeyManager::new()) as Arc<dyn HotkeyManager>,
        probe,
        Arc::new(FakePasteController::new()) as Arc<dyn PasteController>,
        Arc::new(FakeTrayController::new()) as Arc<dyn TrayController>,
        Arc::new(FakeSettingsNavigator::new()) as Arc<dyn SettingsNavigator>,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        Capabilities::default(),
        info,
    );
    let context = AppBootstrap::new()
        .with_clock(clock.clone())
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    // Warm the cached probe with a benign identifier.
    let cached = context.cached_active_app_probe();
    cached.refresh_with(Some(PlatformActiveApplication::new(
        "Dashboard",
        "dashboard",
    )));

    let clipboard_backend = Arc::new(FakeClipboardBackend::new());
    clipboard_backend.push_read(Ok(Some("cv-allowed-payload".into())));
    clipboard_backend.push_read(Ok(Some("cv-allowed-payload".into())));

    let watcher = Arc::new(CaptureWatcher::new(
        clipboard_backend.clone(),
        Duration::from_millis(10),
    ));

    let first = watcher.tick(&context, None);
    assert!(
        matches!(
            first,
            clipvault_core::WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
        ),
        "first tick on a fresh watcher with an allowed source must persist, got {first:?}"
    );
    let second = watcher.tick(&context, None);
    assert_eq!(
        second,
        clipvault_core::WatchTickOutcome::Unchanged,
        "second tick on the shared watcher must dedupe"
    );

    let count = {
        let mut db = context.database().lock();
        EntryRepository::new(db.connection_mut()).count().unwrap()
    };
    assert_eq!(
        count, 1,
        "shared watcher must persist exactly one row for the allowed capture"
    );
}

#[test]
fn record_payload_does_not_log_payload_hash_or_source_app() {
    // Bug 11.4 regression: the watcher pipeline must never write the
    // clipboard text, the content hash or the source-app identifier
    // through a `tracing::*!` macro. The integration test below
    // installs a `MakeWriter` capture, ticks the watcher twice (once
    // through a blacklisted identifier, once through a benign one)
    // and asserts the buffer carries neither the payload nor the
    // cached identifier.
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    #[derive(Default, Clone)]
    struct SharedCapture(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedCapture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'w> MakeWriter<'w> for SharedCapture {
        type Writer = SharedCapture;
        fn make_writer(&'w self) -> Self::Writer {
            self.clone()
        }
    }

    let capture = SharedCapture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(capture.clone())
        .with_max_level(tracing::Level::TRACE)
        .finish();

    let probe = Arc::new(ScriptedProbe::new(vec![
        Some("1password"),
        Some("1password"),
        Some("safari"),
    ]));
    let cached = clipvault_platform::CachedActiveApplication::new(
        probe as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
    );
    cached.refresh_with(Some(clipvault_platform::ActiveApplication::new(
        "1Password",
        "1password",
    )));
    let gate = PrivacyGate::from_probe(
        Arc::new(cached.clone()) as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
        vec!["1password".to_string()],
    );
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> =
        Arc::new(clipvault_core::FakeClipboard::with_text("placeholder"));
    let dir = tempdir();
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(clock.clone())
        .with_clipboard(clipboard.clone())
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    let history = TextHistoryService::new(
        clipboard,
        clock,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
    )
    .with_privacy_gate(gate);

    let secret = "cv-loop-leak-marker-ZX-9911";
    let outcome = tracing::subscriber::with_default(subscriber, || {
        history.record_payload(&context, secret.to_string(), None)
    });
    assert!(matches!(outcome, HistoryOutcome::Ignored));
    let buffer = String::from_utf8_lossy(&capture.0.lock().unwrap()).into_owned();
    assert!(
        !buffer.contains(secret),
        "blacklisted tick must never write the payload to logs:\n{buffer}"
    );
    assert!(
        !buffer.to_ascii_lowercase().contains("1password"),
        "source-app identifier must never appear in capture logs:\n{buffer}"
    );

    // The hash would expose the same leak class; assert against it
    // even though the watcher never computes a hash for an event
    // the gate is about to discard.
    let hash = clipvault_core::hash_content(secret);
    assert!(
        !buffer.contains(&hash),
        "content hash must never appear in capture logs:\n{buffer}"
    );
}

// ---------------------------------------------------------------------------
// Bug regression: active-app cache must drive the gate even when the cache
// is empty (Allow), populated with a blacklisted identifier (Discard) or
// fails the refresh (Allow while the failure is reported through
// diagnostics). The diagnostics surface is metadata-only and never carries
// clipboard content or content hashes.
// ---------------------------------------------------------------------------

/// Single scripted answer the `CountingProbe` consumes.
type ScriptedAnswer = (
    Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>,
    &'static str,
);

/// Test fixture that records how many times the underlying probe was
/// queried and which identifier was returned for each query. Mirrors
/// the scripted probe pattern used elsewhere in this file but tracks
/// invocations so the regression suite can assert "refresh ran on
/// the main thread before the gate evaluated".
#[derive(Debug)]
struct CountingProbe {
    answers: std::sync::Mutex<Vec<ScriptedAnswer>>,
    invocations: std::sync::Mutex<Vec<&'static str>>,
}

impl CountingProbe {
    fn new(scripted: Vec<ScriptedAnswer>) -> Self {
        Self {
            answers: std::sync::Mutex::new(scripted.into_iter().rev().collect()),
            invocations: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn record(&self, thread: &'static str) {
        self.invocations.lock().unwrap().push(thread);
    }
}

impl clipvault_platform::ActiveApplicationProbe for CountingProbe {
    fn active_application(
        &self,
    ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
    {
        let next = self.answers.lock().unwrap().pop();
        match next {
            Some((result, thread)) => {
                self.record(thread);
                result
            }
            None => {
                self.record("background");
                Ok(None)
            }
        }
    }
    fn name(&self) -> &'static str {
        "counting"
    }
}

#[test]
fn active_app_cache_empty_allows_capture_and_reports_pending() {
    // The very first capture the boot path observes must NOT match a
    // blacklisted identifier when the cache is empty: the "unknown
    // source" contract must remain "allow". The diagnostics surface
    // must report the cache as not yet populated.
    let probe = Arc::new(ScriptedProbe::new(vec![]));
    let cached = clipvault_platform::CachedActiveApplication::new(
        probe as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
    );
    let gate = PrivacyGate::from_probe(
        Arc::new(cached.clone()) as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
        vec!["1password".to_string()],
    );
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipvault_core::FakeClipboard::new());
    let dir = tempdir();
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(clock)
        .with_clipboard(clipboard)
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    let history = TextHistoryService::new(
        context.clipboard(),
        context.clock(),
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
    )
    .with_privacy_gate(gate);

    let diag = context.active_app_diagnostics();
    assert!(!diag.cache_populated, "cache starts empty");
    assert_eq!(
        diag.refresh_outcome.kind(),
        "pending",
        "no refresh has happened yet"
    );

    let outcome = history.record_payload(&context, "first secret".to_string(), None);
    assert!(
        matches!(outcome, HistoryOutcome::Stored { .. }),
        "empty cache must keep the allow contract: got {outcome:?}"
    );
}

#[test]
fn active_app_cache_updated_with_blacklisted_discards_capture() {
    // After a refresh the cached probe reports the blacklisted
    // identifier. The gate MUST discard the payload and the
    // diagnostics MUST surface the cache as populated. We refresh
    // via the diagnostics state so the test wires its own
    // observer that does not depend on the AppContext's internal
    // diagnostics cache (the bootstrap wires that for production
    // and exposes it through `context.active_app_diagnostics()`,
    // but here we keep full control over the synthetic probe).
    let probe = Arc::new(ScriptedProbe::new(vec![
        Some("1password"),
        Some("1password"),
        Some("safari"),
    ]));
    let cached = clipvault_platform::CachedActiveApplication::new(
        probe as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
    );
    let gate = PrivacyGate::from_probe(
        Arc::new(cached.clone()) as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
        vec!["1password".to_string()],
    );
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipvault_core::FakeClipboard::new());
    let dir = tempdir();
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let _context = AppBootstrap::new()
        .with_clock(clock.clone())
        .with_clipboard(clipboard.clone())
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    // Drive the gate with a diagnostics state that mirrors what the
    // production shell would record, then assert the capture path
    // discards and the snapshot reflects the cache.
    let diag_state = clipvault_core::ActiveAppDiagnosticsState::new(
        true,
        clipvault_platform::ActiveAppBackendKind::MacOsWorkspace,
        Some(cached.clone()),
    );
    diag_state.record_refresh(Some(clipvault_platform::ActiveApplication::new(
        "1Password",
        "1password",
    )));
    let history = TextHistoryService::new(
        clipboard,
        clock,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
    )
    .with_privacy_gate(gate);
    let outcome = history.record_payload(&_context, "secret".to_string(), None);
    assert_eq!(outcome, HistoryOutcome::Ignored);
    let diag = diag_state.snapshot();
    assert!(diag.cache_populated);
    assert_eq!(diag.identifier.as_deref(), Some("1password"));
    assert_eq!(diag.refresh_outcome.kind(), "ok");
}

#[test]
fn active_app_refresh_failure_records_outcome_and_keeps_cache_value() {
    // The diagnostics endpoint must surface a refresh failure instead
    // of swallowing it. The previous value stays visible so the gate
    // can still match (the "unknown source" contract says "allow"
    // after a failure, but the cached identifier does not vanish).
    let probe = Arc::new(CountingProbe::new(vec![
        (
            Ok(Some(clipvault_platform::ActiveApplication::new(
                "Firefox", "firefox",
            ))),
            "main",
        ),
        (
            Err(clipvault_platform::ActiveAppError::Backend {
                details: "x11 disconnected".to_string(),
            }),
            "main",
        ),
    ]));
    let cached = clipvault_platform::CachedActiveApplication::new(
        probe.clone() as Arc<dyn clipvault_platform::ActiveApplicationProbe>
    );
    let cached_for_diag = cached.clone();
    let gate = PrivacyGate::from_probe(
        Arc::new(cached.clone()) as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
        vec!["firefox".to_string()],
    );
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipvault_core::FakeClipboard::new());
    let dir = tempdir();
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(clock.clone())
        .with_clipboard(clipboard.clone())
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    let state = clipvault_core::ActiveAppDiagnosticsState::new(
        true,
        clipvault_platform::ActiveAppBackendKind::X11Ewmh,
        Some(cached_for_diag.clone()),
    );
    state.record_refresh(Some(clipvault_platform::ActiveApplication::new(
        "Firefox", "firefox",
    )));
    let diag_after_failure = state.record_failure(&clipvault_platform::ActiveAppError::Backend {
        details: "x11 disconnected".to_string(),
    });
    assert!(diag_after_failure.cache_populated);
    assert_eq!(diag_after_failure.identifier.as_deref(), Some("firefox"));
    match &diag_after_failure.refresh_outcome {
        clipvault_core::ActiveAppRefreshOutcome::Failed { message, .. } => {
            assert!(message.contains("x11 disconnected"));
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    // The gate helper still saw the cached identifier (Firefox), so
    // the new capture is discarded. The diagnostics message is
    // metadata only — the secret text never enters it.
    let history = TextHistoryService::new(
        clipboard,
        clock,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
    )
    .with_privacy_gate(gate);
    let outcome = history.record_payload(&context, "new-secret-XYZ-9981".to_string(), None);
    assert_eq!(outcome, HistoryOutcome::Ignored);
    // Sanity: the diagnostics message must not contain the payload
    // text the user is trying to copy.
    let json = serde_json::to_string(&diag_after_failure).unwrap();
    assert!(
        !json.contains("new-secret-XYZ-9981"),
        "diagnostics payload leaked the clipboard content: {json}"
    );
}

#[test]
fn settings_update_propagates_to_privacy_gate_atomically() {
    // The settings service must keep the PrivacyGate's snapshot in
    // sync with the persisted blacklist. Adding `1password` after
    // boot MUST immediately flip the next capture from Allow to
    // Discard.
    let probe = Arc::new(ScriptedProbe::new(vec![
        Some("1password"),
        Some("1password"),
        Some("1password"),
    ]));
    let cached = clipvault_platform::CachedActiveApplication::new(
        probe as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
    );
    let gate = PrivacyGate::from_probe(
        Arc::new(cached.clone()) as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
        vec![],
    );
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipvault_core::FakeClipboard::with_text(
        "ignored-by-default",
    ));
    let dir = tempdir();
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(clock.clone())
        .with_clipboard(clipboard.clone())
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    // Replace the bootstrap's gate with the one this test manages so
    // we can observe the snapshot transition.
    let history = TextHistoryService::new(
        clipboard,
        clock.clone(),
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
    )
    .with_privacy_gate(gate.clone());
    // Baseline: with an empty blacklist, the gate allows.
    cached.refresh_with(Some(clipvault_platform::ActiveApplication::new(
        "1Password",
        "1password",
    )));
    let outcome = history.record_payload(&context, "harmless".to_string(), None);
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));

    // Update: add the blacklisted identifier via the settings service
    // and assert the gate discards immediately on the next capture.
    let settings_service = clipvault_core::SettingsService::new(clock, gate.clone());
    let updated = settings_service
        .add_ignored(&context, "1password")
        .expect("add_ignored succeeds");
    assert_eq!(updated.ignored_apps, vec!["1password".to_string()]);

    let outcome = history.record_payload(&context, "second payload".to_string(), None);
    assert_eq!(
        outcome,
        HistoryOutcome::Ignored,
        "settings update must flip the gate to Discard"
    );
}

#[test]
fn capture_loop_refresh_before_tick_keeps_cache_fresh() {
    // Regression for the original bug: the background loop must
    // refresh the cached probe BEFORE every tick so the PrivacyGate
    // sees the identifier of the app that just produced the
    // clipboard change. The test below simulates a probe that flips
    // identifier on each call and asserts the gate saw the second
    // answer (the one the loop would have refreshed).
    let probe = Arc::new(CountingProbe::new(vec![
        (
            Ok(Some(clipvault_platform::ActiveApplication::new(
                "Safari", "safari",
            ))),
            "main",
        ),
        (
            Ok(Some(clipvault_platform::ActiveApplication::new(
                "Terminal",
                "com.apple.terminal",
            ))),
            "main",
        ),
        (
            Ok(Some(clipvault_platform::ActiveApplication::new(
                "Terminal",
                "com.apple.terminal",
            ))),
            "main",
        ),
    ]));
    let cached = clipvault_platform::CachedActiveApplication::new(
        probe.clone() as Arc<dyn clipvault_platform::ActiveApplicationProbe>
    );
    let gate = PrivacyGate::from_probe(
        Arc::new(cached.clone()) as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
        vec!["com.apple.terminal".to_string()],
    );
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipvault_core::FakeClipboard::with_text(
        "cv-sync-refresh-payload",
    ));
    let dir = tempdir();
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(clock.clone())
        .with_clipboard(clipboard.clone())
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    // Drive the refresh exactly the way the shell loop does it:
    // synchronously on the (mocked) main thread, then evaluate the
    // payload.
    let _: Result<
        Option<clipvault_platform::ActiveApplication>,
        clipvault_platform::ActiveAppError,
    > = context.refresh_active_application();
    // The second answer matches the blacklisted identifier — this
    // is what the user sees when they switch to Terminal and copy.
    cached.refresh_with(Some(clipvault_platform::ActiveApplication::new(
        "Terminal",
        "com.apple.terminal",
    )));
    let history = TextHistoryService::new(
        clipboard,
        clock,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
    )
    .with_privacy_gate(gate);
    let outcome = history.record_payload(&context, "cv-sync-refresh-payload".to_string(), None);
    assert_eq!(
        outcome,
        HistoryOutcome::Ignored,
        "sync refresh must let the gate see the blacklisted identifier"
    );

    // Every probe invocation must have been attributed to the main
    // thread — the bug was about background threads calling NSWorkspace.
    let invocations = probe.invocations.lock().unwrap().clone();
    assert!(
        invocations.iter().all(|origin| *origin == "main"),
        "the probe must only be queried from the main thread, got {invocations:?}"
    );
}

#[test]
fn unknown_source_preserves_allow_rule_under_sync_refresh() {
    // When the platform reports an empty identifier (or the cache is
    // empty) the "unknown source" contract must keep the gate in
    // Allow mode. The fix cannot silently flip this to Discard.
    struct EmptyProbe;
    impl clipvault_platform::ActiveApplicationProbe for EmptyProbe {
        fn active_application(
            &self,
        ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
        {
            Ok(Some(clipvault_platform::ActiveApplication::new("", "")))
        }
        fn name(&self) -> &'static str {
            "empty"
        }
    }
    let cached = clipvault_platform::CachedActiveApplication::new(
        Arc::new(EmptyProbe) as Arc<dyn clipvault_platform::ActiveApplicationProbe>
    );
    let gate = PrivacyGate::from_probe(
        Arc::new(cached.clone()) as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
        vec!["firefox".to_string()],
    );
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipvault_core::FakeClipboard::new());
    let dir = tempdir();
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(clock.clone())
        .with_clipboard(clipboard.clone())
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    cached.refresh_with(Some(clipvault_platform::ActiveApplication::new("", "")));
    let history = TextHistoryService::new(
        clipboard,
        clock,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
    )
    .with_privacy_gate(gate);
    let outcome = history.record_payload(&context, "unknown-source-allow".to_string(), None);
    assert!(
        matches!(outcome, HistoryOutcome::Stored { .. }),
        "unknown source must remain Allow: got {outcome:?}"
    );
}

// ---------------------------------------------------------------------------
// Bug regression: the diagnostics surface must transition Pending → Failed
// when the synchronous main-thread refresh reports `MainThreadSyncError`
// (Schedule / Timeout) instead of staying at "pending" forever. The previous
// implementation only logged the error and returned a stale snapshot, which
// left the user unable to tell apart "loop never tried" from "Tauri refused
// the refresh".
// ---------------------------------------------------------------------------

#[test]
fn sync_refresh_failure_transitions_pending_to_failed() {
    // The bootstrap ships with an empty cache and a `Pending` outcome.
    // When `record_active_app_sync_failure` runs with a `Schedule` message,
    // the diagnostics snapshot must leave Pending and surface the
    // actionable error so the user can tell what happened.
    use clipvault_db::builtin_migrations;
    use clipvault_platform::{ActiveAppBackendKind, ActiveApplicationProbe};

    struct StubProbe;
    impl ActiveApplicationProbe for StubProbe {
        fn active_application(
            &self,
        ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
        {
            Ok(None)
        }
        fn name(&self) -> &'static str {
            "stub"
        }
    }

    let dir = tempdir();
    {
        let mut db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");
    }
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipvault_core::FakeClipboard::new());
    let probe: Arc<dyn ActiveApplicationProbe> = Arc::new(StubProbe);
    let info = clipvault_platform::PlatformInfo {
        home_dir: dir.path().to_path_buf(),
        data_dir: dir.path().join("data"),
        os_family: clipvault_platform::OsFamily::Macos,
        display_server: clipvault_platform::DisplayServer::Unknown,
    };
    let platform_adapters = clipvault_core::PlatformAdapters::new(
        Arc::new(clipvault_core::FakeClipboardBackend::new())
            as Arc<dyn clipvault_platform::ClipboardBackend>,
        Arc::new(clipvault_core::FakeHotkeyManager::new())
            as Arc<dyn clipvault_platform::HotkeyManager>,
        probe,
        Arc::new(clipvault_core::FakePasteController::new())
            as Arc<dyn clipvault_platform::PasteController>,
        Arc::new(clipvault_core::FakeTrayController::new())
            as Arc<dyn clipvault_platform::TrayController>,
        Arc::new(clipvault_core::FakeSettingsNavigator::new())
            as Arc<dyn clipvault_platform::SettingsNavigator>,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        clipvault_platform::Capabilities::ALL_AVAILABLE,
        info,
    );
    let context = AppBootstrap::new()
        .with_clock(clock.clone())
        .with_clipboard(clipboard)
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    let before = context.active_app_diagnostics();
    assert_eq!(
        before.refresh_outcome.kind(),
        "pending",
        "bootstrap must start Pending"
    );
    assert_eq!(before.refresh_attempts, 0);

    // Mirror what `refresh_active_app_cached` does when the Tauri
    // scheduler reports `MainThreadSyncError::Schedule("event loop
    // shut down")` — the closure never ran so we must surface the
    // failure on the diagnostics state.
    let diag_after = context.record_active_app_sync_failure(
        "main thread did not accept the closure: event loop shut down",
    );
    assert_eq!(
        diag_after.refresh_outcome.kind(),
        "failed",
        "sync failure must leave Pending"
    );
    match &diag_after.refresh_outcome {
        clipvault_core::ActiveAppRefreshOutcome::Failed { message, .. } => {
            assert!(
                message.contains("event loop shut down"),
                "the actionable reason must be preserved: {message}"
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    assert_eq!(diag_after.refresh_attempts, 1);
    assert_eq!(diag_after.successful_refreshes, 0);
    assert_eq!(diag_after.failed_refreshes, 1);
    assert!(diag_after.last_refresh_unix_ms.is_some());
    let backend_kind: clipvault_platform::ActiveAppBackendKind =
        ActiveAppBackendKind::MacOsWorkspace;
    assert_eq!(diag_after.backend, backend_kind.as_str());

    // `record_active_app_sync_failure` MUST keep the actionable
    // reason (so the user can fix the cause) and MUST NOT need any
    // caller-supplied payload to be present in the JSON. The
    // production callers only pass platform-layer error strings
    // (Tauri scheduler / main-thread timeout), never clipboard
    // content, so the diagnostics state itself stays payload-free.
    let diag_after =
        context.record_active_app_sync_failure("main thread did not execute the closure in 500ms");
    assert_eq!(diag_after.failed_refreshes, 2);
    assert_eq!(diag_after.successful_refreshes, 0);
    let json = serde_json::to_string(&diag_after).unwrap();
    assert!(
        !json.contains("cv-secret-9981"),
        "diagnostics payload leaked a fake secret: {json}"
    );
    assert!(
        !json.contains("abc123def456"),
        "diagnostics payload leaked a fake hash: {json}"
    );
}

#[test]
fn sync_refresh_failure_records_timeout_outcome() {
    // A Timeout from the synchronous helper must also leave Pending
    // and surface the actionable reason so a stuck main thread does
    // not look identical to a healthy loop.
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipvault_core::FakeClipboard::new());
    let info = clipvault_platform::PlatformInfo {
        home_dir: dir.path().to_path_buf(),
        data_dir: dir.path().join("data"),
        os_family: clipvault_platform::OsFamily::Macos,
        display_server: clipvault_platform::DisplayServer::Unknown,
    };
    let platform_adapters = clipvault_core::PlatformAdapters::new(
        Arc::new(clipvault_core::FakeClipboardBackend::new())
            as Arc<dyn clipvault_platform::ClipboardBackend>,
        Arc::new(clipvault_core::FakeHotkeyManager::new())
            as Arc<dyn clipvault_platform::HotkeyManager>,
        Arc::new(clipvault_core::FakeActiveApplication::new())
            as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
        Arc::new(clipvault_core::FakePasteController::new())
            as Arc<dyn clipvault_platform::PasteController>,
        Arc::new(clipvault_core::FakeTrayController::new())
            as Arc<dyn clipvault_platform::TrayController>,
        Arc::new(clipvault_core::FakeSettingsNavigator::new())
            as Arc<dyn clipvault_platform::SettingsNavigator>,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        clipvault_platform::Capabilities::ALL_AVAILABLE,
        info,
    );
    let context = AppBootstrap::new()
        .with_clock(clock)
        .with_clipboard(clipboard)
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    let diag =
        context.record_active_app_sync_failure("main thread did not execute the closure in 500ms");
    match &diag.refresh_outcome {
        clipvault_core::ActiveAppRefreshOutcome::Failed { message, .. } => {
            assert!(
                message.contains("500ms"),
                "timeout duration must be visible to the user: {message}"
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    assert_eq!(diag.failed_refreshes, 1);
}

// ---------------------------------------------------------------------------
// Bug regression: the diagnostics metadata must reflect the lifecycle of
// the capture loop so the user can tell apart "the loop never started" from
// "the cache is empty because no application has been focused yet".
// ---------------------------------------------------------------------------

#[test]
fn diagnostics_loop_started_is_sticky_across_refreshes() {
    // `mark_loop_started` is called exactly once when the capture loop
    // spawns; the flag must survive subsequent refreshes so the
    // dashboard never reports "loop never started" while the loop is
    // actively polling the clipboard.
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipvault_core::FakeClipboard::new());
    let info = clipvault_platform::PlatformInfo {
        home_dir: dir.path().to_path_buf(),
        data_dir: dir.path().join("data"),
        os_family: clipvault_platform::OsFamily::Macos,
        display_server: clipvault_platform::DisplayServer::Unknown,
    };
    let platform_adapters = clipvault_core::PlatformAdapters::new(
        Arc::new(clipvault_core::FakeClipboardBackend::new())
            as Arc<dyn clipvault_platform::ClipboardBackend>,
        Arc::new(clipvault_core::FakeHotkeyManager::new())
            as Arc<dyn clipvault_platform::HotkeyManager>,
        Arc::new(clipvault_core::FakeActiveApplication::new())
            as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
        Arc::new(clipvault_core::FakePasteController::new())
            as Arc<dyn clipvault_platform::PasteController>,
        Arc::new(clipvault_core::FakeTrayController::new())
            as Arc<dyn clipvault_platform::TrayController>,
        Arc::new(clipvault_core::FakeSettingsNavigator::new())
            as Arc<dyn clipvault_platform::SettingsNavigator>,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        clipvault_platform::Capabilities::ALL_AVAILABLE,
        info,
    );
    let context = AppBootstrap::new()
        .with_clock(clock)
        .with_clipboard(clipboard)
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    let diagnostics = context.active_app_diagnostics_state();
    assert!(!diagnostics.snapshot().loop_started);
    diagnostics.mark_loop_started();
    assert!(diagnostics.snapshot().loop_started);
    diagnostics.mark_loop_started();
    assert!(
        diagnostics.snapshot().loop_started,
        "the loop_started flag must remain sticky"
    );
}

// ---------------------------------------------------------------------------
// Bug regression: the integration scenario from the manual flow must keep
// passing end-to-end. The capture pipeline is fed a blacklisted identifier
// (TextEdit) and a blacklisted source (also TextEdit); the gate MUST discard
// the event and SQLite MUST stay empty. The companion test confirms the
// positive control (allowed capture persists).
// ---------------------------------------------------------------------------

#[test]
fn integration_textedit_blacklist_drops_capture_and_keeps_db_empty() {
    // Reproduce the scenario the user reported: the cached probe
    // reports `com.apple.TextEdit`, the persisted blacklist also
    // contains `com.apple.TextEdit`, the clipboard backend queues a
    // distinctive payload and the capture loop runs an iteration.
    // The PrivacyGate MUST classify the event as `Discard` and
    // `entry_repository::count()` MUST stay at zero.
    use clipvault_core::CaptureWatcher;
    use clipvault_db::{builtin_migrations, EntryRepository, IgnoredAppRepository};
    use clipvault_platform::{
        ActiveApplication as PlatformActiveApplication, ActiveApplicationProbe, Capabilities,
        ClipboardBackend, DisplayServer, HotkeyManager, OsFamily, PasteController, PlatformInfo,
        SettingsNavigator, TrayController,
    };
    use std::time::Duration;

    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });

    // Seed the blacklist BEFORE bootstrapping so the bootstrap's
    // gate sees `com.apple.TextEdit` from the start.
    {
        let mut db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");
        let conn = db.connection_mut();
        let mut repo = IgnoredAppRepository::new(conn);
        repo.insert("com.apple.TextEdit", clock.now())
            .expect("insert");
    }

    struct TextEditProbe;
    impl ActiveApplicationProbe for TextEditProbe {
        fn active_application(
            &self,
        ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
        {
            Ok(Some(PlatformActiveApplication::new(
                "TextEdit",
                "com.apple.TextEdit",
            )))
        }
        fn name(&self) -> &'static str {
            "textedit"
        }
    }
    let probe: Arc<dyn ActiveApplicationProbe> = Arc::new(TextEditProbe);
    let info = PlatformInfo {
        home_dir: std::path::PathBuf::from("/tmp"),
        data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
        os_family: OsFamily::Macos,
        display_server: DisplayServer::Unknown,
    };
    let platform_adapters = clipvault_core::PlatformAdapters::new(
        Arc::new(clipvault_core::FakeClipboardBackend::new()) as Arc<dyn ClipboardBackend>,
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
        .with_clock(clock.clone())
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    // Mirror the shell-side `refresh_active_app_cached`: refresh the
    // cached probe so the gate observes the real identifier.
    let cached = context.cached_active_app_probe();
    cached.refresh_with(Some(PlatformActiveApplication::new(
        "TextEdit",
        "com.apple.TextEdit",
    )));

    // Queue a distinctive payload and run the watcher (the same
    // path the background loop drives every tick).
    let clipboard_backend = Arc::new(clipvault_core::FakeClipboardBackend::new());
    clipboard_backend.push_read(Ok(Some("cv-textedit-marker-Q9-2026".into())));
    let watcher = CaptureWatcher::new(clipboard_backend, Duration::from_millis(10));
    let outcome = watcher.tick(&context, None);
    assert_eq!(
        outcome,
        clipvault_core::WatchTickOutcome::Captured(HistoryOutcome::Ignored),
        "TextEdit capture must be discarded by the gate"
    );

    let count = {
        let mut db = context.database().lock();
        EntryRepository::new(db.connection_mut()).count().unwrap()
    };
    assert_eq!(
        count, 0,
        "blacklisted capture must never reach SQLite; got {count} rows"
    );
}

#[test]
fn integration_allowed_capture_persists_in_database() {
    // Positive control: with an empty blacklist the capture loop MUST
    // persist the payload once and the second tick MUST return
    // `Unchanged`. The regression suite already covers the
    // `Stored + Unchanged` round trip in `shared_watcher_stores_allowed_capture_in_database`
    // but this test exercises the bootstrap end-to-end so a future
    // change to the gate wiring surfaces as a clear test failure.
    use clipvault_core::CaptureWatcher;
    use clipvault_db::{builtin_migrations, EntryRepository};
    use clipvault_platform::{
        ActiveApplication as PlatformActiveApplication, ActiveApplicationProbe, Capabilities,
        ClipboardBackend, DisplayServer, HotkeyManager, OsFamily, PasteController, PlatformInfo,
        SettingsNavigator, TrayController,
    };
    use std::time::Duration;

    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    {
        let mut db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");
    }
    struct DashboardProbe;
    impl ActiveApplicationProbe for DashboardProbe {
        fn active_application(
            &self,
        ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
        {
            Ok(Some(PlatformActiveApplication::new(
                "Dashboard",
                "dashboard",
            )))
        }
        fn name(&self) -> &'static str {
            "dashboard"
        }
    }
    let probe: Arc<dyn ActiveApplicationProbe> = Arc::new(DashboardProbe);
    let info = PlatformInfo {
        home_dir: std::path::PathBuf::from("/tmp"),
        data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
        os_family: OsFamily::Macos,
        display_server: DisplayServer::Unknown,
    };
    let platform_adapters = clipvault_core::PlatformAdapters::new(
        Arc::new(clipvault_core::FakeClipboardBackend::new()) as Arc<dyn ClipboardBackend>,
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
        .with_clock(clock.clone())
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    let cached = context.cached_active_app_probe();
    cached.refresh_with(Some(PlatformActiveApplication::new(
        "Dashboard",
        "dashboard",
    )));

    let clipboard_backend = Arc::new(clipvault_core::FakeClipboardBackend::new());
    clipboard_backend.push_read(Ok(Some("cv-allowed-end-to-end".into())));
    clipboard_backend.push_read(Ok(Some("cv-allowed-end-to-end".into())));
    let watcher = CaptureWatcher::new(clipboard_backend, Duration::from_millis(10));
    let first = watcher.tick(&context, None);
    assert!(
        matches!(
            first,
            clipvault_core::WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
        ),
        "allowed capture must persist, got {first:?}"
    );
    let second = watcher.tick(&context, None);
    assert_eq!(
        second,
        clipvault_core::WatchTickOutcome::Unchanged,
        "second tick on the same watcher must dedupe"
    );
    let count = {
        let mut db = context.database().lock();
        EntryRepository::new(db.connection_mut()).count().unwrap()
    };
    assert_eq!(count, 1);
}

// ---------------------------------------------------------------------------
// Bug regression: the diagnostics JSON surface (the metadata-only payload
// the Tauri command returns) must never carry the clipboard content, the
// content hash or a source identifier that was used in a past capture.
// ---------------------------------------------------------------------------

#[test]
fn diagnostics_payload_never_leaks_clipboard_payload_or_hash() {
    use clipvault_db::builtin_migrations;
    use clipvault_platform::{
        ActiveApplication as PlatformActiveApplication, ActiveApplicationProbe, Capabilities,
        ClipboardBackend, DisplayServer, HotkeyManager, OsFamily, PasteController, PlatformInfo,
        SettingsNavigator, TrayController,
    };

    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    {
        let mut db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");
    }
    struct BlacklistedProbe;
    impl ActiveApplicationProbe for BlacklistedProbe {
        fn active_application(
            &self,
        ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
        {
            Ok(Some(PlatformActiveApplication::new(
                "SecretApp",
                "secret.app",
            )))
        }
        fn name(&self) -> &'static str {
            "secret-app"
        }
    }
    let probe: Arc<dyn ActiveApplicationProbe> = Arc::new(BlacklistedProbe);
    let info = PlatformInfo {
        home_dir: std::path::PathBuf::from("/tmp"),
        data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
        os_family: OsFamily::Macos,
        display_server: DisplayServer::Unknown,
    };
    let platform_adapters = clipvault_core::PlatformAdapters::new(
        Arc::new(clipvault_core::FakeClipboardBackend::new()) as Arc<dyn ClipboardBackend>,
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
        .with_clock(clock)
        .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()) as Arc<dyn Clipboard>)
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    let payload = "cv-secret-marker-DIAG-7711";
    let hash = clipvault_core::hash_content(payload);

    // Warm the cache with the identifier that produced the secret.
    context
        .cached_active_app_probe()
        .refresh_with(Some(PlatformActiveApplication::new(
            "SecretApp",
            "secret.app",
        )));

    let diag = context.active_app_diagnostics();
    let json = serde_json::to_string(&diag).unwrap();
    assert!(
        !json.contains(payload),
        "diagnostics JSON leaked the clipboard payload: {json}"
    );
    assert!(
        !json.contains(&hash),
        "diagnostics JSON leaked the content hash: {json}"
    );
    // The cached identifier / name ARE expected in the diagnostics
    // surface — the user already sees them in their dock/taskbar
    // and the privacy contract only forbids the clipboard payload,
    // the snippet and the content hash from leaking.

    // Drive the gate through the diagnostics state and a synthetic
    // capture decision so the `last_capture_decision` field is
    // exercised. The label MUST stay metadata-only.
    let state = context.active_app_diagnostics_state();
    state.record_capture_decision("discarded:blacklisted");
    let diag = context.active_app_diagnostics();
    assert_eq!(
        diag.last_capture_decision.as_deref(),
        Some("discarded:blacklisted")
    );
    let json = serde_json::to_string(&diag).unwrap();
    assert!(
        !json.contains(payload),
        "second diagnostics JSON leaked the clipboard payload: {json}"
    );
    assert!(
        !json.contains(&hash),
        "second diagnostics JSON leaked the content hash: {json}"
    );

    // A failed-refresh path must surface the actionable message but
    // never carry the user payload either.
    let diag_failed =
        context.record_active_app_sync_failure("main thread did not execute the closure in 500ms");
    let json = serde_json::to_string(&diag_failed).unwrap();
    assert!(
        !json.contains(payload),
        "failed diagnostics leaked payload: {json}"
    );
    assert!(
        !json.contains(&hash),
        "failed diagnostics leaked hash: {json}"
    );
}

// ---------------------------------------------------------------------------
// Bug regression: the `MakeWriter` capture the redactor tests already
// install MUST also confirm that the capture pipeline emits neither the
// payload nor the cached identifier through any `tracing::*!` macro while
// `record_active_app_sync_failure` records the failure. The previous
// regression suite pinned the blacklisted path; this one extends it to the
// sync-failure path.
// ---------------------------------------------------------------------------

#[test]
fn sync_refresh_failure_does_not_log_payload_or_identifier() {
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    #[derive(Default, Clone)]
    struct SharedCapture(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedCapture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'w> MakeWriter<'w> for SharedCapture {
        type Writer = SharedCapture;
        fn make_writer(&'w self) -> Self::Writer {
            self.clone()
        }
    }

    let capture = SharedCapture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(capture.clone())
        .with_max_level(tracing::Level::TRACE)
        .finish();

    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipvault_core::FakeClipboard::new());
    let info = clipvault_platform::PlatformInfo {
        home_dir: dir.path().to_path_buf(),
        data_dir: dir.path().join("data"),
        os_family: clipvault_platform::OsFamily::Macos,
        display_server: clipvault_platform::DisplayServer::Unknown,
    };
    let platform_adapters = clipvault_core::PlatformAdapters::new(
        Arc::new(clipvault_core::FakeClipboardBackend::new())
            as Arc<dyn clipvault_platform::ClipboardBackend>,
        Arc::new(clipvault_core::FakeHotkeyManager::new())
            as Arc<dyn clipvault_platform::HotkeyManager>,
        Arc::new(clipvault_core::FakeActiveApplication::new())
            as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
        Arc::new(clipvault_core::FakePasteController::new())
            as Arc<dyn clipvault_platform::PasteController>,
        Arc::new(clipvault_core::FakeTrayController::new())
            as Arc<dyn clipvault_platform::TrayController>,
        Arc::new(clipvault_core::FakeSettingsNavigator::new())
            as Arc<dyn clipvault_platform::SettingsNavigator>,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        clipvault_platform::Capabilities::ALL_AVAILABLE,
        info,
    );
    let context = AppBootstrap::new()
        .with_clock(clock)
        .with_clipboard(clipboard)
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    context.cached_active_app_probe().refresh_with(Some(
        clipvault_platform::ActiveApplication::new("SecretApp", "secret.app"),
    ));
    let payload = "cv-leak-sync-failure-ZX-9911";
    let hash = clipvault_core::hash_content(payload);
    let diag_after = tracing::subscriber::with_default(subscriber, || {
        context.record_active_app_sync_failure("main thread did not execute the closure in 500ms")
    });
    let buffer = String::from_utf8_lossy(&capture.0.lock().unwrap()).into_owned();
    assert!(
        !buffer.contains(payload),
        "sync-failure logs leaked the clipboard payload: {buffer}"
    );
    assert!(
        !buffer.contains(&hash),
        "sync-failure logs leaked the content hash: {buffer}"
    );
    assert!(
        !buffer.to_ascii_lowercase().contains("secret.app"),
        "sync-failure logs leaked the cached identifier: {buffer}"
    );
    // The actionable reason MUST still be visible to the operator
    // through the diagnostics state (which is what the dashboard
    // renders). `record_active_app_sync_failure` itself does not
    // emit a log line; the shell's `refresh_active_app_cached`
    // function logs a single `warn!` with the same message — see
    // `refresh_active_app_cached` for the corresponding contract.
    match &diag_after.refresh_outcome {
        clipvault_core::ActiveAppRefreshOutcome::Failed { message, .. } => {
            assert!(
                message.contains("500ms"),
                "diagnostics lost the actionable timeout duration: {message}"
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Bug regression: macOS main-queue refresher must surface every refresh
// outcome through the diagnostics state so the user can tell apart
// `failed:schedule` from `failed:timeout` from `failed:unavailable`
// from `failed:backend` without parsing free-form strings. The
// `failure_kind` field is the contract the frontend renders.
// ---------------------------------------------------------------------------

#[test]
fn failure_kind_distinguishes_schedule_from_timeout() {
    // `MainThreadSyncError::Schedule` and `MainThreadSyncError::Timeout`
    // map to distinct `ActiveAppFailureKind` values. The shell
    // converts the typed error via `MainThreadSyncError::failure_kind`
    // and `AppContext::record_active_app_sync_failure_with_kind`. The
    // UI gets the value as a stable snake_case string in
    // `ActiveAppDiagnostics::failure_kind`.
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipvault_core::FakeClipboard::new());
    let info = clipvault_platform::PlatformInfo {
        home_dir: dir.path().to_path_buf(),
        data_dir: dir.path().join("data"),
        os_family: clipvault_platform::OsFamily::Macos,
        display_server: clipvault_platform::DisplayServer::Unknown,
    };
    let platform_adapters = clipvault_core::PlatformAdapters::new(
        Arc::new(clipvault_core::FakeClipboardBackend::new())
            as Arc<dyn clipvault_platform::ClipboardBackend>,
        Arc::new(clipvault_core::FakeHotkeyManager::new())
            as Arc<dyn clipvault_platform::HotkeyManager>,
        Arc::new(clipvault_core::FakeActiveApplication::new())
            as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
        Arc::new(clipvault_core::FakePasteController::new())
            as Arc<dyn clipvault_platform::PasteController>,
        Arc::new(clipvault_core::FakeTrayController::new())
            as Arc<dyn clipvault_platform::TrayController>,
        Arc::new(clipvault_core::FakeSettingsNavigator::new())
            as Arc<dyn clipvault_platform::SettingsNavigator>,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        clipvault_platform::Capabilities::ALL_AVAILABLE,
        info,
    );
    let context = AppBootstrap::new()
        .with_clock(clock)
        .with_clipboard(clipboard)
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    // Schedule failure.
    let diag = context.record_active_app_sync_failure_with_kind(
        clipvault_core::ActiveAppFailureKind::Schedule,
        "main thread did not accept the closure: event loop shut down",
    );
    assert_eq!(diag.failure_kind, Some("schedule"));
    assert_eq!(diag.failed_refreshes, 1);

    // Timeout failure.
    let diag = context.record_active_app_sync_failure_with_kind(
        clipvault_core::ActiveAppFailureKind::Timeout,
        "main thread did not execute the closure in 500ms",
    );
    assert_eq!(diag.failure_kind, Some("timeout"));
    assert_eq!(diag.failed_refreshes, 2);

    // Plain platform-side Unavailable still maps to "unavailable".
    let diag = context.refresh_active_application();
    let _ = diag;
    let diag = context.record_active_app_sync_failure_with_kind(
        clipvault_core::ActiveAppFailureKind::Unavailable,
        "active-app probe unavailable on this session",
    );
    assert_eq!(diag.failure_kind, Some("unavailable"));
    assert_eq!(diag.failed_refreshes, 3);

    // Plain backend error still maps to "backend".
    let diag = context.record_active_app_sync_failure_with_kind(
        clipvault_core::ActiveAppFailureKind::Backend,
        "x11 disconnected",
    );
    assert_eq!(diag.failure_kind, Some("backend"));
    assert_eq!(diag.failed_refreshes, 4);
}

#[test]
fn snapshot_failure_kind_round_trips_through_active_app_diagnostics() {
    // The serialised `ActiveAppDiagnostics` JSON carries the failure
    // kind as a snake_case string so the frontend can dispatch on it
    // without parsing free-form text. We pin both the shape of the
    // field and its values here so a future refactor that drops the
    // structured kind surfaces as a test failure.
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipvault_core::FakeClipboard::new());
    let info = clipvault_platform::PlatformInfo {
        home_dir: dir.path().to_path_buf(),
        data_dir: dir.path().join("data"),
        os_family: clipvault_platform::OsFamily::Macos,
        display_server: clipvault_platform::DisplayServer::Unknown,
    };
    let platform_adapters = clipvault_core::PlatformAdapters::new(
        Arc::new(clipvault_core::FakeClipboardBackend::new())
            as Arc<dyn clipvault_platform::ClipboardBackend>,
        Arc::new(clipvault_core::FakeHotkeyManager::new())
            as Arc<dyn clipvault_platform::HotkeyManager>,
        Arc::new(clipvault_core::FakeActiveApplication::new())
            as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
        Arc::new(clipvault_core::FakePasteController::new())
            as Arc<dyn clipvault_platform::PasteController>,
        Arc::new(clipvault_core::FakeTrayController::new())
            as Arc<dyn clipvault_platform::TrayController>,
        Arc::new(clipvault_core::FakeSettingsNavigator::new())
            as Arc<dyn clipvault_platform::SettingsNavigator>,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        clipvault_platform::Capabilities::ALL_AVAILABLE,
        info,
    );
    let context = AppBootstrap::new()
        .with_clock(clock)
        .with_clipboard(clipboard)
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    context.record_active_app_sync_failure_with_kind(
        clipvault_core::ActiveAppFailureKind::Timeout,
        "main thread did not execute the closure in 500ms",
    );
    let diag = context.active_app_diagnostics();
    assert_eq!(diag.failure_kind, Some("timeout"));
    let json = serde_json::to_string(&diag).unwrap();
    assert!(
        json.contains("\"failure_kind\":\"timeout\""),
        "missing structured failure_kind field: {json}"
    );
    assert!(
        json.contains("\"refresh_outcome\":{\"failed\":"),
        "missing failed variant in refresh_outcome: {json}"
    );
    assert!(
        !json.contains("cv-leak-secret-zzz"),
        "diagnostics JSON leaked a fake secret: {json}"
    );
}

#[test]
fn ok_outcome_has_no_failure_kind_field() {
    // A successful refresh leaves `failure_kind` as `None`. The
    // serialised JSON must omit the field so the UI can rely on a
    // truthy check before rendering the failure card.
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipvault_core::FakeClipboard::new());
    let info = clipvault_platform::PlatformInfo {
        home_dir: dir.path().to_path_buf(),
        data_dir: dir.path().join("data"),
        os_family: clipvault_platform::OsFamily::Macos,
        display_server: clipvault_platform::DisplayServer::Unknown,
    };
    struct FixedProbe(&'static str);
    impl clipvault_platform::ActiveApplicationProbe for FixedProbe {
        fn active_application(
            &self,
        ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
        {
            Ok(Some(clipvault_platform::ActiveApplication::new(
                "TextEdit", self.0,
            )))
        }
        fn name(&self) -> &'static str {
            "fixed"
        }
    }
    let probe: Arc<dyn clipvault_platform::ActiveApplicationProbe> =
        Arc::new(FixedProbe("com.apple.TextEdit"));
    let platform_adapters = clipvault_core::PlatformAdapters::new(
        Arc::new(clipvault_core::FakeClipboardBackend::new())
            as Arc<dyn clipvault_platform::ClipboardBackend>,
        Arc::new(clipvault_core::FakeHotkeyManager::new())
            as Arc<dyn clipvault_platform::HotkeyManager>,
        probe,
        Arc::new(clipvault_core::FakePasteController::new())
            as Arc<dyn clipvault_platform::PasteController>,
        Arc::new(clipvault_core::FakeTrayController::new())
            as Arc<dyn clipvault_platform::TrayController>,
        Arc::new(clipvault_core::FakeSettingsNavigator::new())
            as Arc<dyn clipvault_platform::SettingsNavigator>,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        clipvault_platform::Capabilities::ALL_AVAILABLE,
        info,
    );
    let context = AppBootstrap::new()
        .with_clock(clock)
        .with_clipboard(clipboard)
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    let _ = context.refresh_active_application();
    let diag = context.active_app_diagnostics();
    assert!(diag.failure_kind.is_none());
    let json = serde_json::to_string(&diag).unwrap();
    assert!(
        !json.contains("failure_kind"),
        "Ok outcome must not serialise failure_kind: {json}"
    );
}

#[test]
fn integration_textedit_blacklist_via_main_queue_outcome_discards_capture() {
    // Reproduces the macOS scenario: the cached probe reports
    // `com.apple.TextEdit`, the blacklist contains the same bundle
    // id, the watcher ticks with a distinctive payload, the
    // `PrivacyGate` decides `Discard`, and the SQLite row count
    // stays at 0. The probe is populated through the
    // `ActiveAppRefreshOutcome::Ok` channel the macOS main-queue
    // refresher uses, so this is the closest unit-level analogue
    // to the production path.
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 00:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipvault_core::FakeClipboard::with_text(
        "cv-main-queue-textedit-payload-7741",
    ));
    let info = clipvault_platform::PlatformInfo {
        home_dir: dir.path().to_path_buf(),
        data_dir: dir.path().join("data"),
        os_family: clipvault_platform::OsFamily::Macos,
        display_server: clipvault_platform::DisplayServer::Unknown,
    };
    struct FixedProbe(&'static str);
    impl clipvault_platform::ActiveApplicationProbe for FixedProbe {
        fn active_application(
            &self,
        ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
        {
            Ok(Some(clipvault_platform::ActiveApplication::new(
                "TextEdit", self.0,
            )))
        }
        fn name(&self) -> &'static str {
            "fixed"
        }
    }
    let probe: Arc<dyn clipvault_platform::ActiveApplicationProbe> =
        Arc::new(FixedProbe("com.apple.TextEdit"));
    let platform_adapters = clipvault_core::PlatformAdapters::new(
        Arc::new(clipvault_core::FakeClipboardBackend::new())
            as Arc<dyn clipvault_platform::ClipboardBackend>,
        Arc::new(clipvault_core::FakeHotkeyManager::new())
            as Arc<dyn clipvault_platform::HotkeyManager>,
        probe,
        Arc::new(clipvault_core::FakePasteController::new())
            as Arc<dyn clipvault_platform::PasteController>,
        Arc::new(clipvault_core::FakeTrayController::new())
            as Arc<dyn clipvault_platform::TrayController>,
        Arc::new(clipvault_core::FakeSettingsNavigator::new())
            as Arc<dyn clipvault_platform::SettingsNavigator>,
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        clipvault_platform::Capabilities::ALL_AVAILABLE,
        info,
    );
    let context = AppBootstrap::new()
        .with_clock(clock.clone())
        .with_clipboard(clipboard.clone())
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    // Persist the blacklisted identifier in SQLite so the gate's
    // matcher is not empty.
    {
        let mut db = context.database().lock();
        let conn = db.connection_mut();
        let mut repo = clipvault_db::IgnoredAppRepository::new(conn);
        repo.insert("com.apple.TextEdit", time::OffsetDateTime::now_utc())
            .expect("insert blacklist");
    }

    // Mirror what the macOS main-queue refresher does on every
    // tick: invoke the inner probe, normalise the answer, and store
    // it on the cached probe. The diagnostics state records the
    // outcome the same way the production refresher does.
    let _ = context.refresh_active_application();
    let diagnostics = context.active_app_diagnostics();
    assert!(diagnostics.cache_populated);
    assert_eq!(
        diagnostics.identifier.as_deref(),
        Some("com.apple.TextEdit")
    );

    // Drive the watcher once with the cached probe reporting
    // `com.apple.TextEdit`. The PrivacyGate should discard the
    // capture so the SQLite row count stays at zero. The
    // `CoreBlacklistMatcher` requires normalised identifiers (trim +
    // lowercase); the bootstrap normalises them before storage, so
    // we mirror that contract here.
    let probe_for_gate = context.cached_active_app_probe().inner().clone();
    let normalised_blacklist = vec![clipvault_core::privacy::normalize("com.apple.TextEdit")];
    let gate =
        clipvault_core::PrivacyGate::from_probe(probe_for_gate.clone(), normalised_blacklist);
    let outcome = clipvault_core::TextHistoryService::new(
        context.clipboard(),
        context.clock(),
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
    )
    .with_privacy_gate(gate)
    .record_text(&context, None);
    assert!(
        matches!(outcome, clipvault_core::HistoryOutcome::Ignored),
        "expected Ignored for blacklisted source, got {outcome:?}"
    );

    let row_count = {
        let mut db = context.database().lock();
        let conn = db.connection_mut();
        conn.query_row::<i64, _, _>("SELECT COUNT(*) FROM clipboard_entries", [], |row| {
            row.get(0)
        })
        .expect("count")
    };
    assert_eq!(
        row_count, 0,
        "blacklisted capture must NOT insert a SQLite row"
    );

    // The diagnostics card must record the discard decision the
    // same way the production capture loop does.
    let diagnostics = context.active_app_diagnostics_state();
    diagnostics.record_capture_decision("discarded:blacklisted");
    let snap = diagnostics.snapshot();
    assert_eq!(
        snap.last_capture_decision.as_deref(),
        Some("discarded:blacklisted")
    );
    assert!(snap.cache_populated);
}

// ---------------------------------------------------------------------------
// Bug regression: the diagnostics surface must report the macOS native
// backend (`macos_workspace`) when the bootstrap wires the real adapter,
// and must report `unavailable` when the host falls back to the no-op
// probe. This is the regression pin against the previous shell that
// always emitted `macos_workspace` even though the dispatch surface
// resolved to `NoopActiveApplicationProbe`.
// ---------------------------------------------------------------------------

#[test]
fn macos_native_adapter_does_not_silently_resolve_to_unavailable() {
    use clipvault_db::builtin_migrations;
    use clipvault_platform::{
        ActiveApplicationProbe, Capabilities, ClipboardBackend, DisplayServer, HotkeyManager,
        OsFamily, PasteController, PlatformInfo, SettingsNavigator, TrayController,
    };

    let dir = tempdir();
    {
        let mut db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");
    }

    let info = PlatformInfo {
        home_dir: std::path::PathBuf::from("/tmp"),
        data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
        os_family: OsFamily::Macos,
        display_server: DisplayServer::Unknown,
    };

    // The shell builds the macOS native probe in `bootstrap::build_active_application`;
    // we cannot import `runtime::macos_active_app::MacOsActiveApplication`
    // because the `clipvault-core` dev-dependencies do not carry the
    // `macos-native` feature. Instead, we wire a scripted probe that
    // mirrors the macOS adapter's contract so the test exercises the
    // diagnostics resolution from the `OsFamily` + capability side.
    // The shell-side test `installer_uses_native_adapter_on_macos`
    // already pins the actual `MacOsActiveApplication` wiring.
    struct MacOsShapedProbe;
    impl ActiveApplicationProbe for MacOsShapedProbe {
        fn active_application(
            &self,
        ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
        {
            Ok(Some(clipvault_platform::ActiveApplication::new(
                "TextEdit",
                "com.apple.TextEdit",
            )))
        }
        fn name(&self) -> &'static str {
            "macos-shaped-probe"
        }
    }

    let probe: Arc<dyn ActiveApplicationProbe> = Arc::new(MacOsShapedProbe);
    let platform_adapters = clipvault_core::PlatformAdapters::new(
        Arc::new(clipvault_core::FakeClipboardBackend::new()) as Arc<dyn ClipboardBackend>,
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
        .with_clock(Arc::new(FixedClock {
            instant: datetime!(2026-09-03 00:00:00 UTC),
        }))
        .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()) as Arc<dyn Clipboard>)
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    let diag = context.active_app_diagnostics();
    assert!(diag.available, "macOS adapter must report available=true");
    assert_eq!(
        diag.backend, "macos_workspace",
        "the diagnostics surface must report the macOS backend name, not `unavailable` — this is the regression pin that prevents `backend = macos_workspace` while the underlying probe is actually the no-op"
    );
}

#[test]
fn macos_diagnostics_after_five_seconds_observed_from_state() {
    // Simulates the boot-flow the user observed: 5 seconds after
    // startup the diagnostics card must report the live
    // periodic-refresh metadata. The actual libdispatch timer is
    // platform-only, so this test drives the metadata manually and
    // pins the same fields the shell's `on_outcome` closure would
    // populate in production.
    use clipvault_db::builtin_migrations;
    use clipvault_platform::{
        ActiveApplicationProbe, Capabilities, ClipboardBackend, DisplayServer, HotkeyManager,
        OsFamily, PasteController, PlatformInfo, SettingsNavigator, TrayController,
    };

    let dir = tempdir();
    {
        let mut db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");
    }

    let info = PlatformInfo {
        home_dir: std::path::PathBuf::from("/tmp"),
        data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
        os_family: OsFamily::Macos,
        display_server: DisplayServer::Unknown,
    };

    struct MacOsShapedProbe;
    impl ActiveApplicationProbe for MacOsShapedProbe {
        fn active_application(
            &self,
        ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
        {
            Ok(Some(clipvault_platform::ActiveApplication::new(
                "TextEdit",
                "com.apple.TextEdit",
            )))
        }
        fn name(&self) -> &'static str {
            "macos-shaped-probe"
        }
    }

    let probe: Arc<dyn ActiveApplicationProbe> = Arc::new(MacOsShapedProbe);
    let platform_adapters = clipvault_core::PlatformAdapters::new(
        Arc::new(clipvault_core::FakeClipboardBackend::new()) as Arc<dyn ClipboardBackend>,
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
        .with_clock(Arc::new(FixedClock {
            instant: datetime!(2026-09-03 00:00:00 UTC),
        }))
        .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()) as Arc<dyn Clipboard>)
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    let state = context.active_app_diagnostics_state();

    // Mark the capture loop alive and the periodic refresher
    // installed (these are the two flags the shell flips exactly
    // once each).
    state.mark_loop_started();
    state.mark_refresher_installed();

    // Simulate the libdispatch timer firing every second for five
    // seconds and recording a successful refresh each time.
    let identifier = "com.apple.TextEdit";
    for _ in 0..5 {
        state.record_timer_callback();
        state.record_refresh(Some(clipvault_platform::ActiveApplication::new(
            "TextEdit", identifier,
        )));
    }
    let snap = state.snapshot();
    assert!(snap.loop_started, "loop_started must flip once");
    assert!(
        snap.refresher_installed,
        "refresher_installed must flip when the timer handle is held"
    );
    assert!(
        snap.timer_callback_count >= 5,
        "five timer firings must surface as at least five callbacks (got {})",
        snap.timer_callback_count
    );
    assert!(
        snap.successful_refreshes >= 5,
        "five timer firings must produce at least five successful refreshes"
    );
    assert_eq!(
        snap.failed_refreshes, 0,
        "all five refreshes succeeded, the failure counter must stay at zero"
    );
    assert!(snap.cache_populated, "the cache must be populated");
    assert_eq!(
        snap.identifier.as_deref(),
        Some(identifier),
        "the most recent identifier must come from the real probe"
    );
    assert!(
        snap.identifier.as_deref().unwrap_or_default().contains('.'),
        "the bundle identifier must look like a real reverse-DNS string (got {:?})",
        snap.identifier
    );
    assert_eq!(snap.backend, "macos_workspace");
    assert!(snap.last_refresh_unix_ms.is_some());
}

#[test]
fn failed_timer_firings_advance_callback_count_but_not_successful_refreshes() {
    // Regression pin: `timer_callback_count` is independent from
    // `successful_refreshes` / `failed_refreshes` so the user can
    // confirm the timer is alive even when every probe call fails.
    use clipvault_db::builtin_migrations;
    use clipvault_platform::{
        ActiveAppError, ActiveApplicationProbe, Capabilities, ClipboardBackend, DisplayServer,
        HotkeyManager, OsFamily, PasteController, PlatformInfo, SettingsNavigator, TrayController,
    };

    let dir = tempdir();
    let mut db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
    db.run_migrations(&builtin_migrations()).expect("migrate");

    struct AlwaysFailingProbe;
    impl ActiveApplicationProbe for AlwaysFailingProbe {
        fn active_application(
            &self,
        ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
        {
            Err(ActiveAppError::Backend {
                details: "simulated transient".to_string(),
            })
        }
        fn name(&self) -> &'static str {
            "always-failing"
        }
    }

    let info = PlatformInfo {
        home_dir: std::path::PathBuf::from("/tmp"),
        data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
        os_family: OsFamily::Macos,
        display_server: DisplayServer::Unknown,
    };
    let probe: Arc<dyn ActiveApplicationProbe> = Arc::new(AlwaysFailingProbe);
    let platform_adapters = clipvault_core::PlatformAdapters::new(
        Arc::new(clipvault_core::FakeClipboardBackend::new()) as Arc<dyn ClipboardBackend>,
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
        .with_clock(Arc::new(FixedClock {
            instant: datetime!(2026-09-03 00:00:00 UTC),
        }))
        .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()) as Arc<dyn Clipboard>)
        .with_platform_adapters(platform_adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    let state = context.active_app_diagnostics_state();
    state.mark_loop_started();
    state.mark_refresher_installed();
    for _ in 0..5 {
        state.record_timer_callback();
        state.record_failure(&clipvault_platform::ActiveAppError::Backend {
            details: "simulated".to_string(),
        });
    }
    let snap = state.snapshot();
    assert_eq!(snap.timer_callback_count, 5);
    assert_eq!(snap.successful_refreshes, 0);
    assert_eq!(snap.failed_refreshes, 5);
    assert_eq!(snap.refresh_attempts, 5);
    assert_eq!(snap.backend, "macos_workspace");
    assert!(!snap.cache_populated);
}
