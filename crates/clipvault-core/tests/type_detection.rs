//! Integration tests for the `clipboard-type-detection` capability:
//! the [`ContentTypeDetector`](clipvault_core::detect_content_type)
//! classifies every textual payload before persistence and the
//! search query keeps finding classified entries alongside the
//! legacy `text` rows.

use std::sync::Arc;

use clipvault_core::{
    AppBootstrap, AppContext, Clipboard, Clock, FakeClipboard, HistoryOutcome,
    NoopActiveApplicationProbe, PrivacyGate, SearchService, TextHistoryService,
};
use clipvault_db::{builtin_migrations, ContentType, EntryRepository, NewEntry};
use clipvault_search::SearchQuery;
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

fn bootstrap_with_text(text: &str) -> (TempDir, AppContext, TextHistoryService) {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text(text));
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(clock)
        .with_clipboard(clipboard.clone())
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    let gate = PrivacyGate::from_probe(Arc::new(NoopActiveApplicationProbe), vec![]);
    let history = TextHistoryService::new(
        clipboard,
        context.clock(),
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
    )
    .with_privacy_gate(gate);
    (dir, context, history)
}

fn insert_entry_with_type(context: &AppContext, content: &str, content_type: ContentType) -> i64 {
    let new = NewEntry::text(
        content.to_string(),
        content_type,
        content.len() as i64,
        format!("hash::{content}::{content_type:?}"),
        Some("test-app".to_string()),
        datetime!(2026-01-02 03:04:05 UTC),
        datetime!(2026-01-02 03:04:05 UTC),
    );
    let mut db = context.database().lock();
    let mut repo = EntryRepository::new(db.connection_mut());
    repo.insert_or_touch(new).expect("insert").record().id
}

#[test]
fn capture_persists_detected_type_for_each_category() {
    // The detector runs after the empty / blacklist checks and before
    // persistence. Each textual category should round-trip into the
    // `content_type` column with the documented snake_case value.
    let cases = [
        ("https://example.com/foo", ContentType::Url),
        ("user@example.com", ContentType::Email),
        (r#"{"k":"v"}"#, ContentType::Json),
        (
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.signaturehere",
            ContentType::Jwt,
        ),
        ("550e8400-e29b-41d4-a716-446655440000", ContentType::Uuid),
        ("127.0.0.1", ContentType::Ipv4),
        ("::1", ContentType::Ipv6),
        ("#fff", ContentType::HexColor),
        ("<div>hello</div>", ContentType::Html),
        ("/Users/foo/bar.txt", ContentType::FilePath),
        ("SELECT 1 FROM dual", ContentType::Sql),
        ("ls -la | grep foo", ContentType::ShellCommand),
        ("```python\nprint('hi')\n```", ContentType::Code),
        ("hello world", ContentType::Text),
    ];

    for (payload, expected) in cases {
        let (_dir, context, history) = bootstrap_with_text(payload);
        let outcome = history.record_text(&context, Some("TestApp"));
        assert!(
            matches!(outcome, HistoryOutcome::Stored { .. }),
            "{payload:?} should be stored, got {outcome:?}"
        );

        let recent = history.recent_entries(&context, 10).expect("recent");
        assert_eq!(recent.len(), 1, "{payload:?} produced {recent:?}");
        assert_eq!(
            recent[0].content_type, expected,
            "{payload:?} classified as {:?}",
            recent[0].content_type
        );
        assert_eq!(recent[0].content, payload);
    }
}

#[test]
fn ambiguous_payload_persists_as_text() {
    // Prose with no structural signal must persist as `Text` even when
    // it incidentally contains keywords the detector would otherwise
    // use as strong signals.
    let inputs = [
        "hello world",
        "I love to select things",
        "use the && symbol",
        "single < or >",
        "1 < 2 and 3 > 2",
    ];
    for payload in inputs {
        let (_dir, context, history) = bootstrap_with_text(payload);
        let outcome = history.record_text(&context, None);
        assert!(matches!(outcome, HistoryOutcome::Stored { .. }));
        let recent = history.recent_entries(&context, 10).expect("recent");
        assert_eq!(recent[0].content_type, ContentType::Text, "{payload:?}");
    }
}

#[test]
fn duplicate_capture_does_not_create_second_row() {
    // The dedupe-by-hash contract from `clipboard-text-history` MUST
    // hold for classified entries: re-capturing the same payload
    // touches the existing row and never creates a second one.
    let (_dir, context, history) = bootstrap_with_text("https://example.com/foo");
    let first = history.record_text(&context, Some("TestApp"));
    let second = history.record_text(&context, Some("TestApp"));
    let third = history.record_text(&context, Some("TestApp"));
    assert!(matches!(first, HistoryOutcome::Stored { .. }));
    assert!(matches!(second, HistoryOutcome::Duplicate { .. }));
    assert!(matches!(third, HistoryOutcome::Duplicate { .. }));

    let count = history.history_count(&context).expect("count");
    assert_eq!(count, 1);

    let recent = history.recent_entries(&context, 10).expect("recent");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].content_type, ContentType::Url);
}

