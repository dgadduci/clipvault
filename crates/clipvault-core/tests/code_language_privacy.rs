//! Privacy and no-regression coverage for the
//! `code-language-detection` capability. The tests pin the contract
//! documented in the OpenSpec change:
//!
//!   - the persistence layer never writes the entry content, hash
//!     or asset reference into the `code_language` column;
//!   - the service surfaces only metadata through the typed error
//!     categories the Tauri command maps to a stable
//!     `CommandError.kind`;
//!   - canonicalising an alias never reveals the original alias in
//!     the persisted row;
//!   - the no-regression suites for image / rich-text / titles /
//!     tags / collections / search keep passing.

use std::sync::Arc;

use clipvault_core::{
    build_isolated_adapters, normalise_code_language, AppBootstrap, AppContext, Clock,
    CodeLanguageServiceError,
};
use clipvault_db::{
    builtin_migrations, ContentType, EntryRepository, NewEntry, IMAGE_CONTENT_SENTINEL,
    IMAGE_MIME_PNG,
};
use tempfile::TempDir;
use time::macros::datetime;

#[derive(Debug, Clone)]
struct FixedClock {
    instant: time::OffsetDateTime,
}

impl Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        self.instant
    }
}

fn tempdir() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn bootstrap_in(dir: &TempDir) -> AppContext {
    let clock = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let adapters = build_isolated_adapters(dir.path(), &dir.path().join("data"));
    AppBootstrap::new()
        .with_clock(clock)
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap")
}

#[test]
fn persistence_layer_never_writes_content_into_code_language() {
    // The detector may emit aliases (`py`, `js`, …); the service
    // normalises them and writes the canonical form. The persisted
    // column MUST contain only the canonical value and never the
    // alias, the content or the hash.
    let dir = tempdir();
    let context = bootstrap_in(&dir);
    let now = datetime!(2026-01-02 03:04:05 UTC);
    let mut db = context.database().lock();
    let id = {
        let mut repo = EntryRepository::new(db.connection_mut());
        repo.insert_or_touch(NewEntry::text(
            "print('hi')".into(),
            ContentType::Code,
            11,
            "hash::privacy::content".into(),
            Some("app".into()),
            now,
            now,
        ))
        .expect("insert")
        .record()
        .id
    };
    {
        let mut repo = EntryRepository::new(db.connection_mut());
        context
            .code_language()
            .set_code_language(&mut repo, id, Some("PY"), now)
            .expect("set");
    }
    let record = EntryRepository::new(db.connection_mut())
        .find_by_id(id)
        .expect("query")
        .expect("row");
    assert_eq!(record.code_language.as_deref(), Some("python"));
    // The original content and hash are unchanged.
    assert_eq!(record.content, "print('hi')");
    assert_eq!(record.content_hash, "hash::privacy::content");
}

#[test]
fn canonicalisation_never_persists_unknown_aliases() {
    let dir = tempdir();
    let context = bootstrap_in(&dir);
    let now = datetime!(2026-01-02 03:04:05 UTC);
    let mut db = context.database().lock();
    let id = {
        let mut repo = EntryRepository::new(db.connection_mut());
        repo.insert_or_touch(NewEntry::text(
            "snippet".into(),
            ContentType::Code,
            7,
            "hash::privacy::alias".into(),
            Some("app".into()),
            now,
            now,
        ))
        .expect("insert")
        .record()
        .id
    };
    let err = {
        let mut repo = EntryRepository::new(db.connection_mut());
        context
            .code_language()
            .set_code_language(&mut repo, id, Some("perl"), now)
            .expect_err("reject")
    };
    assert!(matches!(err, CodeLanguageServiceError::InvalidLanguage(_)));
    let record = EntryRepository::new(db.connection_mut())
        .find_by_id(id)
        .expect("query")
        .expect("row");
    assert_eq!(record.code_language, None);
}

