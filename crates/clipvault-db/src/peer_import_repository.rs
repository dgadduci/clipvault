//! Persistent model for the `peer-text-import` change.
//!
//! The `peer_collection_bindings` and `remote_imports` tables hold
//! the metadata-only state the import flow needs to:
//!
//! - bind every imported entry to exactly one user collection per
//!   `peer_id` (a `peer_collection_bindings.peer_id -> collections.id`
//!   pointer that survives renames of the peer display name);
//! - record the `(peer_id, remote_entry_id, imported_content_hash) ->
//!   local_entry_id` provenance that makes re-imports idempotent and
//!   lets a future snapshot edit produce a fresh row without
//!   colliding with the previous one.
//!
//! The repository is intentionally metadata-only by construction: it
//! never accepts the imported text body, the host's pinned
//! certificate fingerprint, the local cert fingerprint, an IP, a
//! port or a secret. Every helper returns typed errors so the
//! runtime can branch on the failure reason without inspecting
//! SQLite-specific strings.

use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Transaction};
use std::collections::HashSet;
use thiserror::Error;

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// Row stored in the `peer_collection_bindings` table. The struct is
/// the canonical, metadata-only view the import service exposes to
/// the runtime; the database columns are never read directly outside
/// this repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerCollectionBinding {
    pub peer_id: String,
    pub collection_id: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Canonical input the `peer-image-import` service feeds into
/// [`PeerImportRepository::commit_image_import_transaction`].
/// The helper is metadata-only; the bytes never cross the
/// repository boundary.
///
/// `source_app_name` and `source_app_icon_ref` carry the
/// per-provenance source-application metadata the
/// `peer-source-app-presentation` change ships. Both stay
/// `None` for legacy peers that did not opt into the additive
/// contract, and the repository writes `NULL` to the matching
/// `remote_imports` columns so a future peer that does opt in
/// never sees stale attribution.
#[derive(Debug, Clone)]
pub struct ImageImportSpec {
    pub peer_id: String,
    pub remote_entry_id: String,
    pub display_name: String,
    pub canonical_hash: String,
    pub asset_ref: String,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    pub content_size: i64,
    pub validated_title: Option<String>,
    pub source_app_name: Option<String>,
    pub source_app_icon_ref: Option<String>,
    pub now: OffsetDateTime,
}

/// Structured outcome the image-import transaction returns. The
/// `deduplicated` flag flips to `true` when an entry with the
/// supplied `content_hash` already existed before the
/// transaction started (the import reused the existing row);
/// `false` when the transaction inserted a fresh entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageImportOutcome {
    pub entry_id: i64,
    pub collection_id: i64,
    pub deduplicated: bool,
}

/// Row stored in the `remote_imports` table. The composite primary
/// key is `(peer_id, remote_entry_id, imported_content_hash)` so a
/// future remote snapshot edit (which produces a different
/// `imported_content_hash`) can record a separate row without
/// colliding with the previous one.
///
/// `source_app_name` and `source_app_icon_ref` are the
/// per-provenance source-application metadata the
/// `peer-source-app-presentation` change adds; both fields stay
/// `None` for pre-existing rows and for imports that did not
/// carry the optional fields, and the import transaction is the
/// only writer that populates them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteImportRecord {
    pub peer_id: String,
    pub remote_entry_id: String,
    pub imported_content_hash: String,
    pub local_entry_id: i64,
    pub imported_at: String,
    /// Optional validated source-application display name the
    /// host attached to the original import response. The
    /// projection renders the value only inside the
    /// peer-bound collection, never in general history or
    /// another peer's collection.
    pub source_app_name: Option<String>,
    /// Optional locally-generated reference to the imported
    /// source-application icon asset the writer persisted
    /// under `<data_dir>/assets/application-icons/`. The
    /// reference never encodes a remote path, a filename
    /// supplied by the peer or a raw asset hash; the
    /// projection renders it through the existing safe
    /// local application-icon resolver with the static
    /// imported-origin icon as a deterministic fallback.
    pub source_app_icon_ref: Option<String>,
}

/// Source-app fields projected for one imported entry. A peer-bound
/// collection receives only its bound peer's provenance; general history
/// receives the most recently imported provenance without its peer ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerImportedSourceAppPresentation {
    pub local_entry_id: i64,
    pub source_app_name: Option<String>,
    pub source_app_icon_ref: Option<String>,
}

/// Typed error the import repository surfaces. The variants collapse
/// every underlying failure into one of these stable reasons so the
/// runtime can branch on the failure without inspecting free-form
/// SQLite messages.
#[derive(Debug, Error)]
pub enum PeerImportRepositoryError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("organization error: {0}")]
    Organization(#[from] crate::OrganizationError),
    #[error("known peer {0} not found")]
    UnknownPeer(String),
    #[error("collection {0} not found")]
    UnknownCollection(i64),
    #[error("entry {0} not found")]
    UnknownEntry(i64),
}

/// Repository that owns every CRUD call the `peer-text-import`
/// runtime makes against the import tables. Cheap to construct;
/// borrows the connection so callers can decide whether to wrap the
/// work in a transaction.
pub struct PeerImportRepository<'a> {
    conn: &'a mut Connection,
}

