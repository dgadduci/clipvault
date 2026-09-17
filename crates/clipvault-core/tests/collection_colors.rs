// ---------------------------------------------------------------------
// `collection-colors-and-card-collection-labels` regression coverage.
//
// The `Collection.color_hex` column lets the sidebar render a colour
// square per collection and each history card render a coloured
// membership label. The tests pin the contract the new
// `set_collection_color` path, the migration backfill and the
// metadata-only invariants the rest of the desktop depends on.
// ---------------------------------------------------------------------

use std::sync::Arc;

use clipvault_core::{AppBootstrap, AppContext, Clock, OrganizationServiceError};
use clipvault_db::{ContentType, EntryRepository, NewEntry, OrganizationError};
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

fn reopen_isolated_context(dir: &TempDir, when: time::OffsetDateTime) -> AppContext {
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    AppBootstrap::new()
        .with_clock(Arc::new(FixedClock { instant: when }))
        .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()))
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("reopen")
}

fn insert_entry(context: &AppContext, content: &str, when: time::OffsetDateTime) -> i64 {
    let new = NewEntry::text(
        content.to_string(),
        ContentType::Text,
        content.len() as i64,
        format!("hash::{content}"),
        Some("test".to_string()),
        when,
        when,
    );
    let mut db = context.database().lock();
    let mut repo = EntryRepository::new(db.connection_mut());
    repo.insert_or_touch(new).expect("insert").record().id
}

fn with_conn<R>(context: &AppContext, f: impl FnOnce(&Connection) -> R) -> R {
    let db = context.database().lock();
    let conn = db.connection();
    f(conn)
}

#[test]
fn history_collection_is_seeded_with_default_color() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let collections = context
        .organization()
        .list_collections(&context)
        .expect("list");
    let history = collections
        .iter()
        .find(|c| c.stable_key.as_deref() == Some(clipvault_db::HISTORY_STABLE_KEY))
        .expect("Historial row");
    assert_eq!(
        history.color_hex,
        clipvault_db::HISTORY_DEFAULT_COLOR_HEX,
        "Historial must keep its default colour after bootstrap",
    );
    assert!(history.color_hex.starts_with('#'));
    assert_eq!(history.color_hex.len(), 7);
}

#[test]
fn create_collection_assigns_a_color_from_the_palette() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    for index in 0..16 {
        let name = format!("Trabajo {index}");
        let created = context
            .organization()
            .create_collection(&context, &name)
            .expect("create");
        let palette = clipvault_db::DEFAULT_COLLECTION_PALETTE;
        assert!(
            palette.iter().any(|hex| *hex == created.color_hex),
            "new collection colour must come from the base palette; got {}",
            created.color_hex,
        );
    }
}

#[test]
fn set_collection_color_normalises_hex() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let created = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create");
    let updated = context
        .organization()
        .set_collection_color(&context, created.id, "#AABBCC")
        .expect("set color");
    assert_eq!(updated.color_hex, "#aabbcc");
}

#[test]
fn set_collection_color_rejects_invalid_hex() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let created = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create");
    let previous = created.color_hex.clone();

    for bad in [
        "red",
        "#aabbccdd",
        "#xyz",
        "aabbcc",
        "#aabbc",
        "#gggggg",
        "",
        "##aabbcc",
        "#12345",
    ] {
        let error = context
            .organization()
            .set_collection_color(&context, created.id, bad)
            .unwrap_err();
        assert!(
            matches!(
                error,
                OrganizationServiceError::Repository(OrganizationError::InvalidCollectionColor(_))
            ),
            "{bad:?} must be rejected; got {error:?}",
        );
    }

    let after = context
        .organization()
        .find_collection(&context, created.id)
        .expect("find")
        .expect("row");
    assert_eq!(
        after.color_hex, previous,
        "rejected writes must never mutate the persisted colour",
    );
}

#[test]
fn set_collection_color_is_idempotent() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let created = context
        .organization()
        .set_collection_color(&context, 1, "#1565c0")
        .expect("first");
    let again = context
        .organization()
        .set_collection_color(&context, 1, "#1565c0")
        .expect("idempotent");
    assert_eq!(created.color_hex, again.color_hex);
    assert_eq!(
        created.updated_at, again.updated_at,
        "idempotent write must not bump updated_at",
    );
}

#[test]
fn set_collection_color_rejects_unknown_collection() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let error = context
        .organization()
        .set_collection_color(&context, 9_999, "#1565c0")
        .unwrap_err();
    assert!(matches!(
        error,
        OrganizationServiceError::Repository(OrganizationError::CollectionNotFound(9_999))
    ));
}

