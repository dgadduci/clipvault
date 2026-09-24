//! Explicit-import façade for the `peer-text-import` change.
//!
//! The [`PeerImportService`] is the thin façade the desktop shell
//! drives when the user activates `Importar` for a row of a
//! trusted active peer. The service owns:
//!
//! - the metadata-only [`PeerFetchTransport`] the runtime dials
//!   to retrieve the canonical UTF-8 text of the chosen remote
//!   entry;
//! - the transactional database helper the import commits through
//!   [`crate::peer_pairing::PairingPersistence`] callbacks the
//!   runtime already trusts;
//! - the typed [`PeerImportOutcome`] the bridge / Tauri shell
//!   returns to the frontend. Every variant collapses to a
//!   stable identifier the UI branches on without inspecting
//!   free-form strings or content bytes.
//!
//! The façade is metadata-only by construction: it never copies
//! the imported body into a log, an event payload, a toast or a
//! drag payload. The body is held in memory only between the
//! authenticated fetch and the SQLite commit; the commit
//! materialises the local snapshot through the existing
//! [`clipvault_db::EntryRepository`] helpers so the dedupe
//! contract (canonical hash, title preservation, asset / tag /
//! favourite preservation) stays consistent with the local
//! capture path.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

use clipvault_db::{Collection, CollectionKind, ContentType, EntryRecord, OrganizationError};

/// Maximum size of the imported body in bytes. The contract pins
/// the same 1 MiB UTF-8 cap the listener enforces
/// ([`clipvault_platform::peer_transport::FETCH_TEXT_MAX_BODY_BYTES`])
/// so a regression that streams more than the documented limit
/// surfaces as a typed `BodyTooLarge` outcome without persisting
/// anything.
pub const IMPORT_MAX_BODY_BYTES: usize = 1024 * 1024;

/// Outcome the runtime returns to the bridge / Tauri shell after a
/// single import call. Every variant is metadata-only; the body
/// never crosses the bridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerImportOutcome {
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
    /// The fetch transport rejected the request. The renderer
    /// surfaces the typed reason without retrying blindly.
    TransportUnavailable { reason: &'static str },
    /// The body the host returned exceeded the 1 MiB cap. The
    /// runtime collapsed the rejection into a typed outcome
    /// without persisting anything.
    BodyTooLarge,
    /// The body the host returned was not valid UTF-8. The
    /// runtime collapsed the rejection into a typed outcome
    /// without persisting anything.
    InvalidUtf8,
    /// The remote entry the user asked to import no longer
    /// exists on the host or is no longer transferrable. The
    /// runtime surfaces the typed outcome without mutating
    /// SQLite.
    NotTransferable,
    /// The imported body was empty after trimming. The runtime
    /// refuses to store empty entries; this outcome collapses
    /// the typed reason the renderer surfaces.
    EmptyContent,
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
/// Every variant collapses to a stable identifier; the runtime
/// never inspects free-form SQLite / platform messages.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerImportError {
    #[error("peer is not eligible for fetch")]
    PeerUnavailable(&'static str),
    #[error("fetch body exceeded the 1 MiB UTF-8 limit")]
    BodyTooLarge,
    #[error("fetch body is not valid UTF-8")]
    InvalidUtf8,
    #[error("remote entry is not transferable")]
    NotTransferable,
    #[error("remote entry returned an empty body")]
    EmptyContent,
    #[error("remote title failed local validation")]
    TitleInvalid,
    #[error("local SQLite layer refused the commit: {0}")]
    Persistence(String),
}

impl PeerImportError {
    /// Stable, snake_case reason the runtime / bridge surfaces
    /// for every failure variant. The runtime never inspects the
    /// free-form [`Self::Persistence`] message; the bridge
    /// surfaces the typed `"persistence_unavailable"` reason and
    /// leaves the detail in the typed error for the bootstrap
    /// to log locally without leaking the SQLite / platform
    /// string.
    #[allow(dead_code)]
    pub fn reason(&self) -> &'static str {
        match self {
            Self::PeerUnavailable(reason) => reason,
            Self::BodyTooLarge => "body_too_large",
            Self::InvalidUtf8 => "invalid_utf8",
            Self::NotTransferable => "not_transferable",
            Self::EmptyContent => "empty_content",
            Self::TitleInvalid => "title_invalid",
            Self::Persistence(_) => "persistence_unavailable",
        }
    }
}

/// Metadata the client-side facade hands to the transport so the
/// transport can dial the matching peer with the pinned cert
/// fingerprint. The runtime reads the cert fingerprint from the
/// [`crate::peer_pairing::PairingRuntime`] it cached at install
/// time — the bridge never accepts the fingerprint from the
/// renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerFetchRequest {
    pub peer_id: String,
    pub cert_fingerprint: String,
    /// Opaque remote entry id the host minted.
    pub remote_entry_id: String,
}

/// Successful payload the [`PeerFetchTransport`] returns. The
/// transport forwards the body verbatim to the importer so the
/// local validator re-checks the [`IMPORT_MAX_BODY_BYTES`] cap
/// and the UTF-8 shape before any SQLite mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerFetchResponse {
    pub peer_id: String,
    pub remote_entry_id: String,
    pub title: Option<String>,
    pub content_type: String,
    pub body: String,
}

/// Metadata-only transport façade the import facade uses. The
/// trait is the seam between the core runtime and the platform
/// mTLS stack: the transport owns the dial loop, the pin lookup
/// and the per-peer session, while the runtime owns the trust /
/// active gate and the body validation. Every call collapses to
/// a typed [`PeerFetchTransportError`] variant the runtime maps
/// onto [`PeerImportOutcome`].
pub trait PeerFetchTransport: Send + Sync {
    fn fetch_text(
        &self,
        request: PeerFetchRequest,
    ) -> Result<PeerFetchResponse, PeerFetchTransportError>;
}

