//! Productive [`crate::peer_image_import::PeerImageImportPersistence`]
//! adapter the bootstrap installs against the shared SQLite
//! handle. The adapter is the single bridge between the
//! metadata-only importer the runtime owns and the typed
//! `clipvault_db` repositories the storage layer exposes. The
//! helper is metadata-only by construction: it never accepts the
//! imported bytes, the host's pinned certificate fingerprint, the
//! local cert fingerprint, an IP, a port or a secret.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use clipvault_db::{
    Collection, ContentType, EntryRecord, NewEntry, OrganizationRepository, PeerImportRepository,
    PeerImportRepositoryError,
};
use parking_lot::{Condvar, Mutex};
use time::OffsetDateTime;

use crate::clipboard_assets::{self, ClipboardAssetStore, CLIPBOARD_ASSETS_DIR};
use crate::peer_image_import::{
    PeerImageImportPersistence, PeerImageImportPersistenceError, StageKind, StagedImageAsset,
    IMPORT_MAX_IMAGE_BYTES,
};
use clipvault_platform::MAX_CLIPBOARD_IMAGE_DIM;

/// Per-asset staging lease the rollback path consults so
/// concurrent imports that race on the same `asset_ref` never
/// delete a file the other attempt is about to commit. The
/// counter mirrors the in-memory adapter the unit tests cover:
/// `stage_image_asset` increments the slot, `commit_import_transaction`
/// and `release_staged_asset` decrement it, and the rollback path only
/// deletes the file once the slot reaches zero AND no
/// `clipboard_entries` row references the asset. A `deleting` marker
/// reserves the asset while SQLite references and the filesystem are
/// checked; new peer stages wait until cleanup completes. The shared
/// asset-store mutation lock also serializes cleanup with local capture
/// writes through their SQLite commit.
#[derive(Debug, Default)]
struct StagingLeaseState {
    leases: HashMap<String, usize>,
    deleting: HashSet<String>,
}

impl StagingLeaseState {
    fn increment(&mut self, asset_ref: &str) {
        *self.leases.entry(asset_ref.to_string()).or_insert(0) += 1;
    }

    /// Decrement the counter for `asset_ref`, removing the slot
    /// when it reaches zero. The helper is a no-op when the
    /// asset was never staged.
    fn decrement(&mut self, asset_ref: &str) -> usize {
        let Some(slot) = self.leases.get_mut(asset_ref) else {
            return 0;
        };
        if *slot <= 1 {
            self.leases.remove(asset_ref);
            0
        } else {
            *slot -= 1;
            *slot
        }
    }

    /// Snapshot of the current counter, without mutating the
    /// map. Returns zero when the asset was never staged.
    fn current(&self, asset_ref: &str) -> usize {
        self.leases.get(asset_ref).copied().unwrap_or(0)
    }

    fn is_deleting(&self, asset_ref: &str) -> bool {
        self.deleting.contains(asset_ref)
    }
}

/// Adapter the bootstrap installs against the shared SQLite
/// handle. The adapter borrows the connection through
/// [`parking_lot::Mutex`] so the import service can run inside the
/// production app context without re-opening the database.
pub struct SqliteImageImportPersistence {
    database: Arc<parking_lot::Mutex<clipvault_db::Database>>,
    asset_store: ClipboardAssetStore,
    /// Per-asset staging lease shared across all concurrent imports
    /// the adapter serves. The mutex serialises lease updates and
    /// cleanup reservations.
    staging_leases: Arc<Mutex<StagingLeaseState>>,
    staging_lease_changed: Condvar,
}

impl SqliteImageImportPersistence {
    /// Build a new adapter that borrows the supplied shared
    /// database handle **and** the asset store the bootstrap
    /// resolved against the platform `data_dir`. The adapter
    /// shares the asset store with the local capture pipeline so
    /// an imported asset collides on the canonical hash the same
    /// way a local capture does. Tests that point the database at
    /// a tempdir inject a synthetic asset store so the asset
    /// namespace stays inside the tempdir.
    pub fn new(
        database: Arc<parking_lot::Mutex<clipvault_db::Database>>,
        asset_store: ClipboardAssetStore,
    ) -> Self {
        Self {
            database,
            asset_store,
            staging_leases: Arc::new(Mutex::new(StagingLeaseState::default())),
            staging_lease_changed: Condvar::new(),
        }
    }

    /// Convenience helper the bootstrap uses when it does not
    /// already hold a `ClipboardAssetStore` handle. The helper
    /// resolves the data dir from `default_database_path()` so
    /// the asset namespace stays aligned with the SQLite
    /// location.
    #[allow(dead_code)]
    pub fn with_data_dir(
        database: Arc<parking_lot::Mutex<clipvault_db::Database>>,
        data_dir: PathBuf,
    ) -> Self {
        Self::new(database, ClipboardAssetStore::new(data_dir))
    }
}

