use std::sync::Arc;

use clipvault_core::{
    AppBootstrap, AppContext, Clipboard, ClipboardError, Clock, DiagnosticsService, FakeClipboard,
    SystemClock,
};
use clipvault_db::builtin_migrations;
use parking_lot::Mutex;
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

fn bootstrap_in(dir: &TempDir) -> AppContext {
    let clock = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    AppBootstrap::new()
        .with_clock(clock)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap")
}

#[test]
fn bootstrap_runs_built_in_migrations() {
    let dir = tempdir();
    let context = bootstrap_in(&dir);

    let db = context.database().lock();
    assert_eq!(
        db.applied_migration_count().unwrap(),
        builtin_migrations().len()
    );
    assert!(db.path().ends_with("clipvault.db"));
}

#[test]
fn bootstrap_is_repeatable_against_existing_database() {
    let dir = tempdir();
    let _ = bootstrap_in(&dir);
    // Re-bootstrapping should succeed (the database already exists and the
    // migrations are already applied).
    let context = bootstrap_in(&dir);
    let db = context.database().lock();
    assert_eq!(
        db.applied_migration_count().unwrap(),
        builtin_migrations().len()
    );
}

#[test]
fn diagnostics_reflect_bootstrap_state() {
    let dir = tempdir();
    let context = bootstrap_in(&dir);

    let diag = DiagnosticsService::snapshot(&context);
    assert_eq!(diag.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(diag.migrations_applied, builtin_migrations().len());
    assert_eq!(diag.started_at, "2026-01-02T03:04:05Z");
    assert!(diag.database_path.ends_with("clipvault.db"));
}

#[test]
fn fake_clock_overrides_system_clock() {
    let dir = tempdir();
    let clock = Arc::new(FixedClock {
        instant: datetime!(2030-12-31 23:59:59 UTC),
    });
    let context = AppBootstrap::new()
        .with_clock(clock)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    assert_eq!(
        DiagnosticsService::snapshot(&context).started_at,
        "2030-12-31T23:59:59Z"
    );
}

#[test]
fn system_clock_returns_utc() {
    let before = time::OffsetDateTime::now_utc();
    let now = SystemClock.now();
    let after = time::OffsetDateTime::now_utc();
    assert!(now >= before && now <= after);
}

#[test]
fn fake_clipboard_does_not_touch_real_clipboard() {
    let cb = FakeClipboard::with_text("hello world");
    let read = <dyn Clipboard>::read_text(&cb).unwrap();
    assert_eq!(read.as_deref(), Some("hello world"));
    cb.clear();
    let err = <dyn Clipboard>::read_text(&cb).unwrap();
    assert_eq!(err, None);
    let _ = ClipboardError::Empty; // keep the symbol exported for downstream
    let _ = Mutex::new(()); // ensure parking_lot stays a direct dependency for app layer
}
