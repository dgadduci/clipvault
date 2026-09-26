//! Explicit-import façade for the `peer-image-import` change.
//!
//! The [`PeerImageImportService`] is the thin façade the desktop
//! shell drives when the user activates `Importar` for an image row
//! of a trusted active peer. The service owns:
//!
//! - the metadata-only [`PeerFetchImageTransport`] the runtime
//!   dials to retrieve the canonical PNG payload of the chosen
//!   remote image entry;
//! - the transactional database helper the import commits through
//!   the shared SQLite handle;
//! - the typed [`PeerImageImportOutcome`] the bridge / Tauri shell
//!   returns to the frontend. Every variant collapses to a stable
//!   identifier the UI branches on without inspecting free-form
//!   strings or content bytes.
//!
//! The façade is metadata-only by construction: it never copies
//! the imported bytes into a log, an event payload, a toast or a
//! drag payload. The bytes are held in memory only between the
//! authenticated fetch and the SQLite commit; the commit
//! materialises the local snapshot through the existing image
//! pipeline so the dedupe contract (canonical hash, title
//! preservation, asset / tag / favourite preservation) stays
//! consistent with the local capture path.
//!
//! ## Atomicity
//!
//! The transaction must be all-or-nothing: if the SQLite layer
//! refuses the commit, the temporary staged asset must be cleaned
//! up. Pre-existing assets MUST NOT be deleted or renamed — a
//! shared asset referenced by another row stays put even when the
//! import fails. The façade runs every step of the import through
//! a single persistence trait so tests can verify the rollback
//! path without linking SQLite.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

use clipvault_db::{Collection, CollectionKind, ContentType, EntryRecord};

/// Maximum size of the imported PNG payload in bytes. The
/// contract pins the same 16 MiB cap the local
/// [`clipboard_assets::MAX_CLIPBOARD_ASSET_BYTES`] constant the
/// `clipboard-rich-content` change ships. The transport layer
/// enforces the cap on both the listener (which refuses any body
/// larger than the threshold) and the caller (which never trusts
/// the listener's word alone).
pub const IMPORT_MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;

/// Outcome the runtime returns to the bridge / Tauri shell after a
/// single image import call. Every variant is metadata-only; the
/// bytes never cross the bridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerImageImportOutcome {
    /// The import committed. `entry_id` is the local row the
    /// dedupe path produced (existing or freshly created);
    /// `collection_id` is the peer-bound collection the entry
    /// was added to; `deduplicated` distinguishes a fresh insert
    /// from a snapshot that reused an existing local entry.
    Imported {
        entry_id: i64,
        collection_id: i64,
        /// `true` when the import reused an existing local row
        /// (an identical canonical hash was already stored or the
        /// same `(peer, remote_entry, hash)` provenance was
        /// already recorded). `false` when the import produced a
        /// fresh local row.
        deduplicated: bool,
    },
    /// The peer is not currently eligible to serve an import
    /// (no known row, not trusted, not active). The runtime never
    /// opened a network call; the renderer surfaces the stable
    /// reason copy.
    PeerUnavailable { reason: &'static str },
    /// The fetch transport rejected the request.
    TransportUnavailable { reason: &'static str },
    /// The body the host returned exceeded the cap. The runtime
    /// collapsed the rejection into a typed outcome without
    /// persisting anything.
    BodyTooLarge,
    /// The body the host returned failed PNG validation
    /// (signature, dimensions, decoded frame). The runtime
    /// collapses the rejection into a typed outcome without
    /// persisting anything.
    InvalidImage,
    /// The remote entry the user asked to import no longer
    /// exists on the host or is no longer transferrable. The
    /// runtime surfaces the typed outcome without mutating
    /// SQLite.
    NotTransferable,
    /// The local asset store refused to persist the bytes. The
    /// runtime rolls the whole transaction back so the local
    /// database stays consistent.
    AssetError,
    /// The remote title the host returned failed local title
    /// validation (too long after trimming). The runtime
    /// continues to import the entry without persisting the
    /// invalid title.
    TitleInvalid,
    /// The local SQLite layer refused the commit. The runtime
    /// rolled the whole transaction back so the local database
    /// stays consistent.
    PersistenceError { reason: &'static str },
}

/// Typed error the runtime surfaces for every import failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerImageImportError {
    #[error("peer is not eligible for fetch")]
    PeerUnavailable(&'static str),
    #[error("fetch body exceeded the 16 MiB PNG limit")]
    BodyTooLarge,
    #[error("fetch body failed PNG validation")]
    InvalidImage,
    #[error("remote entry is not transferable")]
    NotTransferable,
    #[error("remote entry returned an empty payload")]
    EmptyContent,
    #[error("remote title failed local validation")]
    TitleInvalid,
    #[error("local asset store refused the write")]
    AssetError,
    #[error("local SQLite layer refused the commit: {0}")]
    Persistence(String),
}

impl PeerImageImportError {
    /// Stable, snake_case reason the runtime / bridge surfaces
    /// for every failure variant.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::PeerUnavailable(reason) => reason,
            Self::BodyTooLarge => "body_too_large",
            Self::InvalidImage => "invalid_image",
            Self::NotTransferable => "not_transferable",
            Self::EmptyContent => "empty_content",
            Self::TitleInvalid => "title_invalid",
            Self::AssetError => "asset_error",
            Self::Persistence(_) => "persistence_unavailable",
        }
    }
}

/// Metadata the client-side facade hands to the transport so the
/// transport can dial the matching peer with the pinned cert
/// fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerFetchImageRequest {
    pub peer_id: String,
    pub cert_fingerprint: String,
    /// Opaque remote entry id the host minted.
    pub remote_entry_id: String,
}

/// Successful payload the [`PeerFetchImageTransport`] returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerFetchImageResponse {
    pub peer_id: String,
    pub remote_entry_id: String,
    pub title: Option<String>,
    /// Canonical PNG bytes the host validated against the asset
    /// store contract.
    pub bytes: Vec<u8>,
    /// Validated source-application display name the
    /// `peer-source-app-presentation` change attaches to the
    /// original image response. The importer forwards the value
    /// to the persistence trait so the import transaction
    /// commits the name atomically with the entry / binding /
    /// provenance rows. `None` when the host did not opt into
    /// the additive contract or when validation refused the
    /// supplied value; the import still succeeds.
    pub source_app_name: Option<String>,
    /// Validated source-application PNG icon bytes the host
    /// attached to the response. The importer stages the bytes
    /// through the asset store, captures the locally-generated
    /// reference and forwards it to the persistence trait so the
    /// icon ref is committed in the same transaction as the
    /// provenance. `None` when the host did not opt into the
    /// additive contract or when validation refused the bytes.
    pub source_app_icon_bytes: Option<Vec<u8>>,
}

/// Metadata-only transport façade the import facade uses. The
/// trait is the seam between the core runtime and the platform
/// mTLS stack: the transport owns the dial loop, the pin lookup
/// and the per-peer session, while the runtime owns the trust /
/// active gate and the bytes validation.
pub trait PeerFetchImageTransport: Send + Sync {
    fn fetch_image(
        &self,
        request: PeerFetchImageRequest,
    ) -> Result<PeerFetchImageResponse, PeerFetchImageTransportError>;
}

/// Typed transport error the runtime maps onto
/// [`PeerImageImportOutcome`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerFetchImageTransportError {
    #[error("peer fetch image transport is unavailable")]
    Unavailable,
    #[error("peer fetch image transport has no resolved pairing endpoint")]
    PeerUnresolved,
    #[error("peer fetch image transport rejected an unknown peer")]
    UnknownPeer,
    #[error("peer fetch image transport rejected a mismatched TLS identity")]
    KeyMismatch,
    #[error("peer fetch image transport rejected a revoked peer")]
    Revoked,
    #[error("peer fetch image transport rejected a blocked peer")]
    Blocked,
    #[error("peer fetch image transport wire protocol is incompatible")]
    IncompatibleProtocol,
    #[error("peer fetch image transport rejected a malformed payload")]
    Malformed,
    #[error("peer fetch image transport rejected a non-transferrable entry")]
    NotTransferable,
    #[error("peer fetch image transport rejected a body that exceeded the size limit")]
    BodyTooLarge,
    #[error("peer fetch image transport rejected the request because persistence is unavailable")]
    PersistenceUnavailable,
}

impl PeerFetchImageTransportError {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::PeerUnresolved => "unavailable",
            Self::UnknownPeer => "unknown_peer",
            Self::KeyMismatch => "key_mismatch",
            Self::Revoked => "revoked",
            Self::Blocked => "blocked",
            Self::IncompatibleProtocol => "incompatible_protocol",
            Self::Malformed => "malformed",
            Self::NotTransferable => "not_transferable",
            Self::BodyTooLarge => "body_too_large",
            Self::PersistenceUnavailable => "persistence_unavailable",
        }
    }
}

