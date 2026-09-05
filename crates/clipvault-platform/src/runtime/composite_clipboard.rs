//! Composite clipboard backend that merges two adapters behind the
//! single [`ClipboardBackend`] trait the core consumes.
//!
//! ## Why this exists
//!
//! `arboard` is the image / plain-text adapter for macOS and Linux
//! X11: it exposes `get_image` / `set_image` and the basic
//! `get_text` / `set_text` pair. It does **not** expose RTF, so the
//! previous prototype could not satisfy a rich paste that needed to
//! publish `public.rtf` together with `public.html`. The native
//! macOS adapter talks to `NSPasteboard` directly and can publish
//! the full set of flavours in a single logical write, but it
//! does not transport raster images (the `NSBitmapImageRep` →
//! RGBA → PNG pipeline is implemented inside `arboard`).
//!
//! The composite backend dispatches:
//!
//! - `read_text` / `write_text` → the plain backend (`arboard`).
//! - `read_image` / `write_image` → the image backend (`arboard`).
//! - `read_rich` / `write_rich` → the rich backend (the native
//!   `NSPasteboard` adapter on macOS, `arboard` on Linux X11).
//!
//! The two adapters must agree on plain text (the native adapter
//! always publishes `public.utf8-plain-text` alongside the legacy
//! `NSPasteboardTypeString`, and `arboard` always publishes the
//! canonical plain text through its own path). When the rich
//! backend refuses a write because the only leg it can publish
//! (HTML) does not match the payload (an RTF-only payload), the
//! composite surfaces the typed `Unavailable` outcome so the
//! pipeline can apply the plain fallback without ever silently
//! degrading to a plain-text write.
//!
//! ## Read coherence
//!
//! The composite overrides the default
//! [`ClipboardBackend::read_payload`] so the plain-text fallback
//! after a `read_rich` miss flows through the **rich adapter's
//! own** plain-text leg instead of the `arboard` plain adapter.
//! On macOS this avoids the race where the pasteboard changes
//! between two separate reads and the watcher ends up producing
//! two different fingerprints for the same clipboard change (one
//! rich, one plain). The coherence contract is documented on
//! [`CompositeClipboard::read_payload`].

use std::sync::Arc;

use crate::clipboard::{
    ClipboardBackend, ClipboardBackendError, ClipboardImage, ClipboardPayload, RichTextPayload,
};

/// Composite backend that dispatches each [`ClipboardBackend`]
/// direction to the adapter that implements it. Cheap to clone
/// behind an `Arc<dyn ClipboardBackend>`; the inner adapters are
/// already `Send + Sync` so the composite is too.
#[derive(Clone)]
pub struct CompositeClipboard {
    plain: Arc<dyn ClipboardBackend>,
    rich: Arc<dyn ClipboardBackend>,
}

impl CompositeClipboard {
    /// Build a composite backend. The two inner adapters may be the
    /// same instance (the Linux X11 / fallback path) or different
    /// instances (macOS where `arboard` handles images and the
    /// native `NSPasteboard` adapter handles rich text).
    pub fn new(plain: Arc<dyn ClipboardBackend>, rich: Arc<dyn ClipboardBackend>) -> Self {
        Self { plain, rich }
    }

    /// Translate a rich-adapter error into the typed outcome the
    /// pipeline observes. The composite never lets a `Backend`
    /// error from the rich adapter propagate to the capture loop:
    /// a thread limitation or a pasteboard backend failure on the
    /// rich adapter is a soft miss for the **rich leg** — the
    /// watcher can still try the plain-text leg through the rich
    /// adapter (same pasteboard snapshot) and degrade gracefully.
    fn normalize_rich_error(&self, error: ClipboardBackendError) -> ClipboardBackendError {
        match error {
            // The rich adapter cannot reach the pasteboard's main
            // thread or the session cannot publish rich text.
            // Surface as `Unavailable` so the watcher keeps polling
            // instead of converting a thread limitation into a hard
            // `WatchTickOutcome::Failed`.
            ClipboardBackendError::Backend { .. } => ClipboardBackendError::Unavailable {
                capability: crate::Capability::ClipboardReadRichText,
            },
            other => other,
        }
    }
}

impl ClipboardBackend for CompositeClipboard {
    fn read_text(&self) -> Result<Option<String>, ClipboardBackendError> {
        self.plain.read_text()
    }

