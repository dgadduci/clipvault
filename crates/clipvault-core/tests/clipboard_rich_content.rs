//! End-to-end integration tests for the `clipboard-rich-content`
//! capability.
//!
//! The suite walks the whole chain the change contract describes:
//!
//! ```text
//! clipboard adapter -> payload -> PrivacyGate -> PNG normalization
//!   -> hash/dedupe -> local asset -> SQLite -> EntryRecord
//!   -> safe asset read -> paste
//! ```
//!
//! Everything runs against fakes and a temporary data directory, so no
//! real clipboard, graphical session or user data is involved.

use std::collections::BTreeSet;
use std::sync::Arc;

use clipvault_core::{
    AppBootstrap, AppContext, AssetError, Capabilities, ClipboardAssetStore, ClipboardBackend,
    ClipboardBackendError, ClipboardImage, ClipboardPayload, DisplayServer, FakeActiveApplication,
    FakeClipboardBackend, FakeHotkeyManager, FakePasteController, FakeSettingsNavigator,
    FakeTrayController, HistoryOutcome, OsFamily, PasteMode, PasteOutcome, PlatformAdapters,
    PlatformInfo, PlatformIssueKind, RetentionPolicy, SettingsReader, SourceAppFilter,
    WatchTickOutcome, CLIPBOARD_WRITE_IMAGE_CAPABILITY,
};
use clipvault_db::{ContentType, EntryRepository};
use tempfile::TempDir;
use time::macros::datetime;

// ---------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------

#[derive(Debug, Clone)]
struct FixedClock {
    instant: time::OffsetDateTime,
}

impl clipvault_core::Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        self.instant
    }
}

/// Deterministic settings reader so retention tests do not depend on
/// what the `app_settings` table happens to hold.
struct FixedRetention(RetentionPolicy);

impl SettingsReader for FixedRetention {
    fn retention_policy(&self) -> RetentionPolicy {
        self.0
    }
}

fn bitmap(width: u32, height: u32, fill: u8) -> ClipboardImage {
    let len = (width as usize) * (height as usize) * 4;
    ClipboardImage::new(vec![fill; len], width, height).expect("valid bitmap")
}

struct Harness {
    _dir: TempDir,
    context: AppContext,
    clipboard: Arc<FakeClipboardBackend>,
    paste: Arc<FakePasteController>,
    store: ClipboardAssetStore,
}

impl Harness {
    fn assets_on_disk(&self) -> Vec<String> {
        let root = self.store.root();
        let Ok(entries) = std::fs::read_dir(root) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }

    fn referenced_asset_refs(&self) -> BTreeSet<String> {
        let mut db = self.context.database().lock();
        let repo = EntryRepository::new(db.connection_mut());
        repo.referenced_asset_refs().expect("refs")
    }

    fn record(&self, id: i64) -> clipvault_db::EntryRecord {
        let mut db = self.context.database().lock();
        let repo = EntryRepository::new(db.connection_mut());
        repo.find_by_id(id).expect("query").expect("row present")
    }

    fn history_count(&self) -> i64 {
        let mut db = self.context.database().lock();
        let repo = EntryRepository::new(db.connection_mut());
        repo.count().expect("count")
    }
}

/// Build a harness whose data directory is a fresh tempdir, so the
/// asset store is fully isolated from the developer's `~/.clipvault`.
fn harness(ignored_apps: Vec<String>) -> Harness {
    harness_on(OsFamily::Macos, DisplayServer::Unknown, ignored_apps, true)
}

fn harness_on(
    os_family: OsFamily,
    display_server: DisplayServer,
    ignored_apps: Vec<String>,
    image_support: bool,
) -> Harness {
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");

    let clipboard = Arc::new(FakeClipboardBackend::new());
    clipboard.set_image_support(image_support, image_support);
    let paste = Arc::new(FakePasteController::new());
    let active_app = Arc::new(FakeActiveApplication::new());

    let adapters = PlatformAdapters::new(
        Arc::clone(&clipboard) as Arc<dyn ClipboardBackend>,
        Arc::new(FakeHotkeyManager::new()),
        active_app,
        Arc::clone(&paste) as Arc<dyn clipvault_core::PasteController>,
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        Capabilities::ALL_AVAILABLE,
        PlatformInfo {
            home_dir: dir.path().to_path_buf(),
            data_dir: data_dir.clone(),
            os_family,
            display_server,
        },
    );

    let context = AppBootstrap::new()
        .with_clock(Arc::new(FixedClock {
            instant: datetime!(2026-01-02 03:04:05 UTC),
        }))
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    for id in ignored_apps {
        context
            .settings()
            .add_ignored(&context, &id)
            .expect("blacklist the application");
    }

    Harness {
        _dir: dir,
        context,
        clipboard,
        paste,
        store: ClipboardAssetStore::new(data_dir),
    }
}

// ---------------------------------------------------------------------
// 1. Payload priority: text always wins.
// ---------------------------------------------------------------------

#[test]
fn text_wins_when_the_clipboard_offers_text_and_an_image() {
    let h = harness(vec![]);
    h.clipboard.push_read(Ok(Some("copied text".into())));
    h.clipboard.push_image_read(Ok(Some(bitmap(4, 4, 0x11))));

    let payload = h
        .clipboard
        .read_payload()
        .expect("read")
        .expect("payload present");
    assert_eq!(payload.kind(), "text", "text must take priority");
    assert_eq!(payload.as_text(), Some("copied text"));

    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        payload,
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let record = h.record(id);
    assert_eq!(record.content, "copied text");
    assert_ne!(record.content_type, ContentType::Image);
    assert!(record.asset_ref.is_none(), "no asset for a textual capture");
    assert!(
        h.assets_on_disk().is_empty(),
        "a textual capture must not create an asset file"
    );
}

#[test]
fn image_is_captured_only_when_no_usable_text_exists() {
    let h = harness(vec![]);
    // Empty text is "no usable text": the image must be considered.
    h.clipboard.push_read(Ok(Some(String::new())));
    h.clipboard.push_image_read(Ok(Some(bitmap(6, 4, 0x22))));

    let payload = h
        .clipboard
        .read_payload()
        .expect("read")
        .expect("payload present");
    assert_eq!(payload.kind(), "image");

    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        payload,
        Some("com.apple.Preview"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };

    let record = h.record(id);
    assert_eq!(record.content_type, ContentType::Image);
    assert!(record.is_renderable_image());
    assert_eq!(record.payload_width, Some(6));
    assert_eq!(record.payload_height, Some(4));
    assert_eq!(record.mime_type.as_deref(), Some("image/png"));
    assert_eq!(record.content, "", "image rows use the empty sentinel");
    assert_eq!(h.assets_on_disk().len(), 1);
}

#[test]
fn unsupported_representation_is_ignored_and_keeps_the_watcher_alive() {
    let h = harness(vec![]);
    // No text, and the image read reports a representation ClipVault
    // cannot normalize (a file list, RTF-only payload, ...).
    h.clipboard.push_read(Ok(None));
    h.clipboard
        .push_image_read(Err(ClipboardBackendError::UnsupportedFormat));

    assert!(
        h.clipboard.read_payload().expect("soft outcome").is_none(),
        "an unsupported format collapses to Ignored"
    );

    let watcher = clipvault_core::CaptureWatcher::new(
        Arc::clone(&h.clipboard) as Arc<dyn ClipboardBackend>,
        std::time::Duration::from_millis(10),
    );
    h.clipboard.push_read(Ok(None));
    h.clipboard
        .push_image_read(Err(ClipboardBackendError::UnsupportedFormat));
    assert_eq!(watcher.tick(&h.context, None), WatchTickOutcome::Ignored);

    // The watcher is still usable: the next real payload is captured.
    h.clipboard.push_read(Ok(Some("still alive".into())));
    assert!(matches!(
        watcher.tick(&h.context, None),
        WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
    ));
    assert_eq!(h.history_count(), 1);
    assert!(h.assets_on_disk().is_empty());
}

