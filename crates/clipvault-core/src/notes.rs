//! Core service for local notes attached to history entries and collections.
//!
//! The service centralises note CRUD and ownership validation. It receives the
//! shared app context for persistence and never sends note bodies to logs,
//! events or list projections.

use thiserror::Error;

use clipvault_db::{NoteRecord, NoteRepository, NoteRepositoryError};

use crate::bootstrap::AppContext;

#[derive(Debug, Error)]
pub enum NotesServiceError {
    #[error("note persistence error: {0}")]
    Repository(#[from] NoteRepositoryError),
}

impl NotesServiceError {
    pub fn kind_str(&self) -> &'static str {
        match self {
            NotesServiceError::Repository(NoteRepositoryError::Sqlite(_)) => "persistence_error",
            NotesServiceError::Repository(NoteRepositoryError::EntryNotFound(_)) => {
                "entry_not_found"
            }
            NotesServiceError::Repository(NoteRepositoryError::CollectionNotFound(_)) => {
                "collection_not_found"
            }
        }
    }
}

/// Stateless facade over `NoteRepository`; the database and clock are supplied
/// by `AppContext` on each call so tests can use isolated applications.
#[derive(Debug, Clone, Copy, Default)]
pub struct NotesService;

impl NotesService {
    pub fn new() -> Self {
        Self
    }

    pub fn get_entry_note(
        &self,
        context: &AppContext,
        entry_id: i64,
    ) -> Result<Option<NoteRecord>, NotesServiceError> {
        let mut db = context.database().lock();
        Ok(NoteRepository::new(db.connection_mut()).get_entry_note(entry_id)?)
    }

    pub fn set_entry_note(
        &self,
        context: &AppContext,
        entry_id: i64,
        body: &str,
    ) -> Result<bool, NotesServiceError> {
        let now = context.clock().now();
        let mut db = context.database().lock();
        Ok(NoteRepository::new(db.connection_mut()).set_entry_note(entry_id, body, now)?)
    }

    pub fn entry_note_ids(&self, context: &AppContext) -> Result<Vec<i64>, NotesServiceError> {
        let mut db = context.database().lock();
        Ok(NoteRepository::new(db.connection_mut()).entry_note_ids()?)
    }

    pub fn get_collection_note(
        &self,
        context: &AppContext,
        collection_id: i64,
    ) -> Result<Option<NoteRecord>, NotesServiceError> {
        let mut db = context.database().lock();
        Ok(NoteRepository::new(db.connection_mut()).get_collection_note(collection_id)?)
    }

    pub fn set_collection_note(
        &self,
        context: &AppContext,
        collection_id: i64,
        body: &str,
    ) -> Result<bool, NotesServiceError> {
        let now = context.clock().now();
        let mut db = context.database().lock();
        Ok(
            NoteRepository::new(db.connection_mut()).set_collection_note(
                collection_id,
                body,
                now,
            )?,
        )
    }
}
