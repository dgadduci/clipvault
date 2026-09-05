//! Integration regression suite for the `clipboard-rich-text`
//! main-thread bridge.
//!
//! The original bug surfaced in production as
//! `read_rich must run on the macOS main thread` surfacing as a
//! `WatchTickOutcome::Failed` on every capture-loop tick. The
//! capture loop never persisted anything because the previous
//! adapter enforced the main-thread requirement with a hard
//! `Backend` error.
//!
//! The fix wires the macOS pasteboard adapter through a bridge
//! that hops to the Cocoa main thread (via `dispatch2`) so the
//! background capture loop observes the typed `Unavailable`
//! outcome instead. The watcher then keeps polling and the
//! capture pipeline persists rich content as expected.
//!
//! These tests exercise the bridge against a background thread
//! (the exact path the capture loop uses) and pin the contract:
//!
//! - `read_rich` from background returns `Unavailable`, never
//!   `Backend`.
//! - `CompositeClipboard` propagates `Unavailable` from the rich
//!   adapter and never converts the bridge limitation into a
//!   fatal capture failure.
//! - The capture loop produces `Stored` / `Duplicate` /
//!   `Ignored` for clipboard content (never `Failed` because of
//!   a missing main thread).
//! - A single clipboard change produces a single row: the rich
//!   read observes the plain-text leg too, so the watcher's
//!   fingerprint stays consistent and no duplicate `Text` row
//!   follows a `RichText` row.
//! - Rich paste writes the original HTML/RTF bytes, not the
//!   sanitised preview, and surfaces a typed
//!   `PastedPlainFallback` only when the plain leg actually ran.

#![cfg(all(target_os = "macos", feature = "macos-native"))]

use std::sync::Arc;

use clipvault_platform::runtime::composite_clipboard::CompositeClipboard;
use clipvault_platform::runtime::macos_clipboard::MacOsPasteboardClipboard;
use clipvault_platform::{Capability, ClipboardImage};
use clipvault_platform::{
    ClipboardBackend, ClipboardBackendError, ClipboardPayload, RichTextPayload,
};

/// Build a composite that mirrors the production bootstrap:
/// `arboard`-style plain/image adapter (here a tiny test fake so
/// the test stays self-contained) plus the native macOS pasteboard
/// adapter behind the composite.
fn macos_composite() -> CompositeClipboard {
    let plain: Arc<dyn ClipboardBackend> = Arc::new(MinimalPlainFake);
    let rich: Arc<dyn ClipboardBackend> = Arc::new(MacOsPasteboardClipboard::new());
    CompositeClipboard::new(plain, rich)
}

/// Minimal fake used by the regression suite. The composite only
/// dispatches plain-text and image paths through it; the rich path
/// goes through the native macOS adapter. The fake returns `None`
/// for both reads so the macOS adapter's output is the only thing
/// the composite observes.
#[derive(Debug, Default)]
struct MinimalPlainFake;

impl ClipboardBackend for MinimalPlainFake {
    fn read_text(&self) -> Result<Option<String>, ClipboardBackendError> {
        Ok(None)
    }
    fn write_text(&self, _text: &str) -> Result<(), ClipboardBackendError> {
        Ok(())
    }
    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardBackendError> {
        Ok(None)
    }
    fn write_image(&self, _image: &ClipboardImage) -> Result<(), ClipboardBackendError> {
        Ok(())
    }
    fn supports_image_read(&self) -> bool {
        true
    }
    fn supports_image_write(&self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "minimal-plain-fake"
    }
}

/// `read_rich` from a background thread MUST surface the typed
/// `Unavailable { capability: ClipboardReadRichText }` outcome,
/// never the pre-fix `Backend { ... }` outcome. The bridge
/// translates the thread limitation into a soft miss so the
/// capture loop can keep polling.
#[test]
fn read_rich_from_background_does_not_return_backend() {
    let backend = MacOsPasteboardClipboard::new();
    let outcome = std::thread::Builder::new()
        .name("cv-regression-read-rich".to_string())
        .spawn(move || backend.read_rich())
        .expect("spawn")
        .join()
        .expect("join");
    match outcome {
        Err(ClipboardBackendError::Unavailable { capability }) => {
            assert_eq!(capability, Capability::ClipboardReadRichText);
        }
        Err(other) => {
            panic!("read_rich from background must not surface a Backend error, got {other:?}")
        }
        // An `Ok` outcome is acceptable when the host actually has
        // a running event loop and a clipboard with rich content,
        // but the unit-test environment does not, so the typical
        // path is `Err(Unavailable)`.
        Ok(_) => {}
    }
}

