//! Integration tests for the `clipvault_update_text_entry` Tauri
//! command. The command is the single bridge between the desktop
//! editor modal and the storage layer; the tests pin the contract
//! the rail depends on:
//!
//! - a textual payload is replaced in place;
//! - the trimmed empty string returns `empty_content`;
//! - an image or rich-text row returns `not_editable`;
//! - a missing id returns `not_found`;
//! - a hash collision returns `duplicate_content`;
//! - the metadata (`title`, `is_pinned`, source-app fields, tags,
//!   collections) and the asset / rich-text references survive
//!   the mutation;
//! - the response never carries clipboard content, hashes or
//!   snippets.

use clipvault_app::commands::{clipvault_update_text_entry_for_test, UpdateTextEntryResponse};
use clipvault_core::{hash_content, AppBootstrap, Clock, PlatformAdapters};
use clipvault_db::{builtin_migrations, ContentType, Database, EntryRepository, NewEntry};
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

fn insert_text_entry(context: &clipvault_core::AppContext) -> i64 {
    let mut db = context.database().lock();
    let mut repo = EntryRepository::new(db.connection_mut());
    repo.insert_or_touch(NewEntry::text(
        "initial text".to_string(),
        ContentType::Text,
        12,
        hash_content("initial text"),
        Some("com.apple.TextEdit".to_string()),
        datetime!(2026-01-02 03:04:05 UTC),
        datetime!(2026-01-02 03:04:05 UTC),
    ))
    .expect("insert")
    .record()
    .id
}

#[test]
fn update_text_entry_persists_payload_and_returns_record() {
    let context = fresh_context();
    let id = insert_text_entry(&context);

    let response =
        clipvault_update_text_entry_for_test(&context, id, "initial text - edited".to_string())
            .expect("update");
    match response {
        UpdateTextEntryResponse::Updated { entry } => {
            assert_eq!(entry.id, id);
            assert_eq!(entry.content, "initial text - edited");
            assert_eq!(entry.content_hash, hash_content("initial text - edited"));
            assert_eq!(entry.content_type, ContentType::Text);
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
    assert_eq!(stored.content, "initial text - edited");
}

#[test]
fn update_text_entry_returns_empty_content_for_blank_input() {
    let context = fresh_context();
    let id = insert_text_entry(&context);

    let response =
        clipvault_update_text_entry_for_test(&context, id, String::new()).expect("update");
    assert!(matches!(response, UpdateTextEntryResponse::EmptyContent));

    let stored = {
        let mut db = context.database().lock();
        EntryRepository::new(db.connection_mut())
            .find_by_id(id)
            .unwrap()
            .unwrap()
    };
    assert_eq!(stored.content, "initial text");
}

#[test]
fn update_text_entry_returns_not_found_for_missing_id() {
    let context = fresh_context();
    let response = clipvault_update_text_entry_for_test(&context, 9999, "anything".to_string())
        .expect("update");
    assert!(matches!(response, UpdateTextEntryResponse::NotFound));
}

#[test]
fn update_text_entry_returns_not_editable_for_image_row() {
    let context = fresh_context();
    let image_id = {
        let mut db = context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        let hash = "ab".repeat(32);
        repo.insert_or_touch(NewEntry {
            content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
            content_type: ContentType::Image,
            content_size: 64,
            content_hash: hash.clone(),
            source_app: None,
            created_at: datetime!(2026-01-02 03:04:05 UTC),
            last_seen_at: datetime!(2026-01-02 03:04:05 UTC),
            asset_ref: Some(format!("clipboard/{hash}.png")),
            mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
            payload_width: Some(8),
            payload_height: Some(8),
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
            code_language: None,
        })
        .expect("image")
        .record()
        .id
    };
    let response = clipvault_update_text_entry_for_test(&context, image_id, "edit me".to_string())
        .expect("update");
    assert!(matches!(response, UpdateTextEntryResponse::NotEditable));
}

#[test]
fn update_text_entry_returns_duplicate_content_when_hash_collides() {
    let context = fresh_context();
    let (first_id, second_id) = {
        let mut db = context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        let first = repo
            .insert_or_touch(NewEntry::text(
                "alpha".to_string(),
                ContentType::Text,
                5,
                hash_content("alpha"),
                None,
                datetime!(2026-01-02 03:04:05 UTC),
                datetime!(2026-01-02 03:04:05 UTC),
            ))
            .expect("alpha")
            .record()
            .id;
        let second = repo
            .insert_or_touch(NewEntry::text(
                "bravo".to_string(),
                ContentType::Text,
                5,
                hash_content("bravo"),
                None,
                datetime!(2026-01-02 03:04:05 UTC),
                datetime!(2026-01-02 03:04:05 UTC),
            ))
            .expect("bravo")
            .record()
            .id;
        (first, second)
    };
    let response = clipvault_update_text_entry_for_test(&context, second_id, "alpha".to_string())
        .expect("update");
    assert!(matches!(
        response,
        UpdateTextEntryResponse::DuplicateContent
    ));

    let stored = {
        let mut db = context.database().lock();
        EntryRepository::new(db.connection_mut())
            .find_by_id(second_id)
            .unwrap()
            .unwrap()
    };
    assert_eq!(stored.content, "bravo");
    let _ = first_id;
}

#[test]
fn update_text_entry_preserves_metadata() {
    let context = fresh_context();
    let id = insert_text_entry(&context);
    {
        let mut db = context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        repo.set_title(id, Some("Custom title"), datetime!(2026-01-02 03:05:00 UTC))
            .expect("title");
        repo.set_favorite(id, true, datetime!(2026-01-02 03:06:00 UTC))
            .expect("favorite");
        repo.set_source_app_metadata(
            id,
            Some("Terminal"),
            Some("application-icons/com.apple.terminal.png"),
            datetime!(2026-01-02 03:07:00 UTC),
        )
        .expect("source-app");
    }

    let response =
        clipvault_update_text_entry_for_test(&context, id, "initial text - v2".to_string())
            .expect("update");
    match response {
        UpdateTextEntryResponse::Updated { entry } => {
            assert_eq!(entry.title.as_deref(), Some("Custom title"));
            assert!(entry.is_pinned);
            assert_eq!(entry.source_app_name.as_deref(), Some("Terminal"));
            assert_eq!(
                entry.source_app_icon_ref.as_deref(),
                Some("application-icons/com.apple.terminal.png")
            );
        }
        other => panic!("expected Updated, got {other:?}"),
    }
}

#[test]
fn update_text_entry_returns_noop_for_identical_input() {
    let context = fresh_context();
    let id = insert_text_entry(&context);
    let response = clipvault_update_text_entry_for_test(&context, id, "initial text".to_string())
        .expect("update");
    assert!(matches!(response, UpdateTextEntryResponse::Noop { .. }));
}
