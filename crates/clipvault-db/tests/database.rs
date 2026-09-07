use clipvault_db::{builtin_migrations, rollback_migration, Database, Migration, MigrationOutcome};
use rusqlite::Connection;
use tempfile::TempDir;

fn fresh_db() -> (TempDir, Database) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("clipvault.db");
    let db = Database::open(&db_path).expect("open db");
    (dir, db)
}

#[test]
fn open_creates_parent_directory_and_database_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let nested = dir.path().join("nested").join("clipvault.db");
    assert!(!nested.parent().unwrap().exists());

    let db = Database::open(&nested).expect("open db");
    drop(db);

    assert!(nested.exists(), "database file should be created");
}

#[test]
fn first_run_applies_every_builtin_migration() {
    let (_dir, mut db) = fresh_db();

    let outcomes = db.run_migrations(&builtin_migrations()).expect("migrate");
    assert_eq!(outcomes.len(), builtin_migrations().len());
    assert!(outcomes
        .iter()
        .all(|o| matches!(o, MigrationOutcome::Applied { .. })));
    assert_eq!(
        db.applied_migration_count().unwrap(),
        builtin_migrations().len()
    );
}

#[test]
fn reapplying_built_in_migrations_is_idempotent() {
    let (_dir, mut db) = fresh_db();
    db.run_migrations(&builtin_migrations()).expect("first run");

    let outcomes = db
        .run_migrations(&builtin_migrations())
        .expect("second run");
    assert!(outcomes
        .iter()
        .all(|o| matches!(o, MigrationOutcome::AlreadyApplied { .. })));
    assert_eq!(
        db.applied_migration_count().unwrap(),
        builtin_migrations().len()
    );
}

#[test]
fn custom_migration_persists_data() {
    let (_dir, mut db) = fresh_db();

    let migration = Migration {
        version: 100,
        description: "create notes",
        up_sql: "CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT NOT NULL);",
        down_sql: "DROP TABLE notes;",
    };

    db.run_migrations(std::slice::from_ref(&migration))
        .expect("apply");
    db.connection()
        .execute("INSERT INTO notes (body) VALUES (?1)", ["hello"])
        .expect("insert");

    let read = Connection::open(db.path()).expect("reopen");
    let body: String = read
        .query_row("SELECT body FROM notes", [], |row| row.get(0))
        .expect("query");
    assert_eq!(body, "hello");
}

#[test]
fn rollback_removes_schema_and_history_entry() {
    let (_dir, mut db) = fresh_db();

    let migration = Migration {
        version: 200,
        description: "create throwaway",
        up_sql: "CREATE TABLE throwaway (id INTEGER PRIMARY KEY);",
        down_sql: "DROP TABLE throwaway;",
    };

    db.run_migrations(std::slice::from_ref(&migration))
        .expect("apply");
    rollback_migration(&mut db, &migration).expect("rollback");

    let count = db.applied_migration_count().unwrap();
    assert_eq!(
        count, 0,
        "rollback should remove the schema_migrations entry"
    );
}

#[test]
fn default_database_path_lives_under_home() {
    let path = clipvault_db::default_database_path().unwrap();
    assert!(path.ends_with(".clipvault/clipvault.db"));
}

#[test]
fn clipboard_entries_migration_is_reversible() {
    let (_dir, mut db) = fresh_db();
    db.run_migrations(&builtin_migrations()).expect("apply");

    {
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        let now = time::macros::datetime!(2026-01-02 03:04:05 UTC);
        let outcome = repo
            .insert_or_touch(clipvault_db::NewEntry::text(
                "hello".to_string(),
                clipvault_db::ContentType::Text,
                5,
                "hash::hello".to_string(),
                Some("test".to_string()),
                now,
                now,
            ))
            .expect("insert");
        assert!(matches!(outcome, clipvault_db::EntryOutcome::Inserted(_)));
        assert_eq!(repo.count().unwrap(), 1);
    }

    let migration = builtin_migrations()
        .into_iter()
        .find(|m| m.version == 2)
        .expect("clipboard_entries migration registered");
    rollback_migration(&mut db, &migration).expect("rollback");

    // Migration 2 is rolled back; every later migration is still
    // recorded as applied even though the table migration 2 owned is
    // gone — the rollback intentionally does not chain. The expected
    // count is derived from the registry so appending a migration
    // does not require touching this assertion.
    let expected = builtin_migrations().len() - 1;
    assert_eq!(db.applied_migration_count().unwrap(), expected);

    let table_exists: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = ?1",
            ["clipboard_entries"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        table_exists, 0,
        "rollback should drop the clipboard_entries table"
    );
}

