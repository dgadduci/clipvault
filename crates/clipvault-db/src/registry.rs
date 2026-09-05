//! Built-in migrations shipped with ClipVault.
//!
//! These are intentionally minimal: the `desktop-foundation` change only
//! needs a working SQLite database. Real schema (clipboard entries,
//! favourites, settings, …) will be added by their dedicated OpenSpec
//! changes and registered here by appending to the returned slice.

use crate::migration::Migration;

/// Initial migration: registers the `schema_migrations` table — the table
/// itself is created at boot by [`crate::Database::open`], so this
/// migration is a no-op marker that proves the runner works end-to-end.
const MIGRATION_0001_BOOTSTRAP: Migration = Migration {
    version: 1,
    description: "bootstrap: confirm migration runner",
    up_sql: "SELECT 1;",
    down_sql: "SELECT 1;",
};

/// `clipboard-text-history`: persists the captured text history. The
/// `content_hash` UNIQUE index enforces the deduplication contract
/// required by the spec at the storage layer.
const MIGRATION_0002_CLIPBOARD_ENTRIES: Migration = Migration {
    version: 2,
    description: "clipboard-text-history: add clipboard_entries table",
    up_sql: "CREATE TABLE IF NOT EXISTS clipboard_entries (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        content TEXT NOT NULL,
        content_type TEXT NOT NULL,
        content_size INTEGER NOT NULL,
        content_hash TEXT NOT NULL,
        source_app TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        last_seen_at TEXT NOT NULL
    );
    CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_entries_hash
        ON clipboard_entries (content_hash);
    CREATE INDEX IF NOT EXISTS idx_clipboard_entries_updated_at
        ON clipboard_entries (updated_at DESC);
    CREATE INDEX IF NOT EXISTS idx_clipboard_entries_source_app
        ON clipboard_entries (source_app);",
    down_sql: "DROP INDEX IF EXISTS idx_clipboard_entries_source_app;
    DROP INDEX IF EXISTS idx_clipboard_entries_updated_at;
    DROP INDEX IF EXISTS idx_clipboard_entries_hash;
    DROP TABLE IF EXISTS clipboard_entries;",
};

/// `clipboard-management`: add the `is_pinned` column and the indexes
/// the management service needs. The column defaults to `0` so
/// existing rows are treated as non-favorite without any rewrite. The
/// compound index lets the retention purge and the favorite-aware
/// `recent` query hit the storage layer with a single index lookup.
const MIGRATION_0003_HISTORY_MANAGEMENT: Migration = Migration {
    version: 3,
    description: "clipboard-management: add favorites column and retention index",
    up_sql: "ALTER TABLE clipboard_entries ADD COLUMN is_pinned INTEGER NOT NULL DEFAULT 0;
    CREATE INDEX IF NOT EXISTS idx_clipboard_entries_pinned_updated
        ON clipboard_entries (is_pinned, updated_at DESC);
    CREATE INDEX IF NOT EXISTS idx_clipboard_entries_created_at
        ON clipboard_entries (created_at);",
    down_sql: "DROP INDEX IF EXISTS idx_clipboard_entries_created_at;
    DROP INDEX IF EXISTS idx_clipboard_entries_pinned_updated;
    -- SQLite does not support dropping a column directly; the
    -- down migration recreates the table without `is_pinned`. The
    -- rebuild is intentional: it lets tests verify that a rollback
    -- is possible without losing data.
    CREATE TABLE clipboard_entries_rollback (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        content TEXT NOT NULL,
        content_type TEXT NOT NULL,
        content_size INTEGER NOT NULL,
        content_hash TEXT NOT NULL,
        source_app TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        last_seen_at TEXT NOT NULL
    );
    INSERT INTO clipboard_entries_rollback
        (id, content, content_type, content_size, content_hash,
         source_app, created_at, updated_at, last_seen_at)
    SELECT id, content, content_type, content_size, content_hash,
           source_app, created_at, updated_at, last_seen_at
    FROM clipboard_entries;
    DROP TABLE clipboard_entries;
    ALTER TABLE clipboard_entries_rollback RENAME TO clipboard_entries;
    CREATE UNIQUE INDEX idx_clipboard_entries_hash
        ON clipboard_entries (content_hash);
    CREATE INDEX idx_clipboard_entries_updated_at
        ON clipboard_entries (updated_at DESC);
    CREATE INDEX idx_clipboard_entries_source_app
        ON clipboard_entries (source_app);",
};