/// Persistence trait the importer drives to project the image
/// import transaction against the local SQLite handle. The trait
/// is metadata-only by construction: the bytes never cross the
/// boundary; the helpers receive the typed primitives the
/// repository expects and forward them through the existing CRUD
/// methods. Tests inject an in-memory fake.
pub trait PeerImageImportPersistence: Send + Sync {
    /// Return the entry id whose canonical content hash matches
    /// `content_hash`. `None` when no row exists yet.
    fn find_entry_by_hash(
        &self,
        content_hash: &str,
    ) -> Result<Option<i64>, PeerImageImportPersistenceError>;
    /// Insert a fresh image entry with the supplied metadata.
    /// Returns the inserted row id. The helper is responsible
    /// for attaching the row to the system `Historial`
    /// collection inside the same transaction.
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
    ) -> Result<i64, PeerImageImportPersistenceError>;
    /// Update the title of an existing entry. The importer
    /// refuses to overwrite an existing title.
    fn touch_entry_last_seen(
        &self,
        entry_id: i64,
        last_seen_at: OffsetDateTime,
    ) -> Result<i64, PeerImageImportPersistenceError>;
    /// Fetch a single entry by id.
    fn fetch_entry(
        &self,
        entry_id: i64,
    ) -> Result<Option<EntryRecord>, PeerImageImportPersistenceError>;
    /// Look up the binding row the importer needs to attach the
    /// imported entry to the peer collection.
    fn find_binding(&self, peer_id: &str) -> Result<Option<i64>, PeerImageImportPersistenceError>;
    /// Persist a fresh binding row keyed by `peer_id`.
    fn upsert_binding(
        &self,
        peer_id: &str,
        collection_id: i64,
        now: OffsetDateTime,
    ) -> Result<i64, PeerImageImportPersistenceError>;
    /// Attach an entry to the supplied collection id,
    /// idempotent.
    fn attach_entry_to_collection(
        &self,
        entry_id: i64,
        collection_id: i64,
        now: OffsetDateTime,
    ) -> Result<(), PeerImageImportPersistenceError>;
    /// Look up a `(peer_id, remote_entry_id, hash)` provenance
    /// row.
    fn find_import(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
        imported_content_hash: &str,
    ) -> Result<Option<i64>, PeerImageImportPersistenceError>;
    /// Persist a fresh provenance row keyed by `(peer_id,
    /// remote_entry_id, hash)`.
    fn record_import(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
        imported_content_hash: &str,
        local_entry_id: i64,
        now: OffsetDateTime,
    ) -> Result<(), PeerImageImportPersistenceError>;
    /// Look up the canonical user-collection name the importer
    /// must avoid colliding with.
    fn collection_name_exists(&self, name: &str) -> Result<bool, PeerImageImportPersistenceError>;
    /// Create a fresh user collection the importer uses when
    /// the visible peer name does not collide with an existing
    /// user row.
    fn create_user_collection(
        &self,
        name: &str,
        color_hex: &str,
        now: OffsetDateTime,
    ) -> Result<i64, PeerImageImportPersistenceError>;
    /// Fetch a single collection by id.
    fn find_collection(
        &self,
        collection_id: i64,
    ) -> Result<Option<Collection>, PeerImageImportPersistenceError>;
    /// Stage the bytes for the canonical PNG asset the host
    /// returned. Returns the relative `asset_ref` the
    /// `clipboard_entries.asset_ref` column should reference.
    /// The helper validates the PNG, computes the canonical hash
    /// and atomically writes the bytes to the asset store.
    fn stage_image_asset(
        &self,
        bytes: &[u8],
        hint_remote_entry_id: &str,
    ) -> Result<StagedImageAsset, PeerImageImportPersistenceError>;
    /// Drop a staged asset created by [`Self::stage_image_asset`]
    /// when the surrounding transaction rolls back. The helper
    /// MUST NOT delete or rename any pre-existing asset: it only
    /// removes the asset this exact stage call created, identified
    /// by the canonical hash the staging helper returned. A
    /// caller that wants to release a stale temporary can call
    /// this method with the `asset_ref` the staged value
    /// reported.
    fn release_staged_asset(
        &self,
        staged: &StagedImageAsset,
    ) -> Result<(), PeerImageImportPersistenceError>;
    /// Read the canonical PNG bytes the local asset store serves
    /// for the supplied `asset_ref`. The host handler calls this
    /// helper after validating the entry / asset metadata and
    /// before forwarding the payload through the wire envelope.
    /// Implementations MUST reject references that fail the
    /// asset-store validator (empty, absolute, traversal,
    /// out-of-scope, symlink-escaped, not a PNG, oversized,
    /// invalid dimensions) and MUST NOT return a payload larger
    /// than [`IMPORT_MAX_IMAGE_BYTES`].
    fn read_image_bytes(&self, asset_ref: &str)
        -> Result<Vec<u8>, PeerImageImportPersistenceError>;
    /// Best-effort read of an existing source-app icon by its local
    /// application-icons reference. Missing/invalid icon metadata must not
    /// make the actual image import fail.
    fn read_source_app_icon(
        &self,
        _asset_ref: &str,
    ) -> Result<Option<Vec<u8>>, PeerImageImportPersistenceError> {
        Ok(None)
    }
    /// Stage the source-application PNG icon the host returned
    /// through the local application-icon store. The helper
    /// validates the bytes (signature, decode, dimensions, byte
    /// cap), computes a content-addressed reference under
    /// `application-icons/` and never accepts a peer-supplied
    /// path / filename / reference. The returned
    /// [`StagedImageAsset`] mirrors [`Self::stage_image_asset`]:
    /// a [`StageKind::Written`] outcome signals a freshly created
    /// file the caller is allowed to delete on rollback; a
    /// [`StageKind::Reused`] outcome signals a shared icon the
    /// rollback MUST keep. The helper returns
    /// `Ok(None)` when the caller passes `None` (no icon was
    /// sent by the host) so a peer that did not opt into the
    /// additive contract never has to opt into the icon path.
    fn stage_source_app_icon(
        &self,
        bytes: Option<&[u8]>,
    ) -> Result<Option<StagedImageAsset>, PeerImageImportPersistenceError>;
    /// Re-validate a `(width, height, byte_size)` triple against
    /// the local limits the asset store enforces. Returns the
    /// valid triple the importer persists; the helper is
    /// metadata-only and never inspects the asset bytes.
    fn validate_image_metadata(
        &self,
        width: u32,
        height: u32,
        byte_size: i64,
    ) -> Result<(), PeerImageImportPersistenceError>;
    /// Commit the full import transaction: ensure a peer
    /// collection (creating one if missing), insert or reuse the
    /// entry by canonical hash, attach it to the bound
    /// collection, and record the `(peer, remote_entry_id, hash)`
    /// provenance row. The helper runs every step inside a
    /// single SQLite transaction so a failure in any of the
    /// steps rolls the whole import back; the caller MUST then
    /// drop the staged asset through
    /// [`Self::release_staged_asset`] to keep the asset / entry
    /// invariants aligned. The structured return value lets the
    /// importer distinguish the `inserted` / `reused` / `binding`
    /// outcomes without an extra round-trip.
    fn commit_import_transaction(
        &self,
        spec: ImageImportTransactionSpec,
    ) -> Result<ImageImportTransactionOutcome, PeerImageImportPersistenceError>;
}

/// Outcome of staging a remote PNG payload through the asset
/// store. The helper returns the canonical hash, the relative
/// `asset_ref` the importer persists in `clipboard_entries`, the
/// dimensions the persistence layer derived when it normalised
/// the bytes and the typed [`StageKind`] the rollback path
/// consults so a `Reused` asset is never deleted even when the
/// surrounding transaction fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedImageAsset {
    pub asset_ref: String,
    pub canonical_hash: String,
    pub width: u32,
    pub height: u32,
    pub content_size: i64,
    /// Stable discriminator the rollback path uses to decide
    /// whether the staged asset is safe to delete. A
    /// [`StageKind::Written`] value means this attempt created
    /// the file; a [`StageKind::Reused`] value means the asset
    /// pre-existed on disk and the helper MUST keep it even when
    /// the surrounding transaction rolls back.
    pub kind: StageKind,
}

/// Stable discriminator the rollback path uses to honour the
/// rollback contract the `peer-image-import` change pins: an
/// attempt that [`StageKind::Reused`] an existing asset must
/// never delete the asset (even when the import transaction
/// fails and no `clipboard_entries` row references the file);
/// only an attempt that produced a [`StageKind::Written`]
/// outcome is allowed to delete the temporary on rollback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageKind {
    /// The staging helper created a new asset file. The
    /// rollback path MAY delete the file when the surrounding
    /// transaction fails and no row references it.
    Written,
    /// The staging helper reused an existing asset file. The
    /// rollback path MUST keep the file even when the
    /// surrounding transaction fails and no row references it.
    Reused,
}

impl StageKind {
    /// Stable snake_case identifier the structured log emits.
    /// Mirrors the [`clipvault_core::clipboard_assets::StoreOutcome::kind`]
    /// taxonomy so a triage view can correlate the two surfaces.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Written => "written",
            Self::Reused => "reused",
        }
    }
}

/// Metadata the importer passes to
/// [`PeerImageImportPersistence::commit_import_transaction`].
/// The struct bundles the validated primitives the transaction
/// needs; the persistence layer is responsible for projecting
/// them into the typed repository helpers and committing the
/// whole sequence in a single SQLite transaction.
///
/// `source_app_name` and `source_app_icon` are the additive
/// fields the `peer-source-app-presentation` change attaches
/// to a peer-bound provenance row. Both fields stay `None` for
/// peers that did not opt into the additive contract (legacy
/// flow) or when validation refused the supplied value; the
/// persistence layer MUST treat the absence as a typed no-op
/// and commit the remaining rows unchanged. When
/// `source_app_icon` is `Some(StagedImageAsset)`, the caller is
/// responsible for releasing the staged icon through
/// [`PeerImageImportPersistence::release_staged_asset`] when
/// the surrounding transaction rolls back.
#[derive(Debug, Clone)]
pub struct ImageImportTransactionSpec {
    pub peer_id: String,
    pub remote_entry_id: String,
    pub display_name: String,
    pub staged: StagedImageAsset,
    pub validated_title: Option<String>,
    pub source_app_name: Option<String>,
    pub source_app_icon: Option<StagedImageAsset>,
    pub now: OffsetDateTime,
}

/// Typed outcome the transactional commit helper returns. The
/// importer collapses the structured result into the
/// [`PeerImageImportOutcome::Imported`] discriminated union so
/// the bridge / UI can branch on the dedupe status without an
/// extra round-trip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageImportTransactionOutcome {
    pub entry_id: i64,
    pub collection_id: i64,
    pub deduplicated: bool,
}

/// Typed persistence error the runtime surfaces for every
/// SQLite refusal.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerImageImportPersistenceError {
    #[error("sqlite error: {0}")]
    Sqlite(String),
    #[error("organization error: {0}")]
    Organization(String),
    #[error("collection {0} not found")]
    UnknownCollection(i64),
    #[error("asset error: {0}")]
    Asset(String),
}

impl From<OrganizationError> for PeerImageImportPersistenceError {
    fn from(error: OrganizationError) -> Self {
        Self::Organization(format!("{error}"))
    }
}

// `OrganizationError` re-export so the import trait stays
// self-contained.
use clipvault_db::OrganizationError;

/// Clock the importer uses to stamp the import timestamps.
pub trait ImageImportClock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

/// Production clock backed by the host's `OffsetDateTime::now_utc`.
pub struct SystemImageImportClock;

impl ImageImportClock for SystemImageImportClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

/// In-memory persistence adapter the tests use. The adapter
/// mirrors the contract the production adapter exposes.
pub struct InMemoryImageImportPersistence {
    state: Arc<Mutex<InMemoryImageImportState>>,
}

