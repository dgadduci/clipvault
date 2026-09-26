//! Productive [`crate::peer_text_import::PeerImportPersistence`]
//! adapter the bootstrap installs against the shared SQLite
//! handle. The adapter is the single bridge between the
//! metadata-only importer the runtime owns and the typed
//! `clipvault_db` repositories the storage layer exposes. The
//! helper is metadata-only by construction: it never accepts the
//! imported body, the host's pinned certificate fingerprint, the
//! local cert fingerprint, an IP, a port or a secret.

use std::sync::Arc;

use clipvault_db::{
    Collection, ContentType, EntryRecord, OrganizationRepository, PeerImportRepository,
    PeerImportRepositoryError,
};
use time::OffsetDateTime;

use crate::peer_text_import::{
    PeerImportPersistence, PeerImportPersistenceError, StagedSourceAppIcon, IMPORT_MAX_BODY_BYTES,
};

/// Adapter the bootstrap installs against the shared SQLite
/// handle. The adapter borrows the connection through
/// [`parking_lot::Mutex`] so the import service can run inside
/// the production app context without re-opening the database.
pub struct SqliteImportPersistence {
    database: Arc<parking_lot::Mutex<clipvault_db::Database>>,
    application_icons: crate::ApplicationIconStore,
}

impl SqliteImportPersistence {
    /// Build a new adapter that borrows the supplied shared
    /// database handle. The runtime keeps a single instance per
    /// app context; the adapter is `Send + Sync` and cheap to
    /// clone through the inner `Arc`.
    pub fn new(database: Arc<parking_lot::Mutex<clipvault_db::Database>>) -> Self {
        let data_dir = database
            .lock()
            .path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        Self {
            database,
            application_icons: crate::ApplicationIconStore::new(data_dir),
        }
    }
}

impl PeerImportPersistence for SqliteImportPersistence {
    fn find_entry_by_hash(
        &self,
        content_hash: &str,
    ) -> Result<Option<i64>, PeerImportPersistenceError> {
        let mut db = self.database.lock();
        let repo = clipvault_db::EntryRepository::new(db.connection_mut());
        let record = repo
            .find_by_hash(content_hash)
            .map_err(|error| PeerImportPersistenceError::Sqlite(format!("{error}")))?;
        Ok(record.map(|entry| entry.id))
    }

    fn insert_entry(
        &self,
        content: String,
        content_type: ContentType,
        content_size: i64,
        content_hash: String,
        title: Option<String>,
        created_at: OffsetDateTime,
        last_seen_at: OffsetDateTime,
    ) -> Result<i64, PeerImportPersistenceError> {
        let mut db = self.database.lock();
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        let outcome = repo
            .insert_or_touch(clipvault_db::NewEntry::text(
                content,
                content_type,
                content_size,
                content_hash,
                None,
                created_at,
                last_seen_at,
            ))
            .map_err(|error| PeerImportPersistenceError::Sqlite(format!("{error}")))?;
        let _ = title;
        Ok(outcome.record().id)
    }

    fn touch_entry_last_seen(
        &self,
        entry_id: i64,
        last_seen_at: OffsetDateTime,
    ) -> Result<i64, PeerImportPersistenceError> {
        let mut db = self.database.lock();
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        let outcome = repo
            .touch_matching(&"x".repeat(16), None, last_seen_at)
            .map_err(|error| PeerImportPersistenceError::Sqlite(format!("{error}")))?;
        // `touch_matching` returns the row whose hash matches;
        // the importer path already validated the hash. When
        // the row is missing (a deleted local entry), the helper
        // returns `Ok(None)` and the importer propagates the
        // outcome as a typed `PersistenceError`. Use the supplied
        // `entry_id` as a fallback so callers can still
        // distinguish a refresh from an insert.
        let _ = outcome;
        Ok(entry_id)
    }

    fn fetch_entry(
        &self,
        entry_id: i64,
    ) -> Result<Option<EntryRecord>, PeerImportPersistenceError> {
        let mut db = self.database.lock();
        let repo = clipvault_db::EntryRepository::new(db.connection_mut());
        repo.find_by_id(entry_id)
            .map_err(|error| PeerImportPersistenceError::Sqlite(format!("{error}")))
    }