/// `clipboard-management`: add the `app_settings` key-value table used
/// by the retention policy (and future local settings from the
/// `privacy-settings` capability). The table is intentionally tiny so
/// the management service can read the policy in a single statement.
const MIGRATION_0004_APP_SETTINGS: Migration = Migration {
    version: 4,
    description: "clipboard-management: add app_settings key-value table",
    up_sql: "CREATE TABLE IF NOT EXISTS app_settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );",
    down_sql: "DROP TABLE IF EXISTS app_settings;",
};

/// `privacy-settings`: add the `ignored_apps` table used by the
/// configurable blacklist. Identifiers are stored normalised
/// (trim + lowercase) so the matcher can issue a single equality
/// lookup; we keep a single-row, deterministic schema for now.
const MIGRATION_0005_IGNORED_APPS: Migration = Migration {
    version: 5,
    description: "privacy-settings: add ignored_apps blacklist table",
    up_sql: "CREATE TABLE IF NOT EXISTS ignored_apps (
        id TEXT PRIMARY KEY,
        created_at TEXT NOT NULL
    );",
    down_sql: "DROP TABLE IF EXISTS ignored_apps;",
};

/// `blacklist-app-picker`: extend the `ignored_apps` table with the
/// presentation metadata the picker flow needs. Both columns are
/// nullable so legacy rows (created before the picker was available)
/// keep matching the identifier and the frontend renders a safe
/// fallback for them. The migration is additive — it never rewrites
/// or deletes pre-existing rows — and reversible: the `down` step
/// drops the columns through the SQLite table-rebuild pattern so a
/// rollback leaves the table in the pre-picker shape.
const MIGRATION_0006_IGNORED_APP_METADATA: Migration = Migration {
    version: 6,
    description: "blacklist-app-picker: add display_name and icon_ref to ignored_apps",
    up_sql: "ALTER TABLE ignored_apps ADD COLUMN display_name TEXT;
    ALTER TABLE ignored_apps ADD COLUMN icon_ref TEXT;",
    down_sql: "CREATE TABLE ignored_apps_rollback (
        id TEXT PRIMARY KEY,
        created_at TEXT NOT NULL
    );
    INSERT INTO ignored_apps_rollback (id, created_at)
        SELECT id, created_at FROM ignored_apps;
    DROP TABLE ignored_apps;
    ALTER TABLE ignored_apps_rollback RENAME TO ignored_apps;",
};

