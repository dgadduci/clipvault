//! Capture pipeline for the clipboard text history.
//!
//! [`TextHistoryService`] is the only place in the codebase that wires
//! the platform-agnostic [`Clipboard`] adapter, the deterministic
//! [`Clock`], the optional [`ApplicationMetadataProvider`] and the
//! [`EntryRepository`] together. The shell calls
//! [`TextHistoryService::record_text`] whenever the operating system
//! reports a clipboard change; the service returns a typed outcome so
//! the rest of the app can react without inspecting SQLite or the
//! clipboard backend.

use std::sync::Arc;

use thiserror::Error;
use tracing::warn;

use clipvault_db::{
    EntryOutcome, EntryRepository, EntryRepositoryError, NewEntry, SourceAppFilter,
    IMAGE_CONTENT_SENTINEL, IMAGE_MIME_PNG,
};
use clipvault_platform::{
    ApplicationMetadataError, ApplicationMetadataProvider, ClipboardImage, ClipboardPayload,
    RichTextPayload,
};

use crate::bootstrap::AppContext;
use crate::clipboard::Clipboard;
use crate::clipboard_assets::{normalize_image, ClipboardAssetStore};
use crate::clock::Clock;
use crate::content_type::detect_content_type;
use crate::privacy::{CaptureDecision, PrivacyGate};
use crate::rich_text::{canonical_rich_text_hash, RichTextAssetStore};

/// What happened the last time [`TextHistoryService::record_text`] ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryOutcome {
    /// New entry inserted; `id` is its row id.
    Stored { id: i64 },
    /// The clipboard text already exists; the existing row's
    /// `updated_at` / `last_seen_at` were refreshed.
    Duplicate { id: i64 },
    /// The clipboard did not return any usable text (the adapter
    /// returned `Ok(None)` or `Ok(Some(""))`); no row was created.
    Ignored,
    /// The clipboard backend or the repository returned an error.
    /// `message` is a human-readable, non-sensitive description.
    Failed { message: String },
}

