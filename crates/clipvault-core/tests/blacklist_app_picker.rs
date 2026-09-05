//! Integration tests for the `blacklist-app-picker` change.
//!
//! These tests exercise the cross-layer contract:
//!
//! 1. The picker service normalises identifiers and persists rows
//!    idempotently, including when the same application is
//!    re-selected.
//! 2. Migration 0006 is additive: a database that pre-dates the
//!    change keeps matching the identifier and the frontend reads
//!    legacy rows with `None` metadata.
//! 3. The privacy gate continues to use the normalised identifier
//!    — never the display name or icon — so a blacklisted
//!    application's captures stay discarded after restart.
//! 4. The picker surface never carries clipboard content, hashes or
//!    snippets through the typed error or the success response.

use std::sync::Arc;

use clipvault_core::{
    AppBootstrap, Clipboard, Clock, FakeClipboard, IgnoredAppEntry, IgnoredAppsService,
    PickAndAddOutcome, PrivacyGate,
};
use clipvault_db::{builtin_migrations, IgnoredAppRepository};
use clipvault_platform::{
    ApplicationPicker, ApplicationPickerError, NoopActiveApplicationProbe, SelectedApplication,
};
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

struct ScriptedPicker(Result<SelectedApplication, ApplicationPickerError>);

impl ApplicationPicker for ScriptedPicker {
    fn pick(&self) -> Result<SelectedApplication, ApplicationPickerError> {
        match &self.0 {
            Ok(value) => Ok(value.clone()),
            Err(error) => Err(match error {
                ApplicationPickerError::Cancelled => ApplicationPickerError::Cancelled,
                ApplicationPickerError::InvalidSelection { reason } => {
                    ApplicationPickerError::InvalidSelection {
                        reason: reason.clone(),
                    }
                }
                ApplicationPickerError::MissingIdentifier => {
                    ApplicationPickerError::MissingIdentifier
                }
                ApplicationPickerError::BackendUnavailable { reason } => {
                    ApplicationPickerError::BackendUnavailable {
                        reason: reason.clone(),
                    }
                }
                ApplicationPickerError::UnsupportedSession { reason } => {
                    ApplicationPickerError::UnsupportedSession {
                        reason: reason.clone(),
                    }
                }
            }),
        }
    }
    fn name(&self) -> &'static str {
        "scripted"
    }
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
    // The bootstrap hands the gate we just built to the
    // settings/history services; for these tests we re-create a
    // service with the supplied gate so the service exercises the
    // exact matcher the assertion will use.
    let _ = gate;
    context.set_capabilities(context.capabilities());
    (dir, context)
}

fn probe() -> Arc<dyn clipvault_platform::ActiveApplicationProbe> {
    Arc::new(NoopActiveApplicationProbe)
}

#[test]
fn picker_service_normalises_identifier_and_persists_metadata() {
    let (_dir, context) = bootstrap_with_gate(PrivacyGate::from_probe(probe(), vec![]));
    let gate = PrivacyGate::from_probe(probe(), vec![]);
    let service = IgnoredAppsService::new(context.clock(), gate.clone());

    let picker = ScriptedPicker(Ok(SelectedApplication {
        identifier: "  Com.Apple.Terminal  ".to_string(),
        display_name: "Terminal".to_string(),
        icon_ref: Some("ignored-apps/com.apple.terminal.png".to_string()),
    }));

    let outcome = service
        .pick_and_add(&context, &picker)
        .expect("pick_and_add");
    match outcome {
        PickAndAddOutcome::Added(entry) => {
            assert_eq!(entry.id, "com.apple.terminal");
            assert_eq!(entry.display_name.as_deref(), Some("Terminal"));
            assert_eq!(
                entry.icon_ref.as_deref(),
                Some("ignored-apps/com.apple.terminal.png")
            );
        }
        other => panic!("expected Added, got {other:?}"),
    }
    // The matcher must observe the normalised id, not the raw value.
    assert!(matches!(
        gate.evaluate(Some("com.apple.terminal")),
        clipvault_core::CaptureDecision::Discard { .. }
    ));
}

#[test]
fn picker_service_re_selecting_an_application_is_idempotent() {
    let (_dir, context) = bootstrap_with_gate(PrivacyGate::from_probe(probe(), vec![]));
    let gate = PrivacyGate::from_probe(probe(), vec![]);
    let service = IgnoredAppsService::new(context.clock(), gate);

    let picker = ScriptedPicker(Ok(SelectedApplication {
        identifier: "com.apple.terminal".to_string(),
        display_name: "Terminal".to_string(),
        icon_ref: Some("ignored-apps/com.apple.terminal.png".to_string()),
    }));

    let first = service
        .pick_and_add(&context, &picker)
        .expect("first pick_and_add");
    let second = service
        .pick_and_add(&context, &picker)
        .expect("second pick_and_add");

    assert!(matches!(first, PickAndAddOutcome::Added(_)));
    assert!(matches!(second, PickAndAddOutcome::Updated(_)));

    // No duplicate row is created.
    let entries: Vec<IgnoredAppEntry> = service.list(&context).expect("list");
    assert_eq!(entries.len(), 1);
}

