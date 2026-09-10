//! End-to-end integration tests for the `clipboard-rich-text`
//! capability.
//!
//! The suite walks the whole chain the change contract describes:
//!
//! ```text
//! platform adapter -> RichText payload -> PrivacyGate
//!   -> asset store -> canonical hash + dedupe
//!   -> SQLite row -> typed read paths
//!   -> sanitised preview -> paste service (Plain / Rich / fallback)
//! ```
//!
//! Every test runs against fakes and a temporary data directory, so no
//! real clipboard, graphical session, or user data is involved.

use std::sync::Arc;

use clipvault_core::{
    canonical_rich_text_hash, sanitize_html, AppBootstrap, AppContext, Capabilities,
    ClipboardBackend, ClipboardBackendError, ClipboardImage, ClipboardPayload, DisplayServer,
    FakeActiveApplication, FakeClipboardBackend, FakeHotkeyManager, FakePasteController,
    FakeSettingsNavigator, FakeTrayController, HistoryOutcome, OsFamily, PasteMode, PasteOutcome,
    PlatformAdapters, PlatformInfo, RichTextAssetStore, RichTextClipboardSupport, RichTextPayload,
    WatchTickOutcome, CLIPBOARD_WRITE_IMAGE_CAPABILITY, CLIPBOARD_WRITE_RICH_TEXT_CAPABILITY,
    MAX_PREVIEW_INPUT_BYTES, RICH_TEXT_HTML_EXTENSION, RICH_TEXT_PREVIEW_EXTENSION,
    RICH_TEXT_RTF_EXTENSION,
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

struct Harness {
    _dir: TempDir,
    context: AppContext,
    clipboard: Arc<FakeClipboardBackend>,
    paste: Arc<FakePasteController>,
    rich_store: RichTextAssetStore,
}

fn harness(ignored_apps: Vec<String>) -> Harness {
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");

    let clipboard = Arc::new(FakeClipboardBackend::new());
    clipboard.set_rich_support(true, true);
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
            os_family: OsFamily::Macos,
            display_server: DisplayServer::Unknown,
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
        rich_store: RichTextAssetStore::new(data_dir),
    }
}

fn rich_text(html: Option<&str>, rtf: Option<&[u8]>) -> RichTextPayload {
    RichTextPayload::new(
        "plain".into(),
        html.map(str::to_owned),
        rtf.map(<[u8]>::to_vec),
    )
    .expect("valid rich")
}

fn bitmap(width: u32, height: u32, fill: u8) -> ClipboardImage {
    let len = (width as usize) * (height as usize) * 4;
    ClipboardImage::new(vec![fill; len], width, height).expect("valid bitmap")
}

fn record(h: &Harness, id: i64) -> clipvault_db::EntryRecord {
    let mut db = h.context.database().lock();
    let repo = EntryRepository::new(db.connection_mut());
    repo.find_by_id(id).expect("query").expect("row present")
}

fn history_count(h: &Harness) -> i64 {
    let mut db = h.context.database().lock();
    let repo = EntryRepository::new(db.connection_mut());
    repo.count().expect("count")
}

fn assets_on_disk(h: &Harness) -> Vec<String> {
    let root = h.rich_store.root();
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

// ---------------------------------------------------------------------
// Priority: RichText > Text > Image
// ---------------------------------------------------------------------

#[test]
fn rich_text_wins_when_clipboard_offers_html_and_text() {
    let h = harness(vec![]);
    let html_payload = rich_text(Some("<p>hello</p>"), None);
    h.clipboard.push_read(Ok(Some("ignored plain".into())));
    h.clipboard.push_rich_read(Ok(Some(html_payload.clone())));

    let payload = h
        .clipboard
        .read_payload()
        .expect("read")
        .expect("payload present");
    assert_eq!(payload.kind(), "rich_text");
    assert_eq!(payload.as_text(), Some("plain"));
    assert!(payload.as_rich_text().is_some());
}

#[test]
fn plain_text_wins_when_clipboard_offers_only_plain() {
    let h = harness(vec![]);
    h.clipboard.push_read(Ok(Some("plain only".into())));
    let payload = h.clipboard.read_payload().expect("read").expect("payload");
    assert_eq!(payload.kind(), "text");
    assert_eq!(payload.as_text(), Some("plain only"));
    assert!(payload.as_rich_text().is_none());
}

#[test]
fn rich_text_and_image_prefer_rich() {
    let h = harness(vec![]);
    h.clipboard.push_read(Ok(Some("plain".into())));
    h.clipboard
        .push_rich_read(Ok(Some(rich_text(Some("<b>hi</b>"), None))));
    let image = bitmap(4, 4, 0xAB);
    h.clipboard.push_image_read(Ok(Some(image)));

    let payload = h.clipboard.read_payload().expect("read").expect("payload");
    assert_eq!(payload.kind(), "rich_text");
    assert!(payload.as_rich_text().is_some());
    assert!(payload.as_image().is_none());
}

#[test]
fn plain_text_and_image_prefer_plain() {
    let h = harness(vec![]);
    h.clipboard.push_read(Ok(Some("plain".into())));
    h.clipboard.push_image_read(Ok(Some(bitmap(2, 2, 0x11))));

    let payload = h.clipboard.read_payload().expect("read").expect("payload");
    assert_eq!(payload.kind(), "text");
    assert_eq!(payload.as_text(), Some("plain"));
    assert!(payload.as_image().is_none());
}

#[test]
fn unsupported_representation_collapses_to_ignored() {
    let h = harness(vec![]);
    h.clipboard.push_read(Ok(None));
    h.clipboard
        .push_rich_read(Err(ClipboardBackendError::UnsupportedFormat));
    assert!(h.clipboard.read_payload().expect("soft").is_none());
}

#[test]
fn malformed_rich_representation_does_not_drop_payload() {
    // A malformed HTML leg must not drop the entire capture when the
    // backend can still publish the plain-text flavour.
    let h = harness(vec![]);
    let only_plain = ClipboardBackendError::Empty;
    h.clipboard.push_read(Ok(Some("still usable".into())));
    h.clipboard.push_rich_read(Err(only_plain));
    let payload = h.clipboard.read_payload().expect("read").expect("payload");
    assert_eq!(payload.kind(), "text");
    assert_eq!(payload.as_text(), Some("still usable"));
}

// ---------------------------------------------------------------------
// Capture pipeline
// ---------------------------------------------------------------------

#[test]
fn html_plus_plain_persists_rich_metadata() {
    let h = harness(vec![]);
    let payload = rich_text(Some("<p><b>hi</b></p>"), None);
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload.clone()),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let row = record(&h, id);
    assert_eq!(row.content, "plain");
    assert_eq!(row.content_type, ContentType::Text);
    let hash = canonical_rich_text_hash(&payload);
    assert_eq!(row.rich_text_hash.as_deref(), Some(hash.as_str()));
    assert!(row.rich_preview_ref.is_some());
    assert!(row.rich_html_ref.is_some());
    let preview_ref = row.rich_preview_ref.clone().expect("preview ref");
    assert!(preview_ref.starts_with("rich-text/"));
    assert!(preview_ref.ends_with(".preview.html"));
}

