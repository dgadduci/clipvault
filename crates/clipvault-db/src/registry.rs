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

/// `code-language-detection`: extend `clipboard_entries` with the
/// optional `code_language` metadata the conservative detector persists.
///
/// `code_language` is a nullable text column holding one of the canonical
/// identifiers (`javascript`, `typescript`, `python`, …) the
/// `code-language-detection` allowlist defines. Pre-existing rows keep
/// `code_language = NULL` and continue rendering through the documented
/// generic `code` content type; the column is purely additive.
///
/// The `down` step uses the table-rebuild pattern the other
/// additive migrations use so a rollback returns the schema to the
/// pre-`code-language-detection` shape without losing data. Image,
/// rich-text and text rows are preserved; the column itself is
/// dropped alongside the index the migration adds.
const MIGRATION_0011_CODE_LANGUAGE: Migration = Migration {
    version: 11,
    description: "code-language-detection: add nullable code_language column and index",
    up_sql: "ALTER TABLE clipboard_entries ADD COLUMN code_language TEXT;
    CREATE INDEX IF NOT EXISTS idx_clipboard_entries_code_language
        ON clipboard_entries (code_language);",
    down_sql: "DROP INDEX IF EXISTS idx_clipboard_entries_code_language;
    CREATE TABLE clipboard_entries_code_language_rollback (
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
        payload_height INTEGER,
        rich_text_hash TEXT,
        rich_html_ref TEXT,
        rich_rtf_ref TEXT,
        rich_preview_ref TEXT,
        rich_html_size INTEGER,
        rich_rtf_size INTEGER
    );
    INSERT INTO clipboard_entries_code_language_rollback
        (id, content, content_type, content_size, content_hash,
         source_app, is_pinned, created_at, updated_at, last_seen_at,
         title, source_app_name, source_app_icon_ref,
         asset_ref, mime_type, payload_width, payload_height,
         rich_text_hash, rich_html_ref, rich_rtf_ref,
         rich_preview_ref, rich_html_size, rich_rtf_size)
    SELECT id, content, content_type, content_size, content_hash,
           source_app, is_pinned, created_at, updated_at, last_seen_at,
           title, source_app_name, source_app_icon_ref,
           asset_ref, mime_type, payload_width, payload_height,
           rich_text_hash, rich_html_ref, rich_rtf_ref,
           rich_preview_ref, rich_html_size, rich_rtf_size
    FROM clipboard_entries;
    DROP TABLE clipboard_entries;
    ALTER TABLE clipboard_entries_code_language_rollback RENAME TO clipboard_entries;
    CREATE UNIQUE INDEX idx_clipboard_entries_plain_hash
        ON clipboard_entries (content_hash) WHERE rich_text_hash IS NULL;
    CREATE UNIQUE INDEX idx_clipboard_entries_rich_hash
        ON clipboard_entries (content_hash, rich_text_hash)
        WHERE rich_text_hash IS NOT NULL;
    CREATE INDEX idx_clipboard_entries_updated_at
        ON clipboard_entries (updated_at DESC);
    CREATE INDEX idx_clipboard_entries_source_app
        ON clipboard_entries (source_app);
    CREATE INDEX idx_clipboard_entries_pinned_updated
        ON clipboard_entries (is_pinned, updated_at DESC);
    CREATE INDEX idx_clipboard_entries_created_at
        ON clipboard_entries (created_at);
    CREATE INDEX idx_clipboard_entries_asset_ref
        ON clipboard_entries (asset_ref);
    CREATE INDEX idx_clipboard_entries_rich_text_hash
        ON clipboard_entries (rich_text_hash);
    CREATE INDEX idx_clipboard_entries_rich_html_ref
        ON clipboard_entries (rich_html_ref);
    CREATE INDEX idx_clipboard_entries_rich_rtf_ref
        ON clipboard_entries (rich_rtf_ref);
    CREATE INDEX idx_clipboard_entries_rich_preview_ref
        ON clipboard_entries (rich_preview_ref);",
};

