use std::sync::Arc;

use clipvault_core::{
    AppBootstrap, AppContext, ApplicationMetadata, ApplicationMetadataError,
    ApplicationMetadataProvider, Clipboard, ClipboardError, Clock, DiagnosticsService,
    FakeApplicationMetadataProvider, FakeClipboard, HistoryOutcome, SetTitleOutcome,
    TextHistoryService, MAX_TITLE_LENGTH,
};
use clipvault_db::builtin_migrations;
use parking_lot::Mutex;
use tempfile::TempDir;
use time::macros::datetime;

#[derive(Debug, Clone)]
struct FixedClock {
    instant: time::OffsetDateTime,
}

impl clipvault_core::Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        self.instant
    }
}

#[derive(Debug)]
struct ScriptedClipboard {
    responses: Mutex<Vec<Result<Option<String>, ClipboardError>>>,
}

impl ScriptedClipboard {
    fn new(responses: Vec<Result<Option<String>, ClipboardError>>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }
}

impl Clipboard for ScriptedClipboard {
    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        self.responses
            .lock()
            .pop()
            .unwrap_or(Err(ClipboardError::Empty))
    }
}

fn tempdir() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn bootstrap_with_clock_and_clipboard(
    dir: &TempDir,
    clock: Arc<dyn Clock>,
    clipboard: Arc<dyn Clipboard>,
) -> AppContext {
    AppBootstrap::new()
        .with_clock(clock)
        .with_clipboard(clipboard)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap")
}

fn service_for(context: &AppContext) -> TextHistoryService {
    context.history().clone()
}

#[test]
fn captures_new_text_and_stores_it() {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text("hello world"));
    let context = bootstrap_with_clock_and_clipboard(&dir, clock, clipboard);

    let outcome = service_for(&context).record_text(&context, Some("TestApp"));
    assert!(
        matches!(outcome, HistoryOutcome::Stored { .. }),
        "got {outcome:?}"
    );

    let recent = service_for(&context).recent_entries(&context, 10).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].content, "hello world");
    assert_eq!(recent[0].source_app.as_deref(), Some("TestApp"));
    assert_eq!(recent[0].content_size, "hello world".len() as i64);
    assert!(!recent[0].content_hash.is_empty());
    assert_eq!(recent[0].created_at, recent[0].updated_at);
}

#[test]
fn duplicate_text_does_not_create_second_row() {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text("duplicate content"));
    let context = bootstrap_with_clock_and_clipboard(&dir, clock, clipboard);

    let first = service_for(&context).record_text(&context, None);
    let second = service_for(&context).record_text(&context, None);

    assert!(matches!(first, HistoryOutcome::Stored { .. }));
    let first_id = first.id().unwrap();
    assert!(matches!(second, HistoryOutcome::Duplicate { id } if id == first_id));

    let recent = service_for(&context).recent_entries(&context, 10).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(service_for(&context).history_count(&context).unwrap(), 1);
}

#[test]
fn different_content_creates_separate_rows() {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text("alpha"));
    let context = bootstrap_with_clock_and_clipboard(&dir, clock.clone(), clipboard);

    let first = service_for(&context).record_text(&context, None);
    let second_clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text("beta"));
    let context2 = bootstrap_with_clock_and_clipboard(&dir, clock, second_clipboard);
    let second = service_for(&context2).record_text(&context2, None);

    assert_ne!(first.id(), second.id());
    assert_eq!(service_for(&context2).history_count(&context2).unwrap(), 2);
}

#[test]
fn ignored_when_clipboard_has_no_text() {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::new());
    let context = bootstrap_with_clock_and_clipboard(&dir, clock, clipboard);

    let outcome = service_for(&context).record_text(&context, None);
    assert_eq!(outcome, HistoryOutcome::Ignored);
    assert_eq!(service_for(&context).history_count(&context).unwrap(), 0);
}

#[test]
fn ignored_when_clipboard_text_is_empty() {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text(""));
    let context = bootstrap_with_clock_and_clipboard(&dir, clock, clipboard);

    let outcome = service_for(&context).record_text(&context, Some("TestApp"));
    assert_eq!(outcome, HistoryOutcome::Ignored);
    assert_eq!(service_for(&context).history_count(&context).unwrap(), 0);
}

#[test]
fn failed_when_clipboard_backend_returns_error() {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(ScriptedClipboard::new(vec![Err(
        ClipboardError::Backend("backend unavailable".into()),
    )]));
    let context = bootstrap_with_clock_and_clipboard(&dir, clock, clipboard);

    let outcome = service_for(&context).record_text(&context, None);
    assert!(
        matches!(outcome, HistoryOutcome::Failed { .. }),
        "got {outcome:?}"
    );
    assert_eq!(service_for(&context).history_count(&context).unwrap(), 0);
}

#[test]
fn diagnostics_report_history_entries() {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text("first"));
    let context = bootstrap_with_clock_and_clipboard(&dir, clock, clipboard);

    let diag = DiagnosticsService::snapshot(&context);
    assert_eq!(diag.history_entries, 0);
    assert_eq!(diag.migrations_applied, builtin_migrations().len());

    service_for(&context).record_text(&context, None);
    let diag = DiagnosticsService::snapshot(&context);
    assert_eq!(diag.history_entries, 1);
}