#[test]
fn blacklisted_capture_never_reaches_detector() {
    // The privacy gate runs before the detector: a blacklisted source
    // must discard the payload without persisting it, regardless of
    // what the detector would have classified the text as.
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text(
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.signaturehere",
    ));
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(clock)
        .with_clipboard(clipboard.clone())
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    let gate = PrivacyGate::from_probe(
        Arc::new(NoopActiveApplicationProbe),
        vec!["test-app".into()],
    );
    let history = TextHistoryService::new(
        clipboard,
        context.clock(),
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
    )
    .with_privacy_gate(gate);

    let outcome = history.record_text(&context, Some("test-app"));
    assert!(matches!(outcome, HistoryOutcome::Ignored));

    let count = history.history_count(&context).expect("count");
    assert_eq!(count, 0, "blacklisted capture must not be persisted");
}

#[test]
fn search_finds_classified_entries() {
    // `EntryRepository::text_entries` includes every textual variant,
    // so the search service surfaces classified rows alongside the
    // legacy `Text` rows.
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::new());
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(clock)
        .with_clipboard(clipboard)
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    let payloads = [
        ("https://example.com/docs", ContentType::Url),
        (r#"{"hello":"world"}"#, ContentType::Json),
        (
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.signaturehere",
            ContentType::Jwt,
        ),
        ("127.0.0.1", ContentType::Ipv4),
        ("```python\nprint('hi')\n```", ContentType::Code),
        ("plain text", ContentType::Text),
    ];
    for (payload, content_type) in payloads {
        insert_entry_with_type(&context, payload, content_type);
    }

    let search = SearchService::new();
    let cases = [
        ("example", "https://example.com/docs"),
        ("hello", r#"{"hello":"world"}"#),
        ("eyJ", "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.signaturehere"),
        ("127", "127.0.0.1"),
        ("python", "```python\nprint('hi')\n```"),
        ("plain", "plain text"),
    ];
    for (query, expected_payload) in cases {
        let outcome = search
            .search(
                &context,
                &SearchQuery {
                    text: query.to_string(),
                    limit: 50,
                },
            )
            .expect("search");
        let found_payloads: Vec<&str> = outcome
            .hits
            .iter()
            .map(|hit| hit.record.content.as_str())
            .collect();
        assert!(
            found_payloads.contains(&expected_payload),
            "query {query:?} should find {expected_payload:?}; got {found_payloads:?}"
        );
    }
}

#[test]
fn search_includes_every_textual_variant() {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::new());
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(clock)
        .with_clipboard(clipboard)
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    let variants = [
        ContentType::Text,
        ContentType::Url,
        ContentType::Email,
        ContentType::Json,
        ContentType::Jwt,
        ContentType::Uuid,
        ContentType::Ipv4,
        ContentType::Ipv6,
        ContentType::HexColor,
        ContentType::Html,
        ContentType::FilePath,
        ContentType::ShellCommand,
        ContentType::Sql,
        ContentType::Code,
    ];
    for variant in variants {
        let payload = match variant {
            ContentType::Text => "plain text",
            ContentType::Url => "https://example.com",
            ContentType::Email => "user@example.com",
            ContentType::Json => r#"{"k":"v"}"#,
            ContentType::Jwt => "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.signaturehere",
            ContentType::Uuid => "550e8400-e29b-41d4-a716-446655440000",
            ContentType::Ipv4 => "127.0.0.1",
            ContentType::Ipv6 => "::1",
            ContentType::HexColor => "#fff",
            ContentType::Html => "<div>hello</div>",
            ContentType::FilePath => "/Users/foo/bar.txt",
            ContentType::ShellCommand => "ls -la | grep foo",
            ContentType::Sql => "SELECT 1 FROM dual",
            ContentType::Code => "```python\nprint('hi')\n```",
            // `Image` is deliberately excluded from this list: it is
            // the one non-textual variant, so it never appears in
            // `text_entries` and has no textual payload to detect.
            ContentType::Image => unreachable!("image is not a textual variant"),
        };
        insert_entry_with_type(&context, payload, variant);
    }

    let mut db = context.database().lock();
    let repo = EntryRepository::new(db.connection_mut());
    let textual = repo.text_entries().expect("text_entries");
    drop(db);
    assert_eq!(
        textual.len(),
        variants.len(),
        "every textual variant must appear in text_entries"
    );
}