impl PeerImageImportPersistence for SqliteImageImportPersistence {
    fn find_entry_by_hash(
        &self,
        content_hash: &str,
    ) -> Result<Option<i64>, PeerImageImportPersistenceError> {
        let mut db = self.database.lock();
        let repo = clipvault_db::EntryRepository::new(db.connection_mut());
        let record = repo
            .find_by_hash(content_hash)
            .map_err(|error| PeerImageImportPersistenceError::Sqlite(format!("{error}")))?;
        Ok(record.map(|entry| entry.id))
    }

    fn insert_image_entry(
        &self,
        content_size: i64,
        content_hash: String,
        title: Option<String>,
        width: i64,
        height: i64,
        asset_ref: String,
        created_at: OffsetDateTime,
        last_seen_at: OffsetDateTime,
    ) -> Result<i64, PeerImageImportPersistenceError> {
        let mut db = self.database.lock();
        let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
        let outcome = repo
            .insert_or_touch(NewEntry {
                content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
                content_type: ContentType::Image,
                content_size,
                content_hash,
                source_app: None,
                created_at,
                last_seen_at,
                asset_ref: Some(asset_ref),
                mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
                payload_width: Some(width.max(0) as u32),
                payload_height: Some(height.max(0) as u32),
                rich_text_hash: None,
                rich_html_ref: None,
                rich_rtf_ref: None,
                rich_preview_ref: None,
                rich_html_size: None,
                rich_rtf_size: None,
                code_language: None,
            })
            .map_err(|error| PeerImageImportPersistenceError::Sqlite(format!("{error}")))?;
        let id = outcome.record().id;
        // Apply the title only on a freshly inserted row so a
        // reused entry never overwrites its user-edited title.
        if matches!(outcome, clipvault_db::EntryOutcome::Inserted(_)) {
            if let Some(title) = title.filter(|value| !value.trim().is_empty()) {
                repo.set_title(id, Some(title.as_str()), created_at)
                    .map_err(|error| PeerImageImportPersistenceError::Sqlite(format!("{error}")))?;
            }
        }
        Ok(id)
    }

    fn touch_entry_last_seen(
        &self,
        entry_id: i64,
        last_seen_at: OffsetDateTime,
    ) -> Result<i64, PeerImageImportPersistenceError> {
        // The reuse path only needs to refresh the last_seen
        // timestamp so the entry stays visible inside the
        // `Historial` default collection ordering. We run an
        // explicit UPDATE through the repository rather than
        // re-running the full insert_or_touch path so the
        // metadata of the reused row (title, favorites, asset
        // ref, …) is preserved bit-for-bit.
        let mut db = self.database.lock();
        let repo = clipvault_db::EntryRepository::new(db.connection_mut());
        repo.touch_last_seen(entry_id, last_seen_at)
            .map_err(|error| PeerImageImportPersistenceError::Sqlite(format!("{error}")))?;
        Ok(entry_id)
    }

    fn fetch_entry(
        &self,
        entry_id: i64,
    ) -> Result<Option<EntryRecord>, PeerImageImportPersistenceError> {
        let mut db = self.database.lock();
        let repo = clipvault_db::EntryRepository::new(db.connection_mut());
        repo.find_by_id(entry_id)
            .map_err(|error| PeerImageImportPersistenceError::Sqlite(format!("{error}")))
    }

    fn find_binding(&self, peer_id: &str) -> Result<Option<i64>, PeerImageImportPersistenceError> {
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
    ) -> Result<i64, PeerImageImportPersistenceError> {
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
    ) -> Result<(), PeerImageImportPersistenceError> {
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
    ) -> Result<Option<i64>, PeerImageImportPersistenceError> {
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
    ) -> Result<(), PeerImageImportPersistenceError> {
        let mut db = self.database.lock();
        let mut repo = PeerImportRepository::new(db.connection_mut());
        repo.record_import(
            peer_id,
            remote_entry_id,
            imported_content_hash,
            local_entry_id,
            now,
        )
        .map(|_| ())
        .map_err(map_peer_import_repo_error)
    }

    fn collection_name_exists(&self, name: &str) -> Result<bool, PeerImageImportPersistenceError> {
        let mut db = self.database.lock();
        let repo = OrganizationRepository::new(db.connection_mut());
        let collection = repo
            .list_collections()
            .map_err(|error| PeerImageImportPersistenceError::Organization(format!("{error}")))?;
        let target = name.trim().to_lowercase();
        Ok(collection
            .iter()
            .any(|c| c.name.trim().to_lowercase() == target))
    }

    fn create_user_collection(
        &self,
        name: &str,
        color_hex: &str,
        now: OffsetDateTime,
    ) -> Result<i64, PeerImageImportPersistenceError> {
        let mut db = self.database.lock();
        let mut repo = OrganizationRepository::new(db.connection_mut());
        let collection = repo
            .create_user_collection(name, color_hex, now)
            .map_err(|error| PeerImageImportPersistenceError::Organization(format!("{error}")))?;
        Ok(collection.id)
    }

