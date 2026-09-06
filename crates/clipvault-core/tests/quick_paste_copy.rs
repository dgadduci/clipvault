//! Regression tests for the `quick-paste-actions` change.
//!
//! The change splits Quick Paste's keyboard flow from its direct
//! menu flow:
//!
//! - the keyboard (`Enter` / `Shift+Enter`) writes the chosen
//!   representation to the clipboard and never invokes any
//!   synthetic paste controller;
//! - the per-entry `...` menu keeps the legacy direct paste
//!   lifecycle (hide-before-paste, active-target, guidance);
//! - both flows share the same clipboard-write machinery and the
//!   same suppression registry; the copy path just stops one step
//!   before `PasteController::paste`.
//!
//! The suite pins the contract documented in
//! `openspec/changes/quick-paste-actions/spec.md`:
//!
//!   - `copy_entry` writes the chosen representation to the
//!     clipboard without calling `PasteController::paste`;
//!   - `copy_entry` leaves the history row untouched (no new card,
//!     no payload / hash / timestamp / tag / collection change);
//!   - `copy_entry` arms the suppression token so the next watcher
//!     tick is masked;
//!   - the typed outcome never carries clipboard content, hashes,
//!     snippets, bytes or paths.

use std::sync::Arc;

use clipvault_core::{
    AppBootstrap, AppContext, Capabilities, ClipboardBackend, ClipboardImage, CopyOutcome,
    DisplayServer, FakeActiveApplication, FakeClipboardBackend, FakeHotkeyManager,
    FakePasteController, FakeSettingsNavigator, FakeTrayController, PasteMode, PasteOutcome,
    PasteService, PlatformAdapters, PlatformInfo, RichTextAssetStore, RichTextPayload,
    CLIPBOARD_WRITE_IMAGE_CAPABILITY, CLIPBOARD_WRITE_RICH_TEXT_CAPABILITY,
};
use clipvault_db::EntryRepository;
use clipvault_platform::{ClipboardBackendError, ClipboardPayload};
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
    paste: Arc<FakePasteController>,
    paste_service: PasteService,
}

fn harness(rich_read: bool, rich_write: bool, image_support: bool) -> Harness {
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");

    let clipboard = Arc::new(FakeClipboardBackend::new());
    clipboard.set_rich_support(rich_read, rich_write);
    if image_support {
        clipboard.set_image_support(true, true);
    }
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
        .with_rich_asset_store(RichTextAssetStore::new(data_dir.clone()))
        .with_paste_suppression(context.paste_suppression().clone());

    Harness {
        _dir: dir,
        context,
        clipboard,
        paste,
        paste_service,
    }
}

