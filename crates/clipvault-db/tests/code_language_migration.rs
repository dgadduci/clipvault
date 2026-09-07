//! Migration coverage for the `code-language-detection` capability.
//!
//! The tests pin the contract documented in the OpenSpec change:
//! legacy rows survive the new column, the canonical allowlist is
//! reachable from the public surface, and the index supports the
//! metadata-only access patterns the rail and Quick Paste rely on.

use clipvault_db::{builtin_migrations, ContentType, Database, NewEntry};
use tempfile::TempDir;
use time::macros::datetime;

fn open_temp_db() -> (TempDir, Database) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(dir.path().join("clipvault.db")).expect("open");
    let mut db = db;
    db.run_migrations(&builtin_migrations()).expect("migrate");
    (dir, db)
}

fn open_legacy_db(legacy_version: i64) -> (TempDir, Database) {
    // Open the database, run only the migrations up to (and
    // including) `legacy_version`, then return the handle so the
    // test can exercise the new column on top of a pre-existing
    // shape.
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(dir.path().join("clipvault.db")).expect("open");
    let mut db = db;
    let migrations = builtin_migrations()
        .into_iter()
        .filter(|migration| migration.version <= legacy_version)
        .collect::<Vec<_>>();
    db.run_migrations(&migrations).expect("legacy migrate");
    (dir, db)
}

#[test]
fn migration_is_purely_additive_for_legacy_text_rows() {
    // The migration MUST leave every pre-existing textual row with
    // `code_language = NULL` and the same content_type / content the
    // row was created with. The test seeds a row before the
    // migration is applied using the legacy schema (no
    // `code_language` column yet) and asserts the schema upgrade
    // does not touch the persisted payload.
    let (_dir, mut db) = open_legacy_db(10);
    let legacy_id = {
        let conn = db.connection_mut();
        conn.execute(
            "INSERT INTO clipboard_entries
                (content, content_type, content_size, content_hash, source_app,
                 created_at, updated_at, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7)",
            rusqlite::params![
                "print('hi')",
                "code",
                11i64,
                "hash::legacy",
                "app",
                "2026-01-02T03:04:05Z",
                "2026-01-02T03:04:05Z",
            ],
        )
        .expect("insert");
        conn.last_insert_rowid()
    };

    // Apply the new migration.
    let migrations = builtin_migrations();
    db.run_migrations(&migrations).expect("upgrade");

    let conn = db.connection_mut();
    let record = clipvault_db::EntryRepository::new(conn)
        .find_by_id(legacy_id)
        .expect("query")
        .expect("row");
    assert_eq!(record.content_type, ContentType::Code);
    assert_eq!(record.code_language, None);
    assert_eq!(record.content, "print('hi')");
}

#[test]
fn legacy_image_rows_keep_null_code_language() {
    // The migration must not touch image rows either: the column is
    // pure metadata and the rich preview / image bridge stays
    // unchanged.
    let (_dir, mut db) = open_legacy_db(10);
    let hash = "a".repeat(64);
    let legacy_id = {
        let conn = db.connection_mut();
        conn.execute(
            "INSERT INTO clipboard_entries
                (content, content_type, content_size, content_hash, source_app,
                 created_at, updated_at, last_seen_at,
                 asset_ref, mime_type, payload_width, payload_height)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                "",
                "image",
                128i64,
                &hash,
                "app",
                "2026-01-02T03:04:05Z",
                "2026-01-02T03:04:05Z",
                format!("clipboard/{hash}.png"),
                "image/png",
                4i64,
                2i64,
            ],
        )
        .expect("insert");
        conn.last_insert_rowid()
    };

    db.run_migrations(&builtin_migrations()).expect("upgrade");

    let conn = db.connection_mut();
    let record = clipvault_db::EntryRepository::new(conn)
        .find_by_id(legacy_id)
        .expect("query")
        .expect("row");
    assert_eq!(record.content_type, ContentType::Image);
    assert_eq!(record.code_language, None);
}

#[test]
fn migration_creates_index_on_code_language() {
    let (_dir, mut db) = open_legacy_db(10);
    db.run_migrations(&builtin_migrations()).expect("upgrade");
    let conn = db.connection_mut();
    let indexes: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND name LIKE 'idx_clipboard_entries_code_language'")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert!(
        indexes
            .iter()
            .any(|name| name == "idx_clipboard_entries_code_language"),
        "expected idx_clipboard_entries_code_language to exist"
    );
}