#[test]
fn watcher_reports_an_invalid_image_without_dying() {
    let h = harness(vec![]);
    let watcher = clipvault_core::CaptureWatcher::new(
        Arc::clone(&h.clipboard) as Arc<dyn ClipboardBackend>,
        std::time::Duration::from_millis(10),
    );
    h.clipboard.push_read(Ok(None));
    h.clipboard.push_image_read(Err(ClipboardBackendError::from(
        clipvault_core::ImageValidationError::ZeroDimension,
    )));

    match watcher.tick(&h.context, None) {
        WatchTickOutcome::Failed { message } => {
            // Metadata only: dimensions, never pixels.
            assert!(message.contains("invalid"), "got {message}");
        }
        other => panic!("expected Failed, got {other:?}"),
    }

    h.clipboard.push_read(Ok(Some("recovered".into())));
    assert!(matches!(
        watcher.tick(&h.context, None),
        WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
    ));
}

#[test]
fn watcher_captures_an_image_through_the_same_pipeline() {
    let h = harness(vec![]);
    let watcher = clipvault_core::CaptureWatcher::new(
        Arc::clone(&h.clipboard) as Arc<dyn ClipboardBackend>,
        std::time::Duration::from_millis(10),
    );
    h.clipboard.push_read(Ok(None));
    h.clipboard.push_image_read(Ok(Some(bitmap(8, 8, 0x33))));

    match watcher.tick(&h.context, Some("com.apple.Preview")) {
        WatchTickOutcome::Captured(HistoryOutcome::Stored { id }) => {
            let record = h.record(id);
            assert_eq!(record.content_type, ContentType::Image);
            assert_eq!(record.source_app.as_deref(), Some("com.apple.Preview"));
        }
        other => panic!("expected Captured(Stored), got {other:?}"),
    }

    // The same bitmap on the next tick is Unchanged: the watcher does
    // not re-encode a clipboard that has not changed.
    h.clipboard.push_read(Ok(None));
    h.clipboard.push_image_read(Ok(Some(bitmap(8, 8, 0x33))));
    assert_eq!(
        watcher.tick(&h.context, Some("com.apple.Preview")),
        WatchTickOutcome::Unchanged
    );
    assert_eq!(h.history_count(), 1);
    assert_eq!(h.assets_on_disk().len(), 1);
}

// ---------------------------------------------------------------------
// 2. Dedupe and asset reuse.
// ---------------------------------------------------------------------

#[test]
fn identical_image_does_not_duplicate_the_row_or_the_asset() {
    let h = harness(vec![]);
    let first = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(bitmap(10, 10, 0x44)),
        Some("com.apple.Preview"),
    );
    let second = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(bitmap(10, 10, 0x44)),
        Some("com.apple.Preview"),
    );

    let first_id = match first {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    match second {
        HistoryOutcome::Duplicate { id } => assert_eq!(id, first_id),
        other => panic!("expected Duplicate, got {other:?}"),
    }
    assert_eq!(h.history_count(), 1);
    assert_eq!(
        h.assets_on_disk().len(),
        1,
        "the existing asset must be reused"
    );
}

#[test]
fn a_different_image_creates_a_distinct_entry_and_asset() {
    let h = harness(vec![]);
    let first = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(bitmap(4, 4, 0x55)),
        None,
    );
    let second = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(bitmap(4, 4, 0x66)),
        None,
    );
    let (a, b) = match (first, second) {
        (HistoryOutcome::Stored { id: a }, HistoryOutcome::Stored { id: b }) => (a, b),
        other => panic!("expected two Stored outcomes, got {other:?}"),
    };
    assert_ne!(a, b);
    let (ra, rb) = (h.record(a), h.record(b));
    assert_ne!(ra.asset_ref, rb.asset_ref);
    assert_ne!(ra.content_hash, rb.content_hash);
    assert_eq!(h.assets_on_disk().len(), 2);
    // The first entry's metadata is untouched by the second capture.
    assert_eq!(ra.payload_width, Some(4));
    assert!(ra.is_renderable_image());
}

#[test]
fn image_dedupe_hash_is_the_sha256_of_the_normalized_png() {
    let h = harness(vec![]);
    let image = bitmap(5, 5, 0x77);
    let normalized = clipvault_core::normalize_image(&image).expect("normalize");
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(image),
        None,
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let record = h.record(id);
    assert_eq!(record.content_hash, normalized.hash());
    assert_eq!(record.content_size, normalized.byte_len() as i64);
    assert_eq!(
        record.asset_ref.as_deref(),
        Some(normalized.asset_ref().as_str())
    );
    // The persisted reference is relative and lives in the clipboard
    // namespace: never an absolute path.
    let reference = record.asset_ref.expect("reference");
    assert!(reference.starts_with("clipboard/"));
    assert!(!reference.starts_with('/'));
}

// ---------------------------------------------------------------------
// 3. Restart / persistence.
// ---------------------------------------------------------------------

#[test]
fn image_entry_and_asset_survive_a_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    let db_path = dir.path().join("clipvault.db");

    let build = |data_dir: std::path::PathBuf| {
        let clipboard = Arc::new(FakeClipboardBackend::with_image_support());
        let adapters = PlatformAdapters::new(
            clipboard as Arc<dyn ClipboardBackend>,
            Arc::new(FakeHotkeyManager::new()),
            Arc::new(FakeActiveApplication::new()),
            Arc::new(FakePasteController::new()),
            Arc::new(FakeTrayController::new()),
            Arc::new(FakeSettingsNavigator::new()),
            Arc::new(clipvault_core::NoopApplicationMetadataProvider)
                as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
            Capabilities::ALL_AVAILABLE,
            PlatformInfo {
                home_dir: dir.path().to_path_buf(),
                data_dir,
                os_family: OsFamily::Macos,
                display_server: DisplayServer::Unknown,
            },
        );
        AppBootstrap::new()
            .with_clock(Arc::new(FixedClock {
                instant: datetime!(2026-01-02 03:04:05 UTC),
            }))
            .with_platform_adapters(adapters)
            .bootstrap_at(&db_path)
            .expect("bootstrap")
    };

    // First run: capture an image.
    let expected_ref = {
        let context = build(data_dir.clone());
        let outcome = context.history().record_clipboard_payload(
            &context,
            ClipboardPayload::Image(bitmap(12, 6, 0x88)),
            Some("com.apple.Preview"),
        );
        let id = match outcome {
            HistoryOutcome::Stored { id } => id,
            other => panic!("expected Stored, got {other:?}"),
        };
        let mut db = context.database().lock();
        let repo = EntryRepository::new(db.connection_mut());
        repo.find_by_id(id)
            .expect("query")
            .expect("row")
            .asset_ref
            .expect("reference")
    };

    // Second run: a brand-new context over the same database and data
    // directory (migrations re-run and are idempotent).
    let context = build(data_dir.clone());
    let entries = context
        .history()
        .recent_entries(&context, 10)
        .expect("recent");
    assert_eq!(entries.len(), 1);
    let record = &entries[0];
    assert_eq!(record.content_type, ContentType::Image);
    assert!(record.is_renderable_image());
    assert_eq!(record.asset_ref.as_deref(), Some(expected_ref.as_str()));

    // The thumbnail bytes are still readable through the validated
    // bridge, which is what makes the card render after a restart.
    let store = ClipboardAssetStore::new(&data_dir);
    let bytes = store.read_bytes(&expected_ref).expect("asset readable");
    assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
}

// ---------------------------------------------------------------------
// 3b. Restart / load-lifecycle regression coverage.
//
// The previous regression surfaced after the rail started hydrating
// the per-entry `entryOrganization` cache on bootstrap. The image
// card stopped rendering its thumbnail because:
//
//   - the row was persisted correctly (covered by `image_entry_and_asset_survive_a_restart`);
//   - `recent` and `entries_filtered` returned the row;
//   - the asset bytes were still readable on disk;
//   - the bridge returned valid bytes through the validated command;
//   - but the card surface rendered the fallback while the bridge
//     round-trip was in flight, so a stale response could clobber the
//     fresh load.
//
// The tests below walk every layer the regression diagnostic asked us
// to verify and pin the contracts the HistoryCard now depends on:
// `recent_entries` and `recent_entries_with_filter` both return the
// image row with the metadata the frontend needs to drive the
// thumbnail surface; the Tauri wire serialisation preserves every
// payload field; and the backend command returns the exact PNG bytes
// the asset store wrote. These guarantees are independent of the
// hydration flow, so a regression in the hydration listener cannot
// silently break the image surface.
// ---------------------------------------------------------------------

