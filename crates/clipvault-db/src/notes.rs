//! Local notes attached to history entries and collections.
//!
//! Notes are stored separately from clipboard content and are never included
//! in history projections. This repository exposes note bodies only through
//! explicit read operations and exposes note presence as identifiers for
//! metadata-only list refreshes.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Debug, Error)]
pub enum NoteRepositoryError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("entry {0} does not exist")]
    EntryNotFound(i64),
    #[error("collection {0} does not exist")]
    CollectionNotFound(i64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NoteRecord {
    pub body: String,
    pub updated_at: String,
}

pub struct NoteRepository<'a> {
    conn: &'a mut Connection,
}

impl<'a> NoteRepository<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get_entry_note(&self, entry_id: i64) -> Result<Option<NoteRecord>, NoteRepositoryError> {
        ensure_entry_exists(self.conn, entry_id)?;
        read_note(
            self.conn,
            "SELECT body, updated_at FROM entry_notes WHERE entry_id = ?1",
            entry_id,
        )
    }

    pub fn set_entry_note(
        &mut self,
        entry_id: i64,
        body: &str,
        now: OffsetDateTime,
    ) -> Result<bool, NoteRepositoryError> {
        let tx = self.conn.transaction()?;
        ensure_entry_exists(&tx, entry_id)?;
        let has_note = write_note(&tx, "entry_notes", "entry_id", entry_id, body, now)?;
        tx.commit()?;
        Ok(has_note)
    }

    pub fn entry_note_ids(&self) -> Result<Vec<i64>, NoteRepositoryError> {
        Ok(list_note_ids(
            self.conn,
            "SELECT entry_id FROM entry_notes ORDER BY entry_id",
        )?)
    }

    pub fn get_collection_note(
        &self,
        collection_id: i64,
    ) -> Result<Option<NoteRecord>, NoteRepositoryError> {
        ensure_collection_exists(self.conn, collection_id)?;
        read_note(
            self.conn,
            "SELECT body, updated_at FROM collection_notes WHERE collection_id = ?1",
            collection_id,
        )
    }

    pub fn set_collection_note(
        &mut self,
        collection_id: i64,
        body: &str,
        now: OffsetDateTime,
    ) -> Result<bool, NoteRepositoryError> {
        let tx = self.conn.transaction()?;
        ensure_collection_exists(&tx, collection_id)?;
        let has_note = write_note(
            &tx,
            "collection_notes",
            "collection_id",
            collection_id,
            body,
            now,
        )?;
        tx.commit()?;
        Ok(has_note)
    }
}

fn ensure_entry_exists(conn: &Connection, entry_id: i64) -> Result<(), NoteRepositoryError> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM clipboard_entries WHERE id = ?1)",
        params![entry_id],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(NoteRepositoryError::EntryNotFound(entry_id))
    }
}

fn ensure_collection_exists(
    conn: &Connection,
    collection_id: i64,
) -> Result<(), NoteRepositoryError> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM collections WHERE id = ?1)",
        params![collection_id],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(NoteRepositoryError::CollectionNotFound(collection_id))
    }
}

fn read_note(
    conn: &Connection,
    sql: &str,
    owner_id: i64,
) -> Result<Option<NoteRecord>, NoteRepositoryError> {
    Ok(conn
        .query_row(sql, params![owner_id], |row| {
            Ok(NoteRecord {
                body: row.get(0)?,
                updated_at: row.get(1)?,
            })
        })
        .optional()?)
}

fn write_note(
    tx: &Transaction<'_>,
    table: &str,
    owner_column: &str,
    owner_id: i64,
    body: &str,
    now: OffsetDateTime,
) -> Result<bool, rusqlite::Error> {
    if body.is_empty() {
        tx.execute(
            &format!("DELETE FROM {table} WHERE {owner_column} = ?1"),
            params![owner_id],
        )?;
        return Ok(false);
    }
    let timestamp = format_timestamp(now);
    tx.execute(
        &format!(
            "INSERT INTO {table} ({owner_column}, body, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT({owner_column}) DO UPDATE SET
               body = excluded.body,
               updated_at = excluded.updated_at"
        ),
        params![owner_id, body, timestamp],
    )?;
    Ok(true)
}

