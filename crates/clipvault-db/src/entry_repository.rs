//! Repository for the `clipboard_entries` table.
//!
//! Exposes the operations the core needs without leaking the SQL layout
//! to the rest of the application. Reads run against a shared
//! `&Connection`; writes expect a `&mut Connection` so the caller can
//! own the lock returned by the surrounding [`crate::Database`].

use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeSet;
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::entry::{ContentType, EntryRecord, NewEntry};
use crate::organization::OrganizationError;

/// Column list shared by every `SELECT` that materialises an
/// [`EntryRecord`]. Kept in one place so a future column addition
/// cannot be applied to some queries and forgotten in others — the
/// exact bug class that would make an image row load without its
/// `asset_ref` and render as a broken card.
const ENTRY_COLUMNS: &str = "id, content, content_type, content_size, content_hash,
     source_app, is_pinned, created_at, updated_at, last_seen_at,
     title, source_app_name, source_app_icon_ref,
     asset_ref, mime_type, payload_width, payload_height,
     rich_text_hash, rich_html_ref, rich_rtf_ref, rich_preview_ref,
     rich_html_size, rich_rtf_size";

#[derive(Debug, Error)]
pub enum EntryRepositoryError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("organization error: {0}")]
    Organization(#[from] OrganizationError),
}

/// Result of [`EntryRepository::insert_or_touch`]. Tells the service
/// whether a new row was created or an existing one was updated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryOutcome {
    Inserted(EntryRecord),
    Updated(EntryRecord),
}

impl EntryOutcome {
    pub fn record(&self) -> &EntryRecord {
        match self {
            EntryOutcome::Inserted(record) | EntryOutcome::Updated(record) => record,
        }
    }
}

/// Result of [`EntryRepository::set_favorite`]. `updated` is `None`
/// when the target entry does not exist; otherwise it carries the
/// refreshed record so the caller can serialise a summary without an
/// extra `find_by_id` round-trip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetFavoriteOutcome {
    pub updated: Option<EntryRecord>,
}

/// Result of [`EntryRepository::set_title`]. `updated` is `None`
/// when the target entry does not exist; otherwise it carries the
/// refreshed record. The repository never rejects the call when the
/// caller passes an invalid title — that is the service layer's
/// responsibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetTitleOutcome {
    pub updated: Option<EntryRecord>,
}

/// Result of [`EntryRepository::set_source_app_metadata`]. `updated`
/// is `None` when the target entry does not exist; otherwise it
/// carries the refreshed record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetSourceAppMetadataOutcome {
    pub updated: Option<EntryRecord>,
}

pub struct EntryRepository<'a> {
    conn: &'a mut Connection,
}