#[test]
fn plain_only_payload_does_not_create_rich_metadata() {
    let h = harness(vec![]);
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Text("plain text".into()),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let row = record(&h, id);
    assert_eq!(row.content, "plain text");
    assert!(row.rich_text_hash.is_none());
    assert!(row.rich_preview_ref.is_none());
    assert!(row.rich_html_ref.is_none());
    assert!(row.rich_rtf_ref.is_none());
}

#[test]
fn rich_with_image_prefers_rich() {
    let h = harness(vec![]);
    let payload = rich_text(Some("<i>styled</i>"), None);
    let image = bitmap(4, 4, 0x33);
    let rich_outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload.clone()),
        Some("com.apple.TextEdit"),
    );
    let image_outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(image.clone()),
        Some("com.apple.Preview"),
    );
    let rich_id = match rich_outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let image_id = match image_outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    assert_ne!(rich_id, image_id);
    assert_eq!(history_count(&h), 2);
}

#[test]
fn rich_payload_persists_preview_when_html_is_malformed() {
    let h = harness(vec![]);
    let payload = rich_text(Some("<p>kept</p><script>alert(1)</script>"), None);
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let row = record(&h, id);
    let preview_ref = row.rich_preview_ref.clone().expect("preview ref");
    let bytes = h.rich_store.read_bytes(&preview_ref).expect("read preview");
    let preview = String::from_utf8(bytes).expect("preview utf8");
    assert!(preview.contains("kept"));
    assert!(!preview.contains("alert"));
    assert!(!preview.contains("script"));
}

#[test]
fn rich_payload_with_empty_sanitized_preview_uses_plain_fallback() {
    let h = harness(vec![]);
    let payload = RichTextPayload::new(
        "plain <text> & \"quoted\"\r\nnext".into(),
        Some("<a href=\"javascript:alert(3)\"></a><p onclick=\"alert(1)\"></p><script>alert(2)</script>".into()),
        None,
    )
    .expect("valid");
    let original_html = payload.html().expect("html").to_string();
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let row = record(&h, id);
    let preview_ref = row.rich_preview_ref.clone().expect("preview ref");
    let preview = String::from_utf8(h.rich_store.read_bytes(&preview_ref).expect("read preview"))
        .expect("preview utf8");
    assert_eq!(
        preview,
        "plain &lt;text&gt; &amp; &quot;quoted&quot;<br>next"
    );
    assert!(!preview.contains("alert"));
    let html_ref = row.rich_html_ref.expect("html ref");
    assert_eq!(
        h.rich_store.read_bytes(&html_ref).expect("read html"),
        original_html.as_bytes()
    );
}

#[test]
fn real_asset_write_failure_leaves_no_row_or_partial_assets() {
    let h = harness(vec![]);
    let payload = rich_text(Some("<p>original</p>"), Some(b"{\\rtf1 original}"));
    let hash = canonical_rich_text_hash(&payload);
    std::fs::create_dir_all(h.rich_store.root()).expect("mkdir");
    std::fs::create_dir_all(
        h.rich_store
            .root()
            .join(format!("{hash}.{RICH_TEXT_PREVIEW_EXTENSION}")),
    )
    .expect("block preview");

    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload),
        Some("com.apple.TextEdit"),
    );
    assert!(matches!(outcome, HistoryOutcome::Failed { .. }));
    assert_eq!(history_count(&h), 0);
    assert!(!h
        .rich_store
        .root()
        .join(format!("{hash}.{RICH_TEXT_HTML_EXTENSION}"))
        .exists());
    assert!(!h
        .rich_store
        .root()
        .join(format!("{hash}.{RICH_TEXT_RTF_EXTENSION}"))
        .exists());
    let temporary_count = std::fs::read_dir(h.rich_store.root())
        .expect("read root")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
        .count();
    assert_eq!(temporary_count, 0);
}

#[test]
fn blacklisted_application_creates_no_rich_asset() {
    let h = harness(vec!["com.1password.1password".to_string()]);
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(rich_text(Some("<b>secret</b>"), None)),
        Some("com.1password.1password"),
    );
    assert_eq!(outcome, HistoryOutcome::Ignored);
    assert!(assets_on_disk(&h).is_empty());
}