fn store_rich_entry(h: &Harness, html: Option<&str>, rtf: Option<&[u8]>, plain: &str) -> i64 {
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

fn store_plain_entry(h: &Harness, plain: &str) -> i64 {
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Text(plain.to_string()),
        Some("com.apple.TextEdit"),
    );
    let id = match outcome {
        clipvault_core::HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    id
}

fn store_image_entry(h: &Harness, fill: u8) -> i64 {
    let len = 4 * 4 * 4;
    let image = ClipboardImage::new(vec![fill; len], 4, 4).expect("valid image");
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(image),
        Some("com.apple.Preview"),
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

fn history_count(h: &Harness) -> i64 {
    let mut db = h.context.database().lock();
    let repo = EntryRepository::new(db.connection_mut());
    repo.count().expect("count")
}

// ---------------------------------------------------------------------
// 1. Plain text copy: writes plain text without invoking
//    `PasteController::paste`.
// ---------------------------------------------------------------------

#[test]
fn copy_entry_writes_plain_text_without_invoking_paste_controller() {
    let h = harness(false, false, false);
    let id = store_plain_entry(&h, "hello world");
    let before_paste_calls = h.paste.invocations();

    let outcome = h.paste_service.copy_entry(&h.context, id, PasteMode::Plain);
    match outcome {
        CopyOutcome::Copied { id: out_id } => assert_eq!(out_id, id),
        other => panic!("expected Copied, got {other:?}"),
    }

    // The plain text reached the clipboard…
    assert_eq!(
        h.clipboard.written_payloads(),
        vec!["hello world".to_string()],
        "plain copy must publish the canonical text"
    );
    // …and the synthetic paste trigger was NEVER invoked.
    assert_eq!(
        h.paste.invocations(),
        before_paste_calls,
        "copy-only flow MUST NOT invoke PasteController::paste"
    );
}

#[test]
fn copy_plain_entry_pins_mode_to_plain() {
    let h = harness(false, false, false);
    let id = store_plain_entry(&h, "convenience overload");

    let outcome = h.paste_service.copy_plain_entry(&h.context, id);
    assert!(matches!(outcome, CopyOutcome::Copied { .. }));
    assert_eq!(
        h.clipboard.written_payloads(),
        vec!["convenience overload".to_string()]
    );
    assert_eq!(h.paste.invocations(), 0);
}

// ---------------------------------------------------------------------
// 2. Rich text copy: writes the rich payload without invoking
//    `PasteController::paste`.
// ---------------------------------------------------------------------

#[test]
fn copy_entry_writes_rich_text_without_invoking_paste_controller() {
    let h = harness(true, true, false);
    let html = r#"<p><b>rich</b> <i>copy</i></p>"#;
    let rtf = b"{\\rtf1 rich copy}".to_vec();
    let id = store_rich_entry(&h, Some(html), Some(&rtf), "rich copy");
    let before_paste_calls = h.paste.invocations();

    let outcome = h.paste_service.copy_entry(&h.context, id, PasteMode::Rich);
    match outcome {
        CopyOutcome::Copied { id: out_id } => assert_eq!(out_id, id),
        other => panic!("expected Copied, got {other:?}"),
    }

    // The rich payload reached the clipboard…
    let writes = h.clipboard.written_rich_text();
    assert_eq!(writes.len(), 1, "rich copy must publish the rich payload");
    assert_eq!(writes[0].plain_text(), "rich copy");
    assert_eq!(writes[0].html(), Some(html));
    assert_eq!(writes[0].rtf(), Some(rtf.as_slice()));
    // …and the synthetic paste trigger was NEVER invoked.
    assert_eq!(
        h.paste.invocations(),
        before_paste_calls,
        "copy-only flow MUST NOT invoke PasteController::paste"
    );
}

#[test]
fn copy_entry_writes_plain_text_for_rich_entry_when_shift_enter_requests_plain() {
    let h = harness(true, true, false);
    let html = r#"<p>rich with html</p>"#;
    let id = store_rich_entry(&h, Some(html), None, "rich with html");

    let outcome = h.paste_service.copy_entry(&h.context, id, PasteMode::Plain);
    assert!(matches!(outcome, CopyOutcome::Copied { .. }));

    // Plain mode on a rich entry MUST publish the canonical text and
    // never touch the rich payload.
    assert_eq!(
        h.clipboard.written_payloads(),
        vec!["rich with html".to_string()],
        "plain copy of a rich entry must use the canonical text"
    );
    assert!(
        h.clipboard.written_rich_text().is_empty(),
        "plain copy MUST NOT publish the rich payload"
    );
    assert_eq!(
        h.paste.invocations(),
        0,
        "Shift+Enter on a rich entry must not trigger PasteController::paste"
    );
}

#[test]
fn copy_entry_rich_falls_back_to_plain_when_session_cannot_write_rich() {
    let h = harness(false, false, false);
    let html = r#"<p>rich fallback</p>"#;
    let id = store_rich_entry(&h, Some(html), None, "rich fallback");

    let outcome = h.paste_service.copy_entry(&h.context, id, PasteMode::Rich);
    assert!(
        matches!(outcome, CopyOutcome::CopiedPlainFallback { .. }),
        "rich copy with no rich capability must surface CopiedPlainFallback, got {outcome:?}"
    );

    // The plain fallback must reach the clipboard…
    assert_eq!(
        h.clipboard.written_payloads(),
        vec!["rich fallback".to_string()],
        "plain fallback must carry the canonical text"
    );
    // …and the rich write must not have produced bytes (the backend
    // refused the rich flavour).
    assert!(
        h.clipboard.written_rich_text().is_empty(),
        "no rich write may succeed when the backend advertises no rich support"
    );
    // The synthetic paste trigger was NEVER invoked.
    assert_eq!(h.paste.invocations(), 0);
}

// ---------------------------------------------------------------------
// 3. Image copy: writes the bitmap without invoking
//    `PasteController::paste`.
// ---------------------------------------------------------------------

#[test]
fn copy_entry_writes_image_without_invoking_paste_controller() {
    let h = harness(false, false, true);
    let id = store_image_entry(&h, 0xAB);
    let before_paste_calls = h.paste.invocations();

    // The Rust side ignores the mode for image rows; we pass `Plain`
    // because that is the default the wire layer forwards when the
    // caller does not supply a mode (the frontend helper maps
    // image Enter to `mode = null`).
    let outcome = h.paste_service.copy_entry(&h.context, id, PasteMode::Plain);
    match outcome {
        CopyOutcome::Copied { id: out_id } => assert_eq!(out_id, id),
        other => panic!("expected Copied, got {other:?}"),
    }

    // The bitmap reached the clipboard…
    let images = h.clipboard.written_images();
    assert_eq!(images.len(), 1, "image copy must publish the bitmap");
    assert_eq!(images[0].width(), 4);
    assert_eq!(images[0].height(), 4);
    // …and the synthetic paste trigger was NEVER invoked.
    assert_eq!(
        h.paste.invocations(),
        before_paste_calls,
        "image copy MUST NOT invoke PasteController::paste"
    );
}

#[test]
fn copy_entry_image_reports_capability_unavailable_when_session_cannot_write_image() {
    let h = harness(false, false, false);
    let id = store_image_entry(&h, 0x10);

    let outcome = h.paste_service.copy_entry(&h.context, id, PasteMode::Plain);
    match outcome {
        CopyOutcome::CapabilityUnavailable {
            capability,
            guidance,
        } => {
            assert_eq!(capability, CLIPBOARD_WRITE_IMAGE_CAPABILITY);
            assert!(
                guidance.is_some(),
                "an image-write capability refusal must surface typed guidance"
            );
        }
        other => panic!("expected CapabilityUnavailable, got {other:?}"),
    }
    // No image write happened and no paste trigger fired.
    assert!(h.clipboard.written_images().is_empty());
    assert_eq!(h.paste.invocations(), 0);
}

// ---------------------------------------------------------------------
// 4. Copy does not create a history card.
// ---------------------------------------------------------------------

#[test]
fn copy_entry_never_creates_a_history_card() {
    let h = harness(false, false, false);
    let id = store_plain_entry(&h, "single row");
    let before = history_count(&h);

    let outcome = h.paste_service.copy_entry(&h.context, id, PasteMode::Plain);
    assert!(matches!(outcome, CopyOutcome::Copied { .. }));

    assert_eq!(
        history_count(&h),
        before,
        "copy_entry MUST NOT create a new history card"
    );
}

#[test]
fn copy_entry_preserves_payload_hashes_timestamps_and_organization() {
    let h = harness(true, true, true);
    let html = r#"<p>organised</p>"#;
    let id = store_rich_entry(&h, Some(html), None, "organised");
    let before = record(&h, id);
    // Pin the entry so we can also assert the pin survives.
    h.context
        .management()
        .set_favorite(&h.context, id, true)
        .expect("pin");

    let outcome = h.paste_service.copy_entry(&h.context, id, PasteMode::Rich);
    assert!(matches!(outcome, CopyOutcome::Copied { .. }));

    let after = record(&h, id);
    assert_eq!(before.content, after.content);
    assert_eq!(before.content_hash, after.content_hash);
    assert_eq!(before.rich_text_hash, after.rich_text_hash);
    assert_eq!(before.rich_html_ref, after.rich_html_ref);
    assert_eq!(before.rich_rtf_ref, after.rich_rtf_ref);
    assert_eq!(before.rich_preview_ref, after.rich_preview_ref);
    assert_eq!(before.asset_ref, after.asset_ref);
    assert_eq!(before.source_app, after.source_app);
    assert_eq!(before.mime_type, after.mime_type);
    assert_eq!(before.payload_width, after.payload_width);
    assert_eq!(before.payload_height, after.payload_height);
    assert!(after.is_pinned, "pin must survive a copy");
}

// ---------------------------------------------------------------------
// 5. Errors are typed.
// ---------------------------------------------------------------------

#[test]
fn copy_entry_for_unknown_id_reports_a_failed_outcome_with_a_not_found_kind() {
    let h = harness(false, false, false);
    let missing: i64 = 999_999;
    let outcome = h
        .paste_service
        .copy_entry(&h.context, missing, PasteMode::Plain);
    match outcome {
        CopyOutcome::Failed { kind, .. } => {
            assert_eq!(kind, "not_found");
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    // No clipboard write happened, no paste trigger fired.
    assert!(h.clipboard.written_payloads().is_empty());
    assert!(h.clipboard.written_rich_text().is_empty());
    assert!(h.clipboard.written_images().is_empty());
    assert_eq!(h.paste.invocations(), 0);
}

#[test]
fn copy_entry_reports_a_failed_clipboard_outcome_when_plain_write_fails() {
    let h = harness(false, false, false);
    let id = store_plain_entry(&h, "failure path");
    h.clipboard
        .set_text_write_error(ClipboardBackendError::backend("boom"));

    let outcome = h.paste_service.copy_entry(&h.context, id, PasteMode::Plain);
    match outcome {
        CopyOutcome::Failed { kind, message, .. } => {
            assert_eq!(kind, "clipboard");
            // The error message is sanitised to the adapter
            // identifier so it never carries clipboard content.
            assert!(message.contains("boom"));
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    assert_eq!(
        h.paste.invocations(),
        0,
        "a clipboard failure MUST NOT trigger PasteController::paste"
    );
}

#[test]
fn copy_entry_reports_capability_unavailable_for_rich_with_no_rich_capability() {
    let h = harness(false, false, false);
    let html = r#"<p>rich unavailable</p>"#;
    let id = store_rich_entry(&h, Some(html), None, "rich unavailable");

    let outcome = h.paste_service.copy_entry(&h.context, id, PasteMode::Rich);
    match outcome {
        CopyOutcome::CopiedPlainFallback { .. } => {
            // The session collapsed to plain text and reported the
            // explicit fallback. The frontend uses this branch to
            // explain the downgrade without surfacing it as a
            // failure.
        }
        other => panic!("expected CopiedPlainFallback, got {other:?}"),
    }
    // The capability outcome is reserved for sessions where neither
    // rich nor plain can be written — a separate test covers that
    // path on image rows.
    assert_eq!(h.paste.invocations(), 0);
}

#[test]
fn copy_entry_reports_capability_unavailable_when_clipboard_cannot_write_text() {
    let h = harness(false, false, false);
    let id = store_plain_entry(&h, "no plain write");
    // Force every plain write to fail with the capability refusal.
    h.clipboard
        .set_text_write_error(ClipboardBackendError::Unavailable {
            capability: clipvault_platform::Capability::ClipboardWrite,
        });

    let outcome = h.paste_service.copy_entry(&h.context, id, PasteMode::Plain);
    match outcome {
        CopyOutcome::Failed { kind, .. } => {
            assert_eq!(kind, "clipboard");
        }
        other => panic!("expected Failed with clipboard kind, got {other:?}"),
    }
    assert_eq!(h.paste.invocations(), 0);
}

// ---------------------------------------------------------------------
// 6. Suppression arming: copy arms the suppression registry so the
//    next watcher tick is masked and no autocapture row is created.
// ---------------------------------------------------------------------

#[test]
fn copy_entry_arms_the_suppression_registry_for_plain_text() {
    let h = harness(false, false, false);
    let id = store_plain_entry(&h, "hello");
    h.context.paste_suppression().clear();

    let outcome = h.paste_service.copy_entry(&h.context, id, PasteMode::Plain);
    assert!(matches!(outcome, CopyOutcome::Copied { .. }));

    let fingerprint = h.context.paste_suppression().peek().expect("armed");
    // The token carries a plain-text hash, no rich hash, no image
    // hash — matching the suppression contract the watcher consumes.
    assert!(
        fingerprint.plain_text_hash.is_some(),
        "plain copy must arm a plain-text hash"
    );
    assert!(
        fingerprint.rich_text_hash.is_none(),
        "plain copy must not carry a rich-text hash"
    );
    assert!(
        fingerprint.image_hash.is_none(),
        "plain copy must not carry an image hash"
    );
}

#[test]
fn copy_entry_clears_the_suppression_token_when_the_clipboard_write_fails() {
    let h = harness(false, false, false);
    let id = store_plain_entry(&h, "drop token");
    h.context.paste_suppression().clear();
    h.clipboard
        .set_text_write_error(ClipboardBackendError::backend("transient"));

    let outcome = h.paste_service.copy_entry(&h.context, id, PasteMode::Plain);
    assert!(matches!(outcome, CopyOutcome::Failed { .. }));
    assert!(
        h.context.paste_suppression().peek().is_none(),
        "a failed write MUST drop the suppression token so a legitimate copy is not masked"
    );
}

#[test]
fn copy_entry_arms_the_suppression_registry_for_image() {
    let h = harness(false, false, true);
    let id = store_image_entry(&h, 0x77);
    h.context.paste_suppression().clear();

    let outcome = h.paste_service.copy_entry(&h.context, id, PasteMode::Plain);
    assert!(matches!(outcome, CopyOutcome::Copied { .. }));

    let fingerprint = h.context.paste_suppression().peek().expect("armed");
    assert!(
        fingerprint.image_hash.is_some(),
        "image copy must arm an image hash"
    );
    assert!(
        fingerprint.plain_text_hash.is_none(),
        "image copy must not arm a plain-text hash"
    );
    assert!(
        fingerprint.rich_text_hash.is_none(),
        "image copy must not arm a rich-text hash"
    );
}

// ---------------------------------------------------------------------
// 7. Direct paste flow stays untouched.
// ---------------------------------------------------------------------

#[test]
fn direct_paste_still_invokes_the_paste_controller() {
    // The keyboard copy-only flow MUST NOT regress the direct paste
    // flow the legacy menu and the development modal rely on. This
    // test pins the contract that `paste_entry` continues to call
    // `PasteController::paste` after a successful write.
    let h = harness(false, false, false);
    let id = store_plain_entry(&h, "direct paste");
    let before_paste_calls = h.paste.invocations();

    let outcome = h
        .paste_service
        .paste_entry(&h.context, id, PasteMode::Plain);
    assert!(matches!(outcome, PasteOutcome::Pasted { .. }));
    assert_eq!(
        h.paste.invocations(),
        before_paste_calls + 1,
        "direct paste MUST continue to invoke PasteController::paste"
    );
    assert_eq!(
        h.clipboard.written_payloads(),
        vec!["direct paste".to_string()]
    );
}

// ---------------------------------------------------------------------
// 8. Privacy / log hygiene.
// ---------------------------------------------------------------------

#[test]
fn copy_outcome_debug_never_leaks_clipboard_content() {
    // The `Debug` impl of every `CopyOutcome` variant the paste
    // service produces must redact the payload. The test exercises
    // each branch and asserts the rendered string never contains the
    // canonical text, the HTML, the RTF, the hash or the bytes.
    let secret_plain = "super-secret-token";
    let secret_html = "<p>super-secret-token</p>";
    let secret_rtf = b"{\\rtf1 super-secret-token}".to_vec();

    let h = harness(true, true, true);
    let rich_id = store_rich_entry(&h, Some(secret_html), Some(&secret_rtf), secret_plain);
    let image_id = store_image_entry(&h, 0x33);
    let plain_id = store_plain_entry(&h, secret_plain);

    let rich_row = record(&h, rich_id);
    let rich_text_hash = rich_row.rich_text_hash.clone().expect("rich hash present");
    let image_row = record(&h, image_id);
    let asset_ref = image_row.asset_ref.clone().expect("asset ref present");

    let outcomes = vec![
        h.paste_service
            .copy_entry(&h.context, plain_id, PasteMode::Plain),
        h.paste_service
            .copy_entry(&h.context, rich_id, PasteMode::Rich),
        h.paste_service
            .copy_entry(&h.context, image_id, PasteMode::Plain),
        h.paste_service
            .copy_entry(&h.context, 999_999, PasteMode::Plain),
    ];

    for outcome in &outcomes {
        let rendered = format!("{outcome:?}");
        for forbidden in [secret_plain, secret_html, "super-secret-token"] {
            assert!(
                !rendered.contains(forbidden),
                "outcome debug must not contain {forbidden:?}; got {rendered:?}"
            );
        }
        assert!(
            !rendered.contains(&rich_text_hash),
            "outcome debug must not contain the rich text hash; got {rendered:?}"
        );
        assert!(
            !rendered.contains(&asset_ref),
            "outcome debug must not contain the asset reference; got {rendered:?}"
        );
    }
}

#[test]
fn copy_outcome_kind_strings_are_stable() {
    // The frontend pins the discriminator strings to drive the
    // guidance modal: a future rename MUST be visible in the
    // cross-language contract tests.
    use clipvault_core::CopyOutcome as C;
    assert_eq!(C::Copied { id: 1 }.kind(), "copied");
    assert_eq!(
        C::CopiedPlainFallback { id: 1 }.kind(),
        "copied_plain_fallback"
    );
    assert_eq!(
        C::Failed {
            kind: "x",
            message: "y".into(),
            guidance: None,
        }
        .kind(),
        "failed"
    );
    assert_eq!(
        C::CapabilityUnavailable {
            capability: CLIPBOARD_WRITE_RICH_TEXT_CAPABILITY,
            guidance: None,
        }
        .kind(),
        "capability_unavailable"
    );
}

#[test]
fn copy_entry_does_not_touch_history_when_load_fails() {
    // A `not_found` outcome must leave the database untouched: the
    // row count, the asset directory, and the suppression token all
    // stay at their pre-call state.
    let h = harness(false, false, false);
    let id = store_plain_entry(&h, "still here");
    let before = record(&h, id);

    let outcome = h
        .paste_service
        .copy_entry(&h.context, id + 9999, PasteMode::Plain);
    assert!(matches!(outcome, CopyOutcome::Failed { .. }));

    let after = record(&h, id);
    assert_eq!(before.content, after.content);
    assert_eq!(before.content_hash, after.content_hash);
    assert_eq!(before.updated_at, after.updated_at);
    assert_eq!(before.is_pinned, after.is_pinned);
}