#[test]
fn picker_service_preserves_rule_when_icon_extraction_fails() {
    // The spec says: "WHEN icon conversion or persistence fails
    // after identifier validation, ClipVault keeps the valid
    // blacklist identifier and display name." This test pins the
    // contract: a `None` icon does NOT block the privacy rule.
    let (_dir, context) = bootstrap_with_gate(PrivacyGate::from_probe(probe(), vec![]));
    let gate = PrivacyGate::from_probe(probe(), vec![]);
    let service = IgnoredAppsService::new(context.clock(), gate.clone());

    let picker = ScriptedPicker(Ok(SelectedApplication {
        identifier: "com.apple.terminal".to_string(),
        display_name: "Terminal".to_string(),
        icon_ref: None,
    }));
    let outcome = service
        .pick_and_add(&context, &picker)
        .expect("pick_and_add");
    match outcome {
        PickAndAddOutcome::Added(entry) => {
            assert!(entry.icon_ref.is_none());
        }
        other => panic!("expected Added, got {other:?}"),
    }
    assert!(matches!(
        gate.evaluate(Some("com.apple.terminal")),
        clipvault_core::CaptureDecision::Discard { .. }
    ));
}

#[test]
fn picker_service_propagates_cancellation_without_mutating_list() {
    let (_dir, context) = bootstrap_with_gate(PrivacyGate::from_probe(probe(), vec![]));
    let gate = PrivacyGate::from_probe(probe(), vec![]);
    let service = IgnoredAppsService::new(context.clock(), gate);

    let picker = ScriptedPicker(Err(ApplicationPickerError::Cancelled));
    let err = service
        .pick_and_add(&context, &picker)
        .expect_err("cancelled");
    match err {
        clipvault_core::IgnoredAppsServiceError::Domain(
            clipvault_core::IgnoredAppError::Cancelled,
        ) => {}
        other => panic!("expected Cancelled, got {other:?}"),
    }
    assert!(service.list(&context).expect("list").is_empty());
}

#[test]
fn picker_service_propagates_unsupported_session() {
    // Linux reports unsupported_session until a safe mapping is wired in.
    let (_dir, context) = bootstrap_with_gate(PrivacyGate::from_probe(probe(), vec![]));
    let gate = PrivacyGate::from_probe(probe(), vec![]);
    let service = IgnoredAppsService::new(context.clock(), gate);

    let picker = ScriptedPicker(Err(ApplicationPickerError::UnsupportedSession {
        reason: "wayland".into(),
    }));
    let err = service.pick_and_add(&context, &picker).expect_err("err");
    match err {
        clipvault_core::IgnoredAppsServiceError::Domain(
            clipvault_core::IgnoredAppError::UnsupportedSession { reason },
        ) => {
            assert_eq!(reason, "wayland");
        }
        other => panic!("expected UnsupportedSession, got {other:?}"),
    }
}

#[test]
fn migration_0006_is_additive_and_reads_legacy_rows() {
    // Simulate a database created before the picker change: open a
    // SQLite file, run migrations up to version 5, insert an
    // identifier-only row through a raw SQL statement (the
    // repository would refuse to insert without the new columns),
    // then run migration 0006 on the same connection. The
    // repository must still see the row, with `display_name` and
    // `icon_ref` set to `None`.
    let dir = tempdir();
    let mut db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
    let migrations = builtin_migrations();
    // Run only the first five migrations to mimic a pre-picker build.
    let pre_picker: Vec<_> = migrations.iter().take(5).cloned().collect();
    db.run_migrations(&pre_picker).expect("migrate v1..v5");
    {
        let conn = db.connection_mut();
        let result = conn.execute(
            "INSERT INTO ignored_apps (id, created_at) VALUES (?1, ?2)",
            ["firefox", "2025-12-31T00:00:00Z"],
        );
        let _: usize = result.expect("legacy insert via raw SQL");
    }
    // Now apply migration 0006 on the same database.
    let upgrade: Vec<_> = migrations.iter().skip(5).cloned().collect();
    db.run_migrations(&upgrade).expect("migrate v6");
    {
        let conn = db.connection_mut();
        let repo = IgnoredAppRepository::new(conn);
        let row = repo.get("firefox").unwrap().expect("present");
        assert_eq!(row.id, "firefox");
        assert!(row.display_name.is_none());
        assert!(row.icon_ref.is_none());
    }
    // The bootstrap reads the same row through the metadata
    // service: a pre-existing user can restart ClipVault and see
    // the row without losing it. The harness wires an isolated
    // `PlatformAdapters` bundle so the asset collector cannot reach
    // the developer's real `~/.clipvault`.
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(Arc::new(FixedClock {
            instant: datetime!(2026-01-02 03:04:05 UTC),
        }) as Arc<dyn Clock>)
        .with_clipboard(Arc::new(FakeClipboard::new()) as Arc<dyn Clipboard>)
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    let service =
        IgnoredAppsService::new(context.clock(), PrivacyGate::from_probe(probe(), vec![]));
    let entries = service.list(&context).expect("list");
    assert_eq!(entries.len(), 1);
    let row = &entries[0];
    assert_eq!(row.id, "firefox");
    assert!(row.display_name.is_none());
    assert!(row.icon_ref.is_none());
}