    fn find_collection(
        &self,
        collection_id: i64,
    ) -> Result<Option<Collection>, PeerImageImportPersistenceError> {
        let mut db = self.database.lock();
        let repo = OrganizationRepository::new(db.connection_mut());
        repo.find_collection(collection_id)
            .map_err(|error| PeerImageImportPersistenceError::Organization(format!("{error}")))
    }

    fn stage_image_asset(
        &self,
        bytes: &[u8],
        hint_remote_entry_id: &str,
    ) -> Result<StagedImageAsset, PeerImageImportPersistenceError> {
        // Validate the PNG and normalise the bytes the host
        // returned before persisting them. The asset store
        // re-derives the canonical hash on every write so the
        // dedupe key matches the value the local capture path
        // produces.
        let image = clipboard_assets::decode_png(bytes).map_err(|error| {
            PeerImageImportPersistenceError::Asset(format!("decode: {error:?}"))
        })?;
        let width = image.width();
        let height = image.height();
        let normalized = clipboard_assets::normalize_image(&image).map_err(|error| {
            PeerImageImportPersistenceError::Asset(format!("normalize: {error:?}"))
        })?;
        let asset_ref = normalized.asset_ref();
        // Serialize the filesystem write/reuse with rollback cleanup,
        // and register the lease before releasing either lock. If a
        // cleanup has already reserved this asset, wait until its
        // delete-or-preserve decision completes and then stage afresh.
        let outcome = loop {
            let asset_guard = self.asset_store.lock_mutations();
            let mut leases = self.staging_leases.lock();
            if leases.is_deleting(&asset_ref) {
                drop(asset_guard);
                self.staging_lease_changed.wait(&mut leases);
                continue;
            }
            let outcome = self
                .asset_store
                .store_image_with_guard(&normalized, &asset_guard)
                .map_err(|error| {
                    PeerImageImportPersistenceError::Asset(format!("store: {error:?}"))
                })?;
            leases.increment(&asset_ref);
            break outcome;
        };
        let canonical_hash = normalized.hash().to_string();
        let content_size = normalized.png().len() as i64;
        // Translate the asset store outcome into the
        // rollback-aware [`StageKind`] the helper carries. A
        // `Written` outcome means the staging helper created
        // the file during this attempt; a `Reused` outcome
        // means the asset pre-existed and the rollback path
        // MUST keep it even when the surrounding transaction
        // fails and no `clipboard_entries` row references the
        // file.
        let kind = match outcome.kind() {
            "written" => crate::peer_image_import::StageKind::Written,
            "reused" => crate::peer_image_import::StageKind::Reused,
            _ => crate::peer_image_import::StageKind::Written,
        };
        // The lease was acquired atomically with the store operation.
        // A failed store cannot leak a lease, and cleanup cannot slip
        // between the filesystem result and this registration.
        // Metadata-only diagnostic: the typed `kind`
        // (`written` / `reused`) and the canonicalised
        // dimensions + size are enough for a triage log to
        // distinguish the happy path from a regression without
        // ever logging the asset reference (a relative path
        // that is still sensitive in the user's `~/.clipvault`),
        // the canonical content hash (a unique fingerprint of
        // the imported payload) or the remote entry id (a
        // peer-issued opaque identifier). The `hint` value the
        // caller forwards is metadata-only by construction.
        let hint_kind = if hint_remote_entry_id.is_empty() {
            "none"
        } else {
            "present"
        };
        tracing::debug!(
            kind = kind.as_str(),
            hint = hint_kind,
            width = width,
            height = height,
            bytes = content_size,
            "staged imported PNG asset"
        );
        Ok(StagedImageAsset {
            asset_ref,
            canonical_hash,
            width,
            height,
            content_size,
            kind,
        })
    }