struct InMemoryImageImportState {
    entries: HashMap<i64, EntryRecord>,
    next_entry_id: i64,
    bindings: HashMap<String, i64>,
    imports: HashSet<(String, String, String)>,
    collections: HashMap<i64, Collection>,
    next_collection_id: i64,
    by_hash: HashMap<String, i64>,
    /// Per-provenance source-app metadata the
    /// `peer-source-app-presentation` change writes through the
    /// import transaction. Keyed by the composite
    /// `(peer_id, remote_entry_id, imported_content_hash)` so
    /// the in-memory adapter mirrors the production SQLite
    /// projection: every peer keeps its own source-app name /
    /// icon ref even when multiple peers point at the same
    /// local entry.
    provenance_source_app: HashMap<(String, String, String), (Option<String>, Option<String>)>,
    /// Per-staged-asset kind the rollback path consults. A
    /// `Reused` value MUST survive a rolled-back transaction
    /// even when no entry references the asset.
    staged_kinds: HashMap<String, StageKind>,
    asset_bytes: HashMap<String, Vec<u8>>,
    /// Per-asset staging lease the rollback path consults so
    /// concurrent imports that race on the same `asset_ref`
    /// never delete a file the other attempt is about to
    /// reference. The counter tracks how many in-flight
    /// stages still hold the asset; the rollback path only
    /// deletes the file once the counter hits zero AND no
    /// entry references it.
    staged_leases: HashMap<String, usize>,
}

impl Default for InMemoryImageImportPersistence {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(InMemoryImageImportState {
                entries: HashMap::new(),
                next_entry_id: 1,
                bindings: HashMap::new(),
                imports: HashSet::new(),
                collections: HashMap::new(),
                next_collection_id: 1,
                by_hash: HashMap::new(),
                provenance_source_app: HashMap::new(),
                staged_kinds: HashMap::new(),
                asset_bytes: HashMap::new(),
                staged_leases: HashMap::new(),
            })),
        }
    }
}

impl InMemoryImageImportPersistence {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the adapter with a deterministic list of entries.
    pub fn seed_entries(&self, entries: Vec<EntryRecord>) {
        let mut state = self.state.lock();
        for entry in entries {
            state.by_hash.insert(entry.content_hash.clone(), entry.id);
            state.next_entry_id = state.next_entry_id.max(entry.id + 1);
            state.entries.insert(entry.id, entry);
        }
    }

    /// Look up the source-app name + icon ref a previous
    /// `commit_import_transaction` call persisted for the
    /// `(peer_id, remote_entry_id, imported_content_hash)` triple.
    /// Returns `None` when no provenance row exists, mirroring
    /// the production `find_import` helper.
    pub fn source_app_for(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
        imported_content_hash: &str,
    ) -> Option<(Option<String>, Option<String>)> {
        self.state
            .lock()
            .provenance_source_app
            .get(&(
                peer_id.to_string(),
                remote_entry_id.to_string(),
                imported_content_hash.to_string(),
            ))
            .cloned()
    }
}

/// Decrement the per-asset staging lease counter, removing the
/// entry once the counter reaches zero. The helper is the
/// shared bookkeeping primitive the commit + rollback paths use
/// to keep the counter in sync with the actual number of
/// in-flight stages.
fn decrement_staged_lease(state: &mut InMemoryImageImportState, asset_ref: &str) {
    let Some(slot) = state.staged_leases.get_mut(asset_ref) else {
        return;
    };
    if *slot <= 1 {
        state.staged_leases.remove(asset_ref);
    } else {
        *slot -= 1;
    }
}