impl<'a> EntryRepository<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    /// Insert a new entry or refresh the `updated_at` / `last_seen_at`
    /// timestamps of the existing one with the same hash, all within a
    /// single transaction.
    ///
    /// The dedupe rule extends the legacy `content_hash` lookup with a
    /// secondary `rich_text_hash` match: a plain-text capture matches
    /// on `content_hash` alone (and so does an image row), but a
    /// rich-text capture only matches a previous row when *both* the
    /// plain-text hash and the rich-text hash agree. The same plain
    /// text with different styles therefore produces two rows instead
    /// of collapsing silently.
    ///
    /// A freshly inserted row is attached to the system `Historial`
    /// collection inside the same transaction so the invariant "every
    /// entry belongs to `Historial`" holds even when an error rolls
    /// the whole insert back.
    pub fn insert_or_touch(&mut self, new: NewEntry) -> Result<EntryOutcome, EntryRepositoryError> {
        let tx = self.conn.transaction()?;
        let plain_hash = new.content_hash.clone();
        let rich_hash = new.rich_text_hash.clone();
        let last_seen = format_timestamp(new.last_seen_at);

        let existing: Option<i64> = match rich_hash.as_deref() {
            Some(rich) => tx
                .query_row(
                    "SELECT id FROM clipboard_entries
                     WHERE content_hash = ?1
                       AND rich_text_hash IS NOT NULL
                       AND rich_text_hash = ?2",
                    params![plain_hash, rich],
                    |row| row.get(0),
                )
                .optional()?,
            None => tx
                .query_row(
                    "SELECT id FROM clipboard_entries
                     WHERE content_hash = ?1 AND rich_text_hash IS NULL",
                    params![plain_hash],
                    |row| row.get(0),
                )
                .optional()?,
        };

        let outcome = if let Some(id) = existing {
            tx.execute(
                "UPDATE clipboard_entries
                 SET updated_at = ?1,
                     last_seen_at = ?1
                 WHERE id = ?2",
                params![last_seen, id],
            )?;
            let record = fetch_by_id(&tx, id)?.expect("row updated above must still exist");
            EntryOutcome::Updated(record)
        } else {
            let created_at = format_timestamp(new.created_at);
            tx.execute(
                "INSERT INTO clipboard_entries
                    (content, content_type, content_size, content_hash,
                     source_app, created_at, updated_at, last_seen_at,
                     asset_ref, mime_type, payload_width, payload_height,
                     rich_text_hash, rich_html_ref, rich_rtf_ref,
                     rich_preview_ref, rich_html_size, rich_rtf_size)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?8, ?9, ?10, ?11,
                         ?12, ?13, ?14, ?15, ?16, ?17)",
                params![
                    new.content,
                    new.content_type.as_str(),
                    new.content_size,
                    new.content_hash,
                    new.source_app,
                    created_at,
                    last_seen,
                    new.asset_ref,
                    new.mime_type,
                    new.payload_width,
                    new.payload_height,
                    new.rich_text_hash,
                    new.rich_html_ref,
                    new.rich_rtf_ref,
                    new.rich_preview_ref,
                    new.rich_html_size,
                    new.rich_rtf_size,
                ],
            )?;
            let id = tx.last_insert_rowid();
            // Attach the new row to the system `Historial` collection
            // inside the same transaction so an error here rolls the
            // insert back. The migration guarantees the row exists;
            // if it does not we surface the same typed error the
            // organization layer would surface to a service caller.
            crate::organization::OrganizationRepository::attach_entry_to_history_in_tx(
                &tx,
                id,
                new.created_at,
            )
            .map_err(EntryRepositoryError::Organization)?;
            let record = fetch_by_id(&tx, id)?.expect("row inserted above must still exist");
            EntryOutcome::Inserted(record)
        };

        tx.commit()?;
        Ok(outcome)
    }

    pub fn find_by_hash(&self, hash: &str) -> Result<Option<EntryRecord>, EntryRepositoryError> {
        let sql = format!(
            "SELECT {ENTRY_COLUMNS}
             FROM clipboard_entries
             WHERE content_hash = ?1"
        );
        let record = self
            .conn
            .query_row(&sql, params![hash], row_to_record)
            .optional()?;
        Ok(record)
    }

    pub fn find_by_id(&self, id: i64) -> Result<Option<EntryRecord>, EntryRepositoryError> {
        fetch_by_id(self.conn, id)
    }

    /// Most recent entries first, capped by `limit`. The desktop rail
    /// orders by the original capture datetime so a fresh capture
    /// always appears on the left edge, even when an older pinned
    /// row exists; favourites intentionally do NOT rank above newer
    /// captures on this surface — pin/unpin must not silently move
    /// a card out of its chronological slot.
    pub fn recent(&self, limit: usize) -> Result<Vec<EntryRecord>, EntryRepositoryError> {
        let limit = limit as i64;
        let sql = format!(
            "SELECT {ENTRY_COLUMNS}
             FROM clipboard_entries
             ORDER BY created_at DESC, id DESC
             LIMIT ?1"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit], row_to_record)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    /// Every text entry currently stored. Used by the in-memory search
    /// engine. No N+1: a single `SELECT` feeds the result vector.
    ///
    /// The query filters on the canonical textual-types list so
    /// classified entries (URL, JSON, JWT, …) keep surfacing in local
    /// search alongside the legacy `Text` rows, while non-textual rows
    /// (images and any future binary payload) stay out of the textual
    /// index. The search therefore never inspects image bytes.
    ///
    /// Ordering: `created_at DESC, id DESC` — the search engine
    /// ignores the order, but the convention now matches the rail so
    /// a future contributor cannot accidentally drift the two
    /// surfaces apart.
    pub fn text_entries(&self) -> Result<Vec<EntryRecord>, EntryRepositoryError> {
        let placeholders = TEXTUAL_CONTENT_TYPES
            .iter()
            .map(|c| format!("'{}'", c.as_str()))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT {ENTRY_COLUMNS}
             FROM clipboard_entries
             WHERE content_type IN ({placeholders})
             ORDER BY created_at DESC, id DESC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_record)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    /// Same as [`Self::text_entries`] but optionally restricted to the
    /// entries that belong to `collection_id` and/or carry every tag
    /// in `tag_ids` (AND semantics). Passing `collection_id = None`
    /// and an empty `tag_ids` slice reproduces the unfiltered
    /// behaviour of [`Self::text_entries`]; the dedicated unfiltered
    /// method stays as a hot-path optimisation for callers that do
    /// not need the extra `JOIN`s.
    ///
    /// The ordering intentionally matches the rail: `created_at DESC`
    /// then `id DESC` so the search hits the freshness the rest of
    /// the rail surfaces without a second sort.
    pub fn text_entries_filtered(
        &self,
        collection_id: Option<i64>,
        tag_ids: &[i64],
    ) -> Result<Vec<EntryRecord>, EntryRepositoryError> {
        let placeholders = TEXTUAL_CONTENT_TYPES
            .iter()
            .map(|c| format!("'{}'", c.as_str()))
            .collect::<Vec<_>>()
            .join(",");
        let mut sql = format!(
            "SELECT {ENTRY_COLUMNS}
             FROM clipboard_entries
             WHERE content_type IN ({placeholders})"
        );
        if collection_id.is_some() {
            sql.push_str(
                " AND id IN (SELECT entry_id FROM entry_collections WHERE collection_id = ?)",
            );
        }
        if !tag_ids.is_empty() {
            let tag_placeholders = std::iter::repeat_n("?", tag_ids.len())
                .collect::<Vec<_>>()
                .join(",");
            sql.push_str(&format!(
                " AND id IN (
                    SELECT entry_id FROM entry_tags
                     WHERE tag_id IN ({tag_placeholders})
                     GROUP BY entry_id
                     HAVING COUNT(DISTINCT tag_id) = ?
                )"
            ));
        }
        sql.push_str(" ORDER BY created_at DESC, id DESC");

        // Own every parameter as an `i64` so the dynamic Vec lives
        // long enough for `query_map` to consume every binding.
        let mut params_dyn: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(cid) = collection_id {
            params_dyn.push(Box::new(cid));
        }
        for tag in tag_ids {
            params_dyn.push(Box::new(*tag));
        }
        if !tag_ids.is_empty() {
            params_dyn.push(Box::new(tag_ids.len() as i64));
        }
        let bound: Vec<&dyn rusqlite::ToSql> = params_dyn.iter().map(|b| &**b).collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(bound.as_slice(), row_to_record)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    /// Variant of [`Self::text_entries_filtered`] that does **not**
    /// restrict the candidate set to the textual content types: the
    /// recent-entries rail rendered by the Tauri shell must surface
    /// image and rich-text rows too, and the org filter must keep
    /// them eligible when they belong to the selected collection.
    ///
    /// The query shape mirrors [`Self::text_entries_filtered`]; the
    /// only difference is the absence of a `content_type IN (…)`
    /// clause. The ordering follows the rail contract:
    /// `created_at DESC` then `id DESC`. Pin/unpin must never move a
    /// card out of its chronological slot.
    pub fn entries_filtered(
        &self,
        collection_id: Option<i64>,
        tag_ids: &[i64],
    ) -> Result<Vec<EntryRecord>, EntryRepositoryError> {
        let mut sql = format!(
            "SELECT {ENTRY_COLUMNS}
             FROM clipboard_entries"
        );
        let mut first_clause = true;
        if collection_id.is_some() {
            sql.push_str(
                " WHERE id IN (SELECT entry_id FROM entry_collections WHERE collection_id = ?)",
            );
            first_clause = false;
        }
        if !tag_ids.is_empty() {
            let tag_placeholders = std::iter::repeat_n("?", tag_ids.len())
                .collect::<Vec<_>>()
                .join(",");
            sql.push_str(if first_clause { " WHERE " } else { " AND " });
            sql.push_str(&format!(
                "id IN (
                    SELECT entry_id FROM entry_tags
                     WHERE tag_id IN ({tag_placeholders})
                     GROUP BY entry_id
                     HAVING COUNT(DISTINCT tag_id) = ?
                )"
            ));
        }
        sql.push_str(" ORDER BY created_at DESC, id DESC");

        let mut params_dyn: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(cid) = collection_id {
            params_dyn.push(Box::new(cid));
        }
        for tag in tag_ids {
            params_dyn.push(Box::new(*tag));
        }
        if !tag_ids.is_empty() {
            params_dyn.push(Box::new(tag_ids.len() as i64));
        }
        let bound: Vec<&dyn rusqlite::ToSql> = params_dyn.iter().map(|b| &**b).collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(bound.as_slice(), row_to_record)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    /// Every distinct, non-empty `asset_ref` still referenced by a row.
    ///
    /// This is the *only* input the asset collector is allowed to use
    /// to decide what may be deleted: a file whose name is not in this
    /// set has no remaining reference. Because the set is derived from
    /// SQLite rather than from an in-flight operation, two rows that
    /// legitimately share the same normalised asset both keep it
    /// alive.
    pub fn referenced_asset_refs(&self) -> Result<BTreeSet<String>, EntryRepositoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT asset_ref FROM clipboard_entries
             WHERE asset_ref IS NOT NULL AND asset_ref <> ''",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut refs = BTreeSet::new();
        for row in rows {
            refs.insert(row?);
        }
        Ok(refs)
    }

    /// Every distinct rich-text asset reference still in use. The
    /// returned set unions `rich_html_ref`, `rich_rtf_ref` and
    /// `rich_preview_ref` so the collector can walk the rich-text
    /// namespace once and know what to spare.
    pub fn referenced_rich_asset_refs(&self) -> Result<BTreeSet<String>, EntryRepositoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT rich_html_ref FROM clipboard_entries
             WHERE rich_html_ref IS NOT NULL AND rich_html_ref <> ''
             UNION
             SELECT rich_rtf_ref FROM clipboard_entries
             WHERE rich_rtf_ref IS NOT NULL AND rich_rtf_ref <> ''
             UNION
             SELECT rich_preview_ref FROM clipboard_entries
             WHERE rich_preview_ref IS NOT NULL AND rich_preview_ref <> ''",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut refs = BTreeSet::new();
        for row in rows {
            refs.insert(row?);
        }
        Ok(refs)
    }

    /// How many rows currently reference `asset_ref`. Used by the
    /// collector's guard rail: a shared asset must never be deleted
    /// because one of its referring rows disappeared.
    pub fn count_asset_references(&self, asset_ref: &str) -> Result<i64, EntryRepositoryError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM clipboard_entries WHERE asset_ref = ?1",
            params![asset_ref],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Set the favorite flag for a single entry. Idempotent: applying
    /// the same target state leaves the row untouched except for the
    /// `updated_at` refresh, and never creates a duplicate row.
    pub fn set_favorite(
        &mut self,
        id: i64,
        pinned: bool,
        now: OffsetDateTime,
    ) -> Result<SetFavoriteOutcome, EntryRepositoryError> {
        let tx = self.conn.transaction()?;
        let now_str = format_timestamp(now);
        let pinned_int = if pinned { 1i64 } else { 0i64 };
        let updated = tx.execute(
            "UPDATE clipboard_entries
             SET is_pinned = ?1, updated_at = ?2
             WHERE id = ?3",
            params![pinned_int, now_str, id],
        )?;
        let record = if updated == 0 {
            None
        } else {
            fetch_by_id(&tx, id)?
        };
        tx.commit()?;
        Ok(SetFavoriteOutcome { updated: record })
    }

    /// Set or clear the card title for a single entry. The repository
    /// trusts the caller to have validated `title` already (length,
    /// non-empty when present, trimming); it simply stores the
    /// payload, treating an empty string as the "restore default"
    /// signal by clearing the column.
    ///
    /// `title` is `Option<String>` so callers can:
    /// - set a title by passing `Some("custom")`;
    /// - restore the default title by passing `None`.
    pub fn set_title(
        &mut self,
        id: i64,
        title: Option<&str>,
        now: OffsetDateTime,
    ) -> Result<SetTitleOutcome, EntryRepositoryError> {
        let tx = self.conn.transaction()?;
        let now_str = format_timestamp(now);
        let stored = title.map(normalise_title);
        let updated = tx.execute(
            "UPDATE clipboard_entries
             SET title = ?1, updated_at = ?2
             WHERE id = ?3",
            params![stored, now_str, id],
        )?;
        let record = if updated == 0 {
            None
        } else {
            fetch_by_id(&tx, id)?
        };
        tx.commit()?;
        Ok(SetTitleOutcome { updated: record })
    }

    /// Set the source-application presentation metadata for an
    /// existing entry. Passing `None` for either `name` or `icon_ref`
    /// leaves the previously stored value untouched (similar to
    /// [`crate::IgnoredAppRepository::upsert`]) so a transient
    /// resolution failure cannot wipe out previously persisted
    /// metadata.
    pub fn set_source_app_metadata(
        &mut self,
        id: i64,
        source_app_name: Option<&str>,
        source_app_icon_ref: Option<&str>,
        now: OffsetDateTime,
    ) -> Result<SetSourceAppMetadataOutcome, EntryRepositoryError> {
        let tx = self.conn.transaction()?;
        let now_str = format_timestamp(now);
        let updated = tx.execute(
            "UPDATE clipboard_entries
             SET source_app_name = COALESCE(?1, source_app_name),
                 source_app_icon_ref = COALESCE(?2, source_app_icon_ref),
                 updated_at = ?3
             WHERE id = ?4",
            params![source_app_name, source_app_icon_ref, now_str, id],
        )?;
        let record = if updated == 0 {
            None
        } else {
            fetch_by_id(&tx, id)?
        };
        tx.commit()?;
        Ok(SetSourceAppMetadataOutcome { updated: record })
    }

    /// Delete a single entry by id. Returns the number of rows removed
    /// (0 when the entry was already absent, 1 when it existed). The
    /// caller MUST treat 0 as a successful no-op: the contract is
    /// idempotent.
    pub fn delete_entry(&mut self, id: i64) -> Result<usize, EntryRepositoryError> {
        let tx = self.conn.transaction()?;
        let removed = tx.execute("DELETE FROM clipboard_entries WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(removed)
    }

    /// Delete every non-favorite entry. Returns the number of rows
    /// removed. Favorite entries are left intact. The whole operation
    /// is wrapped in a single transaction so a failure leaves the
    /// table untouched.
    pub fn clear_non_favorites(&mut self) -> Result<usize, EntryRepositoryError> {
        let tx = self.conn.transaction()?;
        let removed = tx.execute("DELETE FROM clipboard_entries WHERE is_pinned = 0", [])?;
        tx.commit()?;
        Ok(removed)
    }

    /// Count non-favorite entries that would be removed by
    /// [`Self::clear_unorganized_history`]. Lets the frontend render
    /// a confirmation message before triggering the mutation.
    ///
    /// "Unorganized" means: pinned == 0 AND the row has **no**
    /// association with a user-defined (`kind = "user"`) collection.
    /// Every entry belongs to the system `Historial` collection by
    /// construction, so the user-collection predicate is the only
    /// meaningful one — being in `Historial` alone is not a reason to
    /// keep the row.
    pub fn count_unorganized_clearable(&self) -> Result<i64, EntryRepositoryError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM clipboard_entries ce
              WHERE ce.is_pinned = 0
                AND EXISTS (
                    SELECT 1 FROM entry_collections ec
                      JOIN collections c ON c.id = ec.collection_id
                     WHERE ec.entry_id = ce.id
                       AND c.stable_key = 'history'
                )
                AND NOT EXISTS (
                    SELECT 1 FROM entry_collections ec
                      JOIN collections c ON c.id = ec.collection_id
                     WHERE ec.entry_id = ce.id
                       AND c.kind = 'user'
                )",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Delete every non-favorite entry that does **not** belong to any
    /// user-defined collection. The system `Historial` collection
    /// counts as the default bucket — every entry belongs to it by
    /// construction — so an entry is removed only when the user's
    /// only association is `Historial`.
    ///
    /// Concretely the predicate is:
    ///
    /// - `is_pinned = 0`; favorites and their assets are never removed
    ///   regardless of collection membership;
    /// - the row has at least one `entry_collections` row pointing at
    ///   the system `Historial` collection (always true, asserted
    ///   defensively so a broken database cannot trigger a
    ///   mass-delete);
    /// - the row has **no** `entry_collections` row pointing at a
    ///   user collection.
    ///
    /// Returns the number of rows removed. Wrapped in a single
    /// transaction so a failure leaves the table untouched. Used by
    /// the trash button so the action cannot accidentally wipe an
    /// entry the user explicitly grouped into a secondary collection.
    pub fn clear_unorganized_history(&mut self) -> Result<usize, EntryRepositoryError> {
        let tx = self.conn.transaction()?;
        let removed = tx.execute(
            "DELETE FROM clipboard_entries
              WHERE is_pinned = 0
                AND EXISTS (
                    SELECT 1 FROM entry_collections ec
                      JOIN collections c ON c.id = ec.collection_id
                     WHERE ec.entry_id = clipboard_entries.id
                       AND c.stable_key = 'history'
                )
                AND NOT EXISTS (
                    SELECT 1 FROM entry_collections ec
                      JOIN collections c ON c.id = ec.collection_id
                     WHERE ec.entry_id = clipboard_entries.id
                       AND c.kind = 'user'
                )",
            [],
        )?;
        tx.commit()?;
        Ok(removed)
    }

    /// Delete every non-favorite entry whose `created_at` is older
    /// than `cutoff`. Returns the number of rows removed. Favorite
    /// entries are never removed, regardless of age. The whole
    /// operation is wrapped in a single transaction.
    pub fn delete_non_favorites_older_than(
        &mut self,
        cutoff: OffsetDateTime,
    ) -> Result<usize, EntryRepositoryError> {
        let tx = self.conn.transaction()?;
        let cutoff_str = format_timestamp(cutoff);
        let removed = tx.execute(
            "DELETE FROM clipboard_entries
             WHERE is_pinned = 0 AND created_at < ?1",
            params![cutoff_str],
        )?;
        tx.commit()?;
        Ok(removed)
    }

    /// How many non-favorite entries would be removed by the supplied
    /// cutoff. Used by the frontend to render a confirmation prompt
    /// without triggering the mutation first.
    pub fn count_non_favorites_older_than(
        &self,
        cutoff: OffsetDateTime,
    ) -> Result<i64, EntryRepositoryError> {
        let cutoff_str = format_timestamp(cutoff);
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM clipboard_entries
             WHERE is_pinned = 0 AND created_at < ?1",
            params![cutoff_str],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn count(&self) -> Result<i64, EntryRepositoryError> {
        let count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM clipboard_entries", [], |row| {
                    row.get(0)
                })?;
        Ok(count)
    }
}