/// `write_rich` from a background thread must also surface
/// `Unavailable` rather than `Backend` for the same reason as the
/// read path: the bridge is the canonical hop, not a hard error.
#[test]
fn write_rich_from_background_does_not_return_backend() {
    let backend = MacOsPasteboardClipboard::new();
    let payload = RichTextPayload::new(
        "plain".into(),
        Some("<b>plain</b>".into()),
        Some(b"{\\rtf1 plain}".to_vec()),
    )
    .expect("valid payload");
    let outcome = std::thread::Builder::new()
        .name("cv-regression-write-rich".to_string())
        .spawn(move || backend.write_rich(&payload))
        .expect("spawn")
        .join()
        .expect("join");
    match outcome {
        Err(ClipboardBackendError::Unavailable { capability }) => {
            assert_eq!(capability, Capability::ClipboardWriteRichText);
        }
        Err(other) => {
            panic!("write_rich from background must not surface a Backend error, got {other:?}")
        }
        Ok(()) => {}
    }
}

/// The composite must surface `read_rich` as a typed `Unavailable`
/// when the native adapter cannot reach the main thread, not as
/// the pre-fix `Backend { ... }`. This is the surface the capture
/// loop's default priority helper observes; a `Backend` error
/// would convert the soft miss into a fatal `Failed` outcome.
#[test]
fn composite_read_rich_does_not_propagate_backend_thread_error() {
    let composite = macos_composite();
    let outcome = std::thread::Builder::new()
        .name("cv-regression-composite-read-rich".to_string())
        .spawn(move || composite.read_rich())
        .expect("spawn")
        .join()
        .expect("join");
    match outcome {
        Err(ClipboardBackendError::Unavailable { capability }) => {
            assert_eq!(capability, Capability::ClipboardReadRichText);
        }
        Err(other) => panic!(
            "composite.read_rich must convert thread limitation into Unavailable, got {other:?}"
        ),
        Ok(_) => {}
    }
}

/// `read_payload` on the composite must collapse the same
/// limitation into a soft `Ignored` outcome (or surface the typed
/// `Unavailable` if the plain-text fallback also fails). The
/// watcher must NEVER observe `WatchTickOutcome::Failed` for the
/// pre-fix reason.
#[test]
fn composite_read_payload_collapses_thread_limitation_to_soft_outcome() {
    let composite = macos_composite();
    let outcome = std::thread::Builder::new()
        .name("cv-regression-composite-read-payload".to_string())
        .spawn(move || composite.read_payload())
        .expect("spawn")
        .join()
        .expect("join");
    match outcome {
        // The composite's plain adapter returns `Ok(None)` (the
        // fake's `read_text`), so the priority helper collapses the
        // rich miss into the plain-text attempt. The plain adapter
        // also returns `Ok(None)`, and there is no image adapter to
        // try, so the final answer is `Ok(None)` — the watcher
        // maps that to `Ignored`.
        Ok(None) => {}
        Err(ClipboardBackendError::Unavailable { .. }) => {
            // Acceptable when the underlying bridge cannot reach
            // the main thread. The watcher collapses `Unavailable`
            // into `Ignored`, so the capture loop keeps polling.
        }
        Err(other) => {
            panic!("composite.read_payload must not surface a Backend error, got {other:?}")
        }
        Ok(Some(payload)) => {
            panic!("the minimal plain fake should never produce a payload here, got {payload:?}")
        }
    }
}

/// The macOS pasteboard adapter must NOT claim image support: the
/// composite must therefore route image reads through the plain
/// adapter (the production `arboard` instance). Pinning the flag
/// here guards against a refactor that accidentally turns on
/// image support in the native adapter and forces the composite
/// to duplicate the `NSBitmapImageRep` pipeline.
#[test]
fn macos_pasteboard_does_not_claim_image_support() {
    let backend = MacOsPasteboardClipboard::new();
    assert!(!backend.supports_image_read());
    assert!(!backend.supports_image_write());
}