#[test]
fn set_collection_color_on_historial_is_allowed() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history id");
    let entry_id = insert_entry(&context, "captured", datetime!(2026-01-02 03:04:05 UTC));
    let before_members = context
        .organization()
        .entry_collection_ids(&context, entry_id)
        .expect("ids");

    let updated = context
        .organization()
        .set_collection_color(&context, history_id, "#c62828")
        .expect("color update");

    assert_eq!(updated.id, history_id);
    assert_eq!(updated.color_hex, "#c62828");
    assert_eq!(updated.kind, clipvault_db::CollectionKind::System);

    let after_members = context
        .organization()
        .entry_collection_ids(&context, entry_id)
        .expect("ids");
    assert_eq!(
        before_members, after_members,
        "colour update must never touch the entry-collection association",
    );

    // Rename / delete guards still reject changes that would
    // weaken the protected identity.
    let rename_error = context
        .organization()
        .rename_collection(&context, history_id, "Otro")
        .unwrap_err();
    assert!(matches!(
        rename_error,
        OrganizationServiceError::Repository(OrganizationError::SystemCollectionProtected(_))
    ));
    let delete_error = context
        .organization()
        .delete_collection(&context, history_id)
        .unwrap_err();
    assert!(matches!(
        delete_error,
        OrganizationServiceError::Repository(OrganizationError::SystemCollectionProtected(_))
    ));
}

#[test]
fn color_update_never_modifies_entries_tags_memberships_or_favorites() {
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let (_dir, context) = bootstrap_with_clock(when);
    let text_id = insert_entry(&context, "captured", when);
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    let tag_id = context
        .organization()
        .upsert_tag(&context, "draft")
        .expect("tag")
        .id;
    context
        .organization()
        .replace_entry_collections(&context, text_id, &[trabajo_id])
        .expect("attach collection");
    context
        .organization()
        .replace_entry_tags(&context, text_id, &[tag_id])
        .expect("attach tag");
    context
        .management()
        .set_favorite(&context, text_id, true)
        .expect("pin");

    let snapshot_before: (String, i64, Vec<i64>, Vec<i64>, Vec<i64>) =
        with_conn(&context, |conn| {
            let entry_hash: String = conn
                .query_row(
                    "SELECT content_hash FROM clipboard_entries WHERE id = ?1",
                    rusqlite::params![text_id],
                    |row| row.get(0),
                )
                .expect("hash");
            let entry_pinned: i64 = conn
                .query_row(
                    "SELECT is_pinned FROM clipboard_entries WHERE id = ?1",
                    rusqlite::params![text_id],
                    |row| row.get(0),
                )
                .expect("pin");
            let mut stmt = conn
                .prepare("SELECT entry_id FROM entry_collections ORDER BY entry_id, collection_id")
                .expect("stmt");
            let entries: Vec<i64> = stmt
                .query_map([], |row| row.get(0))
                .expect("query")
                .map(|r| r.expect("row"))
                .collect();
            let mut stmt = conn
                .prepare("SELECT entry_id FROM entry_tags ORDER BY entry_id, tag_id")
                .expect("stmt");
            let tags_rows: Vec<i64> = stmt
                .query_map([], |row| row.get(0))
                .expect("query")
                .map(|r| r.expect("row"))
                .collect();
            let mut stmt = conn
                .prepare("SELECT id FROM tags ORDER BY id")
                .expect("stmt");
            let tag_ids: Vec<i64> = stmt
                .query_map([], |row| row.get(0))
                .expect("query")
                .map(|r| r.expect("row"))
                .collect();
            (entry_hash, entry_pinned, entries, tags_rows, tag_ids)
        });

    let new_color = "#c62828";
    context
        .organization()
        .set_collection_color(&context, trabajo_id, new_color)
        .expect("set color");

    let snapshot_after: (String, i64, Vec<i64>, Vec<i64>, Vec<i64>) = with_conn(&context, |conn| {
        let entry_hash: String = conn
            .query_row(
                "SELECT content_hash FROM clipboard_entries WHERE id = ?1",
                rusqlite::params![text_id],
                |row| row.get(0),
            )
            .expect("hash");
        let entry_pinned: i64 = conn
            .query_row(
                "SELECT is_pinned FROM clipboard_entries WHERE id = ?1",
                rusqlite::params![text_id],
                |row| row.get(0),
            )
            .expect("pin");
        let mut stmt = conn
            .prepare("SELECT entry_id FROM entry_collections ORDER BY entry_id, collection_id")
            .expect("stmt");
        let entries: Vec<i64> = stmt
            .query_map([], |row| row.get(0))
            .expect("query")
            .map(|r| r.expect("row"))
            .collect();
        let mut stmt = conn
            .prepare("SELECT entry_id FROM entry_tags ORDER BY entry_id, tag_id")
            .expect("stmt");
        let tags_rows: Vec<i64> = stmt
            .query_map([], |row| row.get(0))
            .expect("query")
            .map(|r| r.expect("row"))
            .collect();
        let mut stmt = conn
            .prepare("SELECT id FROM tags ORDER BY id")
            .expect("stmt");
        let tag_ids: Vec<i64> = stmt
            .query_map([], |row| row.get(0))
            .expect("query")
            .map(|r| r.expect("row"))
            .collect();
        (entry_hash, entry_pinned, entries, tags_rows, tag_ids)
    });
    assert_eq!(
        snapshot_before, snapshot_after,
        "colour update must not change entries, tags, memberships or favourites",
    );

    let updated = context
        .organization()
        .find_collection(&context, trabajo_id)
        .expect("find")
        .expect("row");
    assert_eq!(updated.color_hex, new_color);
}