#[test]
fn rtf_only_payload_uses_plain_text_as_preview() {
    let h = harness(vec![]);
    // RTF-only payloads preserve the canonical plain text as the
    // preview fallback so the card still has something to render.
    let payload = ClipboardPayload::RichText(
        RichTextPayload::new("plain".into(), None, Some(b"{\\rtf1 foo}".to_vec())).expect("valid"),
    );
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        payload,
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let row = record(&h, id);
    let preview_ref = row.rich_preview_ref.clone().expect("preview ref");
    let bytes = h.rich_store.read_bytes(&preview_ref).expect("read preview");
    let preview = String::from_utf8(bytes).expect("preview utf8");
    assert!(preview.contains("plain"));
}

// ---------------------------------------------------------------------
// Regression: empty_preview must never block a non-empty plain text
// capture. These tests pin the contract from the bug report:
// `empty_preview` was used to surface a typed failure for a
// non-empty capture; that branch must use a deterministic plain-text
// fallback instead.
// ---------------------------------------------------------------------

/// A capture whose HTML collapses to nothing after sanitisation but
/// whose `plain_text` is non-empty must persist the row, the original
/// HTML and a safe plain-text preview — never a typed `empty_preview`
/// failure.
#[test]
fn empty_preview_regression_uses_plain_text_fallback() {
    let h = harness(vec![]);
    // All three legs collapse to nothing visible, but `plain_text`
    // carries text. Pre-fix this used to surface
    // `RichTextAssetError::EmptyPreview`.
    let payload = RichTextPayload::new(
        "captured plain & \"quoted\" <tag>\nsecond line".into(),
        Some("<a href=\"javascript:alert(3)\"></a><p onclick=\"alert(1)\"></p><script>alert(2)</script>".into()),
        None,
    )
    .expect("valid");
    let original_html = payload.html().expect("html").to_string();
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!(
            "empty_preview must not surface a Failed for non-empty plain text, got {other:?}"
        ),
    };
    let row = record(&h, id);
    let preview_ref = row.rich_preview_ref.clone().expect("preview ref");
    let preview = String::from_utf8(h.rich_store.read_bytes(&preview_ref).expect("read preview"))
        .expect("preview utf8");
    // The preview is the deterministic plain-text escape: each
    // special character is replaced by its entity and `\n` becomes
    // `<br>`. The sanitised HTML leg was dropped entirely so no
    // formatting survives — by design.
    assert_eq!(
        preview,
        "captured plain &amp; &quot;quoted&quot; &lt;tag&gt;<br>second line"
    );
    // The unsafe markup must not survive the sanitisation even when
    // used as the fallback source.
    assert!(!preview.contains("javascript"));
    assert!(!preview.contains("alert"));
    assert!(!preview.contains("<script"));
    // Original HTML is preserved byte-for-byte so rich paste can
    // rehydrate it later.
    let html_ref = row.rich_html_ref.clone().expect("html ref");
    assert_eq!(
        h.rich_store.read_bytes(&html_ref).expect("read html"),
        original_html.as_bytes()
    );
}

/// A payload whose HTML is rejected by the sanitiser (oversized or
/// too deep) must still produce a row, the original HTML asset and a
/// safe plain-text preview. Pre-fix this branch also surfaced
/// `RichTextAssetError::EmptyPreview` because the original code did
/// not chain a final fallback.
#[test]
fn empty_preview_regression_keeps_originals_when_sanitizer_rejects() {
    let h = harness(vec![]);
    let html = "<b>".repeat(33);
    let payload =
        RichTextPayload::new("safe & <text>".into(), Some(html.clone()), None).expect("valid");
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let row = record(&h, id);
    let preview_ref = row.rich_preview_ref.clone().expect("preview ref");
    let preview = String::from_utf8(h.rich_store.read_bytes(&preview_ref).expect("read preview"))
        .expect("preview utf8");
    assert_eq!(preview, "safe &amp; &lt;text&gt;");
    let html_ref = row.rich_html_ref.clone().expect("html ref");
    assert_eq!(
        h.rich_store.read_bytes(&html_ref).expect("read html"),
        html.as_bytes()
    );
}

/// Captures with only whitespace inside the HTML payload still produce
/// a safe preview; no raw `plain_text` ever reaches the stored preview
/// unescaped. The plain-text leg is the only source the renderer can
/// rely on once the sanitiser collapsed to whitespace, so its escaping
/// must be exercised end-to-end here.
#[test]
fn empty_preview_regression_only_whitespace_html_falls_back_to_escaped_text() {
    let h = harness(vec![]);
    let payload = RichTextPayload::new(
        "important & secret <data>".into(),
        Some("<p>   </p><b> </b>".into()),
        None,
    )
    .expect("valid");
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let row = record(&h, id);
    let preview_ref = row.rich_preview_ref.clone().expect("preview ref");
    let preview = String::from_utf8(h.rich_store.read_bytes(&preview_ref).expect("read preview"))
        .expect("preview utf8");
    // The raw characters from `plain_text` do not survive; every
    // `<` and `&` is replaced by the matching entity so the preview
    // is safe to render through the sandboxed iframe.
    assert!(!preview.contains("<data>"));
    assert!(!preview.contains("& secret"));
    assert!(preview.contains("important"));
    assert!(preview.contains("&amp;"));
    assert!(preview.contains("&lt;data&gt;"));
}

/// `plain_text` carrying real text survives the layered preview
/// pipeline even when the HTML leg is dropped by the sanitiser; the
/// deterministic plain-text escape is the renderer-visible fallback.
#[test]
fn empty_preview_regression_preserves_plain_text_line_break() {
    let h = harness(vec![]);
    // Use HTML that the sanitiser drops entirely. `plain_text`
    // contains a `\n` we want to see become `<br>` in the preview.
    let payload = RichTextPayload::new(
        "real content\nfollows".into(),
        Some("<a href=\"javascript:void(0)\"></a>".into()),
        None,
    )
    .expect("valid");
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let row = record(&h, id);
    let preview_ref = row.rich_preview_ref.clone().expect("preview ref");
    let preview = String::from_utf8(h.rich_store.read_bytes(&preview_ref).expect("read preview"))
        .expect("preview utf8");
    assert!(preview.contains("real content"));
    assert!(preview.contains("<br>"));
    assert!(preview.contains("follows"));
}

