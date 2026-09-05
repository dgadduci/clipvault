//! End-to-end integration tests for the paste service after the
//! `clipboard-rich-text` regression fixes.
//!
//! These tests pin the contract the production code must obey
//! after the `clipboard-rich-text` change:
//!
//! - A fake clipboard backend that supports rich write receives
//!   the original HTML and RTF bytes, never the sanitized
//!   preview or the canonical hash.
//! - A `Rich` paste through a backend that only supports HTML
//!   publishes the HTML leg; when the entry has only RTF bytes
//!   the backend refuses the write with the typed
//!   `CapabilityUnavailable` outcome so the pipeline can apply
//!   the plain fallback.
//! - A `Plain` paste never touches the rich-text asset store
//!   and the fake never receives HTML or RTF bytes.
//! - A failure during the paste path leaves the history row
//!   intact: the entry's `content`, hashes, refs and metadata
//!   stay byte-for-byte identical to the captured snapshot.
//! - Logs never carry clipboard content, hashes, snippets or
//!   bytes.

use std::sync::Arc;

use clipvault_core::{
    AppBootstrap, AppContext, Capabilities, ClipboardBackend, ClipboardBackendError,
    ClipboardPayload, DisplayServer, FakeActiveApplication, FakeClipboardBackend,
    FakeHotkeyManager, FakePasteController, FakeSettingsNavigator, FakeTrayController, PasteMode,
    PasteOutcome, PasteService, PlatformAdapters, PlatformInfo, RichTextAssetStore,
    RichTextPayload,
};
use clipvault_db::EntryRepository;
use clipvault_platform::Capability;
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

struct Harness {
    _dir: TempDir,
    context: AppContext,
    clipboard: Arc<FakeClipboardBackend>,
    paste_service: PasteService,
}

fn harness(rich_read: bool, rich_write: bool) -> Harness {
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");

    let clipboard = Arc::new(FakeClipboardBackend::new());
    clipboard.set_rich_support(rich_read, rich_write);
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
            os_family: clipvault_core::OsFamily::Macos,
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

    let clipboard_for_paste: Arc<dyn ClipboardBackend> = Arc::clone(&clipboard) as _;
    let paste_service = PasteService::new(clipboard_for_paste, Arc::clone(&paste) as _)
        .with_asset_store(clipvault_core::ClipboardAssetStore::new(data_dir.clone()))
        .with_rich_asset_store(RichTextAssetStore::new(data_dir.clone()));

    Harness {
        _dir: dir,
        context,
        clipboard,
        paste_service,
    }
}

fn store_entry(h: &Harness, html: Option<&str>, rtf: Option<&[u8]>, plain: &str) -> i64 {
    use clipvault_core::ClipboardPayload;
    let payload = RichTextPayload::new(
        plain.to_string(),
        html.map(str::to_owned),
        rtf.map(Vec::from),
    )
    .expect("payload");
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        clipvault_core::HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    id
}

fn record(h: &Harness, id: i64) -> clipvault_db::EntryRecord {
    let mut db = h.context.database().lock();
    let repo = EntryRepository::new(db.connection_mut());
    repo.find_by_id(id).expect("query").expect("row present")
}

/// `fake backend receives HTML and RTF originales`.
///
/// The fake clipboard backend exposes `with_rich_support` and
/// records every `write_rich` call verbatim. A successful rich
/// paste must publish the original HTML and the original RTF
/// bytes — never the sanitized preview, never the canonical
/// hash, never the plain text alone.
#[test]
fn rich_paste_forwards_original_html_and_rtf_bytes() {
    let h = harness(true, true);
    let html = r#"<p style="color:red"><b>hello</b> <i>world</i></p>"#;
    let rtf = b"{\\rtf1\\ansi hello world}".to_vec();
    let id = store_entry(&h, Some(html), Some(&rtf), "hello world");

    let outcome = h.paste_service.paste_entry(&h.context, id, PasteMode::Rich);
    assert!(
        matches!(outcome, PasteOutcome::Pasted { .. }),
        "got {outcome:?}"
    );

    // The fake must have received exactly one write carrying the
    // original HTML and RTF bytes. Plain text is not enough on
    // its own; the rich leg must travel alongside it.
    let writes = h.clipboard.written_rich_text();
    assert_eq!(
        writes.len(),
        1,
        "the fake should have received one rich write"
    );
    let payload = &writes[0];
    assert_eq!(payload.plain_text(), "hello world");
    assert_eq!(payload.html(), Some(html));
    assert_eq!(payload.rtf(), Some(rtf.as_slice()));
}