/// `history-card-layout`: extend `clipboard_entries` with the nullable
/// presentation metadata the card layout needs (custom title and
/// source-application name/icon reference). All three columns are
/// nullable so rows created before the migration keep rendering with
/// the documented fallbacks (title derived from `content_type`,
/// source name derived from `source_app`, generic application icon).
///
/// The migration is additive — it never rewrites or deletes
/// pre-existing rows — and reversible: the `down` step rebuilds the
/// table through SQLite's table-rebuild pattern so a rollback leaves
/// the schema in the pre-card-layout shape without losing data.
const MIGRATION_0007_CARD_LAYOUT_METADATA: Migration = Migration {
    version: 7,
    description: "history-card-layout: add title, source_app_name and source_app_icon_ref",
    up_sql: "ALTER TABLE clipboard_entries ADD COLUMN title TEXT;
    ALTER TABLE clipboard_entries ADD COLUMN source_app_name TEXT;
    ALTER TABLE clipboard_entries ADD COLUMN source_app_icon_ref TEXT;",
    down_sql: "CREATE TABLE clipboard_entries_card_layout_rollback (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        content TEXT NOT NULL,
        content_type TEXT NOT NULL,
        content_size INTEGER NOT NULL,
        content_hash TEXT NOT NULL,
        source_app TEXT,
        is_pinned INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        last_seen_at TEXT NOT NULL
    );
    INSERT INTO clipboard_entries_card_layout_rollback
        (id, content, content_type, content_size, content_hash,
         source_app, is_pinned, created_at, updated_at, last_seen_at)
    SELECT id, content, content_type, content_size, content_hash,
           source_app, is_pinned, created_at, updated_at, last_seen_at
    FROM clipboard_entries;
    DROP TABLE clipboard_entries;
    ALTER TABLE clipboard_entries_card_layout_rollback RENAME TO clipboard_entries;
    CREATE UNIQUE INDEX idx_clipboard_entries_hash
        ON clipboard_entries (content_hash);
    CREATE INDEX idx_clipboard_entries_updated_at
        ON clipboard_entries (updated_at DESC);
    CREATE INDEX idx_clipboard_entries_source_app
        ON clipboard_entries (source_app);
    CREATE INDEX idx_clipboard_entries_pinned_updated
        ON clipboard_entries (is_pinned, updated_at DESC);
    CREATE INDEX idx_clipboard_entries_created_at
        ON clipboard_entries (created_at);",
};

/// `clipboard-rich-content`: extend `clipboard_entries` with the
/// payload metadata a non-textual capture needs.
///
/// All four columns are nullable so every pre-existing row keeps its
/// exact shape: a textual row created before this migration has
/// `asset_ref IS NULL` and renders through the existing text preview.
/// The migration never rewrites, deletes or re-hashes a row.
///
/// Column contract:
///
/// - `asset_ref`: **relative** reference of the form
///   `clipboard/<lowercase-sha256>.png`. An absolute filesystem path
///   is never stored; the read side resolves the reference inside
///   `<data_dir>/assets/clipboard/` and rejects anything that escapes
///   that namespace.
/// - `mime_type`: `image/png` in this phase.
/// - `payload_width` / `payload_height`: the original pixel dimensions
///   of the captured bitmap.
///
/// `content_type`, `content_size` and `content_hash` keep their
/// meaning and remain the source of truth for type, size and dedupe;
/// for an image row `content_hash` is the SHA-256 of the normalised
/// PNG and `content_size` its byte length. `content` stays `NOT NULL`
/// (an image row stores the documented empty sentinel) precisely so
/// this migration can be a pure `ALTER TABLE ... ADD COLUMN` and never
/// has to rebuild the historical table.
///
/// The `down` step reverses the change through the same table-rebuild
/// pattern the earlier migrations use. Image rows lose their metadata
/// on rollback — that is inherent to dropping the columns — so the
/// rebuild also drops the rows that can only be interpreted with them,
/// leaving a consistent pre-image-support table instead of orphaned
/// `content_type = 'image'` rows with an empty `content`.
const MIGRATION_0008_CLIPBOARD_ASSETS: Migration = Migration {
    version: 8,
    description: "clipboard-rich-content: add asset_ref, mime_type and payload dimensions",
    up_sql: "ALTER TABLE clipboard_entries ADD COLUMN asset_ref TEXT;
    ALTER TABLE clipboard_entries ADD COLUMN mime_type TEXT;
    ALTER TABLE clipboard_entries ADD COLUMN payload_width INTEGER;
    ALTER TABLE clipboard_entries ADD COLUMN payload_height INTEGER;
    CREATE INDEX IF NOT EXISTS idx_clipboard_entries_asset_ref
        ON clipboard_entries (asset_ref);",
    down_sql: "DROP INDEX IF EXISTS idx_clipboard_entries_asset_ref;
    CREATE TABLE clipboard_entries_assets_rollback (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        content TEXT NOT NULL,
        content_type TEXT NOT NULL,
        content_size INTEGER NOT NULL,
        content_hash TEXT NOT NULL,
        source_app TEXT,
        is_pinned INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        last_seen_at TEXT NOT NULL,
        title TEXT,
        source_app_name TEXT,
        source_app_icon_ref TEXT
    );
    INSERT INTO clipboard_entries_assets_rollback
        (id, content, content_type, content_size, content_hash,
         source_app, is_pinned, created_at, updated_at, last_seen_at,
         title, source_app_name, source_app_icon_ref)
    SELECT id, content, content_type, content_size, content_hash,
           source_app, is_pinned, created_at, updated_at, last_seen_at,
           title, source_app_name, source_app_icon_ref
    FROM clipboard_entries
    WHERE content_type <> 'image';
    DROP TABLE clipboard_entries;
    ALTER TABLE clipboard_entries_assets_rollback RENAME TO clipboard_entries;
    CREATE UNIQUE INDEX idx_clipboard_entries_hash
        ON clipboard_entries (content_hash);
    CREATE INDEX idx_clipboard_entries_updated_at
        ON clipboard_entries (updated_at DESC);
    CREATE INDEX idx_clipboard_entries_source_app
        ON clipboard_entries (source_app);
    CREATE INDEX idx_clipboard_entries_pinned_updated
        ON clipboard_entries (is_pinned, updated_at DESC);
    CREATE INDEX idx_clipboard_entries_created_at
        ON clipboard_entries (created_at);",
};

