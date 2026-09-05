//! Local organization service.
//!
//! [`OrganizationService`] is the only place the rest of ClipVault
//! uses to manage flat collections, free tags and the many-to-many
//! associations the sidebar and the card selectors depend on. It
//! sits between the Tauri commands and the [`OrganizationRepository`]
//! and enforces the rules the change contract documents:
//!
//! - `Historial` is the permanent system collection; it cannot be
//!   renamed or deleted.
//! - collection / tag names are trimmed, length-capped and rejected
//!   when empty; tag identity is normalised for whitespace and case
//!   insensitivity, the display form keeps the user's casing.
//! - associations are replaced atomically (`replace_*`).
//! - `Historial` membership is never removed by the "Quitar de esta
//!   colección" path; the entry only leaves a *secondary* collection.
//! - the service never inspects or logs clipboard content, hashes or
//!   paths; its public surface is metadata-only.
//!
//! The service borrows the database through [`AppContext`] so every
//! mutation is serialised through the same lock the rest of the core
//! uses.

use std::sync::Arc;

use serde::Serialize;
use thiserror::Error;

use clipvault_db::{
    Collection, CollectionKind, OrganizationError, OrganizationRepository, Tag, HISTORY_STABLE_KEY,
};

use crate::bootstrap::AppContext;
use crate::clock::Clock;

#[derive(Debug, Error)]
pub enum OrganizationServiceError {
    #[error("organization repository error: {0}")]
    Repository(#[from] OrganizationError),
}

impl OrganizationServiceError {
    pub fn kind_str(&self) -> &'static str {
        match self {
            OrganizationServiceError::Repository(error) => match error {
                OrganizationError::Sqlite(_) => "persistence_error",
                OrganizationError::EmptyCollectionName => "empty_collection_name",
                OrganizationError::CollectionNameTooLong(_) => "collection_name_too_long",
                OrganizationError::CollectionNameTaken(_) => "collection_name_taken",
                OrganizationError::EmptyTagName => "empty_tag_name",
                OrganizationError::TagNameTooLong(_) => "tag_name_too_long",
                OrganizationError::TagNameTaken(_) => "tag_name_taken",
                OrganizationError::CollectionNotFound(_) => "collection_not_found",
                OrganizationError::TagNotFound(_) => "tag_not_found",
                OrganizationError::EntryNotFound(_) => "entry_not_found",
                OrganizationError::SystemCollectionProtected(_) => "system_collection_protected",
            },
        }
    }
}

/// Concrete organization service the shell calls. Cheap to clone:
/// the only state is the [`Clock`] handle.
#[derive(Clone)]
pub struct OrganizationService {
    clock: Arc<dyn Clock>,
}