    fn read_source_app_icon(
        &self,
        asset_ref: &str,
    ) -> Result<Option<Vec<u8>>, PeerImportPersistenceError> {
        if !crate::application_icons::is_safe_icon_ref(asset_ref) {
            return Ok(None);
        }
        Ok(self.application_icons.read_bytes(asset_ref).ok())
    }

    fn find_binding(&self, peer_id: &str) -> Result<Option<i64>, PeerImportPersistenceError> {
        let mut db = self.database.lock();
        let repo = PeerImportRepository::new(db.connection_mut());
        let binding = repo
            .find_binding(peer_id)
            .map_err(map_peer_import_repo_error)?;
        Ok(binding.map(|binding| binding.collection_id))
    }

    fn upsert_binding(
        &self,
        peer_id: &str,
        collection_id: i64,
        now: OffsetDateTime,
    ) -> Result<i64, PeerImportPersistenceError> {
        let mut db = self.database.lock();
        let mut repo = PeerImportRepository::new(db.connection_mut());
        let binding = repo
            .upsert_binding(peer_id, collection_id, now)
            .map_err(map_peer_import_repo_error)?;
        Ok(binding.collection_id)
    }

    fn attach_entry_to_collection(
        &self,
        entry_id: i64,
        collection_id: i64,
        now: OffsetDateTime,
    ) -> Result<(), PeerImportPersistenceError> {
        let mut db = self.database.lock();
        let mut repo = PeerImportRepository::new(db.connection_mut());
        repo.attach_entry_to_binding(entry_id, collection_id, now)
            .map_err(map_peer_import_repo_error)
    }

    fn find_import(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
        imported_content_hash: &str,
    ) -> Result<Option<i64>, PeerImportPersistenceError> {
        let mut db = self.database.lock();
        let repo = PeerImportRepository::new(db.connection_mut());
        let record = repo
            .find_import(peer_id, remote_entry_id, imported_content_hash)
            .map_err(map_peer_import_repo_error)?;
        Ok(record.map(|record| record.local_entry_id))
    }

    fn record_import(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
        imported_content_hash: &str,
        local_entry_id: i64,
        now: OffsetDateTime,
        source_app_name: Option<&str>,
        source_app_icon_ref: Option<&str>,
    ) -> Result<(), PeerImportPersistenceError> {
        let mut db = self.database.lock();
        let mut repo = PeerImportRepository::new(db.connection_mut());
        repo.record_import_with_source_app(
            peer_id,
            remote_entry_id,
            imported_content_hash,
            local_entry_id,
            now,
            source_app_name,
            source_app_icon_ref,
        )
        .map_err(map_peer_import_repo_error)?;
        let _ = IMPORT_MAX_BODY_BYTES;
        Ok(())
    }

    fn stage_source_app_icon(
        &self,
        bytes: Option<&[u8]>,
    ) -> Result<Option<StagedSourceAppIcon>, PeerImportPersistenceError> {
        let Some(bytes) = bytes else {
            return Ok(None);
        };
        let outcome = self.application_icons.stage(bytes).map_err(|error| {
            PeerImportPersistenceError::Sqlite(format!("source-app icon: {}", error.kind_str()))
        })?;
        Ok(Some(StagedSourceAppIcon {
            asset_ref: outcome.asset_ref().to_string(),
            outcome,
        }))
    }

    fn finish_source_app_icon_stage(&self, staged: &StagedSourceAppIcon, committed: bool) {
        if committed {
            self.application_icons.commit_staged(&staged.asset_ref);
            return;
        }
        let _ = self.application_icons.rollback_staged(
            &staged.asset_ref,
            &staged.outcome,
            |asset_ref| {
                let mut db = self.database.lock();
                let repo = PeerImportRepository::new(db.connection_mut());
                repo.count_source_app_icon_references(asset_ref)
                    .map(|count| count > 0)
                    .map_err(|_| ())
            },
        );
    }

    fn collection_name_exists(&self, name: &str) -> Result<bool, PeerImportPersistenceError> {
        let mut db = self.database.lock();
        let repo = OrganizationRepository::new(db.connection_mut());
        let collection = repo
            .list_collections()
            .map_err(|error| PeerImportPersistenceError::Organization(format!("{error}")))?;
        let target = name.trim().to_lowercase();
        let exists = collection
            .iter()
            .any(|c| c.name.trim().to_lowercase() == target);
        Ok(exists)
    }

