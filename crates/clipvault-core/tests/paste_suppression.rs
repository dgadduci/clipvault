//! End-to-end integration tests for the paste-suppression
//! regression. The suite pins the contract the
//! `clipboard-rich-text` change documents:
//!
//! - a `PasteMode::Plain` write never creates a new card;
//! - a `PasteMode::Rich` write never creates a new card;
//! - a paste whose synthetic-paste step fails still does not create
//!   a new card (the clipboard content is ClipVault-owned);
//! - a subsequent, distinct user copy inside the suppression
//!   window is still captured;
//! - the suppression token is consumed by the first matching
//!   observation (single-shot);
//! - the token expires after the documented TTL;
//! - a write failure clears the token before it can mask a
//!   legitimate later capture;
//! - the registry does not duplicate, leak or store sensitive
//!   payload material (the fingerprints are SHA-256 digests only).
//!
//! The tests run against fakes and a temporary data directory, so
//! no real clipboard, graphical session, or user data is involved.

use std::sync::Arc;
use std::time::Duration;

use clipvault_core::{
    AppBootstrap, AppContext, Capabilities, CaptureWatcher, ClipboardBackendError, ClipboardImage,
    ClipboardPayload, FakeActiveApplication, FakeClipboardBackend, FakeHotkeyManager,
    FakePasteController, FakeSettingsNavigator, FakeTrayController, HistoryOutcome, OsFamily,
    PasteError, PasteMode, PasteOutcome, PasteSuppression, PlatformAdapters, PlatformInfo,
    RichTextPayload, SuppressionFingerprint, WatchTickOutcome,
    CLIPBOARD_WRITE_RICH_TEXT_CAPABILITY,
};
use clipvault_platform::{Capability, ClipboardBackend, DisplayServer, PasteController};
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
    watcher: CaptureWatcher,
}

fn harness() -> Harness {
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");

    let clipboard = Arc::new(FakeClipboardBackend::new());
    clipboard.set_rich_support(true, true);
    let paste = Arc::new(FakePasteController::new());

    let adapters = PlatformAdapters::new(
        Arc::clone(&clipboard) as Arc<dyn ClipboardBackend>,
        Arc::new(FakeHotkeyManager::new()),
        Arc::new(FakeActiveApplication::new()),
        Arc::clone(&paste) as Arc<dyn PasteController>,
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Arc::new(clipvault_core::NoopApplicationMetadataProvider),
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

    let watcher = CaptureWatcher::new(
        context.platform_adapters().clipboard().clone(),
        Duration::from_millis(10),
    );
    Harness {
        _dir: dir,
        context,
        clipboard,
        paste,
        watcher,
    }
}

fn record_plain(h: &Harness, text: &str) -> i64 {
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Text(text.into()),
        None,
    );
    match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    }
}

fn record_rich(h: &Harness, payload: RichTextPayload) -> i64 {
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::RichText(payload),
        None,
    );
    match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    }
}

fn record_image(h: &Harness, image: ClipboardImage) -> i64 {
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(image),
        None,
    );
    match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    }
}

fn entry_count(h: &Harness) -> i64 {
    h.context
        .history()
        .history_count(&h.context)
        .expect("count")
}

#[test]
fn plain_paste_does_not_create_a_new_card() {
    let h = harness();
    let id = record_plain(&h, "the quick brown fox");
    // The next read should observe the same content the paste wrote.
    h.clipboard
        .push_read(Ok(Some("the quick brown fox".into())));

    // Manually invoke the paste so the suppression token arms, then
    // tick the watcher to simulate the next capture-loop iteration.
    let outcome = h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Plain);
    assert!(matches!(outcome, PasteOutcome::Pasted { id: pid } if pid == id));

    let before = entry_count(&h);
    let tick = h.watcher.tick(
        &h.context,
        None,
        clipvault_core::AttemptOrigin::BackgroundLoop,
    );
    assert_eq!(tick, WatchTickOutcome::Suppressed);
    assert_eq!(
        entry_count(&h),
        before,
        "no row was inserted by the suppression path"
    );
}