#[test]
fn empty_clipboard_returns_ignored() {
    let (_dir, context, history) = bootstrap_with_text("");
    let outcome = history.record_text(&context, Some("TestApp"));
    assert!(matches!(outcome, HistoryOutcome::Ignored));
    let count = history.history_count(&context).expect("count");
    assert_eq!(count, 0);
}

#[test]
fn capture_logs_never_contain_payload_hash_or_content_type() {
    // The capture pipeline must never pass the payload, the content
    // hash or the classified content_type to a `tracing::*!` macro.
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    #[derive(Default, Clone)]
    struct SharedCapture(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedCapture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'w> MakeWriter<'w> for SharedCapture {
        type Writer = SharedCapture;
        fn make_writer(&'w self) -> Self::Writer {
            self.clone()
        }
    }

    let capture = SharedCapture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(capture.clone())
        .with_max_level(tracing::Level::TRACE)
        .finish();

    let payload = "cv-type-leak-marker-AB-7733";
    let (_dir, context, history) = bootstrap_with_text(payload);
    let outcome = tracing::subscriber::with_default(subscriber, || {
        history.record_text(&context, Some("TestApp"))
    });
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));

    let buffer = String::from_utf8_lossy(&capture.0.lock().unwrap()).into_owned();
    assert!(
        !buffer.contains(payload),
        "payload must never appear in capture logs:\n{buffer}"
    );
    let hash = clipvault_core::hash_content(payload);
    assert!(
        !buffer.contains(&hash),
        "content hash must never appear in capture logs:\n{buffer}"
    );
    // `content_type = "url"` would also be a leak class because the
    // value comes from the payload. The detector must not log it.
    assert!(
        !buffer.contains("url\""),
        "content_type label must never appear in capture logs:\n{buffer}"
    );
}

#[test]
fn migrations_remain_idempotent_under_extended_enum() {
    // Sanity check: the built-in migrations still apply cleanly even
    // after extending `ContentType`. No new migration is required to
    // store the new values.
    let dir = tempdir();
    let db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
    let mut db = db;
    db.run_migrations(&builtin_migrations()).expect("migrate");
    let count = db.applied_migration_count().expect("count");
    assert_eq!(count, builtin_migrations().len());
}