impl PeerImageImportPersistence for InMemoryImageImportPersistence {
    fn find_entry_by_hash(
        &self,
        content_hash: &str,
    ) -> Result<Option<i64>, PeerImageImportPersistenceError> {
        Ok(self.state.lock().by_hash.get(content_hash).copied())
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
        let mut state = self.state.lock();
        let id = state.next_entry_id;
        state.next_entry_id += 1;
        let now_string = created_at
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
        let last_seen_string = last_seen_at
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| now_string.clone());
        let record = EntryRecord {
            id,
            content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
            content_type: ContentType::Image,
            content_size,
            content_hash: content_hash.clone(),
            source_app: None,
            is_pinned: false,
            created_at: now_string.clone(),
            updated_at: now_string,
            last_seen_at: last_seen_string,
            title,
            source_app_name: None,
            source_app_icon_ref: None,
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
        };
        state.entries.insert(id, record);
        state.by_hash.insert(content_hash, id);
        Ok(id)
    }

    fn touch_entry_last_seen(
        &self,
        entry_id: i64,
        last_seen_at: OffsetDateTime,
    ) -> Result<i64, PeerImageImportPersistenceError> {
        let mut state = self.state.lock();
        let record =
            state
                .entries
                .get_mut(&entry_id)
                .ok_or(PeerImageImportPersistenceError::Sqlite(format!(
                    "entry {entry_id} not found"
                )))?;
        record.last_seen_at = last_seen_at
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
        Ok(entry_id)
    }

    fn fetch_entry(
        &self,
        entry_id: i64,
    ) -> Result<Option<EntryRecord>, PeerImageImportPersistenceError> {
        Ok(self.state.lock().entries.get(&entry_id).cloned())
    }

    fn find_binding(&self, peer_id: &str) -> Result<Option<i64>, PeerImageImportPersistenceError> {
        Ok(self.state.lock().bindings.get(peer_id).copied())
    }

    fn upsert_binding(
        &self,
        peer_id: &str,
        collection_id: i64,
        _now: OffsetDateTime,
    ) -> Result<i64, PeerImageImportPersistenceError> {
        self.state
            .lock()
            .bindings
            .insert(peer_id.to_string(), collection_id);
        Ok(collection_id)
    }

    fn attach_entry_to_collection(
        &self,
        _entry_id: i64,
        _collection_id: i64,
        _now: OffsetDateTime,
    ) -> Result<(), PeerImageImportPersistenceError> {
        Ok(())
    }

    fn find_import(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
        imported_content_hash: &str,
    ) -> Result<Option<i64>, PeerImageImportPersistenceError> {
        let state = self.state.lock();
        if state.imports.contains(&(
            peer_id.to_string(),
            remote_entry_id.to_string(),
            imported_content_hash.to_string(),
        )) {
            Ok(Some(0))
        } else {
            Ok(None)
        }
    }

    fn record_import(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
        imported_content_hash: &str,
        _local_entry_id: i64,
        _now: OffsetDateTime,
    ) -> Result<(), PeerImageImportPersistenceError> {
        let mut state = self.state.lock();
        state.imports.insert((
            peer_id.to_string(),
            remote_entry_id.to_string(),
            imported_content_hash.to_string(),
        ));
        Ok(())
    }

    fn collection_name_exists(&self, name: &str) -> Result<bool, PeerImageImportPersistenceError> {
        let normalised = name.trim().to_lowercase();
        let exists = self
            .state
            .lock()
            .collections
            .values()
            .any(|collection| collection.name.trim().to_lowercase() == normalised);
        Ok(exists)
    }

    fn create_user_collection(
        &self,
        name: &str,
        color_hex: &str,
        now: OffsetDateTime,
    ) -> Result<i64, PeerImageImportPersistenceError> {
        let mut state = self.state.lock();
        let id = state.next_collection_id;
        state.next_collection_id += 1;
        let ts = now
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
        state.collections.insert(
            id,
            Collection {
                id,
                stable_key: None,
                name: name.to_string(),
                kind: CollectionKind::User,
                color_hex: color_hex.to_string(),
                created_at: ts.clone(),
                updated_at: ts,
                is_peer_bound: true,
                peer_display_name: Some(name.to_string()),
            },
        );
        Ok(id)
    }

    fn find_collection(
        &self,
        collection_id: i64,
    ) -> Result<Option<Collection>, PeerImageImportPersistenceError> {
        Ok(self.state.lock().collections.get(&collection_id).cloned())
    }

    fn stage_image_asset(
        &self,
        bytes: &[u8],
        _hint_remote_entry_id: &str,
    ) -> Result<StagedImageAsset, PeerImageImportPersistenceError> {
        // Validate PNG signature and decode the frame so the
        // dimensions the helper reports agree with the bytes
        // the host returned.
        let image = crate::clipboard_assets::decode_png(bytes).map_err(|error| {
            PeerImageImportPersistenceError::Asset(format!("decode: {error:?}"))
        })?;
        let width = image.width();
        let height = image.height();
        let normalized = crate::clipboard_assets::normalize_image(&image).map_err(|error| {
            PeerImageImportPersistenceError::Asset(format!("normalize: {error:?}"))
        })?;
        let hash = crate::clipboard_assets::sha256_hex(bytes);
        let asset_ref = crate::clipboard_assets::asset_ref_for_hash(&hash);
        let mut state = self.state.lock();
        // The in-memory adapter mirrors the production asset
        // store contract: an asset that the adapter already
        // serves from its in-memory map is treated as
        // `Reused` (a pre-existing snapshot the rollback path
        // must preserve); a brand-new asset is treated as
        // `Written` (safe to delete when the surrounding
        // transaction rolls back).
        let kind = if state.asset_bytes.contains_key(&asset_ref) {
            StageKind::Reused
        } else {
            StageKind::Written
        };
        state.asset_bytes.insert(asset_ref.clone(), bytes.to_vec());
        state.staged_kinds.insert(asset_ref.clone(), kind);
        // Acquire the per-asset staging lease the rollback
        // path consults to serialise concurrent imports. The
        // lease counter survives across stages so two
        // concurrent attempts (A as `Written`, B as `Reused`)
        // both register their hold; the rollback path only
        // deletes the file once the counter drops to zero AND
        // no entry references the asset. The lease is
        // released either by a successful commit or by an
        // explicit rollback that removes the staged entry.
        *state.staged_leases.entry(asset_ref.clone()).or_insert(0) += 1;
        Ok(StagedImageAsset {
            asset_ref,
            canonical_hash: hash,
            width,
            height,
            content_size: normalized.png().len() as i64,
            kind,
        })
    }

    fn stage_source_app_icon(
        &self,
        bytes: Option<&[u8]>,
    ) -> Result<Option<StagedImageAsset>, PeerImageImportPersistenceError> {
        // The helper collapses to `None` when the host did not
        // ship an icon so the caller never has to opt into the
        // icon path; the import transaction treats the absence
        // as a typed no-op and the persisted provenance row
        // keeps its `source_app_icon_ref` column at `NULL`.
        let Some(bytes) = bytes else {
            return Ok(None);
        };
        let image = crate::clipboard_assets::decode_png(bytes).map_err(|error| {
            PeerImageImportPersistenceError::Asset(format!("icon decode: {error:?}"))
        })?;
        let normalized = crate::clipboard_assets::normalize_image(&image).map_err(|error| {
            PeerImageImportPersistenceError::Asset(format!("icon normalize: {error:?}"))
        })?;
        let hash = crate::clipboard_assets::sha256_hex(bytes);
        let asset_ref = format!(
            "{}/{hash}.png",
            crate::application_icons::APPLICATION_ICONS_ASSET_DIR
        );
        let width = image.width();
        let height = image.height();
        let mut state = self.state.lock();
        // The in-memory adapter mirrors the production
        // `application-icons/` writer: an icon the adapter
        // already serves is treated as `Reused` (the rollback
        // path MUST keep it), a brand-new icon is treated as
        // `Written` (safe to delete when the surrounding
        // transaction rolls back).
        let kind = if state.asset_bytes.contains_key(&asset_ref) {
            StageKind::Reused
        } else {
            StageKind::Written
        };
        state.asset_bytes.insert(asset_ref.clone(), bytes.to_vec());
        state.staged_kinds.insert(asset_ref.clone(), kind);
        *state.staged_leases.entry(asset_ref.clone()).or_insert(0) += 1;
        Ok(Some(StagedImageAsset {
            asset_ref,
            canonical_hash: hash,
            width,
            height,
            content_size: normalized.png().len() as i64,
            kind,
        }))
    }

    fn release_staged_asset(
        &self,
        staged: &StagedImageAsset,
    ) -> Result<(), PeerImageImportPersistenceError> {
        let mut state = self.state.lock();
        // The rollback contract the production adapter
        // honours: a [`StageKind::Reused`] asset MUST stay in
        // the in-memory store even when the surrounding
        // transaction rolls back. The check runs BEFORE any
        // reference count so a reused asset referenced by
        // another row stays put (the dedupe path).
        if staged.kind == StageKind::Reused {
            // Drop the staged lease the holder acquired so the
            // counter stays accurate; the file itself never
            // leaves the store.
            decrement_staged_lease(&mut state, &staged.asset_ref);
            return Ok(());
        }
        // Concurrent-import guard: a freshly-written asset
        // can be removed ONLY when no other in-flight stage
        // still references it AND no `clipboard_entries` row
        // owns the asset. The check runs BEFORE the file
        // removal so a concurrent import that is about to
        // commit can never lose its reference to the asset.
        let lease_holders = state
            .staged_leases
            .get(&staged.asset_ref)
            .copied()
            .unwrap_or(0);
        if lease_holders > 1 {
            // Another import still holds the staged asset;
            // release our lease and leave the file in place so
            // the other attempt can commit against it.
            decrement_staged_lease(&mut state, &staged.asset_ref);
            return Ok(());
        }
        decrement_staged_lease(&mut state, &staged.asset_ref);
        state.staged_kinds.remove(&staged.asset_ref);
        state.asset_bytes.remove(&staged.asset_ref);
        Ok(())
    }

    fn read_image_bytes(
        &self,
        asset_ref: &str,
    ) -> Result<Vec<u8>, PeerImageImportPersistenceError> {
        let state = self.state.lock();
        state
            .asset_bytes
            .get(asset_ref)
            .cloned()
            .ok_or(PeerImageImportPersistenceError::Asset(format!(
                "missing asset bytes for {asset_ref}"
            )))
    }

    fn read_source_app_icon(
        &self,
        asset_ref: &str,
    ) -> Result<Option<Vec<u8>>, PeerImageImportPersistenceError> {
        if !crate::application_icons::is_safe_icon_ref(asset_ref) {
            return Ok(None);
        }
        Ok(self.state.lock().asset_bytes.get(asset_ref).cloned())
    }

    fn validate_image_metadata(
        &self,
        width: u32,
        height: u32,
        byte_size: i64,
    ) -> Result<(), PeerImageImportPersistenceError> {
        if width == 0 || height == 0 {
            return Err(PeerImageImportPersistenceError::Asset(format!(
                "invalid dimensions {width}x{height}"
            )));
        }
        if byte_size <= 0 {
            return Err(PeerImageImportPersistenceError::Asset(format!(
                "invalid byte size {byte_size}"
            )));
        }
        if (byte_size as usize) > IMPORT_MAX_IMAGE_BYTES {
            return Err(PeerImageImportPersistenceError::Asset(format!(
                "byte size {byte_size} exceeds the {} byte cap",
                IMPORT_MAX_IMAGE_BYTES
            )));
        }
        Ok(())
    }

    fn commit_import_transaction(
        &self,
        spec: ImageImportTransactionSpec,
    ) -> Result<ImageImportTransactionOutcome, PeerImageImportPersistenceError> {
        // The in-memory adapter uses a single
        // [`parking_lot::Mutex`] for the whole state. A second
        // mutex inside the commit would only complicate the
        // rollback path without adding atomicity guarantees the
        // test surface needs. The production adapter opens a
        // proper SQLite transaction instead.
        let mut state = self.state.lock();
        let hash = spec.staged.canonical_hash.clone();
        // Release the staged leases the helpers acquired
        // during staging so the rollback path's per-asset
        // counters reflect the import has moved past the
        // staging step. The entries the commit inserts below
        // take ownership of the asset references, so the
        // bytes themselves stay in the in-memory store
        // regardless of the lease counter.
        decrement_staged_lease(&mut state, &spec.staged.asset_ref);
        let source_app_icon_ref = spec
            .source_app_icon
            .as_ref()
            .map(|staged| staged.asset_ref.clone());
        if let Some(icon) = spec.source_app_icon.as_ref() {
            decrement_staged_lease(&mut state, &icon.asset_ref);
        }
        let existing_entry_id = state.by_hash.get(&hash).copied();
        let entry_id = match existing_entry_id {
            Some(id) => {
                let record = state
                    .entries
                    .get_mut(&id)
                    .expect("entry referenced by hash index must exist");
                record.last_seen_at = spec
                    .now
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
                id
            }
            None => {
                let id = state.next_entry_id;
                state.next_entry_id += 1;
                let now_string = spec
                    .now
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
                let record = EntryRecord {
                    id,
                    content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
                    content_type: ContentType::Image,
                    content_size: spec.staged.content_size,
                    content_hash: hash.clone(),
                    source_app: None,
                    is_pinned: false,
                    created_at: now_string.clone(),
                    updated_at: now_string,
                    last_seen_at: spec
                        .now
                        .format(&time::format_description::well_known::Rfc3339)
                        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string()),
                    title: spec.validated_title.clone(),
                    source_app_name: None,
                    source_app_icon_ref: None,
                    asset_ref: Some(spec.staged.asset_ref.clone()),
                    mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
                    payload_width: Some(spec.staged.width),
                    payload_height: Some(spec.staged.height),
                    rich_text_hash: None,
                    rich_html_ref: None,
                    rich_rtf_ref: None,
                    rich_preview_ref: None,
                    rich_html_size: None,
                    rich_rtf_size: None,
                    code_language: None,
                };
                state.entries.insert(id, record);
                state.by_hash.insert(hash.clone(), id);
                id
            }
        };

        // Resolve the peer binding. The collection helper is the
        // exact same routine the production adapter runs.
        let collection_id = match state.bindings.get(&spec.peer_id).copied() {
            Some(id) => id,
            None => {
                let trimmed = spec.display_name.trim();
                let base_name = if trimmed.is_empty() {
                    "(equipo)".to_string()
                } else {
                    trimmed.to_string()
                };
                let mut candidate = base_name.clone();
                loop {
                    let exists = state
                        .collections
                        .values()
                        .any(|c| c.name.trim().to_lowercase() == candidate.trim().to_lowercase());
                    if !exists {
                        break;
                    }
                    let suffix = " (equipo)";
                    let combined = format!("{base_name}{suffix}");
                    let cap = clipvault_db::MAX_ORGANIZATION_NAME_CHARS;
                    let chars: Vec<char> = combined.chars().collect();
                    candidate = if chars.len() > cap {
                        chars.into_iter().take(cap).collect()
                    } else {
                        combined
                    };
                    if candidate == base_name {
                        break;
                    }
                }
                let id = state.next_collection_id;
                state.next_collection_id += 1;
                let ts = spec
                    .now
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
                state.collections.insert(
                    id,
                    Collection {
                        id,
                        stable_key: None,
                        name: candidate,
                        kind: CollectionKind::User,
                        color_hex: clipvault_db::HISTORY_DEFAULT_COLOR_HEX.to_string(),
                        created_at: ts.clone(),
                        updated_at: ts,
                        is_peer_bound: true,
                        peer_display_name: Some(spec.display_name.clone()),
                    },
                );
                state.bindings.insert(spec.peer_id.clone(), id);
                id
            }
        };

        // Membership and provenance. The collection attachment
        // is idempotent because the membership table uses
        // `(entry_id, collection_id)` as a composite key; the
        // provenance row uses
        // `(peer_id, remote_entry_id, imported_content_hash)`
        // and the insert collapses to a no-op on a duplicate.
        // The provenance_source_app map carries the
        // per-provenance source-app name + icon ref the
        // `peer-source-app-presentation` change writes
        // atomically with the rest of the transaction. The
        // existing-entry path deliberately keeps the entry's
        // `source_app_name` / `source_app_icon_ref` columns
        // untouched — the per-provenance metadata lives in
        // its own map keyed by the composite provenance.
        state.imports.insert((
            spec.peer_id.clone(),
            spec.remote_entry_id.clone(),
            hash.clone(),
        ));
        state.provenance_source_app.insert(
            (
                spec.peer_id.clone(),
                spec.remote_entry_id.clone(),
                hash.clone(),
            ),
            (spec.source_app_name.clone(), source_app_icon_ref.clone()),
        );

        Ok(ImageImportTransactionOutcome {
            entry_id,
            collection_id,
            deduplicated: existing_entry_id.is_some(),
        })
    }
}

/// Trusted / active cache the importer consults before dialing.
#[derive(Debug, Clone, Copy, Default)]
pub struct PeerImageImportTrustState {
    pub trusted: bool,
    pub active: bool,
}

/// Lightweight per-peer capability resolver the import facade
/// consults before dialing the productive mTLS transport. The
/// resolver returns `true` for every peer that advertised the
/// `image_import` capability through mDNS and `false` for
/// peers that only ship the legacy `pairing` contract. The
/// default resolver returns `true` for every peer so the
/// bootstrap is the only place that wires a SQLite-backed
/// resolver; tests inject deterministic accept / reject
/// closures.
pub type PeerImageCapabilityResolver = Arc<dyn Fn(&str) -> bool + Send + Sync>;

