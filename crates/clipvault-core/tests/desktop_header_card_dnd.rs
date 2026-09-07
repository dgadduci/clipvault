//! Integration coverage for the `desktop-header-card-dnd` change.
//!
//! The Rust layer exposes two helpers the frontend relies on:
//!
//!   - `OrganizationService::replace_entry_collections` (Tauri
//!     command `clipvault_entry_collections_set`): the additive
//!     combine the drag-and-drop flow uses to attach a card to a
//!     user collection. The tests pin the invariants the spec
//!     demands: the call is additive, idempotent, never drops
//!     `Historial` and never touches the entry's image metadata.
//!   - `OrganizationService::entry_collection_ids` + the entries
//!     round-trip: when the application restarts, every image row
//!     must surface `asset_ref`, `mime_type`, `payload_width`,
//!     `payload_height` and the current collection membership
//!     unchanged. The tests open the same database twice (the
//!     `bootstrap_at` helper writes to a real file) to prove the
//!     metadata survives the close/reopen cycle the drag and pin
//!     flows cannot accidentally disturb.
//!
//! The privacy contract is enforced at the repository level:
//! nothing in these tests ever echoes clipboard content, snippets,
//! hashes, asset references or bytes into a log line.

use std::sync::Arc;

use clipvault_core::{AppBootstrap, AppContext, Clock};
use clipvault_db::{ContentType, EntryRepository, NewEntry};
use rusqlite::Connection;
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

fn bootstrap_with_clock(when: time::OffsetDateTime) -> (TempDir, AppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(Arc::new(FixedClock { instant: when }))
        .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()))
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    (dir, context)
}

fn bootstrap_existing(path: &std::path::Path) -> AppContext {
    let dir = path.parent().expect("parent").to_path_buf();
    let adapters = clipvault_core::build_isolated_adapters(&dir, &dir.join("data"));
    AppBootstrap::new()
        .with_clock(Arc::new(FixedClock {
            instant: datetime!(2026-01-02 03:04:05 UTC),
        }))
        .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()))
        .with_platform_adapters(adapters)
        .bootstrap_at(path)
        .expect("reopen")
}

fn insert_image_entry(
    context: &AppContext,
    hash: &str,
    width: u32,
    height: u32,
    when: time::OffsetDateTime,
) -> i64 {
    let new = NewEntry {
        content: String::new(),
        content_type: ContentType::Image,
        content_size: 1_024,
        content_hash: hash.to_string(),
        source_app: Some("test-app".to_string()),
        created_at: when,
        last_seen_at: when,
        asset_ref: Some(format!("clipboard/{hash}.png")),
        mime_type: Some("image/png".to_string()),
        payload_width: Some(width),
        payload_height: Some(height),
        rich_text_hash: None,
        rich_html_ref: None,
        rich_rtf_ref: None,
        rich_preview_ref: None,
        rich_html_size: None,
        rich_rtf_size: None,
        code_language: None,
    };
    let mut db = context.database().lock();
    let mut repo = EntryRepository::new(db.connection_mut());
    repo.insert_or_touch(new).expect("insert").record().id
}

fn with_conn<R>(context: &AppContext, f: impl FnOnce(&Connection) -> R) -> R {
    let db = context.database().lock();
    let conn = db.connection();
    f(conn)
}

// ---------------------------------------------------------------------
// `entry_collections_set` semantics — the additive combine the drag
// flow uses to attach a card to a user collection.
// ---------------------------------------------------------------------