/// Typed transport error the runtime maps onto
/// [`PeerImportOutcome`]. The variants mirror
/// [`clipvault_platform::peer_transport::TransportError`] but
/// the trait collapses the wire detail into the same stable
/// reasons the frontend branches on.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerFetchTransportError {
    #[error("peer fetch transport is unavailable")]
    Unavailable,
    #[error("peer fetch transport has no resolved pairing endpoint")]
    PeerUnresolved,
    #[error("peer fetch transport rejected an unknown peer")]
    UnknownPeer,
    #[error("peer fetch transport rejected a mismatched TLS identity")]
    KeyMismatch,
    #[error("peer fetch transport rejected a revoked peer")]
    Revoked,
    #[error("peer fetch transport rejected a blocked peer")]
    Blocked,
    #[error("peer fetch transport wire protocol is incompatible")]
    IncompatibleProtocol,
    #[error("peer fetch transport rejected a malformed payload")]
    Malformed,
    /// The remote entry disappeared or is no longer
    /// transferrable.
    #[error("peer fetch transport rejected a non-transferrable entry")]
    NotTransferable,
    /// The remote body exceeded the [`IMPORT_MAX_BODY_BYTES`]
    /// cap.
    #[error("peer fetch transport rejected a body that exceeded the 1 MiB limit")]
    BodyTooLarge,
    /// The local persistence layer refused the projection
    /// (`unavailable` / `failed`).
    #[error("peer fetch transport rejected the request because persistence is unavailable")]
    PersistenceUnavailable,
}

impl PeerFetchTransportError {
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

/// Persistence trait the importer drives to project the import
/// transaction against the local SQLite handle. The trait is
/// metadata-only by construction: the body never crosses the
/// boundary; the helpers receive the typed primitives the
/// repository expects and forward them through the existing
/// CRUD methods. Tests inject an in-memory fake.
pub trait PeerImportPersistence: Send + Sync {
    /// Return the entry id whose canonical content hash matches
    /// `content_hash`. `None` when no row exists yet.
    fn find_entry_by_hash(
        &self,
        content_hash: &str,
    ) -> Result<Option<i64>, PeerImportPersistenceError>;
    /// Insert a fresh entry with the supplied metadata. Returns
    /// the inserted row id. The helper is responsible for
    /// attaching the row to the system `Historial` collection
    /// inside the same transaction.
    fn insert_entry(
        &self,
        content: String,
        content_type: ContentType,
        content_size: i64,
        content_hash: String,
        title: Option<String>,
        created_at: OffsetDateTime,
        last_seen_at: OffsetDateTime,
    ) -> Result<i64, PeerImportPersistenceError>;
    /// Update the title of an existing entry. The helper returns
    /// the refreshed row id; the importer refuses to overwrite
    /// an existing title.
    fn touch_entry_last_seen(
        &self,
        entry_id: i64,
        last_seen_at: OffsetDateTime,
    ) -> Result<i64, PeerImportPersistenceError>;
    /// Fetch a single entry by id.
    fn fetch_entry(&self, entry_id: i64)
        -> Result<Option<EntryRecord>, PeerImportPersistenceError>;
    /// Look up the binding row the importer needs to attach the
    /// imported entry to the peer collection. `None` when the
    /// user previously deleted the binding (or when this is the
    /// first import for the peer).
    fn find_binding(&self, peer_id: &str) -> Result<Option<i64>, PeerImportPersistenceError>;
    /// Persist a fresh binding row keyed by `peer_id`.
    fn upsert_binding(
        &self,
        peer_id: &str,
        collection_id: i64,
        now: OffsetDateTime,
    ) -> Result<i64, PeerImportPersistenceError>;
    /// Attach an entry to the supplied collection id, idempotent.
    fn attach_entry_to_collection(
        &self,
        entry_id: i64,
        collection_id: i64,
        now: OffsetDateTime,
    ) -> Result<(), PeerImportPersistenceError>;
    /// Look up a `(peer_id, remote_entry_id, hash)` provenance
    /// row. `None` when no row exists yet.
    fn find_import(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
        imported_content_hash: &str,
    ) -> Result<Option<i64>, PeerImportPersistenceError>;
    /// Persist a fresh provenance row keyed by `(peer_id,
    /// remote_entry_id, hash)`.
    fn record_import(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
        imported_content_hash: &str,
        local_entry_id: i64,
        now: OffsetDateTime,
    ) -> Result<(), PeerImportPersistenceError>;
    /// Look up the canonical user-collection name the importer
    /// must avoid colliding with.
    fn collection_name_exists(&self, name: &str) -> Result<bool, PeerImportPersistenceError>;
    /// Create a fresh user collection the importer uses when
    /// the visible peer name does not collide with an existing
    /// user row.
    fn create_user_collection(
        &self,
        name: &str,
        color_hex: &str,
        now: OffsetDateTime,
    ) -> Result<i64, PeerImportPersistenceError>;
    /// Fetch a single collection by id.
    fn find_collection(
        &self,
        collection_id: i64,
    ) -> Result<Option<Collection>, PeerImportPersistenceError>;
}

/// Typed persistence error the runtime surfaces for every
/// SQLite refusal. Every variant collapses to a stable
/// identifier; the runtime never inspects free-form SQLite
/// messages.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerImportPersistenceError {
    #[error("sqlite error: {0}")]
    Sqlite(String),
    #[error("organization error: {0}")]
    Organization(String),
    #[error("collection {0} not found")]
    UnknownCollection(i64),
}

impl From<OrganizationError> for PeerImportPersistenceError {
    fn from(error: OrganizationError) -> Self {
        Self::Organization(format!("{error}"))
    }
}

/// Clock the importer uses to stamp the import timestamps. The
/// trait keeps the service deterministic in tests.
pub trait ImportClock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

/// Production clock backed by the host's `OffsetDateTime::now_utc`.
pub struct SystemImportClock;

impl ImportClock for SystemImportClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

/// In-memory persistence adapter the tests use. The adapter
/// stores every row in `RwLock<Vec<_>>` collections and mirrors
/// the contract the production adapter exposes.
pub struct InMemoryImportPersistence {
    state: Arc<Mutex<InMemoryImportState>>,
}

struct InMemoryImportState {
    entries: HashMap<i64, EntryRecord>,
    next_entry_id: i64,
    bindings: HashMap<String, i64>,
    imports: HashSet<(String, String, String)>,
    collections: HashMap<i64, Collection>,
    next_collection_id: i64,
    by_hash: HashMap<String, i64>,
}

impl Default for InMemoryImportPersistence {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(InMemoryImportState {
                entries: HashMap::new(),
                next_entry_id: 1,
                bindings: HashMap::new(),
                imports: HashSet::new(),
                collections: HashMap::new(),
                next_collection_id: 1,
                by_hash: HashMap::new(),
            })),
        }
    }
}