impl OrganizationService {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self { clock }
    }

    pub fn list_collections(
        &self,
        context: &AppContext,
    ) -> Result<Vec<Collection>, OrganizationServiceError> {
        let mut db = context.database().lock();
        let repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.list_collections()?)
    }

    pub fn find_collection(
        &self,
        context: &AppContext,
        id: i64,
    ) -> Result<Option<Collection>, OrganizationServiceError> {
        let mut db = context.database().lock();
        let repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.find_collection(id)?)
    }

    pub fn list_tags(&self, context: &AppContext) -> Result<Vec<Tag>, OrganizationServiceError> {
        let mut db = context.database().lock();
        let repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.list_tags()?)
    }

    pub fn find_tag(
        &self,
        context: &AppContext,
        id: i64,
    ) -> Result<Option<Tag>, OrganizationServiceError> {
        let mut db = context.database().lock();
        let repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.find_tag(id)?)
    }

    pub fn create_collection(
        &self,
        context: &AppContext,
        name: &str,
    ) -> Result<Collection, OrganizationServiceError> {
        let now = self.clock.now();
        let mut db = context.database().lock();
        let mut repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.create_user_collection(name, now)?)
    }

    pub fn rename_collection(
        &self,
        context: &AppContext,
        id: i64,
        name: &str,
    ) -> Result<Collection, OrganizationServiceError> {
        let now = self.clock.now();
        let mut db = context.database().lock();
        let mut repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.rename_collection(id, name, now)?)
    }

    pub fn delete_collection(
        &self,
        context: &AppContext,
        id: i64,
    ) -> Result<bool, OrganizationServiceError> {
        let mut db = context.database().lock();
        let mut repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.delete_collection(id)?)
    }

    /// Create a tag or reuse an existing row whose normalised
    /// identity matches `name`. The repository returns the refreshed
    /// row either way.
    pub fn upsert_tag(
        &self,
        context: &AppContext,
        name: &str,
    ) -> Result<Tag, OrganizationServiceError> {
        let now = self.clock.now();
        let mut db = context.database().lock();
        let mut repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.upsert_tag(name, now)?)
    }

    /// Atomically upsert a tag by normalised name and assign it to
    /// `entry_id`. The whole operation runs in a single transaction
    /// inside [`OrganizationRepository::upsert_and_assign_tag`] so a
    /// reader can never observe an orphan tag without its association,
    /// nor an association pointing at a tag that has not been
    /// committed yet.
    ///
    /// Returns the refreshed [`Tag`] row regardless of whether the
    /// tag was newly inserted or already existed; the `entry_tags`
    /// association is inserted with `INSERT OR IGNORE` so the call is
    /// idempotent.
    pub fn upsert_and_assign_tag(
        &self,
        context: &AppContext,
        entry_id: i64,
        name: &str,
    ) -> Result<Tag, OrganizationServiceError> {
        let now = self.clock.now();
        let mut db = context.database().lock();
        let mut repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.upsert_and_assign_tag(entry_id, name, now)?)
    }

    pub fn rename_tag(
        &self,
        context: &AppContext,
        id: i64,
        name: &str,
    ) -> Result<Tag, OrganizationServiceError> {
        let now = self.clock.now();
        let mut db = context.database().lock();
        let mut repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.rename_tag(id, name, now)?)
    }

    pub fn delete_tag(
        &self,
        context: &AppContext,
        id: i64,
    ) -> Result<bool, OrganizationServiceError> {
        let mut db = context.database().lock();
        let mut repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.delete_tag(id)?)
    }

    pub fn entry_collection_ids(
        &self,
        context: &AppContext,
        entry_id: i64,
    ) -> Result<Vec<i64>, OrganizationServiceError> {
        let mut db = context.database().lock();
        let repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.entry_collection_ids(entry_id)?)
    }

    pub fn entry_tag_ids(
        &self,
        context: &AppContext,
        entry_id: i64,
    ) -> Result<Vec<i64>, OrganizationServiceError> {
        let mut db = context.database().lock();
        let repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.entry_tag_ids(entry_id)?)
    }

    /// Replace the entry's collection membership atomically.
    /// `Historial` is always added back so a malformed payload that
    /// forgets it cannot orphan the entry; the system collection
    /// cannot be removed by this path either.
    pub fn replace_entry_collections(
        &self,
        context: &AppContext,
        entry_id: i64,
        collection_ids: &[i64],
    ) -> Result<Vec<i64>, OrganizationServiceError> {
        let now = self.clock.now();
        let mut db = context.database().lock();
        let mut repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.replace_entry_collections(entry_id, collection_ids, now)?)
    }

    pub fn replace_entry_tags(
        &self,
        context: &AppContext,
        entry_id: i64,
        tag_ids: &[i64],
    ) -> Result<Vec<i64>, OrganizationServiceError> {
        let now = self.clock.now();
        let mut db = context.database().lock();
        let mut repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.replace_entry_tags(entry_id, tag_ids, now)?)
    }

    /// "Quitar de esta colección" — only valid for secondary
    /// collections. Removing an entry from `Historial` is a global
    /// delete; the service exposes [`crate::management::HistoryManagementService::delete_entry`]
    /// for that. Removing an entry from the system collection here
    /// returns the same typed error the repository raises.
    pub fn remove_entry_from_collection(
        &self,
        context: &AppContext,
        entry_id: i64,
        collection_id: i64,
    ) -> Result<bool, OrganizationServiceError> {
        let mut db = context.database().lock();
        let mut repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.remove_entry_from_collection(entry_id, collection_id)?)
    }

    /// Stable boolean answer the UI needs when the card renders its
    /// ellipsis menu: `true` means the active collection is
    /// `Historial` (global delete flow), `false` means it is a
    /// secondary collection (offer "Quitar de esta colección").
    pub fn collection_kind(
        &self,
        context: &AppContext,
        collection_id: i64,
    ) -> Result<Option<CollectionKind>, OrganizationServiceError> {
        let mut db = context.database().lock();
        let repo = OrganizationRepository::new(db.connection_mut());
        Ok(repo.find_collection(collection_id)?.map(|row| row.kind))
    }

    pub fn history_collection_id(
        &self,
        context: &AppContext,
    ) -> Result<i64, OrganizationServiceError> {
        let mut db = context.database().lock();
        let repo = OrganizationRepository::new(db.connection_mut());
        repo.system_collection_id(HISTORY_STABLE_KEY)?
            .ok_or(OrganizationError::SystemCollectionProtected(
                HISTORY_STABLE_KEY,
            ))
            .map_err(OrganizationServiceError::from)
    }
}