#[test]
fn replace_entry_collections_adds_target_without_dropping_existing_memberships() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history id");
    let entry_id = insert_image_entry(
        &context,
        &"a".repeat(64),
        640,
        480,
        datetime!(2026-01-02 03:04:05 UTC),
    );

    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    let merged = context
        .organization()
        .replace_entry_collections(&context, entry_id, &[trabajo_id])
        .expect("merge");
    // `replace_entry_collections` returns the merged list in the
    // order it wrote the rows: target + Historial.
    assert_eq!(merged, vec![trabajo_id, history_id]);

    // `entry_collection_ids` is sorted by collection id, so the
    // history row (created first) precedes the user collection.
    let membership = context
        .organization()
        .entry_collection_ids(&context, entry_id)
        .expect("ids");
    assert_eq!(membership, vec![history_id, trabajo_id]);

    // Drop is idempotent: a second call with the same set keeps the
    // single membership per collection (no duplicate row).
    let merged_again = context
        .organization()
        .replace_entry_collections(&context, entry_id, &[trabajo_id])
        .expect("merge again");
    assert_eq!(merged_again, vec![trabajo_id, history_id]);
    let membership_again = context
        .organization()
        .entry_collection_ids(&context, entry_id)
        .expect("ids");
    assert_eq!(membership_again, vec![history_id, trabajo_id]);
}

#[test]
fn replace_entry_collections_always_reattaches_historial() {
    // Regression: a payload that forgets the system collection MUST
    // not orphan the entry. The repository reattaches `Historial`
    // atomically so the entry is never left without the protected
    // membership.
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history id");
    let entry_id = insert_image_entry(
        &context,
        &"b".repeat(64),
        8,
        8,
        datetime!(2026-01-02 03:04:05 UTC),
    );

    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    let clientes_id = context
        .organization()
        .create_collection(&context, "Clientes")
        .expect("create")
        .id;
    let merged = context
        .organization()
        .replace_entry_collections(&context, entry_id, &[trabajo_id, clientes_id])
        .expect("merge");
    assert_eq!(merged, vec![trabajo_id, clientes_id, history_id]);

    let membership = context
        .organization()
        .entry_collection_ids(&context, entry_id)
        .expect("ids");
    assert!(membership.contains(&history_id));
    assert!(membership.contains(&trabajo_id));
    assert!(membership.contains(&clientes_id));
}