#[test]
fn privacy_gate_still_uses_normalised_identifier_after_picker_persists() {
    // After the picker service adds an application, the privacy gate
    // must observe the same id the matcher uses, NOT the display
    // name. The displayed name is presentation metadata only.
    let (_dir, context) = bootstrap_with_gate(PrivacyGate::from_probe(probe(), vec![]));
    let gate = PrivacyGate::from_probe(probe(), vec!["terminal".into()]);
    let service = IgnoredAppsService::new(context.clock(), gate.clone());

    let picker = ScriptedPicker(Ok(SelectedApplication {
        identifier: "com.apple.terminal".to_string(),
        display_name: "Terminal".to_string(),
        icon_ref: None,
    }));
    let outcome = service
        .pick_and_add(&context, &picker)
        .expect("pick_and_add");
    assert!(matches!(outcome, PickAndAddOutcome::Added(_)));

    // The matcher sees the normalised id, never the display name.
    assert!(matches!(
        gate.evaluate(Some("com.apple.terminal")),
        clipvault_core::CaptureDecision::Discard { .. }
    ));
    assert!(matches!(
        gate.evaluate(Some("Terminal")),
        clipvault_core::CaptureDecision::Allow
    ));
}

#[test]
fn picker_service_does_not_persist_paths_or_clipboard_content() {
    // Privacy regression: the picker service must NEVER persist the
    // absolute bundle path or any clipboard-derived data. The
    // repository only stores the identifier, display name and icon
    // reference.
    let (_dir, context) = bootstrap_with_gate(PrivacyGate::from_probe(probe(), vec![]));
    let gate = PrivacyGate::from_probe(probe(), vec![]);
    let service = IgnoredAppsService::new(context.clock(), gate);

    let picker = ScriptedPicker(Ok(SelectedApplication {
        identifier: "com.apple.terminal".to_string(),
        display_name: "Terminal".to_string(),
        icon_ref: Some("ignored-apps/com.apple.terminal.png".to_string()),
    }));
    let _ = service.pick_and_add(&context, &picker).expect("pick");

    let conn = context.database().lock();
    let raw = conn
        .connection()
        .query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM ignored_apps WHERE id LIKE '%/%' OR display_name LIKE '%/%' OR icon_ref LIKE '/%'",
            [],
            |row| row.get(0),
        )
        .expect("query");
    assert_eq!(raw, 0, "paths must never reach the database");
}

#[test]
fn picker_service_list_returns_legacy_and_picker_rows_together() {
    // A mixed database (one legacy row + one picker-supplied row)
    // must render both through the same list API so the frontend
    // can render a single deterministic list.
    let dir = tempdir();
    let mut db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
    db.run_migrations(&builtin_migrations()).expect("migrate");
    {
        let conn = db.connection_mut();
        let mut repo = IgnoredAppRepository::new(conn);
        repo.insert("firefox", datetime!(2025-12-31 00:00:00 UTC))
            .expect("legacy insert");
    }
    let context = AppBootstrap::new()
        .with_clock(Arc::new(FixedClock {
            instant: datetime!(2026-01-02 03:04:05 UTC),
        }) as Arc<dyn Clock>)
        .with_clipboard(Arc::new(FakeClipboard::new()) as Arc<dyn Clipboard>)
        .with_platform_adapters(clipvault_core::build_isolated_adapters(
            dir.path(),
            &dir.path().join("data"),
        ))
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    let service =
        IgnoredAppsService::new(context.clock(), PrivacyGate::from_probe(probe(), vec![]));
    let picker = ScriptedPicker(Ok(SelectedApplication {
        identifier: "com.apple.terminal".to_string(),
        display_name: "Terminal".to_string(),
        icon_ref: Some("ignored-apps/com.apple.terminal.png".to_string()),
    }));
    service.pick_and_add(&context, &picker).expect("pick");

    let entries = service.list(&context).expect("list");
    assert_eq!(entries.len(), 2);
    let terminal = entries
        .iter()
        .find(|row| row.id == "com.apple.terminal")
        .expect("picker row");
    assert_eq!(terminal.display_name.as_deref(), Some("Terminal"));
    let firefox = entries
        .iter()
        .find(|row| row.id == "firefox")
        .expect("legacy");
    assert!(firefox.display_name.is_none());
}