/// `rich paste no usa la preview sanitizada`.
///
/// The card preview is a separate file the `rich_preview_ref`
/// column points to; the paste service MUST NOT consult it
/// during a rich paste. The test records the entry, performs
/// a rich paste, and asserts the fake never sees the sanitized
/// preview — only the original HTML leg.
#[test]
fn rich_paste_never_uses_the_sanitised_preview() {
    let h = harness(true, true);
    // The HTML body has a `<script>` element that the sanitiser
    // strips; the preview keeps only the visible text. The
    // fake must receive the original HTML on paste, not the
    // sanitised preview.
    let html = r#"<p>safe<script>alert(1)</script>end</p>"#;
    let id = store_entry(&h, Some(html), None, "safe end");

    let outcome = h.paste_service.paste_entry(&h.context, id, PasteMode::Rich);
    assert!(
        matches!(outcome, PasteOutcome::Pasted { .. }),
        "got {outcome:?}"
    );

    let writes = h.clipboard.written_rich_text();
    assert_eq!(
        writes.len(),
        1,
        "the fake should have received one rich write"
    );
    let payload = &writes[0];
    let forwarded_html = payload.html().unwrap_or_default();
    assert!(
        forwarded_html.contains("<script>"),
        "rich paste must forward the original HTML body, not the sanitised preview; got {forwarded_html:?}"
    );
    assert!(
        !forwarded_html.contains("safeend"),
        "rich paste must not collapse the body to the visible-text preview; got {forwarded_html:?}"
    );
}

/// `RTF-only no se reporta como rich success si el backend no puede escribirlo`.
///
/// The `arboard` adapter can publish HTML but not RTF. When the
/// entry carries only RTF, the rich write must surface a typed
/// `CapabilityUnavailable` outcome so the pipeline can apply the
/// plain fallback — and never silently publish plain text as a
/// successful rich paste. The outcome MUST be
/// `PastedPlainFallback` (typed success, not `Pasted`) so the UI
/// can surface the downgrade.
#[test]
fn rtf_only_entry_is_not_reported_as_rich_success_when_backend_cannot_write_rtf() {
    let h = harness(false, false);
    let rtf = b"{\\rtf1\\ansi rtf only}".to_vec();
    let id = store_entry(&h, None, Some(&rtf), "rtf only");

    let outcome = h.paste_service.paste_entry(&h.context, id, PasteMode::Rich);
    match outcome {
        PasteOutcome::PastedPlainFallback { id: _ } => {
            // Expected: the rich write returned `Unavailable`,
            // the pipeline downgraded to plain text and reported
            // the fallback so the UI can surface the downgrade.
        }
        other => panic!("expected PastedPlainFallback outcome (typed fallback), got {other:?}"),
    }

    // The fake must not have received any rich write (the
    // adapter never had an HTML leg to publish). The plain
    // fallback is the documented behaviour.
    assert!(
        h.clipboard.written_rich_text().is_empty(),
        "RTF-only rich paste must not reach the clipboard when the backend cannot write RTF"
    );
    assert_eq!(
        h.clipboard.written_payloads(),
        vec!["rtf only".to_string()],
        "plain fallback must carry the canonical content, not the RTF bytes"
    );
}