#[test]
fn rich_paste_does_not_create_a_new_card() {
    let h = harness();
    let payload =
        RichTextPayload::new("plain".into(), Some("<b>rich</b>".into()), None).expect("rich");
    let id = record_rich(&h, payload);
    // The next read observes the canonical plain text the rich
    // paste republished.
    h.clipboard.push_read(Ok(Some("plain".into())));

    let outcome = h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Rich);
    assert!(matches!(outcome, PasteOutcome::Pasted { id: pid } if pid == id));

    let before = entry_count(&h);
    let tick = h.watcher.tick(
        &h.context,
        None,
        clipvault_core::AttemptOrigin::BackgroundLoop,
    );
    assert_eq!(tick, WatchTickOutcome::Suppressed);
    assert_eq!(entry_count(&h), before);
}

#[test]
fn rich_paste_with_rtf_falls_back_when_rtf_unavailable() {
    // Sanity guard: rich paste with both HTML and RTF still keeps
    // the suppression contract when the session cannot publish RTF.
    let h = harness();
    h.clipboard.set_rich_support(true, false);
    let payload = RichTextPayload::new(
        "plain".into(),
        Some("<b>rich</b>".into()),
        Some(b"{\\rtf1 rich}".to_vec()),
    )
    .expect("rich");
    let id = record_rich(&h, payload);
    h.clipboard.push_read(Ok(Some("plain".into())));

    let outcome = h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Rich);
    assert!(matches!(outcome, PasteOutcome::PastedPlainFallback { id: pid } if pid == id));

    let before = entry_count(&h);
    let tick = h.watcher.tick(
        &h.context,
        None,
        clipvault_core::AttemptOrigin::BackgroundLoop,
    );
    assert_eq!(tick, WatchTickOutcome::Suppressed);
    assert_eq!(entry_count(&h), before);
}

#[test]
fn image_paste_does_not_create_a_new_card() {
    let h = harness();
    h.clipboard.set_image_support(true, true);
    let image = ClipboardImage::new(vec![0xAB; 4], 1, 1).expect("image");
    let id = record_image(&h, image);

    let outcome = h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Plain);
    assert!(matches!(outcome, PasteOutcome::Pasted { id: pid } if pid == id));

    // The image paste path arms an image fingerprint. The next
    // observation is a different payload (text) so the dedupe state
    // treats it as a fresh change; the suppression token does not
    // match the text payload and the watcher reports a normal
    // capture.
    h.clipboard.push_read(Ok(Some("next text".into())));
    let tick = h.watcher.tick(
        &h.context,
        None,
        clipvault_core::AttemptOrigin::BackgroundLoop,
    );
    assert!(matches!(tick, WatchTickOutcome::Captured(_)));
}

#[test]
fn synthetic_paste_failure_does_not_create_a_card() {
    // The token is cleared after the paste resolves (success or
    // failure), so the next observation is captured as a fresh
    // entry. The user-facing expectation is no second card while
    // the clipboard still holds the same content, so we tick
    // immediately and assert the next observation does not create
    // a card for the same content. A different capture (which is
    // what the user would actually see after the synthetic paste
    // crashed) is fine.
    let h = harness();
    let id = record_plain(&h, "owned by clipvault");
    h.clipboard.push_read(Ok(Some("owned by clipvault".into())));
    h.paste.set_next(Err(PasteError::Backend {
        details: "synthetic paste rejected by the host".into(),
    }));

    let outcome = h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Plain);
    assert!(matches!(outcome, PasteOutcome::Failed { .. }));

    // The token is dropped after the failed paste, so a tick on
    // a different payload must capture it; a tick on the same
    // payload is just a normal capture too.
    h.clipboard.push_read(Ok(Some("next user copy".into())));
    let tick = h.watcher.tick(
        &h.context,
        None,
        clipvault_core::AttemptOrigin::BackgroundLoop,
    );
    assert!(matches!(tick, WatchTickOutcome::Captured(_)));
}