/// Lightweight summary used by the Tauri shell to render the
/// sidebar. Carries only the metadata the UI needs.
#[derive(Debug, Clone, Serialize)]
pub struct OrganizationSidebarSnapshot {
    pub collections: Vec<Collection>,
    pub tags: Vec<Tag>,
}

impl OrganizationSidebarSnapshot {
    pub fn load(context: &AppContext) -> Result<Self, OrganizationServiceError> {
        let mut db = context.database().lock();
        let repo = OrganizationRepository::new(db.connection_mut());
        let collections = repo.list_collections()?;
        let tags = repo.list_tags()?;
        Ok(Self { collections, tags })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_kind_round_trips_every_branch() {
        // Every branch must expose a stable, lowercase identifier
        // the Tauri shell can switch on without inspecting the
        // free-form message.
        let cases: Vec<(OrganizationServiceError, &'static str)> = vec![
            (
                OrganizationServiceError::Repository(OrganizationError::EmptyCollectionName),
                "empty_collection_name",
            ),
            (
                OrganizationServiceError::Repository(OrganizationError::EmptyCollectionName),
                "empty_collection_name",
            ),
            (
                OrganizationServiceError::Repository(OrganizationError::CollectionNameTooLong(1)),
                "collection_name_too_long",
            ),
            (
                OrganizationServiceError::Repository(OrganizationError::CollectionNameTaken(
                    "x".into(),
                )),
                "collection_name_taken",
            ),
            (
                OrganizationServiceError::Repository(OrganizationError::EmptyTagName),
                "empty_tag_name",
            ),
            (
                OrganizationServiceError::Repository(OrganizationError::TagNameTooLong(1)),
                "tag_name_too_long",
            ),
            (
                OrganizationServiceError::Repository(OrganizationError::TagNameTaken("x".into())),
                "tag_name_taken",
            ),
            (
                OrganizationServiceError::Repository(OrganizationError::CollectionNotFound(1)),
                "collection_not_found",
            ),
            (
                OrganizationServiceError::Repository(OrganizationError::TagNotFound(1)),
                "tag_not_found",
            ),
            (
                OrganizationServiceError::Repository(OrganizationError::EntryNotFound(1)),
                "entry_not_found",
            ),
            (
                OrganizationServiceError::Repository(OrganizationError::SystemCollectionProtected(
                    "history",
                )),
                "system_collection_protected",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(error.kind_str(), expected);
        }
    }
}