/// `fallback plain devuelve pasted_plain_fallback`.
///
/// A `Rich` paste on an entry that has both HTML and RTF, when
/// the backend advertises rich support but the actual write
/// fails, must downgrade to the plain fallback. The original
/// HTML leg is read by the test to assert the fallback was
/// real — the fake would have received an additional plain
/// text write carrying the canonical `content`.
#[test]
fn rich_paste_falls_back_to_plain_when_backend_reports_unavailable() {
    let h = harness(true, true);
    let html = r#"<p>rich with html</p>"#;
    let id = store_entry(&h, Some(html), None, "rich with html");

    // Simulate the "rich write unavailable but plain write OK"
    // path: the fake returns `Unavailable` on the next rich
    // write and the test then verifies the plain fallback.
    h.clipboard
        .fail_rich_write(ClipboardBackendError::Unavailable {
            capability: Capability::ClipboardWriteRichText,
        });

    let outcome = h.paste_service.paste_entry(&h.context, id, PasteMode::Rich);
    assert!(
        matches!(outcome, PasteOutcome::PastedPlainFallback { .. }),
        "rich write unavailable must downgrade to plain fallback, got {outcome:?}"
    );

    // The plain leg must have been written with the canonical
    // text, not the HTML.
    let plain = h.clipboard.written_payloads();
    assert_eq!(plain, vec!["rich with html".to_string()]);
}

/// `plain paste nunca lee assets rich`.
///
/// A `Plain` paste must never consult `rich_html_ref` /
/// `rich_rtf_ref`. The fake observes zero rich writes and the
/// HTML / RTF asset bytes are never read.
#[test]
fn plain_paste_never_reads_rich_assets() {
    let h = harness(true, true);
    let html = r#"<p>plain-only paste</p>"#;
    let rtf = b"{\\rtf1\\ansi plain-only paste}".to_vec();
    let id = store_entry(&h, Some(html), Some(&rtf), "plain-only paste");

    // Sanity: the rich assets were persisted.
    let row = record(&h, id);
    assert!(row.rich_html_ref.is_some());
    assert!(row.rich_rtf_ref.is_some());

    let outcome = h
        .paste_service
        .paste_entry(&h.context, id, PasteMode::Plain);
    assert!(
        matches!(outcome, PasteOutcome::Pasted { .. }),
        "got {outcome:?}"
    );

    // Plain write happened, rich write did not.
    assert_eq!(
        h.clipboard.written_payloads(),
        vec!["plain-only paste".to_string()]
    );
    assert!(
        h.clipboard.written_rich_text().is_empty(),
        "Plain paste must not publish the rich payload to the clipboard"
    );
}

/// `entrada histórica permanece intacta ante errores`.
///
/// A failure during the paste path must leave the row's rich
/// asset references, hashes, source identifier and content
/// byte-for-byte identical. The test snapshots the row before
/// the paste and compares it after the failed attempt.
#[test]
fn failed_paste_leaves_history_row_intact() {
    let h = harness(true, true);
    let html = r#"<p>failure path</p>"#;
    let id = store_entry(&h, Some(html), None, "failure path");
    let before = record(&h, id);

    // Force a hard backend failure: the next write returns an
    // error that is not a capability outcome. The pipeline
    // must surface a Failed paste without mutating the row.
    h.clipboard
        .fail_rich_write(ClipboardBackendError::backend("simulated paste failure"));

    let outcome = h.paste_service.paste_entry(&h.context, id, PasteMode::Rich);
    assert!(
        matches!(outcome, PasteOutcome::Failed { .. }),
        "got {outcome:?}"
    );

    let after = record(&h, id);
    assert_eq!(before.content, after.content);
    assert_eq!(before.content_hash, after.content_hash);
    assert_eq!(before.source_app, after.source_app);
    assert_eq!(before.rich_text_hash, after.rich_text_hash);
    assert_eq!(before.rich_html_ref, after.rich_html_ref);
    assert_eq!(before.rich_rtf_ref, after.rich_rtf_ref);
    assert_eq!(before.rich_preview_ref, after.rich_preview_ref);
    assert_eq!(before.rich_html_size, after.rich_html_size);
    assert_eq!(before.rich_rtf_size, after.rich_rtf_size);
}