#[test]
fn color_update_persists_across_context_reopen() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let later = datetime!(2026-01-02 03:09:00 UTC);
    let created = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create");
    let colour = "#a16207";
    let updated = context
        .organization()
        .set_collection_color(&context, created.id, colour)
        .expect("set color");
    assert_eq!(updated.color_hex, colour);
    drop(context);

    let context = reopen_isolated_context(&_dir, later);
    let reopened = context
        .organization()
        .find_collection(&context, created.id)
        .expect("find")
        .expect("row");
    assert_eq!(
        reopened.color_hex, colour,
        "colour must survive the SQLite close + reopen cycle",
    );
}

#[test]
fn migration_backfills_existing_collections_with_blue() {
    // Bootstrap a database with an older schema (no `color_hex`
    // column), then run the colour migration through the normal
    // bootstrap path. The migration MUST seed every legacy row
    // with the documented fallback (`#1565c0`) without
    // rewriting any existing name, kind or timestamp.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("legacy.db");
    {
        use rusqlite::Connection;
        let conn = Connection::open(&path).expect("open");
        conn.execute_batch(
            "CREATE TABLE collections (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                stable_key TEXT UNIQUE,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            INSERT INTO collections (stable_key, name, kind, created_at, updated_at)
                VALUES ('history', 'Historial', 'system', '1970-01-01T00:00:00Z', '1970-01-01T00:00:00Z');
            INSERT INTO collections (stable_key, name, kind, created_at, updated_at)
                VALUES (NULL, 'Trabajo', 'user', '1970-01-01T00:00:00Z', '1970-01-01T00:00:00Z');",
        )
        .expect("seed");
    }

    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = clipvault_core::AppBootstrap::new()
        .with_clock(Arc::new(FixedClock {
            instant: datetime!(2026-01-02 03:04:05 UTC),
        }))
        .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()))
        .with_platform_adapters(adapters)
        .bootstrap_at(&path)
        .expect("bootstrap with migration");

    let collections = context
        .organization()
        .list_collections(&context)
        .expect("list");
    let history = collections
        .iter()
        .find(|c| c.stable_key.as_deref() == Some(clipvault_db::HISTORY_STABLE_KEY))
        .expect("Historial");
    let trabajo = collections
        .iter()
        .find(|c| c.name == "Trabajo")
        .expect("Trabajo");
    assert_eq!(history.color_hex, clipvault_db::HISTORY_DEFAULT_COLOR_HEX);
    assert_eq!(trabajo.color_hex, clipvault_db::HISTORY_DEFAULT_COLOR_HEX);
    assert_eq!(history.name, clipvault_db::HISTORY_DISPLAY_NAME);
    assert!(matches!(history.kind, clipvault_db::CollectionKind::System));
    assert_eq!(trabajo.name, "Trabajo");
    assert!(matches!(trabajo.kind, clipvault_db::CollectionKind::User));
    assert_eq!(
        trabajo.created_at, "1970-01-01T00:00:00Z",
        "the migration must not rewrite timestamps",
    );
}

#[test]
fn migration_apply_called_twice_idempotent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("idempotent.db");
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = clipvault_core::AppBootstrap::new()
        .with_clock(Arc::new(FixedClock {
            instant: datetime!(2026-01-02 03:04:05 UTC),
        }))
        .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()))
        .with_platform_adapters(adapters.clone())
        .bootstrap_at(&path)
        .expect("first bootstrap");
    let after_first = context
        .organization()
        .list_collections(&context)
        .expect("list");
    drop(context);

    // Re-bootstrap the same file path with the same adapters; the
    // migration runner MUST be idempotent so the colour column
    // and the documented backfill are not applied twice.
    let context = clipvault_core::AppBootstrap::new()
        .with_clock(Arc::new(FixedClock {
            instant: datetime!(2026-01-02 03:04:05 UTC),
        }))
        .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()))
        .with_platform_adapters(adapters)
        .bootstrap_at(&path)
        .expect("second bootstrap");
    let after_second = context
        .organization()
        .list_collections(&context)
        .expect("list");
    let first_signature: Vec<(i64, String, String, String)> = after_first
        .into_iter()
        .map(|c| (c.id, c.name, c.color_hex, c.kind.as_str().to_string()))
        .collect();
    let second_signature: Vec<(i64, String, String, String)> = after_second
        .into_iter()
        .map(|c| (c.id, c.name, c.color_hex, c.kind.as_str().to_string()))
        .collect();
    assert_eq!(
        first_signature, second_signature,
        "the colour migration must be idempotent across re-bootstraps",
    );
}