#[test]
fn legacy_rows_load_with_null_code_language() {
    // The `code_language` column MUST default to NULL for every
    // pre-existing row so the detection runs only on new captures
    // and the UI keeps rendering the generic `code` label.
    let (_dir, mut db) = open_temp_db();
    let t = datetime!(2026-01-02 03:04:05 UTC);
    let id = {
        let conn = db.connection_mut();
        let mut repo = clipvault_db::EntryRepository::new(conn);
        repo.insert_or_touch(NewEntry::text(
            "snippet".into(),
            ContentType::Code,
            7,
            "hash::legacy".into(),
            Some("app".into()),
            t,
            t,
        ))
        .expect("insert")
        .record()
        .id
    };

    let conn = db.connection_mut();
    let record = clipvault_db::EntryRepository::new(conn)
        .find_by_id(id)
        .expect("query")
        .expect("row");
    assert_eq!(record.code_language, None);
    assert_eq!(record.content_type, ContentType::Code);
}

#[test]
fn set_code_language_persists_canonical_value() {
    let (_dir, mut db) = open_temp_db();
    let t = datetime!(2026-01-02 03:04:05 UTC);
    let id = {
        let conn = db.connection_mut();
        let mut repo = clipvault_db::EntryRepository::new(conn);
        repo.insert_or_touch(NewEntry::text(
            "print('hi')".into(),
            ContentType::Code,
            11,
            "hash::canonical".into(),
            Some("app".into()),
            t,
            t,
        ))
        .expect("insert")
        .record()
        .id
    };

    let outcome = {
        let conn = db.connection_mut();
        let mut repo = clipvault_db::EntryRepository::new(conn);
        repo.set_code_language(id, Some("python"), t).expect("set")
    };
    assert!(!outcome.noop);
    let record = outcome.updated.expect("record");
    assert_eq!(record.code_language.as_deref(), Some("python"));
}

#[test]
fn set_code_language_is_idempotent_when_value_matches() {
    let (_dir, mut db) = open_temp_db();
    let t = datetime!(2026-01-02 03:04:05 UTC);
    let id = {
        let conn = db.connection_mut();
        let mut repo = clipvault_db::EntryRepository::new(conn);
        repo.insert_or_touch(NewEntry::text(
            "snippet".into(),
            ContentType::Code,
            7,
            "hash::idem".into(),
            Some("app".into()),
            t,
            t,
        ))
        .expect("insert")
        .record()
        .id
    };
    {
        let conn = db.connection_mut();
        let mut repo = clipvault_db::EntryRepository::new(conn);
        repo.set_code_language(id, Some("rust"), t).expect("first");
    }
    let outcome = {
        let conn = db.connection_mut();
        let mut repo = clipvault_db::EntryRepository::new(conn);
        repo.set_code_language(id, Some("rust"), t).expect("second")
    };
    // The repository refuses to clobber with `null`. When the
    // classification is unchanged, the row stays the same and the
    // service reports `noop = false` (because nothing was
    // preserved — the row already had the classification).
    assert!(!outcome.noop);
}

#[test]
fn set_code_language_refuses_to_clobber_existing_with_null() {
    let (_dir, mut db) = open_temp_db();
    let t = datetime!(2026-01-02 03:04:05 UTC);
    let id = {
        let conn = db.connection_mut();
        let mut repo = clipvault_db::EntryRepository::new(conn);
        repo.insert_or_touch(NewEntry::text(
            "snippet".into(),
            ContentType::Code,
            7,
            "hash::protect".into(),
            Some("app".into()),
            t,
            t,
        ))
        .expect("insert")
        .record()
        .id
    };
    {
        let conn = db.connection_mut();
        let mut repo = clipvault_db::EntryRepository::new(conn);
        repo.set_code_language(id, Some("rust"), t).expect("set");
    }
    let outcome = {
        let conn = db.connection_mut();
        let mut repo = clipvault_db::EntryRepository::new(conn);
        repo.set_code_language(id, None, t).expect("null protect")
    };
    assert!(outcome.noop, "null write must be a noop");
    let record = outcome.updated.expect("record");
    assert_eq!(record.code_language.as_deref(), Some("rust"));
}

#[test]
fn set_code_language_handles_missing_id() {
    let (_dir, mut db) = open_temp_db();
    let conn = db.connection_mut();
    let mut repo = clipvault_db::EntryRepository::new(conn);
    let outcome = repo
        .set_code_language(99_999, Some("python"), datetime!(2026-01-02 03:04:05 UTC))
        .expect("missing");
    assert!(outcome.updated.is_none());
    assert!(!outcome.noop);
}