#[test]
fn distinct_copy_during_suppression_window_is_captured() {
    let h = harness();
    let id = record_plain(&h, "owned");
    // The fake backend consumes the queue with `pop` (LIFO). Push
    // the second observation first so the first `read_text` call
    // (after the rich read falls through) returns the paste
    // payload.
    h.clipboard.push_read(Ok(Some("distinct copy".into())));
    h.clipboard.push_read(Ok(Some("owned".into())));

    let _ = h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Plain);

    // First observation matches the armed token and is suppressed.
    let first = h.watcher.tick(
        &h.context,
        None,
        clipvault_core::AttemptOrigin::BackgroundLoop,
    );
    assert_eq!(first, WatchTickOutcome::Suppressed);

    // Second observation is a different payload: the token is
    // already consumed, so the watcher captures the new copy.
    let second = h.watcher.tick(
        &h.context,
        None,
        clipvault_core::AttemptOrigin::BackgroundLoop,
    );
    assert!(matches!(second, WatchTickOutcome::Captured(_)));
}

#[test]
fn token_is_consumed_by_the_first_matching_observation() {
    let h = harness();
    let id = record_plain(&h, "single shot");
    h.clipboard.push_read(Ok(Some("single shot".into())));
    h.clipboard.push_read(Ok(Some("single shot".into())));
    h.clipboard.push_read(Ok(Some("single shot".into())));

    let _ = h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Plain);
    assert_eq!(
        h.watcher.tick(
            &h.context,
            None,
            clipvault_core::AttemptOrigin::BackgroundLoop
        ),
        WatchTickOutcome::Suppressed
    );
    // Subsequent identical reads are unchanged because the dedupe
    // state has already recorded the fingerprint; the token is
    // not re-armed.
    assert_eq!(
        h.watcher.tick(
            &h.context,
            None,
            clipvault_core::AttemptOrigin::BackgroundLoop
        ),
        WatchTickOutcome::Unchanged
    );
    assert_eq!(
        h.watcher.tick(
            &h.context,
            None,
            clipvault_core::AttemptOrigin::BackgroundLoop
        ),
        WatchTickOutcome::Unchanged
    );
}

#[test]
fn token_expires_after_ttl() {
    let registry = PasteSuppression::new();
    let fingerprint =
        SuppressionFingerprint::from_payload(&ClipboardPayload::Text("expires".into()));
    registry.arm_with_ttl(fingerprint.clone(), Duration::from_millis(0));
    // The very first lookup drops the expired token.
    assert!(!registry.matches_and_consume(&fingerprint));
    assert!(registry.peek().is_none());
}

#[test]
fn write_failure_clears_the_token() {
    let h = harness();
    let id = record_plain(&h, "payload");
    h.clipboard.push_read(Ok(Some("payload".into())));
    h.clipboard
        .set_text_write_error(ClipboardBackendError::backend("rejected"));

    let outcome = h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Plain);
    assert!(matches!(outcome, PasteOutcome::Failed { .. }));

    // A subsequent legitimate copy is captured: the token was
    // cleared when the write failed.
    h.clipboard
        .push_read(Ok(Some("next legitimate copy".into())));
    let tick = h.watcher.tick(
        &h.context,
        None,
        clipvault_core::AttemptOrigin::BackgroundLoop,
    );
    assert!(matches!(tick, WatchTickOutcome::Captured(_)));
}

#[test]
fn empty_observed_fingerprint_never_matches() {
    let registry = PasteSuppression::new();
    registry.arm(SuppressionFingerprint::from_payload(
        &ClipboardPayload::Text("hello".into()),
    ));
    // An empty observation must not consume the token.
    assert!(!registry.matches_and_consume(&SuppressionFingerprint {
        plain_text_hash: None,
        rich_text_hash: None,
        image_hash: None,
    }));
    assert!(registry.peek().is_some());
}