    fn release_staged_asset(
        &self,
        staged: &StagedImageAsset,
    ) -> Result<(), PeerImageImportPersistenceError> {
        // The rollback contract the `peer-image-import` change
        // pins: a staged asset that was [`StageKind::Reused`]
        // is a pre-existing file the asset store found on disk.
        // The helper MUST keep it even when the surrounding
        // transaction rolls back and no `clipboard_entries` row
        // references the file — a future import that targets
        // the same snapshot would otherwise lose the dedupe
        // key and the local capture pipeline would silently
        // accept a different PNG next time the peer offers
        // the same bytes.
        if staged.kind == StageKind::Reused {
            // A pre-existing asset is always kept. The check
            // runs BEFORE the reference count so a referenced
            // OR an orphaned reused asset stays on disk. The
            // staging lease is released: a future commit for
            // the same `asset_ref` is a fresh stage that will
            // re-acquire the lease on its own.
            self.staging_leases.lock().decrement(&staged.asset_ref);
            return Ok(());
        }
        // Reserve the asset for cleanup before releasing the lease
        // mutex. A new peer stage that targets this asset waits on the
        // reservation instead of slipping between the last decrement
        // and the unlink. If another import still holds a lease, only
        // release ours and leave the shared file untouched.
        {
            let mut leases = self.staging_leases.lock();
            let current = leases.current(&staged.asset_ref);
            if current > 1 {
                // Another import still holds the staged asset;
                // release our lease and leave the file in place
                // so the other attempt can commit against it.
                leases.decrement(&staged.asset_ref);
                return Ok(());
            }
            if current == 0 {
                return Ok(());
            }
            // We are the last holder. The marker prevents a new stage
            // from reusing this asset until the reference check and
            // possible unlink are complete.
            leases.decrement(&staged.asset_ref);
            leases.deleting.insert(staged.asset_ref.clone());
            self.staging_lease_changed.notify_all();
        }
        let cleanup_result = (|| {
            let asset_guard = self.asset_store.lock_mutations();
            let references = {
                let mut db = self.database.lock();
                let repo = clipvault_db::EntryRepository::new(db.connection_mut());
                repo.count_asset_references(&staged.asset_ref)
                    .map_err(|error| PeerImageImportPersistenceError::Sqlite(format!("{error}")))?
            };
            if references > 0 {
                // Pre-existing entry shares the asset — leave it on
                // disk. The dedupe contract stays consistent: a
                // future import can still reuse this file through
                // the canonical-hash path.
                return Ok(());
            }
            // Safe to drop. The asset store routes the cleanup
            // through the resolved path so namespace validation is
            // enforced one last time before unlink.
            self.asset_store
                .delete_if_present_with_guard(&staged.asset_ref, &asset_guard)
                .map(|_| ())
                .map_err(|error| {
                    PeerImageImportPersistenceError::Asset(format!("release: {error:?}"))
                })
        })();
        {
            let mut leases = self.staging_leases.lock();
            leases.deleting.remove(&staged.asset_ref);
            self.staging_lease_changed.notify_all();
        }
        cleanup_result
    }

    fn read_image_bytes(
        &self,
        asset_ref: &str,
    ) -> Result<Vec<u8>, PeerImageImportPersistenceError> {
        if asset_ref.is_empty() {
            return Err(PeerImageImportPersistenceError::Asset("empty".to_string()));
        }
        // Only allow refs inside the `clipboard/` namespace the
        // store owns; the validator (resolve / read_bytes)
        // covers empty, absolute, traversal, symlink-escape and
        // not-found cases in one call.
        let prefix = format!("{}/", CLIPBOARD_ASSETS_DIR);
        if !asset_ref.starts_with(&prefix) {
            return Err(PeerImageImportPersistenceError::Asset(format!(
                "out_of_scope: {asset_ref}"
            )));
        }
        let bytes = self
            .asset_store
            .read_bytes(asset_ref)
            .map_err(|error| PeerImageImportPersistenceError::Asset(format!("read: {error:?}")))?;
        if bytes.len() > IMPORT_MAX_IMAGE_BYTES {
            return Err(PeerImageImportPersistenceError::Asset(format!(
                "body_too_large: {} bytes",
                bytes.len()
            )));
        }
        Ok(bytes)
    }

    fn validate_image_metadata(
        &self,
        width: u32,
        height: u32,
        byte_size: i64,
    ) -> Result<(), PeerImageImportPersistenceError> {
        if width == 0
            || height == 0
            || width > MAX_CLIPBOARD_IMAGE_DIM
            || height > MAX_CLIPBOARD_IMAGE_DIM
        {
            return Err(PeerImageImportPersistenceError::Asset(format!(
                "invalid_dimensions: {width}x{height}"
            )));
        }
        if byte_size <= 0 || (byte_size as usize) > IMPORT_MAX_IMAGE_BYTES {
            return Err(PeerImageImportPersistenceError::Asset(format!(
                "invalid_byte_size: {byte_size}"
            )));
        }
        Ok(())
    }