/// Normalise a title before persistence. Whitespace is trimmed; an
/// empty result is mapped to `None` so the "restore default" path
/// stays a single column write.
fn normalise_title(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn fetch_by_id(conn: &Connection, id: i64) -> Result<Option<EntryRecord>, EntryRepositoryError> {
    let sql = format!(
        "SELECT {ENTRY_COLUMNS}
         FROM clipboard_entries
         WHERE id = ?1"
    );
    let record = conn
        .query_row(&sql, params![id], row_to_record)
        .optional()?;
    Ok(record)
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<EntryRecord> {
    let content_type_raw: String = row.get(2)?;
    let content_type = parse_content_type(&content_type_raw).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(EntryRepositoryError::Sqlite(rusqlite::Error::InvalidQuery)),
        )
    })?;
    let pinned: i64 = row.get(6)?;

    Ok(EntryRecord {
        id: row.get(0)?,
        content: row.get(1)?,
        content_type,
        content_size: row.get(3)?,
        content_hash: row.get(4)?,
        source_app: row.get(5)?,
        is_pinned: pinned != 0,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        last_seen_at: row.get(9)?,
        title: row.get(10)?,
        source_app_name: row.get(11)?,
        source_app_icon_ref: row.get(12)?,
        asset_ref: row.get(13)?,
        mime_type: row.get(14)?,
        payload_width: row.get(15)?,
        payload_height: row.get(16)?,
        rich_text_hash: row.get(17)?,
        rich_html_ref: row.get(18)?,
        rich_rtf_ref: row.get(19)?,
        rich_preview_ref: row.get(20)?,
        rich_html_size: row.get(21)?,
        rich_rtf_size: row.get(22)?,
    })
}

fn parse_content_type(raw: &str) -> Option<ContentType> {
    match raw {
        "text" => Some(ContentType::Text),
        "url" => Some(ContentType::Url),
        "email" => Some(ContentType::Email),
        "json" => Some(ContentType::Json),
        "jwt" => Some(ContentType::Jwt),
        "uuid" => Some(ContentType::Uuid),
        "ipv4" => Some(ContentType::Ipv4),
        "ipv6" => Some(ContentType::Ipv6),
        "hex_color" => Some(ContentType::HexColor),
        "html" => Some(ContentType::Html),
        "file_path" => Some(ContentType::FilePath),
        "shell_command" => Some(ContentType::ShellCommand),
        "sql" => Some(ContentType::Sql),
        "code" => Some(ContentType::Code),
        "image" => Some(ContentType::Image),
        _ => None,
    }
}

/// Canonical list of textual content types included by
/// [`EntryRepository::text_entries`]. Kept in source so the SQL filter
/// and the test suite share the same truth source.
pub const TEXTUAL_CONTENT_TYPES: &[ContentType] = &[
    ContentType::Text,
    ContentType::Url,
    ContentType::Email,
    ContentType::Json,
    ContentType::Jwt,
    ContentType::Uuid,
    ContentType::Ipv4,
    ContentType::Ipv6,
    ContentType::HexColor,
    ContentType::Html,
    ContentType::FilePath,
    ContentType::ShellCommand,
    ContentType::Sql,
    ContentType::Code,
];