impl HistoryOutcome {
    pub fn kind(&self) -> &'static str {
        match self {
            HistoryOutcome::Stored { .. } => "stored",
            HistoryOutcome::Duplicate { .. } => "duplicate",
            HistoryOutcome::Ignored => "ignored",
            HistoryOutcome::Failed { .. } => "failed",
        }
    }

    pub fn id(&self) -> Option<i64> {
        match self {
            HistoryOutcome::Stored { id } | HistoryOutcome::Duplicate { id } => Some(*id),
            HistoryOutcome::Ignored | HistoryOutcome::Failed { .. } => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum HistoryServiceError {
    #[error("entry repository error: {0}")]
    Repository(#[from] EntryRepositoryError),
    #[error("invalid card title: {0}")]
    InvalidTitle(#[from] TitleValidationError),
}

/// Maximum length of a user-defined card title. The repository stores
/// the title unchanged after trimming; the service enforces this bound
/// so a runaway input cannot blow past the renderer layout.
pub const MAX_TITLE_LENGTH: usize = 80;

/// Result of [`TextHistoryService::set_title`]. Mirrors the management
/// layer so the shell surfaces a stable, typed outcome without
/// inspecting SQLite.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum SetTitleOutcome {
    /// The title was set or restored; `record` is the refreshed entry.
    Updated { record: clipvault_db::EntryRecord },
    /// The target entry does not exist (anymore).
    NotFound,
}

/// Validation outcome of [`TextHistoryService::validate_title`]. The
/// helper centralises the rules the GUI enforces so the shell can
/// surface consistent copy.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TitleValidationError {
    /// The trimmed input exceeded [`MAX_TITLE_LENGTH`] characters.
    #[error("card title exceeds the maximum length of {0} characters")]
    TooLong(usize),
}

impl TitleValidationError {
    pub fn kind_str(&self) -> &'static str {
        match self {
            TitleValidationError::TooLong(_) => "too_long",
        }
    }
}

/// Stable, non-sensitive failure categories carried by
/// [`HistoryOutcome::Failed`].
///
/// The capture pipeline never puts clipboard content, image bytes, a
/// content hash or a filesystem path in the message, so these
/// categories are the whole diagnostic surface a caller gets.
pub mod failure {
    /// The clipboard backend could not be read.
    pub const CLIPBOARD: &str = "clipboard read failed";
    /// The captured image could not be validated or encoded to PNG.
    pub const IMAGE_NORMALIZE: &str = "clipboard image could not be normalized";
    /// The normalized asset could not be written to the local store.
    pub const ASSET_STORE: &str = "clipboard asset could not be persisted";
    /// No asset store is wired in, so an image cannot be persisted.
    pub const ASSET_STORE_UNAVAILABLE: &str = "clipboard asset store unavailable";
    /// The local database rejected the row.
    pub const PERSISTENCE: &str = "history persistence failed";
}

/// Stable handle on the capture pipeline that the shell can keep in
/// the [`AppContext`]. Cheap to clone: it borrows the [`Arc`]s of the
/// [`Clipboard`] and [`Clock`] it needs.
#[derive(Clone)]
pub struct TextHistoryService {
    clipboard: Arc<dyn Clipboard>,
    clock: Arc<dyn Clock>,
    privacy_gate: Option<PrivacyGate>,
    app_metadata: Arc<dyn ApplicationMetadataProvider>,
    /// Local store for non-textual payloads. `None` in unit tests that
    /// only exercise the textual path; the bootstrap always wires one
    /// from `PlatformInfo::data_dir`. An image capture without a store
    /// is a typed, non-fatal failure — never a panic.
    asset_store: Option<ClipboardAssetStore>,
    /// Local store for rich-text payloads. Mirrors the image store:
    /// derived from `PlatformInfo::data_dir` so tests that inject a
    /// synthetic `PlatformInfo` get an isolated namespace.
    rich_asset_store: Option<RichTextAssetStore>,
}

impl TextHistoryService {
    /// Build a service backed by the provided clipboard, clock and
    /// metadata provider. The provider can be the no-op adapter when
    /// the platform cannot enrich the capture with metadata.
    pub fn new(
        clipboard: Arc<dyn Clipboard>,
        clock: Arc<dyn Clock>,
        app_metadata: Arc<dyn ApplicationMetadataProvider>,
    ) -> Self {
        Self {
            clipboard,
            clock,
            privacy_gate: None,
            app_metadata,
            asset_store: None,
            rich_asset_store: None,
        }
    }

    /// Attach a [`PrivacyGate`] consulted by every capture. The gate is
    /// optional so unit tests can exercise the service without a probe.
    pub fn with_privacy_gate(mut self, gate: PrivacyGate) -> Self {
        self.privacy_gate = Some(gate);
        self
    }

    /// Attach the local asset store used to persist non-textual
    /// payloads. Without it, an image capture returns a typed failure
    /// instead of silently dropping the payload.
    pub fn with_asset_store(mut self, store: ClipboardAssetStore) -> Self {
        self.asset_store = Some(store);
        self
    }

    /// Attach the rich-text asset store. Built from the same data
    /// directory as the image store; the bootstrap wires one
    /// automatically.
    pub fn with_rich_asset_store(mut self, store: RichTextAssetStore) -> Self {
        self.rich_asset_store = Some(store);
        self
    }

    /// The configured asset store, if any.
    pub fn asset_store(&self) -> Option<&ClipboardAssetStore> {
        self.asset_store.as_ref()
    }

    /// The configured rich-text asset store, if any.
    pub fn rich_asset_store(&self) -> Option<&RichTextAssetStore> {
        self.rich_asset_store.as_ref()
    }

    /// Replace the metadata provider. Used by tests that wire a
    /// scripted provider around the same service instance.
    #[allow(dead_code)]
    pub fn with_app_metadata_provider(
        mut self,
        provider: Arc<dyn ApplicationMetadataProvider>,
    ) -> Self {
        self.app_metadata = provider;
        self
    }

    /// Read the clipboard, persist the text and return a typed outcome.
    ///
    /// The method never panics: clipboard backend errors and database
    /// failures are converted into [`HistoryOutcome::Failed`] while a
    /// missing or empty clipboard payload becomes
    /// [`HistoryOutcome::Ignored`]; in both cases the caller can keep
    /// running.
    pub fn record_text(&self, context: &AppContext, source_app: Option<&str>) -> HistoryOutcome {
        let text = match self.clipboard.read_text() {
            Ok(Some(text)) if !text.is_empty() => text,
            Ok(_) => return HistoryOutcome::Ignored,
            Err(error) => {
                warn!(error = %error, "clipboard read failed");
                return HistoryOutcome::Failed {
                    message: error.to_string(),
                };
            }
        };

        self.record_payload(context, text, source_app)
    }

    /// Persist a textual payload that has already been read from the
    /// clipboard. Used by the [`crate::watcher::CaptureWatcher`] and
    /// any other pipeline that needs to drive the capture flow
    /// without going through the core [`crate::clipboard::Clipboard`]
    /// adapter (for example when the platform backend is
    /// `clipvault_platform::ClipboardBackend`).
    ///
    /// Kept as a thin wrapper over
    /// [`Self::record_clipboard_payload`] so every pre-existing caller
    /// keeps compiling and observes byte-for-byte identical behaviour.
    pub fn record_payload(
        &self,
        context: &AppContext,
        text: String,
        source_app: Option<&str>,
    ) -> HistoryOutcome {
        self.record_clipboard_payload(context, ClipboardPayload::Text(text), source_app)
    }

    /// Persist a payload of either supported shape.
    ///
    /// The order is fixed by the change contract and is the reason this
    /// is a single function rather than two:
    ///
    /// ```text
    /// normalize source identifier
    ///     -> PrivacyGate
    ///     -> normalize + hash (image only)
    ///     -> write/reuse local asset (image only)
    ///     -> SQLite transaction
    ///     -> metadata enrichment (best effort)
    /// ```
    ///
    /// The gate runs **before** any permanent artefact exists, so a
    /// blacklisted application can never produce a row, an asset file
    /// or application metadata.
    pub fn record_clipboard_payload(
        &self,
        context: &AppContext,
        payload: ClipboardPayload,
        source_app: Option<&str>,
    ) -> HistoryOutcome {
        // Normalise the identifier: a whitespace-only value carries no
        // useful information for the gate or the metadata enrichment
        // and would otherwise round-trip through SQLite. Keeping the
        // trim centralised here means the watcher's `Some("")` and
        // `Some("   ")` paths both produce a `None` row.
        let source_app = source_app.and_then(normalize_source_identifier);

        if let Some(gate) = &self.privacy_gate {
            match gate.evaluate(source_app.as_deref()) {
                CaptureDecision::Allow => {}
                CaptureDecision::Discard { reason } => {
                    tracing::trace!(reason, "privacy gate discarded capture");
                    return HistoryOutcome::Ignored;
                }
            }
        }

        match payload {
            ClipboardPayload::Text(text) if text.is_empty() => HistoryOutcome::Ignored,
            ClipboardPayload::Text(text) => self.persist_text(context, text, source_app),
            ClipboardPayload::Image(image) => self.persist_image(context, &image, source_app),
            ClipboardPayload::RichText(payload) => {
                self.persist_rich_text(context, &payload, source_app)
            }
        }
    }

    /// Persist a textual capture. Unchanged from the pre-image
    /// behaviour: the same hash, the same detector, the same columns.
    fn persist_text(
        &self,
        context: &AppContext,
        text: String,
        source_app: Option<String>,
    ) -> HistoryOutcome {
        let hash = hash_content(&text);
        let size = text.len() as i64;
        let now = self.clock.now();
        let content_type = detect_content_type(&text);

        let new_entry =
            NewEntry::text(text, content_type, size, hash, source_app.clone(), now, now);

        self.commit(context, new_entry, source_app.as_deref())
    }

    /// Persist a rich-text capture.
    ///
    /// The payload's canonical rich-text hash is computed *before*
    /// any I/O so the asset store, the dedupe key and the row metadata
    /// all agree on the same identifier. The original HTML and RTF
    /// representations are stored verbatim (no format conversion
    /// happens here) and the sanitised preview is computed
    /// independently by the asset store; the card only ever receives
    /// the preview reference, never the raw bytes.
    ///
    /// The dedupe column compares both `content_hash` and
    /// `rich_text_hash`, so two captures with identical plain text but
    /// different styles do not collapse to a single row.
    fn persist_rich_text(
        &self,
        context: &AppContext,
        payload: &RichTextPayload,
        source_app: Option<String>,
    ) -> HistoryOutcome {
        let Some(store) = self.rich_asset_store.as_ref() else {
            return HistoryOutcome::Failed {
                message: failure::ASSET_STORE_UNAVAILABLE.to_string(),
            };
        };

        let rich_hash = canonical_rich_text_hash(payload);
        let outcome = match store.store(&rich_hash, payload) {
            Ok(outcome) => outcome,
            Err(error) => {
                // Metadata-only: the kind is stable, the bytes never appear.
                warn!(reason = error.kind_str(), "rich text asset write failed");
                return HistoryOutcome::Failed {
                    message: failure::ASSET_STORE.to_string(),
                };
            }
        };

        let now = self.clock.now();
        let plain = payload.plain_text();
        let content_type = detect_content_type(plain);
        let plain_hash = hash_content(plain);

        let new_entry = NewEntry {
            content: plain.to_string(),
            content_type,
            content_size: plain.len() as i64,
            content_hash: plain_hash,
            source_app: source_app.clone(),
            created_at: now,
            last_seen_at: now,
            // The image-side `asset_ref` stays `None` for rich-text
            // rows: the rich preview lives behind
            // `rich_preview_ref`, which the card bridge serves.
            asset_ref: None,
            mime_type: None,
            payload_width: None,
            payload_height: None,
            rich_text_hash: Some(rich_hash),
            rich_html_ref: outcome.html_ref().map(str::to_string),
            rich_rtf_ref: outcome.rtf_ref().map(str::to_string),
            rich_preview_ref: outcome.preview_ref().map(str::to_string),
            rich_html_size: payload.html().map(|h| h.len() as i64),
            rich_rtf_size: payload.rtf().map(|r| r.len() as i64),
        };

        self.commit(context, new_entry, source_app.as_deref())
    }

    /// Persist an image capture.
    ///
    /// The bitmap is normalised to a canonical PNG first so the hash
    /// (and therefore the dedupe decision and the asset name) is
    /// derived from a representation that is stable across platforms —
    /// an RGBA buffer is not.
    ///
    /// If SQLite rejects the row *after* the asset was written, the
    /// file stays on disk as a safe orphan: no row references it, so
    /// the next collector pass reclaims it. Deleting it here would risk
    /// removing a file another row legitimately shares.
    fn persist_image(
        &self,
        context: &AppContext,
        image: &ClipboardImage,
        source_app: Option<String>,
    ) -> HistoryOutcome {
        let Some(store) = self.asset_store.as_ref() else {
            return HistoryOutcome::Failed {
                message: failure::ASSET_STORE_UNAVAILABLE.to_string(),
            };
        };

        let normalized = match normalize_image(image) {
            Ok(normalized) => normalized,
            Err(error) => {
                // Metadata-only: `kind_str` never carries pixels.
                warn!(
                    reason = error.kind_str(),
                    "clipboard image normalization failed"
                );
                return HistoryOutcome::Failed {
                    message: failure::IMAGE_NORMALIZE.to_string(),
                };
            }
        };

        let outcome = match store.store_image(&normalized) {
            Ok(outcome) => outcome,
            Err(error) => {
                warn!(reason = error.kind_str(), "clipboard asset write failed");
                return HistoryOutcome::Failed {
                    message: failure::ASSET_STORE.to_string(),
                };
            }
        };

        let now = self.clock.now();
        let new_entry = NewEntry {
            // An image row carries no textual payload; the column stays
            // NOT NULL through the documented empty sentinel.
            content: IMAGE_CONTENT_SENTINEL.to_string(),
            content_type: clipvault_db::ContentType::Image,
            content_size: normalized.byte_len() as i64,
            // Dedupe key: the hash of the normalized PNG.
            content_hash: normalized.hash().to_string(),
            source_app: source_app.clone(),
            created_at: now,
            last_seen_at: now,
            asset_ref: Some(outcome.asset_ref().to_string()),
            mime_type: Some(IMAGE_MIME_PNG.to_string()),
            payload_width: Some(normalized.width()),
            payload_height: Some(normalized.height()),
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
        };

        self.commit(context, new_entry, source_app.as_deref())
    }

    /// Shared tail of both persistence paths: one transaction, then a
    /// best-effort metadata enrichment.
    fn commit(
        &self,
        context: &AppContext,
        new_entry: NewEntry,
        source_app: Option<&str>,
    ) -> HistoryOutcome {
        let outcome = {
            let mut db = context.database().lock();
            let mut repo = EntryRepository::new(db.connection_mut());
            match repo.insert_or_touch(new_entry) {
                Ok(outcome) => outcome,
                Err(error) => {
                    warn!(error = %error, "history persistence failed");
                    return HistoryOutcome::Failed {
                        message: error.to_string(),
                    };
                }
            }
        };

        // Enrich the row with the source-application metadata. The
        // enrichment is best-effort: a failure MUST NOT convert a
        // valid capture into a `Failed` outcome. The shell can call
        // `enrich_metadata` again on the platform thread if the
        // platform provider needs it (macOS requires main thread).
        let record = outcome.record().clone();
        self.enrich_metadata(context, record.id, source_app);

        match outcome {
            EntryOutcome::Inserted(record) => HistoryOutcome::Stored { id: record.id },
            EntryOutcome::Updated(record) => HistoryOutcome::Duplicate { id: record.id },
        }
    }

    /// Persist the source-application metadata on a freshly inserted
    /// or refreshed row. The method swallows any provider failure and
    /// logs a trace-level message so the rest of the capture flow
    /// keeps running.
    ///
    /// The method is exposed so the shell can re-run the enrichment
    /// on the platform thread (for example macOS, where
    /// `NSWorkspace` requires the main thread) after the capture
    /// pipeline has finished persisting the entry. Calling it twice
    /// for the same `entry_id` is safe: the repository uses
    /// `COALESCE` so a successful second run does not clobber the
    /// first and a failed one leaves the row untouched.
    pub fn enrich_metadata(&self, context: &AppContext, entry_id: i64, source_app: Option<&str>) {
        let identifier = match source_app.map(str::trim).filter(|id| !id.is_empty()) {
            Some(identifier) => identifier,
            None => return,
        };
        let lookup = self.app_metadata.lookup(identifier);
        let metadata = match lookup {
            Ok(Some(metadata)) => metadata,
            Ok(None) => return,
            Err(ApplicationMetadataError::Unavailable) => return,
            Err(error) => {
                tracing::trace!(error = %error, "application metadata lookup failed");
                return;
            }
        };
        let name = metadata.display_name.trim();
        if name.is_empty() {
            return;
        }
        let mut db = context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        if let Err(error) = repo.set_source_app_metadata(
            entry_id,
            Some(name),
            metadata.icon_ref.as_deref(),
            self.clock.now(),
        ) {
            tracing::trace!(error = %error, "metadata persistence failed");
        }
    }

    /// List the most recent entries, newest first.
    pub fn recent_entries(
        &self,
        context: &AppContext,
        limit: usize,
    ) -> Result<Vec<clipvault_db::EntryRecord>, HistoryServiceError> {
        let mut db = context.database().lock();
        let repo = EntryRepository::new(db.connection_mut());
        Ok(repo.recent(limit)?)
    }

    /// Same as [`Self::recent_entries`] but optionally restricted to
    /// the entries that belong to `collection_id` (when supplied)
    /// and/or carry every tag in `tag_ids` (AND semantics).
    ///
    /// The query is *not* restricted to the textual content types:
    /// the rail renders image and rich-text rows through the same
    /// card surface, so a collection selection or a tag filter must
    /// keep them eligible. The textual-only variant is preserved on
    /// [`clipvault_db::EntryRepository::text_entries_filtered`] for
    /// the local search engine that deliberately never inspects
    /// image bytes.
    ///
    /// The optional `source_app` filter is appended to the same
    /// additive query the rest of the filter set drives: a
    /// `SourceAppFilter::All` reproduces the pre-extension
    /// behaviour bit-for-bit; `Known(identifier)` and `Unknown`
    /// restrict the candidate set without disturbing the
    /// chronological ordering.
    pub fn recent_entries_with_filter(
        &self,
        context: &AppContext,
        collection_id: Option<i64>,
        tag_ids: &[i64],
        source_app: &SourceAppFilter,
        limit: usize,
    ) -> Result<Vec<clipvault_db::EntryRecord>, HistoryServiceError> {
        let records = {
            let mut db = context.database().lock();
            let repo = EntryRepository::new(db.connection_mut());
            repo.entries_filtered(collection_id, tag_ids, source_app)?
        };
        // `entries_filtered` already enforces the ordering contract.
        // Truncate to the requested limit so the rail stays
        // predictable regardless of the filter cardinality.
        Ok(records.into_iter().take(limit).collect())
    }

    pub fn history_count(&self, context: &AppContext) -> Result<i64, HistoryServiceError> {
        let mut db = context.database().lock();
        let repo = EntryRepository::new(db.connection_mut());
        Ok(repo.count()?)
    }

    /// Validate the user-supplied title before the GUI persists it.
    /// The function enforces trim + length and returns a typed error
    /// so the caller can surface a localised message.
    pub fn validate_title(raw: &str) -> Result<Option<String>, TitleValidationError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        if trimmed.chars().count() > MAX_TITLE_LENGTH {
            return Err(TitleValidationError::TooLong(MAX_TITLE_LENGTH));
        }
        Ok(Some(trimmed.to_string()))
    }

    /// Set or restore the card title for `id`. The repository stores
    /// `None` when the caller passes `None` (restore default) and
    /// `Some(value)` after trimming / validation. The method is
    /// idempotent and never raises a fatal error for a missing id:
    /// it returns [`SetTitleOutcome::NotFound`] so the shell can
    /// surface a soft error to the UI.
    pub fn set_title(
        &self,
        context: &AppContext,
        id: i64,
        title: Option<&str>,
    ) -> Result<SetTitleOutcome, HistoryServiceError> {
        let validated = match title {
            Some(value) => Self::validate_title(value)?,
            None => None,
        };
        let mut db = context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        let outcome = repo.set_title(id, validated.as_deref(), self.clock.now())?;
        Ok(match outcome.updated {
            Some(record) => SetTitleOutcome::Updated { record },
            None => SetTitleOutcome::NotFound,
        })
    }
}