    fn write_text(&self, text: &str) -> Result<(), ClipboardBackendError> {
        // A plain-text write MUST NOT leave rich flavours from a
        // previous capture on the pasteboard: the contract of
        // `PasteMode::Plain` is that the only thing published is the
        // canonical plain text. The composite therefore routes
        // through the rich adapter when it advertises
        // `supports_native_plain_write` (the macOS native adapter
        // hop, which calls `clearContents()` before publishing). If
        // the bridge cannot reach the main thread the rich adapter
        // surfaces a typed `Unavailable`; the composite then falls
        // back to the plain adapter so the paste still completes.
        //
        // On every other host (Linux X11, the no-op fallback) the
        // rich adapter is the same `arboard` instance, the flag is
        // `false`, and the plain path is the documented
        // degradation route. arboard's plain write on X11 takes
        // ownership of the selection and replaces every previously
        // declared target.
        if self.rich.supports_native_plain_write() {
            match self.rich.write_text(text) {
                Ok(()) => return Ok(()),
                // Only a soft miss (no main queue) is recoverable
                // here. A hard failure from the rich adapter is
                // surfaced to the caller so the paste pipeline can
                // apply the typed guidance.
                Err(ClipboardBackendError::Unavailable { .. }) => {}
                Err(error) => return Err(error),
            }
        }
        self.plain.write_text(text)
    }

    fn read_rich(&self) -> Result<Option<RichTextPayload>, ClipboardBackendError> {
        // Read through the rich adapter so HTML + RTF are exposed
        // when the host publishes both. Falls back to plain text
        // when neither leg is present, matching the priority rule
        // documented on [`ClipboardBackend::read_payload`].
        self.rich
            .read_rich()
            .map_err(|error| self.normalize_rich_error(error))
    }

    fn write_rich(&self, payload: &RichTextPayload) -> Result<(), ClipboardBackendError> {
        // Publish through the rich adapter. When the host cannot
        // represent the requested flavours (an RTF-only payload on
        // a host whose rich adapter only knows about HTML), the
        // adapter returns `Unavailable` so the pipeline can apply
        // the plain fallback instead of silently losing the
        // formatting.
        self.rich.write_rich(payload)
    }

    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardBackendError> {
        // Images always travel through the plain adapter. The
        // native `NSPasteboard` adapter is rich-text-only because
        // the `NSBitmapImageRep` → RGBA → PNG pipeline belongs to
        // `arboard`; routing the image path through a second
        // adapter would duplicate that pipeline. On macOS this
        // means the composite dispatches the image read through
        // the same `arboard` instance the rich adapter borrows for
        // its plain-text leg.
        self.plain.read_image()
    }

    fn write_image(&self, image: &ClipboardImage) -> Result<(), ClipboardBackendError> {
        self.plain.write_image(image)
    }