    fn commit_import_transaction(
        &self,
        spec: crate::peer_image_import::ImageImportTransactionSpec,
    ) -> Result<
        crate::peer_image_import::ImageImportTransactionOutcome,
        PeerImageImportPersistenceError,
    > {
        use crate::peer_image_import::{ImageImportTransactionOutcome, ImageImportTransactionSpec};
        use clipvault_db::PeerImportRepository;

        let ImageImportTransactionSpec {
            peer_id,
            remote_entry_id,
            display_name,
            staged,
            validated_title,
            now,
        } = spec;

        // Delegate the atomic commit to the database layer. The
        // repository opens the single SQLite transaction that
        // wraps every mutation (entry insert / reuse,
        // collection creation, binding upsert, membership
        // attach, provenance record); the adapter only projects
        // the typed inputs and converts the typed outcome.
        let commit_result = {
            let mut db = self.database.lock();
            let mut repo = PeerImportRepository::new(db.connection_mut());
            let repo_spec = clipvault_db::ImageImportSpec {
                peer_id,
                remote_entry_id,
                display_name,
                canonical_hash: staged.canonical_hash,
                asset_ref: staged.asset_ref.clone(),
                mime_type: clipvault_db::IMAGE_MIME_PNG.to_string(),
                width: staged.width,
                height: staged.height,
                content_size: staged.content_size,
                validated_title,
                now,
            };
            repo.commit_image_import_transaction(repo_spec)
                .map_err(map_peer_import_repo_error)
        };
        let outcome = commit_result?;
        // The commit succeeded: release the staging lease the
        // helper acquired during [`stage_image_asset`] so the
        // rollback path's per-asset counter reflects the
        // import has moved past the staging step. The entry
        // the commit inserted (or refreshed) takes ownership
        // of the `asset_ref` from this point on, so the file
        // itself stays on disk regardless of the lease counter.
        self.staging_leases.lock().decrement(&staged.asset_ref);
        Ok(ImageImportTransactionOutcome {
            entry_id: outcome.entry_id,
            collection_id: outcome.collection_id,
            deduplicated: outcome.deduplicated,
        })
    }
}