/// Build a fresh `AppContext` over an existing database and data
/// directory. Mirrors the pattern `image_entry_and_asset_survive_a_restart`
/// uses but exposes the helpers we need for the layered assertions.
fn reopen_context(
    db_path: &std::path::Path,
    data_dir: &std::path::Path,
    when: time::OffsetDateTime,
) -> AppContext {
    let clipboard = Arc::new(FakeClipboardBackend::with_image_support());
    let adapters = PlatformAdapters::new(
        clipboard as Arc<dyn ClipboardBackend>,
        Arc::new(FakeHotkeyManager::new()),
        Arc::new(FakeActiveApplication::new()),
        Arc::new(FakePasteController::new()),
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
        Capabilities::ALL_AVAILABLE,
        PlatformInfo {
            home_dir: data_dir.parent().unwrap_or(data_dir).to_path_buf(),
            data_dir: data_dir.to_path_buf(),
            os_family: OsFamily::Macos,
            display_server: DisplayServer::Unknown,
        },
    );
    AppBootstrap::new()
        .with_clock(Arc::new(FixedClock { instant: when }))
        .with_platform_adapters(adapters)
        .bootstrap_at(db_path)
        .expect("reopen")
}

#[test]
fn recent_entries_returns_image_rows_with_complete_payload_metadata_after_restart() {
    // The "Historial" rail query is the unfiltered branch. It must
    // return the image row with every payload field intact so the
    // card can drive `hasRenderableImage` and the loading
    // placeholder. The diagnostic the regression required:
    //   1. the row survives the close/reopen cycle;
    //   2. `content_type` stays `Image`;
    //   3. `asset_ref` is preserved verbatim;
    //   4. `mime_type`, `payload_width`, `payload_height` survive.
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    let db_path = dir.path().join("clipvault.db");
    let when = datetime!(2026-01-02 03:04:05 UTC);

    // Capture an image during the first session.
    let context = reopen_context(&db_path, &data_dir, when);
    let outcome = context.history().record_clipboard_payload(
        &context,
        ClipboardPayload::Image(bitmap(64, 32, 0x42)),
        Some("com.apple.Preview"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    drop(context);

    // Reopen the same database and data directory.
    let reopened = reopen_context(&db_path, &data_dir, when);
    let records = reopened
        .history()
        .recent_entries(&reopened, 50)
        .expect("recent_entries");
    assert_eq!(records.len(), 1);
    let record = &records[0];

    // (1) the row survives.
    assert_eq!(record.id, id);
    // (2) content_type stays image.
    assert_eq!(record.content_type, ContentType::Image);
    // (3) asset_ref is preserved verbatim — never rewritten, never
    // prefixed with an absolute path.
    let expected_ref = record.asset_ref.clone().expect("asset_ref");
    assert!(expected_ref.starts_with("clipboard/"));
    assert!(!expected_ref.starts_with('/'));
    assert!(record.is_renderable_image(), "row must be renderable");
    // (4) MIME type and dimensions survive verbatim.
    assert_eq!(
        record.mime_type.as_deref(),
        Some(clipvault_db::IMAGE_MIME_PNG)
    );
    assert_eq!(record.payload_width, Some(64));
    assert_eq!(record.payload_height, Some(32));
}

#[test]
fn recent_entries_with_filter_returns_image_rows_in_history_after_restart() {
    // The filtered-rail branch (`selectedCollectionId === null` still
    // goes through this path because the legacy contract routes the
    // empty filter through `recent_entries_with_filter(context,
    // None, &[], limit)` to keep the image-aware query shape). The
    // query must return the image row with full metadata after a
    // restart.
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    let db_path = dir.path().join("clipvault.db");
    let when = datetime!(2026-01-02 03:04:05 UTC);

    let context = reopen_context(&db_path, &data_dir, when);
    let outcome = context.history().record_clipboard_payload(
        &context,
        ClipboardPayload::Image(bitmap(24, 24, 0x55)),
        Some("com.apple.Preview"),
    );
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));
    drop(context);

    let reopened = reopen_context(&db_path, &data_dir, when);
    let records = reopened
        .history()
        .recent_entries_with_filter(&reopened, None, &[], &SourceAppFilter::default(), 50)
        .expect("recent_entries_with_filter");
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.content_type, ContentType::Image);
    assert!(record.is_renderable_image());
    assert_eq!(
        record.mime_type.as_deref(),
        Some(clipvault_db::IMAGE_MIME_PNG)
    );
    assert_eq!(record.payload_width, Some(24));
    assert_eq!(record.payload_height, Some(24));
    assert!(record.asset_ref.is_some());
}

#[test]
fn image_row_round_trips_through_row_to_record_with_complete_metadata() {
    // Pin the `row_to_record` contract for an image row: every payload
    // column must survive the materialisation. A future migration
    // that adds a column without updating `row_to_record` would
    // surface here because the assertion reads the value the
    // repository returned, not the value the inserter wrote.
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    let db_path = dir.path().join("clipvault.db");
    let when = datetime!(2026-01-02 03:04:05 UTC);

    let context = reopen_context(&db_path, &data_dir, when);
    let outcome = context.history().record_clipboard_payload(
        &context,
        ClipboardPayload::Image(bitmap(16, 16, 0x77)),
        Some("com.apple.Preview"),
    );
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));
    drop(context);

    let reopened = reopen_context(&db_path, &data_dir, when);
    let mut db = reopened.database().lock();
    let repo = EntryRepository::new(db.connection_mut());
    let records = repo.recent(10).expect("recent");
    assert_eq!(records.len(), 1);
    let record = &records[0];

    // Every payload column is preserved.
    assert_eq!(record.content, clipvault_db::IMAGE_CONTENT_SENTINEL);
    assert_eq!(record.content_type, ContentType::Image);
    let reference = record.asset_ref.as_deref().expect("asset_ref");
    assert!(reference.starts_with("clipboard/"));
    assert!(reference.ends_with(".png"));
    assert_eq!(
        record.mime_type.as_deref(),
        Some(clipvault_db::IMAGE_MIME_PNG)
    );
    assert_eq!(record.payload_width, Some(16));
    assert_eq!(record.payload_height, Some(16));

    // The Tauri wire serialisation must carry the same fields so the
    // frontend can rebuild the same predicate. The frontend never
    // inspects `content` for an image row, but the column is part of
    // the wire contract and must keep its `NOT NULL` sentinel.
    let serialised = serde_json::to_string(record).expect("serialise");
    assert!(
        serialised.contains("\"asset_ref\":\"clipboard/"),
        "got {serialised}"
    );
    assert!(
        serialised.contains("\"mime_type\":\"image/png\""),
        "got {serialised}"
    );
    assert!(
        serialised.contains("\"payload_width\":16"),
        "got {serialised}"
    );
    assert!(
        serialised.contains("\"payload_height\":16"),
        "got {serialised}"
    );
    assert!(
        serialised.contains("\"content_type\":\"image\""),
        "got {serialised}"
    );
    assert!(
        !serialised.contains("/Users/"),
        "absolute path leaked: {serialised}"
    );
}