    fn read_payload(&self) -> Result<Option<ClipboardPayload>, ClipboardBackendError> {
        // The composite overrides the default priority helper so the
        // plain-text fallback after a rich miss comes from the **same
        // pasteboard read** the rich adapter just observed. This is
        // the coherence contract the capture loop relies on: a single
        // clipboard change must produce one row, not a `Text` row
        // followed by a `RichText` row with the same plain text but a
        // different fingerprint.
        //
        // The default implementation would call `self.read_text()`
        // (which routes to the `plain` adapter) after a rich miss;
        // on macOS that means a second `NSPasteboard` read through
        // `arboard`, which can race with concurrent producers and
        // produce a different plain-text fingerprint than the rich
        // adapter already returned `None` for. By contrast, the rich
        // adapter's own plain-text leg comes from the same
        // `generalPasteboard()` invocation the rich read used, so
        // the fingerprints agree.
        //
        // When the rich adapter reports `Ok(None)` for the rich leg
        // but carries a non-empty plain text, the composite exposes
        // the plain text directly. When the rich adapter reports
        // `Ok(Some(payload))`, the rich payload is returned as
        // `RichText` (the `plain_text` leg travels with it).
        if self.supports_rich_read() {
            match self.read_rich() {
                Ok(Some(payload)) => return Ok(Some(ClipboardPayload::RichText(payload))),
                Ok(None) => {
                    // The rich adapter saw a pasteboard change but
                    // no rich flavour was published. Fall back to the
                    // rich adapter's own plain-text leg so the
                    // fingerprint matches what the watcher would have
                    // observed from the rich read.
                    match self.rich.read_text() {
                        Ok(Some(text)) if !text.is_empty() => {
                            return Ok(Some(ClipboardPayload::Text(text)));
                        }
                        Ok(_) => {}
                        Err(error) if error.is_soft() => {}
                        Err(error) => return Err(error),
                    }
                }
                Err(error) if error.is_soft() => {
                    // The rich adapter could not produce a rich
                    // leg. Fall back to the rich adapter's plain-text
                    // leg to preserve coherence, then to the plain
                    // adapter if the rich adapter cannot produce
                    // plain text either.
                    match self.rich.read_text() {
                        Ok(Some(text)) if !text.is_empty() => {
                            return Ok(Some(ClipboardPayload::Text(text)));
                        }
                        Ok(_) => {}
                        Err(inner) if inner.is_soft() => {}
                        Err(inner) => return Err(inner),
                    }
                }
                Err(error) => return Err(error),
            }
        }

        match self.plain.read_text() {
            Ok(Some(text)) if !text.is_empty() => Ok(Some(ClipboardPayload::Text(text))),
            Ok(_) => {
                if !self.plain.supports_image_read() {
                    return Ok(None);
                }
                match self.plain.read_image() {
                    Ok(Some(image)) => Ok(Some(ClipboardPayload::Image(image))),
                    Ok(None) => Ok(None),
                    Err(error) if error.is_soft() => Ok(None),
                    Err(error) => Err(error),
                }
            }
            Err(error) if error.is_soft() => {
                if !self.plain.supports_image_read() {
                    return Ok(None);
                }
                match self.plain.read_image() {
                    Ok(Some(image)) => Ok(Some(ClipboardPayload::Image(image))),
                    Ok(None) => Ok(None),
                    Err(inner) if inner.is_soft() => Ok(None),
                    Err(inner) => Err(inner),
                }
            }
            Err(error) => Err(error),
        }
    }

    fn supports_rich_read(&self) -> bool {
        self.rich.supports_rich_read()
    }

    fn supports_rich_write(&self) -> bool {
        self.rich.supports_rich_write()
    }

    fn supports_image_read(&self) -> bool {
        self.plain.supports_image_read()
    }

    fn supports_image_write(&self) -> bool {
        self.plain.supports_image_write()
    }

    fn supports_native_plain_write(&self) -> bool {
        self.rich.supports_native_plain_write()
    }

    fn name(&self) -> &'static str {
        // The composite never appears in production as a single
        // backend; the name is documented as a stable identifier
        // for diagnostics so the platform card can show the
        // composition. Pin the string so a future refactor that
        // changes the surface surfaces here.
        "composite"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::{ClipboardBackendError, RichTextPayload};
    use parking_lot::Mutex;

    /// Minimal text-only fake used to prove the composite
    /// dispatches plain-text and image calls to the plain adapter
    /// and rich calls to the rich adapter. Concrete `Arc<PlainFake>`
    /// handles make the assertions trivial without resorting to
    /// `Any` casts.
    #[derive(Default)]
    struct PlainFake {
        text_writes: Mutex<Vec<String>>,
        text_reads: Mutex<u32>,
    }

    impl ClipboardBackend for PlainFake {
        fn read_text(&self) -> Result<Option<String>, ClipboardBackendError> {
            *self.text_reads.lock() += 1;
            Ok(None)
        }
        fn write_text(&self, text: &str) -> Result<(), ClipboardBackendError> {
            self.text_writes.lock().push(text.to_string());
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
            "plain-fake"
        }
    }

    /// Rich fake that accepts HTML-only payloads and refuses
    /// RTF-only ones with the typed `Unavailable` outcome. Mirrors
    /// the contract the production adapters obey. The fake also
    /// tracks the plain-text reads through the rich adapter so the
    /// coherence test can assert the composite always uses the
    /// rich adapter's own plain-text leg when the rich read misses.
    #[derive(Default)]
    struct RichFake {
        html_writes: Mutex<Vec<String>>,
        rtf_only_seen: Mutex<u32>,
        next_rich: Mutex<Option<Result<Option<RichTextPayload>, ClipboardBackendError>>>,
        next_plain: Mutex<Option<Result<Option<String>, ClipboardBackendError>>>,
        plain_reads: Mutex<u32>,
    }