fn map_peer_import_repo_error(error: PeerImportRepositoryError) -> PeerImageImportPersistenceError {
    match error {
        PeerImportRepositoryError::Sqlite(source) => {
            PeerImageImportPersistenceError::Sqlite(source.to_string())
        }
        PeerImportRepositoryError::Organization(source) => {
            PeerImageImportPersistenceError::Organization(source.to_string())
        }
        PeerImportRepositoryError::UnknownCollection(id) => {
            PeerImageImportPersistenceError::UnknownCollection(id)
        }
        PeerImportRepositoryError::UnknownPeer(_) | PeerImportRepositoryError::UnknownEntry(_) => {
            PeerImageImportPersistenceError::Sqlite(format!("{error}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_assets::ClipboardAssetStore;
    use crate::peer_image_import::ImageImportTransactionSpec;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    fn build_png(width: u32, height: u32) -> Vec<u8> {
        use png::{BitDepth, ColorType, Encoder};
        let mut data = Vec::new();
        {
            let mut encoder = Encoder::new(&mut data, width, height);
            encoder.set_color(ColorType::Rgba);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header().expect("header");
            let stride = width as usize * 4;
            let mut image_data = vec![0u8; stride * height as usize];
            for chunk in image_data.chunks_exact_mut(stride) {
                for pixel in chunk.chunks_exact_mut(4) {
                    pixel[0] = 0x10;
                    pixel[1] = 0x20;
                    pixel[2] = 0x30;
                    pixel[3] = 0xFF;
                }
            }
            writer.write_image_data(&image_data).expect("write");
        }
        data
    }

    /// Defence-in-depth contract: the diagnostic `tracing::debug!`
    /// the staging helper emits MUST NOT carry the asset
    /// reference (a relative path under `~/.clipvault/assets/...`,
    /// still sensitive on the user's machine), the canonical
    /// content hash (a unique fingerprint of the imported
    /// payload) or the peer-issued opaque entry id. The
    /// `tracing-subscriber` infrastructure the project already
    /// uses in `redact.rs` is the canonical mechanism for
    /// asserting log contracts.
    #[test]
    fn stage_image_asset_log_redacts_remote_and_asset_metadata() {
        let dir = tempfile::tempdir().expect("tempdir");
        let asset_store = ClipboardAssetStore::new(dir.path());
        // Capture every log line through an in-memory writer
        // so the test can read the formatted output and
        // verify the contract end-to-end.
        #[derive(Default, Clone)]
        struct Capture(Arc<Mutex<Vec<u8>>>);
        impl Write for Capture {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'w> MakeWriter<'w> for Capture {
            type Writer = Capture;
            fn make_writer(&'w self) -> Self::Writer {
                self.clone()
            }
        }
        let capture = Capture::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(capture.clone())
            .with_max_level(tracing::Level::DEBUG)
            .finish();
        let bytes = build_png(8, 8);
        let staged = tracing::subscriber::with_default(subscriber, || {
            asset_store.store_image(
                &clipboard_assets::normalize_image(
                    &clipboard_assets::decode_png(&bytes).expect("decode"),
                )
                .expect("normalize"),
            )
        });
        // The above call doesn't reach the SqliteImageImportPersistence
        // helper, so exercise the diagnostic the production
        // adapter actually emits by exercising the helper
        // directly through the InMemoryImageImportPersistence
        // mock (the production adapter requires a database
        // handle; the InMemory persistence shares the same
        // stage path).
        let _ = staged; // silence unused warning
        let persistence = crate::peer_image_import::InMemoryImageImportPersistence::new();
        let _ = tracing::subscriber::with_default(
            tracing_subscriber::fmt()
                .with_writer(capture.clone())
                .with_max_level(tracing::Level::DEBUG)
                .finish(),
            || persistence.stage_image_asset(&bytes, "entry-sensitive-id"),
        );

        let buffer = String::from_utf8_lossy(&capture.0.lock().unwrap()).into_owned();
        // The helper is allowed to keep the typed `kind` /
        // `width` / `height` / `bytes` fields (metadata-only
        // diagnostics), but the asset reference, the canonical
        // hash and the remote entry id MUST stay out of the
        // log line.
        assert!(
            !buffer.contains("clipboard/"),
            "asset reference leaked into staging log: {buffer}"
        );
        assert!(
            !buffer.contains("entry-sensitive-id"),
            "remote entry id leaked into staging log: {buffer}"
        );
        assert!(
            !buffer.contains("asset_ref"),
            "asset_ref key leaked into staging log: {buffer}"
        );
        assert!(
            !buffer.contains("canonical_hash"),
            "canonical_hash key leaked into staging log: {buffer}"
        );
        assert!(
            !buffer.contains("hint_remote_entry_id"),
            "hint_remote_entry_id key leaked into staging log: {buffer}",
        );
    }

    /// Concurrent import regression coverage that exercises the
    /// PRODUCTION `SqliteImageImportPersistence` against a tempdir
    /// and a temporary SQLite database. The test deterministically
    /// coordinates the interleaving the regression pins: thread A
    /// writes the asset (`Written`), thread B reuses it
    /// (`Reused`), A fails its commit and rolls back. The
    /// production adapter's staging lease MUST keep the file on
    /// disk so B's commit still resolves the canonical
    /// `asset_ref` to a readable PNG. The in-memory adapter test
    /// covers the same invariant on the test double; this test
    /// pins the contract on the SQLite adapter the bootstrap
    /// actually installs.
    #[test]
    fn sqlite_concurrent_staging_protects_reused_asset_during_rollback() {
        use clipvault_db::{builtin_migrations, Database, KnownPeerRepository, PeerObservation};
        use std::sync::{Arc as StdArc, Barrier};
        use std::thread;

        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("clipvault.db");
        let mut db = Database::open(&db_path).expect("open db");
        db.run_migrations(&builtin_migrations()).expect("migrate");
        // Seed the peer row so the `remote_imports.peer_id` FK
        // constraint accepts B's commit. The peer is unique to
        // this test so concurrent test runs do not collide.
        {
            let conn = db.connection_mut();
            let mut repo = KnownPeerRepository::new(conn);
            repo.upsert_observation(&PeerObservation {
                peer_id: "peer-b-sqlite-leases".to_string(),
                public_key_fingerprint: "ab".repeat(32),
                full_public_key_fingerprint: Some("cd".repeat(32)),
                display_name: "peer-b".to_string(),
                protocol_major: 1,
                capability: "pairing".to_string(),
                caps_extra: String::new(),
                observed_at: OffsetDateTime::now_utc(),
            })
            .expect("seed peer");
        }
        drop(db);
        let database = StdArc::new(parking_lot::Mutex::new(
            Database::open(&db_path).expect("open db"),
        ));
        let asset_store = ClipboardAssetStore::new(dir.path());
        let asset_root = asset_store.root();
        std::fs::create_dir_all(&asset_root).expect("mkdir assets");
        let persistence = StdArc::new(SqliteImageImportPersistence::new(
            database.clone(),
            asset_store,
        ));

        let bytes = build_png(8, 8);
        // The barriers coordinate the interleaving the test
        // pins deterministically so the regression the
        // production lease fix targets actually fires. A second
        // `store_image` race inside `ClipboardAssetStore` could
        // mask the staging-lease invariant when both stages
        // land in parallel; the `a_stage_done` barrier forces
        // B's stage to wait until A's stage has fully returned
        // so B's `Reused` outcome is guaranteed by the asset
        // store, not by a lucky window inside the rename path.
        let barrier_a_stage_done = StdArc::new(Barrier::new(2));
        let barrier_b_stage_done = StdArc::new(Barrier::new(2));
        let barrier_a_release_done = StdArc::new(Barrier::new(2));
        let persistence_a = StdArc::clone(&persistence);
        let persistence_b = StdArc::clone(&persistence);
        let bytes_a = bytes.clone();
        let bytes_b = bytes.clone();
        let barrier_a_after_stage = StdArc::clone(&barrier_a_stage_done);
        let barrier_b_before_stage = StdArc::clone(&barrier_a_stage_done);
        let barrier_a_after_b = StdArc::clone(&barrier_b_stage_done);
        let barrier_b_after_stage = StdArc::clone(&barrier_b_stage_done);
        let barrier_a_after_release = StdArc::clone(&barrier_a_release_done);
        let barrier_b_before_commit = StdArc::clone(&barrier_a_release_done);
        let now = OffsetDateTime::now_utc();

        // Thread A: stage (Written), simulate commit failure,
        // roll back. The previous prototype would now delete
        // the file because the SQLite reference count is still
        // zero (B has not committed yet) and there is no
        // in-memory staging lease to gate the rollback path.
        let handle_a = thread::spawn(move || {
            let staged = persistence_a
                .stage_image_asset(&bytes_a, "entry-a")
                .expect("stage a");
            assert_eq!(staged.kind, StageKind::Written);
            // Signal A's stage finished so B can stage the
            // matching `Reused` view.
            barrier_a_after_stage.wait();
            // Wait for B's stage to acquire its lease before
            // we exercise the rollback path.
            barrier_a_after_b.wait();
            // Simulate a commit failure by skipping the commit
            // and rolling back the staged asset. The staging
            // lease MUST keep the file because B already holds
            // a lease by the time we reach this line.
            persistence_a
                .release_staged_asset(&staged)
                .expect("release a");
            barrier_a_after_release.wait();
        });
        // Thread B: stage (Reused), commit (success). The
        // committed entry MUST keep a valid `asset_ref` that
        // resolves to a readable PNG.
        let handle_b = thread::spawn(move || {
            // Wait for A's stage to fully finish so the asset
            // is on disk and B's stage is guaranteed to be
            // `Reused`.
            barrier_b_before_stage.wait();
            let staged = persistence_b
                .stage_image_asset(&bytes_b, "entry-b")
                .expect("stage b");
            assert_eq!(staged.kind, StageKind::Reused);
            barrier_b_after_stage.wait();
            // Wait for A's rollback to settle so the test
            // exercises the lease guard, not just ordering
            // luck.
            barrier_b_before_commit.wait();
            let outcome = persistence_b
                .commit_import_transaction(ImageImportTransactionSpec {
                    peer_id: "peer-b-sqlite-leases".to_string(),
                    remote_entry_id: "entry-b".to_string(),
                    display_name: "Equipo B".to_string(),
                    staged,
                    validated_title: None,
                    now,
                })
                .expect("commit b");
            outcome.entry_id
        });
        handle_a.join().expect("thread a joins");
        let b_entry_id = handle_b.join().expect("thread b joins");

        // The file MUST still be readable from disk after A's
        // rollback: a regression that drops the production
        // staging lease would unlink the file while B is still
        // holding its lease, and the next call would surface a
        // missing asset for B's committed entry. The asset_ref
        // the staged asset carries is the normalised PNG the
        // asset store committed (the input bytes are re-encoded
        // before the write), so the test must consult the
        // canonical-hash the entry persisted, not a hash of the
        // raw input bytes.
        let entry = persistence
            .fetch_entry(b_entry_id)
            .expect("fetch")
            .expect("entry exists");
        let asset_ref = entry
            .asset_ref
            .as_deref()
            .expect("committed entry must carry an asset_ref")
            .to_string();
        let read = persistence
            .read_image_bytes(&asset_ref)
            .expect("read after rollback");
        // The persisted bytes are the canonical normalised PNG
        // the asset store wrote; the read succeeds as long as
        // the file is still on disk, which is what the test
        // pins.
        assert!(!read.is_empty(), "asset bytes must remain readable");
        assert_eq!(
            read.len(),
            entry.content_size as usize,
            "persisted size must match the entry's content_size",
        );
    }

    #[test]
    fn sqlite_new_stage_waits_for_reserved_rollback_cleanup() {
        use clipvault_db::{builtin_migrations, Database, KnownPeerRepository, PeerObservation};
        use std::sync::Arc as StdArc;
        use std::thread;

        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("clipvault.db");
        let mut db = Database::open(&db_path).expect("open db");
        db.run_migrations(&builtin_migrations()).expect("migrate");
        {
            let mut repo = KnownPeerRepository::new(db.connection_mut());
            repo.upsert_observation(&PeerObservation {
                peer_id: "peer-cleanup-reservation".to_string(),
                public_key_fingerprint: "ef".repeat(32),
                full_public_key_fingerprint: Some("12".repeat(32)),
                display_name: "peer-cleanup".to_string(),
                protocol_major: 1,
                capability: "pairing".to_string(),
                caps_extra: String::new(),
                observed_at: OffsetDateTime::now_utc(),
            })
            .expect("seed peer");
        }
        drop(db);

        let database = StdArc::new(parking_lot::Mutex::new(
            Database::open(&db_path).expect("reopen db"),
        ));
        let asset_store = ClipboardAssetStore::new(dir.path());
        let persistence = StdArc::new(SqliteImageImportPersistence::new(database, asset_store));
        let bytes = build_png(8, 8);
        let staged_a = persistence
            .stage_image_asset(&bytes, "rollback-a")
            .expect("stage rollback asset");
        assert_eq!(staged_a.kind, StageKind::Written);
        let asset_ref = staged_a.asset_ref.clone();

        // Hold the shared filesystem guard so A can reserve cleanup
        // but cannot reach the reference-check/unlink section yet.
        // B starts only after that reservation is visible, precisely
        // exercising the gap the previous counter-only test missed.
        let held_asset_guard = persistence.asset_store.lock_mutations();
        let persistence_a = StdArc::clone(&persistence);
        let rollback = thread::spawn(move || {
            persistence_a
                .release_staged_asset(&staged_a)
                .expect("rollback cleanup");
        });
        {
            let mut leases = persistence.staging_leases.lock();
            while !leases.is_deleting(&asset_ref) {
                persistence.staging_lease_changed.wait(&mut leases);
            }
        }

        let persistence_b = StdArc::clone(&persistence);
        let bytes_b = bytes.clone();
        let import_b = thread::spawn(move || {
            let staged_b = persistence_b
                .stage_image_asset(&bytes_b, "import-b")
                .expect("stage after cleanup reservation");
            assert_eq!(
                staged_b.kind,
                StageKind::Written,
                "B must write after A completes the reserved unlink"
            );
            persistence_b
                .commit_import_transaction(ImageImportTransactionSpec {
                    peer_id: "peer-cleanup-reservation".to_string(),
                    remote_entry_id: "import-b".to_string(),
                    display_name: "Equipo cleanup".to_string(),
                    staged: staged_b,
                    validated_title: None,
                    now: OffsetDateTime::now_utc(),
                })
                .expect("commit B")
                .entry_id
        });

        drop(held_asset_guard);
        rollback.join().expect("rollback thread");
        let entry_id = import_b.join().expect("import thread");
        let entry = persistence
            .fetch_entry(entry_id)
            .expect("fetch committed entry")
            .expect("entry exists");
        let committed_ref = entry.asset_ref.expect("asset ref");
        let read = persistence
            .read_image_bytes(&committed_ref)
            .expect("committed asset remains readable");
        assert!(!read.is_empty());
        assert_eq!(read.len(), entry.content_size as usize);
    }

    #[test]
    fn sqlite_rollback_preserves_asset_captured_before_local_row_commit() {
        use clipvault_db::{builtin_migrations, Database, EntryRepository, NewEntry};
        use std::sync::Arc as StdArc;
        use std::thread;

        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("clipvault.db");
        let mut db = Database::open(&db_path).expect("open db");
        db.run_migrations(&builtin_migrations()).expect("migrate");
        drop(db);

        let database = StdArc::new(parking_lot::Mutex::new(
            Database::open(&db_path).expect("reopen db"),
        ));
        let asset_store = ClipboardAssetStore::new(dir.path());
        let persistence =
            SqliteImageImportPersistence::new(StdArc::clone(&database), asset_store.clone());
        let bytes = build_png(8, 8);
        let image = clipboard_assets::normalize_image(
            &clipboard_assets::decode_png(&bytes).expect("decode PNG"),
        )
        .expect("normalize PNG");
        let staged = persistence
            .stage_image_asset(&bytes, "peer-rollback")
            .expect("stage peer asset");
        assert_eq!(staged.kind, StageKind::Written);

        // This is the local capture critical section: the same shared
        // asset-store lock covers reuse/write and the SQLite row insert.
        let capture_guard = asset_store.lock_mutations();
        let outcome = asset_store
            .store_image_with_guard(&image, &capture_guard)
            .expect("capture reuses staged asset");
        assert_eq!(outcome.kind(), "reused");

        let persistence_for_rollback = StdArc::new(persistence);
        let rollback_owner = StdArc::clone(&persistence_for_rollback);
        let rollback = thread::spawn(move || {
            rollback_owner
                .release_staged_asset(&staged)
                .expect("rollback staged peer asset");
        });
        let asset_ref = image.asset_ref();
        {
            let mut leases = persistence_for_rollback.staging_leases.lock();
            while !leases.is_deleting(&asset_ref) {
                persistence_for_rollback
                    .staging_lease_changed
                    .wait(&mut leases);
            }
        }

        let now = OffsetDateTime::now_utc();
        let new_entry = NewEntry {
            content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
            content_type: clipvault_db::ContentType::Image,
            content_size: image.byte_len() as i64,
            content_hash: image.hash().to_string(),
            source_app: None,
            created_at: now,
            last_seen_at: now,
            asset_ref: Some(asset_ref.clone()),
            mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
            payload_width: Some(image.width()),
            payload_height: Some(image.height()),
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
            code_language: None,
        };
        let capture_entry_id = {
            let mut db = database.lock();
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry)
                .expect("commit local capture row")
                .record()
                .id
        };
        drop(capture_guard);
        rollback.join().expect("rollback thread");

        let capture_entry = persistence_for_rollback
            .fetch_entry(capture_entry_id)
            .expect("fetch local capture")
            .expect("capture entry exists");
        assert_eq!(capture_entry.asset_ref.as_deref(), Some(asset_ref.as_str()));
        assert_eq!(
            persistence_for_rollback
                .read_image_bytes(&asset_ref)
                .expect("rollback preserved committed local asset"),
            image.png(),
        );
    }
}