fn default_image_capability_resolver() -> PeerImageCapabilityResolver {
    Arc::new(|_| true)
}

/// Service the shell drives.
#[derive(Clone)]
pub struct PeerImageImportService {
    transport: Arc<dyn PeerFetchImageTransport>,
    persistence: Arc<dyn PeerImageImportPersistence>,
    clock: Arc<dyn ImageImportClock>,
    trust_state: Arc<Mutex<HashMap<String, PeerImageImportTrustState>>>,
    capability_resolver: PeerImageCapabilityResolver,
}

impl PeerImageImportService {
    pub fn new(
        transport: Arc<dyn PeerFetchImageTransport>,
        persistence: Arc<dyn PeerImageImportPersistence>,
        clock: Arc<dyn ImageImportClock>,
    ) -> Self {
        Self {
            transport,
            persistence,
            clock,
            trust_state: Arc::new(Mutex::new(HashMap::new())),
            capability_resolver: default_image_capability_resolver(),
        }
    }

    /// Replace the per-peer capability resolver the import facade
    /// consults before dialling the productive mTLS transport.
    /// The bootstrap installs the SQLite-backed resolver the
    /// pairing persistence owns so a peer that did not advertise
    /// `image_import` collapses to the typed `peer_unavailable`
    /// outcome with reason `not_available` instead of leaking
    /// the import request over the wire.
    pub fn with_capability_resolver(mut self, resolver: PeerImageCapabilityResolver) -> Self {
        self.capability_resolver = resolver;
        self
    }

    pub fn record_peer_state(&self, peer_id: &str, state: PeerImageImportTrustState) {
        self.trust_state.lock().insert(peer_id.to_string(), state);
    }

    pub fn forget_peer(&self, peer_id: &str) {
        self.trust_state.lock().remove(peer_id);
    }

    pub fn peer_state(&self, peer_id: &str) -> Option<PeerImageImportTrustState> {
        self.trust_state.lock().get(peer_id).copied()
    }

    /// Import a single image row of a trusted, active peer.
    pub fn import(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        remote_entry_id: &str,
        display_name: &str,
    ) -> PeerImageImportOutcome {
        match self.peer_state(peer_id) {
            None => {
                return PeerImageImportOutcome::PeerUnavailable {
                    reason: "no_known_peer",
                };
            }
            Some(state) if !state.trusted => {
                return PeerImageImportOutcome::PeerUnavailable {
                    reason: "not_trusted",
                };
            }
            Some(state) if !state.active => {
                return PeerImageImportOutcome::PeerUnavailable {
                    reason: "not_active",
                };
            }
            Some(_) => {}
        }
        if !(self.capability_resolver)(peer_id) {
            return PeerImageImportOutcome::PeerUnavailable {
                reason: "not_available",
            };
        }
        let request = PeerFetchImageRequest {
            peer_id: peer_id.to_string(),
            cert_fingerprint: cert_fingerprint.to_string(),
            remote_entry_id: remote_entry_id.to_string(),
        };
        match self.transport.fetch_image(request) {
            Ok(response) => self.commit(peer_id, display_name, response),
            Err(error) => match error {
                PeerFetchImageTransportError::Revoked
                | PeerFetchImageTransportError::Blocked
                | PeerFetchImageTransportError::KeyMismatch
                | PeerFetchImageTransportError::UnknownPeer => {
                    PeerImageImportOutcome::PeerUnavailable {
                        reason: "not_trusted",
                    }
                }
                PeerFetchImageTransportError::NotTransferable => {
                    PeerImageImportOutcome::NotTransferable
                }
                PeerFetchImageTransportError::BodyTooLarge => PeerImageImportOutcome::BodyTooLarge,
                PeerFetchImageTransportError::PersistenceUnavailable => {
                    PeerImageImportOutcome::PersistenceError {
                        reason: error.reason(),
                    }
                }
                PeerFetchImageTransportError::Unavailable
                | PeerFetchImageTransportError::PeerUnresolved
                | PeerFetchImageTransportError::IncompatibleProtocol
                | PeerFetchImageTransportError::Malformed => {
                    PeerImageImportOutcome::TransportUnavailable {
                        reason: error.reason(),
                    }
                }
            },
        }
    }

    fn commit(
        &self,
        peer_id: &str,
        display_name: &str,
        response: PeerFetchImageResponse,
    ) -> PeerImageImportOutcome {
        // Local re-validation. The transport enforces the cap
        // but a drifted host could still try to ship a body
        // larger than the documented limit; the importer
        // re-checks the size before persisting anything.
        if response.bytes.is_empty() {
            return PeerImageImportOutcome::NotTransferable;
        }
        if response.bytes.len() > IMPORT_MAX_IMAGE_BYTES {
            return PeerImageImportOutcome::BodyTooLarge;
        }

        // Title validation.
        let validated_title = match crate::history::TextHistoryService::validate_title(
            response.title.as_deref().unwrap_or(""),
        ) {
            Ok(Some(value)) => Some(value),
            Ok(None) => None,
            Err(_) => return PeerImageImportOutcome::TitleInvalid,
        };

        let now = self.clock.now();

        // Stage the asset through the asset store so the
        // canonical hash and `asset_ref` are computed before any
        // SQLite mutation. The staged asset MUST be released if
        // anything below fails so a rolled-back transaction does
        // not leave a temporary behind.
        let staged = match self
            .persistence
            .stage_image_asset(&response.bytes, &response.remote_entry_id)
        {
            Ok(staged) => staged,
            Err(error) => {
                return PeerImageImportOutcome::PersistenceError {
                    reason: match error {
                        PeerImageImportPersistenceError::Sqlite(_) => "sqlite",
                        PeerImageImportPersistenceError::Organization(_) => "organization",
                        PeerImageImportPersistenceError::UnknownCollection(_) => {
                            "unknown_collection"
                        }
                        PeerImageImportPersistenceError::Asset(_) => "asset_error",
                    },
                };
            }
        };

        // Stage the source-app icon (when present) through the
        // dedicated `application-icons/` writer. The helper
        // validates the bytes independently of the image, so a
        // peer that ships a valid image + an invalid icon
        // collapses to a no-op icon and the import still
        // succeeds. The staged icon MUST be released if the
        // surrounding transaction rolls back.
        let source_app_name = response.source_app_name.as_deref().and_then(|name| {
            crate::peer_source_app_presentation::validate_source_app_name(name).ok()
        });
        let valid_source_app_icon = response.source_app_icon_bytes.as_deref().filter(|bytes| {
            crate::peer_source_app_presentation::validate_source_app_icon(bytes).is_ok()
        });
        // Presentation metadata is optional. Invalid bytes or an icon-store
        // failure must not roll back a valid user-requested image import.
        let staged_icon = valid_source_app_icon.and_then(|bytes| {
            self.persistence
                .stage_source_app_icon(Some(bytes))
                .ok()
                .flatten()
        });

        let result = self.commit_after_staging(
            peer_id,
            display_name,
            &response,
            &staged,
            staged_icon.as_ref(),
            source_app_name.as_deref(),
            validated_title,
            now,
        );

        // Rollback: only the freshly staged assets are removed.
        // A pre-existing asset that happens to share the hash
        // was already on disk before this import began and the
        // staging helper would have returned a `Reused` outcome
        // (no temporary created). The persistence helper only
        // removes the file when no entry references it, so a
        // shared asset stays put even when this call rolled
        // back.
        if !matches!(result, PeerImageImportOutcome::Imported { .. }) {
            let _ = self.persistence.release_staged_asset(&staged);
            if let Some(icon) = staged_icon.as_ref() {
                let _ = self.persistence.release_staged_asset(icon);
            }
        }

        result
    }

    fn commit_after_staging(
        &self,
        peer_id: &str,
        display_name: &str,
        response: &PeerFetchImageResponse,
        staged: &StagedImageAsset,
        staged_icon: Option<&StagedImageAsset>,
        source_app_name: Option<&str>,
        validated_title: Option<String>,
        now: OffsetDateTime,
    ) -> PeerImageImportOutcome {
        // The whole sequence (binding lookup / creation, entry
        // insert or reuse, membership attach, provenance record,
        // source-app name + icon ref) runs through a single
        // SQLite transaction so a failure anywhere rolls the
        // import back atomically. The persistence helper is
        // responsible for opening the transaction and committing
        // it; the importer only supplies the validated inputs
        // and consumes the typed outcome.
        let spec = ImageImportTransactionSpec {
            peer_id: peer_id.to_string(),
            remote_entry_id: response.remote_entry_id.clone(),
            display_name: display_name.to_string(),
            staged: staged.clone(),
            validated_title,
            source_app_name: source_app_name.map(str::to_string),
            source_app_icon: staged_icon.cloned(),
            now,
        };
        match self.persistence.commit_import_transaction(spec) {
            Ok(outcome) => PeerImageImportOutcome::Imported {
                entry_id: outcome.entry_id,
                collection_id: outcome.collection_id,
                deduplicated: outcome.deduplicated,
            },
            Err(error) => PeerImageImportOutcome::PersistenceError {
                reason: persistence_reason(&error),
            },
        }
    }
}

impl PeerFetchImageResponse {
    /// Borrow the opaque remote entry id the host minted.
    /// Mirrors the [`crate::peer_text_import::PeerFetchResponse::remote_entry_id`]
    /// helper.
    pub fn remote_entry_id(&self) -> &str {
        &self.remote_entry_id
    }
}

fn persistence_reason(error: &PeerImageImportPersistenceError) -> &'static str {
    match error {
        PeerImageImportPersistenceError::Sqlite(_) => "sqlite",
        PeerImageImportPersistenceError::Organization(_) => "organization",
        PeerImageImportPersistenceError::UnknownCollection(_) => "unknown_collection",
        PeerImageImportPersistenceError::Asset(_) => "asset_error",
    }
}

