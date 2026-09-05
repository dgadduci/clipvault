//! Persistent model for the `tags-and-collections` capability.
//!
//! This module owns:
//!
//! - the `Collection` and `Tag` row layouts the Tauri shell
//!   serialises;
//! - the `OrganizationRepository` that the core service uses to
//!   manage flat collections, free tags and many-to-many
//!   associations;
//! - the validation rules (name normalisation, length, case
//!   insensitivity, deduplication) the core layer depends on so the
//!   shell can surface typed outcomes;
//! - the canonical `HISTORY_STABLE_KEY` and `HISTORY_DISPLAY_NAME`
//!   identifiers every other layer reads from.
//!
//! All SQL operations run against the connection the surrounding
//! `Database` owns; the repository borrows the connection so a
//! caller that needs multiple statements in the same transaction
//! can hold a `Transaction` and the repository methods that take
//! `&Connection` plus a manual scope.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// Stable identifier the seed migration uses for the permanent
/// `Historial` collection. Every other layer references `Historial`
/// by this key so renaming the display name never breaks
/// integration.
pub const HISTORY_STABLE_KEY: &str = "history";

/// Default display name for the system `Historial` collection.
/// Surfaced in the sidebar; users cannot rename it.
pub const HISTORY_DISPLAY_NAME: &str = "Historial";

/// Maximum number of characters a collection or tag display name may
/// hold after trimming. Shared by the core validation layer so the
/// GUI and the SQLite constraint agree.
pub const MAX_ORGANIZATION_NAME_CHARS: usize = 80;

#[derive(Debug, Error)]
pub enum OrganizationError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("collection name must not be empty")]
    EmptyCollectionName,
    #[error("collection name must not exceed {0} characters")]
    CollectionNameTooLong(usize),
    #[error("collection name '{0}' already exists")]
    CollectionNameTaken(String),
    #[error("tag name must not be empty")]
    EmptyTagName,
    #[error("tag name must not exceed {0} characters")]
    TagNameTooLong(usize),
    #[error("tag '{0}' already exists")]
    TagNameTaken(String),
    #[error("collection {0} does not exist")]
    CollectionNotFound(i64),
    #[error("tag {0} does not exist")]
    TagNotFound(i64),
    #[error("entry {0} does not exist")]
    EntryNotFound(i64),
    #[error("system collection '{0}' is protected and cannot be modified")]
    SystemCollectionProtected(&'static str),
}

/// Row stored in the `collections` table. The `stable_key` is `None`
/// for user-defined collections; system collections always carry the
/// stable key the migration seeded (`"history"`). `kind` mirrors the
/// `kind` column (`"system"` or `"user"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Collection {
    pub id: i64,
    pub stable_key: Option<String>,
    pub name: String,
    pub kind: CollectionKind,
    pub created_at: String,
    pub updated_at: String,
}