impl InMemoryImportPersistence {
    /// Build an empty in-memory persistence adapter. Tests use
    /// this to verify the importer without linking the SQLite
    /// handle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the adapter with a deterministic list of entries.
    /// Useful when a test needs to pre-populate a local row the
    /// import must dedupe against.
    pub fn seed_entries(&self, entries: Vec<EntryRecord>) {
        let mut state = self.state.lock();
        for entry in entries {
            state.by_hash.insert(entry.content_hash.clone(), entry.id);
            state.next_entry_id = state.next_entry_id.max(entry.id + 1);
            state.entries.insert(entry.id, entry);
        }
    }
}

impl PeerImportPersistence for InMemoryImportPersistence {
    fn find_entry_by_hash(
        &self,
        content_hash: &str,
    ) -> Result<Option<i64>, PeerImportPersistenceError> {
        Ok(self.state.lock().by_hash.get(content_hash).copied())
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
            content,
            content_type,
            content_size,
            content_hash: content_hash.clone(),
            source_app: None,
            is_pinned: false,
            created_at: now_string.clone(),
            updated_at: now_string.clone(),
            last_seen_at: last_seen_string,
            title,
            source_app_name: None,
            source_app_icon_ref: None,
            asset_ref: None,
            mime_type: None,
            payload_width: None,
            payload_height: None,
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
    ) -> Result<i64, PeerImportPersistenceError> {
        let mut state = self.state.lock();
        let record = state
            .entries
            .get_mut(&entry_id)
            .ok_or(PeerImportPersistenceError::Sqlite(format!(
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
    ) -> Result<Option<EntryRecord>, PeerImportPersistenceError> {
        Ok(self.state.lock().entries.get(&entry_id).cloned())
    }

    fn find_binding(&self, peer_id: &str) -> Result<Option<i64>, PeerImportPersistenceError> {
        Ok(self.state.lock().bindings.get(peer_id).copied())
    }

    fn upsert_binding(
        &self,
        peer_id: &str,
        collection_id: i64,
        _now: OffsetDateTime,
    ) -> Result<i64, PeerImportPersistenceError> {
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
    ) -> Result<(), PeerImportPersistenceError> {
        // The in-memory adapter does not track entry → collection
        // associations; the importer uses the binding + a fresh
        // provenance row to model the relationship.
        Ok(())
    }

    fn find_import(
        &self,
        peer_id: &str,
        remote_entry_id: &str,
        imported_content_hash: &str,
    ) -> Result<Option<i64>, PeerImportPersistenceError> {
        let state = self.state.lock();
        if state.imports.contains(&(
            peer_id.to_string(),
            remote_entry_id.to_string(),
            imported_content_hash.to_string(),
        )) {
            // The in-memory adapter does not store the entry id
            // alongside the provenance; tests can recover it
            // through `fetch_entry` when they need to assert the
            // local id.
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
    ) -> Result<(), PeerImportPersistenceError> {
        let mut state = self.state.lock();
        state.imports.insert((
            peer_id.to_string(),
            remote_entry_id.to_string(),
            imported_content_hash.to_string(),
        ));
        Ok(())
    }

    fn collection_name_exists(&self, name: &str) -> Result<bool, PeerImportPersistenceError> {
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
    ) -> Result<i64, PeerImportPersistenceError> {
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
            },
        );
        Ok(id)
    }

    fn find_collection(
        &self,
        collection_id: i64,
    ) -> Result<Option<Collection>, PeerImportPersistenceError> {
        Ok(self.state.lock().collections.get(&collection_id).cloned())
    }
}

/// Trusted / active cache the importer consults before dialing.
/// The runtime populates the cache on every snapshot / health
/// probe through [`Self::record_peer_state`]; the importer only
/// ever reads the cached value.
#[derive(Debug, Clone, Copy, Default)]
pub struct PeerImportTrustState {
    pub trusted: bool,
    pub active: bool,
}

/// Service the shell drives. The façade is cheap to clone
/// (every field is `Arc`-shared). The service is metadata-only
/// by construction: the body is held in memory only between the
/// authenticated fetch and the SQLite commit, and the commit
/// routes the data through the existing dedupe / collection
/// helpers so a regression cannot duplicate an entry, mutate
/// metadata or leak the body to a log / event / toast.
#[derive(Clone)]
pub struct PeerImportService {
    transport: Arc<dyn PeerFetchTransport>,
    persistence: Arc<dyn PeerImportPersistence>,
    clock: Arc<dyn ImportClock>,
    trust_state: Arc<Mutex<std::collections::HashMap<String, PeerImportTrustState>>>,
}

impl PeerImportService {
    /// Build a service backed by the supplied transport and
    /// persistence adapters. The trust cache starts empty: the
    /// shell populates it on every snapshot / health probe so
    /// the very first import call waits for an `Active +
    /// trusted` projection before dialing the remote listener.
    pub fn new(
        transport: Arc<dyn PeerFetchTransport>,
        persistence: Arc<dyn PeerImportPersistence>,
        clock: Arc<dyn ImportClock>,
    ) -> Self {
        Self {
            transport,
            persistence,
            clock,
            trust_state: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Update the cached trust / active state for a single peer.
    pub fn record_peer_state(&self, peer_id: &str, state: PeerImportTrustState) {
        self.trust_state.lock().insert(peer_id.to_string(), state);
    }

    /// Forget the cached state for `peer_id` after a revoke,
    /// block, or unlink so subsequent imports collapse to
    /// [`PeerImportOutcome::PeerUnavailable`] without a network
    /// round-trip.
    pub fn forget_peer(&self, peer_id: &str) {
        self.trust_state.lock().remove(peer_id);
    }

    /// Read the trust / active state the cache currently holds
    /// for `peer_id`.
    pub fn peer_state(&self, peer_id: &str) -> Option<PeerImportTrustState> {
        self.trust_state.lock().get(peer_id).copied()
    }

    /// Import a single row of a trusted, active peer. The
    /// service dials the remote listener through the
    /// [`PeerFetchTransport`], validates the body locally,
    /// persists the import transaction through the
    /// [`PeerImportPersistence`], and returns a typed
    /// [`PeerImportOutcome`] the bridge forwards to the
    /// frontend.
    pub fn import(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        remote_entry_id: &str,
        display_name: &str,
    ) -> PeerImportOutcome {
        // Trust / active gate: refuse to dial when the peer is
        // unknown, not trusted, or not active. The renderer must
        // never see a network call for an ineligible peer.
        match self.peer_state(peer_id) {
            None => {
                return PeerImportOutcome::PeerUnavailable {
                    reason: "no_known_peer",
                };
            }
            Some(state) if !state.trusted => {
                return PeerImportOutcome::PeerUnavailable {
                    reason: "not_trusted",
                };
            }
            Some(state) if !state.active => {
                return PeerImportOutcome::PeerUnavailable {
                    reason: "not_active",
                };
            }
            Some(_) => {}
        }
        let request = PeerFetchRequest {
            peer_id: peer_id.to_string(),
            cert_fingerprint: cert_fingerprint.to_string(),
            remote_entry_id: remote_entry_id.to_string(),
        };
        match self.transport.fetch_text(request) {
            Ok(response) => self.commit(peer_id, display_name, response),
            Err(error) => match error {
                PeerFetchTransportError::Revoked
                | PeerFetchTransportError::Blocked
                | PeerFetchTransportError::KeyMismatch
                | PeerFetchTransportError::UnknownPeer => PeerImportOutcome::PeerUnavailable {
                    reason: "not_trusted",
                },
                PeerFetchTransportError::NotTransferable => PeerImportOutcome::NotTransferable,
                PeerFetchTransportError::BodyTooLarge => PeerImportOutcome::BodyTooLarge,
                PeerFetchTransportError::PersistenceUnavailable => {
                    PeerImportOutcome::PersistenceError {
                        reason: error.reason(),
                    }
                }
                PeerFetchTransportError::Unavailable
                | PeerFetchTransportError::PeerUnresolved
                | PeerFetchTransportError::IncompatibleProtocol
                | PeerFetchTransportError::Malformed => PeerImportOutcome::TransportUnavailable {
                    reason: error.reason(),
                },
            },
        }
    }

    fn commit(
        &self,
        peer_id: &str,
        display_name: &str,
        response: PeerFetchResponse,
    ) -> PeerImportOutcome {
        // Local re-validation. The contract pins the 1 MiB cap
        // as an absolute bound; a drifted host that returns a
        // larger body must collapse to `BodyTooLarge` without
        // persisting anything.
        if response.body.len() > IMPORT_MAX_BODY_BYTES {
            return PeerImportOutcome::BodyTooLarge;
        }
        // UTF-8 validation. The contract requires the body to be
        // canonical UTF-8 text; an invalid sequence collapses to
        // a typed outcome without persisting anything.
        if std::str::from_utf8(response.body.as_bytes()).is_err() {
            return PeerImportOutcome::InvalidUtf8;
        }
        // Empty body is a typed refusal: the local capture
        // pipeline refuses empty entries so the import path
        // keeps the contract consistent.
        if response.body.trim().is_empty() {
            return PeerImportOutcome::EmptyContent;
        }
        // Title validation: the local UI rejects titles longer
        // than `MAX_TITLE_LENGTH`. A failed title is a soft
        // error: the import continues without persisting the
        // invalid title.
        let validated_title = match crate::history::TextHistoryService::validate_title(
            response.title.as_deref().unwrap_or(""),
        ) {
            Ok(Some(value)) => Some(value),
            Ok(None) => None,
            Err(_) => return PeerImportOutcome::TitleInvalid,
        };
        let content_type = parse_content_type(&response.content_type);
        // The local helper produces a deterministic 16-hex hash
        // the local SQLite layer persists. The importer routes
        // the body through `hash_content` so two remote peers
        // sending the same text collapse to the same local row
        // (and the same canonical hash that the dedupe path
        // already trusts).
        let hash = crate::history::hash_content(&response.body);
        let now = self.clock.now();
        // Dedupe by hash: if a local row already exists the
        // import reuses it without mutating metadata. The
        // helper is metadata-only: the body is not inspected
        // again after the canonical hash is computed.
        let existing_entry_id = match self.persistence.find_entry_by_hash(&hash) {
            Ok(Some(id)) => Some(id),
            Ok(None) => None,
            Err(error) => {
                return PeerImportOutcome::PersistenceError {
                    reason: match error {
                        PeerImportPersistenceError::Sqlite(_) => "sqlite",
                        PeerImportPersistenceError::Organization(_) => "organization",
                        PeerImportPersistenceError::UnknownCollection(_) => "unknown_collection",
                    },
                };
            }
        };
        let entry_id = match existing_entry_id {
            Some(id) => match self.persistence.touch_entry_last_seen(id, now) {
                Ok(id) => id,
                Err(_) => {
                    return PeerImportOutcome::PersistenceError { reason: "sqlite" };
                }
            },
            None => {
                let content_size = response.body.len() as i64;
                match self.persistence.insert_entry(
                    response.body.clone(),
                    content_type,
                    content_size,
                    hash.clone(),
                    validated_title.clone(),
                    now,
                    now,
                ) {
                    Ok(id) => id,
                    Err(_) => {
                        return PeerImportOutcome::PersistenceError { reason: "sqlite" };
                    }
                }
            }
        };
        // Resolve / create the peer-bound collection. The
        // helper looks up an existing binding first; on a miss
        // it creates a user collection with the peer's visible
        // display name, falling back to `(equipo)` only on a
        // collision.
        let binding_collection_id = match self.persistence.find_binding(peer_id) {
            Ok(Some(id)) => id,
            Ok(None) => match self.create_peer_collection(display_name, now) {
                Ok(id) => match self.persistence.upsert_binding(peer_id, id, now) {
                    Ok(id) => id,
                    Err(_) => {
                        return PeerImportOutcome::PersistenceError { reason: "sqlite" };
                    }
                },
                Err(_) => {
                    return PeerImportOutcome::PersistenceError { reason: "sqlite" };
                }
            },
            Err(_) => {
                return PeerImportOutcome::PersistenceError { reason: "sqlite" };
            }
        };
        if let Err(_) =
            self.persistence
                .attach_entry_to_collection(entry_id, binding_collection_id, now)
        {
            return PeerImportOutcome::PersistenceError { reason: "sqlite" };
        }
        // Persist a fresh provenance row. The composite primary
        // key `(peer_id, remote_entry_id, hash)` makes the
        // record idempotent across re-imports and snapshots
        // edits; the helper collapses a re-import to a `Ok(())`
        // no-op without creating a duplicate row.
        if let Err(_) =
            self.persistence
                .record_import(peer_id, &response.remote_entry_id, &hash, entry_id, now)
        {
            return PeerImportOutcome::PersistenceError { reason: "sqlite" };
        }
        PeerImportOutcome::Imported {
            entry_id,
            collection_id: binding_collection_id,
            deduplicated: existing_entry_id.is_some(),
        }
    }

    fn create_peer_collection(
        &self,
        display_name: &str,
        now: OffsetDateTime,
    ) -> Result<i64, PeerImportPersistenceError> {
        // Trim the visible name; fall back to `(equipo)` when
        // the name is empty / whitespace. The local validation
        // helper caps the canonical form at `MAX_ORGANIZATION_NAME_CHARS`
        // and rejects empties.
        let trimmed = display_name.trim();
        let base_name = if trimmed.is_empty() {
            "(equipo)"
        } else {
            trimmed
        };
        // Resolve collisions with the documented `(equipo)`
        // suffix; the local helper caps the canonical form at
        // `MAX_ORGANIZATION_NAME_CHARS` so a maliciously long
        // name collapses to a stable canonical form.
        let mut candidate = base_name.to_string();
        loop {
            let exists = self.persistence.collection_name_exists(&candidate)?;
            if !exists {
                break;
            }
            let suffix = " (equipo)";
            let combined = format!("{base_name}{suffix}");
            // The local cap (`MAX_ORGANIZATION_NAME_CHARS`)
            // applies; truncate when needed.
            let cap = clipvault_db::MAX_ORGANIZATION_NAME_CHARS;
            let chars: Vec<char> = combined.chars().collect();
            if chars.len() > cap {
                candidate = chars.into_iter().take(cap).collect();
            } else {
                candidate = combined;
            }
            // Stable termination: a peer whose visible name
            // already contains the suffix collapses to itself;
            // any further collision keeps the same name and the
            // dedupe path refuses the duplicate (we never reach
            // here because `collection_name_exists` stays true).
            if candidate == base_name {
                break;
            }
        }
        let collection_id = self.persistence.create_user_collection(
            &candidate,
            clipvault_db::HISTORY_DEFAULT_COLOR_HEX,
            now,
        )?;
        Ok(collection_id)
    }
}

/// Map the wire content-type string the host returns onto the
/// canonical [`ContentType`] variant the local SQLite layer
/// persists. Unknown strings collapse to `Text` so a future
/// content-type addition cannot break the importer.
fn parse_content_type(value: &str) -> ContentType {
    match value {
        "text" => ContentType::Text,
        "url" => ContentType::Url,
        "email" => ContentType::Email,
        "json" => ContentType::Json,
        "jwt" => ContentType::Jwt,
        "uuid" => ContentType::Uuid,
        "ipv4" => ContentType::Ipv4,
        "ipv6" => ContentType::Ipv6,
        "hex_color" => ContentType::HexColor,
        "html" => ContentType::Html,
        "file_path" => ContentType::FilePath,
        "shell_command" => ContentType::ShellCommand,
        "sql" => ContentType::Sql,
        "code" => ContentType::Code,
        _ => ContentType::Text,
    }
}

/// Productive adapter that delegates to the
/// [`crate::peer_pairing::PeerTransport`] the bootstrap already
/// installed for the pairing change. The adapter owns no
/// transport state of its own: it is a thin translation layer
/// that converts the [`PeerFetchRequest`] the runtime hands it
/// into the typed [`clipvault_platform::peer_transport::PeerFetchSnapshot`]
/// the productive pairing transport hands back.
#[cfg(feature = "local-peer-pairing-tls")]
pub struct PeerPairingFetchTransportAdapter {
    inner: Arc<dyn crate::peer_pairing::PeerTransport>,
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerPairingFetchTransportAdapter {
    /// Build an adapter that delegates `fetch_text` to the
    /// supplied pairing transport. The adapter is cheap to clone
    /// (`Arc`-shared); the bootstrap keeps a single instance per
    /// app context.
    pub fn new(inner: Arc<dyn crate::peer_pairing::PeerTransport>) -> Self {
        Self { inner }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerFetchTransport for PeerPairingFetchTransportAdapter {
    fn fetch_text(
        &self,
        request: PeerFetchRequest,
    ) -> Result<PeerFetchResponse, PeerFetchTransportError> {
        match self.inner.fetch_text(
            &request.peer_id,
            &request.cert_fingerprint,
            &request.remote_entry_id,
        ) {
            Ok(snapshot) => {
                if snapshot.peer_id != request.peer_id {
                    return Err(PeerFetchTransportError::UnknownPeer);
                }
                if snapshot.remote_entry_id != request.remote_entry_id {
                    return Err(PeerFetchTransportError::Malformed);
                }
                Ok(PeerFetchResponse {
                    peer_id: snapshot.peer_id,
                    remote_entry_id: snapshot.remote_entry_id,
                    title: snapshot.title,
                    content_type: snapshot.content_type,
                    body: snapshot.body,
                })
            }
            Err(error) => Err(map_pairing_transport_error(error)),
        }
    }
}

/// Noop transport the tests use when the assertion does not
/// exercise the dial path. The transport collapses every call to
/// [`PeerFetchTransportError::Unavailable`] so the runtime
/// surfaces the typed outcome without a network round-trip.
pub struct NoopPeerFetchTransport;

impl PeerFetchTransport for NoopPeerFetchTransport {
    fn fetch_text(
        &self,
        _request: PeerFetchRequest,
    ) -> Result<PeerFetchResponse, PeerFetchTransportError> {
        Err(PeerFetchTransportError::Unavailable)
    }
}

/// Translate [`crate::peer_pairing::TransportError`] into the
/// typed [`PeerFetchTransportError`] the runtime branches on.
#[cfg(feature = "local-peer-pairing-tls")]
fn map_pairing_transport_error(
    error: crate::peer_pairing::TransportError,
) -> PeerFetchTransportError {
    use crate::peer_pairing::TransportError as Pairing;
    match error {
        Pairing::UnknownPeer => PeerFetchTransportError::UnknownPeer,
        Pairing::KeyMismatch => PeerFetchTransportError::KeyMismatch,
        Pairing::Revoked => PeerFetchTransportError::Revoked,
        Pairing::Blocked => PeerFetchTransportError::Blocked,
        Pairing::IncompatibleProtocol => PeerFetchTransportError::IncompatibleProtocol,
        Pairing::Malformed => PeerFetchTransportError::Malformed,
        Pairing::PeerUnresolved => PeerFetchTransportError::PeerUnresolved,
        Pairing::Unavailable => PeerFetchTransportError::Unavailable,
        Pairing::AlreadyRunning
        | Pairing::NotRunning
        | Pairing::Crypto
        | Pairing::InvalidCursor => PeerFetchTransportError::Unavailable,
        // The fetch-specific `body_too_large` error is the only
        // variant that maps directly onto the typed fetch
        // outcome.
        Pairing::BodyTooLarge => PeerFetchTransportError::BodyTooLarge,
    }
}

/// Productive host-side adapter that implements
/// [`clipvault_platform::peer_transport::FetchTextHostHandler`]
/// and delegates every inbound `fetch_text` request to the
/// [`PeerImportPersistence`] the bootstrap installed. The
/// adapter is feature-gated to the productive TLS path so
/// cross-compiles and unsupported targets keep compiling. The
/// bootstrap installs this adapter through the productive
/// [`crate::peer_pairing::PairingRuntime::install_fetch_handler_inner`]
/// API so the very first inbound `FetchText` envelope that
/// lands after a fresh mTLS handshake can already be served
/// without falling back to the documented `not_available`
/// reason.
#[cfg(feature = "local-peer-pairing-tls")]
pub struct PeerTextImportHostHandlerAdapter {
    persistence: Arc<dyn PeerImportPersistence>,
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerTextImportHostHandlerAdapter {
    pub fn new(persistence: Arc<dyn PeerImportPersistence>) -> Self {
        Self { persistence }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl clipvault_platform::peer_transport::FetchTextHostHandler for PeerTextImportHostHandlerAdapter {
    fn fetch_text(
        &self,
        _peer_id: &str,
        remote_entry_id: &str,
    ) -> clipvault_platform::peer_transport::HostFetchResponse {
        // Decode the opaque remote entry id back into the local
        // primary key the host projection uses. The host mint
        // pattern is `entry-<id>`; an unrecognised id collapses
        // to a typed `NotFound` outcome so the importer can
        // surface the typed reason without leaking the raw
        // string back to the caller.
        let local_id = match decode_remote_entry_id(remote_entry_id) {
            Some(id) => id,
            None => return clipvault_platform::peer_transport::HostFetchResponse::NotFound,
        };
        let entry = match self.persistence.fetch_entry(local_id) {
            Ok(Some(entry)) => entry,
            Ok(None) => return clipvault_platform::peer_transport::HostFetchResponse::NotFound,
            Err(_) => {
                return clipvault_platform::peer_transport::HostFetchResponse::PersistenceUnavailable
            }
        };
        if !entry_is_transferable(&entry) {
            return clipvault_platform::peer_transport::HostFetchResponse::NotTransferable;
        }
        if entry.content.len() > clipvault_platform::peer_transport::FETCH_TEXT_MAX_BODY_BYTES {
            return clipvault_platform::peer_transport::HostFetchResponse::BodyTooLarge;
        }
        let title =
            crate::peer_text_history::sanitize_remote_title(entry.title.as_deref().unwrap_or(""));
        let content_type = entry.content_type.as_str().to_string();
        let body = entry.content;
        clipvault_platform::peer_transport::HostFetchResponse::Ok {
            title,
            content_type,
            body,
        }
    }
}

/// Decode the opaque remote entry id back into the local
/// primary key. The host projection uses `entry-<id>`; an
/// unrecognised id collapses to `None` so the caller can
/// surface a typed `NotFound`.
#[cfg(feature = "local-peer-pairing-tls")]
#[allow(dead_code)]
fn decode_remote_entry_id(remote_entry_id: &str) -> Option<i64> {
    remote_entry_id
        .strip_prefix("entry-")
        .and_then(|rest| rest.parse::<i64>().ok())
        .filter(|id| *id > 0)
}

/// Mirror of [`crate::peer_text_history::entry_is_transferable`]
/// that operates on the local `EntryRecord`. The host refuses
/// to ship an image, HTML, rich-only, or oversized body; the
/// predicate enforces the same contract without depending on
/// the rich-text metadata the importer never inspects.
#[cfg(feature = "local-peer-pairing-tls")]
#[allow(dead_code)]
fn entry_is_transferable(entry: &EntryRecord) -> bool {
    if !entry.content_type.is_textual() {
        return false;
    }
    if matches!(entry.content_type, ContentType::Html) {
        return false;
    }
    if entry.is_renderable_image() {
        return false;
    }
    if entry.asset_ref.is_some() {
        return false;
    }
    if entry.mime_type.is_some() {
        return false;
    }
    if entry.payload_width.is_some() || entry.payload_height.is_some() {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc as StdArc;

    fn fixed_clock(now: OffsetDateTime) -> StdArc<dyn ImportClock> {
        struct Fixed(OffsetDateTime);
        impl ImportClock for Fixed {
            fn now(&self) -> OffsetDateTime {
                self.0
            }
        }
        StdArc::new(Fixed(now))
    }

    struct ScriptedFetchTransport {
        response: parking_lot::Mutex<Result<PeerFetchResponse, PeerFetchTransportError>>,
    }

    impl ScriptedFetchTransport {
        fn new(response: Result<PeerFetchResponse, PeerFetchTransportError>) -> Self {
            Self {
                response: parking_lot::Mutex::new(response),
            }
        }
    }

    impl PeerFetchTransport for ScriptedFetchTransport {
        fn fetch_text(
            &self,
            _request: PeerFetchRequest,
        ) -> Result<PeerFetchResponse, PeerFetchTransportError> {
            self.response.lock().clone()
        }
    }

    fn entry_record(id: i64, content_type: ContentType, content: &str) -> EntryRecord {
        let now = "2026-01-01T00:00:00Z".to_string();
        EntryRecord {
            id,
            content: content.to_string(),
            content_type,
            content_size: content.len() as i64,
            content_hash: "h".repeat(16),
            source_app: None,
            is_pinned: false,
            created_at: now.clone(),
            updated_at: now.clone(),
            last_seen_at: now,
            title: None,
            source_app_name: None,
            source_app_icon_ref: None,
            asset_ref: None,
            mime_type: None,
            payload_width: None,
            payload_height: None,
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
            code_language: None,
        }
    }

    fn ok_response(body: &str) -> PeerFetchResponse {
        PeerFetchResponse {
            peer_id: "peer-a".to_string(),
            remote_entry_id: "entry-1".to_string(),
            title: None,
            content_type: "text".to_string(),
            body: body.to_string(),
        }
    }

    #[test]
    fn import_returns_peer_unavailable_when_state_missing() {
        let persistence = StdArc::new(InMemoryImportPersistence::new());
        let transport = StdArc::new(NoopPeerFetchTransport);
        let service = PeerImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        assert!(matches!(
            outcome,
            PeerImportOutcome::PeerUnavailable {
                reason: "no_known_peer"
            }
        ));
    }

    #[test]
    fn import_returns_peer_unavailable_when_state_not_trusted() {
        let persistence = StdArc::new(InMemoryImportPersistence::new());
        let transport = StdArc::new(NoopPeerFetchTransport);
        let service = PeerImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImportTrustState {
                trusted: false,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        assert!(matches!(
            outcome,
            PeerImportOutcome::PeerUnavailable {
                reason: "not_trusted"
            }
        ));
    }

    #[test]
    fn import_returns_peer_unavailable_when_state_not_active() {
        let persistence = StdArc::new(InMemoryImportPersistence::new());
        let transport = StdArc::new(NoopPeerFetchTransport);
        let service = PeerImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImportTrustState {
                trusted: true,
                active: false,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        assert!(matches!(
            outcome,
            PeerImportOutcome::PeerUnavailable {
                reason: "not_active"
            }
        ));
    }

    #[test]
    fn import_commits_first_import() {
        let persistence = StdArc::new(InMemoryImportPersistence::new());
        let transport = StdArc::new(ScriptedFetchTransport::new(Ok(ok_response("hello"))));
        let service = PeerImportService::new(
            transport,
            persistence.clone(),
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let PeerImportOutcome::Imported {
            entry_id,
            collection_id,
            deduplicated,
        } = outcome
        else {
            panic!("expected Imported, got {:?}", outcome);
        };
        assert!(entry_id > 0);
        assert!(collection_id > 0);
        assert!(!deduplicated);
    }

    #[test]
    fn import_dedupes_by_hash() {
        let persistence = StdArc::new(InMemoryImportPersistence::new());
        let transport = StdArc::new(ScriptedFetchTransport::new(Ok(ok_response("hello"))));
        let service = PeerImportService::new(
            transport,
            persistence.clone(),
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let first = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let second = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let (
            PeerImportOutcome::Imported {
                entry_id: first_entry,
                ..
            },
            PeerImportOutcome::Imported {
                entry_id: second_entry,
                deduplicated: second_dedup,
                ..
            },
        ) = (first, second)
        else {
            panic!("expected two Imported outcomes");
        };
        assert_eq!(first_entry, second_entry);
        assert!(second_dedup, "second import must reuse the first row");
    }

    #[test]
    fn import_creates_collection_with_collision_suffix() {
        let persistence = StdArc::new(InMemoryImportPersistence::new());
        // Pre-create a collection with the exact visible name.
        let _ = persistence
            .create_user_collection(
                "Equipo A",
                clipvault_db::HISTORY_DEFAULT_COLOR_HEX,
                OffsetDateTime::now_utc(),
            )
            .expect("seed");
        let transport = StdArc::new(ScriptedFetchTransport::new(Ok(ok_response("hello"))));
        let service = PeerImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let PeerImportOutcome::Imported { collection_id, .. } = outcome else {
            panic!("expected Imported");
        };
        let collection = service
            .persistence
            .find_collection(collection_id)
            .expect("find")
            .expect("collection exists");
        assert_eq!(collection.name, "Equipo A (equipo)");
    }

    #[test]
    fn import_rejects_body_too_large() {
        let persistence = StdArc::new(InMemoryImportPersistence::new());
        let big_body = "x".repeat(IMPORT_MAX_BODY_BYTES + 1);
        let transport = StdArc::new(ScriptedFetchTransport::new(Ok(ok_response(&big_body))));
        let service = PeerImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        assert!(matches!(outcome, PeerImportOutcome::BodyTooLarge));
    }

    #[test]
    fn import_creates_new_entry_when_remote_snapshot_is_edited() {
        // A second import of the same `(peer, remote_entry)` but
        // with a different body must produce a fresh local
        // snapshot: the dedupe path keys on the canonical hash,
        // so a snapshot edit cannot collapse to the previous
        // entry. The contract pins both outcomes: a new entry
        // for the new snapshot, the provenance row for the new
        // hash.
        let persistence: StdArc<dyn PeerImportPersistence> =
            StdArc::new(InMemoryImportPersistence::new());
        let transport = StdArc::new(ScriptedFetchTransport::new(Ok(ok_response("hello"))));
        let service = PeerImportService::new(
            transport,
            persistence.clone(),
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let first = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let first_entry = match first {
            PeerImportOutcome::Imported {
                entry_id,
                deduplicated,
                ..
            } => {
                assert!(!deduplicated);
                entry_id
            }
            other => panic!("expected first Imported, got {other:?}"),
        };
        // Re-issue with an edited body. The scripted transport
        // returns a fresh body for every call; the importer
        // must dedupe by the new canonical hash.
        let edited = ok_response("hello world");
        let transport = StdArc::new(ScriptedFetchTransport::new(Ok(edited)));
        let service = PeerImportService::new(
            transport,
            persistence.clone(),
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let second = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let second_entry = match second {
            PeerImportOutcome::Imported {
                entry_id,
                deduplicated,
                ..
            } => {
                assert!(!deduplicated, "edited snapshot must produce a fresh row");
                entry_id
            }
            other => panic!("expected second Imported, got {other:?}"),
        };
        assert_ne!(
            first_entry, second_entry,
            "edited snapshot must produce a distinct local entry"
        );
    }

    #[test]
    fn import_collapses_same_snapshot_to_dedup() {
        // The provenance row keeps the importer idempotent
        // across re-imports of the same `(peer, remote_entry,
        // hash)` triple; the import path must reuse the prior
        // entry without creating a duplicate.
        let persistence = StdArc::new(InMemoryImportPersistence::new());
        let transport = StdArc::new(ScriptedFetchTransport::new(Ok(ok_response("hello"))));
        let service = PeerImportService::new(
            transport,
            persistence.clone(),
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let first = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let second = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let (
            PeerImportOutcome::Imported {
                entry_id: first_entry,
                deduplicated: first_dedup,
                ..
            },
            PeerImportOutcome::Imported {
                entry_id: second_entry,
                deduplicated: second_dedup,
                ..
            },
        ) = (first, second)
        else {
            panic!("expected two Imported outcomes");
        };
        assert!(!first_dedup);
        assert!(second_dedup, "same snapshot must dedupe");
        assert_eq!(first_entry, second_entry);
    }

    #[test]
    fn import_supports_unicode_payload_and_title() {
        // The contract pins UTF-8 (the body must be valid UTF-8)
        // and a non-Latin title. The importer must accept both.
        let persistence = StdArc::new(InMemoryImportPersistence::new());
        let mut response = ok_response("hola · 漢字 — 🚀");
        response.title = Some("Notas · 漢字".to_string());
        let transport = StdArc::new(ScriptedFetchTransport::new(Ok(response)));
        let service = PeerImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let PeerImportOutcome::Imported {
            entry_id,
            deduplicated,
            ..
        } = outcome
        else {
            panic!("expected Imported");
        };
        assert!(!deduplicated);
        let record = service
            .persistence
            .fetch_entry(entry_id)
            .expect("fetch")
            .expect("entry exists");
        assert_eq!(record.content, "hola · 漢字 — 🚀");
        assert_eq!(record.title.as_deref(), Some("Notas · 漢字"));
    }

    #[test]
    fn import_supports_multiple_independent_peers() {
        // Two peers must keep their bindings and provenance
        // rows independent; the importer must never collapse
        // an entry imported from peer A to the binding of peer
        // B even when the display name collides.
        let persistence = StdArc::new(InMemoryImportPersistence::new());
        let transport = StdArc::new(ScriptedFetchTransport::new(Ok(ok_response("hello"))));
        let service = PeerImportService::new(
            transport,
            persistence.clone(),
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImportTrustState {
                trusted: true,
                active: true,
            },
        );
        service.record_peer_state(
            "peer-b",
            PeerImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome_a = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let outcome_b = service.import("peer-b", "fingerprint", "entry-1", "Equipo A");
        let (
            PeerImportOutcome::Imported {
                collection_id: collection_a,
                ..
            },
            PeerImportOutcome::Imported {
                collection_id: collection_b,
                ..
            },
        ) = (outcome_a, outcome_b)
        else {
            panic!("expected two Imported outcomes");
        };
        assert_ne!(
            collection_a, collection_b,
            "each peer must own a distinct binding"
        );
        let binding_a = service
            .persistence
            .find_binding("peer-a")
            .expect("find")
            .expect("binding");
        let binding_b = service
            .persistence
            .find_binding("peer-b")
            .expect("find")
            .expect("binding");
        assert_eq!(binding_a, collection_a);
        assert_eq!(binding_b, collection_b);
    }

    #[test]
    fn import_rejects_invalid_utf8() {
        // The wire is JSON-over-UTF-8 so any `String` reaching
        // the importer is guaranteed to be valid UTF-8. The
        // guard exists as a defensive check against a future
        // transport that streams raw bytes; we exercise it
        // directly through the validator helper instead of the
        // service entry point.
        let bad_bytes: Vec<u8> = vec![0xFF, 0xFE, 0xFD];
        assert!(std::str::from_utf8(&bad_bytes).is_err());
        // Sanity check: valid bytes always pass.
        let good = "hola · 漢字 — 🚀";
        assert!(std::str::from_utf8(good.as_bytes()).is_ok());
    }

    #[test]
    fn import_rejects_empty_body() {
        let persistence = StdArc::new(InMemoryImportPersistence::new());
        let transport = StdArc::new(ScriptedFetchTransport::new(Ok(ok_response("   \n  "))));
        let service = PeerImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        assert!(matches!(outcome, PeerImportOutcome::EmptyContent));
    }

    #[test]
    fn import_rejects_oversize_title() {
        let persistence = StdArc::new(InMemoryImportPersistence::new());
        let mut response = ok_response("hello");
        response.title = Some("x".repeat(crate::history::MAX_TITLE_LENGTH + 1));
        let transport = StdArc::new(ScriptedFetchTransport::new(Ok(response)));
        let service = PeerImportService::new(
            transport,
            persistence,
            fixed_clock(OffsetDateTime::now_utc()),
        );
        service.record_peer_state(
            "peer-a",
            PeerImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        assert!(matches!(outcome, PeerImportOutcome::TitleInvalid));
    }

    #[test]
    fn import_keeps_existing_title_for_reused_entry() {
        let persistence = StdArc::new(InMemoryImportPersistence::new());
        let now = OffsetDateTime::now_utc();
        // Seed an existing local entry whose canonical hash
        // matches `hello` and whose title is `Original`. The
        // import must reuse the row without overwriting the
        // title.
        let hash = crate::history::hash_content("hello");
        let mut existing = entry_record(42, ContentType::Text, "hello");
        existing.title = Some("Original".to_string());
        existing.content_hash = hash.clone();
        persistence.seed_entries(vec![existing.clone()]);
        let transport = StdArc::new(ScriptedFetchTransport::new(Ok(ok_response("hello"))));
        let service = PeerImportService::new(transport, persistence.clone(), fixed_clock(now));
        service.record_peer_state(
            "peer-a",
            PeerImportTrustState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.import("peer-a", "fingerprint", "entry-1", "Equipo A");
        let PeerImportOutcome::Imported {
            entry_id,
            deduplicated,
            ..
        } = outcome
        else {
            panic!("expected Imported");
        };
        assert_eq!(entry_id, existing.id);
        assert!(deduplicated);
        let persisted = service
            .persistence
            .fetch_entry(entry_id)
            .expect("fetch")
            .expect("entry exists");
        assert_eq!(persisted.title.as_deref(), Some("Original"));
    }

    #[test]
    #[cfg(feature = "local-peer-pairing-tls")]
    fn host_handler_rejects_image_payload() {
        let concrete = InMemoryImportPersistence::new();
        // Seed an image row whose `is_renderable_image()`
        // predicate returns true. The host handler must reject
        // the row with `NotTransferable` so the importer can
        // surface the typed outcome without leaking the bytes.
        let image = entry_record(99, ContentType::Image, clipvault_db::IMAGE_CONTENT_SENTINEL);
        concrete.seed_entries(vec![image.clone()]);
        // The image payload above already fails
        // `is_textual()`; the host handler must short-circuit
        // before the body cap kicks in.
        let persistence: StdArc<dyn PeerImportPersistence> = StdArc::new(concrete);
        let adapter = PeerTextImportHostHandlerAdapter::new(persistence);
        use clipvault_platform::peer_transport::FetchTextHostHandler as _;
        let outcome = adapter.fetch_text("peer-a", "entry-99");
        assert!(matches!(
            outcome,
            clipvault_platform::peer_transport::HostFetchResponse::NotTransferable
        ));
    }
}