#[test]
fn clipboard_asset_command_returns_persisted_bytes_after_restart() {
    // The exact integration the HistoryCard runs in production: the
    // image row is fetched through the rail query, the resolver calls
    // `clipvault_clipboard_asset` with the relative reference, and
    // the command returns the PNG bytes the asset store wrote. The
    // test exercises the same `ClipboardAssetStore::read_bytes`
    // entry point the Tauri command wraps (`read_clipboard_asset`),
    // so the byte-for-byte contract the frontend depends on is pinned
    // here without standing up a Tauri runtime.
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    let db_path = dir.path().join("clipvault.db");
    let when = datetime!(2026-01-02 03:04:05 UTC);

    let context = reopen_context(&db_path, &data_dir, when);
    let outcome = context.history().record_clipboard_payload(
        &context,
        ClipboardPayload::Image(bitmap(8, 8, 0xAA)),
        Some("com.apple.Preview"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    drop(context);

    let reopened = reopen_context(&db_path, &data_dir, when);
    let records = reopened
        .history()
        .recent_entries(&reopened, 10)
        .expect("recent");
    let record = records.iter().find(|r| r.id == id).expect("row");
    let asset_ref = record.asset_ref.as_deref().expect("asset_ref");

    let store = ClipboardAssetStore::new(&data_dir);
    let bytes = store
        .read_bytes(asset_ref)
        .expect("asset bytes returned after restart");
    assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
    assert!(!bytes.is_empty());
}

#[test]
fn clipboard_asset_command_returns_invalid_asset_ref_after_restart() {
    // Symmetric to the success path: a corrupted or missing file must
    // surface a typed error reason. The frontend uses the kind to
    // drive the `"error"` state and the accessible fallback; the
    // `ClipboardAssetStore` exposes the same `kind_str()` the Tauri
    // command maps into the `CommandError::message`.
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    let db_path = dir.path().join("clipvault.db");
    let when = datetime!(2026-01-02 03:04:05 UTC);

    let context = reopen_context(&db_path, &data_dir, when);
    drop(context);

    let store = ClipboardAssetStore::new(&data_dir);
    let error = store
        .read_bytes("clipboard/ghost.png")
        .expect_err("missing asset must reject");
    assert_eq!(error.kind_str(), "not_found");
}

// ---------------------------------------------------------------------
// 4. Safe asset reads.
// ---------------------------------------------------------------------

#[test]
fn asset_bridge_rejects_hostile_references_without_returning_bytes() {
    let h = harness(vec![]);
    h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(bitmap(4, 4, 0x99)),
        None,
    );

    let cases: [(&str, &str); 7] = [
        ("", "empty"),
        ("/etc/passwd", "absolute"),
        ("clipboard/../../etc/passwd", "traversal"),
        ("ignored-apps/com.apple.textedit.png", "out_of_scope"),
        ("application-icons/com.apple.Terminal.png", "out_of_scope"),
        ("clipboard/nested/x.png", "out_of_scope"),
        ("clipboard/ghost.png", "not_found"),
    ];
    for (reference, expected_kind) in cases {
        let error = h
            .store
            .read_bytes(reference)
            .expect_err("hostile reference must be rejected");
        assert_eq!(
            error.kind_str(),
            expected_kind,
            "reference {reference} produced {error:?}"
        );
    }
}

#[test]
fn asset_bridge_rejects_a_corrupt_asset_and_leaves_history_usable() {
    let h = harness(vec![]);
    // A textual entry that must keep working no matter what happens to
    // the image asset.
    h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Text("text still fine".into()),
        None,
    );
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(bitmap(4, 4, 0xAA)),
        None,
    );
    let image_id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let reference = h.record(image_id).asset_ref.expect("reference");

    // Corrupt the asset on disk behind ClipVault's back.
    let path = h.store.root().join(
        reference
            .strip_prefix("clipboard/")
            .expect("namespace prefix"),
    );
    std::fs::write(&path, b"not a png anymore").expect("corrupt");

    assert_eq!(
        h.store.read_bytes(&reference).expect_err("must reject"),
        AssetError::NotPng
    );

    // The row is still there (the UI will render its fallback) and the
    // textual history is unaffected.
    assert_eq!(h.history_count(), 2);
    let entries = h
        .context
        .history()
        .recent_entries(&h.context, 10)
        .expect("recent");
    assert!(entries
        .iter()
        .any(|entry| entry.content == "text still fine"));
    let search = h
        .context
        .search()
        .search(
            &h.context,
            &clipvault_core::SearchQuery {
                text: "still fine".into(),
                limit: 10,
            },
        )
        .expect("search");
    assert_eq!(search.hits.len(), 1, "textual search keeps working");
}

#[test]
fn textual_search_never_returns_image_entries() {
    let h = harness(vec![]);
    h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Text("searchable text".into()),
        None,
    );
    h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(bitmap(4, 4, 0xBB)),
        None,
    );

    // An empty-ish query that would match the image row's empty
    // sentinel if images participated in the index.
    for query in ["searchable", "png", "image", "clipboard"] {
        let outcome = h
            .context
            .search()
            .search(
                &h.context,
                &clipvault_core::SearchQuery {
                    text: query.into(),
                    limit: 10,
                },
            )
            .expect("search");
        for hit in outcome.hits {
            assert_ne!(
                hit.record.content_type,
                ContentType::Image,
                "query {query} surfaced an image entry"
            );
        }
    }
}

// ---------------------------------------------------------------------
// 5. Privacy: the blacklist must produce no artefact at all.
// ---------------------------------------------------------------------

#[test]
fn blacklisted_application_creates_no_row_and_no_asset() {
    let h = harness(vec!["com.1password.1password".to_string()]);
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(bitmap(16, 16, 0xCC)),
        Some("com.1password.1password"),
    );

    assert_eq!(
        outcome,
        HistoryOutcome::Ignored,
        "a blacklisted capture must be discarded"
    );
    assert_eq!(h.history_count(), 0, "no row may be created");
    assert!(
        h.assets_on_disk().is_empty(),
        "PrivacyGate runs before the asset is written"
    );
    assert!(h.referenced_asset_refs().is_empty());
    // The namespace directory itself must not even be created for a
    // discarded capture.
    assert!(
        !h.store.root().exists(),
        "no asset directory for a blacklisted capture"
    );
}

#[test]
fn allowed_application_after_a_blacklisted_one_still_persists() {
    // Proves the gate discards rather than disables: the pipeline keeps
    // working for a permitted source right after a rejection.
    let h = harness(vec!["com.1password.1password".to_string()]);
    h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(bitmap(8, 8, 0xDD)),
        Some("com.1password.1password"),
    );
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(bitmap(8, 8, 0xDD)),
        Some("com.apple.Preview"),
    );
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));
    assert_eq!(h.history_count(), 1);
    assert_eq!(h.assets_on_disk().len(), 1);
}

// ---------------------------------------------------------------------
// 6. Lifecycle: delete, clear, retention, favorites.
// ---------------------------------------------------------------------