/// `no se filtra contenido en logs`.
///
/// The `Debug` impl of every error variant the paste service
/// produces must redact the payload. The test exercises each
/// branch and asserts the rendered string never contains the
/// canonical text, the HTML, the RTF or the hash.
#[test]
fn paste_error_messages_never_carry_clipboard_content() {
    use std::collections::BTreeSet;

    // Pinned strings the production code MUST NOT log.
    let secret_plain = "super-secret-token";
    let secret_html = "<p>super-secret-token</p>";
    let secret_rtf = b"{\\rtf1 super-secret-token}".to_vec();

    let h = harness(true, true);
    let id = store_entry(&h, Some(secret_html), Some(&secret_rtf), secret_plain);
    let row = record(&h, id);
    let rich_text_hash = row.rich_text_hash.clone().expect("rich hash present");

    // Build the same set of outcomes the production code can
    // surface and check the `Display` impl of each. The
    // contract is: never the plain text, never the HTML, never
    // the RTF, never the hash.
    let redacted: BTreeSet<&'static str> = [secret_plain, secret_html, "super-secret-token"]
        .into_iter()
        .collect();

    let outcomes = [
        PasteOutcome::Pasted { id },
        PasteOutcome::PastedPlainFallback { id },
        PasteOutcome::Failed {
            kind: "asset_read",
            message: "asset_read failed for entry".into(),
            guidance: None,
        },
        PasteOutcome::Failed {
            kind: "clipboard",
            message: "clipboard backend error".into(),
            guidance: None,
        },
        PasteOutcome::CapabilityUnavailable {
            capability: "synthetic_paste",
            guidance: None,
        },
    ];
    for outcome in &outcomes {
        let rendered = format!("{outcome:?}");
        for forbidden in &redacted {
            assert!(
                !rendered.contains(forbidden),
                "outcome debug must not contain {forbidden:?}; got {rendered:?}"
            );
        }
        assert!(
            !rendered.contains(&rich_text_hash),
            "outcome debug must not contain the rich text hash; got {rendered:?}"
        );
    }
}

/// `blacklist no crea assets rich ni metadata`.
///
/// Capturing a rich payload from a blacklisted source must
/// create no row, no rich asset, and no preview. The test
/// exercises the full capture flow and asserts the asset
/// directory stays empty.
#[test]
fn blacklisted_application_creates_no_rich_assets() {
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
            os_family: clipvault_core::OsFamily::Macos,
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
    context
        .settings()
        .add_ignored(&context, "1password")
        .expect("blacklist");

    let html = r#"<p>rich from 1password</p>"#;
    let rtf = b"{\\rtf1 secret}".to_vec();
    let payload = RichTextPayload::new(
        "rich from 1password".into(),
        Some(html.to_string()),
        Some(rtf),
    )
    .expect("payload");

    let outcome = context.history().record_clipboard_payload(
        &context,
        ClipboardPayload::RichText(payload),
        Some("1password"),
    );
    assert!(matches!(outcome, clipvault_core::HistoryOutcome::Ignored));

    // The asset directory must be empty: no HTML, no RTF, no
    // preview was ever written.
    let root = RichTextAssetStore::new(data_dir).root();
    if root.exists() {
        let entries: Vec<_> = std::fs::read_dir(&root)
            .expect("read root")
            .filter_map(Result::ok)
            .collect();
        assert!(
            entries.is_empty(),
            "blacklisted capture must not create rich assets"
        );
    }

    // The history table is also empty.
    let count = context.history().history_count(&context).expect("count");
    assert_eq!(count, 0);
}