#[test]
fn history_management_migration_adds_is_pinned_and_indexes() {
    let (_dir, mut db) = fresh_db();
    db.run_migrations(&builtin_migrations()).expect("apply");

    // New column exists with the documented default.
    let pinned_default: i64 = db
        .connection()
        .query_row(
            "SELECT is_pinned FROM clipboard_entries LIMIT 0",
            [],
            |_| Ok(0),
        )
        .unwrap_or(0);
    let _ = pinned_default; // The column is present if the query prepares.

    let column_count: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('clipboard_entries')
             WHERE name = 'is_pinned'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(column_count, 1, "is_pinned column must be present");

    // Indexes introduced by the management migration are present.
    for index in [
        "idx_clipboard_entries_pinned_updated",
        "idx_clipboard_entries_created_at",
    ] {
        let exists: i64 = db
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'index' AND name = ?1",
                [index],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "index {index} should be created");
    }

    // Existing rows are treated as non-favorite.
    let now = time::macros::datetime!(2026-01-02 03:04:05 UTC);
    let id = {
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        let outcome = repo
            .insert_or_touch(clipvault_db::NewEntry::text(
                "preexisting".to_string(),
                clipvault_db::ContentType::Text,
                10,
                "hash::preexisting".to_string(),
                Some("legacy".to_string()),
                now,
                now,
            ))
            .expect("insert");
        assert!(
            !outcome.record().is_pinned,
            "new rows default to not pinned"
        );
        outcome.record().id
    };

    let is_pinned: i64 = db
        .connection()
        .query_row(
            "SELECT is_pinned FROM clipboard_entries WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(is_pinned, 0, "legacy rows should default to is_pinned = 0");
}

#[test]
fn history_management_migration_is_reversible() {
    let (_dir, mut db) = fresh_db();
    db.run_migrations(&builtin_migrations()).expect("apply");

    // Insert a row, then mark it as favorite so the rollback rebuild
    // has to drop the column without losing the rest of the data.
    let now = time::macros::datetime!(2026-01-02 03:04:05 UTC);
    {
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        repo.insert_or_touch(clipvault_db::NewEntry::text(
            "survives-rollback".to_string(),
            clipvault_db::ContentType::Text,
            16,
            "hash::survives".to_string(),
            Some("test".to_string()),
            now,
            now,
        ))
        .expect("insert");
    }
    let migration = builtin_migrations()
        .into_iter()
        .find(|m| m.version == 3)
        .expect("history_management migration registered");
    rollback_migration(&mut db, &migration).expect("rollback");

    // The applied-version row is removed; every other migration is
    // still recorded. Derived from the registry so appending a
    // migration does not require touching this assertion.
    let expected = builtin_migrations().len() - 1;
    let count = db.applied_migration_count().unwrap();
    assert_eq!(count, expected, "rollback should remove only version 3");

    // is_pinned column is gone after rollback.
    let column_count: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('clipboard_entries')
             WHERE name = 'is_pinned'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(column_count, 0, "is_pinned column must be dropped");

    // The original row is still readable. We avoid `repo.recent`
    // because its query still references `is_pinned`; a simple count
    // proves the data survived the rebuild.
    let count: i64 = db
        .connection()
        .query_row("SELECT COUNT(*) FROM clipboard_entries", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 1);
    let content: String = db
        .connection()
        .query_row("SELECT content FROM clipboard_entries LIMIT 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(content, "survives-rollback");
}

#[test]
fn card_layout_migration_adds_nullable_metadata_columns() {
    let (_dir, mut db) = fresh_db();
    db.run_migrations(&builtin_migrations()).expect("apply");

    for column in ["title", "source_app_name", "source_app_icon_ref"] {
        let column_count: i64 = db
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('clipboard_entries')
                 WHERE name = ?1",
                [column],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            column_count, 1,
            "{column} column must be present after migration 7"
        );
    }

    // Legacy rows keep `None` metadata and the row remains readable
    // through the repository without any rewrite.
    let now = time::macros::datetime!(2026-01-02 03:04:05 UTC);
    let id = {
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        repo.insert_or_touch(clipvault_db::NewEntry::text(
            "legacy".to_string(),
            clipvault_db::ContentType::Text,
            6,
            "hash::legacy-card-layout".to_string(),
            Some("com.apple.TextEdit".to_string()),
            now,
            now,
        ))
        .expect("insert")
        .record()
        .id
    };
    let record = {
        let repo = clipvault_db::EntryRepository::new(db.connection_mut());
        repo.find_by_id(id).expect("find").expect("present")
    };
    assert_eq!(record.title, None);
    assert_eq!(record.source_app_name, None);
    assert_eq!(record.source_app_icon_ref, None);
    assert_eq!(record.content, "legacy");
}