fn store_image(h: &Harness, fill: u8) -> (i64, String) {
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(bitmap(6, 6, fill)),
        Some("com.apple.Preview"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let reference = h.record(id).asset_ref.expect("reference");
    (id, reference)
}

#[test]
fn deleting_an_image_entry_collects_its_asset() {
    let h = harness(vec![]);
    let (id, reference) = store_image(&h, 0x01);
    assert!(h.store.read_bytes(&reference).is_ok());

    let outcome = h
        .context
        .management()
        .delete_entry(&h.context, id, true)
        .expect("delete");
    assert_eq!(outcome.removed(), Some(1));
    assert_eq!(h.history_count(), 0);
    assert!(
        h.assets_on_disk().is_empty(),
        "the unreferenced asset must be collected"
    );
    assert_eq!(
        h.store.read_bytes(&reference).unwrap_err(),
        AssetError::NotFound
    );
}

#[test]
fn delete_without_confirmation_touches_nothing() {
    let h = harness(vec![]);
    let (id, reference) = store_image(&h, 0x02);
    let outcome = h
        .context
        .management()
        .delete_entry(&h.context, id, false)
        .expect("delete");
    assert_eq!(outcome.removed(), None);
    assert_eq!(h.history_count(), 1);
    assert!(h.store.read_bytes(&reference).is_ok());
}

#[test]
fn clear_history_keeps_the_favorite_image_and_its_asset() {
    let h = harness(vec![]);
    let (keep_id, keep_ref) = store_image(&h, 0x03);
    let (_drop_id, drop_ref) = store_image(&h, 0x04);
    h.context
        .management()
        .set_favorite(&h.context, keep_id, true)
        .expect("favorite");

    let outcome = h
        .context
        .management()
        .clear_non_favorites(&h.context, true)
        .expect("clear");
    assert_eq!(outcome.removed(), Some(1));
    assert_eq!(h.history_count(), 1);
    assert!(
        h.store.read_bytes(&keep_ref).is_ok(),
        "a favorite image keeps its asset"
    );
    assert_eq!(
        h.store.read_bytes(&drop_ref).unwrap_err(),
        AssetError::NotFound
    );
}

#[test]
fn retention_collects_a_stale_image_and_spares_a_favorite() {
    let h = harness(vec![]);
    let (favorite_id, favorite_ref) = store_image(&h, 0x05);
    let (_stale_id, stale_ref) = store_image(&h, 0x06);
    h.context
        .management()
        .set_favorite(&h.context, favorite_id, true)
        .expect("favorite");

    // Every row was created at the fixed clock instant, so a 7-day
    // policy evaluated "now" leaves them all in range; forcing the
    // cutoff far in the future is what the fixed reader models.
    let outcome = h
        .context
        .management()
        .apply_retention(&h.context, &FixedRetention(RetentionPolicy::Forever))
        .expect("retention");
    assert_eq!(outcome.removed, 0);
    assert!(h.store.read_bytes(&favorite_ref).is_ok());
    assert!(h.store.read_bytes(&stale_ref).is_ok());

    // Now delete the non-favorite directly and prove retention's
    // collector reclaims exactly one asset while sparing the favorite.
    {
        let mut db = h.context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        repo.delete_non_favorites_older_than(datetime!(2030-01-01 00:00:00 UTC))
            .expect("purge");
    }
    h.context
        .management()
        .apply_retention(&h.context, &FixedRetention(RetentionPolicy::Forever))
        .expect("retention");
    assert!(
        h.store.read_bytes(&favorite_ref).is_ok(),
        "the favorite image and its asset remain available"
    );
    assert_eq!(
        h.store.read_bytes(&stale_ref).unwrap_err(),
        AssetError::NotFound
    );
}

#[test]
fn collector_keeps_an_asset_still_referenced_by_another_row() {
    let h = harness(vec![]);
    let (id, reference) = store_image(&h, 0x07);

    // A second row that deliberately shares the same asset with a
    // different dedupe hash — the shape a re-import or a rollback can
    // produce.
    {
        let mut db = h.context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        let mut shared = clipvault_db::NewEntry {
            content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
            content_type: ContentType::Image,
            content_size: 128,
            content_hash: format!("{reference}-variant"),
            source_app: None,
            created_at: datetime!(2026-01-02 03:04:05 UTC),
            last_seen_at: datetime!(2026-01-02 03:04:05 UTC),
            asset_ref: Some(reference.clone()),
            mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
            payload_width: Some(6),
            payload_height: Some(6),
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
        };
        shared.content_hash = format!("{reference}-variant");
        repo.insert_or_touch(shared).expect("shared row");
    }

    h.context
        .management()
        .delete_entry(&h.context, id, true)
        .expect("delete");

    assert!(
        h.store.read_bytes(&reference).is_ok(),
        "an asset referenced by another row must survive"
    );
    assert_eq!(h.assets_on_disk().len(), 1);
}

#[test]
fn collector_reclaims_an_asset_orphaned_by_an_interrupted_capture() {
    // Simulates the documented "SQLite failed after the asset was
    // written" path: the file is a safe orphan the next collector pass
    // reclaims, and it never removes a referenced asset alongside it.
    let h = harness(vec![]);
    let (_id, live_ref) = store_image(&h, 0x08);

    let orphan = clipvault_core::normalize_image(&bitmap(3, 3, 0x09)).expect("normalize");
    let orphan_ref = h
        .store
        .store_image(&orphan)
        .expect("write orphan")
        .asset_ref()
        .to_string();
    assert_eq!(h.assets_on_disk().len(), 2);

    let removed = h
        .context
        .management()
        .collect_unreferenced_assets(&h.context);
    assert_eq!(removed.assets_removed_count, 1);
    assert!(removed.image_reference_query_succeeded);
    assert!(!removed.image_collection_skipped);
    assert!(h.store.read_bytes(&live_ref).is_ok());
    assert_eq!(
        h.store.read_bytes(&orphan_ref).unwrap_err(),
        AssetError::NotFound
    );
    // Idempotent: a second pass removes nothing.
    let again = h
        .context
        .management()
        .collect_unreferenced_assets(&h.context);
    assert_eq!(again.assets_removed_count, 0);
    assert!(again.image_reference_query_succeeded);
}

// ---------------------------------------------------------------------
// 6a. Collector safety: a failing reference query MUST NOT delete any
// asset, even though the row in `clipboard_entries` still references it.
// These tests pin the regression reported on
// `clipboard-legacy-image-assets`: an `Err` from
// `referenced_asset_refs` was previously collapsed to an empty
// `BTreeSet`, causing the collector to wipe every PNG under
// `<data_dir>/assets/clipboard/` on startup. The fix splits the
// `Ok(empty_set)` and `Err(...)` branches so a failed query leaves the
// assets untouched.
// ---------------------------------------------------------------------

/// Rename the `clipboard_entries` table so the next
/// `referenced_asset_refs` / `referenced_rich_asset_refs` query
/// fails (the SQL targets `clipboard_entries`, which no longer
/// exists). The rename is reversible so tests that need a working
/// table afterwards can call [`repair_clipboard_table`] to put it
/// back.
fn damage_clipboard_table(h: &Harness) {
    h.context
        .database()
        .lock()
        .connection_mut()
        .execute_batch("ALTER TABLE clipboard_entries RENAME TO clipboard_entries_damaged;")
        .expect("rename table");
}

/// Restore the `clipboard_entries` name after [`damage_clipboard_table`].
/// Only call this once per harness — a second call would try to rename
/// a table that no longer exists.
fn repair_clipboard_table(h: &Harness) {
    h.context
        .database()
        .lock()
        .connection_mut()
        .execute_batch("ALTER TABLE clipboard_entries_damaged RENAME TO clipboard_entries;")
        .expect("restore table");
}

#[test]
fn failing_image_reference_query_keeps_every_image_asset_on_disk() {
    // The asset directory contains two files: one is referenced by a
    // row in `clipboard_entries`, the other is an orphan. The
    // collector runs after the table is dropped, so the live reference
    // query fails. The contract: NOT A SINGLE FILE is removed.
    let h = harness(vec![]);
    let (_id, live_ref) = store_image(&h, 0x70);

    // Inject an extra orphan PNG the harness did not register.
    let orphan = clipvault_core::normalize_image(&bitmap(4, 4, 0x71)).expect("normalize");
    let orphan_ref = h
        .store
        .store_image(&orphan)
        .expect("write orphan")
        .asset_ref()
        .to_string();

    damage_clipboard_table(&h);

    let outcome = h
        .context
        .management()
        .collect_unreferenced_assets(&h.context);
    assert!(
        outcome.image_reference_query_failed,
        "the image reference query must report failure"
    );
    assert!(
        outcome.image_collection_skipped,
        "the image collector must skip deletion when the query fails"
    );
    assert!(
        !outcome.image_reference_query_succeeded,
        "the image reference query must NOT be marked as successful"
    );
    assert_eq!(
        outcome.assets_removed_count, 0,
        "no file may be reclaimed while the live set is unknown"
    );

    assert!(
        h.store.read_bytes(&live_ref).is_ok(),
        "a referenced image must survive a failing query"
    );
    assert!(
        h.store.read_bytes(&orphan_ref).is_ok(),
        "an orphan image must also survive a failing query — the collector cannot prove it is unreferenced"
    );
}

#[test]
fn apply_retention_with_a_failing_reference_query_keeps_every_asset() {
    // The startup / shutdown retention pass must call
    // `collect_unreferenced_assets` and survive a SQLite failure
    // without losing any PNG. The harness models the startup path:
    // rows are persisted, retention runs with `Forever` so the DELETE
    // never fires, the table is renamed to break the live-set query,
    // and the assets must remain untouched.
    let h = harness(vec![]);
    let (_live_id, live_ref) = store_image(&h, 0x72);
    let orphan = clipvault_core::normalize_image(&bitmap(4, 4, 0x73)).expect("normalize");
    let orphan_ref = h
        .store
        .store_image(&orphan)
        .expect("write orphan")
        .asset_ref()
        .to_string();

    damage_clipboard_table(&h);

    // `Forever` short-circuits the DELETE so the test only exercises
    // the `collect_unreferenced_assets` step; that step must observe
    // the missing table, skip deletion and preserve every file on
    // disk.
    let outcome = h
        .context
        .management()
        .apply_retention(&h.context, &FixedRetention(RetentionPolicy::Forever))
        .expect("retention must not error when the query fails");
    assert_eq!(outcome.policy, RetentionPolicy::Forever);
    assert_eq!(
        outcome.removed, 0,
        "Forever never purges; the only pass is the asset collector"
    );

    assert!(
        h.store.read_bytes(&live_ref).is_ok(),
        "a referenced image must survive a failing retention pass"
    );
    assert!(
        h.store.read_bytes(&orphan_ref).is_ok(),
        "an orphan image must also survive a failing retention pass"
    );
}

#[test]
fn failing_reference_query_reports_metadata_only_diagnostics() {
    // The diagnostic surface must surface `reference_query_failed`,
    // `image_collection_skipped` and a zero `assets_removed_count`
    // without leaking paths, hashes or row identifiers.
    let h = harness(vec![]);
    let (_id, live_ref) = store_image(&h, 0x74);

    damage_clipboard_table(&h);

    let outcome = h
        .context
        .management()
        .collect_unreferenced_assets(&h.context);

    assert!(outcome.image_reference_query_failed);
    assert!(outcome.image_collection_skipped);
    assert!(!outcome.image_reference_query_succeeded);
    assert_eq!(outcome.assets_removed_count, 0);

    // The serialised form must stay metadata-only: no asset_ref, no
    // absolute path, no hash. Snake_case keys are the documented
    // public contract.
    let json = serde_json::to_string(&outcome).expect("serialise");
    assert!(json.contains("\"image_reference_query_failed\":true"));
    assert!(json.contains("\"image_collection_skipped\":true"));
    assert!(json.contains("\"assets_removed_count\":0"));
    assert!(
        !json.contains(&live_ref),
        "diagnostics must never carry the live asset reference"
    );
    assert!(
        !json.contains("png"),
        "diagnostics must never carry the asset extension"
    );
}

#[test]
fn successful_query_with_referenced_assets_keeps_every_file() {
    // The happy path stays intact: a query that returns the live set
    // must NOT touch a referenced file even when the namespace also
    // contains an orphan.
    let h = harness(vec![]);
    let (_live_id, live_ref) = store_image(&h, 0x75);
    let orphan = clipvault_core::normalize_image(&bitmap(4, 4, 0x76)).expect("normalize");
    let orphan_ref = h
        .store
        .store_image(&orphan)
        .expect("write orphan")
        .asset_ref()
        .to_string();

    let outcome = h
        .context
        .management()
        .collect_unreferenced_assets(&h.context);

    assert!(outcome.image_reference_query_succeeded);
    assert!(!outcome.image_reference_query_failed);
    assert!(!outcome.image_collection_skipped);
    assert_eq!(outcome.assets_removed_count, 1);
    assert!(h.store.read_bytes(&live_ref).is_ok());
    assert_eq!(
        h.store.read_bytes(&orphan_ref).unwrap_err(),
        AssetError::NotFound
    );
}

#[test]
fn successful_query_with_no_references_removes_only_orphans() {
    // The empty-set case is the legitimate reclaim path: a successful
    // query that returns no rows means the database confirms nothing
    // is referenced, so the collector can safely remove the orphans
    // left over from an interrupted capture.
    let h = harness(vec![]);
    let orphan = clipvault_core::normalize_image(&bitmap(4, 4, 0x77)).expect("normalize");
    let orphan_ref = h
        .store
        .store_image(&orphan)
        .expect("write orphan")
        .asset_ref()
        .to_string();
    assert_eq!(h.assets_on_disk().len(), 1);

    // Drop the row that would have referenced the asset so the live
    // set is genuinely empty.
    h.context
        .database()
        .lock()
        .connection_mut()
        .execute_batch("DELETE FROM clipboard_entries;")
        .expect("delete");

    let outcome = h
        .context
        .management()
        .collect_unreferenced_assets(&h.context);
    assert!(outcome.image_reference_query_succeeded);
    assert!(!outcome.image_reference_query_failed);
    assert!(!outcome.image_collection_skipped);
    assert_eq!(outcome.assets_removed_count, 1);
    assert_eq!(
        h.store.read_bytes(&orphan_ref).unwrap_err(),
        AssetError::NotFound
    );
}

#[test]
fn shared_asset_survives_every_pass_including_failing_query() {
    // Two rows share the same asset. A failing reference query must
    // not delete the file because the collector cannot prove the
    // other row does not need it.
    let h = harness(vec![]);
    let (first_id, shared_ref) = store_image(&h, 0x78);
    let shared_hash = h.record(first_id).content_hash.clone();

    {
        let mut db = h.context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        let shared = clipvault_db::NewEntry {
            content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
            content_type: ContentType::Image,
            content_size: 128,
            content_hash: format!("{shared_hash}-variant"),
            source_app: None,
            created_at: datetime!(2026-01-02 03:04:05 UTC),
            last_seen_at: datetime!(2026-01-02 03:04:05 UTC),
            asset_ref: Some(shared_ref.clone()),
            mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
            payload_width: Some(6),
            payload_height: Some(6),
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
        };
        repo.insert_or_touch(shared).expect("shared row");
    }

    damage_clipboard_table(&h);

    let outcome = h
        .context
        .management()
        .collect_unreferenced_assets(&h.context);
    assert!(outcome.image_reference_query_failed);
    assert!(outcome.image_collection_skipped);
    assert_eq!(outcome.assets_removed_count, 0);
    assert!(
        h.store.read_bytes(&shared_ref).is_ok(),
        "a shared asset must survive a failing query"
    );
}

#[test]
fn fresh_capture_survives_a_previous_failed_collection_pass() {
    // After a failed pass leaves files on disk, a brand-new capture
    // must continue to be persisted normally and remain visible.
    // The rename trick simulates the transient SQLite failure without
    // removing the schema: after the failed pass we restore the name
    // and confirm the next capture goes through the regular pipeline.
    let h = harness(vec![]);
    let (_id, _ref) = store_image(&h, 0x79);

    damage_clipboard_table(&h);
    let failed = h
        .context
        .management()
        .collect_unreferenced_assets(&h.context);
    assert!(failed.image_reference_query_failed);
    assert_eq!(failed.assets_removed_count, 0);
    repair_clipboard_table(&h);

    // The next capture goes through the regular history pipeline.
    let (fresh_id, fresh_ref) = store_image(&h, 0x7A);
    let fresh_record = h.record(fresh_id);
    assert_eq!(fresh_record.asset_ref.as_deref(), Some(fresh_ref.as_str()));
    assert!(h.store.read_bytes(&fresh_ref).is_ok());
}

#[test]
fn startup_retention_pass_with_a_failing_query_keeps_every_asset() {
    // The Tauri `setup` callback runs `run_retention` on startup. The
    // contract: a SQLite failure in that pass must NOT cause the
    // startup path to delete any asset. We model the startup pass
    // through `apply_retention` with `Forever` (the DELETE short-
    // circuits) so the only work performed is the asset collector.
    let h = harness(vec![]);
    let (_live_id, live_ref) = store_image(&h, 0x7B);
    let orphan = clipvault_core::normalize_image(&bitmap(4, 4, 0x7C)).expect("normalize");
    let orphan_ref = h
        .store
        .store_image(&orphan)
        .expect("write orphan")
        .asset_ref()
        .to_string();

    damage_clipboard_table(&h);
    let outcome = h
        .context
        .management()
        .apply_retention(&h.context, &FixedRetention(RetentionPolicy::Forever))
        .expect("startup pass must not error when the query fails");
    assert_eq!(outcome.policy, RetentionPolicy::Forever);
    repair_clipboard_table(&h);

    assert!(
        h.store.read_bytes(&live_ref).is_ok(),
        "a referenced image must survive a failing startup retention pass"
    );
    assert!(
        h.store.read_bytes(&orphan_ref).is_ok(),
        "an orphan image must also survive a failing startup retention pass"
    );
}

#[test]
fn shutdown_retention_pass_with_a_failing_query_keeps_every_asset() {
    // The Tauri `RunEvent::ExitRequested` handler runs the same
    // `run_retention` helper on shutdown. A SQLite failure must NOT
    // cause the shutdown path to delete any asset either.
    let h = harness(vec![]);
    let (_live_id, live_ref) = store_image(&h, 0x7D);
    let orphan = clipvault_core::normalize_image(&bitmap(4, 4, 0x7E)).expect("normalize");
    let orphan_ref = h
        .store
        .store_image(&orphan)
        .expect("write orphan")
        .asset_ref()
        .to_string();

    damage_clipboard_table(&h);
    let outcome = h
        .context
        .management()
        .apply_retention(&h.context, &FixedRetention(RetentionPolicy::Forever))
        .expect("shutdown pass must not error when the query fails");
    assert_eq!(outcome.policy, RetentionPolicy::Forever);
    repair_clipboard_table(&h);

    assert!(
        h.store.read_bytes(&live_ref).is_ok(),
        "a referenced image must survive a failing shutdown retention pass"
    );
    assert!(
        h.store.read_bytes(&orphan_ref).is_ok(),
        "an orphan image must also survive a failing shutdown retention pass"
    );
}

#[test]
fn failing_rich_text_reference_query_keeps_every_rich_asset_on_disk() {
    // The rich-text collector uses a different query and the same
    // skip-on-failure contract. The harness ships with a rich-text
    // asset store wired by `AppBootstrap`. We write a rich payload
    // directly through the rich store, register a row that references
    // it, then rename the table and confirm the collector preserves
    // every file.
    use clipvault_core::{canonical_rich_text_hash, RichTextAssetStore, RichTextPayload};
    let h = harness(vec![]);

    // The image store root is `<data_dir>/assets/clipboard`; the rich
    // store root is `<data_dir>/assets/rich-text`. Walk back two
    // parents to recover the harness data directory.
    let data_dir = h
        .store
        .root()
        .parent()
        .and_then(|p| p.parent())
        .expect("data dir parent")
        .to_path_buf();
    let rich_store = RichTextAssetStore::new(data_dir);

    let plain = "hello rich text";
    let html = "<p>hello <b>rich</b></p>";
    let payload =
        RichTextPayload::new(plain.to_string(), Some(html.to_string()), None).expect("payload");
    let rich_hash = canonical_rich_text_hash(&payload);
    let outcome = rich_store.store(&rich_hash, &payload).expect("store rich");

    // Register a row that points at the rich-text HTML reference.
    let rich_html_ref = outcome.html_ref().expect("html ref").to_string();
    {
        let mut db = h.context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        let rich_row = clipvault_db::NewEntry {
            content: plain.to_string(),
            content_type: ContentType::Html,
            content_size: plain.len() as i64,
            content_hash: format!("rich::{rich_hash}"),
            source_app: None,
            created_at: datetime!(2026-01-02 03:04:05 UTC),
            last_seen_at: datetime!(2026-01-02 03:04:05 UTC),
            asset_ref: None,
            mime_type: None,
            payload_width: None,
            payload_height: None,
            rich_text_hash: Some(rich_hash.clone()),
            rich_html_ref: Some(rich_html_ref.clone()),
            rich_rtf_ref: outcome.rtf_ref().map(str::to_owned),
            rich_preview_ref: outcome.preview_ref().map(str::to_owned),
            rich_html_size: Some(html.len() as i64),
            rich_rtf_size: None,
        };
        repo.insert_or_touch(rich_row).expect("insert rich row");
    }

    // Now break the table and verify the rich-text collector skips.
    damage_clipboard_table(&h);
    let collected = h
        .context
        .management()
        .collect_unreferenced_assets(&h.context);
    assert!(
        collected.rich_reference_query_failed,
        "the rich-text reference query must report failure"
    );
    assert!(
        collected.rich_collection_skipped,
        "the rich-text collector must skip deletion when the query fails"
    );
    assert!(!collected.rich_reference_query_succeeded);
    assert_eq!(
        collected.assets_removed_count, 0,
        "no rich-text asset may be reclaimed while the live set is unknown"
    );

    assert!(
        rich_store.read_bytes(&rich_html_ref).is_ok(),
        "the rich-text HTML asset must survive a failing query"
    );
}

#[test]
fn asset_collection_outcome_serialises_with_snake_case_fields() {
    // The frontend / log shippers rely on the snake_case contract.
    let outcome = clipvault_core::AssetCollectionOutcome {
        image_reference_query_succeeded: true,
        image_reference_query_failed: false,
        image_collection_skipped: false,
        rich_reference_query_succeeded: false,
        rich_reference_query_failed: true,
        rich_collection_skipped: true,
        assets_removed_count: 4,
    };
    let json = serde_json::to_string(&outcome).expect("serialise");
    assert!(
        json.contains("\"image_reference_query_succeeded\":true"),
        "got {json}"
    );
    assert!(
        json.contains("\"image_reference_query_failed\":false"),
        "got {json}"
    );
    assert!(
        json.contains("\"image_collection_skipped\":false"),
        "got {json}"
    );
    assert!(
        json.contains("\"rich_reference_query_failed\":true"),
        "got {json}"
    );
    assert!(
        json.contains("\"rich_collection_skipped\":true"),
        "got {json}"
    );
    assert!(json.contains("\"assets_removed_count\":4"), "got {json}");
}

#[test]
fn asset_collection_outcome_default_reports_no_diagnostics() {
    // The default value models the row-only test harness: no asset
    // stores, no queries, no deletions.
    let outcome = clipvault_core::AssetCollectionOutcome::default();
    assert!(!outcome.image_reference_query_succeeded);
    assert!(!outcome.image_reference_query_failed);
    assert!(!outcome.image_collection_skipped);
    assert!(!outcome.rich_reference_query_succeeded);
    assert!(!outcome.rich_reference_query_failed);
    assert!(!outcome.rich_collection_skipped);
    assert_eq!(outcome.assets_removed_count, 0);
}

// ---------------------------------------------------------------------
// 7. Paste.
// ---------------------------------------------------------------------

#[test]
fn pasting_an_image_writes_the_bitmap_and_triggers_the_controller() {
    let h = harness(vec![]);
    let (id, _reference) = store_image(&h, 0x0A);

    let outcome = h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Plain);
    assert!(
        matches!(outcome, PasteOutcome::Pasted { id: pasted } if pasted == id),
        "got {outcome:?}"
    );

    let written = h.clipboard.written_images();
    assert_eq!(written.len(), 1, "exactly one bitmap reaches the clipboard");
    assert_eq!((written[0].width(), written[0].height()), (6, 6));
    assert_eq!(h.paste.invocations(), 1, "the paste controller runs after");
    assert!(
        h.clipboard.written_payloads().is_empty(),
        "an image must never be converted to text"
    );
}