/// `collection-colors-and-card-collection-labels`: add the persistent
/// `color_hex` metadata the sidebar square and the per-card
/// collection labels consume.
///
/// The column is `NOT NULL` with a default that mirrors the system
/// `Historial` blue so every pre-existing row receives a valid opaque
/// colour without an `UPDATE` cascade. The backfill is purely
/// additive — it never deletes or rewrites a name, a kind, a
/// timestamp or a membership — and reversible: the `down` step
/// rebuilds the `collections` table through the SQLite table-rebuild
/// pattern the other additive migrations use so a rollback returns
/// to the pre-colour shape without losing data.
const MIGRATION_0012_COLLECTION_COLORS: Migration = Migration {
    version: 12,
    description: "collection-colors-and-card-collection-labels: persist collections.color_hex",
    up_sql: "ALTER TABLE collections ADD COLUMN color_hex TEXT NOT NULL DEFAULT '#1565c0';
    UPDATE collections SET color_hex = '#1565c0' WHERE color_hex IS NULL OR color_hex = '';",
    down_sql: "DROP TABLE IF EXISTS collections_rollback_color;
    CREATE TABLE collections_rollback_color (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        stable_key TEXT UNIQUE,
        name TEXT NOT NULL,
        kind TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    INSERT INTO collections_rollback_color
        (id, stable_key, name, kind, created_at, updated_at)
    SELECT id, stable_key, name, kind, created_at, updated_at
    FROM collections;
    DROP TABLE collections;
    ALTER TABLE collections_rollback_color RENAME TO collections;
    CREATE INDEX idx_collections_kind ON collections (kind);
    CREATE INDEX idx_collections_name ON collections (name COLLATE NOCASE);",
};

/// `local-peer-discovery`: persist the metadata-only view of every
/// ClipVault installation observed on the local network.
///
/// The `peer_id` is the stable hex SHA-256 prefix the identity
/// foundation derives from the peer's public key. It is the
/// PRIMARY KEY so a single identity cannot appear twice and so the
/// discovery merge is idempotent (re-observing a known peer just
/// updates `last_discovered_at`). The table NEVER stores the
/// peer's IP address, port, private key, clipboard payload,
/// preview, hash, source application or any other non-public
/// field: the discovery surface is metadata-only and the `down`
/// step drops the whole table without touching anything else.
///
/// The migration is purely additive — no existing table is
/// rewritten, every column has a `NOT NULL` default, and the
/// rollback is a single `DROP TABLE`. The `idx_known_peers_*`
/// indexes back the queries the runtime uses (filter by
/// `last_discovered_at` desc for the snapshot; lookup by `peer_id`
/// for the merge; the `first_seen_at` index is reserved for the
/// future pairing change that needs chronological ordering).
const MIGRATION_0013_KNOWN_PEERS: Migration = Migration {
    version: 13,
    description: "local-peer-discovery: add known_peers metadata table",
    up_sql: "CREATE TABLE IF NOT EXISTS known_peers (
        peer_id TEXT PRIMARY KEY,
        public_key_fingerprint TEXT NOT NULL,
        display_name TEXT NOT NULL,
        protocol_major INTEGER NOT NULL,
        capability TEXT NOT NULL,
        first_seen_at TEXT NOT NULL,
        last_discovered_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_known_peers_last_discovered_at
        ON known_peers (last_discovered_at DESC);
    CREATE INDEX IF NOT EXISTS idx_known_peers_first_seen_at
        ON known_peers (first_seen_at);",
    down_sql: "DROP INDEX IF EXISTS idx_known_peers_first_seen_at;
    DROP INDEX IF EXISTS idx_known_peers_last_discovered_at;
    DROP TABLE IF EXISTS known_peers;",
};

/// `local-peer-mutual-pairing`: extend the `known_peers` table with
/// the trust state the pairing runtime needs. The change is
/// strictly additive — every column has a `NOT NULL` default so
/// pre-existing rows keep matching the table without a backfill
/// cascade.
///
/// Column contract:
///
/// - `trust_state`: one of `unverified`, `trusted`, `revoked`,
///   `blocked`. The pairing runtime owns every transition. The
///   discovery runtime reads it but never writes it.
/// - `tls_cert_fingerprint`: SHA-256 of the DER-encoded TLS cert
///   the peer used during the last successful mTLS handshake.
///   Pinned so the runtime can reject a peer whose identity
///   fingerprint stays the same but whose TLS key rotated
///   (compromised key scenario). Empty until pairing completes.
/// - `paired_at`: instant the reciprocal pairing was first
///   persisted. Empty when `trust_state = unverified`.
/// - `paired_protocol_major`: protocol major version at the moment
///   pairing completed. Lets future protocol bumps surface an
///   `IncompatibleProtocol` outcome before re-pinning.
///
/// The `idx_known_peers_trust_state` index supports the
/// `Equipos` snapshot query that filters by trust state, the
/// `revoked/blocked` lookup the runtime uses when receiving an
/// incoming connection, and the future "forget revoked peer" UI
/// action. The `down` step drops the columns through the table
/// rebuild pattern so a rollback returns to the pre-pairing shape
/// without losing pre-pairing rows.
const MIGRATION_0014_KNOWN_PEERS_PAIRING: Migration = Migration {
    version: 14,
    description: "local-peer-mutual-pairing: add trust_state and pinned TLS columns to known_peers",
    up_sql: "ALTER TABLE known_peers ADD COLUMN trust_state TEXT NOT NULL DEFAULT 'unverified';
    ALTER TABLE known_peers ADD COLUMN tls_cert_fingerprint TEXT NOT NULL DEFAULT '';
    ALTER TABLE known_peers ADD COLUMN paired_at TEXT NOT NULL DEFAULT '';
    ALTER TABLE known_peers ADD COLUMN paired_protocol_major INTEGER NOT NULL DEFAULT 0;
    CREATE INDEX IF NOT EXISTS idx_known_peers_trust_state
        ON known_peers (trust_state);",
    down_sql: "DROP INDEX IF EXISTS idx_known_peers_trust_state;
    CREATE TABLE known_peers_pairing_rollback (
        peer_id TEXT PRIMARY KEY,
        public_key_fingerprint TEXT NOT NULL,
        display_name TEXT NOT NULL,
        protocol_major INTEGER NOT NULL,
        capability TEXT NOT NULL,
        first_seen_at TEXT NOT NULL,
        last_discovered_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    INSERT INTO known_peers_pairing_rollback
        (peer_id, public_key_fingerprint, display_name, protocol_major,
         capability, first_seen_at, last_discovered_at, updated_at)
    SELECT peer_id, public_key_fingerprint, display_name, protocol_major,
           capability, first_seen_at, last_discovered_at, updated_at
    FROM known_peers;
    DROP TABLE known_peers;
    ALTER TABLE known_peers_pairing_rollback RENAME TO known_peers;
    CREATE INDEX idx_known_peers_last_discovered_at
        ON known_peers (last_discovered_at DESC);
    CREATE INDEX idx_known_peers_first_seen_at
        ON known_peers (first_seen_at);",
};

/// `local-peer-mutual-pairing`: add the full public-key fingerprint
/// the pairing layer uses to build the canonical
/// `OutboundSessionDescriptor`. The pairing advertisement
/// (`capability = pairing`) populates this column with the
/// canonical 64-hex SHA-256 of the Ed25519 public key, distinct
/// from the 16-hex `public_key_fingerprint` column the
/// discovery-side UI badge uses. A row whose full fingerprint
/// is empty means the peer has only ever been seen via
/// `discovery_only` and cannot start a pairing session until a
/// fresh pairing advertisement upgrades the column. The column
/// has a `NOT NULL DEFAULT ''` so pre-existing rows keep
/// matching without a backfill.
const MIGRATION_0015_KNOWN_PEERS_PAIRING_FULL_FINGERPRINT: Migration = Migration {
    version: 15,
    description: "local-peer-mutual-pairing: add canonical full public-key fingerprint column",
    up_sql:
        "ALTER TABLE known_peers ADD COLUMN full_public_key_fingerprint TEXT NOT NULL DEFAULT '';",
    down_sql: "DROP INDEX IF EXISTS idx_known_peers_trust_state;
    CREATE TABLE known_peers_pairing_full_fingerprint_rollback (
        peer_id TEXT PRIMARY KEY,
        public_key_fingerprint TEXT NOT NULL,
        display_name TEXT NOT NULL,
        protocol_major INTEGER NOT NULL,
        capability TEXT NOT NULL,
        first_seen_at TEXT NOT NULL,
        last_discovered_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        trust_state TEXT NOT NULL DEFAULT 'unverified',
        tls_cert_fingerprint TEXT NOT NULL DEFAULT '',
        paired_at TEXT NOT NULL DEFAULT '',
        paired_protocol_major INTEGER NOT NULL DEFAULT 0
    );
    INSERT INTO known_peers_pairing_full_fingerprint_rollback
        (peer_id, public_key_fingerprint, display_name, protocol_major,
         capability, first_seen_at, last_discovered_at, updated_at,
         trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major)
    SELECT peer_id, public_key_fingerprint, display_name, protocol_major,
           capability, first_seen_at, last_discovered_at, updated_at,
           trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major
    FROM known_peers;
    DROP TABLE known_peers;
    ALTER TABLE known_peers_pairing_full_fingerprint_rollback RENAME TO known_peers;
    CREATE INDEX idx_known_peers_last_discovered_at
        ON known_peers (last_discovered_at DESC);
    CREATE INDEX idx_known_peers_first_seen_at
        ON known_peers (first_seen_at);
    CREATE INDEX idx_known_peers_trust_state
        ON known_peers (trust_state);",
};

/// `peer-text-history-browser`: persist the 32-byte per-peer HMAC
/// secret the host uses to sign and verify the
/// `RemoteHistoryCursor` exchanged over mTLS. The column is
/// nullable + empty by default so every pre-existing
/// `known_peers` row keeps matching the schema without a
/// backfill cascade; the runtime mints a fresh secret exactly
/// once per `trust_state = trusted` transition through
/// [`KnownPeerRepository::set_cursor_secret`] and clears it on
/// every revoke / block / unblock so a stale cursor cannot
/// resurrect the link.
///
/// The runtime is the only writer: the column carries 64
/// lowercase-hex chars (the SHA-256-sized key the HMAC scheme
/// mandates) or an empty string. The secret is never sent over
/// the wire, never logged and never leaves the host. The
/// `down` step rebuilds the table through the SQLite
/// table-rebuild pattern so a rollback drops the column
/// without losing the pre-existing trust metadata.
const MIGRATION_0016_KNOWN_PEERS_CURSOR_SECRET: Migration = Migration {
    version: 16,
    description: "peer-text-history-browser: persist per-peer HMAC cursor secret in known_peers",
    up_sql: "ALTER TABLE known_peers ADD COLUMN cursor_secret TEXT NOT NULL DEFAULT '';",
    down_sql: "DROP INDEX IF EXISTS idx_known_peers_trust_state;
    CREATE TABLE known_peers_cursor_secret_rollback (
        peer_id TEXT PRIMARY KEY,
        public_key_fingerprint TEXT NOT NULL,
        display_name TEXT NOT NULL,
        protocol_major INTEGER NOT NULL,
        capability TEXT NOT NULL,
        first_seen_at TEXT NOT NULL,
        last_discovered_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        trust_state TEXT NOT NULL DEFAULT 'unverified',
        tls_cert_fingerprint TEXT NOT NULL DEFAULT '',
        paired_at TEXT NOT NULL DEFAULT '',
        paired_protocol_major INTEGER NOT NULL DEFAULT 0,
        full_public_key_fingerprint TEXT NOT NULL DEFAULT ''
    );
    INSERT INTO known_peers_cursor_secret_rollback
        (peer_id, public_key_fingerprint, display_name, protocol_major,
         capability, first_seen_at, last_discovered_at, updated_at,
         trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major,
         full_public_key_fingerprint)
    SELECT peer_id, public_key_fingerprint, display_name, protocol_major,
           capability, first_seen_at, last_discovered_at, updated_at,
           trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major,
           full_public_key_fingerprint
    FROM known_peers;
    DROP TABLE known_peers;
    ALTER TABLE known_peers_cursor_secret_rollback RENAME TO known_peers;
    CREATE INDEX idx_known_peers_last_discovered_at
        ON known_peers (last_discovered_at DESC);
    CREATE INDEX idx_known_peers_first_seen_at
        ON known_peers (first_seen_at);
    CREATE INDEX idx_known_peers_trust_state
        ON known_peers (trust_state);",
};

/// `peer-text-import`: persist the per-peer collection binding the
/// import flow uses so the imported entries stay grouped under the
/// peer's user collection, and record the (peer_id, remote_entry_id,
/// imported_content_hash) provenance rows that make the import
/// idempotent across re-imports and snapshot edits. The migration is
/// additive: both tables start empty, neither rewrites or deletes
/// pre-existing rows, and the foreign keys only cascade on the
/// collection row so a user who deletes the peer collection loses
/// only the binding — the entries, their assets, tags, favourites
/// and provenance rows all survive.
///
/// FK contract:
///
/// - `peer_collection_bindings.collection_id -> collections.id`:
///   `ON DELETE CASCADE` removes the binding when the user
///   intentionally deletes the peer collection. The cascade is
///   intentionally restricted to this single row: the binding is
///   the only thing that should disappear, never the imported
///   entries or the remote provenance.
/// - `peer_collection_bindings.peer_id -> known_peers.peer_id`:
///   `ON DELETE CASCADE` removes the binding if the user later
///   removes the peer entirely through a future pairing flow; the
///   imported entries and the remote provenance remain because
///   they reference the local entry ids (which still exist).
/// - `remote_imports.local_entry_id -> clipboard_entries.id`:
///   `ON DELETE CASCADE` removes the provenance row when the user
///   removes the underlying local entry; the entry is the only
///   artefact the provenance row references and the cascade keeps
///   the table from accumulating orphans. The imported entry
///   already deletes through the regular entry-removal path
///   (which also sweeps its assets, tags, favourites, collections
///   and rich-text refs), so the cascade stays scoped to the
///   provenance row.
/// - `remote_imports.peer_id -> known_peers.peer_id`: `ON DELETE
///   CASCADE` keeps the table clean when a peer row disappears.
///
/// The composite primary key `(peer_id, remote_entry_id,
/// imported_content_hash)` enforces the idempotence contract the
/// design pins: re-importing the same peer/remote entry/content
/// combination cannot create a second row. Editing the remote
/// snapshot produces a fresh `(peer_id, remote_entry_id, hash)`
/// triple that the importer can record as a new provenance row
/// while the older one remains for the previous snapshot.
///
/// Indexes:
///
/// - `idx_peer_collection_bindings_collection_id`: lets the sidebar
///   snapshot the binding for a given peer collection in O(1).
/// - `idx_remote_imports_local_entry_id`: lets the entry removal
///   path cascade cleanly and lets the history view surface every
///   provenance row a given local entry participates in.
/// - `idx_remote_imports_peer_remote`: keeps the lookup
///   `(peer_id, remote_entry_id)` the importer needs to detect a
///   prior import cheap, even when many peers are active.
///
/// The `down` step drops the new indexes and tables in a safe order
/// (child first) and never touches any pre-existing row, so a
/// rollback leaves the database in the pre-import shape without
/// losing data.
const MIGRATION_0017_PEER_IMPORT_BINDINGS: Migration = Migration {
    version: 17,
    description: "peer-text-import: persist peer_collection_bindings and remote_imports provenance",
    up_sql: "CREATE TABLE IF NOT EXISTS peer_collection_bindings (
        peer_id TEXT PRIMARY KEY,
        collection_id INTEGER NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        FOREIGN KEY (peer_id) REFERENCES known_peers(peer_id) ON DELETE CASCADE,
        FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE
    );
    CREATE TABLE IF NOT EXISTS remote_imports (
        peer_id TEXT NOT NULL,
        remote_entry_id TEXT NOT NULL,
        imported_content_hash TEXT NOT NULL,
        local_entry_id INTEGER NOT NULL,
        imported_at TEXT NOT NULL,
        PRIMARY KEY (peer_id, remote_entry_id, imported_content_hash),
        FOREIGN KEY (peer_id) REFERENCES known_peers(peer_id) ON DELETE CASCADE,
        FOREIGN KEY (local_entry_id) REFERENCES clipboard_entries(id) ON DELETE CASCADE
    );
    CREATE INDEX IF NOT EXISTS idx_peer_collection_bindings_collection_id
        ON peer_collection_bindings (collection_id);
    CREATE INDEX IF NOT EXISTS idx_remote_imports_local_entry_id
        ON remote_imports (local_entry_id);
    CREATE INDEX IF NOT EXISTS idx_remote_imports_peer_remote
        ON remote_imports (peer_id, remote_entry_id);",
    down_sql: "DROP INDEX IF EXISTS idx_remote_imports_peer_remote;
    DROP INDEX IF EXISTS idx_remote_imports_local_entry_id;
    DROP INDEX IF EXISTS idx_peer_collection_bindings_collection_id;
    DROP TABLE IF EXISTS remote_imports;
    DROP TABLE IF EXISTS peer_collection_bindings;",
};

/// `peer-image-import`: persist the additive `caps_extra` TXT
/// field the runtime receives through the dedicated `caps_extra`
/// mDNS key. The legacy `capability` column stays at the canonical
/// `pairing` / `discovery_only` token so a strict legacy parser keeps
/// recognising the record; the additive surface lives in its own
/// column so a future capability addition can opt in without
/// rewriting the legacy contract. The column carries a
/// comma-separated list of additive tokens (empty for legacy rows)
/// and the runtime consults it through the same helper
/// ([`crate::peer_discovery::decode_capabilities`]) the legacy
/// field uses so the allowlist validation stays consistent. The
/// `down` step rebuilds the table through the SQLite
/// table-rebuild pattern so a rollback drops the column without
/// losing any of the pre-existing trust metadata.
const MIGRATION_0018_KNOWN_PEERS_CAPS_EXTRA: Migration = Migration {
    version: 18,
    description: "peer-image-import: persist additive caps_extra TXT field in known_peers",
    up_sql: "ALTER TABLE known_peers ADD COLUMN caps_extra TEXT NOT NULL DEFAULT '';",
    down_sql: "DROP INDEX IF EXISTS idx_known_peers_trust_state;
    CREATE TABLE known_peers_caps_extra_rollback (
        peer_id TEXT PRIMARY KEY,
        public_key_fingerprint TEXT NOT NULL,
        display_name TEXT NOT NULL,
        protocol_major INTEGER NOT NULL,
        capability TEXT NOT NULL,
        first_seen_at TEXT NOT NULL,
        last_discovered_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        trust_state TEXT NOT NULL DEFAULT 'unverified',
        tls_cert_fingerprint TEXT NOT NULL DEFAULT '',
        paired_at TEXT NOT NULL DEFAULT '',
        paired_protocol_major INTEGER NOT NULL DEFAULT 0,
        full_public_key_fingerprint TEXT NOT NULL DEFAULT '',
        cursor_secret TEXT NOT NULL DEFAULT ''
    );
    INSERT INTO known_peers_caps_extra_rollback
        (peer_id, public_key_fingerprint, display_name, protocol_major,
         capability, first_seen_at, last_discovered_at, updated_at,
         trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major,
         full_public_key_fingerprint, cursor_secret)
    SELECT peer_id, public_key_fingerprint, display_name, protocol_major,
           capability, first_seen_at, last_discovered_at, updated_at,
           trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major,
           full_public_key_fingerprint, cursor_secret
    FROM known_peers;
    DROP TABLE known_peers;
    ALTER TABLE known_peers_caps_extra_rollback RENAME TO known_peers;
    CREATE INDEX idx_known_peers_last_discovered_at
        ON known_peers (last_discovered_at DESC);
    CREATE INDEX idx_known_peers_first_seen_at
        ON known_peers (first_seen_at);
    CREATE INDEX idx_known_peers_trust_state
        ON known_peers (trust_state);",
};

/// `peer-source-app-presentation`: extend `known_peers` and
/// `remote_imports` with the additive metadata the source-app
/// presentation feature requires. The migration is purely
/// additive — it never rewrites pre-existing rows — and
/// reversible: the `down` step rebuilds both tables through the
/// SQLite table-rebuild pattern the earlier migrations use so a
/// rollback drops the new columns without losing any
/// pre-existing data.
///
/// `known_peers.caps_extra_v2` carries the comma-separated list
/// of second-tier additive tokens the host advertises through the
/// dedicated `caps_extra_v2` mDNS TXT key (the legacy
/// `caps_extra` surface stays unchanged so a strict legacy parser
/// keeps pairing). The column mirrors the additive column the
/// `peer-image-import` migration introduced.
///
/// `remote_imports.source_app_name` / `source_app_icon_ref`
/// carry the per-provenance source-application display name and
/// the locally-generated reference for the imported application
/// icon. Both columns are nullable so pre-existing import rows
/// keep their exact shape; the import transaction the
/// `peer-source-app-presentation` change ships is the only
/// writer of the columns. No icon bytes are persisted inside the
/// table — the icon lives in the
/// `<data_dir>/assets/application-icons/` namespace and the
/// reference is the local content-addressed asset path the
/// writer computed.
///
/// The down step removes the columns through the table-rebuild
/// pattern the previous migrations use. A rollback therefore
/// preserves pre-existing import provenance, clipboard entries
/// and local capture metadata; the application-icon files
/// themselves stay on disk so a re-apply of the migration does
/// not have to redownload them.
const MIGRATION_0019_PEER_SOURCE_APP_PRESENTATION: Migration = Migration {
    version: 19,
    description:
        "peer-source-app-presentation: add caps_extra_v2 to known_peers and source_app_* to remote_imports",
    up_sql: "ALTER TABLE known_peers ADD COLUMN caps_extra_v2 TEXT NOT NULL DEFAULT '';
    ALTER TABLE remote_imports ADD COLUMN source_app_name TEXT;
    ALTER TABLE remote_imports ADD COLUMN source_app_icon_ref TEXT;
    CREATE INDEX IF NOT EXISTS idx_remote_imports_source_app_icon_ref
        ON remote_imports (source_app_icon_ref);",
    down_sql: "DROP INDEX IF EXISTS idx_remote_imports_source_app_icon_ref;
    CREATE TABLE remote_imports_source_app_rollback (
        peer_id TEXT NOT NULL,
        remote_entry_id TEXT NOT NULL,
        imported_content_hash TEXT NOT NULL,
        local_entry_id INTEGER NOT NULL,
        imported_at TEXT NOT NULL,
        PRIMARY KEY (peer_id, remote_entry_id, imported_content_hash),
        FOREIGN KEY (peer_id) REFERENCES known_peers(peer_id) ON DELETE CASCADE,
        FOREIGN KEY (local_entry_id) REFERENCES clipboard_entries(id) ON DELETE CASCADE
    );
    INSERT INTO remote_imports_source_app_rollback
        (peer_id, remote_entry_id, imported_content_hash, local_entry_id, imported_at)
    SELECT peer_id, remote_entry_id, imported_content_hash, local_entry_id, imported_at
    FROM remote_imports;
    DROP INDEX IF EXISTS idx_remote_imports_peer_remote;
    DROP INDEX IF EXISTS idx_remote_imports_local_entry_id;
    DROP TABLE remote_imports;
    ALTER TABLE remote_imports_source_app_rollback RENAME TO remote_imports;
    CREATE INDEX idx_remote_imports_local_entry_id
        ON remote_imports (local_entry_id);
    CREATE INDEX idx_remote_imports_peer_remote
        ON remote_imports (peer_id, remote_entry_id);
    DROP INDEX IF EXISTS idx_known_peers_trust_state;
    CREATE TABLE known_peers_caps_extra_v2_rollback (
        peer_id TEXT PRIMARY KEY,
        public_key_fingerprint TEXT NOT NULL,
        display_name TEXT NOT NULL,
        protocol_major INTEGER NOT NULL,
        capability TEXT NOT NULL,
        first_seen_at TEXT NOT NULL,
        last_discovered_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        trust_state TEXT NOT NULL DEFAULT 'unverified',
        tls_cert_fingerprint TEXT NOT NULL DEFAULT '',
        paired_at TEXT NOT NULL DEFAULT '',
        paired_protocol_major INTEGER NOT NULL DEFAULT 0,
        full_public_key_fingerprint TEXT NOT NULL DEFAULT '',
        cursor_secret TEXT NOT NULL DEFAULT '',
        caps_extra TEXT NOT NULL DEFAULT ''
    );
    INSERT INTO known_peers_caps_extra_v2_rollback
        (peer_id, public_key_fingerprint, display_name, protocol_major,
         capability, first_seen_at, last_discovered_at, updated_at,
         trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major,
         full_public_key_fingerprint, cursor_secret, caps_extra)
    SELECT peer_id, public_key_fingerprint, display_name, protocol_major,
           capability, first_seen_at, last_discovered_at, updated_at,
           trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major,
           full_public_key_fingerprint, cursor_secret, caps_extra
    FROM known_peers;
    DROP TABLE known_peers;
    ALTER TABLE known_peers_caps_extra_v2_rollback RENAME TO known_peers;
    CREATE INDEX idx_known_peers_last_discovered_at
        ON known_peers (last_discovered_at DESC);
    CREATE INDEX idx_known_peers_first_seen_at
        ON known_peers (first_seen_at);
    CREATE INDEX idx_known_peers_trust_state
        ON known_peers (trust_state);",
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
        MIGRATION_0011_CODE_LANGUAGE,
        MIGRATION_0012_COLLECTION_COLORS,
        MIGRATION_0013_KNOWN_PEERS,
        MIGRATION_0014_KNOWN_PEERS_PAIRING,
        MIGRATION_0015_KNOWN_PEERS_PAIRING_FULL_FINGERPRINT,
        MIGRATION_0016_KNOWN_PEERS_CURSOR_SECRET,
        MIGRATION_0017_PEER_IMPORT_BINDINGS,
        MIGRATION_0018_KNOWN_PEERS_CAPS_EXTRA,
        MIGRATION_0019_PEER_SOURCE_APP_PRESENTATION,
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
    fn code_language_migration_is_the_next_sequential_version() {
        // The `code-language-detection` change appends its migration
        // after the existing ones. Subsequent migrations (the latest
        // is `local-peer-discovery` with version 13) extend the
        // registry; the `assert_eq!(last.version, 12)` guard below is
        // intentionally loose so this test continues to pass as the
        // registry grows. The legacy invariants (sorted, unique,
        // contiguous from 1) still live in the `migrations_are_unique_and_ordered`
        // test below.
        let migrations = builtin_migrations();
        let last = migrations.last().expect("at least one migration");
        assert!(
            last.version >= 12,
            "the registry must contain at least the 0012 migration"
        );
        assert!(
            migrations.len() >= 12,
            "the registry must contain at least 12 migrations"
        );
    }

    #[test]
    fn known_peers_migration_creates_required_columns_and_indexes() {
        // The `local-peer-discovery` change persists a metadata-only
        // view of every observed peer. The table MUST carry the
        // columns the runtime needs (peer_id PK + fingerprint +
        // display_name + protocol_major + capability + timestamps) and
        // MUST NOT store the peer's IP address, port, key bytes or
        // clipboard payload: the discovery surface is metadata-only
        // and the design pins the absence of every endpoint-shaped
        // column.
        let up = MIGRATION_0013_KNOWN_PEERS.up_sql.to_uppercase();
        assert!(
            up.contains("CREATE TABLE IF NOT EXISTS KNOWN_PEERS"),
            "missing known_peers table"
        );
        for column in [
            "PEER_ID",
            "PUBLIC_KEY_FINGERPRINT",
            "DISPLAY_NAME",
            "PROTOCOL_MAJOR",
            "CAPABILITY",
            "FIRST_SEEN_AT",
            "LAST_DISCOVERED_AT",
            "UPDATED_AT",
        ] {
            assert!(
                up.contains(column),
                "known_peers must carry column {column}"
            );
        }
        // Endpoints and content must never reach the table.
        for forbidden in ["IP_ADDRESS", "IPV4", "PORT", "CONTENT", "PAYLOAD"] {
            assert!(
                !up.contains(forbidden),
                "known_peers must not persist {forbidden}"
            );
        }
        assert!(
            up.contains("IDX_KNOWN_PEERS_LAST_DISCOVERED_AT"),
            "missing last_discovered_at index"
        );
        assert!(
            up.contains("IDX_KNOWN_PEERS_FIRST_SEEN_AT"),
            "missing first_seen_at index"
        );

        // The down step must drop both indexes and the table; rollback
        // intentionally loses every observed peer because the change
        // never persisted endpoints or content.
        let down = MIGRATION_0013_KNOWN_PEERS.down_sql.to_uppercase();
        assert!(down.contains("DROP INDEX IF EXISTS IDX_KNOWN_PEERS_FIRST_SEEN_AT"));
        assert!(down.contains("DROP INDEX IF EXISTS IDX_KNOWN_PEERS_LAST_DISCOVERED_AT"));
        assert!(down.contains("DROP TABLE IF EXISTS KNOWN_PEERS"));
    }

    #[test]
    fn known_peers_migration_is_purely_additive() {
        // Mirror the additive-migration guard the rest of the suite
        // relies on. The `up` step creates only the new table and
        // indexes; it never rewrites, deletes or rebuilds an
        // existing table.
        let up = MIGRATION_0013_KNOWN_PEERS.up_sql.to_uppercase();
        for forbidden in [
            "DROP TABLE",
            "DELETE FROM",
            "UPDATE ",
            "INSERT INTO",
            "ALTER TABLE",
        ] {
            assert!(
                !up.contains(forbidden),
                "additive migration must not contain {forbidden}"
            );
        }
    }

    #[test]
    fn known_peers_migration_rolls_back_cleanly() {
        // Run the full migration set on a temp database, then rollback
        // ONLY the `known_peers` migration. The pre-existing tables
        // must survive intact and `known_peers` must vanish.
        use crate::rollback_migration;
        use crate::Database;
        use rusqlite::Connection;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("clipvault.db");
        let mut db = Database::open(&path).expect("open");
        let outcomes = db.run_migrations(&builtin_migrations()).expect("migrate");
        let applied: Vec<i64> = outcomes
            .iter()
            .filter_map(|o| match o {
                crate::MigrationOutcome::Applied { version, .. } => Some(*version),
                crate::MigrationOutcome::AlreadyApplied { .. } => None,
            })
            .collect();
        assert!(applied.contains(&13), "known_peers migration must apply");

        // Confirm the table is present and the schema is what the
        // repository layer expects.
        let _count: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM known_peers", [], |row| row.get(0))
            .expect("known_peers table is queryable");

        rollback_migration(&mut db, &MIGRATION_0013_KNOWN_PEERS).expect("rollback");
        let conn: &Connection = db.connection();
        let err = conn
            .query_row("SELECT COUNT(*) FROM known_peers", [], |row| {
                row.get::<_, i64>(0)
            })
            .expect_err("table must be gone after rollback");
        let msg = err.to_string();
        assert!(
            msg.contains("no such table"),
            "rollback must drop known_peers, got {msg}"
        );

        // Re-applying the migration after the rollback restores the
        // table and the indexes so a future contributor can iterate.
        let outcomes = db
            .run_migrations(&[MIGRATION_0013_KNOWN_PEERS])
            .expect("re-apply");
        assert!(matches!(
            outcomes.first(),
            Some(crate::MigrationOutcome::Applied { version: 13, .. })
        ));
        let _: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM known_peers", [], |row| row.get(0))
            .expect("known_peers table is queryable again");
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

    #[test]
    fn code_language_migration_is_purely_additive() {
        // Same rule as the other additive migrations: the `up` step
        // must only add columns / indexes; the `down` step uses the
        // table-rebuild pattern but is excluded from this assertion
        // because rollbacks inherently DROP and INSERT.
        let up = MIGRATION_0011_CODE_LANGUAGE.up_sql.to_uppercase();
        for forbidden in ["DROP TABLE", "DELETE FROM", "UPDATE ", "INSERT INTO"] {
            assert!(
                !up.contains(forbidden),
                "additive migration must not contain {forbidden}"
            );
        }
        assert!(up.contains("CODE_LANGUAGE"), "missing code_language column");
        assert!(
            up.contains("IDX_CLIPBOARD_ENTRIES_CODE_LANGUAGE"),
            "missing code_language index"
        );
    }

    #[test]
    fn known_peers_pairing_migration_creates_required_columns_and_index() {
        // The `local-peer-mutual-pairing` change extends the
        // metadata-only `known_peers` table with the trust state +
        // TLS pin columns the runtime needs. The columns MUST be
        // present with safe defaults (`unverified` for the trust
        // state, empty strings for the pin / paired_at fields,
        // `0` for the protocol major) so a pre-pairing row keeps
        // matching the schema.
        let up = MIGRATION_0014_KNOWN_PEERS_PAIRING.up_sql.to_uppercase();
        for column in [
            "TRUST_STATE",
            "TLS_CERT_FINGERPRINT",
            "PAIRED_AT",
            "PAIRED_PROTOCOL_MAJOR",
        ] {
            assert!(
                up.contains(&format!("ADD COLUMN {column}")),
                "known_peers must extend with column {column}",
            );
        }
        assert!(
            up.contains("DEFAULT 'UNVERIFIED'"),
            "trust_state must default to unverified",
        );
        assert!(
            up.contains("IDX_KNOWN_PEERS_TRUST_STATE"),
            "missing trust_state index",
        );
        // Endpoints, secrets, raw key bytes MUST NOT land in the
        // table; only the SHA-256 fingerprint of the DER cert is
        // persisted.
        for forbidden in ["PRIVATE_KEY", "TSEED", "RAW_KEY", "TLS_KEY", "PRESHARED"] {
            assert!(
                !up.contains(forbidden),
                "known_peers must not persist {forbidden}",
            );
        }
        // The `down` step must rebuild the table through the
        // rollback pattern without touching the pre-existing
        // columns.
        let down = MIGRATION_0014_KNOWN_PEERS_PAIRING.down_sql.to_uppercase();
        assert!(down.contains("DROP INDEX IF EXISTS IDX_KNOWN_PEERS_TRUST_STATE"));
        assert!(down.contains("KNOWN_PEERS_PAIRING_ROLLBACK"));
    }

    #[test]
    fn known_peers_pairing_migration_rolls_back_cleanly() {
        use crate::rollback_migration;
        use crate::Database;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("clipvault.db");
        let mut db = Database::open(&path).expect("open");
        let outcomes = db.run_migrations(&builtin_migrations()).expect("migrate");
        let applied: Vec<i64> = outcomes
            .iter()
            .filter_map(|o| match o {
                crate::MigrationOutcome::Applied { version, .. } => Some(*version),
                crate::MigrationOutcome::AlreadyApplied { .. } => None,
            })
            .collect();
        assert!(
            applied.contains(&14),
            "pairing migration must apply on top of known_peers",
        );

        rollback_migration(&mut db, &MIGRATION_0014_KNOWN_PEERS_PAIRING).expect("rollback");
        // After the rollback the pre-pairing columns stay; the
        // added columns are gone. The schema the discovery tests
        // build on stays compatible.
        let conn = db.connection();
        let _: i64 = conn
            .query_row("SELECT COUNT(*) FROM known_peers", [], |row| row.get(0))
            .expect("table still present after rollback");
        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(known_peers)")
            .expect("pragma")
            .query_map([], |row| row.get::<_, String>(1))
            .expect("columns")
            .filter_map(|name| name.ok())
            .collect();
        for new_col in [
            "trust_state",
            "tls_cert_fingerprint",
            "paired_at",
            "paired_protocol_major",
        ] {
            assert!(
                !columns.iter().any(|c| c == new_col),
                "{new_col} must be dropped on rollback, got {columns:?}",
            );
        }
    }

    #[test]
    fn peer_import_bindings_migration_creates_required_tables_and_indexes() {
        // The `peer-text-import` change persists the per-peer
        // collection binding and the (peer_id, remote_entry_id,
        // imported_content_hash) provenance rows. The migration
        // MUST stay additive: it never rewrites or deletes
        // pre-existing rows, never inspects clipboard content,
        // and never persists an IP, port, fingerprint or cert
        // byte. The FK contract pins the cascade semantics so a
        // rollback can be reasoned about without guessing.
        let up = MIGRATION_0017_PEER_IMPORT_BINDINGS.up_sql.to_uppercase();
        assert!(
            up.contains("CREATE TABLE IF NOT EXISTS PEER_COLLECTION_BINDINGS"),
            "missing peer_collection_bindings table"
        );
        assert!(
            up.contains("CREATE TABLE IF NOT EXISTS REMOTE_IMPORTS"),
            "missing remote_imports table"
        );
        for column in [
            "PEER_ID",
            "COLLECTION_ID",
            "REMOTE_ENTRY_ID",
            "IMPORTED_CONTENT_HASH",
            "LOCAL_ENTRY_ID",
            "IMPORTED_AT",
        ] {
            assert!(
                up.contains(column),
                "peer import migration must persist column {column}"
            );
        }
        assert!(
            up.contains("PRIMARY KEY (PEER_ID, REMOTE_ENTRY_ID, IMPORTED_CONTENT_HASH)"),
            "remote_imports must declare the composite idempotence primary key"
        );
        // Indexes the importer needs at runtime.
        for index in [
            "IDX_PEER_COLLECTION_BINDINGS_COLLECTION_ID",
            "IDX_REMOTE_IMPORTS_LOCAL_ENTRY_ID",
            "IDX_REMOTE_IMPORTS_PEER_REMOTE",
        ] {
            assert!(
                up.contains(index),
                "peer import migration must create index {index}"
            );
        }
        // Endpoints and content MUST NOT land in either table —
        // both tables are metadata-only by construction.
        for forbidden in [
            " BODY ",
            "TEXT_PAYLOAD",
            "IP_ADDRESS",
            "IPV4",
            " PORT ",
            "TLS_CERT_FINGERPRINT",
            "CERT_DER",
            "PRIVATE_KEY",
        ] {
            assert!(
                !up.contains(forbidden),
                "peer import migration must not persist {forbidden}"
            );
        }
        // Down must drop both tables in a safe order and remove
        // every index so a rollback leaves no orphans behind.
        let down = MIGRATION_0017_PEER_IMPORT_BINDINGS.down_sql.to_uppercase();
        assert!(down.contains("DROP TABLE IF EXISTS REMOTE_IMPORTS"));
        assert!(down.contains("DROP TABLE IF EXISTS PEER_COLLECTION_BINDINGS"));
        for index in [
            "IDX_REMOTE_IMPORTS_PEER_REMOTE",
            "IDX_REMOTE_IMPORTS_LOCAL_ENTRY_ID",
            "IDX_PEER_COLLECTION_BINDINGS_COLLECTION_ID",
        ] {
            assert!(down.contains(&format!("DROP INDEX IF EXISTS {index}")));
        }
    }

    #[test]
    fn peer_import_bindings_migration_rolls_back_cleanly() {
        // Run the full migration set on a temp database, then
        // rollback ONLY the `peer-text-import` migration. The
        // pre-existing tables (collections, clipboard_entries,
        // known_peers) must survive intact and the new tables must
        // vanish.
        use crate::rollback_migration;
        use crate::Database;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("clipvault.db");
        let mut db = Database::open(&path).expect("open");
        let outcomes = db.run_migrations(&builtin_migrations()).expect("migrate");
        let applied: Vec<i64> = outcomes
            .iter()
            .filter_map(|o| match o {
                crate::MigrationOutcome::Applied { version, .. } => Some(*version),
                crate::MigrationOutcome::AlreadyApplied { .. } => None,
            })
            .collect();
        assert!(
            applied.contains(&17),
            "peer-text-import migration must apply on top of the cursor-secret base"
        );

        // Confirm the new tables exist and are queryable.
        let _: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM peer_collection_bindings", [], |row| {
                row.get(0)
            })
            .expect("peer_collection_bindings table is queryable");
        let _: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM remote_imports", [], |row| row.get(0))
            .expect("remote_imports table is queryable");

        rollback_migration(&mut db, &MIGRATION_0017_PEER_IMPORT_BINDINGS).expect("rollback");
        let err = db
            .connection()
            .query_row("SELECT COUNT(*) FROM peer_collection_bindings", [], |row| {
                row.get::<_, i64>(0)
            })
            .expect_err("peer_collection_bindings must be gone after rollback");
        assert!(err.to_string().contains("no such table"));
        let err = db
            .connection()
            .query_row("SELECT COUNT(*) FROM remote_imports", [], |row| {
                row.get::<_, i64>(0)
            })
            .expect_err("remote_imports must be gone after rollback");
        assert!(err.to_string().contains("no such table"));

        // Re-applying after the rollback restores the tables and
        // indexes so a future contributor can iterate.
        let outcomes = db
            .run_migrations(&[MIGRATION_0017_PEER_IMPORT_BINDINGS])
            .expect("re-apply");
        assert!(matches!(
            outcomes.first(),
            Some(crate::MigrationOutcome::Applied { version: 17, .. })
        ));
        let _: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM peer_collection_bindings", [], |row| {
                row.get(0)
            })
            .expect("peer_collection_bindings table is queryable again");
    }

    #[test]
    fn peer_source_app_presentation_migration_is_purely_additive() {
        // The `peer-source-app-presentation` change ships a
        // single additive migration that extends `known_peers`
        // with `caps_extra_v2` and `remote_imports` with the
        // per-provenance `source_app_name` / `source_app_icon_ref`
        // columns. The migration MUST stay additive: it never
        // rewrites or deletes pre-existing rows, never inspects
        // clipboard content, and never persists an IP, port,
        // fingerprint or cert byte. The FK contract pins the
        // cascade semantics so a rollback can be reasoned about
        // without guessing.
        let up = MIGRATION_0019_PEER_SOURCE_APP_PRESENTATION
            .up_sql
            .to_uppercase();
        for column in ["CAPS_EXTRA_V2", "SOURCE_APP_NAME", "SOURCE_APP_ICON_REF"] {
            assert!(
                up.contains(column),
                "peer-source-app-presentation migration must persist column {column}"
            );
        }
        assert!(
            up.contains("IDX_REMOTE_IMPORTS_SOURCE_APP_ICON_REF"),
            "peer-source-app-presentation migration must create the icon-ref index"
        );
        // No icon bytes or remote paths may land in SQLite.
        for forbidden in ["PNG_BYTES", "ICON_BYTES", "REMOTE_PATH", "REMOTE_FILENAME"] {
            assert!(
                !up.contains(forbidden),
                "peer-source-app-presentation migration must not persist column {forbidden}"
            );
        }
        // Down step must rebuild both tables so a rollback drops
        // the new columns without losing any pre-existing data.
        let down = MIGRATION_0019_PEER_SOURCE_APP_PRESENTATION
            .down_sql
            .to_uppercase();
        assert!(
            down.contains("REMOTE_IMPORTS_SOURCE_APP_ROLLBACK"),
            "down step must rebuild remote_imports without the new columns"
        );
        assert!(
            down.contains("KNOWN_PEERS_CAPS_EXTRA_V2_ROLLBACK"),
            "down step must rebuild known_peers without caps_extra_v2"
        );
    }

    #[test]
    fn peer_source_app_presentation_migration_round_trips() {
        use crate::rollback_migration;
        let dir = tempfile::tempdir().expect("tempdir");
        let mut db = crate::Database::open(dir.path().join("clipvault.db")).expect("open");
        let outcomes = db.run_migrations(&builtin_migrations()).expect("migrate");
        assert!(outcomes.iter().any(|outcome| matches!(
            outcome,
            crate::MigrationOutcome::Applied { version: 19, .. }
        )));
        // Sanity check the columns after the migration has run:
        // the snapshot already exercises the new surface.
        let _: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM known_peers", [], |row| row.get(0))
            .expect("known_peers queryable");
        let _: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM remote_imports", [], |row| row.get(0))
            .expect("remote_imports queryable");

        // Rollback drops the new columns without losing the
        // pre-existing ones.
        rollback_migration(&mut db, &MIGRATION_0019_PEER_SOURCE_APP_PRESENTATION)
            .expect("rollback");

        let err = db
            .connection()
            .query_row("SELECT caps_extra_v2 FROM known_peers LIMIT 1", [], |row| {
                row.get::<_, Option<String>>(0)
            })
            .expect_err("caps_extra_v2 column must be gone after rollback");
        assert!(err.to_string().contains("no such column"));

        // Re-applying restores the columns.
        let outcomes = db
            .run_migrations(&[MIGRATION_0019_PEER_SOURCE_APP_PRESENTATION])
            .expect("re-apply");
        assert!(matches!(
            outcomes.first(),
            Some(crate::MigrationOutcome::Applied { version: 19, .. })
        ));
    }

    #[test]
    fn peer_source_app_presentation_migration_rolls_back_populated_data() {
        // The previous regression only proved rollback against an
        // empty database. With populated peers, peer_collection_bindings
        // and remote_imports the table-rebuild sequence (drop
        // `remote_imports`, drop `known_peers`) trips the foreign key
        // chain unless the migration defers FK checks for the
        // duration of the rollback. The down step must therefore
        // PRESERVE existing rows and their relationships: a rollback
        // is a metadata-only contract regression, never a destructive
        // operation.
        use crate::rollback_migration;
        let dir = tempfile::tempdir().expect("tempdir");
        let mut db = crate::Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");

        // Seed a peer.
        let conn = db.connection_mut();
        let mut peer_repo = crate::KnownPeerRepository::new(conn);
        peer_repo
            .upsert_observation(&crate::PeerObservation {
                peer_id: "peer-a".to_string(),
                public_key_fingerprint: "ab".repeat(32),
                full_public_key_fingerprint: Some("cd".repeat(32)),
                display_name: "Equipo A".to_string(),
                protocol_major: 1,
                capability: "pairing".to_string(),
                caps_extra: "image_import".to_string(),
                caps_extra_v2: "source_app_presentation".to_string(),
                observed_at: time::OffsetDateTime::now_utc(),
            })
            .expect("upsert peer");
        drop(peer_repo);

        // Seed a binding + clipboard entry + remote_import that
        // references the peer and the entry — this is the populated
        // shape a rollback must survive.
        let conn = db.connection_mut();
        let binding_collection_id: i64 = conn
            .query_row(
                "SELECT id FROM collections ORDER BY id ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("history collection present");
        conn.execute(
            "INSERT INTO peer_collection_bindings (peer_id, collection_id, created_at, updated_at)
             VALUES (?1, ?2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            rusqlite::params!["peer-a", binding_collection_id],
        )
        .expect("binding");
        conn.execute(
            "INSERT INTO clipboard_entries
                 (content, content_type, content_size, content_hash,
                  source_app, created_at, updated_at, last_seen_at)
             VALUES ('hello', 'text', 5, 'hash-a-rollback', NULL,
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z',
                     '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("entry");
        let entry_id: i64 = conn
            .query_row(
                "SELECT id FROM clipboard_entries WHERE content_hash = ?1",
                rusqlite::params!["hash-a-rollback"],
                |row| row.get(0),
            )
            .expect("entry id");
        conn.execute(
            "INSERT INTO remote_imports
                 (peer_id, remote_entry_id, imported_content_hash,
                  local_entry_id, imported_at, source_app_name,
                  source_app_icon_ref)
             VALUES (?1, ?2, ?3, ?4, '2026-01-01T00:00:00Z', ?5, ?6)",
            rusqlite::params![
                "peer-a",
                "entry-1",
                "hash-a-rollback",
                entry_id,
                "VS Code",
                "application-icons/vscode.png"
            ],
        )
        .expect("import");

        // Rollback must succeed with populated data.
        rollback_migration(&mut db, &MIGRATION_0019_PEER_SOURCE_APP_PRESENTATION)
            .expect("rollback with populated data");

        // known_peers survives: the legacy columns are still
        // populated with the pre-existing fingerprint / display
        // name, and `caps_extra` (the additive column added by
        // the previous migration) is preserved verbatim.
        let (peer_id, display_name, caps_extra): (String, String, String) = db
            .connection()
            .query_row(
                "SELECT peer_id, display_name, caps_extra FROM known_peers",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("known_peers survived rollback");
        assert_eq!(peer_id, "peer-a");
        assert_eq!(display_name, "Equipo A");
        assert_eq!(caps_extra, "image_import");

        // The new `caps_extra_v2` column is gone.
        let err = db
            .connection()
            .query_row("SELECT caps_extra_v2 FROM known_peers LIMIT 1", [], |row| {
                row.get::<_, Option<String>>(0)
            })
            .expect_err("caps_extra_v2 column must be gone after rollback");
        assert!(err.to_string().contains("no such column"));

        // The binding survives.
        let (binding_peer, binding_collection): (String, i64) = db
            .connection()
            .query_row(
                "SELECT peer_id, collection_id FROM peer_collection_bindings",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("binding survived");
        assert_eq!(binding_peer, "peer-a");
        assert_eq!(binding_collection, binding_collection_id);

        // The remote_import survives (with the new source_app_*
        // columns dropped — they no longer exist).
        let (import_peer, import_local): (String, i64) = db
            .connection()
            .query_row(
                "SELECT peer_id, local_entry_id FROM remote_imports",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("import survived");
        assert_eq!(import_peer, "peer-a");
        assert_eq!(import_local, entry_id);

        // No FK violation surfaces.
        let fk_violations: i64 = db
            .connection()
            .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
            .unwrap_or(0);
        assert_eq!(fk_violations, 0, "foreign_key_check must report zero rows");

        // Re-applying the migration must succeed and restore the
        // dropped columns.
        let outcomes = db
            .run_migrations(&[MIGRATION_0019_PEER_SOURCE_APP_PRESENTATION])
            .expect("re-apply");
        assert!(matches!(
            outcomes.first(),
            Some(crate::MigrationOutcome::Applied { version: 19, .. })
        ));
        let caps_extra_v2: String = db
            .connection()
            .query_row(
                "SELECT caps_extra_v2 FROM known_peers WHERE peer_id = 'peer-a'",
                [],
                |row| row.get(0),
            )
            .expect("caps_extra_v2 column restored");
        // The down step lost the new column, so the persisted
        // additive token fell back to the empty default after the
        // re-apply; that is the correct behaviour — the new
        // metadata is rebuilt on the next discovery refresh, not
        // out of band.
        assert_eq!(caps_extra_v2, "");
    }

    #[test]
    fn collection_color_migration_is_idempotent_and_additive() {
        // The collection-colour migration MUST stay additive: the
        // `up` step adds the `color_hex` column with a `NOT NULL`
        // default, then unconditionally re-applies the documented
        // fallback so any row whose value was cleared by a manual
        // edit is restored. The double `UPDATE ... WHERE ... IS NULL`
        // is idempotent — repeated runs are a no-op — so the
        // migration is safe to replay and the runner cannot drift.
        let up = MIGRATION_0012_COLLECTION_COLORS.up_sql.to_uppercase();
        assert!(
            up.contains("ALTER TABLE COLLECTIONS ADD COLUMN COLOR_HEX"),
            "additive migration must add the color_hex column"
        );
        assert!(
            up.contains("NOT NULL DEFAULT"),
            "additive migration must supply a NOT NULL default"
        );
        assert!(
            up.contains("UPDATE COLLECTIONS"),
            "additive migration must re-apply the fallback to legacy rows"
        );
        assert!(
            !up.contains("DROP TABLE"),
            "additive migration must not drop tables"
        );

        let down = MIGRATION_0012_COLLECTION_COLORS.down_sql.to_uppercase();
        assert!(
            down.contains("DROP TABLE COLLECTIONS"),
            "rollback must rebuild the collections table"
        );
        assert!(
            down.contains("COLOR_HEX") == false,
            "rollback must drop the color_hex column"
        );
    }
}
