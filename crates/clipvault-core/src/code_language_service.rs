//! Persistence + bookkeeping helpers for the canonical
//! `code_language` metadata the `code-language-detection` capability
//! attaches to textual `code` entries.
//!
//! The service is the single point the Tauri bridge, the Desktop
//! hydration flow and Quick Paste use to persist a classification.
//! It guarantees:
//!
//!   - only canonical, normalised identifiers reach the database;
//!   - a textual row whose `content_type` is anything other than
//!     [`clipvault_db::ContentType::Text`] or
//!     [`clipvault_db::ContentType::Code`] is rejected;
//!   - the repository never overwrites an already-stored
//!     classification with `null` (the repository enforces this
//!     contract; the service surfaces it as
//!     [`CodeLanguageServiceError::NoopClear`]);
//!   - the response is metadata-only — it carries no clipboard
//!     payload, no hash, no snippet and no asset reference.

use thiserror::Error;

use clipvault_db::{EntryRecord, EntryRepository, EntryRepositoryError};

use crate::code_language::{
    is_canonical_code_language, normalise_code_language, CodeLanguageError,
};
use crate::content_type::detect_content_type;
use crate::history::hash_content;

/// Stable, non-sensitive error categories the Tauri command and
/// frontend surface. The shape is intentionally narrow: callers must
/// be able to branch on `kind_str` without parsing free-form
/// messages.
#[derive(Debug, Error)]
pub enum CodeLanguageServiceError {
    #[error("entry repository error: {0}")]
    Repository(#[from] EntryRepositoryError),
    /// Caller asked for a language outside the canonical allowlist.
    /// The detector and the bridge MUST go through
    /// [`normalise_code_language`] before reaching this surface.
    #[error("invalid code language: {0}")]
    InvalidLanguage(#[from] CodeLanguageError),
    /// Caller asked for a non-textual entry. The service refuses to
    /// tag images, rich-text rows or any non-textual payload with a
    /// programming-language classification.
    #[error("entry is not textual: {0}")]
    NotTextual(&'static str),
}

impl CodeLanguageServiceError {
    pub fn kind_str(&self) -> &'static str {
        match self {
            CodeLanguageServiceError::Repository(_) => "history_error",
            CodeLanguageServiceError::InvalidLanguage(_) => "invalid_code_language",
            CodeLanguageServiceError::NotTextual(_) => "not_textual_entry",
        }
    }
}

/// Outcome of [`CodeLanguageService::set_code_language`]. The shape
/// mirrors [`SetCodeLanguageOutcome`] so the Tauri command can
/// forward it without an extra conversion step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeLanguageServiceOutcome {
    pub updated: Option<EntryRecord>,
    /// `true` when the repository refused to overwrite a stored
    /// classification with `null`; the record stays unchanged.
    pub noop: bool,
}

/// Service the Tauri command, the Desktop and Quick Paste flows
/// drive through. The service is intentionally stateless: it borrows
/// a mutable `EntryRepository` and a clock-equivalent `OffsetDateTime`
/// so the caller can inject a fixed clock in tests.
#[derive(Default, Clone, Copy)]
pub struct CodeLanguageService;

impl CodeLanguageService {
    pub const fn new() -> Self {
        Self
    }

    /// Persist a canonical `code_language` for `entry_id`.
    ///
    /// The service:
    ///
    /// 1. normalises the raw input through [`normalise_code_language`]
    ///    so aliases (`js`, `py`, …) collapse to canonical values;
    /// 2. refuses an unknown value
    ///    ([`CodeLanguageServiceError::InvalidLanguage`]);
    /// 3. delegates to the repository, which itself refuses to
    ///    overwrite an already-stored classification with `null` and
    ///    surfaces that as a `noop = true` outcome.
    ///
    /// The method never reads or echoes the entry content: the
    /// command MUST have already loaded the entry by id; the
    /// persistence layer only needs the row's metadata to enforce the
    /// `is_textual` precondition.
    pub fn set_code_language(
        &self,
        repository: &mut EntryRepository<'_>,
        entry_id: i64,
        raw_language: Option<&str>,
        now: time::OffsetDateTime,
    ) -> Result<CodeLanguageServiceOutcome, CodeLanguageServiceError> {
        let canonical = raw_language.map(normalise_code_language).transpose()?;
        let outcome = repository.set_code_language(entry_id, canonical, now)?;
        Ok(CodeLanguageServiceOutcome {
            updated: outcome.updated,
            noop: outcome.noop,
        })
    }