#[test]
fn history_persists_across_reopens() {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text("persistent"));
    let context = bootstrap_with_clock_and_clipboard(&dir, clock.clone(), clipboard);
    service_for(&context).record_text(&context, None);
    drop(context);

    let clipboard2: Arc<dyn Clipboard> = Arc::new(FakeClipboard::new());
    let context2 = bootstrap_with_clock_and_clipboard(&dir, clock, clipboard2);
    assert_eq!(service_for(&context2).history_count(&context2).unwrap(), 1);
    let recent = service_for(&context2)
        .recent_entries(&context2, 10)
        .unwrap();
    assert_eq!(recent[0].content, "persistent");
}

#[test]
fn capture_enriches_record_with_application_metadata() {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text("hello"));
    let context = bootstrap_with_clock_and_clipboard(&dir, clock, clipboard);
    let provider = Arc::new(FakeApplicationMetadataProvider::new());
    provider.push_lookup(Ok(Some(ApplicationMetadata {
        display_name: "TextEdit".to_string(),
        icon_ref: Some("application-icons/com.apple.textedit.png".to_string()),
    })));
    let history = service_for(&context)
        .with_app_metadata_provider(provider.clone() as Arc<dyn ApplicationMetadataProvider>);

    let outcome = history.record_text(&context, Some("com.apple.TextEdit"));
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));

    let recent = history.recent_entries(&context, 10).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].source_app_name.as_deref(), Some("TextEdit"));
    assert_eq!(
        recent[0].source_app_icon_ref.as_deref(),
        Some("application-icons/com.apple.textedit.png")
    );
    assert_eq!(provider.calls(), vec!["com.apple.TextEdit".to_string()]);
}

#[test]
fn capture_succeeds_when_metadata_provider_is_unavailable() {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text("hello"));
    let context = bootstrap_with_clock_and_clipboard(&dir, clock, clipboard);
    let provider = Arc::new(FakeApplicationMetadataProvider::new());
    provider.push_lookup(Err(ApplicationMetadataError::Unavailable));
    let history = service_for(&context)
        .with_app_metadata_provider(provider.clone() as Arc<dyn ApplicationMetadataProvider>);

    let outcome = history.record_text(&context, Some("com.apple.TextEdit"));
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));

    let recent = history.recent_entries(&context, 10).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].source_app_name, None);
    assert_eq!(recent[0].source_app_icon_ref, None);
}

#[test]
fn capture_skips_metadata_when_source_identifier_is_missing() {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text("hello"));
    let context = bootstrap_with_clock_and_clipboard(&dir, clock, clipboard);
    let provider = Arc::new(FakeApplicationMetadataProvider::new());
    let history = service_for(&context)
        .with_app_metadata_provider(provider.clone() as Arc<dyn ApplicationMetadataProvider>);

    let outcome = history.record_text(&context, None);
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));

    assert!(
        provider.calls().is_empty(),
        "provider must not be queried when source_app is None"
    );
}

#[test]
fn set_title_persists_validates_and_restores() {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text("hello"));
    let context = bootstrap_with_clock_and_clipboard(&dir, clock, clipboard);
    let history = service_for(&context);

    let outcome = history.record_text(&context, Some("TestApp"));
    let id = outcome.id().expect("stored");

    let updated = history
        .set_title(&context, id, Some("Custom"))
        .expect("set_title");
    assert!(matches!(updated, SetTitleOutcome::Updated { .. }));

    let recent = history.recent_entries(&context, 10).unwrap();
    assert_eq!(recent[0].title.as_deref(), Some("Custom"));

    let restored = history.set_title(&context, id, None).expect("restore");
    assert!(matches!(restored, SetTitleOutcome::Updated { .. }));

    let recent = history.recent_entries(&context, 10).unwrap();
    assert_eq!(recent[0].title, None);
}

#[test]
fn set_title_rejects_overlong_input() {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text("hello"));
    let context = bootstrap_with_clock_and_clipboard(&dir, clock, clipboard);
    let history = service_for(&context);

    let outcome = history.record_text(&context, None);
    let id = outcome.id().expect("stored");

    let long = "x".repeat(MAX_TITLE_LENGTH + 1);
    let err = history
        .set_title(&context, id, Some(&long))
        .expect_err("must reject");
    match err {
        clipvault_core::HistoryServiceError::InvalidTitle(
            clipvault_core::TitleValidationError::TooLong(limit),
        ) => assert_eq!(limit, MAX_TITLE_LENGTH),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn set_title_for_missing_id_returns_not_found() {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-01-02 03:04:05 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text("hello"));
    let context = bootstrap_with_clock_and_clipboard(&dir, clock, clipboard);
    let history = service_for(&context);

    let outcome = history
        .set_title(&context, 999, Some("label"))
        .expect("not_found");
    assert!(matches!(outcome, SetTitleOutcome::NotFound));
}