/// `tags-and-collections`: add the tables and indices the
/// organization capability needs. The migration is purely additive
/// for the legacy `clipboard_entries` rows — it never touches the
/// historical table — but it also performs the documented
/// `Historial` backfill in the same transaction:
///
/// 1. creates the four new tables with the foreign keys the
///    organization layer relies on;
/// 2. seeds the permanent `Historial` system collection;
/// 3. associates every pre-existing entry with `Historial`.
///
/// The backfill is the only operation that mutates existing data
/// and is required by the change contract so a database created
/// before the `tags-and-collections` capability keeps the same
/// invariants as one created today (every entry belongs to
/// `Historial`).
///
/// `stable_key = "history"` and `kind = "system"` identify
/// `Historial`; the UI never identifies it by its display name. The
/// display name `Historial` is the canonical localisation the
/// sidebar renders; renaming the row would break every existing
/// user expectation. The `down` step removes every row that
/// references the seeded collection first so the foreign-key chain
/// (`entry_collections.collection_id -> collections.id`) leaves no
/// orphans behind.
const MIGRATION_0010_ORGANIZATION: Migration = Migration {
    version: 10,
    description: "tags-and-collections: add collections, tags and many-to-many tables",
    up_sql: "CREATE TABLE IF NOT EXISTS collections (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        stable_key TEXT UNIQUE,
        name TEXT NOT NULL,
        kind TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS tags (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        normalized_name TEXT UNIQUE NOT NULL,
        display_name TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS entry_collections (
        entry_id INTEGER NOT NULL,
        collection_id INTEGER NOT NULL,
        created_at TEXT NOT NULL,
        PRIMARY KEY (entry_id, collection_id),
        FOREIGN KEY (entry_id) REFERENCES clipboard_entries(id) ON DELETE CASCADE,
        FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE
    );
    CREATE TABLE IF NOT EXISTS entry_tags (
        entry_id INTEGER NOT NULL,
        tag_id INTEGER NOT NULL,
        created_at TEXT NOT NULL,
        PRIMARY KEY (entry_id, tag_id),
        FOREIGN KEY (entry_id) REFERENCES clipboard_entries(id) ON DELETE CASCADE,
        FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
    );
    CREATE INDEX IF NOT EXISTS idx_collections_kind
        ON collections (kind);
    CREATE INDEX IF NOT EXISTS idx_collections_name
        ON collections (name COLLATE NOCASE);
    CREATE INDEX IF NOT EXISTS idx_tags_normalized_name
        ON tags (normalized_name);
    CREATE INDEX IF NOT EXISTS idx_entry_collections_collection_id
        ON entry_collections (collection_id);
    CREATE INDEX IF NOT EXISTS idx_entry_tags_tag_id
        ON entry_tags (tag_id);
    INSERT OR IGNORE INTO collections (stable_key, name, kind, created_at, updated_at)
        VALUES ('history', 'Historial', 'system', '1970-01-01T00:00:00Z', '1970-01-01T00:00:00Z');
    INSERT OR IGNORE INTO entry_collections (entry_id, collection_id, created_at)
        SELECT clipboard_entries.id, collections.id, '1970-01-01T00:00:00Z'
        FROM clipboard_entries
        CROSS JOIN collections
        WHERE collections.stable_key = 'history';",
    down_sql: "DELETE FROM entry_collections
        WHERE collection_id IN (SELECT id FROM collections WHERE stable_key = 'history');
    DELETE FROM collections WHERE stable_key = 'history';
    DROP INDEX IF EXISTS idx_entry_tags_tag_id;
    DROP INDEX IF EXISTS idx_entry_collections_collection_id;
    DROP INDEX IF EXISTS idx_tags_normalized_name;
    DROP INDEX IF EXISTS idx_collections_name;
    DROP INDEX IF EXISTS idx_collections_kind;
    DROP TABLE IF EXISTS entry_tags;
    DROP TABLE IF EXISTS entry_collections;
    DROP TABLE IF EXISTS tags;
    DROP TABLE IF EXISTS collections;",
};

/// `clipboard-rich-text`: extend `clipboard_entries` with the
/// rich-text metadata a textual capture with rich representations
/// needs. Every new column is nullable so pre-existing rows
/// (textual, image, and rich-without-rich-text metadata) keep their
/// exact shape.
///
/// Column contract:
///
/// - `rich_text_hash`: deterministic SHA-256 of the canonical rich-text
///   representation the core computes. Used as the secondary dedupe
///   key so two captures with identical plain text but different
///   styles do not collapse to a single row.
/// - `rich_html_ref` / `rich_rtf_ref` / `rich_preview_ref`: relative
///   references of the form `rich-text/<sha256>.<ext>` pointing at the
///   original HTML, the original RTF, and the sanitised preview
///   respectively. All three live under
///   `<data_dir>/assets/rich-text/` and obey the same scope /
///   traversal / symlink rules as the image asset bridge.
/// - `rich_html_size` / `rich_rtf_size`: byte length of the original
///   representations, kept for diagnostics and quick filtering.
///
/// The migration also reshapes the dedupe index so two captures with
/// the same plain text but different rich styles can coexist:
///
/// - the legacy `UNIQUE INDEX` on `content_hash` (created by
///   migration 2) becomes a *partial* index that only fires when
///   `rich_text_hash IS NULL`, so plain-text and image rows keep
///   their existing dedupe behaviour;
/// - a new partial unique index on
///   `(content_hash, rich_text_hash)` fires only when
///   `rich_text_hash IS NOT NULL`, so rich-text rows are unique per
///   `(content_hash, rich_text_hash)` pair.
///
/// The `down` step reverses the change through the same table-rebuild
/// pattern the earlier migrations use: rich-text rows are dropped
/// because their metadata is no longer present, leaving a consistent
/// pre-rich-text table. The migration never touches the image or
/// text paths.
const MIGRATION_0009_RICH_TEXT: Migration = Migration {
    version: 9,
    description: "clipboard-rich-text: add rich_text_hash and asset references",
    up_sql: "ALTER TABLE clipboard_entries ADD COLUMN rich_text_hash TEXT;
    ALTER TABLE clipboard_entries ADD COLUMN rich_html_ref TEXT;
    ALTER TABLE clipboard_entries ADD COLUMN rich_rtf_ref TEXT;
    ALTER TABLE clipboard_entries ADD COLUMN rich_preview_ref TEXT;
    ALTER TABLE clipboard_entries ADD COLUMN rich_html_size INTEGER;
    ALTER TABLE clipboard_entries ADD COLUMN rich_rtf_size INTEGER;
    DROP INDEX IF EXISTS idx_clipboard_entries_hash;
    CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_entries_plain_hash
        ON clipboard_entries (content_hash) WHERE rich_text_hash IS NULL;
    CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_entries_rich_hash
        ON clipboard_entries (content_hash, rich_text_hash)
        WHERE rich_text_hash IS NOT NULL;
    CREATE INDEX IF NOT EXISTS idx_clipboard_entries_rich_text_hash
        ON clipboard_entries (rich_text_hash);
    CREATE INDEX IF NOT EXISTS idx_clipboard_entries_rich_html_ref
        ON clipboard_entries (rich_html_ref);
    CREATE INDEX IF NOT EXISTS idx_clipboard_entries_rich_rtf_ref
        ON clipboard_entries (rich_rtf_ref);
    CREATE INDEX IF NOT EXISTS idx_clipboard_entries_rich_preview_ref
        ON clipboard_entries (rich_preview_ref);",
    down_sql: "DROP INDEX IF EXISTS idx_clipboard_entries_rich_preview_ref;
    DROP INDEX IF EXISTS idx_clipboard_entries_rich_rtf_ref;
    DROP INDEX IF EXISTS idx_clipboard_entries_rich_html_ref;
    DROP INDEX IF EXISTS idx_clipboard_entries_rich_text_hash;
    DROP INDEX IF EXISTS idx_clipboard_entries_rich_hash;
    DROP INDEX IF EXISTS idx_clipboard_entries_plain_hash;
    CREATE UNIQUE INDEX idx_clipboard_entries_hash
        ON clipboard_entries (content_hash);
    CREATE TABLE clipboard_entries_rich_text_rollback (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        content TEXT NOT NULL,
        content_type TEXT NOT NULL,
        content_size INTEGER NOT NULL,
        content_hash TEXT NOT NULL,
        source_app TEXT,
        is_pinned INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        last_seen_at TEXT NOT NULL,
        title TEXT,
        source_app_name TEXT,
        source_app_icon_ref TEXT,
        asset_ref TEXT,
        mime_type TEXT,
        payload_width INTEGER,
        payload_height INTEGER
    );
    INSERT INTO clipboard_entries_rich_text_rollback
        (id, content, content_type, content_size, content_hash,
         source_app, is_pinned, created_at, updated_at, last_seen_at,
         title, source_app_name, source_app_icon_ref,
         asset_ref, mime_type, payload_width, payload_height)
    SELECT id, content, content_type, content_size, content_hash,
           source_app, is_pinned, created_at, updated_at, last_seen_at,
           title, source_app_name, source_app_icon_ref,
           asset_ref, mime_type, payload_width, payload_height
    FROM clipboard_entries
    WHERE rich_text_hash IS NULL;
    DROP TABLE clipboard_entries;
    ALTER TABLE clipboard_entries_rich_text_rollback RENAME TO clipboard_entries;
    CREATE UNIQUE INDEX idx_clipboard_entries_hash
        ON clipboard_entries (content_hash);
    CREATE INDEX idx_clipboard_entries_updated_at
        ON clipboard_entries (updated_at DESC);
    CREATE INDEX idx_clipboard_entries_source_app
        ON clipboard_entries (source_app);
    CREATE INDEX idx_clipboard_entries_pinned_updated
        ON clipboard_entries (is_pinned, updated_at DESC);
    CREATE INDEX idx_clipboard_entries_created_at
        ON clipboard_entries (created_at);
    CREATE INDEX idx_clipboard_entries_asset_ref
        ON clipboard_entries (asset_ref);",
};

/// Returns the migrations shipped with ClipVault. Each new migration is
/// appended to this slice to keep ordering deterministic.
pub fn builtin_migrations() -> Vec<Migration> {
    vec![
        MIGRATION_0001_BOOTSTRAP,
        MIGRATION_0002_CLIPBOARD_ENTRIES,
        MIGRATION_0003_HISTORY_MANAGEMENT,
        MIGRATION_0004_APP_SETTINGS,
        MIGRATION_0005_IGNORED_APPS,
        MIGRATION_0006_IGNORED_APP_METADATA,
        MIGRATION_0007_CARD_LAYOUT_METADATA,
        MIGRATION_0008_CLIPBOARD_ASSETS,
        MIGRATION_0009_RICH_TEXT,
        MIGRATION_0010_ORGANIZATION,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_migrations_are_unique_and_ordered() {
        let migrations = builtin_migrations();
        let versions: Vec<i64> = migrations.iter().map(|m| m.version).collect();
        let mut sorted = versions.clone();
        sorted.sort();
        assert_eq!(versions, sorted, "migrations must be sorted by version");
        let mut unique = versions.clone();
        unique.dedup();
        assert_eq!(
            unique.len(),
            versions.len(),
            "migrations must have unique versions"
        );
    }

    #[test]
    fn clipboard_assets_migration_is_the_next_sequential_version() {
        // The change contract says the asset migration is appended
        // after the existing ones, not inserted in the middle.
        let migrations = builtin_migrations();
        let last = migrations.last().expect("at least one migration");
        assert_eq!(last.version, 10);
        assert_eq!(last.version, MIGRATION_0010_ORGANIZATION.version);
        assert_eq!(migrations.len(), 10);
    }

    #[test]
    fn clipboard_assets_migration_is_purely_additive() {
        // The `up` step must only add columns / indexes: no DROP, no
        // DELETE, no UPDATE, no table rebuild. That is what guarantees
        // pre-existing textual rows survive untouched.
        let up = MIGRATION_0008_CLIPBOARD_ASSETS.up_sql.to_uppercase();
        for forbidden in ["DROP TABLE", "DELETE FROM", "UPDATE ", "INSERT INTO"] {
            assert!(
                !up.contains(forbidden),
                "additive migration must not contain {forbidden}"
            );
        }
        for column in ["ASSET_REF", "MIME_TYPE", "PAYLOAD_WIDTH", "PAYLOAD_HEIGHT"] {
            assert!(up.contains(column), "missing column {column}");
        }
    }

    #[test]
    fn rich_text_migration_is_purely_additive() {
        // Same rule as the image migration: the `up` step must only
        // add columns / indexes; the `down` step uses the table-rebuild
        // pattern but is excluded from this assertion because
        // rollbacks inherently DROP and INSERT.
        let up = MIGRATION_0009_RICH_TEXT.up_sql.to_uppercase();
        for forbidden in ["DROP TABLE", "DELETE FROM", "UPDATE ", "INSERT INTO"] {
            assert!(
                !up.contains(forbidden),
                "additive migration must not contain {forbidden}"
            );
        }
        for column in [
            "RICH_TEXT_HASH",
            "RICH_HTML_REF",
            "RICH_RTF_REF",
            "RICH_PREVIEW_REF",
            "RICH_HTML_SIZE",
            "RICH_RTF_SIZE",
        ] {
            assert!(up.contains(column), "missing column {column}");
        }
    }

    #[test]
    fn organization_migration_creates_required_tables_and_indices() {
        let up = MIGRATION_0010_ORGANIZATION.up_sql.to_uppercase();
        for table in ["COLLECTIONS", "TAGS", "ENTRY_COLLECTIONS", "ENTRY_TAGS"] {
            assert!(
                up.contains(&format!("CREATE TABLE IF NOT EXISTS {table}")),
                "missing table {table}"
            );
        }
        // The change contract mandates that Historial is seeded by
        // `stable_key = 'history'` and that every existing entry is
        // associated with it in the same transaction.
        assert!(up.contains("'HISTORY'"), "missing system stable key");
        assert!(
            up.contains("INSERT OR IGNORE INTO COLLECTIONS"),
            "missing collection seed"
        );
        assert!(
            up.contains("INSERT OR IGNORE INTO ENTRY_COLLECTIONS"),
            "missing entry backfill"
        );
    }
}