/// Productive adapter that delegates to the
/// [`crate::peer_pairing::PeerTransport`] the bootstrap already
/// installed.
#[cfg(feature = "local-peer-pairing-tls")]
pub struct PeerPairingFetchImageTransportAdapter {
    inner: Arc<dyn crate::peer_pairing::PeerTransport>,
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerPairingFetchImageTransportAdapter {
    pub fn new(inner: Arc<dyn crate::peer_pairing::PeerTransport>) -> Self {
        Self { inner }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerFetchImageTransport for PeerPairingFetchImageTransportAdapter {
    fn fetch_image(
        &self,
        request: PeerFetchImageRequest,
    ) -> Result<PeerFetchImageResponse, PeerFetchImageTransportError> {
        match self.inner.fetch_image(
            &request.peer_id,
            &request.cert_fingerprint,
            &request.remote_entry_id,
        ) {
            Ok(snapshot) => {
                if snapshot.peer_id != request.peer_id {
                    return Err(PeerFetchImageTransportError::UnknownPeer);
                }
                if snapshot.remote_entry_id != request.remote_entry_id {
                    return Err(PeerFetchImageTransportError::Malformed);
                }
                Ok(PeerFetchImageResponse {
                    peer_id: snapshot.peer_id,
                    remote_entry_id: snapshot.remote_entry_id,
                    title: snapshot.title,
                    bytes: snapshot.bytes,
                    source_app_name: snapshot.source_app_name,
                    source_app_icon_bytes: snapshot.source_app_icon_bytes,
                })
            }
            Err(error) => Err(map_pairing_image_fetch_transport_error(error)),
        }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
fn map_pairing_image_fetch_transport_error(
    error: crate::peer_pairing::TransportError,
) -> PeerFetchImageTransportError {
    use crate::peer_pairing::TransportError as Pairing;
    match error {
        Pairing::UnknownPeer => PeerFetchImageTransportError::UnknownPeer,
        Pairing::KeyMismatch => PeerFetchImageTransportError::KeyMismatch,
        Pairing::Revoked => PeerFetchImageTransportError::Revoked,
        Pairing::Blocked => PeerFetchImageTransportError::Blocked,
        Pairing::IncompatibleProtocol => PeerFetchImageTransportError::IncompatibleProtocol,
        Pairing::Malformed => PeerFetchImageTransportError::Malformed,
        Pairing::PeerUnresolved => PeerFetchImageTransportError::PeerUnresolved,
        Pairing::Unavailable => PeerFetchImageTransportError::Unavailable,
        Pairing::AlreadyRunning
        | Pairing::NotRunning
        | Pairing::Crypto
        | Pairing::InvalidCursor => PeerFetchImageTransportError::Unavailable,
        Pairing::BodyTooLarge => PeerFetchImageTransportError::BodyTooLarge,
    }
}

/// Noop transport the tests use when the assertion does not
/// exercise the dial path.
pub struct NoopPeerFetchImageTransport;

impl PeerFetchImageTransport for NoopPeerFetchImageTransport {
    fn fetch_image(
        &self,
        _request: PeerFetchImageRequest,
    ) -> Result<PeerFetchImageResponse, PeerFetchImageTransportError> {
        Err(PeerFetchImageTransportError::Unavailable)
    }
}

/// Productive host-side adapter that implements
/// [`clipvault_platform::peer_transport::FetchImageHostHandler`]
/// and delegates every inbound `fetch_image` request to the
/// [`PeerImageImportPersistence`] the bootstrap installed.
///
/// The adapter carries a per-peer capability resolver that
/// the host consults before serving the bytes: a caller that
/// did not advertise the `image_import` capability through mDNS
/// collapses to the typed `NotTransferable` outcome so the host
/// never serves image bytes to a peer that did not opt into the
/// contract. The default resolver accepts every peer; the
/// bootstrap always replaces it with the SQLite-backed resolver
/// the pairing persistence owns.
#[cfg(feature = "local-peer-pairing-tls")]
pub struct PeerImageImportHostHandlerAdapter {
    persistence: Arc<dyn PeerImageImportPersistence>,
    capability_resolver: PeerImageCapabilityResolver,
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerImageImportHostHandlerAdapter {
    pub fn new(persistence: Arc<dyn PeerImageImportPersistence>) -> Self {
        Self {
            persistence,
            capability_resolver: default_image_capability_resolver(),
        }
    }

    /// Install the per-peer capability resolver the host
    /// consults before serving the PNG payload. The bootstrap
    /// passes a closure that resolves the `capability` column
    /// the pairing persistence owns so the wire contract stays
    /// consistent with the client-side check the
    /// [`PeerImageImportService::import`] façade applies.
    pub fn with_capability_resolver(mut self, resolver: PeerImageCapabilityResolver) -> Self {
        self.capability_resolver = resolver;
        self
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl clipvault_platform::peer_transport::FetchImageHostHandler
    for PeerImageImportHostHandlerAdapter
{
    fn fetch_image(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
    ) -> clipvault_platform::peer_transport::HostImageFetchResponse {
        if !(self.capability_resolver)(peer_id) {
            // A peer that did not advertise `image_import` cannot
            // import bytes through the listener. Surface the
            // `NotTransferable` outcome so the importer collapses
            // the call into a typed `peer_unavailable` reason
            // without persisting anything.
            return clipvault_platform::peer_transport::HostImageFetchResponse::NotTransferable;
        }
        let local_id = match decode_remote_entry_id(remote_entry_id) {
            Some(id) => id,
            None => return clipvault_platform::peer_transport::HostImageFetchResponse::NotFound,
        };
        let entry = match self.persistence.fetch_entry(local_id) {
            Ok(Some(entry)) => entry,
            Ok(None) => return clipvault_platform::peer_transport::HostImageFetchResponse::NotFound,
            Err(_) => {
                return clipvault_platform::peer_transport::HostImageFetchResponse::PersistenceUnavailable
            }
        };
        if !image_entry_is_transferable_host(&entry) {
            return clipvault_platform::peer_transport::HostImageFetchResponse::NotTransferable;
        }
        let asset_ref = match entry.asset_ref.as_deref() {
            Some(value) => value,
            None => {
                return clipvault_platform::peer_transport::HostImageFetchResponse::NotTransferable
            }
        };
        // Re-validate the metadata the projection accepted so a
        // drifted / oversized asset the listing time stamp could
        // have missed is rejected here. The persistence adapter
        // keeps the helper metadata-only and bounds-checked
        // against the documented limits.
        if let Err(_) = self.persistence.validate_image_metadata(
            entry.payload_width.unwrap_or(0),
            entry.payload_height.unwrap_or(0),
            entry.content_size,
        ) {
            return clipvault_platform::peer_transport::HostImageFetchResponse::NotTransferable;
        }
        // Read the canonical PNG through the asset store the
        // adapter wraps. The validator (resolve / read_bytes)
        // covers empty / absolute / traversal / symlink-escape /
        // not-found / not-a-png / oversized cases in a single
        // call so we do not have to repeat the checks here.
        match self.persistence.read_image_bytes(asset_ref) {
            Ok(bytes) => {
                if bytes.len() > clipvault_platform::peer_transport::FETCH_IMAGE_MAX_BODY_BYTES {
                    return clipvault_platform::peer_transport::HostImageFetchResponse::BodyTooLarge;
                }
                let source_app_name = entry.source_app_name.as_deref().and_then(|name| {
                    crate::peer_source_app_presentation::validate_source_app_name(name).ok()
                });
                let source_app_icon_bytes = entry
                    .source_app_icon_ref
                    .as_deref()
                    .filter(|icon_ref| crate::application_icons::is_safe_icon_ref(icon_ref))
                    .and_then(|icon_ref| {
                        self.persistence
                            .read_source_app_icon(icon_ref)
                            .ok()
                            .flatten()
                    })
                    .filter(|icon| {
                        crate::peer_source_app_presentation::validate_source_app_icon(icon).is_ok()
                    });
                clipvault_platform::peer_transport::HostImageFetchResponse::Ok {
                    title: entry.title,
                    bytes,
                    source_app_name,
                    source_app_icon_bytes,
                }
            }
            Err(_) => clipvault_platform::peer_transport::HostImageFetchResponse::NotTransferable,
        }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
fn decode_remote_entry_id(remote_entry_id: &str) -> Option<i64> {
    remote_entry_id
        .strip_prefix("entry-")
        .and_then(|rest| rest.parse::<i64>().ok())
        .filter(|id| *id > 0)
}

/// Mirror of [`crate::peer_image_history::image_entry_is_transferable`]
/// that gates the host handler so the wire contract stays
/// consistent.
#[cfg(feature = "local-peer-pairing-tls")]
fn image_entry_is_transferable_host(entry: &EntryRecord) -> bool {
    crate::peer_image_history::image_entry_is_transferable(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc as StdArc;

    use crate::clipboard_assets::{decode_png, normalize_image};

    fn fixed_clock(now: OffsetDateTime) -> StdArc<dyn ImageImportClock> {
        struct Fixed(OffsetDateTime);
        impl ImageImportClock for Fixed {
            fn now(&self) -> OffsetDateTime {
                self.0
            }
        }
        StdArc::new(Fixed(now))
    }

    fn build_png(width: u32, height: u32) -> Vec<u8> {
        use png::{BitDepth, ColorType, Encoder};
        let mut data = Vec::new();
        {
            let mut encoder = Encoder::new(&mut data, width, height);
            encoder.set_color(ColorType::Rgba);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header().expect("header");
            let bytes_per_pixel = 4;
            let stride = width as usize * bytes_per_pixel;
            let mut image_data = vec![0u8; stride * height as usize];
            for y in 0..height as usize {
                for x in 0..width as usize {
                    let offset = y * stride + x * bytes_per_pixel;
                    image_data[offset] = 0x10;
                    image_data[offset + 1] = 0x20;
                    image_data[offset + 2] = 0x30;
                    image_data[offset + 3] = 0xFF;
                }
            }
            writer.write_image_data(&image_data).expect("write");
        }
        data
    }

    fn entry_record(id: i64, hash: &str, title: Option<String>) -> EntryRecord {
        EntryRecord {
            id,
            content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
            content_type: ContentType::Image,
            content_size: 64,
            content_hash: hash.to_string(),
            source_app: None,
            is_pinned: false,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            last_seen_at: "2026-01-01T00:00:00Z".to_string(),
            title,
            source_app_name: None,
            source_app_icon_ref: None,
            asset_ref: Some(format!("clipboard/{}.png", hash)),
            mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
            payload_width: Some(8),
            payload_height: Some(4),
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
            code_language: None,
        }
    }

    fn ok_response(bytes: Vec<u8>, remote_entry_id: &str) -> PeerFetchImageResponse {
        PeerFetchImageResponse {
            peer_id: "peer-a".to_string(),
            remote_entry_id: remote_entry_id.to_string(),
            title: None,
            bytes,
            source_app_name: None,
            source_app_icon_bytes: None,
        }
    }

    #[test]
    fn import_returns_peer_unavailable_when_state_missing() {
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        let transport = StdArc::new(NoopPeerFetchImageTransport);
        let service = PeerImageImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        assert!(matches!(
            outcome,
            PeerImageImportOutcome::PeerUnavailable {
                reason: "no_known_peer"
            }
        ));
    }

    #[test]
    fn import_returns_body_too_large() {
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        let big = vec![0u8; IMPORT_MAX_IMAGE_BYTES + 1];
        let transport = StdArc::new(ScriptedFetchImageTransport::new(Ok(ok_response(
            big, "entry-1",
        ))));
        let service = PeerImageImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImageImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        assert!(matches!(outcome, PeerImageImportOutcome::BodyTooLarge));
    }

    #[test]
    fn import_commits_first_image() {
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        let bytes = build_png(16, 16);
        let mut response = ok_response(bytes, "entry-1");
        response.source_app_name = Some("  Screenshot App  ".to_string());
        response.source_app_icon_bytes = Some(build_png(24, 24));
        let transport = StdArc::new(ScriptedFetchImageTransport::new(Ok(response)));
        let service = PeerImageImportService::new(
            transport,
            persistence.clone(),
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImageImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let PeerImageImportOutcome::Imported {
            entry_id,
            collection_id,
            deduplicated,
        } = outcome
        else {
            panic!("expected Imported");
        };
        assert!(entry_id > 0);
        assert!(collection_id > 0);
        assert!(!deduplicated);
        let record = service
            .persistence
            .fetch_entry(entry_id)
            .expect("fetch")
            .expect("entry exists");
        assert_eq!(record.content_type, ContentType::Image);
        assert_eq!(
            record.asset_ref.as_deref(),
            Some(format!("clipboard/{}.png", record.content_hash).as_str())
        );
        let provenance = persistence
            .source_app_for("peer-a", "entry-1", &record.content_hash)
            .expect("source-app provenance recorded");
        assert_eq!(provenance.0.as_deref(), Some("Screenshot App"));
        assert!(provenance
            .1
            .as_deref()
            .is_some_and(|icon_ref| icon_ref.starts_with("application-icons/")));
    }

    #[test]
    fn invalid_source_app_metadata_does_not_fail_image_import() {
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        let mut response = ok_response(build_png(16, 16), "entry-1");
        response.source_app_name = Some("Bad\nName".to_string());
        response.source_app_icon_bytes = Some(vec![1, 2, 3, 4]);
        let transport = StdArc::new(ScriptedFetchImageTransport::new(Ok(response)));
        let service = PeerImageImportService::new(
            transport,
            persistence.clone(),
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImageImportTrustState {
                trusted: true,
                active: true,
            },
        );

        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let PeerImageImportOutcome::Imported { entry_id, .. } = outcome else {
            panic!("invalid optional app metadata must not fail a valid image import");
        };
        let entry = service
            .persistence
            .fetch_entry(entry_id)
            .expect("entry lookup")
            .expect("imported image exists");
        assert_eq!(
            persistence.source_app_for("peer-a", "entry-1", &entry.content_hash),
            Some((None, None))
        );
    }

    #[test]
    fn import_dedupes_when_bytes_repeat() {
        let bytes = build_png(8, 8);
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        let transport_a = StdArc::new(ScriptedFetchImageTransport::new(Ok(ok_response(
            bytes.clone(),
            "entry-1",
        ))));
        let service_a = PeerImageImportService::new(
            transport_a,
            persistence.clone(),
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service_a.record_peer_state(
            "peer-a",
            PeerImageImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let first = service_a.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let transport_b = StdArc::new(ScriptedFetchImageTransport::new(Ok(ok_response(
            bytes, "entry-1",
        ))));
        let service_b = PeerImageImportService::new(
            transport_b,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service_b.record_peer_state(
            "peer-a",
            PeerImageImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let second = service_b.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let (
            PeerImageImportOutcome::Imported {
                entry_id: first_entry,
                deduplicated: first_dedup,
                ..
            },
            PeerImageImportOutcome::Imported {
                entry_id: second_entry,
                deduplicated: second_dedup,
                ..
            },
        ) = (first, second)
        else {
            panic!("expected two Imported outcomes");
        };
        assert_eq!(first_entry, second_entry);
        assert!(!first_dedup);
        assert!(second_dedup);
    }

    #[test]
    fn import_keeps_existing_title_for_reused_entry() {
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        let bytes = build_png(8, 8);
        // Stage the bytes once so the canonical hash is known.
        let staged = persistence
            .stage_image_asset(&bytes, "entry-1")
            .expect("stage");
        let mut existing = entry_record(42, &staged.canonical_hash, Some("Original".to_string()));
        existing.content_size = bytes.len() as i64;
        persistence.seed_entries(vec![existing.clone()]);
        let transport = StdArc::new(ScriptedFetchImageTransport::new(Ok(ok_response(
            bytes, "entry-1",
        ))));
        let service = PeerImageImportService::new(
            transport,
            persistence.clone(),
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImageImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let PeerImageImportOutcome::Imported { entry_id, .. } = outcome else {
            panic!("expected Imported");
        };
        assert_eq!(entry_id, existing.id);
        let persisted = service
            .persistence
            .fetch_entry(entry_id)
            .expect("fetch")
            .expect("entry");
        assert_eq!(persisted.title.as_deref(), Some("Original"));
    }

    struct ScriptedFetchImageTransport {
        response: parking_lot::Mutex<Result<PeerFetchImageResponse, PeerFetchImageTransportError>>,
    }

    impl ScriptedFetchImageTransport {
        fn new(response: Result<PeerFetchImageResponse, PeerFetchImageTransportError>) -> Self {
            Self {
                response: parking_lot::Mutex::new(response),
            }
        }
    }

    impl PeerFetchImageTransport for ScriptedFetchImageTransport {
        fn fetch_image(
            &self,
            _request: PeerFetchImageRequest,
        ) -> Result<PeerFetchImageResponse, PeerFetchImageTransportError> {
            self.response.lock().clone()
        }
    }

    #[test]
    fn decode_png_round_trips_built_png() {
        let bytes = build_png(8, 4);
        let image = decode_png(&bytes).expect("decode");
        assert_eq!(image.width(), 8);
        assert_eq!(image.height(), 4);
        let _ = normalize_image(&image).expect("normalize");
    }

    #[test]
    fn staging_produces_canonical_asset_ref() {
        let bytes = build_png(2, 2);
        let persistence = InMemoryImageImportPersistence::new();
        let staged = persistence
            .stage_image_asset(&bytes, "entry-1")
            .expect("stage");
        assert!(staged.asset_ref.starts_with("clipboard/"));
        assert!(staged.asset_ref.ends_with(".png"));
        assert_eq!(staged.canonical_hash.len(), 64);
    }

    #[test]
    fn staged_asset_records_whether_it_was_written_or_reused() {
        // The first staging of a brand-new asset MUST report
        // `Written`; a second staging of identical bytes MUST
        // report `Reused` so the rollback path can preserve a
        // pre-existing asset even when the surrounding
        // transaction rolls back.
        let bytes = build_png(8, 8);
        let persistence = InMemoryImageImportPersistence::new();
        let first = persistence
            .stage_image_asset(&bytes, "entry-1")
            .expect("stage first");
        assert_eq!(first.kind, StageKind::Written);
        let second = persistence
            .stage_image_asset(&bytes, "entry-2")
            .expect("stage second");
        assert_eq!(second.kind, StageKind::Reused);
    }

    #[test]
    fn rollback_preserves_a_reused_asset_with_no_references() {
        // The rollback contract: an asset that the staging
        // helper marked as [`StageKind::Reused`] (a
        // pre-existing file on disk) MUST survive a rolled-
        // back transaction even when no `clipboard_entries`
        // row references it. The previous prototype would
        // have deleted it because `count_asset_references`
        // returned zero, silently breaking the dedupe path.
        let bytes = build_png(8, 8);
        let persistence = InMemoryImageImportPersistence::new();
        // Stage the bytes once and commit the import so the
        // asset is "pre-existing" in the in-memory store when
        // the second staging helper runs.
        let first = persistence
            .stage_image_asset(&bytes, "entry-pre-existing")
            .expect("stage pre-existing");
        let first_outcome = persistence
            .commit_import_transaction(ImageImportTransactionSpec {
                peer_id: "peer-a".to_string(),
                remote_entry_id: "entry-pre-existing".to_string(),
                display_name: "Equipo A".to_string(),
                staged: first.clone(),
                validated_title: None,
                source_app_name: None,
                source_app_icon: None,
                now: OffsetDateTime::now_utc(),
            })
            .expect("commit pre-existing");
        // Drop the entry the commit created so the asset has
        // zero references — the rollback must still keep the
        // asset because the helper flagged it as `Reused`.
        let _ = first_outcome;
        // The second staging sees the in-memory asset and
        // marks the staged value as `Reused`.
        let reused = persistence
            .stage_image_asset(&bytes, "entry-reused")
            .expect("stage reused");
        assert_eq!(reused.kind, StageKind::Reused);
        // Roll back the second attempt. The asset MUST stay
        // because the rollback contract honours the `Reused`
        // marker.
        persistence
            .release_staged_asset(&reused)
            .expect("release reused");
        // The asset is still readable through the in-memory
        // store. We exercise the read helper to confirm the
        // rollback did not delete the file.
        let read = persistence
            .read_image_bytes(&reused.asset_ref)
            .expect("read after rollback");
        assert_eq!(read, bytes);
    }

    #[test]
    fn rollback_clears_a_freshly_written_asset() {
        // Symmetric coverage: a freshly-written asset with no
        // references IS safe to delete on rollback. The helper
        // marks the staged value as `Written` and the rollback
        // removes the asset bytes from the in-memory store.
        let bytes = build_png(8, 8);
        let persistence = InMemoryImageImportPersistence::new();
        let staged = persistence
            .stage_image_asset(&bytes, "entry-1")
            .expect("stage");
        assert_eq!(staged.kind, StageKind::Written);
        persistence.release_staged_asset(&staged).expect("release");
        // The asset is gone from the in-memory store.
        let result = persistence.read_image_bytes(&staged.asset_ref);
        assert!(result.is_err());
    }

    /// Concurrent import regression coverage: two threads stage
    /// the same PNG bytes simultaneously, one as `Written` and
    /// the other as `Reused`. The thread that staged the asset
    /// as `Written` then fails its commit; the rollback path MUST
    /// keep the file on disk because the second thread's
    /// `Reused` staging lease is still active. The second thread
    /// then commits successfully; the entry MUST keep a valid
    /// `asset_ref` so the bytes the local asset store surfaces
    /// stay readable.
    ///
    /// The test uses a [`std::sync::Barrier`] to coordinate the
    /// stage / rollback / commit windows deterministically so the
    /// race the regression pins actually fires (without the
    /// barrier the runtime could schedule B before A and the
    /// asset's first `Written` stage would never happen).
    #[test]
    fn concurrent_staging_does_not_delete_an_asset_a_concurrent_reused_stage_will_commit() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let bytes = build_png(8, 8);
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        // Order the stages explicitly so A is always the first
        // writer and B is guaranteed to observe `Reused`; then
        // keep B from committing until A's rollback completes.
        let barrier_after_a_stage = Arc::new(Barrier::new(2));
        let barrier_after_b_stage = Arc::new(Barrier::new(2));
        let barrier_after_a_rollback = Arc::new(Barrier::new(2));
        let persistence_a = StdArc::clone(&persistence);
        let persistence_b = StdArc::clone(&persistence);
        let bytes_a = bytes.clone();
        let bytes_b = bytes.clone();
        let barrier_a1 = Arc::clone(&barrier_after_a_stage);
        let barrier_b1 = Arc::clone(&barrier_after_a_stage);
        let barrier_a2 = Arc::clone(&barrier_after_b_stage);
        let barrier_b2 = Arc::clone(&barrier_after_b_stage);
        let barrier_a3 = Arc::clone(&barrier_after_a_rollback);
        let barrier_b3 = Arc::clone(&barrier_after_a_rollback);
        let now = OffsetDateTime::now_utc();
        // Thread A: stage (Written), commit failure, rollback.
        let handle_a = thread::spawn(move || {
            let staged = persistence_a
                .stage_image_asset(&bytes_a, "entry-a")
                .expect("stage a");
            // Sanity: A is the first writer.
            assert_eq!(staged.kind, StageKind::Written);
            // B may stage only after A has definitely written.
            barrier_a1.wait();
            // Wait until B owns its lease before rolling back A.
            barrier_a2.wait();
            // Simulate a commit failure by NOT calling
            // `commit_import_transaction` and rolling back the
            // staged asset. The previous prototype would now
            // delete the file because the asset ref-count is
            // still zero (B has not committed yet).
            persistence_a
                .release_staged_asset(&staged)
                .expect("release a");
            // Signal that the rollback finished so B can
            // safely commit without being interrupted.
            barrier_a3.wait();
        });
        // Thread B: stage (Reused), commit (success).
        let handle_b = thread::spawn(move || {
            // A's stage is the first writer, so this stage must reuse it.
            barrier_b1.wait();
            let staged = persistence_b
                .stage_image_asset(&bytes_b, "entry-b")
                .expect("stage b");
            // Sanity: B reuses the asset A wrote.
            assert_eq!(staged.kind, StageKind::Reused);
            // Wait for A's rollback to finish before committing
            // so the test exercises the lease guard, not just
            // ordering luck.
            barrier_b2.wait();
            barrier_b3.wait();
            let outcome = persistence_b
                .commit_import_transaction(ImageImportTransactionSpec {
                    peer_id: "peer-b".to_string(),
                    remote_entry_id: "entry-b".to_string(),
                    display_name: "Equipo B".to_string(),
                    staged,
                    validated_title: None,
                    source_app_name: None,
                    source_app_icon: None,
                    now,
                })
                .expect("commit b");
            assert!(outcome.deduplicated || !outcome.deduplicated);
            outcome.entry_id
        });
        handle_a.join().expect("thread a joins");
        let b_entry_id = handle_b.join().expect("thread b joins");
        // The asset MUST still be readable through the
        // in-memory store after A's rollback. If the lease
        // guard regressed, A would have deleted the file and
        // B's entry would point at a missing asset.
        let read = persistence
            .read_image_bytes(&asset_ref_for_bytes(&bytes))
            .expect("read after rollback");
        assert_eq!(read, bytes);
        // The B entry MUST still own the canonical asset_ref so
        // the local asset store can resolve it later.
        let entry = persistence
            .fetch_entry(b_entry_id)
            .expect("fetch")
            .expect("entry exists");
        assert_eq!(
            entry.asset_ref.as_deref(),
            Some(asset_ref_for_bytes(&bytes).as_str()),
        );
    }

    fn asset_ref_for_bytes(bytes: &[u8]) -> String {
        let hash = crate::clipboard_assets::sha256_hex(bytes);
        crate::clipboard_assets::asset_ref_for_hash(&hash)
    }

    #[test]
    fn import_rejects_invalid_png() {
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        let transport = StdArc::new(ScriptedFetchImageTransport::new(Ok(
            PeerFetchImageResponse {
                peer_id: "peer-a".to_string(),
                remote_entry_id: "entry-1".to_string(),
                title: None,
                bytes: vec![0u8; 16],
                source_app_name: None,
                source_app_icon_bytes: None,
            },
        )));
        let service = PeerImageImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImageImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        // The PNG is rejected at the staging step; the
        // importer collapses the failure to `PersistenceError`
        // so the renderer never sees free-form error strings.
        assert!(matches!(
            outcome,
            PeerImageImportOutcome::PersistenceError { .. }
        ));
    }

    #[test]
    fn import_returns_peer_unavailable_when_state_not_active() {
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        let transport = StdArc::new(NoopPeerFetchImageTransport);
        let service = PeerImageImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImageImportTrustState {
                trusted: true,
                active: false,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        assert!(matches!(
            outcome,
            PeerImageImportOutcome::PeerUnavailable {
                reason: "not_active"
            }
        ));
    }

    #[test]
    fn import_returns_peer_unavailable_when_state_not_trusted() {
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        let transport = StdArc::new(NoopPeerFetchImageTransport);
        let service = PeerImageImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImageImportTrustState {
                trusted: false,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        assert!(matches!(
            outcome,
            PeerImageImportOutcome::PeerUnavailable {
                reason: "not_trusted"
            }
        ));
    }

    #[test]
    fn import_returns_peer_unavailable_when_capability_missing() {
        // Defence in depth: a trusted active peer that did not
        // advertise the `image_import` capability MUST NOT
        // trigger a network round-trip. The façade collapses
        // the request into the typed `peer_unavailable` outcome
        // with reason `not_available` so the renderer never
        // surfaces a successful Import path.
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        let transport = StdArc::new(NoopPeerFetchImageTransport);
        let resolver: PeerImageCapabilityResolver =
            StdArc::new(|peer_id| peer_id == "peer-with-capability");
        let service = PeerImageImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        )
        .with_capability_resolver(resolver);
        service.record_peer_state(
            "peer-a",
            PeerImageImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        assert!(matches!(
            outcome,
            PeerImageImportOutcome::PeerUnavailable {
                reason: "not_available"
            }
        ));
    }

    #[test]
    fn import_keeps_capability_resolver_idempotent() {
        // The capability gate runs BEFORE the transport call so
        // a peer that advertises the capability reaches the
        // productive path. The test seeds a successful
        // transport response and asserts the import commits.
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        let bytes = build_png(8, 8);
        let transport = StdArc::new(ScriptedFetchImageTransport::new(Ok(ok_response(
            bytes, "entry-1",
        ))));
        let resolver: PeerImageCapabilityResolver = StdArc::new(|peer_id| peer_id == "peer-a");
        let service = PeerImageImportService::new(
            transport,
            persistence.clone(),
            fixed_clock(OffsetDateTime::now_utc()),
        )
        .with_capability_resolver(resolver);
        service.record_peer_state(
            "peer-a",
            PeerImageImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        assert!(matches!(outcome, PeerImageImportOutcome::Imported { .. }));
    }

    #[test]
    fn import_rejects_transport_unavailable() {
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        let transport = StdArc::new(ScriptedFetchImageTransport::new(Err(
            PeerFetchImageTransportError::Unavailable,
        )));
        let service = PeerImageImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImageImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        assert!(matches!(
            outcome,
            PeerImageImportOutcome::TransportUnavailable { .. }
        ));
    }

    #[test]
    fn import_rejects_revoked_peer() {
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        let transport = StdArc::new(ScriptedFetchImageTransport::new(Err(
            PeerFetchImageTransportError::Revoked,
        )));
        let service = PeerImageImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImageImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        assert!(matches!(
            outcome,
            PeerImageImportOutcome::PeerUnavailable {
                reason: "not_trusted"
            }
        ));
    }

    #[test]
    fn import_rejects_oversize_title() {
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        let bytes = build_png(8, 8);
        let transport = StdArc::new(ScriptedFetchImageTransport::new(Ok(
            PeerFetchImageResponse {
                peer_id: "peer-a".to_string(),
                remote_entry_id: "entry-1".to_string(),
                title: Some("x".repeat(crate::history::MAX_TITLE_LENGTH + 1)),
                bytes,
                source_app_name: None,
                source_app_icon_bytes: None,
            },
        )));
        let service = PeerImageImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImageImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        assert!(matches!(outcome, PeerImageImportOutcome::TitleInvalid));
    }

    #[test]
    fn import_rejects_empty_payload() {
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        let transport = StdArc::new(ScriptedFetchImageTransport::new(Ok(
            PeerFetchImageResponse {
                peer_id: "peer-a".to_string(),
                remote_entry_id: "entry-1".to_string(),
                title: None,
                bytes: vec![],
                source_app_name: None,
                source_app_icon_bytes: None,
            },
        )));
        let service = PeerImageImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImageImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        assert!(matches!(outcome, PeerImageImportOutcome::NotTransferable));
    }

    #[test]
    fn import_creates_collection_with_collision_suffix() {
        let persistence = StdArc::new(InMemoryImageImportPersistence::new());
        // Pre-create a collection with the exact visible name.
        let _ = persistence
            .create_user_collection(
                "Equipo A",
                clipvault_db::HISTORY_DEFAULT_COLOR_HEX,
                OffsetDateTime::now_utc(),
            )
            .expect("seed");
        let bytes = build_png(8, 8);
        let transport = StdArc::new(ScriptedFetchImageTransport::new(Ok(
            PeerFetchImageResponse {
                peer_id: "peer-a".to_string(),
                remote_entry_id: "entry-1".to_string(),
                title: None,
                bytes,
                source_app_name: None,
                source_app_icon_bytes: None,
            },
        )));
        let service = PeerImageImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImageImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let PeerImageImportOutcome::Imported { collection_id, .. } = outcome else {
            panic!("expected Imported");
        };
        let collection = service
            .persistence
            .find_collection(collection_id)
            .expect("find")
            .expect("collection exists");
        assert_eq!(collection.name, "Equipo A (equipo)");
    }
}
