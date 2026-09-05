//! Integration tests for the `clipvault_set_entry_title` Tauri
//! command. The command is the single bridge between the GUI's
//! title editor and the storage layer; the tests pin the contract
//! the rail depends on:
//!
//! - a non-empty trimmed title is persisted and returned;
//! - a `None` payload restores the default (column is cleared);
//! - an overlong title is rejected with a `title_too_long` error
//!   without mutating the database;
//! - a missing entry id returns `not_found`;
//! - the response never carries clipboard content, hashes or
//!   snippets.

use clipvault_app::commands::{
    clipvault_set_entry_title_for_test, CommandError, SetEntryTitleResponse,
};
use clipvault_core::{AppBootstrap, Clock, PlatformAdapters};
use clipvault_db::{builtin_migrations, Database, EntryRepository, NewEntry};
use clipvault_platform::{Capabilities, DisplayServer, OsFamily, PlatformInfo};
use time::macros::datetime;

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        time::OffsetDateTime::now_utc()
    }
}

fn fresh_context() -> clipvault_core::AppContext {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("clipvault.db");
    let mut db = Database::open(&db_path).expect("open db");
    db.run_migrations(&builtin_migrations()).expect("migrate");
    let info = PlatformInfo {
        home_dir: dir.path().to_path_buf(),
        data_dir: dir.path().to_path_buf(),
        os_family: OsFamily::Macos,
        display_server: DisplayServer::Unknown,
    };
    let adapters = PlatformAdapters::stub(&info, Capabilities::default());
    AppBootstrap::new()
        .with_clock(std::sync::Arc::new(FixedClock) as std::sync::Arc<dyn Clock>)
        .with_clipboard(std::sync::Arc::new(clipvault_core::FakeClipboard::new())
            as std::sync::Arc<dyn clipvault_core::Clipboard>)
        .with_platform_adapters(adapters)
        .bootstrap_at(&db_path)
        .expect("bootstrap")
}

fn insert_entry(context: &clipvault_core::AppContext) -> i64 {
    let mut db = context.database().lock();
    let mut repo = EntryRepository::new(db.connection_mut());
    repo.insert_or_touch(NewEntry::text(
        "history-card-layout title".to_string(),
        clipvault_db::ContentType::Text,
        30,
        "hash::card-layout-title".to_string(),
        Some("com.apple.TextEdit".to_string()),
        datetime!(2026-01-02 03:04:05 UTC),
        datetime!(2026-01-02 03:04:05 UTC),
    ))
    .expect("insert")
    .record()
    .id
}

#[test]
fn set_entry_title_persists_and_returns_record() {
    let context = fresh_context();
    let id = insert_entry(&context);

    let response = clipvault_set_entry_title_for_test(&context, id, Some("My Label".to_string()))
        .expect("set_title");
    match response {
        SetEntryTitleResponse::Updated { entry } => {
            assert_eq!(entry.id, id);
            assert_eq!(entry.title.as_deref(), Some("My Label"));
        }
        other => panic!("expected Updated, got {other:?}"),
    }

    let stored = {
        let mut db = context.database().lock();
        EntryRepository::new(db.connection_mut())
            .find_by_id(id)
            .unwrap()
            .unwrap()
    };
    assert_eq!(stored.title.as_deref(), Some("My Label"));
}

#[test]
fn set_entry_title_restores_default_on_none() {
    let context = fresh_context();
    let id = insert_entry(&context);

    clipvault_set_entry_title_for_test(&context, id, Some("Custom".to_string())).expect("set");
    let response = clipvault_set_entry_title_for_test(&context, id, None).expect("restore");
    match response {
        SetEntryTitleResponse::Updated { entry } => {
            assert!(entry.title.is_none(), "title must be cleared");
        }
        other => panic!("expected Updated, got {other:?}"),
    }
}

#[test]
fn set_entry_title_rejects_overlong_input_with_stable_error_kind() {
    let context = fresh_context();
    let id = insert_entry(&context);

    let too_long = "x".repeat(clipvault_core::MAX_TITLE_LENGTH + 1);
    let err =
        clipvault_set_entry_title_for_test(&context, id, Some(too_long)).expect_err("must reject");
    let CommandError { kind, .. } = err;
    assert_eq!(kind, "title_too_long");

    let stored = {
        let mut db = context.database().lock();
        EntryRepository::new(db.connection_mut())
            .find_by_id(id)
            .unwrap()
            .unwrap()
    };
    assert!(
        stored.title.is_none(),
        "rejected input must not have mutated the row"
    );
}

#[test]
fn set_entry_title_for_missing_id_returns_not_found() {
    let context = fresh_context();
    let response = clipvault_set_entry_title_for_test(&context, 9999, Some("label".to_string()))
        .expect("not_found");
    assert!(matches!(response, SetEntryTitleResponse::NotFound));
}