#[test]
fn card_layout_migration_is_reversible() {
    let (_dir, mut db) = fresh_db();
    db.run_migrations(&builtin_migrations()).expect("apply");

    let now = time::macros::datetime!(2026-01-02 03:04:05 UTC);
    let id = {
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        repo.insert_or_touch(clipvault_db::NewEntry::text(
            "rollback-survivor".to_string(),
            clipvault_db::ContentType::Url,
            18,
            "hash::rollback-card-layout".to_string(),
            Some("com.apple.Safari".to_string()),
            now,
            now,
        ))
        .expect("insert")
        .record()
        .id
    };

    let migration = builtin_migrations()
        .into_iter()
        .find(|m| m.version == 7)
        .expect("card_layout migration registered");
    rollback_migration(&mut db, &migration).expect("rollback");

    let column_count: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('clipboard_entries')
             WHERE name IN ('title', 'source_app_name', 'source_app_icon_ref')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(column_count, 0, "card-layout columns must be dropped");

    let content: String = db
        .connection()
        .query_row(
            "SELECT content FROM clipboard_entries WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(content, "rollback-survivor");
}

// ---------------------------------------------------------------------
// `clipboard-rich-content`: migration 8 (clipboard asset metadata).
//
// The suite proves the three guarantees the change contract requires:
// the migration is additive, a database created *before* image support
// keeps every textual row readable after the upgrade, and the rollback
// leaves a consistent pre-image-support schema.
// ---------------------------------------------------------------------

/// Migrations shipped before image support landed. Used to simulate a
/// user database created by an earlier ClipVault release.
fn migrations_before_clipboard_assets() -> Vec<Migration> {
    builtin_migrations()
        .into_iter()
        .filter(|m| m.version < 8)
        .collect()
}

#[test]
fn clipboard_assets_migration_adds_nullable_payload_columns() {
    let (_dir, mut db) = fresh_db();
    db.run_migrations(&builtin_migrations()).expect("apply");

    for column in ["asset_ref", "mime_type", "payload_width", "payload_height"] {
        let column_count: i64 = db
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('clipboard_entries')
                 WHERE name = ?1",
                [column],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            column_count, 1,
            "{column} column must be present after migration 8"
        );
        // Nullability is what keeps the migration compatible with
        // pre-existing rows: a `NOT NULL` column would require a
        // default and a rewrite.
        let notnull: i64 = db
            .connection()
            .query_row(
                "SELECT [notnull] FROM pragma_table_info('clipboard_entries')
                 WHERE name = ?1",
                [column],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(notnull, 0, "{column} must be nullable");
    }

    let index_exists: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'index' AND name = 'idx_clipboard_entries_asset_ref'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(index_exists, 1, "asset_ref index must be created");
}

#[test]
fn database_created_before_image_support_upgrades_without_data_loss() {
    let (_dir, mut db) = fresh_db();
    // 1. Simulate the pre-image-support release.
    db.run_migrations(&migrations_before_clipboard_assets())
        .expect("apply legacy migrations");

    let now = time::macros::datetime!(2026-01-02 03:04:05 UTC);
    // Insert through raw SQL on purpose: the pre-image-support binary
    // did not know about the four payload columns, so the legacy row
    // must be written with the *old* column list. Going through the
    // current repository would reference columns that do not exist
    // yet and would not reproduce a real user database.
    let legacy_id = {
        let conn = db.connection();
        conn.execute(
            "INSERT INTO clipboard_entries
                (content, content_type, content_size, content_hash,
                 source_app, is_pinned, created_at, updated_at, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?6, ?6)",
            rusqlite::params![
                "captured before image support",
                "url",
                29i64,
                "hash::legacy-pre-assets",
                "com.apple.Safari",
                now.format(&time::format_description::well_known::Rfc3339)
                    .expect("format"),
            ],
        )
        .expect("legacy insert");
        conn.last_insert_rowid()
    };

    // 2. Upgrade to the current schema.
    let outcomes = db.run_migrations(&builtin_migrations()).expect("upgrade");
    assert!(
        outcomes
            .iter()
            .any(|outcome| matches!(outcome, MigrationOutcome::Applied { version: 8, .. })),
        "migration 8 must be applied on upgrade, got {outcomes:?}"
    );

    // 3. The legacy row is readable, searchable and unchanged.
    let repo = clipvault_db::EntryRepository::new(db.connection_mut());
    let record = repo.find_by_id(legacy_id).expect("find").expect("present");
    assert_eq!(record.content, "captured before image support");
    assert_eq!(record.content_type, clipvault_db::ContentType::Url);
    assert_eq!(record.content_hash, "hash::legacy-pre-assets");
    assert_eq!(record.source_app.as_deref(), Some("com.apple.Safari"));
    // The new columns default to NULL — no rewrite, no sentinel.
    assert!(record.asset_ref.is_none());
    assert!(record.mime_type.is_none());
    assert!(record.payload_width.is_none());
    assert!(record.payload_height.is_none());
    assert!(!record.is_renderable_image());
    // Still part of the textual index.
    let textual = repo.text_entries().expect("text_entries");
    assert_eq!(textual.len(), 1);
    assert_eq!(textual[0].id, legacy_id);
    // No asset reference exists yet, so the collector sees nothing.
    assert!(repo.referenced_asset_refs().expect("refs").is_empty());
}

#[test]
fn clipboard_assets_migration_is_reversible_and_keeps_textual_rows() {
    let (_dir, mut db) = fresh_db();
    db.run_migrations(&builtin_migrations()).expect("apply");

    let now = time::macros::datetime!(2026-01-02 03:04:05 UTC);
    let hash = "9".repeat(64);
    let textual_id = {
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        let textual = repo
            .insert_or_touch(clipvault_db::NewEntry::text(
                "text survives rollback".to_string(),
                clipvault_db::ContentType::Text,
                22,
                "hash::text-survives-assets-rollback".to_string(),
                Some("com.apple.TextEdit".to_string()),
                now,
                now,
            ))
            .expect("insert text")
            .record()
            .id;
        repo.insert_or_touch(clipvault_db::NewEntry {
            content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
            content_type: clipvault_db::ContentType::Image,
            content_size: 2_048,
            content_hash: hash.clone(),
            source_app: Some("com.apple.Preview".to_string()),
            created_at: now,
            last_seen_at: now,
            asset_ref: Some(format!("clipboard/{hash}.png")),
            mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
            payload_width: Some(32),
            payload_height: Some(16),
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
            code_language: None,
        })
        .expect("insert image");
        textual
    };

    let migration = builtin_migrations()
        .into_iter()
        .find(|m| m.version == 8)
        .expect("clipboard_assets migration registered");
    rollback_migration(&mut db, &migration).expect("rollback");

    // The four payload columns and the index are gone.
    let column_count: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('clipboard_entries')
             WHERE name IN ('asset_ref', 'mime_type', 'payload_width', 'payload_height')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(column_count, 0, "payload columns must be dropped");

    // The textual row survives untouched, with its card-layout columns.
    let content: String = db
        .connection()
        .query_row(
            "SELECT content FROM clipboard_entries WHERE id = ?1",
            [textual_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(content, "text survives rollback");

    // The image row is dropped rather than left as an uninterpretable
    // `content_type = 'image'` row with an empty payload.
    let orphaned: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM clipboard_entries WHERE content_type = 'image'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(orphaned, 0, "image rows must not survive the rollback");

    let total: i64 = db
        .connection()
        .query_row("SELECT COUNT(*) FROM clipboard_entries", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(total, 1);
}

// ---------------------------------------------------------------------
// `tags-and-collections` migration: bootstrap, backfill, reversibility.
// ---------------------------------------------------------------------

/// Migrations shipped before the organization capability landed.
fn migrations_before_organization() -> Vec<Migration> {
    builtin_migrations()
        .into_iter()
        .filter(|m| m.version < 10)
        .collect()
}

#[test]
fn database_created_before_organization_upgrades_and_backfills_historial() {
    let (_dir, mut db) = fresh_db();
    // 1. Simulate the pre-organization release.
    db.run_migrations(&migrations_before_organization())
        .expect("apply legacy migrations");

    let now = time::macros::datetime!(2026-01-02 03:04:05 UTC);
    let mut legacy_ids = Vec::new();
    for (content, hash) in [
        ("legacy entry one", "hash::legacy-one"),
        ("legacy entry two", "hash::legacy-two"),
    ] {
        let conn = db.connection();
        conn.execute(
            "INSERT INTO clipboard_entries
                (content, content_type, content_size, content_hash,
                 source_app, is_pinned, created_at, updated_at, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?6, ?6)",
            rusqlite::params![
                content,
                "text",
                content.len() as i64,
                hash,
                "com.apple.Safari",
                now.format(&time::format_description::well_known::Rfc3339)
                    .expect("format"),
            ],
        )
        .expect("legacy insert");
        legacy_ids.push(conn.last_insert_rowid());
    }

    // 2. Upgrade to the current schema (migration 10 included).
    let outcomes = db.run_migrations(&builtin_migrations()).expect("upgrade");
    assert!(
        outcomes
            .iter()
            .any(|outcome| matches!(outcome, MigrationOutcome::Applied { version: 10, .. })),
        "migration 10 must be applied on upgrade, got {outcomes:?}"
    );

    // 3. `Historial` is seeded and every legacy entry belongs to it.
    let org = clipvault_db::OrganizationRepository::new(db.connection_mut());
    let history = org
        .system_collection_id(clipvault_db::HISTORY_STABLE_KEY)
        .expect("lookup")
        .expect("seeded");
    let members = org.entry_ids_in_history().expect("members");
    let mut sorted_members = members.clone();
    sorted_members.sort();
    let mut sorted_legacy = legacy_ids.clone();
    sorted_legacy.sort();
    assert_eq!(sorted_members, sorted_legacy);

    let collections = org.list_collections().expect("collections");
    let history_row = collections
        .iter()
        .find(|row| row.id == history)
        .expect("history row");
    assert!(history_row.is_system());
    assert_eq!(history_row.name, clipvault_db::HISTORY_DISPLAY_NAME);
}

#[test]
fn organization_migration_is_reversible_and_drops_associations() {
    let (_dir, mut db) = fresh_db();
    db.run_migrations(&builtin_migrations()).expect("apply");

    let now = time::macros::datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = {
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        repo.insert_or_touch(clipvault_db::NewEntry::text(
            "rollback me".into(),
            clipvault_db::ContentType::Text,
            11,
            "hash::rollback-me".into(),
            Some("com.apple.TextEdit".into()),
            now,
            now,
        ))
        .expect("insert")
        .record()
        .id
    };

    let organization_migration = builtin_migrations()
        .into_iter()
        .find(|m| m.version == 10)
        .expect("migration 10");
    rollback_migration(&mut db, &organization_migration).expect("rollback");

    let collections_count: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'collections'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(collections_count, 0, "collections table must be dropped");
    let entry_collections_count: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'entry_collections'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        entry_collections_count, 0,
        "entry_collections table must be dropped"
    );
    let _ = entry_id;
}