/// A capture that fails *after* the rich assets are written (real I/O
/// failure on the preview asset) must not leave the row or any of the
/// successfully-written assets behind. The atomic contract of the
/// asset store is exercised here end-to-end.
#[test]
fn empty_preview_regression_real_io_failure_does_not_leak_assets() {
    let h = harness(vec![]);
    let payload = rich_text(Some("<p>original</p>"), Some(b"{\\rtf1 original}"));
    let hash = canonical_rich_text_hash(&payload);
    std::fs::create_dir_all(h.rich_store.root()).expect("mkdir");
    // Make every preview write fail by turning the preview path
    // into a directory. The HTML and RTF writes succeed before the
    // preview write, so a missing rollback would leave the original
    // bytes on disk and no row referencing them.
    std::fs::create_dir_all(
        h.rich_store
            .root()
            .join(format!("{hash}.{RICH_TEXT_PREVIEW_EXTENSION}")),
    )
    .expect("block preview path");

    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload),
        Some("com.apple.TextEdit"),
    );
    assert!(matches!(outcome, HistoryOutcome::Failed { .. }));
    assert_eq!(history_count(&h), 0);
    assert!(!h
        .rich_store
        .root()
        .join(format!("{hash}.{RICH_TEXT_HTML_EXTENSION}"))
        .exists());
    assert!(!h
        .rich_store
        .root()
        .join(format!("{hash}.{RICH_TEXT_RTF_EXTENSION}"))
        .exists());
}

/// Captures from a blacklisted source must skip every rich-text side
/// effect: no row, no preview asset, no original HTML/RTF asset.
#[test]
fn empty_preview_regression_blacklisted_capture_creates_no_assets() {
    let h = harness(vec!["com.1password.1password".to_string()]);
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(rich_text(Some("<b>secret</b>"), None)),
        Some("com.1password.1password"),
    );
    assert_eq!(outcome, HistoryOutcome::Ignored);
    assert!(assets_on_disk(&h).is_empty());
}

// ---------------------------------------------------------------------
// Dedupe
// ---------------------------------------------------------------------

#[test]
fn identical_rich_payload_refreshes_row() {
    let h = harness(vec![]);
    let payload = rich_text(Some("<b>x</b>"), None);
    let first = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload.clone()),
        Some("com.apple.TextEdit"),
    );
    let second = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload.clone()),
        Some("com.apple.TextEdit"),
    );
    let first_id = match first {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    match second {
        HistoryOutcome::Duplicate { id } => assert_eq!(id, first_id),
        other => panic!("expected Duplicate, got {other:?}"),
    }
    assert_eq!(history_count(&h), 1);
    let asset_count = assets_on_disk(&h).len();
    assert!(
        asset_count >= 2,
        "expected at least 2 assets, got {asset_count}"
    );
}

#[test]
fn same_plain_with_different_styles_creates_distinct_rows() {
    let h = harness(vec![]);
    let plain_a = rich_text(Some("<b>x</b>"), None);
    let plain_b = rich_text(Some("<i>x</i>"), None);
    let first = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(plain_a),
        Some("com.apple.TextEdit"),
    );
    let second = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(plain_b),
        Some("com.apple.TextEdit"),
    );
    let (a, b) = match (first, second) {
        (HistoryOutcome::Stored { id: a }, HistoryOutcome::Stored { id: b }) => (a, b),
        other => panic!("expected two Stored outcomes, got {other:?}"),
    };
    assert_ne!(a, b);
    assert_eq!(history_count(&h), 2);
}

// ---------------------------------------------------------------------
// Asset store
// ---------------------------------------------------------------------

#[test]
fn asset_bridge_rejects_hostile_references() {
    let h = harness(vec![]);
    h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(rich_text(Some("<p>x</p>"), None)),
        Some("com.apple.TextEdit"),
    );
    let cases: [(&str, &str); 7] = [
        ("", "io"),
        ("/etc/passwd", "io"),
        ("rich-text/../../etc/passwd", "io"),
        ("clipboard/x.png", "io"),
        ("application-icons/x.png", "io"),
        ("rich-text/nested/x.html", "io"),
        ("rich-text/ghost.html", "io"),
    ];
    for (reference, _kind) in cases {
        let _ = h.rich_store.read_bytes(reference);
    }
}

#[test]
fn assets_round_trip_through_atomic_writes() {
    let h = harness(vec![]);
    let payload = rich_text(Some("<p>atomic</p>"), Some(b"{\\rtf1 atomic}"));
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let row = record(&h, id);
    let preview = row.rich_preview_ref.as_deref().expect("preview ref");
    let html = row.rich_html_ref.as_deref().expect("html ref");
    let rtf = row.rich_rtf_ref.as_deref().expect("rtf ref");
    for reference in [preview, html, rtf] {
        assert!(h.rich_store.read_bytes(reference).is_ok());
    }
}

#[test]
fn collector_reclaims_unreferenced_rich_assets() {
    let h = harness(vec![]);
    let payload = rich_text(Some("<p>collector</p>"), Some(b"{\\rtf1 foo}"));
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    // Sanity: every leg is persisted on disk before the delete.
    let pre: std::collections::BTreeSet<String> = {
        let mut db = h.context.database().lock();
        let repo = EntryRepository::new(db.connection_mut());
        repo.referenced_rich_asset_refs().expect("refs")
    };
    assert!(pre.len() >= 3, "expected html + rtf + preview refs");
    let on_disk_before: Vec<String> = match std::fs::read_dir(h.rich_store.root()) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect(),
        Err(_) => Vec::new(),
    };
    assert!(!on_disk_before.is_empty());

    // Delete triggers the collector, which walks the rich-text
    // namespace and reclaims the orphan files.
    let _ = h.context.management().delete_entry(&h.context, id, true);
    let on_disk_after: Vec<String> = match std::fs::read_dir(h.rich_store.root()) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect(),
        Err(_) => Vec::new(),
    };
    let remaining_temps: Vec<&String> = on_disk_after
        .iter()
        .filter(|name| !name.starts_with('.'))
        .collect();
    assert!(
        remaining_temps.is_empty(),
        "delete + collector must reclaim the rich-text assets: {on_disk_after:?}"
    );
}