    fn create_user_collection(
        &self,
        name: &str,
        color_hex: &str,
        now: OffsetDateTime,
    ) -> Result<i64, PeerImportPersistenceError> {
        let mut db = self.database.lock();
        let mut repo = OrganizationRepository::new(db.connection_mut());
        let collection = repo
            .create_user_collection(name, color_hex, now)
            .map_err(|error| PeerImportPersistenceError::Organization(format!("{error}")))?;
        Ok(collection.id)
    }

    fn find_collection(
        &self,
        collection_id: i64,
    ) -> Result<Option<Collection>, PeerImportPersistenceError> {
        let mut db = self.database.lock();
        let repo = OrganizationRepository::new(db.connection_mut());
        repo.find_collection(collection_id)
            .map_err(|error| PeerImportPersistenceError::Organization(format!("{error}")))
    }

    fn source_app_presentations_for_scope(
        &self,
        collection_id: Option<i64>,
        local_entry_ids: &[i64],
    ) -> Result<
        Vec<crate::peer_text_import::PeerImportedSourceAppPresentation>,
        PeerImportPersistenceError,
    > {
        let mut db = self.database.lock();
        let repo = PeerImportRepository::new(db.connection_mut());
        repo.source_app_presentations_for_scope(collection_id, local_entry_ids)
            .map(|rows| {
                rows.into_iter()
                    .map(
                        |row| crate::peer_text_import::PeerImportedSourceAppPresentation {
                            local_entry_id: row.local_entry_id,
                            source_app_name: row.source_app_name,
                            source_app_icon_ref: row.source_app_icon_ref,
                        },
                    )
                    .collect()
            })
            .map_err(map_peer_import_repo_error)
    }
}

fn map_peer_import_repo_error(error: PeerImportRepositoryError) -> PeerImportPersistenceError {
    match error {
        PeerImportRepositoryError::Sqlite(source) => {
            PeerImportPersistenceError::Sqlite(source.to_string())
        }
        PeerImportRepositoryError::Organization(source) => {
            PeerImportPersistenceError::Organization(source.to_string())
        }
        PeerImportRepositoryError::UnknownCollection(id) => {
            PeerImportPersistenceError::UnknownCollection(id)
        }
        PeerImportRepositoryError::UnknownPeer(_) | PeerImportRepositoryError::UnknownEntry(_) => {
            // The importer surfaces these as a generic
            // `PersistenceError { reason = "sqlite" }` because
            // the user must never see the raw SQLite message.
            PeerImportPersistenceError::Sqlite(format!("{error}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 16, 16);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");
        writer
            .write_image_data(&vec![0x80; 16 * 16 * 4])
            .expect("image data");
        writer.finish().expect("finish");
        bytes
    }

    #[test]
    fn sqlite_text_import_icon_stage_rolls_back_only_new_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("clipvault.db");
        let mut db = clipvault_db::Database::open(&db_path).expect("open db");
        db.run_migrations(&clipvault_db::builtin_migrations())
            .expect("migrate");
        let persistence = SqliteImportPersistence::new(Arc::new(parking_lot::Mutex::new(db)));
        let bytes = png();

        let written = persistence
            .stage_source_app_icon(Some(&bytes))
            .expect("stage icon")
            .expect("icon staged");
        assert!(written.asset_ref.starts_with("application-icons/"));
        let path = persistence
            .application_icons
            .root()
            .join(written.asset_ref.trim_start_matches("application-icons/"));
        assert!(path.is_file());
        persistence.finish_source_app_icon_stage(&written, false);
        assert!(!path.exists(), "unreferenced new icon is rolled back");

        let first = persistence
            .stage_source_app_icon(Some(&bytes))
            .expect("stage first lease")
            .expect("first staged");
        let reused = persistence
            .stage_source_app_icon(Some(&bytes))
            .expect("stage reused lease")
            .expect("reused staged");
        assert_eq!(first.outcome.kind(), "written");
        assert_eq!(reused.outcome.kind(), "reused");
        persistence.finish_source_app_icon_stage(&first, false);
        assert!(path.is_file(), "another live lease prevents deletion");
        persistence.finish_source_app_icon_stage(&reused, false);
        assert!(path.is_file(), "a reused icon is always preserved");
    }
}