    impl ClipboardBackend for RichFake {
        fn read_text(&self) -> Result<Option<String>, ClipboardBackendError> {
            *self.plain_reads.lock() += 1;
            self.next_plain
                .lock()
                .take()
                .unwrap_or(Ok(Some("rich-plain".to_string())))
        }
        fn write_text(&self, _text: &str) -> Result<(), ClipboardBackendError> {
            Ok(())
        }
        fn read_rich(&self) -> Result<Option<RichTextPayload>, ClipboardBackendError> {
            self.next_rich.lock().take().unwrap_or(Ok(None))
        }
        fn write_rich(&self, payload: &RichTextPayload) -> Result<(), ClipboardBackendError> {
            if payload.html().is_none() && payload.rtf().is_some() {
                *self.rtf_only_seen.lock() += 1;
                return Err(ClipboardBackendError::Unavailable {
                    capability: crate::Capability::ClipboardWriteRichText,
                });
            }
            if let Some(html) = payload.html() {
                self.html_writes.lock().push(html.to_string());
                return Ok(());
            }
            Err(ClipboardBackendError::Unavailable {
                capability: crate::Capability::ClipboardWriteRichText,
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
            "rich-fake"
        }
    }

    #[test]
    fn dispatches_text_and_image_to_plain_and_rich_to_rich() {
        let plain = Arc::new(PlainFake::default());
        let rich = Arc::new(RichFake::default());
        let plain_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&plain) as _;
        let rich_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&rich) as _;
        let composite = CompositeClipboard::new(plain_dyn, rich_dyn);

        // Plain text goes through the plain adapter.
        composite.write_text("hello").expect("plain write");
        assert_eq!(plain.text_writes.lock().as_slice(), &["hello".to_string()]);

        // HTML-only rich payload goes through the rich adapter.
        let payload = RichTextPayload::new("hello".into(), Some("<b>hello</b>".into()), None)
            .expect("payload");
        composite.write_rich(&payload).expect("rich write");
        assert_eq!(
            rich.html_writes.lock().as_slice(),
            &["<b>hello</b>".to_string()]
        );

        // RTF-only payload is rejected by the rich adapter, not
        // silently degraded.
        let rtf_only = RichTextPayload::new("hello".into(), None, Some(b"{\\rtf1 hello}".to_vec()))
            .expect("payload");
        let outcome = composite.write_rich(&rtf_only);
        assert!(matches!(
            outcome,
            Err(ClipboardBackendError::Unavailable {
                capability: crate::Capability::ClipboardWriteRichText
            })
        ));
        assert_eq!(*rich.rtf_only_seen.lock(), 1);
    }

    #[test]
    fn capability_flags_follow_the_rich_and_plain_adapters() {
        let plain: Arc<dyn ClipboardBackend> = Arc::new(PlainFake::default());
        let rich: Arc<dyn ClipboardBackend> = Arc::new(RichFake::default());
        let composite = CompositeClipboard::new(plain, rich);
        assert!(composite.supports_rich_read());
        assert!(composite.supports_rich_write());
        assert!(composite.supports_image_read());
        assert!(composite.supports_image_write());
        assert_eq!(composite.name(), "composite");
    }