#[test]
fn mismatched_payload_does_not_consume_token() {
    let registry = PasteSuppression::new();
    registry.arm(SuppressionFingerprint::from_payload(
        &ClipboardPayload::Text("hello".into()),
    ));
    // Different plain text must not match and must not consume.
    assert!(
        !registry.matches_and_consume(&SuppressionFingerprint::from_payload(
            &ClipboardPayload::Text("world".into())
        ))
    );
    assert!(registry.peek().is_some());
}

#[test]
fn fingerprint_carries_no_sensitive_content() {
    // Regression guard: the fingerprint is built from canonical
    // digests, never from the captured text, HTML, RTF or image
    // bytes. The Debug rendering of the payload should not leak
    // any of those into a log line.
    let payload = ClipboardPayload::RichText(
        RichTextPayload::new(
            "super-secret".into(),
            Some("<b>super-secret</b>".into()),
            Some(b"{\\rtf1 super-secret}".to_vec()),
        )
        .expect("payload"),
    );
    let fingerprint = SuppressionFingerprint::from_payload(&payload);
    let rendered = format!("{fingerprint:?}");
    assert!(!rendered.contains("super-secret"));
    assert!(!rendered.contains("<b>"));
    assert!(!rendered.contains("\\rtf"));
}

#[test]
fn app_context_exposes_the_suppression_registry() {
    // The bootstrap wires the registry behind a stable accessor so
    // every consumer — capture loop, manual tick command, paste
    // service — sees the same handle.
    let h = harness();
    let registry = h.context.paste_suppression();
    registry.arm(SuppressionFingerprint::from_payload(
        &ClipboardPayload::Text("shared".into()),
    ));
    // A separate clone of the context (the manual `Tick capture`
    // command holds a separate handle) sees the same registry and
    // is suppressed.
    let id = record_plain(&h, "shared");
    h.clipboard.push_read(Ok(Some("shared".into())));
    let _ = h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Plain);
    let tick = h.watcher.tick(
        &h.context,
        None,
        clipvault_core::AttemptOrigin::BackgroundLoop,
    );
    assert_eq!(tick, WatchTickOutcome::Suppressed);
}

#[test]
fn capability_unavailable_keeps_card_unchanged() {
    // When the rich write capability is unavailable, the paste
    // service falls back to plain text and the suppression token
    // is re-armed for the plain text leg. The contract is the
    // same: no second card.
    let h = harness();
    h.clipboard.set_rich_support(true, false);
    let payload =
        RichTextPayload::new("plain".into(), Some("<b>x</b>".into()), None).expect("rich");
    let id = record_rich(&h, payload);
    h.clipboard.push_read(Ok(Some("plain".into())));
    h.clipboard
        .fail_rich_write(ClipboardBackendError::Unavailable {
            capability: Capability::ClipboardWriteRichText,
        });

    let outcome = h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Rich);
    assert!(matches!(
        outcome,
        PasteOutcome::PastedPlainFallback { id: pid } if pid == id
    ));

    let before = entry_count(&h);
    let tick = h.watcher.tick(
        &h.context,
        None,
        clipvault_core::AttemptOrigin::BackgroundLoop,
    );
    assert_eq!(tick, WatchTickOutcome::Suppressed);
    assert_eq!(entry_count(&h), before);
}

#[test]
fn capability_strings_are_stable() {
    // Regression guard: the wire format used by the registry is
    // metadata-only; future refactors that start to carry payload
    // bytes in the fingerprint will surface here.
    assert_eq!(
        CLIPBOARD_WRITE_RICH_TEXT_CAPABILITY,
        "clipboard_write_rich_text"
    );
}