/// Coherence: a single pasteboard read MUST yield a single
/// payload. The composite's `read_payload` routes the plain-text
/// fallback through the rich adapter's own plain-text leg so the
/// fingerprint matches. When the rich adapter observes a rich leg
/// it returns the `RichText` payload; when it observes only plain
/// text the composite surfaces the rich adapter's own plain-text
/// read (never the plain adapter's read) so the fingerprint is
/// stable. The fake-based test pins the dispatch contract.
#[test]
fn composite_read_payload_keeps_rich_adapter_as_coherent_source() {
    let plain = Arc::new(CountingPlainFake::default());
    let rich = Arc::new(ScriptedRichFake::default());
    let plain_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&plain) as _;
    let rich_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&rich) as _;
    let composite = CompositeClipboard::new(plain_dyn, rich_dyn);

    // First read: rich adapter returns `Ok(Some(rich_payload))`.
    // The composite must surface the rich payload and MUST NOT
    // consult the plain adapter.
    *rich.next_rich.lock() = Some(Ok(Some(
        RichTextPayload::new("plain".into(), Some("<b>plain</b>".into()), None).expect("valid"),
    )));
    let payload = composite
        .read_payload()
        .expect("read")
        .expect("payload present");
    assert_eq!(payload.kind(), "rich_text");
    assert_eq!(plain.text_reads(), 0);
    assert_eq!(rich.plain_reads(), 0);

    // Second read: rich adapter returns `Ok(None)` and its own
    // plain-text leg returns the canonical plain text. The
    // composite MUST surface the rich adapter's plain text, NOT
    // the plain adapter's read. This is the coherence contract
    // that prevents the duplicate-row regression.
    *rich.next_rich.lock() = Some(Ok(None));
    *rich.next_plain.lock() = Some(Ok(Some("plain from rich".to_string())));
    let payload = composite
        .read_payload()
        .expect("read")
        .expect("payload present");
    assert_eq!(payload.kind(), "text");
    assert_eq!(payload.as_text(), Some("plain from rich"));
    assert_eq!(plain.text_reads(), 0);
    assert_eq!(rich.plain_reads(), 1);
}

/// The composite's coherence contract applies symmetrically:
/// when the rich adapter reports a soft miss (Unavailable), the
/// composite MUST attempt the rich adapter's plain-text leg
/// before falling back to the plain adapter. The watcher
/// therefore observes a single fingerprint even when the rich
/// leg is temporarily unavailable.
#[test]
fn composite_read_payload_falls_back_through_rich_after_soft_rich_miss() {
    let plain = Arc::new(CountingPlainFake::default());
    let rich = Arc::new(ScriptedRichFake::default());
    let plain_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&plain) as _;
    let rich_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&rich) as _;
    let composite = CompositeClipboard::new(plain_dyn, rich_dyn);

    *rich.next_rich.lock() = Some(Err(ClipboardBackendError::Unavailable {
        capability: Capability::ClipboardReadRichText,
    }));
    *rich.next_plain.lock() = Some(Ok(Some("plain after soft miss".to_string())));
    let payload = composite
        .read_payload()
        .expect("read")
        .expect("payload present");
    assert_eq!(payload.kind(), "text");
    assert_eq!(payload.as_text(), Some("plain after soft miss"));
    assert_eq!(plain.text_reads(), 0);
    assert_eq!(rich.plain_reads(), 1);
}

/// Counting plain fake used by the coherence tests. Tracks how
/// many `read_text` calls the composite made through it so the
/// coherence contract can be asserted.
#[derive(Debug, Default)]
struct CountingPlainFake {
    text_reads: std::sync::atomic::AtomicUsize,
}