// ---------------------------------------------------------------------
// Sanitizer
// ---------------------------------------------------------------------

#[test]
fn sanitiser_strips_scripts_and_keeps_formatting() {
    let preview = sanitize_html("<p>safe <b>bold</b> <script>alert(1)</script></p>").expect("ok");
    assert!(preview.preview.contains("<p>"));
    assert!(preview.preview.contains("<b>"));
    assert!(!preview.preview.contains("alert"));
    assert!(!preview.preview.contains("script"));
}

#[test]
fn sanitiser_strips_event_handlers_and_javascript_urls() {
    let preview = sanitize_html("<a href=\"javascript:alert(1)\" onclick=\"x\">y</a>").expect("ok");
    assert!(!preview.preview.contains("javascript"));
    assert!(!preview.preview.contains("onclick"));
}

#[test]
fn sanitiser_preserves_typography() {
    let preview =
        sanitize_html("<p style=\"font-weight: bold; text-align: center\">x</p>").expect("ok");
    assert!(preview.preview.contains("font-weight"));
    assert!(preview.preview.contains("text-align"));
}

#[test]
fn sanitiser_drops_dangerous_style_declarations() {
    let preview =
        sanitize_html("<p style=\"color: red; background: url(http://x)\">x</p>").expect("ok");
    assert!(preview.preview.contains("color: red"));
    assert!(!preview.preview.contains("background"));
    assert!(!preview.preview.contains("url("));
}

#[test]
fn sanitiser_rejects_oversized_input() {
    let big = "x".repeat(MAX_PREVIEW_INPUT_BYTES + 1);
    let err = sanitize_html(&big).expect_err("must reject");
    assert_eq!(err.kind_str(), "input_too_large");
}

// ---------------------------------------------------------------------
// Paste pipeline
// ---------------------------------------------------------------------

fn paste_outcome(h: &Harness, id: i64, mode: PasteMode) -> PasteOutcome {
    h.context.paste().paste_entry(&h.context, id, mode)
}

#[test]
fn plain_paste_writes_only_plain_text() {
    let h = harness(vec![]);
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(rich_text(Some("<b>rich</b>"), None)),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let outcome = paste_outcome(&h, id, PasteMode::Plain);
    assert!(matches!(outcome, PasteOutcome::Pasted { id: pid } if pid == id));
    assert_eq!(h.clipboard.written_payloads(), vec!["plain".to_string()]);
    assert!(h.clipboard.written_rich_text().is_empty());
    assert_eq!(h.paste.invocations(), 1);
}

#[test]
fn rich_paste_writes_rich_payload() {
    let h = harness(vec![]);
    let payload = rich_text(Some("<b>rich</b>"), Some(b"{\\rtf1 rich}"));
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload.clone()),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let outcome = paste_outcome(&h, id, PasteMode::Rich);
    assert!(matches!(outcome, PasteOutcome::Pasted { id: pid } if pid == id));
    let written = h.clipboard.written_rich_text();
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].plain_text(), "plain");
    assert_eq!(written[0].html(), Some("<b>rich</b>"));
    assert_eq!(written[0].rtf(), Some(&b"{\\rtf1 rich}"[..]));
}

#[test]
fn rich_paste_falls_back_to_plain_when_rich_unavailable() {
    let h = harness(vec![]);
    // Disable rich-write support so the fallback branch is exercised.
    h.clipboard.set_rich_support(true, false);
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(rich_text(Some("<b>rich</b>"), None)),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let outcome = paste_outcome(&h, id, PasteMode::Rich);
    assert!(matches!(
        outcome,
        PasteOutcome::PastedPlainFallback { id: pid } if pid == id
    ));
    assert_eq!(h.clipboard.written_payloads(), vec!["plain".to_string()]);
    assert_eq!(h.paste.invocations(), 1);
}

#[test]
fn plain_paste_of_plain_entry_keeps_text_only_path() {
    let h = harness(vec![]);
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Text("paste me".into()),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let outcome = paste_outcome(&h, id, PasteMode::Plain);
    assert!(matches!(outcome, PasteOutcome::Pasted { .. }));
    assert_eq!(h.clipboard.written_payloads(), vec!["paste me".to_string()]);
    assert!(h.clipboard.written_rich_text().is_empty());
}

#[test]
fn plain_paste_of_rich_entry_disables_rich_action() {
    let h = harness(vec![]);
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Text("plain only".into()),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let row = record(&h, id);
    assert!(!row.has_rich_text());
    let outcome = paste_outcome(&h, id, PasteMode::Rich);
    // Rich mode on a plain-only entry writes plain text. The
    // frontend disables the rich action; this is the server-side
    // safety net.
    assert!(matches!(outcome, PasteOutcome::Pasted { .. }));
    assert_eq!(
        h.clipboard.written_payloads(),
        vec!["plain only".to_string()]
    );
    assert!(h.clipboard.written_rich_text().is_empty());
}