impl Collection {
    pub fn is_system(&self) -> bool {
        matches!(self.kind, CollectionKind::System)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionKind {
    System,
    User,
}

impl CollectionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CollectionKind::System => "system",
            CollectionKind::User => "user",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "system" => Some(CollectionKind::System),
            "user" => Some(CollectionKind::User),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tag {
    pub id: i64,
    /// Identity form: trimmed, internal whitespace collapsed and
    /// lowercased. Stable across case differences.
    pub normalized_name: String,
    /// Readable form, preserved as the user typed it (after trim).
    pub display_name: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Repository that owns every CRUD call the core layer makes against
/// the organisation tables. Cheap to construct; borrows the
/// connection so callers can decide whether to wrap the work in a
/// transaction.
pub struct OrganizationRepository<'a> {
    conn: &'a mut Connection,
}

impl<'a> OrganizationRepository<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    /// Resolve the system collection's primary key by stable key.
    /// Used by the capture pipeline to attach new entries to
    /// `Historial` and by every guard that protects the row from
    /// accidental rename / delete.
    pub fn system_collection_id(&self, stable_key: &str) -> Result<Option<i64>, OrganizationError> {
        let id = self
            .conn
            .query_row(
                "SELECT id FROM collections WHERE stable_key = ?1",
                params![stable_key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id)
    }

    /// List every collection sorted so the system `Historial` row
    /// always comes first and user collections follow in a
    /// deterministic case-insensitive order.
    pub fn list_collections(&self) -> Result<Vec<Collection>, OrganizationError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, stable_key, name, kind, created_at, updated_at
             FROM collections
             ORDER BY CASE WHEN kind = 'system' THEN 0 ELSE 1 END,
                      name COLLATE NOCASE ASC",
        )?;
        let rows = stmt.query_map([], row_to_collection)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Look up a collection by id. Returns `None` when the row does
    /// not exist (for example because it was deleted between two
    /// consecutive calls).
    pub fn find_collection(&self, id: i64) -> Result<Option<Collection>, OrganizationError> {
        let record = self
            .conn
            .query_row(
                "SELECT id, stable_key, name, kind, created_at, updated_at
                 FROM collections WHERE id = ?1",
                params![id],
                row_to_collection,
            )
            .optional()?;
        Ok(record)
    }

    /// Create a user collection. The repository refuses to insert
    /// the system stable key from this entry point and rejects
    /// duplicates case-insensitively.
    pub fn create_user_collection(
        &mut self,
        name: &str,
        now: OffsetDateTime,
    ) -> Result<Collection, OrganizationError> {
        let trimmed = validate_user_collection_name(name)?;
        if self.collection_name_exists(&trimmed)? {
            return Err(OrganizationError::CollectionNameTaken(trimmed));
        }
        let ts = format_timestamp(now);
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO collections (stable_key, name, kind, created_at, updated_at)
             VALUES (?1, ?2, 'user', ?3, ?3)",
            params![Option::<String>::None, trimmed, ts],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(Collection {
            id,
            stable_key: None,
            name: trimmed,
            kind: CollectionKind::User,
            created_at: ts.clone(),
            updated_at: ts,
        })
    }

    /// Rename a user collection. The system collection is rejected
    /// with a typed error so the UI can render the right copy
    /// without inspecting free-form messages.
    pub fn rename_collection(
        &mut self,
        id: i64,
        name: &str,
        now: OffsetDateTime,
    ) -> Result<Collection, OrganizationError> {
        let trimmed = validate_user_collection_name(name)?;
        let current = self
            .find_collection(id)?
            .ok_or(OrganizationError::CollectionNotFound(id))?;
        if current.is_system() {
            return Err(OrganizationError::SystemCollectionProtected(
                HISTORY_STABLE_KEY,
            ));
        }
        if current.name == trimmed {
            return Ok(current);
        }
        if self.collection_name_exists(&trimmed)? {
            return Err(OrganizationError::CollectionNameTaken(trimmed));
        }
        let ts = format_timestamp(now);
        let tx = self.conn.transaction()?;
        let updated = tx.execute(
            "UPDATE collections SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![trimmed, ts, id],
        )?;
        if updated == 0 {
            return Err(OrganizationError::CollectionNotFound(id));
        }
        tx.commit()?;
        Ok(Collection {
            name: trimmed,
            updated_at: ts,
            ..current
        })
    }

    /// Delete a user collection. The system collection cannot be
    /// deleted. Returns `true` when a row was removed. The foreign
    /// keys declared by `entry_collections` cascade the association
    /// rows so no orphan is left behind.
    pub fn delete_collection(&mut self, id: i64) -> Result<bool, OrganizationError> {
        let current = self
            .find_collection(id)?
            .ok_or(OrganizationError::CollectionNotFound(id))?;
        if current.is_system() {
            return Err(OrganizationError::SystemCollectionProtected(
                HISTORY_STABLE_KEY,
            ));
        }
        let tx = self.conn.transaction()?;
        let removed = tx.execute("DELETE FROM collections WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(removed > 0)
    }

    /// List every tag in case-insensitive display order. The
    /// `display_name` is preserved as the user typed it (after
    /// trimming) so two tags with identical normalised identity
    /// never coexist.
    pub fn list_tags(&self) -> Result<Vec<Tag>, OrganizationError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, normalized_name, display_name, created_at, updated_at
             FROM tags ORDER BY normalized_name ASC",
        )?;
        let rows = stmt.query_map([], row_to_tag)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn find_tag(&self, id: i64) -> Result<Option<Tag>, OrganizationError> {
        let record = self
            .conn
            .query_row(
                "SELECT id, normalized_name, display_name, created_at, updated_at
                 FROM tags WHERE id = ?1",
                params![id],
                row_to_tag,
            )
            .optional()?;
        Ok(record)
    }

    /// Create or reuse a tag whose normalised name equals `name`.
    /// The repository is idempotent: a normalised collision returns
    /// the existing row instead of creating a duplicate.
    pub fn upsert_tag(
        &mut self,
        name: &str,
        now: OffsetDateTime,
    ) -> Result<Tag, OrganizationError> {
        let normalised = normalise_tag_identity(name)?;
        let display = display_name_for_tag(name);
        if display.chars().count() > MAX_ORGANIZATION_NAME_CHARS {
            return Err(OrganizationError::TagNameTooLong(
                MAX_ORGANIZATION_NAME_CHARS,
            ));
        }
        let tx = self.conn.transaction()?;
        // Use a per-tag transaction so the INSERT-then-SELECT pair
        // shares the same snapshot.
        tx.execute(
            "INSERT OR IGNORE INTO tags
                (normalized_name, display_name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)",
            params![normalised, display, format_timestamp(now)],
        )?;
        let row = tx
            .query_row(
                "SELECT id, normalized_name, display_name, created_at, updated_at
                 FROM tags WHERE normalized_name = ?1",
                params![normalised],
                row_to_tag,
            )
            .optional()?;
        tx.commit()?;
        row.ok_or(OrganizationError::Sqlite(
            rusqlite::Error::QueryReturnedNoRows,
        ))
    }

    /// Atomically upsert a tag by normalised name **and** assign it to
    /// `entry_id`. Returns the refreshed [`Tag`] row regardless of
    /// whether the row was newly inserted or already existed.
    ///
    /// The whole operation runs inside a single transaction so a
    /// concurrent reader cannot observe an orphan tag without its
    /// association, nor an association pointing at a tag that is not
    /// yet committed. The tag row is created with the documented
    /// normalised identity and a readable display name; the
    /// `entry_tags` association is inserted with
    /// `INSERT OR IGNORE` so calling the method twice for the same
    /// pair is idempotent.
    ///
    /// The method refuses to attach a tag to an entry that does not
    /// exist (returning [`OrganizationError::EntryNotFound`]) so the
    /// shell can surface a typed error without inspecting free-form
    /// messages. The validation rules for the tag name
    /// (non-empty, length cap) are enforced by
    /// [`normalise_tag_identity`].
    pub fn upsert_and_assign_tag(
        &mut self,
        entry_id: i64,
        name: &str,
        now: OffsetDateTime,
    ) -> Result<Tag, OrganizationError> {
        let normalised = normalise_tag_identity(name)?;
        let display = display_name_for_tag(name);
        if display.chars().count() > MAX_ORGANIZATION_NAME_CHARS {
            return Err(OrganizationError::TagNameTooLong(
                MAX_ORGANIZATION_NAME_CHARS,
            ));
        }
        let tx = self.conn.transaction()?;
        // Verify the entry exists before touching the join table so a
        // stale entry id never produces an orphan association.
        ensure_entry_exists(&tx, entry_id)?;
        let ts = format_timestamp(now);
        tx.execute(
            "INSERT OR IGNORE INTO tags
                (normalized_name, display_name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)",
            params![normalised, display, ts],
        )?;
        let tag = tx
            .query_row(
                "SELECT id, normalized_name, display_name, created_at, updated_at
                 FROM tags WHERE normalized_name = ?1",
                params![normalised],
                row_to_tag,
            )
            .optional()?
            .ok_or(OrganizationError::Sqlite(
                rusqlite::Error::QueryReturnedNoRows,
            ))?;
        tx.execute(
            "INSERT OR IGNORE INTO entry_tags (entry_id, tag_id, created_at)
             VALUES (?1, ?2, ?3)",
            params![entry_id, tag.id, ts],
        )?;
        tx.commit()?;
        Ok(tag)
    }

    /// Rename a tag. Returns the refreshed row. The repository
    /// enforces the same case-insensitive uniqueness rule as
    /// [`upsert_tag`].
    pub fn rename_tag(
        &mut self,
        id: i64,
        name: &str,
        now: OffsetDateTime,
    ) -> Result<Tag, OrganizationError> {
        let normalised = normalise_tag_identity(name)?;
        let display = display_name_for_tag(name);
        if display.chars().count() > MAX_ORGANIZATION_NAME_CHARS {
            return Err(OrganizationError::TagNameTooLong(
                MAX_ORGANIZATION_NAME_CHARS,
            ));
        }
        let current = self
            .find_tag(id)?
            .ok_or(OrganizationError::TagNotFound(id))?;
        if current.normalized_name == normalised {
            return Ok(current);
        }
        let tx = self.conn.transaction()?;
        let exists: Option<i64> = tx
            .query_row(
                "SELECT id FROM tags WHERE normalized_name = ?1 AND id <> ?2",
                params![normalised, id],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_some() {
            return Err(OrganizationError::TagNameTaken(normalised));
        }
        let ts = format_timestamp(now);
        let updated = tx.execute(
            "UPDATE tags SET normalized_name = ?1, display_name = ?2, updated_at = ?3
             WHERE id = ?4",
            params![normalised, display, ts, id],
        )?;
        if updated == 0 {
            return Err(OrganizationError::TagNotFound(id));
        }
        tx.commit()?;
        Ok(Tag {
            normalized_name: normalised,
            display_name: display,
            updated_at: ts,
            ..current
        })
    }

    /// Delete a tag. Returns `true` when a row was removed.
    /// Association rows in `entry_tags` cascade.
    pub fn delete_tag(&mut self, id: i64) -> Result<bool, OrganizationError> {
        let tx = self.conn.transaction()?;
        let removed = tx.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(removed > 0)
    }

    /// Return every collection id an entry currently belongs to.
    pub fn entry_collection_ids(&self, entry_id: i64) -> Result<Vec<i64>, OrganizationError> {
        let mut stmt = self.conn.prepare(
            "SELECT collection_id FROM entry_collections WHERE entry_id = ?1
             ORDER BY collection_id ASC",
        )?;
        let rows = stmt.query_map(params![entry_id], |row| row.get(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Return every tag id an entry currently carries.
    pub fn entry_tag_ids(&self, entry_id: i64) -> Result<Vec<i64>, OrganizationError> {
        let mut stmt = self
            .conn
            .prepare("SELECT tag_id FROM entry_tags WHERE entry_id = ?1 ORDER BY tag_id ASC")?;
        let rows = stmt.query_map(params![entry_id], |row| row.get(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Atomically replace the set of collections an entry belongs
    /// to. The system `Historial` collection is always added back
    /// so a malformed payload that omits it cannot orphan the
    /// entry; entries always belong to `Historial`. The caller may
    /// include the system collection in `collection_ids` to make
    /// the intent explicit — duplicates are silently de-duplicated
    /// by the composite primary key.
    pub fn replace_entry_collections(
        &mut self,
        entry_id: i64,
        collection_ids: &[i64],
        now: OffsetDateTime,
    ) -> Result<Vec<i64>, OrganizationError> {
        let history_id = self
            .system_collection_id(HISTORY_STABLE_KEY)?
            .ok_or_else(|| OrganizationError::SystemCollectionProtected(HISTORY_STABLE_KEY))?;
        let tx = self.conn.transaction()?;
        ensure_entry_exists(&tx, entry_id)?;
        let ts = format_timestamp(now);
        tx.execute(
            "DELETE FROM entry_collections WHERE entry_id = ?1",
            params![entry_id],
        )?;
        let mut final_ids: Vec<i64> = Vec::with_capacity(collection_ids.len() + 1);
        let mut seen = std::collections::BTreeSet::new();
        for collection_id in collection_ids {
            if seen.insert(*collection_id) {
                final_ids.push(*collection_id);
            }
        }
        if seen.insert(history_id) {
            final_ids.push(history_id);
        }
        for collection_id in &final_ids {
            tx.execute(
                "INSERT OR IGNORE INTO entry_collections
                    (entry_id, collection_id, created_at)
                 VALUES (?1, ?2, ?3)",
                params![entry_id, collection_id, ts],
            )?;
        }
        tx.commit()?;
        Ok(final_ids)
    }

    /// Atomically replace the set of tags an entry carries.
    pub fn replace_entry_tags(
        &mut self,
        entry_id: i64,
        tag_ids: &[i64],
        now: OffsetDateTime,
    ) -> Result<Vec<i64>, OrganizationError> {
        let tx = self.conn.transaction()?;
        ensure_entry_exists(&tx, entry_id)?;
        let ts = format_timestamp(now);
        tx.execute(
            "DELETE FROM entry_tags WHERE entry_id = ?1",
            params![entry_id],
        )?;
        let mut final_ids: Vec<i64> = Vec::with_capacity(tag_ids.len());
        let mut seen = std::collections::BTreeSet::new();
        for tag_id in tag_ids {
            if seen.insert(*tag_id) {
                final_ids.push(*tag_id);
            }
        }
        for tag_id in &final_ids {
            tx.execute(
                "INSERT OR IGNORE INTO entry_tags (entry_id, tag_id, created_at)
                 VALUES (?1, ?2, ?3)",
                params![entry_id, tag_id, ts],
            )?;
        }
        tx.commit()?;
        Ok(final_ids)
    }

    /// Remove one entry from one collection. The system collection
    /// is rejected: `Historial` membership is the invariant the
    /// spec enforces and the core layer never offers an
    /// "unlink" entry point for it.
    pub fn remove_entry_from_collection(
        &mut self,
        entry_id: i64,
        collection_id: i64,
    ) -> Result<bool, OrganizationError> {
        let target = self
            .find_collection(collection_id)?
            .ok_or(OrganizationError::CollectionNotFound(collection_id))?;
        if target.is_system() {
            return Err(OrganizationError::SystemCollectionProtected(
                HISTORY_STABLE_KEY,
            ));
        }
        let tx = self.conn.transaction()?;
        let removed = tx.execute(
            "DELETE FROM entry_collections
             WHERE entry_id = ?1 AND collection_id = ?2",
            params![entry_id, collection_id],
        )?;
        tx.commit()?;
        Ok(removed > 0)
    }

    /// Look up every entry id currently associated with `collection_id`.
    /// Useful for tests and for filtering the rail when a user
    /// selects a collection.
    pub fn entry_ids_in_collection(
        &self,
        collection_id: i64,
    ) -> Result<Vec<i64>, OrganizationError> {
        let mut stmt = self.conn.prepare(
            "SELECT entry_id FROM entry_collections
             WHERE collection_id = ?1
             ORDER BY entry_id ASC",
        )?;
        let rows = stmt.query_map(params![collection_id], |row| row.get(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Return every entry id that carries **every** tag in `tag_ids`.
    /// When `tag_ids` is empty the method returns an empty vector:
    /// callers should short-circuit instead of querying with an
    /// empty filter, because the AND semantics would otherwise
    /// match every row.
    pub fn entry_ids_with_all_tags(&self, tag_ids: &[i64]) -> Result<Vec<i64>, OrganizationError> {
        if tag_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = std::iter::repeat_n("?", tag_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT entry_id FROM entry_tags
             WHERE tag_id IN ({placeholders})
             GROUP BY entry_id
             HAVING COUNT(DISTINCT tag_id) = ?{n}",
            n = tag_ids.len() + 1,
        );
        let mut params_dyn: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(tag_ids.len() + 1);
        for tag_id in tag_ids {
            params_dyn.push(tag_id);
        }
        let required = tag_ids.len() as i64;
        params_dyn.push(&required);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_dyn.as_slice(), |row| row.get(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Return every entry id that belongs to the system `Historial`
    /// collection. Used by the bootstrap / migration tests to
    /// validate the backfill completed.
    pub fn entry_ids_in_history(&self) -> Result<Vec<i64>, OrganizationError> {
        let mut stmt = self.conn.prepare(
            "SELECT ec.entry_id FROM entry_collections ec
             JOIN collections c ON c.id = ec.collection_id
             WHERE c.stable_key = ?1
             ORDER BY ec.entry_id ASC",
        )?;
        let rows = stmt.query_map(params![HISTORY_STABLE_KEY], |row| row.get(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Attach an entry to the system `Historial` collection. The
    /// capture pipeline calls this in the same transaction as
    /// `INSERT INTO clipboard_entries` so every new row satisfies
    /// the invariant atomically.
    pub fn attach_entry_to_history_in_tx(
        tx: &Transaction<'_>,
        entry_id: i64,
        now: OffsetDateTime,
    ) -> Result<(), OrganizationError> {
        let history_id: i64 = tx
            .query_row(
                "SELECT id FROM collections WHERE stable_key = ?1",
                params![HISTORY_STABLE_KEY],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(OrganizationError::SystemCollectionProtected(
                HISTORY_STABLE_KEY,
            ))?;
        tx.execute(
            "INSERT OR IGNORE INTO entry_collections
                (entry_id, collection_id, created_at)
             VALUES (?1, ?2, ?3)",
            params![entry_id, history_id, format_timestamp(now)],
        )?;
        Ok(())
    }

    fn collection_name_exists(&self, name: &str) -> Result<bool, OrganizationError> {
        let normalised = name.trim().to_lowercase();
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM collections WHERE LOWER(name) = ?1",
            params![normalised],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

fn row_to_collection(row: &rusqlite::Row<'_>) -> rusqlite::Result<Collection> {
    let kind_raw: String = row.get(3)?;
    let kind = CollectionKind::parse(&kind_raw).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(OrganizationError::Sqlite(rusqlite::Error::InvalidQuery)),
        )
    })?;
    Ok(Collection {
        id: row.get(0)?,
        stable_key: row.get(1)?,
        name: row.get(2)?,
        kind,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn row_to_tag(row: &rusqlite::Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: row.get(0)?,
        normalized_name: row.get(1)?,
        display_name: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn ensure_entry_exists(tx: &Transaction<'_>, entry_id: i64) -> Result<(), OrganizationError> {
    let exists: Option<i64> = tx
        .query_row(
            "SELECT id FROM clipboard_entries WHERE id = ?1",
            params![entry_id],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(OrganizationError::EntryNotFound(entry_id));
    }
    Ok(())
}

fn format_timestamp(ts: OffsetDateTime) -> String {
    ts.format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// Validate a user-supplied collection name. Trims surrounding
/// whitespace, rejects empty results, enforces the documented
/// character cap and returns the trimmed canonical form.
pub fn validate_user_collection_name(name: &str) -> Result<String, OrganizationError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(OrganizationError::EmptyCollectionName);
    }
    if trimmed.chars().count() > MAX_ORGANIZATION_NAME_CHARS {
        return Err(OrganizationError::CollectionNameTooLong(
            MAX_ORGANIZATION_NAME_CHARS,
        ));
    }
    Ok(trimmed.to_string())
}

/// Normalise a tag identity: trim, collapse internal whitespace and
/// lowercase the result. The display name is the trimmed form
/// without the case folding so the user keeps the casing they
/// typed.
pub fn normalise_tag_identity(name: &str) -> Result<String, OrganizationError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(OrganizationError::EmptyTagName);
    }
    let collapsed: String = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return Err(OrganizationError::EmptyTagName);
    }
    if collapsed.chars().count() > MAX_ORGANIZATION_NAME_CHARS {
        return Err(OrganizationError::TagNameTooLong(
            MAX_ORGANIZATION_NAME_CHARS,
        ));
    }
    Ok(collapsed.to_lowercase())
}

/// Display name for a tag: trim and collapse internal whitespace
/// for legibility, but preserve the user's casing.
pub fn display_name_for_tag(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_temp_db() -> (tempfile::TempDir, crate::Database) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = crate::Database::open(dir.path().join("clipvault.db")).expect("open");
        let mut db = db;
        db.run_migrations(&crate::builtin_migrations())
            .expect("migrate");
        (dir, db)
    }

    #[test]
    fn normalise_tag_identity_trims_and_lowercases() {
        let identity = normalise_tag_identity("  Work    Notes ").expect("ok");
        assert_eq!(identity, "work notes");
    }

    #[test]
    fn normalise_tag_identity_collapses_internal_whitespace() {
        let identity = normalise_tag_identity("\tProject\t\tAlpha \t Z").expect("ok");
        assert_eq!(identity, "project alpha z");
    }

    #[test]
    fn normalise_tag_identity_is_case_insensitive() {
        let a = normalise_tag_identity("Critical").expect("ok");
        let b = normalise_tag_identity("  CRITICAL  ").expect("ok");
        assert_eq!(a, b);
    }

    #[test]
    fn normalise_tag_identity_rejects_empty() {
        assert!(matches!(
            normalise_tag_identity("   "),
            Err(OrganizationError::EmptyTagName)
        ));
    }

    #[test]
    fn normalise_tag_identity_rejects_overlong() {
        let long = "a".repeat(MAX_ORGANIZATION_NAME_CHARS + 1);
        assert!(matches!(
            normalise_tag_identity(&long),
            Err(OrganizationError::TagNameTooLong(_))
        ));
    }

    #[test]
    fn validate_user_collection_name_trims_and_rejects_empty() {
        assert_eq!(
            validate_user_collection_name("  Trabajo  ").unwrap(),
            "Trabajo"
        );
        assert!(matches!(
            validate_user_collection_name("   "),
            Err(OrganizationError::EmptyCollectionName)
        ));
    }

    #[test]
    fn histo_collection_is_seeded_by_migration() {
        let (_dir, mut db) = open_temp_db();
        let repo = OrganizationRepository::new(db.connection_mut());
        let collections = repo.list_collections().expect("list");
        let history = collections
            .iter()
            .find(|c| c.stable_key.as_deref() == Some(HISTORY_STABLE_KEY))
            .expect("Historial row seeded");
        assert_eq!(history.name, HISTORY_DISPLAY_NAME);
        assert!(history.is_system());
        assert!(collections.first().expect("non-empty").is_system());
    }

    // -----------------------------------------------------------------
    // `tags-and-collections` regression coverage for the atomic
    // upsert + assign path the card menu uses when the user types a
    // new tag name. The regression the original change introduced —
    // "tags typed from the modal do not appear on the card and do
    // not persist across restarts" — was caused by the frontend
    // emitting synthetic names and skipping the assignment; this
    // suite pins the contract the new `upsert_and_assign_tag`
    // method must satisfy so the fix cannot drift back.
    // -----------------------------------------------------------------

    use crate::entry::{ContentType, NewEntry};
    use crate::entry_repository::EntryRepository;
    use time::macros::datetime;

    fn insert_text_entry(conn: &mut Connection, content: &str, when: OffsetDateTime) -> i64 {
        let new = NewEntry::text(
            content.to_string(),
            ContentType::Text,
            content.len() as i64,
            format!("hash::{content}"),
            Some("test-app".to_string()),
            when,
            when,
        );
        let mut repo = EntryRepository::new(conn);
        repo.insert_or_touch(new).expect("insert").record().id
    }

    #[test]
    fn upsert_and_assign_tag_creates_row_and_association_atomically() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let entry_id = insert_text_entry(db.connection_mut(), "captured", when);

        let mut repo = OrganizationRepository::new(db.connection_mut());
        let tag = repo
            .upsert_and_assign_tag(entry_id, "Critical", when)
            .expect("upsert+assign");

        assert_eq!(tag.normalized_name, "critical");
        assert_eq!(tag.display_name, "Critical");

        // The tag row exists.
        let tags = repo.list_tags().expect("list");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].id, tag.id);

        // The entry is associated with the tag.
        let assigned = repo.entry_tag_ids(entry_id).expect("entry_tag_ids");
        assert_eq!(assigned, vec![tag.id]);
    }

    #[test]
    fn upsert_and_assign_tag_is_idempotent_when_repeated() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let entry_id = insert_text_entry(db.connection_mut(), "captured", when);

        let mut repo = OrganizationRepository::new(db.connection_mut());
        let first = repo
            .upsert_and_assign_tag(entry_id, "Critical", when)
            .expect("first");
        let second = repo
            .upsert_and_assign_tag(entry_id, "  CRITICAL  ", when)
            .expect("second");

        assert_eq!(first.id, second.id);
        assert_eq!(first.normalized_name, second.normalized_name);

        // Only one tag row exists and the entry still carries exactly
        // one association — the second call must not create a
        // duplicate row or a second association.
        let tags = repo.list_tags().expect("list");
        assert_eq!(tags.len(), 1);
        let assigned = repo.entry_tag_ids(entry_id).expect("entry_tag_ids");
        assert_eq!(assigned, vec![first.id]);
    }

    #[test]
    fn upsert_and_assign_tag_rejects_missing_entry() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let mut repo = OrganizationRepository::new(db.connection_mut());
        let error = repo
            .upsert_and_assign_tag(9_999, "draft", when)
            .expect_err("missing entry");
        assert!(matches!(error, OrganizationError::EntryNotFound(9_999)));

        // The rejection must leave the tag table untouched: no orphan
        // tag is allowed to linger when the association step fails.
        let tags = repo.list_tags().expect("list");
        assert!(
            tags.is_empty(),
            "no orphan tag must survive a missing entry"
        );
    }

    #[test]
    fn upsert_and_assign_tag_rejects_empty_and_overlong_names() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let entry_id = insert_text_entry(db.connection_mut(), "captured", when);

        let mut repo = OrganizationRepository::new(db.connection_mut());
        assert!(matches!(
            repo.upsert_and_assign_tag(entry_id, "   ", when),
            Err(OrganizationError::EmptyTagName)
        ));
        let too_long = "x".repeat(MAX_ORGANIZATION_NAME_CHARS + 1);
        assert!(matches!(
            repo.upsert_and_assign_tag(entry_id, &too_long, when),
            Err(OrganizationError::TagNameTooLong(_))
        ));

        // Neither rejection may have produced a tag row.
        let tags = repo.list_tags().expect("list");
        assert!(tags.is_empty());
    }

    #[test]
    fn upsert_and_assign_tag_keeps_other_entries_untouched() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let target = insert_text_entry(db.connection_mut(), "target", when);
        let other = insert_text_entry(db.connection_mut(), "other", when);

        let mut repo = OrganizationRepository::new(db.connection_mut());
        let tag_id = repo
            .upsert_and_assign_tag(target, "draft", when)
            .expect("upsert+assign")
            .id;

        // The other entry must not gain the tag the target was just
        // assigned.
        let other_tags = repo.entry_tag_ids(other).expect("ids");
        assert!(
            other_tags.is_empty(),
            "other entry must not receive the tag"
        );
        let target_tags = repo.entry_tag_ids(target).expect("ids");
        assert_eq!(target_tags, vec![tag_id]);
    }
}