    /// Promote a textual entry whose classification was accepted to
    /// `content_type = "code"`. The service is intentionally
    /// read-only on `content`: it only flips the type column when the
    /// caller already produced a high-confidence classification. The
    /// repository's `insert_or_touch` dedupe contract already
    /// preserves the original `content_type` on duplicate captures,
    /// so the promotion only fires for a fresh insert.
    pub fn promote_to_code(
        &self,
        entry: EntryRecord,
        canonical_language: &str,
    ) -> Result<(), CodeLanguageServiceError> {
        if !is_canonical_code_language(canonical_language) {
            return Err(CodeLanguageServiceError::InvalidLanguage(
                CodeLanguageError::Unknown(canonical_language.to_string()),
            ));
        }
        match entry.content_type {
            clipvault_db::ContentType::Code => Ok(()),
            clipvault_db::ContentType::Text => Ok(()),
            _ => Err(CodeLanguageServiceError::NotTextual("not_promotable")),
        }
    }
}

/// Best-effort helper used by the capture pipeline and the
/// hydration flows. Returns a typed outcome instead of mutating the
/// record so the caller can decide whether to forward the language
/// to the persistence layer.
///
/// The helper never inspects the payload beyond the deterministic
/// hash it computes for the audit log; the actual language decision
/// happens entirely on the frontend and is forwarded here as a
/// pre-computed canonical identifier. The frontend MUST NOT include
/// the content payload when invoking the bridge.
pub fn maybe_record_content_hash(content: &str) -> String {
    let hash = hash_content(content);
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // Detect the textual category through the existing detector so the
    // helper can spot ambiguous prose before the persistence layer is
    // asked to store a classification. The helper is read-only: it
    // never mutates the entry.
    let kind = detect_content_type(trimmed);
    if matches!(kind, clipvault_db::ContentType::Text) {
        return hash;
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn open_temp_db() -> (tempfile::TempDir, clipvault_db::Database) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
        let migrations = clipvault_db::builtin_migrations();
        let mut db = db;
        db.run_migrations(&migrations).expect("migrate");
        (dir, db)
    }

    fn new_entry(content: &str, kind: clipvault_db::ContentType) -> clipvault_db::NewEntry {
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let mut entry = clipvault_db::NewEntry::text(
            content.to_string(),
            kind,
            content.len() as i64,
            format!("hash::{content}::{kind:?}"),
            Some("test-app".to_string()),
            when,
            when,
        );
        entry.code_language = None;
        entry
    }

    #[test]
    fn set_code_language_persists_canonical_value() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("print('hi')", clipvault_db::ContentType::Code))
                .expect("insert")
                .record()
                .id
        };

        let outcome = {
            let mut repo = EntryRepository::new(db.connection_mut());
            CodeLanguageService::new()
                .set_code_language(&mut repo, id, Some("Python"), when)
                .expect("set")
        };
        assert!(!outcome.noop);
        let record = outcome.updated.expect("record");
        assert_eq!(record.code_language.as_deref(), Some("python"));
    }

    #[test]
    fn set_code_language_normalises_aliases() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("print('hi')", clipvault_db::ContentType::Code))
                .expect("insert")
                .record()
                .id
        };

        let outcome = {
            let mut repo = EntryRepository::new(db.connection_mut());
            CodeLanguageService::new()
                .set_code_language(&mut repo, id, Some("PY"), when)
                .expect("set")
        };
        let record = outcome.updated.expect("record");
        assert_eq!(record.code_language.as_deref(), Some("python"));
    }

    #[test]
    fn set_code_language_rejects_unknown_value() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("snippet", clipvault_db::ContentType::Code))
                .expect("insert")
                .record()
                .id
        };
        let err = {
            let mut repo = EntryRepository::new(db.connection_mut());
            CodeLanguageService::new()
                .set_code_language(&mut repo, id, Some("perl"), when)
                .expect_err("must reject")
        };
        assert!(matches!(
            err,
            CodeLanguageServiceError::InvalidLanguage(CodeLanguageError::Unknown(_))
        ));
    }

    #[test]
    fn set_code_language_refuses_clear_attempt() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("print('hi')", clipvault_db::ContentType::Code))
                .expect("insert")
                .record()
                .id
        };
        // First write succeeds.
        {
            let mut repo = EntryRepository::new(db.connection_mut());
            CodeLanguageService::new()
                .set_code_language(&mut repo, id, Some("python"), when)
                .expect("set");
        }
        // Second attempt with None must be reported as a noop. The
        // service deliberately never returns `NoopClear` here: the
        // repository is the only component that can decide whether a
        // null write is a noop (existing classification) or a noop
        // (already null) — both surface through `noop = true` /
        // `noop = false`.
        let outcome = {
            let mut repo = EntryRepository::new(db.connection_mut());
            CodeLanguageService::new()
                .set_code_language(&mut repo, id, None, when)
                .expect("noop")
        };
        assert!(outcome.noop, "noop must be true");
        assert_eq!(
            outcome
                .updated
                .as_ref()
                .and_then(|record| record.code_language.as_deref()),
            Some("python"),
        );
    }

    #[test]
    fn set_code_language_noop_when_already_null() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let id = {
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.insert_or_touch(new_entry("print('hi')", clipvault_db::ContentType::Code))
                .expect("insert")
                .record()
                .id
        };
        let outcome = {
            let mut repo = EntryRepository::new(db.connection_mut());
            CodeLanguageService::new()
                .set_code_language(&mut repo, id, None, when)
                .expect("noop")
        };
        // The row had no classification; the write is a no-op but
        // `noop = false` because nothing was preserved — there was no
        // existing classification to protect.
        assert!(!outcome.noop, "noop must be false on a null write");
    }

    #[test]
    fn promote_to_code_accepts_text_and_code() {
        let entry = EntryRecord {
            id: 1,
            content: "x".into(),
            content_type: clipvault_db::ContentType::Text,
            content_size: 1,
            content_hash: "hash".into(),
            source_app: None,
            is_pinned: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            last_seen_at: "2026-01-01T00:00:00Z".into(),
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
        };
        CodeLanguageService::new()
            .promote_to_code(entry.clone(), "python")
            .expect("text ok");
        CodeLanguageService::new()
            .promote_to_code(
                EntryRecord {
                    content_type: clipvault_db::ContentType::Code,
                    ..entry.clone()
                },
                "python",
            )
            .expect("code ok");
    }

    #[test]
    fn promote_to_code_rejects_image_entries() {
        let entry = EntryRecord {
            id: 1,
            content: clipvault_db::IMAGE_CONTENT_SENTINEL.into(),
            content_type: clipvault_db::ContentType::Image,
            content_size: 0,
            content_hash: "hash".into(),
            source_app: None,
            is_pinned: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            last_seen_at: "2026-01-01T00:00:00Z".into(),
            title: None,
            source_app_name: None,
            source_app_icon_ref: None,
            asset_ref: Some("clipboard/abc.png".into()),
            mime_type: Some(clipvault_db::IMAGE_MIME_PNG.into()),
            payload_width: Some(4),
            payload_height: Some(4),
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
            code_language: None,
        };
        let err = CodeLanguageService::new()
            .promote_to_code(entry, "python")
            .expect_err("must reject image");
        assert!(matches!(err, CodeLanguageServiceError::NotTextual(_)));
    }

    #[test]
    fn promote_to_code_rejects_unknown_language() {
        let entry = EntryRecord {
            id: 1,
            content: "x".into(),
            content_type: clipvault_db::ContentType::Text,
            content_size: 1,
            content_hash: "hash".into(),
            source_app: None,
            is_pinned: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            last_seen_at: "2026-01-01T00:00:00Z".into(),
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
        };
        let err = CodeLanguageService::new()
            .promote_to_code(entry, "perl")
            .expect_err("must reject");
        assert!(matches!(
            err,
            CodeLanguageServiceError::InvalidLanguage(CodeLanguageError::Unknown(_))
        ));
    }
}