#[test]
fn pasting_a_text_entry_still_writes_text_only() {
    let h = harness(vec![]);
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Text("paste me".into()),
        None,
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };

    let outcome = h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Plain);
    assert!(matches!(outcome, PasteOutcome::Pasted { .. }));
    assert_eq!(h.clipboard.written_payloads(), vec!["paste me".to_string()]);
    assert!(h.clipboard.written_images().is_empty());
}

#[test]
fn image_paste_without_write_capability_returns_a_typed_outcome() {
    let h = harness(vec![]);
    let (id, reference) = store_image(&h, 0x0B);
    // Model a session that can read images but not write them.
    h.clipboard.set_image_support(true, false);

    let outcome = h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Plain);
    match outcome {
        PasteOutcome::CapabilityUnavailable {
            capability,
            guidance,
        } => {
            assert_eq!(capability, CLIPBOARD_WRITE_IMAGE_CAPABILITY);
            let guidance = guidance.expect("actionable guidance");
            assert_eq!(guidance.capability, CLIPBOARD_WRITE_IMAGE_CAPABILITY);
        }
        other => panic!("expected CapabilityUnavailable, got {other:?}"),
    }

    // The entry, its metadata and its asset are all untouched, and the
    // image was never turned into text.
    assert_eq!(h.history_count(), 1);
    let record = h.record(id);
    assert!(record.is_renderable_image());
    assert_eq!(record.asset_ref.as_deref(), Some(reference.as_str()));
    assert!(h.store.read_bytes(&reference).is_ok());
    assert!(h.clipboard.written_payloads().is_empty());
    assert!(h.clipboard.written_images().is_empty());
    assert_eq!(h.paste.invocations(), 0, "paste must not be triggered");
}