/// Reduce a caller-supplied source identifier to its canonical
/// form: trim surrounding whitespace and return `None` for the
/// empty result so the persistence path can store an explicit
/// `NULL` instead of a meaningless placeholder.
fn normalize_source_identifier(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Deterministic, non-cryptographic content fingerprint. Used as the
/// primary deduplication key by the storage layer.
pub fn hash_content(text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    "clipvault-history:v1".hash(&mut hasher);
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic() {
        assert_eq!(hash_content("hello"), hash_content("hello"));
    }

    #[test]
    fn hash_differs_for_different_inputs() {
        assert_ne!(hash_content("hello"), hash_content("world"));
    }

    #[test]
    fn hash_handles_empty_string() {
        let h = hash_content("");
        assert_eq!(h.len(), 16);
    }

    #[test]
    fn validate_title_trims_whitespace() {
        assert_eq!(
            TextHistoryService::validate_title("  hello  ").unwrap(),
            Some("hello".to_string())
        );
    }

    #[test]
    fn validate_title_maps_empty_to_none() {
        assert_eq!(TextHistoryService::validate_title("   ").unwrap(), None);
    }

    #[test]
    fn validate_title_rejects_overlong_input() {
        let long = "x".repeat(MAX_TITLE_LENGTH + 1);
        let err = TextHistoryService::validate_title(&long).unwrap_err();
        assert_eq!(err, TitleValidationError::TooLong(MAX_TITLE_LENGTH));
    }

    #[test]
    fn validate_title_accepts_boundary_length() {
        let exact = "x".repeat(MAX_TITLE_LENGTH);
        assert_eq!(
            TextHistoryService::validate_title(&exact).unwrap(),
            Some(exact)
        );
    }
}