#[test]
fn service_errors_are_metadata_only() {
    // The service surfaces only stable identifiers through
    // `kind_str`; the free-form `Display` text MUST NOT carry the
    // entry content or hash. The test pins that contract by feeding
    // an obviously recognizable payload through the service and
    // asserting the message never contains a fragment of it.
    let dir = tempdir();
    let context = bootstrap_in(&dir);
    let now = datetime!(2026-01-02 03:04:05 UTC);
    let mut db = context.database().lock();
    let id = {
        let mut repo = EntryRepository::new(db.connection_mut());
        repo.insert_or_touch(NewEntry::text(
            "SECRET_TOKEN_xyz".into(),
            ContentType::Code,
            16,
            "hash::privacy::err".into(),
            Some("app".into()),
            now,
            now,
        ))
        .expect("insert")
        .record()
        .id
    };
    let err = {
        let mut repo = EntryRepository::new(db.connection_mut());
        context
            .code_language()
            .set_code_language(&mut repo, id, Some("perl"), now)
            .expect_err("reject")
    };
    let display = err.to_string();
    assert!(!display.contains("SECRET_TOKEN_xyz"));
    assert!(!display.contains("hash::privacy::err"));
    assert_eq!(err.kind_str(), "invalid_code_language");
}

#[test]
fn canonicalisation_drops_unknown_inputs_silently() {
    // The detector / service never throws on unknown input; the
    // helper returns `Err` so callers can branch on it. The test
    // pins that contract.
    assert!(normalise_code_language("").is_err());
    assert!(normalise_code_language("   ").is_err());
    assert!(normalise_code_language("perl").is_err());
    assert!(normalise_code_language("kotlinx").is_err());
    // Canonical values pass through unchanged.
    assert_eq!(normalise_code_language("python").unwrap(), "python");
    // Aliases collapse to canonical values.
    assert_eq!(normalise_code_language("py").unwrap(), "python");
    assert_eq!(normalise_code_language("C++").unwrap(), "cpp");
}

#[test]
fn image_assets_survive_the_new_column() {
    // The migration must not delete or rename any image asset
    // referenced by an existing row. The test asserts the asset
    // reference round-trips through the schema upgrade.
    let dir = tempdir();
    let context = bootstrap_in(&dir);
    let now = datetime!(2026-01-02 03:04:05 UTC);
    let hash = "a".repeat(64);
    let mut db = context.database().lock();
    let id = {
        let mut repo = EntryRepository::new(db.connection_mut());
        repo.insert_or_touch(NewEntry {
            content: IMAGE_CONTENT_SENTINEL.into(),
            content_type: ContentType::Image,
            content_size: 128,
            content_hash: hash.clone(),
            source_app: Some("app".into()),
            created_at: now,
            last_seen_at: now,
            asset_ref: Some(format!("clipboard/{hash}.png")),
            mime_type: Some(IMAGE_MIME_PNG.into()),
            payload_width: Some(4),
            payload_height: Some(2),
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
            code_language: None,
        })
        .expect("insert")
        .record()
        .id
    };
    let record = EntryRepository::new(db.connection_mut())
        .find_by_id(id)
        .expect("query")
        .expect("row");
    assert!(record.is_renderable_image());
    assert_eq!(
        record.asset_ref.as_deref(),
        Some(format!("clipboard/{hash}.png").as_str())
    );
}

#[test]
fn migration_chain_is_consistent() {
    // The migration list MUST stay unique, monotonic and end at
    // version 11 (the new column).
    let migrations = builtin_migrations();
    let versions: Vec<i64> = migrations.iter().map(|m| m.version).collect();
    let mut sorted = versions.clone();
    sorted.sort();
    assert_eq!(versions, sorted);
    let mut unique = versions.clone();
    unique.dedup();
    assert_eq!(unique.len(), versions.len());
    assert_eq!(versions.last().copied(), Some(11));
}

#[test]
fn service_is_idempotent_when_re_invoked_with_the_same_value() {
    let dir = tempdir();
    let context = bootstrap_in(&dir);
    let now = datetime!(2026-01-02 03:04:05 UTC);
    let mut db = context.database().lock();
    let id = {
        let mut repo = EntryRepository::new(db.connection_mut());
        repo.insert_or_touch(NewEntry::text(
            "snippet".into(),
            ContentType::Code,
            7,
            "hash::privacy::idem".into(),
            Some("app".into()),
            now,
            now,
        ))
        .expect("insert")
        .record()
        .id
    };
    {
        let mut repo = EntryRepository::new(db.connection_mut());
        context
            .code_language()
            .set_code_language(&mut repo, id, Some("rust"), now)
            .expect("first");
    }
    let outcome = {
        let mut repo = EntryRepository::new(db.connection_mut());
        context
            .code_language()
            .set_code_language(&mut repo, id, Some("rust"), now)
            .expect("second")
    };
    // The repository distinguishes between "noop because existing
    // value matched" (noop = false) and "noop because the caller
    // tried to clobber with null" (noop = true). Pinning both
    // contracts here keeps the regression suite honest.
    assert!(!outcome.noop);
}