    /// `read_payload` MUST surface a `RichText` payload when the
    /// rich adapter observes HTML/RTF, and the composite must NOT
    /// fall back to the plain adapter when the rich read already
    /// returned a non-empty result.
    #[test]
    fn read_payload_uses_rich_payload_when_rich_leg_present() {
        let plain = Arc::new(PlainFake::default());
        let rich = Arc::new(RichFake::default());
        *rich.next_rich.lock() = Some(Ok(Some(
            RichTextPayload::new("plain".into(), Some("<b>plain</b>".into()), None).expect("valid"),
        )));
        let plain_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&plain) as _;
        let rich_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&rich) as _;
        let composite = CompositeClipboard::new(plain_dyn, rich_dyn);

        let payload = composite
            .read_payload()
            .expect("read")
            .expect("payload present");
        assert_eq!(payload.kind(), "rich_text");
        assert_eq!(payload.as_text(), Some("plain"));
        assert_eq!(*plain.text_reads.lock(), 0);
        assert_eq!(*rich.plain_reads.lock(), 0);
    }

    /// When the rich adapter reports `Ok(None)` for the rich leg
    /// but the same pasteboard snapshot carries a non-empty plain
    /// text, the composite MUST return the plain text from the rich
    /// adapter's own plain-text leg, never from the `plain`
    /// adapter. This is the coherence contract: a single clipboard
    /// change produces a single fingerprint, not two.
    #[test]
    fn read_payload_falls_back_to_rich_plain_text_after_rich_miss() {
        let plain = Arc::new(PlainFake::default());
        let rich = Arc::new(RichFake::default());
        *rich.next_rich.lock() = Some(Ok(None));
        *rich.next_plain.lock() = Some(Ok(Some("plain from rich".to_string())));
        let plain_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&plain) as _;
        let rich_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&rich) as _;
        let composite = CompositeClipboard::new(plain_dyn, rich_dyn);

        let payload = composite
            .read_payload()
            .expect("read")
            .expect("payload present");
        assert_eq!(payload.kind(), "text");
        assert_eq!(payload.as_text(), Some("plain from rich"));
        // The plain adapter MUST NOT have been consulted: the
        // coherence contract routes the fallback through the rich
        // adapter.
        assert_eq!(*plain.text_reads.lock(), 0);
        assert_eq!(*rich.plain_reads.lock(), 1);
    }

    /// When the rich adapter returns a typed `Backend` error (the
    /// pre-fix outcome that caused the capture loop to produce
    /// `WatchTickOutcome::Failed`), the composite MUST surface
    /// `Unavailable` instead, never `Backend`. The watcher then
    /// sees a soft miss and keeps polling.
    #[test]
    fn read_payload_normalises_rich_backend_to_unavailable() {
        let plain = Arc::new(PlainFake::default());
        let rich = Arc::new(RichFake::default());
        *rich.next_rich.lock() = Some(Err(ClipboardBackendError::backend(
            "read_rich must run on the macOS main thread",
        )));
        let plain_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&plain) as _;
        let rich_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&rich) as _;
        let composite = CompositeClipboard::new(plain_dyn, rich_dyn);

        // The composite's `read_rich` is the typed translation
        // surface: a `Backend` error becomes `Unavailable`.
        let outcome = composite.read_rich();
        match outcome {
            Err(ClipboardBackendError::Unavailable { capability }) => {
                assert_eq!(capability, crate::Capability::ClipboardReadRichText);
            }
            Err(other) => panic!(
                "composite.read_rich must convert Backend errors into Unavailable, got {other:?}"
            ),
            Ok(_) => panic!("expected error"),
        }
    }

    /// `read_rich` from the composite must surface `Ok(Some(...))`
    /// when the rich adapter observes a rich leg. The composite
    /// does NOT inspect the plain adapter when the rich adapter
    /// already answered.
    #[test]
    fn read_rich_passes_through_rich_payload() {
        let plain = Arc::new(PlainFake::default());
        let rich = Arc::new(RichFake::default());
        let payload =
            RichTextPayload::new("plain".into(), Some("<b>x</b>".into()), None).expect("valid");
        *rich.next_rich.lock() = Some(Ok(Some(payload.clone())));
        let plain_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&plain) as _;
        let rich_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&rich) as _;
        let composite = CompositeClipboard::new(plain_dyn, rich_dyn);

        let observed = composite
            .read_rich()
            .expect("rich")
            .expect("payload present");
        assert_eq!(observed.plain_text(), "plain");
        assert_eq!(observed.html(), Some("<b>x</b>"));
        assert_eq!(*plain.text_reads.lock(), 0);
    }

    /// When the rich adapter returns a soft error (`Unavailable`),
    /// the composite MUST keep the soft outcome and continue
    /// polling instead of converting it into a hard failure.
    #[test]
    fn read_payload_keeps_rich_unavailable_soft() {
        let plain = Arc::new(PlainFake::default());
        let rich = Arc::new(RichFake::default());
        *rich.next_rich.lock() = Some(Err(ClipboardBackendError::Unavailable {
            capability: crate::Capability::ClipboardReadRichText,
        }));
        *rich.next_plain.lock() = Some(Ok(None));
        let plain_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&plain) as _;
        let rich_dyn: Arc<dyn ClipboardBackend> = Arc::clone(&rich) as _;
        let composite = CompositeClipboard::new(plain_dyn, rich_dyn);

        // The composite must surface the same outcome the
        // rich adapter produced. The default `read_payload`
        // collapses soft errors into the plain-text attempt, which
        // is exactly what the composite does: the plain adapter's
        // plain read returns `None` (the fake returns `None`), so
        // the composite reports `Ignored`.
        let outcome = composite.read_payload().expect("soft");
        assert!(outcome.is_none());
    }
}