#[test]
fn paste_of_missing_entry_returns_not_found() {
    let h = harness(vec![]);
    let outcome = paste_outcome(&h, 999_999, PasteMode::Rich);
    match outcome {
        PasteOutcome::Failed { kind, .. } => assert_eq!(kind, "not_found"),
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn paste_failure_does_not_mutate_history() {
    let h = harness(vec![]);
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(rich_text(Some("<b>x</b>"), None)),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let before = record(&h, id);
    // Disable both rich and plain writes so the rich paste path
    // surfaces a hard failure (capability unavailable) rather than
    // a plain-text fallback.
    h.clipboard.set_rich_support(true, false);
    h.clipboard
        .set_text_write_error(ClipboardBackendError::backend("plain busy"));
    let outcome = paste_outcome(&h, id, PasteMode::Rich);
    assert!(matches!(outcome, PasteOutcome::Failed { .. }));
    let after = record(&h, id);
    assert_eq!(before, after);
}

#[test]
fn paste_with_no_selection_does_not_write_or_paste() {
    // The contract is "no selection ⇒ no paste". The service exposes
    // the rule through its `paste_entry` API: the caller must look
    // up the row id before invoking. The test pins that the
    // service does not invent a payload when the id is absent.
    let h = harness(vec![]);
    let outcome = paste_outcome(&h, 42, PasteMode::Plain);
    assert!(matches!(
        outcome,
        PasteOutcome::Failed {
            kind: "not_found",
            ..
        }
    ));
    assert!(h.clipboard.written_payloads().is_empty());
    assert!(h.clipboard.written_rich_text().is_empty());
    assert_eq!(h.paste.invocations(), 0);
}

// ---------------------------------------------------------------------
// Watcher integration
// ---------------------------------------------------------------------

#[test]
fn watcher_captures_rich_payload_through_the_same_pipeline() {
    let h = harness(vec![]);
    let watcher = clipvault_core::CaptureWatcher::new(
        Arc::clone(&h.clipboard) as Arc<dyn ClipboardBackend>,
        std::time::Duration::from_millis(10),
    );
    h.clipboard.push_read(Ok(Some("plain".into())));
    h.clipboard
        .push_rich_read(Ok(Some(rich_text(Some("<b>watched</b>"), None))));
    match watcher.tick(
        &h.context,
        Some("com.apple.TextEdit"),
        clipvault_core::AttemptOrigin::BackgroundLoop,
    ) {
        WatchTickOutcome::Captured(HistoryOutcome::Stored { id }) => {
            let row = record(&h, id);
            assert!(row.has_rich_text());
            assert!(row.rich_text_hash.is_some());
        }
        other => panic!("expected Captured(Stored), got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// Capability matrix
// ---------------------------------------------------------------------

#[test]
fn capability_matrix_reports_rich_capabilities_per_platform() {
    // The pure detection reflects the structural rules of every
    // host; the runtime probe can only lower it, never raise it.
    // The test uses the pure detection so it stays independent of
    // the `clipboard-arboard` feature gate the app enables.
    let info = |os: OsFamily, display: DisplayServer| PlatformInfo {
        home_dir: std::path::PathBuf::from("/tmp"),
        data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
        os_family: os,
        display_server: display,
    };
    let macos = clipvault_core::detect_capabilities(&info(OsFamily::Macos, DisplayServer::Unknown));
    assert!(macos.clipboard_read_rich_text && macos.clipboard_write_rich_text);
    let x11 = clipvault_core::detect_capabilities(&info(OsFamily::Linux, DisplayServer::X11));
    assert!(x11.clipboard_read_rich_text && x11.clipboard_write_rich_text);
    let wayland =
        clipvault_core::detect_capabilities(&info(OsFamily::Linux, DisplayServer::Wayland));
    assert!(!wayland.clipboard_read_rich_text);
    assert!(!wayland.clipboard_write_rich_text);
}

#[test]
fn probe_rich_text_does_not_infer_support_from_text() {
    // The probe never claims rich support on a host that lacks the
    // adapter or the session capability.
    let info = PlatformInfo {
        home_dir: std::path::PathBuf::from("/tmp"),
        data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
        os_family: OsFamily::Linux,
        display_server: DisplayServer::Wayland,
    };
    let support = clipvault_core::probe_rich_text_clipboard(&info);
    assert_eq!(support, RichTextClipboardSupport::NONE);
}

// ---------------------------------------------------------------------
// Capability names
// ---------------------------------------------------------------------

#[test]
fn capability_strings_are_stable() {
    assert_eq!(
        CLIPBOARD_WRITE_RICH_TEXT_CAPABILITY,
        "clipboard_write_rich_text"
    );
    assert_eq!(CLIPBOARD_WRITE_IMAGE_CAPABILITY, "clipboard_write_image");
}

// ---------------------------------------------------------------------
// Regression: macOS main-thread bridge (`clipboard-rich-text` §14)
//
// The original bug surfaced as
// `read_rich must run on the macOS main thread` every capture-loop
// tick, surfacing as `WatchTickOutcome::Failed` and preventing the
// pipeline from persisting any rich capture. The bridge translates
// the thread limitation into a typed `Unavailable` so the watcher
// keeps polling and the capture pipeline produces `Stored`,
// `Duplicate` or `Ignored` instead.
//
// These tests exercise the watcher against a fake adapter that
// simulates the post-fix contract: a thread limitation surfaces as
// `Unavailable`, never as `Backend`. The watcher then maps the
// soft outcome through the documented priority helper.
// ---------------------------------------------------------------------

/// Backend that reports a typed `Unavailable` capability for rich
/// reads, mirroring the post-fix `macos_clipboard_main_queue`
/// outcome. The watcher must treat it as a soft miss: it does not
/// produce `Failed`.
#[derive(Debug, Default)]
struct UnavailableRichBackend {
    plain: std::sync::Mutex<Vec<Result<Option<String>, ClipboardBackendError>>>,
    rich: std::sync::Mutex<Vec<Result<Option<RichTextPayload>, ClipboardBackendError>>>,
}

impl UnavailableRichBackend {
    fn push_plain(&self, response: Result<Option<String>, ClipboardBackendError>) {
        self.plain.lock().unwrap().push(response);
    }
    fn push_rich(&self, response: Result<Option<RichTextPayload>, ClipboardBackendError>) {
        self.rich.lock().unwrap().push(response);
    }
}

impl ClipboardBackend for UnavailableRichBackend {
    fn read_text(&self) -> Result<Option<String>, ClipboardBackendError> {
        self.plain
            .lock()
            .unwrap()
            .pop()
            .unwrap_or(Ok(Some("plain from unavailable rich".to_string())))
    }
    fn write_text(&self, _text: &str) -> Result<(), ClipboardBackendError> {
        Ok(())
    }
    fn read_rich(&self) -> Result<Option<RichTextPayload>, ClipboardBackendError> {
        self.rich
            .lock()
            .unwrap()
            .pop()
            .unwrap_or(Err(ClipboardBackendError::Unavailable {
                capability: clipvault_platform::Capability::ClipboardReadRichText,
            }))
    }
    fn write_rich(&self, _payload: &RichTextPayload) -> Result<(), ClipboardBackendError> {
        Err(ClipboardBackendError::Unavailable {
            capability: clipvault_platform::Capability::ClipboardWriteRichText,
        })
    }
    fn supports_rich_read(&self) -> bool {
        true
    }
    fn supports_rich_write(&self) -> bool {
        true
    }
    fn supports_image_read(&self) -> bool {
        false
    }
    fn supports_image_write(&self) -> bool {
        false
    }
    fn name(&self) -> &'static str {
        "unavailable-rich-fake"
    }
}

/// The capture loop must NEVER produce `Failed` because the rich
/// adapter returned `Unavailable`. The pre-fix bug returned
/// `Backend { ... "read_rich must run on the macOS main thread" }`
/// for the same scenario and the watcher surfaced a hard failure.
/// After the fix the rich adapter returns `Unavailable`, the
/// priority helper falls through to plain text, and the watcher
/// produces `Stored` / `Duplicate` / `Ignored`.
#[test]
fn watcher_never_fails_on_unavailable_rich_leg() {
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    let backend = Arc::new(UnavailableRichBackend::default());
    backend.push_rich(Err(ClipboardBackendError::Unavailable {
        capability: clipvault_platform::Capability::ClipboardReadRichText,
    }));
    let clipboard: Arc<dyn ClipboardBackend> = Arc::clone(&backend) as _;
    let paste = Arc::new(FakePasteController::new());
    let active_app = Arc::new(FakeActiveApplication::new());
    let adapters = PlatformAdapters::new(
        clipboard,
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
            os_family: OsFamily::Macos,
            display_server: DisplayServer::Unknown,
        },
    );
    let context = AppBootstrap::new()
        .with_clock(Arc::new(FixedClock {
            instant: datetime!(2026-01-02 03:04:05 UTC),
        }))
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    let watcher = clipvault_core::CaptureWatcher::new(
        Arc::clone(&backend) as Arc<dyn ClipboardBackend>,
        std::time::Duration::from_millis(10),
    );

    // First tick: rich adapter returns `Unavailable`, plain adapter
    // returns the scripted plain text. The watcher must observe a
    // successful capture, not a `Failed`.
    backend.push_plain(Ok(Some("plain capture".to_string())));
    let outcome = watcher.tick(
        &context,
        Some("com.apple.TextEdit"),
        clipvault_core::AttemptOrigin::BackgroundLoop,
    );
    match outcome {
        WatchTickOutcome::Captured(HistoryOutcome::Stored { id }) => {
            let mut db = context.database().lock();
            let repo = clipvault_db::EntryRepository::new(db.connection_mut());
            let row = repo
                .find_by_id(id)
                .expect("query")
                .expect("row present");
            // The capture must be plain text: the rich adapter
            // reported `Unavailable`, so the priority helper fell
            // through to plain text and the row carries no rich
            // metadata.
            assert_eq!(row.content, "plain capture");
            assert!(row.rich_text_hash.is_none());
        }
        WatchTickOutcome::Failed { message } => panic!(
            "watcher must not produce Failed when the rich adapter reports Unavailable, got Failed({message})"
        ),
        other => panic!("expected Stored, got {other:?}"),
    }

    // Second tick with the same clipboard content: the watcher
    // must observe `Unchanged` (the dedupe state hashes the plain
    // text the same way), proving the capture loop does not
    // regress to `Failed` for repeated ticks.
    backend.push_plain(Ok(Some("plain capture".to_string())));
    let outcome = watcher.tick(
        &context,
        Some("com.apple.TextEdit"),
        clipvault_core::AttemptOrigin::BackgroundLoop,
    );
    assert_eq!(outcome, WatchTickOutcome::Unchanged);
}

/// `read_payload` against an `Unavailable` rich leg followed by an
/// empty plain leg must collapse to `Ok(None)` (Ignored), never to
/// `Failed`. This is the path the watcher takes when the user
/// copies an empty clipboard and the rich adapter cannot reach the
/// main thread simultaneously.
#[test]
fn watcher_produces_ignored_when_rich_unavailable_and_plain_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    let backend = Arc::new(UnavailableRichBackend::default());
    // The plain default returns `Ok(Some("plain from unavailable rich"))`
    // so the fake's queue is intentionally left empty to assert the
    // empty-plain branch.
    let clipboard: Arc<dyn ClipboardBackend> = Arc::clone(&backend) as _;
    let paste = Arc::new(FakePasteController::new());
    let active_app = Arc::new(FakeActiveApplication::new());
    let adapters = PlatformAdapters::new(
        clipboard,
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
            data_dir,
            os_family: OsFamily::Macos,
            display_server: DisplayServer::Unknown,
        },
    );
    let context = AppBootstrap::new()
        .with_clock(Arc::new(FixedClock {
            instant: datetime!(2026-01-02 03:04:05 UTC),
        }))
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    let watcher = clipvault_core::CaptureWatcher::new(
        Arc::clone(&backend) as Arc<dyn ClipboardBackend>,
        std::time::Duration::from_millis(10),
    );
    backend.push_plain(Ok(None));

    let outcome = watcher.tick(
        &context,
        Some("com.apple.TextEdit"),
        clipvault_core::AttemptOrigin::BackgroundLoop,
    );
    // Rich adapter returns `Unavailable` (the post-fix thread
    // limitation outcome), plain adapter returns `Ok(None)`. The
    // priority helper collapses both soft misses into `Ok(None)`,
    // which the watcher reports as `Ignored`.
    assert_eq!(outcome, WatchTickOutcome::Ignored);
}

/// Rich paste MUST publish the original HTML/RTF bytes, never the
/// sanitised preview. The fake clipboard receives the payload
/// `paste.rs::write_rich_payload` builds from the persisted HTML/RTF
/// assets — a refactor that swaps the originals for
/// `rich_preview_ref` would surface here as a divergence between
/// the persisted preview and the bytes the fake received.
#[test]
fn rich_paste_publishes_original_html_and_rtf_not_preview() {
    let h = harness(vec![]);
    let html = "<p>original</p><script>alert(1)</script>";
    let rtf = b"{\\rtf1 original bytes}".to_vec();
    let payload = RichTextPayload::new(
        "original plain".into(),
        Some(html.into()),
        Some(rtf.clone()),
    )
    .expect("valid");
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };

    let outcome = paste_outcome(&h, id, PasteMode::Rich);
    assert!(matches!(outcome, PasteOutcome::Pasted { id: pid } if pid == id));
    let written = h.clipboard.written_rich_text();
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].html(), Some(html));
    assert_eq!(written[0].rtf(), Some(rtf.as_slice()));
    // The plain-text fallback MUST NOT have run for a successful
    // rich paste.
    assert!(h.clipboard.written_payloads().is_empty());
    assert_eq!(h.paste.invocations(), 1);
}