#[test]
fn replace_entry_collections_never_touches_image_metadata() {
    // The drag flow merges collection ids; it must not write to
    // any of the image payload metadata columns. The repository
    // contract enforces this — the merge only touches
    // `entry_collections` — but pinning it here makes a
    // regression that widened the SQL surface fail loudly.
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let entry_id = insert_image_entry(
        &context,
        &"c".repeat(64),
        320,
        240,
        datetime!(2026-01-02 03:04:05 UTC),
    );

    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    context
        .organization()
        .replace_entry_collections(&context, entry_id, &[trabajo_id])
        .expect("merge");

    with_conn(&context, |conn| {
        let (asset_ref, mime_type, width, height): (
            Option<String>,
            Option<String>,
            Option<u32>,
            Option<u32>,
        ) = conn
            .query_row(
                "SELECT asset_ref, mime_type, payload_width, payload_height
                 FROM clipboard_entries WHERE id = ?1",
                rusqlite::params![entry_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("select");
        assert_eq!(
            asset_ref.as_deref(),
            Some(format!("clipboard/{}.png", "c".repeat(64)).as_str())
        );
        assert_eq!(mime_type.as_deref(), Some("image/png"));
        assert_eq!(width, Some(320));
        assert_eq!(height, Some(240));
    });
}

// ---------------------------------------------------------------------
// Image persistence: every image row's metadata survives a
// restart. The test closes the database and reopens it through a
// fresh `AppContext`; the new context sees the same rows the
// original insert produced.
// ---------------------------------------------------------------------

#[test]
fn image_rows_survive_application_restart_with_payload_metadata_intact() {
    let (dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history id");
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    let entry_id = insert_image_entry(
        &context,
        &"d".repeat(64),
        1024,
        768,
        datetime!(2026-01-02 03:04:05 UTC),
    );
    context
        .organization()
        .replace_entry_collections(&context, entry_id, &[trabajo_id])
        .expect("merge");

    // Simulate a restart: drop the original context and re-bootstrap
    // against the same database file.
    drop(context);
    let reopened = bootstrap_existing(&dir.path().join("clipvault.db"));

    let membership = reopened
        .organization()
        .entry_collection_ids(&reopened, entry_id)
        .expect("ids");
    assert_eq!(membership, vec![history_id, trabajo_id]);

    with_conn(&reopened, |conn| {
        let (asset_ref, mime_type, width, height, content_size): (
            Option<String>,
            Option<String>,
            Option<u32>,
            Option<u32>,
            i64,
        ) = conn
            .query_row(
                "SELECT asset_ref, mime_type, payload_width, payload_height, content_size
                 FROM clipboard_entries WHERE id = ?1",
                rusqlite::params![entry_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .expect("select after reopen");
        assert_eq!(
            asset_ref.as_deref(),
            Some(format!("clipboard/{}.png", "d".repeat(64)).as_str())
        );
        assert_eq!(mime_type.as_deref(), Some("image/png"));
        assert_eq!(width, Some(1024));
        assert_eq!(height, Some(768));
        assert_eq!(content_size, 1_024);
    });
}

// ---------------------------------------------------------------------
// Pin/unpin must NOT touch the membership set the drag flow just
// wrote. The favorite command is metadata-only.
// ---------------------------------------------------------------------

#[test]
fn pinning_an_image_entry_does_not_drop_collection_memberships() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history id");
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    let entry_id = insert_image_entry(
        &context,
        &"e".repeat(64),
        200,
        100,
        datetime!(2026-01-02 03:04:05 UTC),
    );
    context
        .organization()
        .replace_entry_collections(&context, entry_id, &[trabajo_id])
        .expect("merge");

    // Pin / unpin via the management service. The favourite
    // command is the same path the Tauri command
    // `clipvault_set_favorite` exercises.
    let pinned = context
        .management()
        .set_favorite(&context, entry_id, true)
        .expect("pin");
    assert_eq!(pinned.kind(), "updated");
    assert!(pinned.entry.is_some());
    let membership = context
        .organization()
        .entry_collection_ids(&context, entry_id)
        .expect("ids");
    assert_eq!(membership, vec![history_id, trabajo_id]);

    let unpinned = context
        .management()
        .set_favorite(&context, entry_id, false)
        .expect("unpin");
    assert_eq!(unpinned.kind(), "updated");
    assert!(unpinned.entry.is_some());
    let membership_after = context
        .organization()
        .entry_collection_ids(&context, entry_id)
        .expect("ids");
    assert_eq!(membership_after, vec![history_id, trabajo_id]);

    // Image metadata also survives the pin/unpin round.
    with_conn(&context, |conn| {
        let (asset_ref, mime_type, width, height): (
            Option<String>,
            Option<String>,
            Option<u32>,
            Option<u32>,
        ) = conn
            .query_row(
                "SELECT asset_ref, mime_type, payload_width, payload_height
                 FROM clipboard_entries WHERE id = ?1",
                rusqlite::params![entry_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("select");
        assert_eq!(
            asset_ref.as_deref(),
            Some(format!("clipboard/{}.png", "e".repeat(64)).as_str())
        );
        assert_eq!(mime_type.as_deref(), Some("image/png"));
        assert_eq!(width, Some(200));
        assert_eq!(height, Some(100));
    });
}

// ---------------------------------------------------------------------
// `Historial` is protected: a drop on the system collection is a
// no-op the repository never accepts. The drag handler surfaces a
// `system_collection` no-op; the backend here simply never receives
// a path that forgets the entry.
// ---------------------------------------------------------------------

#[test]
fn remove_entry_from_collection_refuses_historial() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history id");
    let entry_id = insert_image_entry(
        &context,
        &"f".repeat(64),
        16,
        16,
        datetime!(2026-01-02 03:04:05 UTC),
    );

    let error = context
        .organization()
        .remove_entry_from_collection(&context, entry_id, history_id)
        .unwrap_err();
    assert!(matches!(
        error,
        clipvault_core::OrganizationServiceError::Repository(
            clipvault_db::OrganizationError::SystemCollectionProtected(_)
        )
    ));
}