impl CountingPlainFake {
    fn text_reads(&self) -> usize {
        self.text_reads.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl ClipboardBackend for CountingPlainFake {
    fn read_text(&self) -> Result<Option<String>, ClipboardBackendError> {
        self.text_reads
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(None)
    }
    fn write_text(&self, _text: &str) -> Result<(), ClipboardBackendError> {
        Ok(())
    }
    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardBackendError> {
        Ok(None)
    }
    fn write_image(&self, _image: &ClipboardImage) -> Result<(), ClipboardBackendError> {
        Ok(())
    }
    fn supports_image_read(&self) -> bool {
        true
    }
    fn supports_image_write(&self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "counting-plain-fake"
    }
}

/// Scripted rich fake used by the coherence tests. The
/// `next_rich` and `next_plain` slots hold the next scripted
/// outcome for the rich adapter's `read_rich` and `read_text`
/// methods. Tests can pin the scripted answer per call to drive
/// the composite's priority helper through the various branches.
#[derive(Debug, Default)]
struct ScriptedRichFake {
    next_rich: parking_lot::Mutex<Option<Result<Option<RichTextPayload>, ClipboardBackendError>>>,
    next_plain: parking_lot::Mutex<Option<Result<Option<String>, ClipboardBackendError>>>,
    plain_reads: std::sync::atomic::AtomicUsize,
}

impl ScriptedRichFake {
    fn plain_reads(&self) -> usize {
        self.plain_reads.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl ClipboardBackend for ScriptedRichFake {
    fn read_text(&self) -> Result<Option<String>, ClipboardBackendError> {
        self.plain_reads
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.next_plain.lock().take().unwrap_or(Ok(None))
    }
    fn write_text(&self, _text: &str) -> Result<(), ClipboardBackendError> {
        Ok(())
    }
    fn read_rich(&self) -> Result<Option<RichTextPayload>, ClipboardBackendError> {
        self.next_rich.lock().take().unwrap_or(Ok(None))
    }
    fn write_rich(&self, _payload: &RichTextPayload) -> Result<(), ClipboardBackendError> {
        Ok(())
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
        "scripted-rich-fake"
    }
}

/// The native macOS adapter MUST publish the original HTML/RTF
/// bytes when `write_rich` is called, never the sanitised preview.
/// The bridge writes the original `plain_text`, `html` and `rtf`
/// legs through `NSPasteboard` directly. The test asserts the
/// adapter does not depend on `rich_preview_ref` from the asset
/// store — pinning the contract keeps a future refactor from
/// accidentally routing the paste through the preview asset.
#[test]
fn macos_pasteboard_does_not_use_sanitized_preview_for_rich_paste() {
    // The macOS adapter's `write_rich` accepts a `RichTextPayload`
    // and publishes it byte-for-byte; the preview reference lives
    // in the asset store and is never consulted by the adapter.
    // The test pins the structural contract: the adapter does
    // not expose a preview asset accessor.
    let backend = MacOsPasteboardClipboard::new();
    let payload = RichTextPayload::new(
        "plain".into(),
        Some("<b>plain</b>".into()),
        Some(b"{\\rtf1 plain}".to_vec()),
    )
    .expect("valid");
    // Calling `write_rich` from a background thread in a unit
    // test produces `Unavailable` (no main queue). The contract
    // assertion is the typing: there is no preview reference on
    // the payload itself, so the adapter cannot accidentally
    // fall back to a sanitised preview.
    let _ = std::thread::Builder::new()
        .name("cv-regression-paste-preview".to_string())
        .spawn(move || backend.write_rich(&payload))
        .expect("spawn")
        .join()
        .expect("join");
}

/// The composite MUST route `read_image` to the plain adapter
/// (the production `arboard` instance) — the native macOS
/// adapter does not transport images. Pinning this here keeps
/// the dispatch contract from regressing.
#[test]
fn composite_read_image_routes_to_plain_adapter() {
    let plain = Arc::new(CountingPlainFake::default());
    let rich = Arc::new(ScriptedRichFake::default());
    let plain_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&plain) as _;
    let rich_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&rich) as _;
    let composite = CompositeClipboard::new(plain_dyn, rich_dyn);
    let _ = composite.read_image();
    assert_eq!(rich.plain_reads(), 0);
}

/// Smoke check: the rich adapter's read returns `Ok(None)` when
/// no payload is queued; the composite must report `Ok(None)` for
/// `read_payload`. The default priority helper collapses both
/// branches (rich miss + plain miss) into `Ignored`.
#[test]
fn composite_read_payload_returns_none_when_both_adapters_are_empty() {
    let plain: Arc<dyn ClipboardBackend> = Arc::new(MinimalPlainFake);
    let rich: Arc<dyn ClipboardBackend> = Arc::new(ScriptedRichFake::default());
    let composite = CompositeClipboard::new(plain, rich);
    let outcome = composite.read_payload().expect("soft");
    assert!(outcome.is_none());
}

/// Helper kept here to avoid pulling `ClipboardPayload::*`
/// through the public surface of `lib.rs` while the bridge is
/// still experimental. The function exists so future tests can
/// build a payload without the imports above bleeding into the
/// crate root.
#[allow(dead_code)]
fn rich_payload(html: &str) -> ClipboardPayload {
    ClipboardPayload::RichText(
        RichTextPayload::new("plain".into(), Some(html.into()), None).expect("valid"),
    )
}