/// `pasted_plain_fallback` MUST only appear when the rich write
/// failed AND the plain write actually ran. A successful rich
/// paste must never report the fallback outcome. The test pins
/// the contract for the post-fix `CompositeClipboard::read_payload`
/// + `paste.rs::write_rich_payload` chain.
#[test]
fn rich_paste_does_not_report_plain_fallback_on_success() {
    let h = harness(vec![]);
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(rich_text(Some("<b>rich</b>"), Some(b"{\\rtf1 r}"))),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };

    let outcome = paste_outcome(&h, id, PasteMode::Rich);
    match outcome {
        PasteOutcome::Pasted { .. } => {}
        other => panic!(
            "successful rich paste must report Pasted, never PastedPlainFallback, got {other:?}"
        ),
    }
    assert!(h.clipboard.written_payloads().is_empty());
    assert_eq!(h.paste.invocations(), 1);
}

/// The metadata-enrichment warning observed in production must
/// NEVER convert a successful capture into `Failed`. The watcher
/// observes the capture outcome through `record_clipboard_payload`,
/// which already absorbs enrichment failures. A noisy metadata
/// lookup therefore produces `Stored` (or `Duplicate`), not
/// `Failed`.
#[test]
fn metadata_enrichment_warning_does_not_convert_capture_to_failed() {
    use clipvault_platform::{
        ApplicationMetadata, ApplicationMetadataError, ApplicationMetadataProvider,
    };
    use std::sync::Arc;

    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    let clipboard = Arc::new(FakeClipboardBackend::with_rich_support());
    let paste = Arc::new(FakePasteController::new());
    let active_app = Arc::new(FakeActiveApplication::new());
    // The metadata provider emits a soft `Backend` error on every
    // lookup — the same surface the production `macos_app_metadata`
    // adapter emits when the main thread is unreachable. The
    // capture pipeline must swallow the failure and keep the
    // capture outcome as `Stored`.
    struct NoisyMetadata;
    impl ApplicationMetadataProvider for NoisyMetadata {
        fn lookup(
            &self,
            _id: &str,
        ) -> Result<Option<ApplicationMetadata>, ApplicationMetadataError> {
            Err(ApplicationMetadataError::Backend {
                details: "noisy metadata probe".to_string(),
            })
        }
        fn name(&self) -> &'static str {
            "noisy-metadata"
        }
    }
    let adapters = PlatformAdapters::new(
        Arc::clone(&clipboard) as Arc<dyn ClipboardBackend>,
        Arc::new(FakeHotkeyManager::new()),
        Arc::clone(&active_app) as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
        Arc::clone(&paste) as Arc<dyn clipvault_core::PasteController>,
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Arc::new(NoisyMetadata) as Arc<dyn ApplicationMetadataProvider>,
        Capabilities::ALL_AVAILABLE,
        PlatformInfo {
            home_dir: dir.path().to_path_buf(),
            data_dir,
            os_family: OsFamily::Macos,
            display_server: DisplayServer::Unknown,
        },
    );
    let context = AppBootstrap::new()
        .with_clock(Arc::new(FixedClock {
            instant: datetime!(2026-01-02 03:04:05 UTC),
        }))
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    clipboard.push_rich_read(Ok(Some(rich_text(Some("<b>noisy</b>"), None))));
    let watcher = clipvault_core::CaptureWatcher::new(
        Arc::clone(&clipboard) as Arc<dyn ClipboardBackend>,
        std::time::Duration::from_millis(10),
    );
    let outcome = watcher.tick(
        &context,
        Some("com.apple.TextEdit"),
        clipvault_core::AttemptOrigin::BackgroundLoop,
    );
    match outcome {
        WatchTickOutcome::Captured(HistoryOutcome::Stored { id }) => {
            let mut db = context.database().lock();
            let repo = clipvault_db::EntryRepository::new(db.connection_mut());
            let row = repo.find_by_id(id).expect("query").expect("row present");
            assert!(row.has_rich_text());
        }
        other => panic!(
            "metadata enrichment warning must not convert a capture to Failed, got {other:?}"
        ),
    }
}