#[test]
fn wayland_image_paste_reports_a_session_limit_not_a_permission() {
    // Wayland's lack of image transport is structural for this build, so
    // the guidance must be `UnsupportedSession` — presenting a
    // permission prompt would be a lie the user cannot act on.
    let h = harness_on(OsFamily::Linux, DisplayServer::Wayland, vec![], false);
    // The store still works, so seed the row through the store + repo
    // directly (the session cannot read images either).
    let normalized = clipvault_core::normalize_image(&bitmap(4, 4, 0x0C)).expect("normalize");
    let reference = h
        .store
        .store_image(&normalized)
        .expect("write")
        .asset_ref()
        .to_string();
    let id = {
        let mut db = h.context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        repo.insert_or_touch(clipvault_db::NewEntry {
            content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
            content_type: ContentType::Image,
            content_size: normalized.byte_len() as i64,
            content_hash: normalized.hash().to_string(),
            source_app: None,
            created_at: datetime!(2026-01-02 03:04:05 UTC),
            last_seen_at: datetime!(2026-01-02 03:04:05 UTC),
            asset_ref: Some(reference.clone()),
            mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
            payload_width: Some(4),
            payload_height: Some(4),
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
        })
        .expect("insert")
        .record()
        .id
    };

    match h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Plain)
    {
        PasteOutcome::CapabilityUnavailable {
            capability,
            guidance,
        } => {
            assert_eq!(capability, CLIPBOARD_WRITE_IMAGE_CAPABILITY);
            let guidance = guidance.expect("guidance");
            assert_eq!(
                guidance.kind,
                PlatformIssueKind::UnsupportedSession,
                "Wayland must not receive a permission prompt"
            );
            assert!(
                !guidance.can_open_settings,
                "there is no settings pane that fixes a session limitation"
            );
        }
        other => panic!("expected CapabilityUnavailable, got {other:?}"),
    }
    // History untouched.
    assert!(h.record(id).is_renderable_image());
}