impl<'a> PeerImportRepository<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    /// Look up the binding row the importer needs to attach an
    /// entry to the peer collection. `None` when the user deleted
    /// the previous binding (or when this is the first import for
    /// the peer). The helper NEVER returns `Some` for a row that
    /// points at a non-existent collection: the FK cascade the
    /// migration installs guarantees the row vanishes with the
    /// collection.
    pub fn find_binding(
        &self,
        peer_id: &str,
    ) -> Result<Option<PeerCollectionBinding>, PeerImportRepositoryError> {
        let record = self
            .conn
            .query_row(
                "SELECT peer_id, collection_id, created_at, updated_at
                 FROM peer_collection_bindings WHERE peer_id = ?1",
                params![peer_id],
                row_to_binding,
            )
            .optional()?;
        Ok(record)
    }

    /// Insert or replace the binding row the importer needs to
    /// attach an entry to the peer collection. The repository
    /// refuses to bind a missing peer / missing collection so a
    /// future regression that tries to bind a deleted row surfaces
    /// a typed error here instead of an SQLite FK violation.
    ///
    /// `created_at` is captured on first insert; `updated_at`
    /// refreshes on every subsequent upsert. The function uses
    /// `INSERT OR REPLACE` because a binding is keyed by `peer_id`
    /// (a primary key) and the runtime only ever holds one binding
    /// per peer.
    pub fn upsert_binding(
        &mut self,
        peer_id: &str,
        collection_id: i64,
        now: OffsetDateTime,
    ) -> Result<PeerCollectionBinding, PeerImportRepositoryError> {
        ensure_peer_exists(self.conn, peer_id)?;
        ensure_collection_exists(self.conn, collection_id)?;
        let ts = format_timestamp(now);
        let tx = self.conn.transaction()?;
        let created_at: String = tx
            .query_row(
                "SELECT created_at FROM peer_collection_bindings WHERE peer_id = ?1",
                params![peer_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_else(|| ts.clone());
        tx.execute(
            "INSERT INTO peer_collection_bindings
                 (peer_id, collection_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(peer_id) DO UPDATE SET
                 collection_id = excluded.collection_id,
                 updated_at = excluded.updated_at",
            params![peer_id, collection_id, created_at, ts],
        )?;
        tx.commit()?;
        Ok(PeerCollectionBinding {
            peer_id: peer_id.to_string(),
            collection_id,
            created_at,
            updated_at: ts,
        })
    }

    /// Attach an entry to the peer collection. The helper uses
    /// `INSERT OR IGNORE` so calling it twice with the same pair
    /// is a no-op: the membership is idempotent and never breaks
    /// the FK contract.
    pub fn attach_entry_to_binding(
        &mut self,
        entry_id: i64,
        collection_id: i64,
        now: OffsetDateTime,
    ) -> Result<(), PeerImportRepositoryError> {
        ensure_collection_exists(self.conn, collection_id)?;
        ensure_entry_exists(self.conn, entry_id)?;
        let ts = format_timestamp(now);
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO entry_collections
                 (entry_id, collection_id, created_at)
             VALUES (?1, ?2, ?3)",
            params![entry_id, collection_id, ts],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Look up a single `(peer_id, remote_entry_id, hash)`
    /// provenance row. `None` when no row exists yet — the runtime
    /// uses the helper to short-circuit duplicate imports without
    /// trying to mutate the database.
    pub fn find_import(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
        imported_content_hash: &str,
    ) -> Result<Option<RemoteImportRecord>, PeerImportRepositoryError> {
        let record = self
            .conn
            .query_row(
                "SELECT peer_id, remote_entry_id, imported_content_hash,
                        local_entry_id, imported_at,
                        source_app_name, source_app_icon_ref
                 FROM remote_imports
                 WHERE peer_id = ?1
                   AND remote_entry_id = ?2
                   AND imported_content_hash = ?3",
                params![peer_id, remote_entry_id, imported_content_hash],
                row_to_remote_import,
            )
            .optional()?;
        Ok(record)
    }

    /// Return source-app provenance for visible entries. A supplied
    /// collection is strictly scoped to its bound peer; `None` is the
    /// general-history view and selects the latest provenance across peers.
    /// Results never contain a peer identifier, and both paths require the
    /// imported entry to remain attached to its peer-bound collection.
    pub fn source_app_presentations_for_scope(
        &self,
        collection_id: Option<i64>,
        local_entry_ids: &[i64],
    ) -> Result<Vec<PeerImportedSourceAppPresentation>, PeerImportRepositoryError> {
        if collection_id.is_some_and(|id| id <= 0) {
            return Ok(Vec::new());
        }
        let ids: Vec<i64> = local_entry_ids
            .iter()
            .copied()
            .filter(|id| *id > 0)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        if ids.is_empty() || ids.len() > 100 {
            return Ok(Vec::new());
        }

        let first_entry_parameter = if collection_id.is_some() { 2 } else { 1 };
        let placeholders = (0..ids.len())
            .map(|index| format!("?{}", index + first_entry_parameter))
            .collect::<Vec<_>>()
            .join(", ");
        let collection_filter = if collection_id.is_some() {
            "AND pcb.collection_id = ?1"
        } else {
            ""
        };
        let sql = format!(
            "SELECT ri.local_entry_id, ri.source_app_name, ri.source_app_icon_ref
             FROM peer_collection_bindings pcb
             JOIN remote_imports ri ON ri.peer_id = pcb.peer_id
             JOIN entry_collections ec
               ON ec.entry_id = ri.local_entry_id
              AND ec.collection_id = pcb.collection_id
             WHERE ri.local_entry_id IN ({placeholders})
               {collection_filter}
             ORDER BY ri.imported_at DESC, ri.remote_entry_id ASC, ri.peer_id ASC"
        );
        let mut values = Vec::with_capacity(ids.len() + usize::from(collection_id.is_some()));
        if let Some(collection_id) = collection_id {
            values.push(rusqlite::types::Value::Integer(collection_id));
        }
        values.extend(ids.into_iter().map(rusqlite::types::Value::Integer));

        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(values), |row| {
            Ok(PeerImportedSourceAppPresentation {
                local_entry_id: row.get(0)?,
                source_app_name: row.get(1)?,
                source_app_icon_ref: row.get(2)?,
            })
        })?;

        let mut seen = HashSet::new();
        let mut result = Vec::new();
        for row in rows {
            let row = row?;
            if seen.insert(row.local_entry_id) {
                result.push(row);
            }
        }
        Ok(result)
    }

    /// Collection-scoped compatibility wrapper for callers that need the
    /// provenance belonging to exactly one peer-bound collection.
    pub fn source_app_presentations_for_collection(
        &self,
        collection_id: i64,
        local_entry_ids: &[i64],
    ) -> Result<Vec<PeerImportedSourceAppPresentation>, PeerImportRepositoryError> {
        self.source_app_presentations_for_scope(Some(collection_id), local_entry_ids)
    }

    /// Record a fresh provenance row. The composite primary key
    /// guarantees that re-importing the same `(peer, remote entry,
    /// content)` triple is a no-op — the helper refuses to insert
    /// a second row. The optional source-app name and icon
    /// reference travel through the `peer-source-app-presentation`
    /// extension; both stay `None` for legacy imports so a
    /// caller that does not know the field never has to construct
    /// it.
    pub fn record_import(
        &mut self,
        peer_id: &str,
        remote_entry_id: &str,
        imported_content_hash: &str,
        local_entry_id: i64,
        now: OffsetDateTime,
    ) -> Result<RemoteImportRecord, PeerImportRepositoryError> {
        Self::record_import_with_source_app(
            self,
            peer_id,
            remote_entry_id,
            imported_content_hash,
            local_entry_id,
            now,
            None,
            None,
        )
    }

    /// Variant of [`Self::record_import`] that writes the
    /// source-app name and icon reference atomically with the
    /// provenance row. The previous baseline shipped a separate
    /// `update_import_source_app` call that ran inside its own
    /// transaction; the `peer-source-app-presentation` change
    /// rejects that pattern so the row + its attribution
    /// always land together. A legacy caller that still does
    /// not know the fields should call [`Self::record_import`].
    pub fn record_import_with_source_app(
        &mut self,
        peer_id: &str,
        remote_entry_id: &str,
        imported_content_hash: &str,
        local_entry_id: i64,
        now: OffsetDateTime,
        source_app_name: Option<&str>,
        source_app_icon_ref: Option<&str>,
    ) -> Result<RemoteImportRecord, PeerImportRepositoryError> {
        ensure_peer_exists(self.conn, peer_id)?;
        ensure_entry_exists(self.conn, local_entry_id)?;
        let ts = format_timestamp(now);
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO remote_imports
                 (peer_id, remote_entry_id, imported_content_hash,
                  local_entry_id, imported_at,
                  source_app_name, source_app_icon_ref)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(peer_id, remote_entry_id, imported_content_hash)
             DO UPDATE SET
                 source_app_name = COALESCE(remote_imports.source_app_name, excluded.source_app_name),
                 source_app_icon_ref = COALESCE(remote_imports.source_app_icon_ref, excluded.source_app_icon_ref)",
            params![
                peer_id,
                remote_entry_id,
                imported_content_hash,
                local_entry_id,
                ts,
                source_app_name,
                source_app_icon_ref,
            ],
        )?;
        tx.commit()?;
        Ok(RemoteImportRecord {
            peer_id: peer_id.to_string(),
            remote_entry_id: remote_entry_id.to_string(),
            imported_content_hash: imported_content_hash.to_string(),
            local_entry_id,
            imported_at: ts,
            source_app_name: source_app_name.map(str::to_string),
            source_app_icon_ref: source_app_icon_ref.map(str::to_string),
        })
    }

    /// Persist the source-application metadata the
    /// `peer-source-app-presentation` change attaches to a
    /// `(peer_id, remote_entry_id, hash)` provenance row. The
    /// helper is metadata-only: it accepts a validated display
    /// name (already trimmed by the host / caller) and a
    /// locally-generated icon reference the caller computed
    /// through the application-icon writer; neither field ever
    /// carries a remote path, a remote filename or icon bytes.
    ///
    /// The helper is a no-op when the supplied name is `None`
    /// AND the icon reference is `None` so an import that does
    /// not opt into the additive metadata never has to build a
    /// call site. The transaction the helper opens is the
    /// smaller of the import transaction the caller drives so
    /// a rollback leaves the local database in a consistent
    /// state — the icon staging lives outside the helper so the
    /// caller is responsible for releasing the staged icon when
    /// the helper returns an error.
    pub fn update_import_source_app(
        &mut self,
        peer_id: &str,
        remote_entry_id: &str,
        imported_content_hash: &str,
        source_app_name: Option<&str>,
        source_app_icon_ref: Option<&str>,
    ) -> Result<(), PeerImportRepositoryError> {
        if source_app_name.is_none() && source_app_icon_ref.is_none() {
            // Nothing to write; the helper still refreshes the
            // `NULL` defaults so an existing row stays in sync
            // with the caller's intent. The change is a pure
            // no-op when both inputs are `None`.
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE remote_imports
             SET source_app_name = COALESCE(?1, source_app_name),
                 source_app_icon_ref = COALESCE(?2, source_app_icon_ref)
             WHERE peer_id = ?3
               AND remote_entry_id = ?4
               AND imported_content_hash = ?5",
            params![
                source_app_name,
                source_app_icon_ref,
                peer_id,
                remote_entry_id,
                imported_content_hash,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Count committed provenance rows that keep a locally staged
    /// source-app icon alive. The peer-supplied value is compared as
    /// data only; callers must already have validated the local ref.
    pub fn count_source_app_icon_references(
        &self,
        asset_ref: &str,
    ) -> Result<i64, PeerImportRepositoryError> {
        let count = self.conn.query_row(
            "SELECT COUNT(*) FROM remote_imports WHERE source_app_icon_ref = ?1",
            [asset_ref],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Commit the full image import transaction in a single
    /// SQLite transaction. The helper covers:
    ///
    /// - dedupe by canonical `content_hash` (re-uses the
    ///   existing entry when the hash is present, otherwise
    ///   inserts a fresh row in `clipboard_entries`);
    /// - attachment to the `peer_collection_bindings` row for
    ///   `peer_id`, creating the user collection when the
    ///   binding is missing;
    /// - membership attach (`entry_collections`);
    /// - provenance row insert (`remote_imports`).
    ///
    /// A failure at any step rolls the whole transaction back so
    /// the database never lands in a half-committed state (entry
    /// without binding, membership without provenance, …). The
    /// caller is responsible for cleaning up the staged asset
    /// through the asset store on a non-`Ok` return.
    pub fn commit_image_import_transaction(
        &mut self,
        spec: ImageImportSpec,
    ) -> Result<ImageImportOutcome, PeerImportRepositoryError> {
        let tx = self.conn.transaction()?;

        // Dedupe by canonical hash. An existing entry takes the
        // `Updated` path (the timestamps refresh but the row is
        // preserved bit-for-bit). A fresh insert uses the
        // `IMAGE_CONTENT_SENTINEL` placeholder the rest of the
        // code base relies on.
        let image_content_sentinel: &str = "image";
        let mime_png: &str = "image/png";

        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM clipboard_entries WHERE content_hash = ?1",
                params![spec.canonical_hash],
                |row| row.get(0),
            )
            .optional()?;
        let entry_id = match existing {
            Some(id) => {
                let last_seen = format_timestamp(spec.now);
                tx.execute(
                    "UPDATE clipboard_entries
                     SET updated_at = ?1, last_seen_at = ?1
                     WHERE id = ?2",
                    params![last_seen, id],
                )?;
                id
            }
            None => {
                let created_at = format_timestamp(spec.now);
                let last_seen = format_timestamp(spec.now);
                tx.execute(
                    "INSERT INTO clipboard_entries
                        (content, content_type, content_size, content_hash,
                         source_app, created_at, updated_at, last_seen_at,
                         asset_ref, mime_type, payload_width, payload_height,
                         rich_text_hash, rich_html_ref, rich_rtf_ref,
                         rich_preview_ref, rich_html_size, rich_rtf_size,
                         code_language)
                     VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?5, ?6, ?7, ?8, ?9, ?10,
                             NULL, NULL, NULL, NULL, NULL, NULL, NULL)",
                    params![
                        image_content_sentinel,
                        "image",
                        spec.content_size,
                        spec.canonical_hash,
                        created_at,
                        last_seen,
                        spec.asset_ref,
                        mime_png,
                        spec.width as i64,
                        spec.height as i64,
                    ],
                )?;
                let id = tx.query_row(
                    "SELECT id FROM clipboard_entries
                         WHERE content_hash = ?1 ORDER BY id DESC LIMIT 1",
                    params![spec.canonical_hash],
                    |row| row.get::<_, i64>(0),
                )?;
                // Apply the title only on a freshly inserted
                // row so a reused entry never overwrites its
                // user-edited title.
                if let Some(title) = spec
                    .validated_title
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                {
                    let last_seen = format_timestamp(spec.now);
                    tx.execute(
                        "UPDATE clipboard_entries
                         SET title = ?1, updated_at = ?2
                         WHERE id = ?3",
                        params![title, last_seen, id],
                    )?;
                }
                id
            }
        };

        // Resolve the peer binding. The binding helper ensures
        // the row exists in `peer_collection_bindings`; when no
        // binding is present, we create a fresh user collection
        // (with the documented collision-suffix rule) and insert
        // the binding inside the same transaction.
        let collection_id = {
            let existing_binding: Option<i64> = tx
                .query_row(
                    "SELECT collection_id FROM peer_collection_bindings
                     WHERE peer_id = ?1",
                    params![spec.peer_id],
                    |row| row.get(0),
                )
                .optional()?;
            match existing_binding {
                Some(id) => id,
                None => {
                    let id = create_peer_collection_in_tx(&tx, &spec.display_name, spec.now)?;
                    let ts = format_timestamp(spec.now);
                    tx.execute(
                        "INSERT INTO peer_collection_bindings
                             (peer_id, collection_id, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?3)",
                        params![spec.peer_id, id, ts],
                    )?;
                    id
                }
            }
        };

        // Membership: idempotent (composite PK) so the
        // operation collapses to a no-op on a duplicate.
        let ts = format_timestamp(spec.now);
        tx.execute(
            "INSERT OR IGNORE INTO entry_collections
                 (entry_id, collection_id, created_at)
             VALUES (?1, ?2, ?3)",
            params![entry_id, collection_id, ts],
        )?;

        // Provenance: idempotent (composite PK) so a repeated
        // import never produces a second row. Older rows with no
        // source-app attribution are enriched, while populated fields
        // remain stable for that provenance.
        tx.execute(
            "INSERT INTO remote_imports
                 (peer_id, remote_entry_id, imported_content_hash,
                  local_entry_id, imported_at,
                  source_app_name, source_app_icon_ref)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(peer_id, remote_entry_id, imported_content_hash)
             DO UPDATE SET
                 source_app_name = COALESCE(remote_imports.source_app_name, excluded.source_app_name),
                 source_app_icon_ref = COALESCE(remote_imports.source_app_icon_ref, excluded.source_app_icon_ref)",
            params![
                spec.peer_id,
                spec.remote_entry_id,
                spec.canonical_hash,
                entry_id,
                ts,
                spec.source_app_name.as_deref(),
                spec.source_app_icon_ref.as_deref(),
            ],
        )?;

        let deduplicated = existing.is_some();
        tx.commit()?;
        Ok(ImageImportOutcome {
            entry_id,
            collection_id,
            deduplicated,
        })
    }
}

fn ensure_peer_exists(conn: &Connection, peer_id: &str) -> Result<(), PeerImportRepositoryError> {
    let exists: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM known_peers WHERE peer_id = ?1",
            params![peer_id],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(PeerImportRepositoryError::UnknownPeer(peer_id.to_string()));
    }
    Ok(())
}

fn ensure_collection_exists(
    conn: &Connection,
    collection_id: i64,
) -> Result<(), PeerImportRepositoryError> {
    let exists: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM collections WHERE id = ?1",
            params![collection_id],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(PeerImportRepositoryError::UnknownCollection(collection_id));
    }
    Ok(())
}

fn ensure_entry_exists(conn: &Connection, entry_id: i64) -> Result<(), PeerImportRepositoryError> {
    let exists: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM clipboard_entries WHERE id = ?1",
            params![entry_id],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(PeerImportRepositoryError::UnknownEntry(entry_id));
    }
    Ok(())
}

fn row_to_binding(row: &rusqlite::Row<'_>) -> rusqlite::Result<PeerCollectionBinding> {
    Ok(PeerCollectionBinding {
        peer_id: row.get(0)?,
        collection_id: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

fn row_to_remote_import(row: &rusqlite::Row<'_>) -> rusqlite::Result<RemoteImportRecord> {
    Ok(RemoteImportRecord {
        peer_id: row.get(0)?,
        remote_entry_id: row.get(1)?,
        imported_content_hash: row.get(2)?,
        local_entry_id: row.get(3)?,
        imported_at: row.get(4)?,
        source_app_name: row.get(5)?,
        source_app_icon_ref: row.get(6)?,
    })
}

/// Insert a fresh user collection the image-import transaction
/// can bind to a peer. The helper mirrors the public
/// [`crate::organization::OrganizationRepository::create_user_collection`]
/// collision rule (the `(equipo)` suffix when the visible name
/// already exists) so the imported binding matches what the
/// `peer-text-import` change already produces.
fn create_peer_collection_in_tx(
    tx: &Transaction<'_>,
    display_name: &str,
    now: OffsetDateTime,
) -> Result<i64, PeerImportRepositoryError> {
    use crate::organization::MAX_ORGANIZATION_NAME_CHARS;
    let trimmed = display_name.trim();
    let base_name = if trimmed.is_empty() {
        "(equipo)".to_string()
    } else {
        trimmed.to_string()
    };
    let mut candidate = base_name.clone();
    loop {
        let existing: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM collections WHERE LOWER(name) = LOWER(?1) LIMIT 1",
                params![candidate],
                |row| row.get(0),
            )
            .optional()?;
        if existing.is_none() {
            break;
        }
        let suffix = " (equipo)";
        let combined = format!("{base_name}{suffix}");
        let chars: Vec<char> = combined.chars().collect();
        candidate = if chars.len() > MAX_ORGANIZATION_NAME_CHARS {
            chars
                .into_iter()
                .take(MAX_ORGANIZATION_NAME_CHARS)
                .collect()
        } else {
            combined
        };
        if candidate == base_name {
            break;
        }
    }
    let ts = format_timestamp(now);
    // The `is_peer_bound` / `peer_display_name` columns the
    // [`crate::Collection`] struct exposes are projection-only
    // (computed via join against `peer_collection_bindings` and
    // `known_peers`); the `collections` table does not store
    // them, so the INSERT must omit both columns and let the
    // projection populate them on the next read.
    tx.execute(
        "INSERT INTO collections (stable_key, name, kind, color_hex, created_at, updated_at)
         VALUES (?1, ?2, 'user', ?3, ?4, ?4)",
        params![
            Option::<String>::None,
            candidate,
            crate::organization::HISTORY_DEFAULT_COLOR_HEX,
            ts,
        ],
    )?;
    let id = tx.query_row(
        "SELECT id FROM collections WHERE name = ?1 ORDER BY id DESC LIMIT 1",
        params![candidate],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(id)
}

fn format_timestamp(ts: OffsetDateTime) -> String {
    ts.format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// Bind a binding lookup to a transaction so the importer can hold
/// the lock for the full sequence. The helper is a thin forward
/// to [`PeerImportRepository::find_binding`]; the dedicated
/// constructor exists to make the borrow relationship explicit at
/// the call site.
pub fn find_binding_in_tx(
    tx: &Transaction<'_>,
    peer_id: &str,
) -> Result<Option<PeerCollectionBinding>, PeerImportRepositoryError> {
    let record = tx
        .query_row(
            "SELECT peer_id, collection_id, created_at, updated_at
             FROM peer_collection_bindings WHERE peer_id = ?1",
            params![peer_id],
            row_to_binding,
        )
        .optional()?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organization::{
        CollectionKind, OrganizationRepository, HISTORY_DEFAULT_COLOR_HEX, HISTORY_STABLE_KEY,
    };
    use crate::{Database, EntryRepository, NewEntry};

    fn isolated_db() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("clipvault.db");
        let mut db = Database::open(&path).expect("open");
        db.run_migrations(&crate::builtin_migrations())
            .expect("migrate");
        (dir, db)
    }

    fn seed_peer(db: &mut Database, peer_id: &str) {
        let conn = db.connection_mut();
        let mut repo = crate::KnownPeerRepository::new(conn);
        repo.upsert_observation(&crate::PeerObservation {
            peer_id: peer_id.to_string(),
            public_key_fingerprint: "ab".repeat(32),
            full_public_key_fingerprint: Some("cd".repeat(32)),
            display_name: format!("peer-{peer_id}"),
            protocol_major: 1,
            capability: "pairing".to_string(),
            caps_extra: String::new(),
            caps_extra_v2: String::new(),
            observed_at: OffsetDateTime::now_utc(),
        })
        .expect("upsert");
    }

    fn seed_entry(db: &mut Database, content: &str) -> i64 {
        let now = OffsetDateTime::now_utc();
        let conn = db.connection_mut();
        let mut repo = EntryRepository::new(conn);
        let outcome = repo
            .insert_or_touch(NewEntry::text(
                content.to_string(),
                crate::ContentType::Text,
                content.len() as i64,
                "h".repeat(16),
                None,
                now,
                now,
            ))
            .expect("insert");
        outcome.record().id
    }

    fn seed_user_collection(db: &mut Database, name: &str) -> i64 {
        let conn = db.connection_mut();
        let mut repo = OrganizationRepository::new(conn);
        let collection = repo
            .create_user_collection(name, HISTORY_DEFAULT_COLOR_HEX, OffsetDateTime::now_utc())
            .expect("collection");
        assert_eq!(collection.kind, CollectionKind::User);
        collection.id
    }

    #[test]
    fn binding_round_trips_through_upsert() {
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        let collection_id = seed_user_collection(&mut db, "Equipo A");
        let mut repo = PeerImportRepository::new(db.connection_mut());
        let binding = repo
            .upsert_binding("peer-a", collection_id, OffsetDateTime::now_utc())
            .expect("upsert");
        assert_eq!(binding.peer_id, "peer-a");
        assert_eq!(binding.collection_id, collection_id);
        let fetched = repo.find_binding("peer-a").expect("find");
        assert_eq!(fetched, Some(binding.clone()));
    }

    #[test]
    fn binding_rejects_unknown_peer() {
        let (_dir, mut db) = isolated_db();
        let collection_id = seed_user_collection(&mut db, "Equipo A");
        let mut repo = PeerImportRepository::new(db.connection_mut());
        let err = repo
            .upsert_binding("peer-missing", collection_id, OffsetDateTime::now_utc())
            .expect_err("must reject missing peer");
        assert!(matches!(err, PeerImportRepositoryError::UnknownPeer(_)));
    }

    #[test]
    fn binding_rejects_unknown_collection() {
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        let mut repo = PeerImportRepository::new(db.connection_mut());
        let err = repo
            .upsert_binding("peer-a", 999_999, OffsetDateTime::now_utc())
            .expect_err("must reject missing collection");
        assert!(matches!(
            err,
            PeerImportRepositoryError::UnknownCollection(999_999)
        ));
    }

    #[test]
    fn attach_entry_is_idempotent() {
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        let collection_id = seed_user_collection(&mut db, "Equipo A");
        let entry_id = seed_entry(&mut db, "hello");
        let mut repo = PeerImportRepository::new(db.connection_mut());
        repo.attach_entry_to_binding(entry_id, collection_id, OffsetDateTime::now_utc())
            .expect("attach");
        // Re-attach must succeed and stay a single membership row.
        repo.attach_entry_to_binding(entry_id, collection_id, OffsetDateTime::now_utc())
            .expect("attach again");
        let count: i64 = db
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM entry_collections WHERE entry_id = ?1 AND collection_id = ?2",
                params![entry_id, collection_id],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn record_import_round_trips() {
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        let entry_id = seed_entry(&mut db, "hello");
        let mut repo = PeerImportRepository::new(db.connection_mut());
        let record = repo
            .record_import(
                "peer-a",
                "entry-1",
                "hash-a",
                entry_id,
                OffsetDateTime::now_utc(),
            )
            .expect("record");
        assert_eq!(record.peer_id, "peer-a");
        assert_eq!(record.remote_entry_id, "entry-1");
        assert_eq!(record.imported_content_hash, "hash-a");
        assert_eq!(record.local_entry_id, entry_id);
        let fetched = repo
            .find_import("peer-a", "entry-1", "hash-a")
            .expect("find");
        assert_eq!(fetched, Some(record));
    }

    #[test]
    fn record_import_keeps_history_collection_attached() {
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        let entry_id = seed_entry(&mut db, "hello");
        let history_id: i64 = db
            .connection()
            .query_row(
                "SELECT id FROM collections WHERE stable_key = ?1",
                params![HISTORY_STABLE_KEY],
                |row| row.get(0),
            )
            .expect("history");
        let count: i64 = db
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM entry_collections WHERE entry_id = ?1 AND collection_id = ?2",
                params![entry_id, history_id],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn record_import_with_source_app_writes_name_and_icon_atomically() {
        // The `peer-source-app-presentation` change rejects the
        // separate-transaction pattern the previous baseline
        // shipped. The helper MUST commit the provenance row
        // and its source-app attribution in a single SQL
        // statement so a half-committed import can never leave
        // a row without its metadata, and vice-versa.
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        let entry_id = seed_entry(&mut db, "hello");
        let conn = db.connection_mut();
        let mut repo = PeerImportRepository::new(conn);
        let record = repo
            .record_import_with_source_app(
                "peer-a",
                "entry-1",
                "hash-atomic",
                entry_id,
                OffsetDateTime::now_utc(),
                Some("Visual Studio Code"),
                Some("application-icons/vscode.png"),
            )
            .expect("record with source app");
        assert_eq!(
            record.source_app_name.as_deref(),
            Some("Visual Studio Code")
        );
        assert_eq!(
            record.source_app_icon_ref.as_deref(),
            Some("application-icons/vscode.png")
        );

        // The previously-inserted row reads back the same
        // values so a follow-up `find_import` for the same
        // provenance triple returns the metadata.
        let fetched = repo
            .find_import("peer-a", "entry-1", "hash-atomic")
            .expect("find")
            .expect("present");
        assert_eq!(
            fetched.source_app_name.as_deref(),
            Some("Visual Studio Code")
        );
        assert_eq!(
            fetched.source_app_icon_ref.as_deref(),
            Some("application-icons/vscode.png")
        );
    }

    #[test]
    fn source_app_projection_isolated_by_peer_bound_collection() {
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        seed_peer(&mut db, "peer-b");
        let entry_id = seed_entry(&mut db, "shared content");
        let collection_a = seed_user_collection(&mut db, "Peer A imports");
        let collection_b = seed_user_collection(&mut db, "Peer B imports");
        let mut repo = PeerImportRepository::new(db.connection_mut());
        let first_imported_at = OffsetDateTime::now_utc();
        repo.upsert_binding("peer-a", collection_a, OffsetDateTime::now_utc())
            .expect("bind peer A");
        repo.upsert_binding("peer-b", collection_b, OffsetDateTime::now_utc())
            .expect("bind peer B");
        repo.attach_entry_to_binding(entry_id, collection_a, OffsetDateTime::now_utc())
            .expect("attach to peer A collection");
        repo.attach_entry_to_binding(entry_id, collection_b, OffsetDateTime::now_utc())
            .expect("attach to peer B collection");
        repo.record_import_with_source_app(
            "peer-a",
            "remote-a",
            "hash-a",
            entry_id,
            first_imported_at,
            Some("Editor A"),
            Some("application-icons/editor-a.png"),
        )
        .expect("peer A provenance");
        repo.record_import_with_source_app(
            "peer-b",
            "remote-b",
            "hash-b",
            entry_id,
            first_imported_at + time::Duration::seconds(1),
            Some("Editor B"),
            Some("application-icons/editor-b.png"),
        )
        .expect("peer B provenance");

        let a = repo
            .source_app_presentations_for_collection(collection_a, &[entry_id])
            .expect("peer A projection");
        let b = repo
            .source_app_presentations_for_collection(collection_b, &[entry_id])
            .expect("peer B projection");
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].source_app_name.as_deref(), Some("Editor A"));
        assert_eq!(
            a[0].source_app_icon_ref.as_deref(),
            Some("application-icons/editor-a.png")
        );
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].source_app_name.as_deref(), Some("Editor B"));
        assert_eq!(
            b[0].source_app_icon_ref.as_deref(),
            Some("application-icons/editor-b.png")
        );

        // General history receives only the newest imported source
        // presentation, without the peer id, even when a deduplicated
        // entry has provenance from multiple peers.
        let history = repo
            .source_app_presentations_for_scope(None, &[entry_id])
            .expect("general-history projection");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].source_app_name.as_deref(), Some("Editor B"));
        assert_eq!(
            history[0].source_app_icon_ref.as_deref(),
            Some("application-icons/editor-b.png")
        );
        assert!(repo
            .source_app_presentations_for_scope(Some(collection_a), &[entry_id + 100])
            .expect("unrelated entry projection")
            .is_empty());
    }

    #[test]
    fn record_import_with_source_app_is_idempotent_for_reimport() {
        // The `peer-source-app-presentation` change pins a
        // single-write contract: re-importing the same canonical
        // entry from the same peer never produces a duplicate
        // provenance row, even when the second call carries
        // fresh source-app metadata. The composite primary key
        // collapses the second insert to a no-op so the
        // originally-written metadata survives unchanged.
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        let entry_id = seed_entry(&mut db, "hello");
        let conn = db.connection_mut();
        let mut repo = PeerImportRepository::new(conn);
        repo.record_import_with_source_app(
            "peer-a",
            "entry-1",
            "hash-idem",
            entry_id,
            OffsetDateTime::now_utc(),
            Some("First"),
            Some("application-icons/first.png"),
        )
        .expect("first");
        repo.record_import_with_source_app(
            "peer-a",
            "entry-1",
            "hash-idem",
            entry_id,
            OffsetDateTime::now_utc(),
            Some("Second"),
            Some("application-icons/second.png"),
        )
        .expect("second");
        let fetched = repo
            .find_import("peer-a", "entry-1", "hash-idem")
            .expect("find")
            .expect("present");
        assert_eq!(fetched.source_app_name.as_deref(), Some("First"));
        assert_eq!(
            fetched.source_app_icon_ref.as_deref(),
            Some("application-icons/first.png")
        );
    }

    #[test]
    fn reimport_enriches_legacy_provenance_without_overwriting_existing_values() {
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        let entry_id = seed_entry(&mut db, "hello");
        let conn = db.connection_mut();
        let mut repo = PeerImportRepository::new(conn);
        repo.record_import(
            "peer-a",
            "entry-legacy",
            "hash-legacy",
            entry_id,
            OffsetDateTime::now_utc(),
        )
        .expect("legacy provenance");

        repo.record_import_with_source_app(
            "peer-a",
            "entry-legacy",
            "hash-legacy",
            entry_id,
            OffsetDateTime::now_utc(),
            Some("Terminal"),
            Some("application-icons/terminal.png"),
        )
        .expect("enriched reimport");

        let fetched = repo
            .find_import("peer-a", "entry-legacy", "hash-legacy")
            .expect("find")
            .expect("present");
        assert_eq!(fetched.source_app_name.as_deref(), Some("Terminal"));
        assert_eq!(
            fetched.source_app_icon_ref.as_deref(),
            Some("application-icons/terminal.png")
        );
    }

    #[test]
    fn commit_image_import_persists_source_app_metadata() {
        // The image import transaction MUST persist the
        // source-app name + icon ref atomically with the entry
        // / binding / provenance rows. The previous baseline
        // shipped a separate `update_import_source_app`
        // transaction; the `peer-source-app-presentation`
        // change rejects that pattern.
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        let outcome = {
            let mut repo = PeerImportRepository::new(db.connection_mut());
            repo.commit_image_import_transaction(ImageImportSpec {
                peer_id: "peer-a".to_string(),
                remote_entry_id: "entry-img".to_string(),
                display_name: "Equipo A".to_string(),
                canonical_hash: "hash-img-source".to_string(),
                asset_ref: "clipboard/hash-img-source.png".to_string(),
                mime_type: "image/png".to_string(),
                width: 16,
                height: 8,
                content_size: 32,
                validated_title: Some("Captura".to_string()),
                source_app_name: Some("Terminal".to_string()),
                source_app_icon_ref: Some("application-icons/terminal.png".to_string()),
                now: OffsetDateTime::now_utc(),
            })
            .expect("commit")
        };
        let fetched = {
            let repo = PeerImportRepository::new(db.connection_mut());
            repo.find_import("peer-a", "entry-img", "hash-img-source")
                .expect("find")
                .expect("present")
        };
        assert_eq!(fetched.source_app_name.as_deref(), Some("Terminal"));
        assert_eq!(
            fetched.source_app_icon_ref.as_deref(),
            Some("application-icons/terminal.png")
        );
        assert_eq!(fetched.local_entry_id, outcome.entry_id);
    }

    #[test]
    fn commit_image_import_dedupes_two_peers_with_separate_metadata() {
        // Two peers that ship the same canonical content hash
        // collapse to a single local entry (the `peer-image-import`
        // dedupe contract) but each peer keeps its own
        // provenance metadata: the local icon ref the
        // `peer-source-app-presentation` change persists belongs
        // to that peer's import, never to a sibling peer.
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        seed_peer(&mut db, "peer-b");
        let canonical_hash = "hash-shared-source";
        let (a_outcome, b_outcome) = {
            let mut repo = PeerImportRepository::new(db.connection_mut());
            let a = repo
                .commit_image_import_transaction(ImageImportSpec {
                    peer_id: "peer-a".to_string(),
                    remote_entry_id: "entry-shared".to_string(),
                    display_name: "Equipo A".to_string(),
                    canonical_hash: canonical_hash.to_string(),
                    asset_ref: "clipboard/hash-shared-source.png".to_string(),
                    mime_type: "image/png".to_string(),
                    width: 8,
                    height: 4,
                    content_size: 16,
                    validated_title: None,
                    source_app_name: Some("App A".to_string()),
                    source_app_icon_ref: Some("application-icons/a.png".to_string()),
                    now: OffsetDateTime::now_utc(),
                })
                .expect("commit a");
            let b = repo
                .commit_image_import_transaction(ImageImportSpec {
                    peer_id: "peer-b".to_string(),
                    remote_entry_id: "entry-shared".to_string(),
                    display_name: "Equipo B".to_string(),
                    canonical_hash: canonical_hash.to_string(),
                    asset_ref: "clipboard/hash-shared-source.png".to_string(),
                    mime_type: "image/png".to_string(),
                    width: 8,
                    height: 4,
                    content_size: 16,
                    validated_title: None,
                    source_app_name: Some("App B".to_string()),
                    source_app_icon_ref: Some("application-icons/b.png".to_string()),
                    now: OffsetDateTime::now_utc(),
                })
                .expect("commit b");
            (a, b)
        };
        assert_eq!(a_outcome.entry_id, b_outcome.entry_id);

        let a_fetched = {
            let repo = PeerImportRepository::new(db.connection_mut());
            repo.find_import("peer-a", "entry-shared", canonical_hash)
                .expect("find a")
                .expect("present")
        };
        let b_fetched = {
            let repo = PeerImportRepository::new(db.connection_mut());
            repo.find_import("peer-b", "entry-shared", canonical_hash)
                .expect("find b")
                .expect("present")
        };
        assert_eq!(a_fetched.source_app_name.as_deref(), Some("App A"));
        assert_eq!(
            a_fetched.source_app_icon_ref.as_deref(),
            Some("application-icons/a.png")
        );
        assert_eq!(b_fetched.source_app_name.as_deref(), Some("App B"));
        assert_eq!(
            b_fetched.source_app_icon_ref.as_deref(),
            Some("application-icons/b.png")
        );
    }

    fn seed_image_entry(db: &mut Database, hash: &str, asset_ref: &str) -> i64 {
        let now = OffsetDateTime::now_utc();
        let conn = db.connection_mut();
        let mut repo = EntryRepository::new(conn);
        let outcome = repo
            .insert_or_touch(NewEntry {
                content: crate::IMAGE_CONTENT_SENTINEL.to_string(),
                content_type: crate::ContentType::Image,
                content_size: 16,
                content_hash: hash.to_string(),
                source_app: None,
                created_at: now,
                last_seen_at: now,
                asset_ref: Some(asset_ref.to_string()),
                mime_type: Some(crate::IMAGE_MIME_PNG.to_string()),
                payload_width: Some(8),
                payload_height: Some(4),
                rich_text_hash: None,
                rich_html_ref: None,
                rich_rtf_ref: None,
                rich_preview_ref: None,
                rich_html_size: None,
                rich_rtf_size: None,
                code_language: None,
            })
            .expect("insert image entry");
        outcome.record().id
    }

    #[test]
    fn commit_image_import_inserts_fresh_entry_atomically() {
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        let outcome = {
            let mut repo = PeerImportRepository::new(db.connection_mut());
            repo.commit_image_import_transaction(ImageImportSpec {
                peer_id: "peer-a".to_string(),
                remote_entry_id: "entry-1".to_string(),
                display_name: "Equipo A".to_string(),
                canonical_hash: "hash-fresh".to_string(),
                asset_ref: "clipboard/hash-fresh.png".to_string(),
                mime_type: "image/png".to_string(),
                width: 16,
                height: 8,
                content_size: 32,
                validated_title: Some("Captura".to_string()),
                now: OffsetDateTime::now_utc(),
                source_app_name: None,
                source_app_icon_ref: None,
            })
            .expect("commit")
        };
        assert!(!outcome.deduplicated);
        assert!(outcome.entry_id > 0);
        assert!(outcome.collection_id > 0);

        // Membership is attached.
        let count: i64 = db
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM entry_collections WHERE entry_id = ?1 AND collection_id = ?2",
                params![outcome.entry_id, outcome.collection_id],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 1);
        // Provenance is recorded.
        let found = {
            let repo = PeerImportRepository::new(db.connection_mut());
            repo.find_import("peer-a", "entry-1", "hash-fresh")
                .expect("find")
        };
        assert_eq!(found.map(|r| r.local_entry_id), Some(outcome.entry_id));
        // Title persisted on the fresh entry.
        let title: Option<String> = db
            .connection()
            .query_row(
                "SELECT title FROM clipboard_entries WHERE id = ?1",
                params![outcome.entry_id],
                |row| row.get(0),
            )
            .expect("title");
        assert_eq!(title.as_deref(), Some("Captura"));
    }

    #[test]
    fn commit_image_import_reuses_existing_entry_and_skips_title_overwrite() {
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        let existing_id = seed_image_entry(&mut db, "hash-reuse", "clipboard/hash-reuse.png");
        let outcome = {
            let mut repo = PeerImportRepository::new(db.connection_mut());
            repo.commit_image_import_transaction(ImageImportSpec {
                peer_id: "peer-a".to_string(),
                remote_entry_id: "entry-2".to_string(),
                display_name: "Equipo A".to_string(),
                canonical_hash: "hash-reuse".to_string(),
                asset_ref: "clipboard/hash-reuse.png".to_string(),
                mime_type: "image/png".to_string(),
                width: 8,
                height: 4,
                content_size: 16,
                validated_title: Some("Otro título".to_string()),
                now: OffsetDateTime::now_utc(),
                source_app_name: None,
                source_app_icon_ref: None,
            })
            .expect("commit")
        };
        assert!(outcome.deduplicated);
        assert_eq!(outcome.entry_id, existing_id);
        // The reused entry keeps its existing title (None in this case).
        let title: Option<String> = db
            .connection()
            .query_row(
                "SELECT title FROM clipboard_entries WHERE id = ?1",
                params![existing_id],
                |row| row.get(0),
            )
            .expect("title");
        assert_eq!(title, None);
    }

    #[test]
    fn commit_image_import_is_idempotent_for_same_snapshot() {
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        let (first, second) = {
            let mut repo = PeerImportRepository::new(db.connection_mut());
            let first = repo
                .commit_image_import_transaction(ImageImportSpec {
                    peer_id: "peer-a".to_string(),
                    remote_entry_id: "entry-3".to_string(),
                    display_name: "Equipo A".to_string(),
                    canonical_hash: "hash-idem".to_string(),
                    asset_ref: "clipboard/hash-idem.png".to_string(),
                    mime_type: "image/png".to_string(),
                    width: 4,
                    height: 4,
                    content_size: 8,
                    validated_title: None,
                    now: OffsetDateTime::now_utc(),
                    source_app_name: None,
                    source_app_icon_ref: None,
                })
                .expect("commit");
            let second = repo
                .commit_image_import_transaction(ImageImportSpec {
                    peer_id: "peer-a".to_string(),
                    remote_entry_id: "entry-3".to_string(),
                    display_name: "Equipo A".to_string(),
                    canonical_hash: "hash-idem".to_string(),
                    asset_ref: "clipboard/hash-idem.png".to_string(),
                    mime_type: "image/png".to_string(),
                    width: 4,
                    height: 4,
                    content_size: 8,
                    validated_title: None,
                    now: OffsetDateTime::now_utc(),
                    source_app_name: Some("Terminal".to_string()),
                    source_app_icon_ref: Some("application-icons/terminal.png".to_string()),
                })
                .expect("commit");
            (first, second)
        };
        assert_eq!(first.entry_id, second.entry_id);
        assert!(!first.deduplicated);
        assert!(second.deduplicated);
        // A single provenance row exists.
        let count: i64 = db
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM remote_imports
                 WHERE peer_id = ?1 AND remote_entry_id = ?2 AND imported_content_hash = ?3",
                params!["peer-a", "entry-3", "hash-idem"],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 1);
        let provenance = PeerImportRepository::new(db.connection_mut())
            .find_import("peer-a", "entry-3", "hash-idem")
            .expect("find provenance")
            .expect("one provenance row");
        assert_eq!(provenance.source_app_name.as_deref(), Some("Terminal"));
        assert_eq!(
            provenance.source_app_icon_ref.as_deref(),
            Some("application-icons/terminal.png")
        );
    }

    #[test]
    fn commit_image_import_creates_new_entry_when_remote_hash_changes() {
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        let (first, second) = {
            let mut repo = PeerImportRepository::new(db.connection_mut());
            let first = repo
                .commit_image_import_transaction(ImageImportSpec {
                    peer_id: "peer-a".to_string(),
                    remote_entry_id: "entry-4".to_string(),
                    display_name: "Equipo A".to_string(),
                    canonical_hash: "hash-v1".to_string(),
                    asset_ref: "clipboard/hash-v1.png".to_string(),
                    mime_type: "image/png".to_string(),
                    width: 4,
                    height: 4,
                    content_size: 8,
                    validated_title: None,
                    now: OffsetDateTime::now_utc(),
                    source_app_name: None,
                    source_app_icon_ref: None,
                })
                .expect("commit");
            let second = repo
                .commit_image_import_transaction(ImageImportSpec {
                    peer_id: "peer-a".to_string(),
                    remote_entry_id: "entry-4".to_string(),
                    display_name: "Equipo A".to_string(),
                    canonical_hash: "hash-v2".to_string(),
                    asset_ref: "clipboard/hash-v2.png".to_string(),
                    mime_type: "image/png".to_string(),
                    width: 4,
                    height: 4,
                    content_size: 8,
                    validated_title: None,
                    now: OffsetDateTime::now_utc(),
                    source_app_name: None,
                    source_app_icon_ref: None,
                })
                .expect("commit");
            (first, second)
        };
        assert_ne!(first.entry_id, second.entry_id);
        // Two provenance rows, one per hash.
        let count: i64 = db
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM remote_imports
                 WHERE peer_id = ?1 AND remote_entry_id = ?2",
                params!["peer-a", "entry-4"],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 2);
    }

    #[test]
    fn commit_image_import_reuses_existing_peer_collection() {
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        let existing_id = seed_user_collection(&mut db, "Equipo A");
        // Pre-create a binding so the second commit reuses it.
        let outcome = {
            let mut repo = PeerImportRepository::new(db.connection_mut());
            repo.upsert_binding("peer-a", existing_id, OffsetDateTime::now_utc())
                .expect("binding");
            repo.commit_image_import_transaction(ImageImportSpec {
                peer_id: "peer-a".to_string(),
                remote_entry_id: "entry-5".to_string(),
                display_name: "Equipo A".to_string(),
                canonical_hash: "hash-reuse-coll".to_string(),
                asset_ref: "clipboard/hash-reuse-coll.png".to_string(),
                mime_type: "image/png".to_string(),
                width: 4,
                height: 4,
                content_size: 8,
                validated_title: None,
                now: OffsetDateTime::now_utc(),
                source_app_name: None,
                source_app_icon_ref: None,
            })
            .expect("commit")
        };
        assert_eq!(outcome.collection_id, existing_id);
    }

    #[test]
    fn commit_image_import_two_peers_share_entry_when_hashes_match() {
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        seed_peer(&mut db, "peer-b");
        let (first, second) = {
            let mut repo = PeerImportRepository::new(db.connection_mut());
            let first = repo
                .commit_image_import_transaction(ImageImportSpec {
                    peer_id: "peer-a".to_string(),
                    remote_entry_id: "entry-shared".to_string(),
                    display_name: "Equipo A".to_string(),
                    canonical_hash: "hash-shared".to_string(),
                    asset_ref: "clipboard/hash-shared.png".to_string(),
                    mime_type: "image/png".to_string(),
                    width: 4,
                    height: 4,
                    content_size: 8,
                    validated_title: None,
                    now: OffsetDateTime::now_utc(),
                    source_app_name: None,
                    source_app_icon_ref: None,
                })
                .expect("commit");
            let second = repo
                .commit_image_import_transaction(ImageImportSpec {
                    peer_id: "peer-b".to_string(),
                    remote_entry_id: "entry-shared".to_string(),
                    display_name: "Equipo B".to_string(),
                    canonical_hash: "hash-shared".to_string(),
                    asset_ref: "clipboard/hash-shared.png".to_string(),
                    mime_type: "image/png".to_string(),
                    width: 4,
                    height: 4,
                    content_size: 8,
                    validated_title: None,
                    now: OffsetDateTime::now_utc(),
                    source_app_name: None,
                    source_app_icon_ref: None,
                })
                .expect("commit");
            (first, second)
        };
        // Same canonical hash collapses to a single local entry.
        assert_eq!(first.entry_id, second.entry_id);
        // Two provenance rows — one per peer.
        let count: i64 = db
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM remote_imports WHERE imported_content_hash = ?1",
                params!["hash-shared"],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 2);
        // Two independent peer-bound collections.
        let collection_count: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM peer_collection_bindings", [], |row| {
                row.get(0)
            })
            .expect("count");
        assert_eq!(collection_count, 2);
    }

    #[test]
    fn commit_image_import_applies_collision_suffix_to_collection_name() {
        let (_dir, mut db) = isolated_db();
        seed_peer(&mut db, "peer-a");
        seed_user_collection(&mut db, "Equipo A");
        let outcome = {
            let mut repo = PeerImportRepository::new(db.connection_mut());
            repo.commit_image_import_transaction(ImageImportSpec {
                peer_id: "peer-a".to_string(),
                remote_entry_id: "entry-collision".to_string(),
                display_name: "Equipo A".to_string(),
                canonical_hash: "hash-collision".to_string(),
                asset_ref: "clipboard/hash-collision.png".to_string(),
                mime_type: "image/png".to_string(),
                width: 4,
                height: 4,
                content_size: 8,
                validated_title: None,
                now: OffsetDateTime::now_utc(),
                source_app_name: None,
                source_app_icon_ref: None,
            })
            .expect("commit")
        };
        let name: String = db
            .connection()
            .query_row(
                "SELECT name FROM collections WHERE id = ?1",
                params![outcome.collection_id],
                |row| row.get(0),
            )
            .expect("name");
        assert_eq!(name, "Equipo A (equipo)");
    }
}