fn format_timestamp(now: OffsetDateTime) -> String {
    now.format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn list_note_ids(conn: &Connection, sql: &str) -> Result<Vec<i64>, rusqlite::Error> {
    let mut statement = conn.prepare(sql)?;
    let rows = statement.query_map([], |row| row.get(0))?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{builtin_migrations, Database, EntryRepository, NewEntry, OrganizationRepository};
    use time::macros::datetime;

    fn seeded() -> (tempfile::TempDir, Database, i64, i64, i64) {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut db = Database::open(dir.path().join("notes.db")).expect("database");
        db.run_migrations(&builtin_migrations()).expect("migrate");
        let now = datetime!(2025-01-01 00:00 UTC);
        let (entry_id, history_id, collection_id) = {
            let entry = {
                let mut repo = EntryRepository::new(db.connection_mut());
                repo.insert_or_touch(NewEntry::text(
                    "captured".to_string(),
                    crate::ContentType::Text,
                    8,
                    "entry-note-hash".to_string(),
                    None,
                    now,
                    now,
                ))
                .expect("entry")
                .record()
                .id
            };
            let mut org = OrganizationRepository::new(db.connection_mut());
            let history = org
                .system_collection_id(crate::HISTORY_STABLE_KEY)
                .expect("history query")
                .expect("history collection");
            let collection = org
                .create_user_collection("Notas", crate::HISTORY_DEFAULT_COLOR_HEX, now)
                .expect("collection")
                .id;
            (entry, history, collection)
        };
        (dir, db, entry_id, history_id, collection_id)
    }

    #[test]
    fn entry_note_round_trips_multiline_and_empty_removes_it() {
        let (_dir, mut db, entry_id, _, _) = seeded();
        let now = datetime!(2025-01-02 00:00 UTC);
        let mut repo = NoteRepository::new(db.connection_mut());
        assert!(repo
            .set_entry_note(entry_id, "first line\nsecond line", now)
            .expect("save note"));
        assert_eq!(repo.entry_note_ids().expect("note ids"), vec![entry_id]);
        assert_eq!(
            repo.get_entry_note(entry_id).expect("read note"),
            Some(NoteRecord {
                body: "first line\nsecond line".to_string(),
                updated_at: "2025-01-02T00:00:00Z".to_string(),
            })
        );
        assert!(!repo
            .set_entry_note(entry_id, "", datetime!(2025-01-03 00:00 UTC))
            .expect("remove note"));
        assert!(repo
            .get_entry_note(entry_id)
            .expect("read removed note")
            .is_none());
    }

    #[test]
    fn collection_note_round_trips_and_owner_deletion_cascades() {
        let (_dir, mut db, _, _, collection_id) = seeded();
        let now = datetime!(2025-01-02 00:00 UTC);
        {
            let mut repo = NoteRepository::new(db.connection_mut());
            assert!(repo
                .set_collection_note(collection_id, "collection\nnote", now)
                .expect("save collection note"));
        }
        let projected = OrganizationRepository::new(db.connection_mut())
            .find_collection(collection_id)
            .expect("find collection")
            .expect("collection exists");
        assert!(projected.has_note);
        assert_eq!(
            NoteRepository::new(db.connection_mut())
                .get_collection_note(collection_id)
                .expect("read collection note")
                .map(|note| note.body),
            Some("collection\nnote".to_string())
        );
        OrganizationRepository::new(db.connection_mut())
            .delete_collection(collection_id)
            .expect("delete collection");
        assert!(NoteRepository::new(db.connection_mut())
            .get_collection_note(collection_id)
            .is_err());
    }

    #[test]
    fn deleting_entry_cascades_its_note() {
        let (_dir, mut db, entry_id, _, _) = seeded();
        let mut notes = NoteRepository::new(db.connection_mut());
        notes
            .set_entry_note(
                entry_id,
                "local annotation",
                datetime!(2025-01-02 00:00 UTC),
            )
            .expect("save note");
        drop(notes);
        db.connection_mut()
            .execute(
                "DELETE FROM clipboard_entries WHERE id = ?1",
                params![entry_id],
            )
            .expect("delete entry");
        assert!(NoteRepository::new(db.connection_mut())
            .entry_note_ids()
            .expect("note ids")
            .is_empty());
    }
}