#[test]
fn image_paste_failure_does_not_mutate_the_history_entry() {
    let h = harness(vec![]);
    let (id, reference) = store_image(&h, 0x0D);
    let before = h.record(id);
    h.clipboard
        .fail_image_write(ClipboardBackendError::backend("pasteboard busy"));

    let outcome = h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Plain);
    match outcome {
        PasteOutcome::Failed { kind, .. } => assert_eq!(kind, "clipboard_image"),
        other => panic!("expected Failed, got {other:?}"),
    }

    let after = h.record(id);
    assert_eq!(before, after, "the row must be byte-for-byte unchanged");
    assert!(h.store.read_bytes(&reference).is_ok(), "asset preserved");
    assert_eq!(h.paste.invocations(), 0);
}

#[test]
fn image_paste_with_a_missing_asset_fails_without_mutating_history() {
    let h = harness(vec![]);
    let (id, reference) = store_image(&h, 0x0E);
    let before = h.record(id);

    // Remove the asset behind ClipVault's back.
    let path = h
        .store
        .root()
        .join(reference.strip_prefix("clipboard/").expect("prefix"));
    std::fs::remove_file(&path).expect("remove asset");

    match h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Plain)
    {
        PasteOutcome::Failed { kind, message, .. } => {
            assert_eq!(kind, "asset_read");
            // Stable kind only — no reference, no path, no bytes.
            assert_eq!(message, "not_found");
            assert!(!message.contains('/'));
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    assert_eq!(before, h.record(id));
    assert_eq!(h.paste.invocations(), 0);
}

// ---------------------------------------------------------------------
// 8. Privacy of diagnostics: no bytes, hashes or paths leak.
// ---------------------------------------------------------------------

#[test]
fn typed_outcomes_never_carry_bytes_hashes_or_absolute_paths() {
    let h = harness(vec![]);
    let (id, reference) = store_image(&h, 0x0F);
    let record = h.record(id);
    let hash = record.content_hash.clone();
    let data_dir = h.context.platform().data_dir.display().to_string();

    // Collect every typed string surface the change introduces.
    let mut rendered = Vec::new();
    rendered.push(format!("{:?}", HistoryOutcome::Stored { id }));
    rendered.push(format!(
        "{:?}",
        h.context.history().record_clipboard_payload(
            &h.context,
            ClipboardPayload::Image(bitmap(6, 6, 0x0F)),
            None
        )
    ));
    rendered.push(format!("{:?}", ClipboardPayload::Image(bitmap(2, 2, 0xFF))));
    rendered.push(format!(
        "{:?}",
        ClipboardPayload::Text("super-secret-token".into())
    ));
    rendered.push(format!(
        "{:?}",
        clipvault_core::normalize_image(&bitmap(2, 2, 0xFF)).expect("normalize")
    ));
    for reference in ["", "/etc/passwd", "clipboard/ghost.png"] {
        if let Err(error) = h.store.read_bytes(reference) {
            rendered.push(error.to_string());
        }
    }
    rendered.push(format!(
        "{:?}",
        h.context
            .paste()
            .paste_entry(&h.context, 999_999, PasteMode::Plain)
    ));

    for text in &rendered {
        assert!(
            !text.contains(&hash),
            "content hash leaked into a diagnostic: {text}"
        );
        assert!(
            !text.contains(&data_dir),
            "absolute data dir leaked into a diagnostic: {text}"
        );
        assert!(
            !text.contains("super-secret-token"),
            "clipboard content leaked into a diagnostic: {text}"
        );
        assert!(
            !text.contains("/Users/"),
            "absolute path leaked into a diagnostic: {text}"
        );
    }
    // The reference itself is metadata the frontend legitimately holds,
    // but it must never appear inside an *error* message.
    let error = h.store.read_bytes("clipboard/ghost.png").unwrap_err();
    assert!(!error.to_string().contains(&reference));
}

#[test]
fn watcher_failure_messages_stay_metadata_only() {
    let h = harness(vec![]);
    let watcher = clipvault_core::CaptureWatcher::new(
        Arc::clone(&h.clipboard) as Arc<dyn ClipboardBackend>,
        std::time::Duration::from_millis(10),
    );
    h.clipboard
        .push_read(Err(ClipboardBackendError::backend("ContentNotAvailable")));

    match watcher.tick(&h.context, None) {
        WatchTickOutcome::Failed { message } => {
            assert_eq!(message, "clipboard backend failed: ContentNotAvailable");
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// 9. Capabilities per platform.
// ---------------------------------------------------------------------

#[test]
fn image_capabilities_are_reported_independently_per_session() {
    fn granted() -> bool {
        true
    }
    fn probe_available(_info: &PlatformInfo) -> clipvault_core::ImageClipboardSupport {
        clipvault_core::ImageClipboardSupport::BOTH
    }

    let info = |os: OsFamily, display: DisplayServer| PlatformInfo {
        home_dir: std::path::PathBuf::from("/tmp"),
        data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
        os_family: os,
        display_server: display,
    };

    let macos = clipvault_platform::detect_capabilities_with_probes(
        &info(OsFamily::Macos, DisplayServer::Unknown),
        granted,
        probe_available,
        clipvault_core::probe_rich_text_clipboard,
    );
    assert!(macos.clipboard_read_image && macos.clipboard_write_image);

    let x11 = clipvault_platform::detect_capabilities_with_probes(
        &info(OsFamily::Linux, DisplayServer::X11),
        granted,
        probe_available,
        clipvault_core::probe_rich_text_clipboard,
    );
    assert!(x11.clipboard_read_image && x11.clipboard_write_image);

    let wayland = clipvault_platform::detect_capabilities_with_probes(
        &info(OsFamily::Linux, DisplayServer::Wayland),
        granted,
        probe_available,
        clipvault_core::probe_rich_text_clipboard,
    );
    assert!(
        !wayland.clipboard_read_image && !wayland.clipboard_write_image,
        "Wayland must not inherit X11 image support"
    );
    // Text history keeps working under Wayland regardless.
    assert!(wayland.clipboard_read && wayland.clipboard_write);
}

#[test]
fn a_backend_without_image_support_never_claims_it() {
    let h = harness_on(OsFamily::Macos, DisplayServer::Unknown, vec![], false);
    assert!(!h.clipboard.supports_image_read());
    assert!(!h.clipboard.supports_image_write());
    // A clipboard holding an image but no text collapses to Ignored
    // rather than pretending to capture it.
    h.clipboard.push_read(Ok(None));
    h.clipboard.push_image_read(Ok(Some(bitmap(4, 4, 0x10))));
    assert!(h.clipboard.read_payload().expect("read").is_none());
    assert_eq!(h.history_count(), 0);
}