fn format_timestamp(ts: OffsetDateTime) -> String {
    ts.format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::ContentType;
    use time::macros::datetime;

    fn new_entry(content: &str, when: OffsetDateTime) -> NewEntry {
        NewEntry::text(
            content.to_string(),
            ContentType::Text,
            content.len() as i64,
            format!("hash::{content}"),
            Some("test-app".to_string()),
            when,
            when,
        )
    }

    fn new_entry_with_type(
        content: &str,
        content_type: ContentType,
        when: OffsetDateTime,
    ) -> NewEntry {
        NewEntry::text(
            content.to_string(),
            content_type,
            content.len() as i64,
            format!("hash::{content}::{content_type:?}"),
            Some("test-app".to_string()),
            when,
            when,
        )
    }

    /// Image entry shaped exactly like the capture pipeline produces
    /// one: empty `content` sentinel, `image` type, PNG hash as the
    /// dedupe key and the relative asset reference derived from it.
    fn new_image_entry(hash: &str, width: u32, height: u32, when: OffsetDateTime) -> NewEntry {
        NewEntry {
            content: crate::entry::IMAGE_CONTENT_SENTINEL.to_string(),
            content_type: ContentType::Image,
            content_size: 1_024,
            content_hash: hash.to_string(),
            source_app: Some("test-app".to_string()),
            created_at: when,
            last_seen_at: when,
            asset_ref: Some(format!("clipboard/{hash}.png")),
            mime_type: Some(crate::entry::IMAGE_MIME_PNG.to_string()),
            payload_width: Some(width),
            payload_height: Some(height),
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
        }
    }

    #[allow(dead_code)]
    fn new_rich_entry(
        plain: &str,
        hash: &str,
        html: Option<&str>,
        rtf: Option<&[u8]>,
        when: OffsetDateTime,
    ) -> NewEntry {
        NewEntry {
            content: plain.to_string(),
            content_type: ContentType::Text,
            content_size: plain.len() as i64,
            content_hash: format!("hash::{plain}"),
            source_app: Some("test-app".to_string()),
            created_at: when,
            last_seen_at: when,
            asset_ref: None,
            mime_type: None,
            payload_width: None,
            payload_height: None,
            rich_text_hash: Some(hash.to_string()),
            rich_html_ref: html.map(|_| format!("rich-text/{hash}.html")),
            rich_rtf_ref: rtf.map(|_| format!("rich-text/{hash}.rtf")),
            rich_preview_ref: Some(format!("rich-text/{hash}.preview.html")),
            rich_html_size: html.map(|value| value.len() as i64),
            rich_rtf_size: rtf.map(|value| value.len() as i64),
        }
    }

    fn open_temp_db() -> (tempfile::TempDir, crate::Database) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = crate::Database::open(dir.path().join("clipvault.db")).expect("open");
        let migrations = crate::builtin_migrations();
        let mut db = db;
        db.run_migrations(&migrations).expect("migrate");
        (dir, db)
    }

    #[test]
    fn insert_or_touch_creates_then_updates() {
        let (_dir, mut db) = open_temp_db();
        let t1 = datetime!(2026-01-02 03:04:05 UTC);
        let t2 = datetime!(2026-01-02 03:05:00 UTC);

        let outcome = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("hello", t1))
                .expect("first insert")
        };
        let first_id = outcome.record().id;

        let outcome = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("hello", t2)).expect("touch")
        };

        match outcome {
            EntryOutcome::Updated(record) => {
                assert_eq!(record.id, first_id);
                assert_eq!(record.updated_at, format_timestamp(t2));
                assert_eq!(record.last_seen_at, format_timestamp(t2));
                assert_eq!(record.created_at, format_timestamp(t1));
                assert_eq!(record.content, "hello");
            }
            other => panic!("expected Updated, got {other:?}"),
        }

        let repo = EntryRepository::new(db.connection_mut());
        assert_eq!(repo.count().unwrap(), 1);
    }

    #[test]
    fn insert_or_touch_different_content_creates_new_row() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);

        let first = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("hello", when))
                .expect("first")
        };
        let second = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("world", when))
                .expect("second")
        };

        assert_ne!(first.record().id, second.record().id);
        let repo = EntryRepository::new(db.connection_mut());
        assert_eq!(repo.count().unwrap(), 2);
    }

    #[test]
    fn recent_returns_entries_in_descending_updated_order() {
        let (_dir, mut db) = open_temp_db();
        let t1 = datetime!(2026-01-02 03:04:05 UTC);
        let t2 = datetime!(2026-01-02 03:10:00 UTC);

        {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("first", t1)).unwrap();
        }
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("second", t2)).unwrap();
        }

        let repo = EntryRepository::new(db.connection_mut());
        let recent = repo.recent(10).expect("recent");
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].content, "second");
        assert_eq!(recent[1].content, "first");
    }

    #[test]
    fn find_by_hash_returns_none_for_unknown_content() {
        let (_dir, mut db) = open_temp_db();
        let repo = EntryRepository::new(db.connection_mut());
        assert!(repo.find_by_hash("missing").unwrap().is_none());
    }

    #[test]
    fn text_entries_returns_only_text_rows_in_recent_order() {
        let (_dir, mut db) = open_temp_db();
        let t1 = datetime!(2026-01-02 03:04:05 UTC);
        let t2 = datetime!(2026-01-02 03:10:00 UTC);

        {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("first", t1)).unwrap();
        }
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("second", t2)).unwrap();
        }
        // Re-touch the older entry to update its timestamp; the
        // text_entries query should still return it because it kept its
        // text type.
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("first", t2)).unwrap();
        }

        let repo = EntryRepository::new(db.connection_mut());
        let entries = repo.text_entries().unwrap();
        let contents: Vec<&str> = entries.iter().map(|e| e.content.as_str()).collect();
        // Both rows share the same `updated_at` (t2), so the id-desc
        // tiebreak orders "second" (higher id) before "first".
        assert_eq!(contents, vec!["second", "first"]);
    }

    #[test]
    fn set_favorite_is_idempotent_and_returns_record() {
        let (_dir, mut db) = open_temp_db();
        let t1 = datetime!(2026-01-02 03:04:05 UTC);
        let t2 = datetime!(2026-01-02 03:10:00 UTC);
        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("alpha", t1))
                .expect("insert")
                .record()
                .id
        };

        // First pin: row is updated.
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            let outcome = repo.set_favorite(id, true, t2).expect("set_favorite");
            let record = outcome.updated.expect("record returned");
            assert!(record.is_pinned);
            assert_eq!(record.updated_at, format_timestamp(t2));
        }

        // Second pin with the same target state: still successful, no
        // new rows created.
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            let outcome = repo.set_favorite(id, true, t2).expect("set_favorite again");
            assert!(outcome.updated.expect("record").is_pinned);
        }
        assert_eq!(
            EntryRepository::new(db.connection_mut()).count().unwrap(),
            1
        );
    }

    #[test]
    fn set_favorite_for_missing_id_returns_none() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let mut repo = EntryRepository::new(db.connection_mut());
        let outcome = repo.set_favorite(999, true, when).expect("set_favorite");
        assert!(outcome.updated.is_none());
    }

    #[test]
    fn delete_entry_is_idempotent_and_removes_only_target() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let (keep, drop) = {
            let mut repo = EntryRepository::new(db.connection_mut());
            let a = repo
                .insert_or_touch(new_entry("keep", when))
                .expect("a")
                .record()
                .id;
            let b = repo
                .insert_or_touch(new_entry("drop", when))
                .expect("b")
                .record()
                .id;
            (a, b)
        };

        {
            let mut repo = EntryRepository::new(db.connection_mut());
            let removed = repo.delete_entry(drop).expect("delete");
            assert_eq!(removed, 1);
        }
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            let removed = repo.delete_entry(drop).expect("delete again");
            assert_eq!(removed, 0, "second delete is a no-op");
        }
        let repo = EntryRepository::new(db.connection_mut());
        let remaining = repo.count().unwrap();
        assert_eq!(remaining, 1);
        assert_eq!(repo.find_by_id(keep).unwrap().unwrap().content, "keep");
    }

    #[test]
    fn clear_non_favorites_preserves_pinned_entries() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let (pinned, _) = {
            let mut repo = EntryRepository::new(db.connection_mut());
            let pinned = repo
                .insert_or_touch(new_entry("pinned", when))
                .expect("pinned")
                .record()
                .id;
            repo.insert_or_touch(new_entry("other-1", when))
                .expect("other-1");
            repo.insert_or_touch(new_entry("other-2", when))
                .expect("other-2");
            repo.set_favorite(pinned, true, when)
                .expect("favorite")
                .updated
                .expect("record");
            (pinned, ())
        };

        let removed = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.clear_non_favorites().expect("clear")
        };
        assert_eq!(removed, 2);

        let repo = EntryRepository::new(db.connection_mut());
        let remaining: Vec<String> = repo
            .recent(10)
            .unwrap()
            .into_iter()
            .map(|r| r.content)
            .collect();
        assert_eq!(remaining, vec!["pinned".to_string()]);
        let _ = pinned;
    }

    #[test]
    fn count_unorganized_clearable_ignores_pinned_and_secondary_collection_rows() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let (drop, pinned, secondary) = {
            let mut repo = EntryRepository::new(db.connection_mut());
            let drop_id = repo
                .insert_or_touch(new_entry("drop", when))
                .expect("drop")
                .record()
                .id;
            let pinned_id = repo
                .insert_or_touch(new_entry("pinned", when))
                .expect("pinned")
                .record()
                .id;
            let secondary_id = repo
                .insert_or_touch(new_entry("secondary", when))
                .expect("secondary")
                .record()
                .id;
            repo.set_favorite(pinned_id, true, when)
                .expect("favorite")
                .updated
                .expect("record");
            (drop_id, pinned_id, secondary_id)
        };
        let work_id = {
            let mut repo = crate::OrganizationRepository::new(db.connection_mut());
            let work = repo
                .create_user_collection("Trabajo", when)
                .expect("create")
                .id;
            repo.replace_entry_collections(secondary, &[work], when)
                .expect("associate");
            work
        };

        let repo = EntryRepository::new(db.connection_mut());
        assert_eq!(repo.count_unorganized_clearable().unwrap(), 1);
        let _ = (drop, pinned, work_id);
    }

    #[test]
    fn clear_unorganized_history_keeps_pinned_and_secondary_rows() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let (drop, pinned, secondary, multi) = {
            let mut repo = EntryRepository::new(db.connection_mut());
            let drop = repo
                .insert_or_touch(new_entry("drop", when))
                .expect("drop")
                .record()
                .id;
            let pinned = repo
                .insert_or_touch(new_entry("pinned", when))
                .expect("pinned")
                .record()
                .id;
            let secondary = repo
                .insert_or_touch(new_entry("secondary", when))
                .expect("secondary")
                .record()
                .id;
            let multi = repo
                .insert_or_touch(new_entry("multi", when))
                .expect("multi")
                .record()
                .id;
            repo.set_favorite(pinned, true, when)
                .expect("favorite")
                .updated
                .expect("record");
            (drop, pinned, secondary, multi)
        };
        let (work, personal) = {
            let mut repo = crate::OrganizationRepository::new(db.connection_mut());
            let work = repo
                .create_user_collection("Trabajo", when)
                .expect("create")
                .id;
            let personal = repo
                .create_user_collection("Personal", when)
                .expect("create personal")
                .id;
            // `secondary` only lives in `Trabajo`.
            repo.replace_entry_collections(secondary, &[work], when)
                .expect("associate secondary");
            // `multi` lives in BOTH secondary collections.
            repo.replace_entry_collections(multi, &[work, personal], when)
                .expect("associate multi");
            (work, personal)
        };

        let removed = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.clear_unorganized_history().expect("clear")
        };
        assert_eq!(removed, 1, "only the unorganized row should be removed");

        let repo = EntryRepository::new(db.connection_mut());
        assert!(repo.find_by_id(drop).unwrap().is_none());
        assert!(repo.find_by_id(pinned).unwrap().is_some());
        assert!(repo.find_by_id(secondary).unwrap().is_some());
        assert!(repo.find_by_id(multi).unwrap().is_some());
        let _ = (work, personal);
    }

    #[test]
    fn clear_unorganized_history_keeps_image_row_with_secondary_collection() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let work_id = {
            let mut repo = crate::OrganizationRepository::new(db.connection_mut());
            repo.create_user_collection("Trabajo", when)
                .expect("create")
                .id
        };
        let (image_id, image_hash) = {
            let mut repo = EntryRepository::new(db.connection_mut());
            let hash = "ab".repeat(32);
            let id = repo
                .insert_or_touch(new_image_entry(&hash, 8, 8, when))
                .expect("image")
                .record()
                .id;
            (id, hash)
        };
        {
            let mut repo = crate::OrganizationRepository::new(db.connection_mut());
            repo.replace_entry_collections(image_id, &[work_id], when)
                .expect("associate image");
        }

        let removed = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.clear_unorganized_history().expect("clear")
        };
        assert_eq!(removed, 0, "an image in a user collection must survive");

        let repo = EntryRepository::new(db.connection_mut());
        let survivor = repo.find_by_id(image_id).unwrap().expect("image preserved");
        assert!(survivor.is_renderable_image());
        let refs = repo.referenced_asset_refs().expect("refs");
        assert!(refs.contains(&format!("clipboard/{image_hash}.png")));
    }

    #[test]
    fn clear_unorganized_history_is_idempotent_and_leaves_a_clean_table() {
        // The operation is wrapped in a single transaction. After it
        // runs, calling it again must remove zero rows so a re-run
        // (e.g. a duplicated click on the trash button) is a no-op
        // rather than a second mutation.
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("drop-1", when))
                .expect("drop-1");
            repo.insert_or_touch(new_entry("drop-2", when))
                .expect("drop-2");
        }

        let first = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.clear_unorganized_history().expect("clear")
        };
        assert_eq!(first, 2);

        let second = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.clear_unorganized_history().expect("clear again")
        };
        assert_eq!(second, 0, "a second call must be a no-op");

        let repo = EntryRepository::new(db.connection_mut());
        assert_eq!(repo.count().unwrap(), 0);
    }

    #[test]
    fn delete_older_than_skips_pinned_entries() {
        let (_dir, mut db) = open_temp_db();
        let old = datetime!(2025-12-01 00:00:00 UTC);
        let recent = datetime!(2026-02-01 00:00:00 UTC);
        let cutoff = datetime!(2026-01-15 00:00:00 UTC);

        let (old_pinned_id, _) = {
            let mut repo = EntryRepository::new(db.connection_mut());
            let old_pinned = repo
                .insert_or_touch(new_entry("old-favorite", old))
                .expect("pinned")
                .record()
                .id;
            repo.insert_or_touch(new_entry("old-cleanup", old))
                .expect("old-cleanup");
            repo.insert_or_touch(new_entry("fresh", recent))
                .expect("fresh");
            repo.set_favorite(old_pinned, true, old).expect("favorite");
            (old_pinned, ())
        };

        let removed = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.delete_non_favorites_older_than(cutoff).expect("purge")
        };
        assert_eq!(removed, 1);

        let repo = EntryRepository::new(db.connection_mut());
        assert!(repo.find_by_id(old_pinned_id).unwrap().is_some());
        assert_eq!(repo.count().unwrap(), 2);
    }

    #[test]
    fn count_older_than_does_not_mutate() {
        let (_dir, mut db) = open_temp_db();
        let old = datetime!(2025-12-01 00:00:00 UTC);
        let recent = datetime!(2026-02-01 00:00:00 UTC);
        let cutoff = datetime!(2026-01-15 00:00:00 UTC);

        {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("old", old)).expect("old");
            repo.insert_or_touch(new_entry("new", recent)).expect("new");
        }

        let count_before = EntryRepository::new(db.connection_mut()).count().unwrap();
        let count = {
            let repo = EntryRepository::new(db.connection_mut());
            repo.count_non_favorites_older_than(cutoff).expect("count")
        };
        assert_eq!(count, 1);
        let count_after = EntryRepository::new(db.connection_mut()).count().unwrap();
        assert_eq!(count_before, count_after);
    }

    #[test]
    fn parse_content_type_round_trips_every_variant() {
        let cases = [
            ("text", ContentType::Text),
            ("url", ContentType::Url),
            ("email", ContentType::Email),
            ("json", ContentType::Json),
            ("jwt", ContentType::Jwt),
            ("uuid", ContentType::Uuid),
            ("ipv4", ContentType::Ipv4),
            ("ipv6", ContentType::Ipv6),
            ("hex_color", ContentType::HexColor),
            ("html", ContentType::Html),
            ("file_path", ContentType::FilePath),
            ("shell_command", ContentType::ShellCommand),
            ("sql", ContentType::Sql),
            ("code", ContentType::Code),
        ];
        for (raw, expected) in cases {
            assert_eq!(parse_content_type(raw), Some(expected), "raw = {raw:?}");
            assert_eq!(expected.as_str(), raw);
        }
    }

    #[test]
    fn parse_content_type_rejects_unknown_values() {
        assert!(parse_content_type("").is_none());
        assert!(parse_content_type("unknown").is_none());
        assert!(
            parse_content_type("TEXT").is_none(),
            "case must be snake_case"
        );
        assert!(parse_content_type("text/plain").is_none());
        assert!(parse_content_type("text ").is_none());
    }

    #[test]
    fn textual_content_types_constant_lists_every_textual_variant() {
        let variants = TEXTUAL_CONTENT_TYPES;
        assert!(variants.contains(&ContentType::Text));
        assert!(variants.contains(&ContentType::Url));
        assert!(variants.contains(&ContentType::Email));
        assert!(variants.contains(&ContentType::Json));
        assert!(variants.contains(&ContentType::Jwt));
        assert!(variants.contains(&ContentType::Uuid));
        assert!(variants.contains(&ContentType::Ipv4));
        assert!(variants.contains(&ContentType::Ipv6));
        assert!(variants.contains(&ContentType::HexColor));
        assert!(variants.contains(&ContentType::Html));
        assert!(variants.contains(&ContentType::FilePath));
        assert!(variants.contains(&ContentType::ShellCommand));
        assert!(variants.contains(&ContentType::Sql));
        assert!(variants.contains(&ContentType::Code));
        assert_eq!(
            variants.len(),
            14,
            "every textual variant listed exactly once"
        );
    }

    #[test]
    fn insert_or_touch_persists_classified_content_type() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);

        let variants = [
            ContentType::Text,
            ContentType::Url,
            ContentType::Email,
            ContentType::Json,
            ContentType::Jwt,
            ContentType::Uuid,
            ContentType::Ipv4,
            ContentType::Ipv6,
            ContentType::HexColor,
            ContentType::Html,
            ContentType::FilePath,
            ContentType::ShellCommand,
            ContentType::Sql,
            ContentType::Code,
        ];
        for variant in variants {
            let label = variant.as_str();
            let outcome = {
                let mut repo = EntryRepository::new(db.connection_mut());
                repo.insert_or_touch(new_entry_with_type(label, variant, when))
                    .expect("insert")
            };
            assert_eq!(outcome.record().content_type, variant);
        }
    }

    #[test]
    fn text_entries_includes_every_textual_variant() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);

        let variants = [
            ContentType::Text,
            ContentType::Url,
            ContentType::Email,
            ContentType::Json,
            ContentType::Jwt,
            ContentType::Uuid,
            ContentType::Ipv4,
            ContentType::Ipv6,
            ContentType::HexColor,
            ContentType::Html,
            ContentType::FilePath,
            ContentType::ShellCommand,
            ContentType::Sql,
            ContentType::Code,
        ];
        for variant in variants {
            let mut repo = EntryRepository::new(db.connection_mut());
            let label = variant.as_str();
            repo.insert_or_touch(new_entry_with_type(label, variant, when))
                .expect("insert");
        }

        let repo = EntryRepository::new(db.connection_mut());
        let entries = repo.text_entries().expect("text_entries");
        let observed: Vec<ContentType> = entries.iter().map(|e| e.content_type).collect();
        for variant in variants {
            assert!(
                observed.contains(&variant),
                "missing {variant:?} in {observed:?}"
            );
        }
        assert_eq!(entries.len(), variants.len());
    }

    #[test]
    fn duplicate_touch_preserves_original_content_type() {
        let (_dir, mut db) = open_temp_db();
        let t1 = datetime!(2026-01-02 03:04:05 UTC);
        let t2 = datetime!(2026-01-02 03:05:00 UTC);

        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry_with_type(
                "https://example.com",
                ContentType::Url,
                t1,
            ))
            .expect("first")
            .record()
            .id
        };

        // Re-insert the same hash but with a different (wrong) type —
        // the dedupe path MUST keep the original `content_type`.
        let updated = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(NewEntry::text(
                "https://example.com".to_string(),
                ContentType::Json, // would be wrong on purpose
                19,
                "hash::https://example.com::Url".to_string(),
                Some("test-app".to_string()),
                t1,
                t2,
            ))
            .expect("touch")
        };

        match updated {
            EntryOutcome::Updated(record) => {
                assert_eq!(record.id, id);
                assert_eq!(record.content_type, ContentType::Url);
            }
            other => panic!("expected Updated, got {other:?}"),
        }
        assert_eq!(
            EntryRepository::new(db.connection_mut()).count().unwrap(),
            1
        );
    }

    #[test]
    fn set_title_persists_value_and_restores_default() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let later = datetime!(2026-01-02 03:05:00 UTC);
        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("card-title", when))
                .expect("insert")
                .record()
                .id
        };

        let outcome = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.set_title(id, Some("Custom label"), later)
                .expect("set_title")
        };
        let record = outcome.updated.expect("record returned");
        assert_eq!(record.title.as_deref(), Some("Custom label"));
        assert_eq!(record.updated_at, format_timestamp(later));

        let restored = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.set_title(id, None, later + time::Duration::seconds(30))
                .expect("restore")
        };
        assert!(restored.updated.expect("record").title.is_none());
    }

    #[test]
    fn set_title_trims_whitespace_and_treats_empty_as_restore() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("trimmed", when))
                .expect("insert")
                .record()
                .id
        };

        let outcome = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.set_title(id, Some("   "), when).expect("set_title")
        };
        assert!(
            outcome.updated.expect("record").title.is_none(),
            "empty / whitespace title must clear the column"
        );

        let stored = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.set_title(id, Some("  spaced  "), when)
                .expect("set_title")
        };
        assert_eq!(
            stored.updated.expect("record").title.as_deref(),
            Some("spaced"),
            "whitespace must be trimmed before persistence"
        );
    }

    #[test]
    fn set_title_for_missing_id_returns_none() {
        let (_dir, mut db) = open_temp_db();
        let outcome = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.set_title(999, Some("label"), datetime!(2026-01-02 03:04:05 UTC))
                .expect("set_title")
        };
        assert!(outcome.updated.is_none());
    }

    #[test]
    fn set_source_app_metadata_preserves_existing_when_payload_is_none() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("with-app", when))
                .expect("insert")
                .record()
                .id
        };

        let first = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.set_source_app_metadata(
                id,
                Some("Terminal"),
                Some("application-icons/com.apple.terminal.png"),
                when,
            )
            .expect("set_metadata")
        };
        let record = first.updated.expect("record returned");
        assert_eq!(record.source_app_name.as_deref(), Some("Terminal"));
        assert_eq!(
            record.source_app_icon_ref.as_deref(),
            Some("application-icons/com.apple.terminal.png")
        );

        // A subsequent call with `None` for both fields MUST preserve
        // the previously stored values, mirroring the picker flow when
        // a re-selection cannot extract a fresh icon.
        let preserved = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.set_source_app_metadata(id, None, None, when)
                .expect("preserve")
        };
        let record = preserved.updated.expect("record");
        assert_eq!(record.source_app_name.as_deref(), Some("Terminal"));
        assert_eq!(
            record.source_app_icon_ref.as_deref(),
            Some("application-icons/com.apple.terminal.png")
        );
    }

    // -----------------------------------------------------------------
    // `clipboard-rich-content`: image rows, asset references and the
    // compatibility guarantees for pre-existing textual rows.
    // -----------------------------------------------------------------

    #[test]
    fn textual_rows_keep_null_payload_metadata() {
        // Compatibility contract: the four columns added by the asset
        // migration must stay `NULL` for a textual capture so a row
        // created before the migration and one created after are
        // indistinguishable.
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let record = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("plain text", when))
                .expect("insert")
                .record()
                .clone()
        };
        assert_eq!(record.content, "plain text");
        assert_eq!(record.content_type, ContentType::Text);
        assert!(record.asset_ref.is_none());
        assert!(record.mime_type.is_none());
        assert!(record.payload_width.is_none());
        assert!(record.payload_height.is_none());
        assert!(!record.is_renderable_image());
    }

    #[test]
    fn image_row_round_trips_every_metadata_field() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let hash = "b".repeat(64);
        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_image_entry(&hash, 640, 480, when))
                .expect("insert")
                .record()
                .id
        };

        let repo = EntryRepository::new(db.connection_mut());
        let record = repo.find_by_id(id).expect("query").expect("row");
        assert_eq!(record.content_type, ContentType::Image);
        assert_eq!(record.content, crate::entry::IMAGE_CONTENT_SENTINEL);
        assert_eq!(record.content_hash, hash);
        assert_eq!(
            record.asset_ref.as_deref(),
            Some(&*format!("clipboard/{hash}.png"))
        );
        assert_eq!(record.mime_type.as_deref(), Some("image/png"));
        assert_eq!(record.payload_width, Some(640));
        assert_eq!(record.payload_height, Some(480));
        assert!(record.is_renderable_image());
    }

    #[test]
    fn every_read_path_materialises_the_payload_metadata() {
        // Regression guard for the "one query forgot the new column"
        // bug class: `recent`, `find_by_id` and `find_by_hash` must all
        // return the asset reference.
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let hash = "c".repeat(64);
        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_image_entry(&hash, 8, 8, when))
                .expect("insert")
                .record()
                .id
        };

        let repo = EntryRepository::new(db.connection_mut());
        let expected = format!("clipboard/{hash}.png");
        assert_eq!(
            repo.find_by_id(id).unwrap().unwrap().asset_ref.as_deref(),
            Some(expected.as_str())
        );
        assert_eq!(
            repo.find_by_hash(&hash)
                .unwrap()
                .unwrap()
                .asset_ref
                .as_deref(),
            Some(expected.as_str())
        );
        let recent = repo.recent(10).expect("recent");
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].asset_ref.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn text_entries_excludes_image_rows() {
        // The local search stays textual: an image row must never
        // reach the index, so its bytes are never searched.
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("searchable", when))
                .expect("text");
            repo.insert_or_touch(new_image_entry(&"d".repeat(64), 4, 4, when))
                .expect("image");
        }
        let repo = EntryRepository::new(db.connection_mut());
        let entries = repo.text_entries().expect("text_entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "searchable");
        // The image row is still part of the history, just not of the
        // textual index.
        assert_eq!(repo.count().expect("count"), 2);
        assert_eq!(repo.recent(10).expect("recent").len(), 2);
    }

    #[test]
    fn insert_or_touch_attaches_new_rows_to_historial_atomically() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let outcome = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("hello world", when))
                .expect("insert")
        };
        let id = outcome.record().id;
        let history_id = {
            let repo = crate::OrganizationRepository::new(db.connection_mut());
            repo.system_collection_id(crate::HISTORY_STABLE_KEY)
                .expect("id")
                .expect("seeded")
        };
        let repo = crate::OrganizationRepository::new(db.connection_mut());
        let attached = repo.entry_collection_ids(id).expect("entry_collection_ids");
        assert_eq!(attached, vec![history_id]);
    }

    #[test]
    fn touch_does_not_duplicate_history_membership() {
        let (_dir, mut db) = open_temp_db();
        let t1 = datetime!(2026-01-02 03:04:05 UTC);
        let t2 = datetime!(2026-01-02 03:05:00 UTC);
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("dedupe", t1))
                .expect("first");
            repo.insert_or_touch(new_entry("dedupe", t2))
                .expect("touch");
        }
        let repo = crate::OrganizationRepository::new(db.connection_mut());
        let history = repo.entry_ids_in_history().expect("history ids");
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn text_entries_filtered_by_collection_returns_only_members() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let (a, b) = {
            let mut repo = EntryRepository::new(db.connection_mut());
            let a = repo
                .insert_or_touch(new_entry("alpha", when))
                .expect("alpha")
                .record()
                .id;
            let b = repo
                .insert_or_touch(new_entry("beta", when))
                .expect("beta")
                .record()
                .id;
            (a, b)
        };
        let collection_id = {
            let mut repo = crate::OrganizationRepository::new(db.connection_mut());
            repo.create_user_collection("Trabajo", when)
                .expect("create")
                .id
        };
        {
            let mut repo = crate::OrganizationRepository::new(db.connection_mut());
            repo.replace_entry_collections(b, &[collection_id], when)
                .expect("associate");
            let _ = a;
        }
        let repo = EntryRepository::new(db.connection_mut());
        let filtered = repo
            .text_entries_filtered(Some(collection_id), &[])
            .expect("filtered");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, b);
    }

    #[test]
    fn text_entries_filtered_by_tags_uses_and_semantics() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let (a, b) = {
            let mut repo = EntryRepository::new(db.connection_mut());
            let a = repo
                .insert_or_touch(new_entry("alpha", when))
                .expect("alpha")
                .record()
                .id;
            let b = repo
                .insert_or_touch(new_entry("beta", when))
                .expect("beta")
                .record()
                .id;
            (a, b)
        };
        let (codigo, pendiente) = {
            let mut repo = crate::OrganizationRepository::new(db.connection_mut());
            let codigo = repo.upsert_tag("codigo", when).expect("codigo").id;
            let pendiente = repo.upsert_tag("pendiente", when).expect("pendiente").id;
            repo.replace_entry_tags(a, &[codigo], when).expect("a tags");
            repo.replace_entry_tags(b, &[codigo, pendiente], when)
                .expect("b tags");
            (codigo, pendiente)
        };
        let repo = EntryRepository::new(db.connection_mut());
        let both = repo
            .text_entries_filtered(None, &[codigo, pendiente])
            .expect("both");
        assert_eq!(both.len(), 1);
        assert_eq!(both[0].id, b);
    }

    #[test]
    fn text_entries_filtered_with_no_filters_matches_unfiltered_query() {
        // When no collection or tag filters are supplied the filtered
        // query must produce the same row set as the unfiltered
        // helper — both reflect "every textual entry in the local
        // history", which is the implicit "no filter" baseline the
        // sidebar / search rely on.
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("alpha", when))
                .expect("alpha");
            repo.insert_or_touch(new_entry("beta", when)).expect("beta");
        }
        let unfiltered = {
            let repo = EntryRepository::new(db.connection_mut());
            repo.text_entries().expect("text_entries")
        };
        let filtered = {
            let repo = EntryRepository::new(db.connection_mut());
            repo.text_entries_filtered(None, &[]).expect("filtered")
        };
        let unfiltered_ids: Vec<i64> = unfiltered.iter().map(|r| r.id).collect();
        let filtered_ids: Vec<i64> = filtered.iter().map(|r| r.id).collect();
        assert_eq!(unfiltered_ids, filtered_ids);
    }

    // -----------------------------------------------------------------
    // `tags-and-collections` regression coverage for the rail filter.
    // The original change routed the filtered-rail query through the
    // textual-only helper, which silently dropped image and
    // rich-text rows as soon as the user selected a collection.
    // `entries_filtered` exists to keep every entry type eligible; the
    // tests below pin the contract so the regression cannot drift
    // back.
    // -----------------------------------------------------------------

    #[test]
    fn entries_filtered_with_no_filters_includes_image_rows() {
        // Regression: a freshly captured image must surface in the
        // rail even when the user has not selected any collection
        // or tag. The textual-only `text_entries()` helper would have
        // hidden it; `entries_filtered` must not.
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let text_id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("captured", when))
                .expect("text")
                .record()
                .id
        };
        let image_id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_image_entry(&"e".repeat(64), 8, 8, when))
                .expect("image")
                .record()
                .id
        };

        let repo = EntryRepository::new(db.connection_mut());
        let all = repo.entries_filtered(None, &[]).expect("entries_filtered");
        let ids: Vec<i64> = all.iter().map(|r| r.id).collect();
        assert!(ids.contains(&text_id), "text entry must surface");
        assert!(ids.contains(&image_id), "image entry must surface");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn entries_filtered_by_collection_includes_image_rows() {
        // Regression: selecting a user collection must not drop image
        // rows that belong to it. The buggy
        // `recent_entries_with_filter` used `text_entries_filtered`
        // and produced a rail that hid every image the user had
        // organised in `Trabajo`.
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let trabajo_id = {
            let mut repo = crate::OrganizationRepository::new(db.connection_mut());
            repo.create_user_collection("Trabajo", when)
                .expect("create")
                .id
        };
        let text_id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            let id = repo
                .insert_or_touch(new_entry("plain note", when))
                .expect("text")
                .record()
                .id;
            let mut org = crate::OrganizationRepository::new(db.connection_mut());
            org.replace_entry_collections(id, &[trabajo_id], when)
                .expect("attach");
            id
        };
        let image_id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            let id = repo
                .insert_or_touch(new_image_entry(&"f".repeat(64), 16, 16, when))
                .expect("image")
                .record()
                .id;
            let mut org = crate::OrganizationRepository::new(db.connection_mut());
            org.replace_entry_collections(id, &[trabajo_id], when)
                .expect("attach");
            id
        };

        let repo = EntryRepository::new(db.connection_mut());
        let filtered = repo
            .entries_filtered(Some(trabajo_id), &[])
            .expect("filtered");
        let ids: Vec<i64> = filtered.iter().map(|r| r.id).collect();
        assert!(ids.contains(&text_id), "text entry must surface");
        assert!(
            ids.contains(&image_id),
            "image entry must surface inside the collection"
        );
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn entries_filtered_by_tags_includes_image_rows() {
        // Regression: tagging an image must not drop it from the
        // rail. The textual-only helper hid the row the moment the
        // user attached a tag to an image capture.
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let tag_id = {
            let mut repo = crate::OrganizationRepository::new(db.connection_mut());
            repo.upsert_tag("draft", when).expect("tag").id
        };
        let text_id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            let id = repo
                .insert_or_touch(new_entry("plain note", when))
                .expect("text")
                .record()
                .id;
            let mut org = crate::OrganizationRepository::new(db.connection_mut());
            org.replace_entry_tags(id, &[tag_id], when)
                .expect("attach text");
            id
        };
        let image_id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            let id = repo
                .insert_or_touch(new_image_entry(&"1".repeat(64), 4, 4, when))
                .expect("image")
                .record()
                .id;
            let mut org = crate::OrganizationRepository::new(db.connection_mut());
            org.replace_entry_tags(id, &[tag_id], when)
                .expect("attach image");
            id
        };

        let repo = EntryRepository::new(db.connection_mut());
        let filtered = repo.entries_filtered(None, &[tag_id]).expect("filtered");
        let ids: Vec<i64> = filtered.iter().map(|r| r.id).collect();
        assert!(ids.contains(&text_id));
        assert!(ids.contains(&image_id));
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn entries_filtered_preserves_image_metadata() {
        // Regression: the filter must not strip asset_ref, MIME or
        // dimensions from the records it returns. The card relies on
        // those fields to render the thumbnail and the accessible
        // fallback.
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let hash = "a".repeat(64);
        let expected_ref = format!("clipboard/{hash}.png");
        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_image_entry(&hash, 32, 24, when))
                .expect("image")
                .record()
                .id
        };

        let repo = EntryRepository::new(db.connection_mut());
        let filtered = repo.entries_filtered(None, &[]).expect("filtered");
        let image = filtered
            .iter()
            .find(|r| r.id == id)
            .expect("image entry must surface");
        assert_eq!(image.asset_ref.as_deref(), Some(expected_ref.as_str()));
        assert_eq!(image.mime_type.as_deref(), Some("image/png"));
        assert_eq!(image.payload_width, Some(32));
        assert_eq!(image.payload_height, Some(24));
        assert!(image.is_renderable_image());
    }

    #[test]
    fn identical_image_hash_touches_the_existing_row_without_duplicating() {
        // Dedupe contract for images: the same normalised PNG hash
        // refreshes the row instead of creating a second one, and the
        // asset reference is untouched.
        let (_dir, mut db) = open_temp_db();
        let t1 = datetime!(2026-01-02 03:04:05 UTC);
        let t2 = datetime!(2026-01-02 03:09:05 UTC);
        let hash = "e".repeat(64);

        let first = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_image_entry(&hash, 10, 10, t1))
                .expect("first")
        };
        assert!(matches!(first, EntryOutcome::Inserted(_)));
        let first_id = first.record().id;

        let second = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_image_entry(&hash, 10, 10, t2))
                .expect("second")
        };
        match second {
            EntryOutcome::Updated(record) => {
                assert_eq!(record.id, first_id);
                assert_eq!(record.updated_at, format_timestamp(t2));
                assert_eq!(record.created_at, format_timestamp(t1));
                assert_eq!(
                    record.asset_ref.as_deref(),
                    Some(&*format!("clipboard/{hash}.png"))
                );
            }
            other => panic!("expected Updated, got {other:?}"),
        }
        assert_eq!(
            EntryRepository::new(db.connection_mut()).count().unwrap(),
            1
        );
    }

    #[test]
    fn different_image_hash_creates_a_distinct_row() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let (first, second) = {
            let mut repo = EntryRepository::new(db.connection_mut());
            let a = repo
                .insert_or_touch(new_image_entry(&"1".repeat(64), 2, 2, when))
                .expect("a")
                .record()
                .clone();
            let b = repo
                .insert_or_touch(new_image_entry(&"2".repeat(64), 3, 3, when))
                .expect("b")
                .record()
                .clone();
            (a, b)
        };
        assert_ne!(first.id, second.id);
        assert_ne!(first.asset_ref, second.asset_ref);
        // The prior entry's metadata is untouched by the new capture.
        let repo = EntryRepository::new(db.connection_mut());
        let reloaded = repo.find_by_id(first.id).unwrap().unwrap();
        assert_eq!(reloaded.asset_ref, first.asset_ref);
        assert_eq!(reloaded.payload_width, Some(2));
    }

    #[test]
    fn referenced_asset_refs_returns_only_live_distinct_references() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let alpha = "a1".repeat(32);
        let beta = "b2".repeat(32);
        let removable = {
            let mut repo = EntryRepository::new(db.connection_mut());
            // Textual rows contribute nothing to the set.
            repo.insert_or_touch(new_entry("text row", when))
                .expect("t");
            repo.insert_or_touch(new_image_entry(&alpha, 4, 4, when))
                .expect("alpha");
            repo.insert_or_touch(new_image_entry(&beta, 4, 4, when))
                .expect("beta")
                .record()
                .id
        };

        {
            let repo = EntryRepository::new(db.connection_mut());
            let refs = repo.referenced_asset_refs().expect("refs");
            assert_eq!(refs.len(), 2);
            assert!(refs.contains(&format!("clipboard/{alpha}.png")));
            assert!(refs.contains(&format!("clipboard/{beta}.png")));
        }

        {
            let mut repo = EntryRepository::new(db.connection_mut());
            assert_eq!(repo.delete_entry(removable).expect("delete"), 1);
        }

        let repo = EntryRepository::new(db.connection_mut());
        let refs = repo.referenced_asset_refs().expect("refs");
        assert_eq!(refs.len(), 1, "deleted row must drop out of the live set");
        assert!(refs.contains(&format!("clipboard/{alpha}.png")));
        assert!(!refs.contains(&format!("clipboard/{beta}.png")));
    }

    #[test]
    fn count_asset_references_sees_a_shared_asset() {
        // Two rows may legitimately point at the same normalised asset
        // (for example after a rollback/re-import). Deleting one must
        // leave the reference count at 1 so the collector keeps the file.
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let hash = "f".repeat(64);
        let shared_ref = format!("clipboard/{hash}.png");

        let second_id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_image_entry(&hash, 5, 5, when))
                .expect("first");
            // Distinct dedupe hash, deliberately the same asset.
            let mut shared = new_image_entry(&hash, 5, 5, when);
            shared.content_hash = format!("{hash}-variant");
            repo.insert_or_touch(shared).expect("second").record().id
        };

        {
            let repo = EntryRepository::new(db.connection_mut());
            assert_eq!(repo.count_asset_references(&shared_ref).unwrap(), 2);
        }
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.delete_entry(second_id).expect("delete");
        }
        let repo = EntryRepository::new(db.connection_mut());
        assert_eq!(
            repo.count_asset_references(&shared_ref).unwrap(),
            1,
            "the surviving row still references the asset"
        );
        let refs = repo.referenced_asset_refs().expect("refs");
        assert!(refs.contains(&shared_ref));
    }

    #[test]
    fn favorite_and_retention_rules_apply_to_image_rows() {
        // Images participate in favorites and retention like any other
        // entry: a pinned image survives the purge, a stale one does not.
        let (_dir, mut db) = open_temp_db();
        let old = datetime!(2025-12-01 00:00:00 UTC);
        let cutoff = datetime!(2026-01-15 00:00:00 UTC);
        let pinned_hash = "aa".repeat(32);
        let stale_hash = "bb".repeat(32);

        let pinned_id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            let pinned = repo
                .insert_or_touch(new_image_entry(&pinned_hash, 4, 4, old))
                .expect("pinned")
                .record()
                .id;
            repo.insert_or_touch(new_image_entry(&stale_hash, 4, 4, old))
                .expect("stale");
            let outcome = repo.set_favorite(pinned, true, old).expect("favorite");
            let record = outcome.updated.expect("record");
            assert!(record.is_pinned);
            // Pinning must not disturb the payload metadata.
            assert!(record.is_renderable_image());
            pinned
        };

        let removed = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.delete_non_favorites_older_than(cutoff).expect("purge")
        };
        assert_eq!(removed, 1);

        let repo = EntryRepository::new(db.connection_mut());
        let survivor = repo.find_by_id(pinned_id).unwrap().expect("favorite kept");
        assert!(survivor.is_renderable_image());
        let refs = repo.referenced_asset_refs().expect("refs");
        assert_eq!(refs.len(), 1);
        assert!(refs.contains(&format!("clipboard/{pinned_hash}.png")));
    }

    #[test]
    fn clear_non_favorites_drops_image_references_but_keeps_favorites() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let keep_hash = "cc".repeat(32);
        let drop_hash = "dd".repeat(32);
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            let keep = repo
                .insert_or_touch(new_image_entry(&keep_hash, 4, 4, when))
                .expect("keep")
                .record()
                .id;
            repo.insert_or_touch(new_image_entry(&drop_hash, 4, 4, when))
                .expect("drop");
            repo.set_favorite(keep, true, when).expect("favorite");
            assert_eq!(repo.clear_non_favorites().expect("clear"), 1);
        }
        let repo = EntryRepository::new(db.connection_mut());
        let refs = repo.referenced_asset_refs().expect("refs");
        assert_eq!(refs.len(), 1);
        assert!(refs.contains(&format!("clipboard/{keep_hash}.png")));
    }

    #[test]
    fn parse_content_type_accepts_the_image_wire_value() {
        assert_eq!(parse_content_type("image"), Some(ContentType::Image));
        assert_eq!(ContentType::Image.as_str(), "image");
        assert!(parse_content_type("Image").is_none());
        assert!(parse_content_type("image/png").is_none());
    }

    #[test]
    fn textual_content_types_constant_excludes_image() {
        assert!(!TEXTUAL_CONTENT_TYPES.contains(&ContentType::Image));
    }

    // -----------------------------------------------------------------
    // `desktop-shell-layout` 10.8: regression coverage for the
    // image-after-restart scenario. The user-reported regression was
    // that previously saved images stopped showing in the rail after
    // a restart. The tests below pin every layer the renderer relies
    // on so the failure cannot drift back: the row, every payload
    // metadata column, the dedicated asset bridge, the unfiltered
    // `recent_entries` query, the filtered `entries_filtered` query
    // (both with and without a collection scope), the pin/unpin
    // round-trip, and the touch path on a duplicate hash.
    // -----------------------------------------------------------------

    /// Build a database, insert the supplied entries, drop the
    /// handle, reopen the database and return the refreshed
    /// connection. Mirrors the restart loop the desktop exercises.
    fn reopen_db(path: &std::path::Path) -> crate::Database {
        let db = crate::Database::open(path).expect("reopen");
        let mut db = db;
        db.run_migrations(&crate::builtin_migrations())
            .expect("migrate");
        db
    }

    #[test]
    fn image_row_survives_close_and_reopen() {
        // Regression: a persisted image row must keep every payload
        // metadata column after the SQLite handle is closed and a
        // fresh connection opens against the same file.
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("clipvault.db");
        let hash = "d".repeat(64);

        let id = {
            let mut db = crate::Database::open(&db_path).expect("open");
            db.run_migrations(&crate::builtin_migrations())
                .expect("migrate");
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_image_entry(
                &hash,
                320,
                240,
                datetime!(2026-01-02 03:04:05 UTC),
            ))
            .expect("insert image")
            .record()
            .id
        };

        let mut db = reopen_db(&db_path);
        let repo = EntryRepository::new(db.connection_mut());
        let record = repo
            .find_by_id(id)
            .expect("query")
            .expect("image row must survive a close/reopen cycle");
        assert_eq!(record.id, id);
        assert_eq!(record.content_type, ContentType::Image);
        assert_eq!(record.content, crate::entry::IMAGE_CONTENT_SENTINEL);
        assert_eq!(
            record.asset_ref.as_deref(),
            Some(format!("clipboard/{hash}.png").as_str()),
            "asset_ref must survive a restart",
        );
        assert_eq!(record.mime_type.as_deref(), Some("image/png"));
        assert_eq!(record.payload_width, Some(320));
        assert_eq!(record.payload_height, Some(240));
        assert!(record.is_renderable_image());
    }

    #[test]
    fn recent_entries_returns_image_rows_after_reopen() {
        // `recent_entries` is the unfiltered path the rail uses when
        // the user has not picked a collection. It must include image
        // rows after a restart and keep every payload column intact
        // so `hasRenderableImage` returns true on the frontend.
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("clipvault.db");
        let text_id;
        let image_id;
        {
            let mut db = crate::Database::open(&db_path).expect("open");
            db.run_migrations(&crate::builtin_migrations())
                .expect("migrate");
            let mut repo = EntryRepository::new(db.connection_mut());
            text_id = repo
                .insert_or_touch(new_entry(
                    "captured note",
                    datetime!(2026-01-02 03:04:05 UTC),
                ))
                .expect("text")
                .record()
                .id;
            image_id = repo
                .insert_or_touch(new_image_entry(
                    &"e".repeat(64),
                    8,
                    8,
                    datetime!(2026-01-02 03:04:05 UTC),
                ))
                .expect("image")
                .record()
                .id;
        }

        let mut db = reopen_db(&db_path);
        let repo = EntryRepository::new(db.connection_mut());
        let records = repo.recent(50).expect("recent");
        let ids: Vec<i64> = records.iter().map(|r| r.id).collect();
        assert!(ids.contains(&text_id), "text row must be returned");
        assert!(ids.contains(&image_id), "image row must be returned");
        let image = records
            .iter()
            .find(|r| r.id == image_id)
            .expect("image row in recent");
        assert!(image.is_renderable_image());
        assert_eq!(
            image.asset_ref.as_deref(),
            Some("clipboard/eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee.png"),
        );
        assert_eq!(image.mime_type.as_deref(), Some("image/png"));
        assert_eq!(image.payload_width, Some(8));
        assert_eq!(image.payload_height, Some(8));
    }

    #[test]
    fn entries_filtered_with_no_scope_returns_image_rows_after_reopen() {
        // `entries_filtered(None, &[])` is the canonical path the
        // rail uses when no collection is selected. After a restart
        // it must keep surfacing image rows; the legacy textual-only
        // helper used to drop them silently.
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("clipvault.db");
        let image_id;
        {
            let mut db = crate::Database::open(&db_path).expect("open");
            db.run_migrations(&crate::builtin_migrations())
                .expect("migrate");
            let mut repo = EntryRepository::new(db.connection_mut());
            image_id = repo
                .insert_or_touch(new_image_entry(
                    &"f".repeat(64),
                    16,
                    16,
                    datetime!(2026-01-02 03:04:05 UTC),
                ))
                .expect("image")
                .record()
                .id;
        }

        let mut db = reopen_db(&db_path);
        let repo = EntryRepository::new(db.connection_mut());
        let records = repo.entries_filtered(None, &[]).expect("filtered");
        assert!(
            records.iter().any(|r| r.id == image_id),
            "image row must surface in the unfiltered filtered query",
        );
        let image = records
            .iter()
            .find(|r| r.id == image_id)
            .expect("image in filtered");
        assert_eq!(
            image.asset_ref.as_deref(),
            Some("clipboard/ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff.png"),
        );
        assert!(image.is_renderable_image());
    }

    #[test]
    fn entries_filtered_by_collection_keeps_image_rows_after_reopen() {
        // A user (secondary) collection that contains an image must
        // still expose it after a restart; the textual-only helper
        // hid the row the moment a collection scope was supplied.
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("clipvault.db");
        let trabajo_id;
        let image_id;
        {
            let mut db = crate::Database::open(&db_path).expect("open");
            db.run_migrations(&crate::builtin_migrations())
                .expect("migrate");
            trabajo_id = {
                let mut repo = crate::OrganizationRepository::new(db.connection_mut());
                repo.create_user_collection("Trabajo", datetime!(2026-01-02 03:04:05 UTC))
                    .expect("create")
                    .id
            };
            image_id = {
                let mut repo = EntryRepository::new(db.connection_mut());
                repo.insert_or_touch(new_image_entry(
                    &"1".repeat(64),
                    4,
                    4,
                    datetime!(2026-01-02 03:04:05 UTC),
                ))
                .expect("image")
                .record()
                .id
            };
            let mut org = crate::OrganizationRepository::new(db.connection_mut());
            org.replace_entry_collections(
                image_id,
                &[trabajo_id],
                datetime!(2026-01-02 03:04:05 UTC),
            )
            .expect("attach image");
        }

        let mut db = reopen_db(&db_path);
        let repo = EntryRepository::new(db.connection_mut());
        let records = repo
            .entries_filtered(Some(trabajo_id), &[])
            .expect("filtered");
        assert!(
            records.iter().any(|r| r.id == image_id),
            "image row must surface inside the collection after restart",
        );
        let image = records
            .iter()
            .find(|r| r.id == image_id)
            .expect("image in filtered");
        assert!(image.is_renderable_image());
    }

    #[test]
    fn set_favorite_preserves_every_image_metadata_field_after_reopen() {
        // The pin/unpin round-trip must never strip asset_ref,
        // mime_type or the payload dimensions. The frontend routes
        // `applyPinUpdate` over the in-memory list but the persisted
        // row is the source of truth.
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("clipvault.db");
        let hash = "2".repeat(64);
        let id;
        {
            let mut db = crate::Database::open(&db_path).expect("open");
            db.run_migrations(&crate::builtin_migrations())
                .expect("migrate");
            let mut repo = EntryRepository::new(db.connection_mut());
            id = repo
                .insert_or_touch(new_image_entry(
                    &hash,
                    12,
                    12,
                    datetime!(2026-01-02 03:04:05 UTC),
                ))
                .expect("image")
                .record()
                .id;
        }

        let id = {
            let mut db = reopen_db(&db_path);
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.set_favorite(id, true, datetime!(2026-01-02 03:09:00 UTC))
                .expect("pin")
                .updated
                .expect("record")
                .id
        };

        let mut db = reopen_db(&db_path);
        let repo = EntryRepository::new(db.connection_mut());
        let record = repo.find_by_id(id).expect("query").expect("row");
        assert!(record.is_pinned, "pin state must persist");
        assert!(record.is_renderable_image());
        assert_eq!(
            record.asset_ref.as_deref(),
            Some(format!("clipboard/{hash}.png").as_str()),
            "asset_ref must survive pin/unpin",
        );
        assert_eq!(record.mime_type.as_deref(), Some("image/png"));
        assert_eq!(record.payload_width, Some(12));
        assert_eq!(record.payload_height, Some(12));
    }

    #[test]
    fn duplicate_touch_preserves_asset_metadata_after_reopen() {
        // Re-touching an image row (e.g. the user copied the same
        // image again) must NOT wipe the payload metadata or the
        // asset reference; the row should keep its `asset_ref`,
        // `mime_type` and dimensions across the touch.
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("clipvault.db");
        let hash = "3".repeat(64);
        let first_id;
        {
            let mut db = crate::Database::open(&db_path).expect("open");
            db.run_migrations(&crate::builtin_migrations())
                .expect("migrate");
            let mut repo = EntryRepository::new(db.connection_mut());
            first_id = repo
                .insert_or_touch(new_image_entry(
                    &hash,
                    16,
                    16,
                    datetime!(2026-01-02 03:04:05 UTC),
                ))
                .expect("first")
                .record()
                .id;
        }
        {
            let mut db = reopen_db(&db_path);
            let mut repo = EntryRepository::new(db.connection_mut());
            let outcome = repo
                .insert_or_touch(new_image_entry(
                    &hash,
                    16,
                    16,
                    datetime!(2026-01-02 03:09:00 UTC),
                ))
                .expect("touch");
            match outcome {
                EntryOutcome::Updated(record) => {
                    assert_eq!(record.id, first_id);
                    assert!(record.is_renderable_image());
                    assert_eq!(
                        record.asset_ref.as_deref(),
                        Some(format!("clipboard/{hash}.png").as_str()),
                    );
                }
                other => panic!("expected Updated, got {other:?}"),
            }
        }
    }

    // -----------------------------------------------------------------
    // `desktop-collection-card-polish` regression coverage for the rail
    // ordering / `created_at` immutability contract that the desktop
    // change relies on. The contract being pinned:
    //
    //   - `created_at` is the immutable capture datetime and must
    //     survive a close / reopen of SQLite;
    //   - the rail queries (`recent`, `entries_filtered`) order by
    //     `created_at DESC, id DESC` with no `is_pinned` priority
    //     because pin/unpin must not move a card out of its
    //     chronological slot;
    //   - every metadata mutation (pin, title, source_app, tag,
    //     collection) leaves `created_at` and the image payload
    //     metadata untouched;
    //   - the image payload size stored in `content_size` is the
    //     size of the original PNG bytes and survives all the
    //     mutations above.
    // -----------------------------------------------------------------

    #[test]
    fn insert_or_touch_persists_created_at_in_same_transaction() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-05-01 12:34:56 UTC);
        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("anchor", when))
                .expect("insert")
                .record()
                .id
        };

        let repo = EntryRepository::new(db.connection_mut());
        let record = repo.find_by_id(id).expect("query").expect("row");
        assert_eq!(record.created_at, format_timestamp(when));
    }

    #[test]
    fn created_at_survives_close_and_reopen() {
        // Regression for the `created_at` round-trip: the original
        // capture datetime must persist across a SQLite handle
        // cycle because the card's "Hace N…" label is derived from
        // it; a future regression that loses this column would tip
        // every card to "Recién capturado" on restart.
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("clipvault.db");
        let when = datetime!(2026-05-01 12:34:56 UTC);
        let id;
        {
            let mut db = crate::Database::open(&db_path).expect("open");
            db.run_migrations(&crate::builtin_migrations())
                .expect("migrate");
            let mut repo = EntryRepository::new(db.connection_mut());
            id = repo
                .insert_or_touch(new_entry("survive", when))
                .expect("insert")
                .record()
                .id;
        }

        let mut db = reopen_db(&db_path);
        let repo = EntryRepository::new(db.connection_mut());
        let record = repo
            .find_by_id(id)
            .expect("query")
            .expect("row must survive a close/reopen");
        assert_eq!(record.created_at, format_timestamp(when));
    }

    #[test]
    fn touch_preserves_original_created_at() {
        // When the user copies the same content again, the
        // repository must refresh `updated_at` / `last_seen_at` but
        // NEVER rewrite `created_at`. The card's "Hace N…" label
        // ties to the original capture datetime, so a regression
        // here would reset the counter every time the source
        // application re-pushed the same value.
        let (_dir, mut db) = open_temp_db();
        let t1 = datetime!(2026-05-01 12:00:00 UTC);
        let t2 = datetime!(2026-05-02 09:00:00 UTC);
        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("repeated", t1))
                .expect("insert")
                .record()
                .id
        };

        let outcome = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("repeated", t2))
                .expect("touch")
        };
        match outcome {
            EntryOutcome::Updated(record) => {
                assert_eq!(record.created_at, format_timestamp(t1));
                assert_eq!(record.updated_at, format_timestamp(t2));
                assert_eq!(record.last_seen_at, format_timestamp(t2));
            }
            other => panic!("expected Updated, got {other:?}"),
        }
        let _ = id;
    }

    #[test]
    fn pin_metadata_and_title_leave_created_at_untouched() {
        // All four metadata mutations documented in
        // `desktop-collection-card-polish` MUST leave `created_at`
        // untouched. A regression that rewrote the column would
        // make the "Hace N…" label jump every time the user pinned
        // / retitled / re-tagged / re-collected an entry.
        let (_dir, mut db) = open_temp_db();
        let captured_at = datetime!(2026-05-01 12:00:00 UTC);
        let mut when = datetime!(2026-05-02 09:00:00 UTC);
        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("unchanged", captured_at))
                .expect("insert")
                .record()
                .id
        };

        // pin
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.set_favorite(id, true, when).expect("pin");
        }
        when += time::Duration::minutes(5);
        // title
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.set_title(id, Some("Custom"), when).expect("title");
        }
        when += time::Duration::minutes(5);
        // source-app metadata
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.set_source_app_metadata(
                id,
                Some("Terminal"),
                Some("application-icons/com.apple.terminal.png"),
                when,
            )
            .expect("source app");
        }

        let repo = EntryRepository::new(db.connection_mut());
        let record = repo.find_by_id(id).expect("query").expect("row");
        assert_eq!(record.created_at, format_timestamp(captured_at));
        assert!(record.is_pinned);
        assert_eq!(record.title.as_deref(), Some("Custom"));
        assert_eq!(record.source_app_name.as_deref(), Some("Terminal"));
    }

    #[test]
    fn recent_orders_by_created_at_desc_and_id_desc() {
        let (_dir, mut db) = open_temp_db();
        let t1 = datetime!(2026-05-01 08:00:00 UTC);
        let t2 = datetime!(2026-05-01 09:00:00 UTC);
        let t3 = datetime!(2026-05-01 10:00:00 UTC);
        let pinned_id;
        let fresh_id;
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            // Insert in reverse chronological order so the rail
            // must reorder, not just echo the insert sequence.
            pinned_id = repo
                .insert_or_touch(new_entry("pinned", t1))
                .expect("pinned")
                .record()
                .id;
            fresh_id = repo
                .insert_or_touch(new_entry("fresh", t3))
                .expect("fresh")
                .record()
                .id;
            repo.set_favorite(pinned_id, true, t3).expect("pin");
            // Insert the middle row last so its row id is the
            // highest of the three.
            repo.insert_or_touch(new_entry("middle", t2))
                .expect("middle");
        }

        let repo = EntryRepository::new(db.connection_mut());
        let recent = repo.recent(10).expect("recent");
        let ids: Vec<i64> = recent.iter().map(|r| r.id).collect();
        let expected_first = fresh_id;
        assert_eq!(ids[0], expected_first, "newer created_at must lead");
        // Pinned row must NOT bubble above the freshly-captured
        // row just because it carries the favourite flag.
        assert!(
            ids.iter().position(|id| *id == pinned_id).unwrap()
                > ids.iter().position(|id| *id == fresh_id).unwrap(),
            "pin/unpin must not move a card ahead of a newer created_at",
        );
        let _ = pinned_id;
    }

    #[test]
    fn recent_uses_id_desc_as_a_deterministic_tie_break() {
        // Two captures with an identical `created_at` (to the
        // second) MUST order by `id DESC` so the rail is stable
        // across reloads even when the clock resolution is
        // coarse.
        let (_dir, mut db) = open_temp_db();
        let shared_ts = datetime!(2026-05-01 12:00:00 UTC);
        let ids;
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            ids = [
                repo.insert_or_touch(new_entry("first", shared_ts))
                    .expect("first")
                    .record()
                    .id,
                repo.insert_or_touch(new_entry("second", shared_ts))
                    .expect("second")
                    .record()
                    .id,
            ];
        }

        let repo = EntryRepository::new(db.connection_mut());
        let recent = repo.recent(10).expect("recent");
        let observed: Vec<i64> = recent.iter().map(|r| r.id).collect();
        let mut sorted = observed.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(observed, sorted, "tie break must be id DESC");
        // Both ids must appear; the relative order is id-DESC.
        assert!(observed.contains(&ids[0]));
        assert!(observed.contains(&ids[1]));
    }

    #[test]
    fn entries_filtered_orders_by_created_at_desc_and_id_desc() {
        // Mirrors `recent()` for the user-collection rail. Both
        // queries are required to agree so a switch from
        // "Historial" to "Trabajo" never reorders the cards.
        let (_dir, mut db) = open_temp_db();
        let t1 = datetime!(2026-05-01 08:00:00 UTC);
        let t2 = datetime!(2026-05-01 10:00:00 UTC);
        let work_id = {
            let mut repo = crate::OrganizationRepository::new(db.connection_mut());
            repo.create_user_collection("Trabajo", t1)
                .expect("create")
                .id
        };
        let first_id;
        let second_id;
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            first_id = repo
                .insert_or_touch(new_entry("first", t1))
                .expect("first")
                .record()
                .id;
            second_id = repo
                .insert_or_touch(new_entry("second", t2))
                .expect("second")
                .record()
                .id;
            let mut org = crate::OrganizationRepository::new(db.connection_mut());
            org.replace_entry_collections(first_id, &[work_id], t1)
                .expect("attach first");
            org.replace_entry_collections(second_id, &[work_id], t2)
                .expect("attach second");
        }

        let repo = EntryRepository::new(db.connection_mut());
        let filtered = repo.entries_filtered(Some(work_id), &[]).expect("filtered");
        let ids: Vec<i64> = filtered.iter().map(|r| r.id).collect();
        assert_eq!(ids[0], second_id, "newer created_at must lead");
        assert_eq!(ids[1], first_id);
    }

    #[test]
    fn content_size_survives_metadata_mutations_for_image_rows() {
        // The card uses `content_size` for the byte-shorthand under
        // the image thumbnail. Pin/unpin/rename MUST NOT alter the
        // payload size so the formatted B/KB/MB label stays
        // accurate. The reference value is the byte length the
        // capture pipeline wrote into `NewEntry::content_size`.
        let (_dir, mut db) = open_temp_db();
        let captured_at = datetime!(2026-05-01 12:00:00 UTC);
        let when = datetime!(2026-05-02 09:00:00 UTC);
        let hash = "a".repeat(64);

        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            let mut entry = new_image_entry(&hash, 320, 240, captured_at);
            entry.content_size = 4096;
            repo.insert_or_touch(entry).expect("insert").record().id
        };

        {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.set_favorite(id, true, when).expect("pin");
        }
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.set_title(id, Some("Custom image"), when)
                .expect("title");
        }

        let repo = EntryRepository::new(db.connection_mut());
        let record = repo.find_by_id(id).expect("query").expect("row");
        assert_eq!(
            record.content_size, 4096,
            "image bytes must survive metadata mutations"
        );
        assert!(record.is_renderable_image());
        assert!(record.is_pinned);
        assert_eq!(record.title.as_deref(), Some("Custom image"));
    }

    #[test]
    fn recent_entries_returns_images_after_reopen_in_pure_created_at_order() {
        // Combined regression for the rail surface: after a
        // close/reopen, the rail query returns the image row in
        // the expected created_at-DESC slot rather than dropping it
        // (the user-reported regression) or surfacing it ahead of a
        // newer text capture.
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("clipvault.db");
        let t_text = datetime!(2026-05-01 09:00:00 UTC);
        let t_image = datetime!(2026-05-01 11:00:00 UTC);
        let text_id;
        let image_id;
        {
            let mut db = crate::Database::open(&db_path).expect("open");
            db.run_migrations(&crate::builtin_migrations())
                .expect("migrate");
            let mut repo = EntryRepository::new(db.connection_mut());
            text_id = repo
                .insert_or_touch(new_entry("first", t_text))
                .expect("text")
                .record()
                .id;
            image_id = repo
                .insert_or_touch(new_image_entry(&"9".repeat(64), 8, 8, t_image))
                .expect("image")
                .record()
                .id;
        }

        let mut db = reopen_db(&db_path);
        let repo = EntryRepository::new(db.connection_mut());
        let records = repo.recent(50).expect("recent");
        let ids: Vec<i64> = records.iter().map(|r| r.id).collect();
        assert_eq!(ids[0], image_id, "newer image must lead");
        assert_eq!(ids[1], text_id, "older text follows");
        let image = records
            .iter()
            .find(|r| r.id == image_id)
            .expect("image in recent");
        assert!(image.is_renderable_image());
    }

    #[test]
    fn entries_filtered_returns_images_in_pure_created_at_order() {
        // Companion of the previous test for the
        // `recent_entries_with_filter` path. The filter must NOT
        // sort the rows a second time: the rail's chronological
        // ordering is preserved through the filter.
        let (_dir, mut db) = open_temp_db();
        let t_text = datetime!(2026-05-01 09:00:00 UTC);
        let t_image = datetime!(2026-05-01 11:00:00 UTC);
        let work_id = {
            let mut repo = crate::OrganizationRepository::new(db.connection_mut());
            repo.create_user_collection("Trabajo", t_text)
                .expect("create")
                .id
        };
        let text_id;
        let image_id;
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            text_id = repo
                .insert_or_touch(new_entry("text", t_text))
                .expect("text")
                .record()
                .id;
            image_id = repo
                .insert_or_touch(new_image_entry(&"f".repeat(64), 8, 8, t_image))
                .expect("image")
                .record()
                .id;
            let mut org = crate::OrganizationRepository::new(db.connection_mut());
            org.replace_entry_collections(text_id, &[work_id], t_text)
                .expect("attach text");
            org.replace_entry_collections(image_id, &[work_id], t_image)
                .expect("attach image");
        }

        let repo = EntryRepository::new(db.connection_mut());
        let filtered = repo.entries_filtered(Some(work_id), &[]).expect("filtered");
        let ids: Vec<i64> = filtered.iter().map(|r| r.id).collect();
        assert_eq!(ids[0], image_id);
        assert_eq!(ids[1], text_id);
    }

    #[test]
    fn clipvault_clipboard_asset_returns_png_bytes_after_reopen() {
        // Asset bridge round-trip contract. The dedicated command
        // is implemented elsewhere but the repository surface that
        // backs it (`asset_ref`, `content_size`) must keep its
        // bytes-identifying columns intact across the same restart
        // the rest of the rail survives.
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("clipvault.db");
        let hash = "c".repeat(64);
        let when = datetime!(2026-05-01 12:00:00 UTC);
        let id;
        let expected_ref: String;
        let expected_size: i64;
        {
            let mut db = crate::Database::open(&db_path).expect("open");
            db.run_migrations(&crate::builtin_migrations())
                .expect("migrate");
            let mut repo = EntryRepository::new(db.connection_mut());
            let mut entry = new_image_entry(&hash, 16, 16, when);
            entry.content_size = 2048;
            expected_size = entry.content_size;
            id = repo.insert_or_touch(entry).expect("insert").record().id;
            expected_ref = format!("clipboard/{hash}.png");
        }

        let mut db = reopen_db(&db_path);
        let repo = EntryRepository::new(db.connection_mut());
        let record = repo
            .find_by_id(id)
            .expect("query")
            .expect("image row must survive a close/reopen cycle");
        assert_eq!(record.asset_ref.as_deref(), Some(expected_ref.as_str()));
        assert_eq!(record.mime_type.as_deref(), Some("image/png"));
        assert_eq!(record.content_size, expected_size);
        assert!(record.is_renderable_image());
    }
}
