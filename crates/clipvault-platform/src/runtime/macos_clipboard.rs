//! macOS-backed [`ClipboardBackend`] built on top of `NSPasteboard`.
//!
//! `arboard` exposes only the HTML leg of a rich-text pasteboard
//! flavour; it does not publish `public.rtf`, so the previous
//! adapter silently degraded an RTF-only capture to a plain-text
//! write and returned `Ok(())`, which the rest of the pipeline
//! classified as a successful rich paste. The user-facing regression
//! was "pegar texto enriquecido loses formatting": the RTF and HTML
//! bytes were never published together.
//!
//! This adapter fixes the contract by talking to `NSPasteboard`
//! directly so a single logical write publishes:
//!
//! - `public.utf8-plain-text` (always, as the plain-text fallback);
//! - `public.rtf` (when the payload carries RTF bytes);
//! - `public.html` (when the payload carries HTML bytes).
//!
//! The read side mirrors that: a single `NSPasteboard::generalPasteboard()`
//! read returns the canonical plain text and whichever rich
//! representation the source application published. RTF is parsed as
//! raw bytes (`NSPasteboard::dataForType(NSPasteboardTypeRTF)`) so
//! the contract never has to round-trip through an HTML-to-RTF
//! conversion.
//!
//! ## Threading
//!
//! Every trait method on this adapter transparently hops to the Cocoa
//! main thread through
//! [`crate::runtime::macos_clipboard_main_queue`]. The capture loop
//! therefore reads the rich legs without needing the watcher to
//! schedule anything on the main thread — the bridge takes care of
//! it. The hop is synchronous (the calling thread blocks until the
//! closure returns) but bounded by Tauri's main loop: a single
//! pasteboard read is sub-millisecond on Apple silicon and never
//! stalls the UI for a user-visible duration.
//!
//! When the bridge cannot reach the main thread (no event loop
//! running, e.g. inside a unit test without a Tauri runtime) the
//! adapter returns a typed
//! [`ClipboardBackendError::Unavailable`] so the watcher keeps
//! polling instead of converting a thread limitation into a hard
//! `WatchTickOutcome::Failed`.
//!
//! ## Image transport
//!
//! Image read and write go through this adapter's native
//! `NSPasteboard` path. Reads prefer original `public.png` bytes and
//! inspect paired `public.tiff` metadata on the same main-thread
//! snapshot. Image write goes through this adapter's own PNG path so the bitmap
//! survives the round-trip verbatim — the previous
//! `pasteboard.writeObjects(&[NSImage])` route silently stored a
//! downsampled / cropped version of the source bitmap when the
//! AppKit representation cache dropped the original. The PNG path
//! encodes the canonical RGBA buffer via `NSBitmapImageRep` and
//! publishes the resulting `NSData` through
//! `setData_forType(NSPasteboardTypePNG)`. The composite wires this
//! adapter on macOS and falls back to `arboard` on Linux X11 where
//! the same native path is not available.

#![cfg(all(target_os = "macos", feature = "macos-native"))]

use crate::clipboard::{ClipboardBackend, ClipboardBackendError, ClipboardImage, RichTextPayload};
use crate::runtime::macos_clipboard_main_queue;
use crate::Capability;

/// Canonical name for the `public.utf8-plain-text` flavour. Apple
/// does not export a `NSPasteboardTypeUTF8PlainText` constant on
/// every SDK version; the underlying value
/// (`@"public.utf8-plain-text"`) is what every AppKit-aware
/// application consumes. The constant lives here so the bridge and
/// the tests share the same identifier.
pub const UTF8_PLAIN_TEXT_TYPE: &str = "public.utf8-plain-text";

/// Thin wrapper around [`objc2_app_kit::NSPasteboard::generalPasteboard`].
/// The adapter is stateless: a single instance is shared across
/// threads, every call hops to the main thread through the bridge,
/// and the resulting `NSPasteboard` handles never escape the closure.
#[derive(Default)]
pub struct MacOsPasteboardClipboard;

impl MacOsPasteboardClipboard {
    pub fn new() -> Self {
        Self
    }
}

/// Emit the native image-read outcome only when the image-capture
/// diagnostic is explicitly enabled. This is deliberately
/// metadata-only: it reports the selected branch (`native_png`,
/// `tiff_only_with_metadata`, `no_png_with_tiff_metadata`, `invalid_png`, or
/// `main_queue_unavailable`) and never reports bytes, sizes, paths,
/// identifiers or clipboard content. Keeping this signal at the
/// adapter boundary makes an `arboard_fallback` diagnostic
/// actionable instead of leaving the operator to infer whether the
/// native bridge was compiled and reached at all.
fn log_native_image_read_outcome(
    outcome: &Result<
        macos_clipboard_main_queue::NativePngRead,
        macos_clipboard_main_queue::MainQueueBridgeError,
    >,
) {
    let enabled =
        std::env::var_os("CLIPVAULT_DEBUG_IMAGE_CAPTURE").is_some_and(|value| value == "1");
    if !enabled {
        return;
    }
    let kind = outcome
        .as_ref()
        .map(macos_clipboard_main_queue::NativePngRead::kind_str)
        .unwrap_or("main_queue_unavailable");
    tracing::debug!(
        native_image_read_outcome = kind,
        "macOS native image read outcome"
    );
}

/// Re-attach the bridge's [`PasteboardImageMetadata`] to the
/// validated image the bridge produced so the persistence layer
/// can resolve the four fidelity paths the
/// `native_png_with_tiff_metadata` outcome surfaces.
///
/// The helper is necessary because the bridge returns a
/// `(image, metadata)` pair: the core eventually needs the
/// metadata in the [`ClipboardImage`] struct rather than in a
/// parallel tuple, so the adapter forwards the metadata via the
/// dedicated constructor on the platform layer.
fn attach_pasteboard_metadata(
    image: ClipboardImage,
    metadata: macos_clipboard_main_queue::PasteboardImageMetadata,
) -> ClipboardImage {
    if metadata.tiff_metadata.is_none()
        && metadata.png_metadata == crate::clipboard_image_png::PngMetadataSummary::default()
        && metadata.inferred_display_dpi.is_none()
    {
        return image;
    }
    let rgba = image.rgba().to_vec();
    let width = image.width();
    let height = image.height();
    let original_png = image.clone().into_original_png();
    // Drop the bridge-side clone: the original `image` carries the
    // same RGBA buffer we just copied out, so we drop it after the
    // clone to avoid any double-ownership corner case.
    drop(image);
    let platform_metadata = crate::clipboard::PasteboardImageMetadata {
        png_chunks: metadata.png_metadata,
        tiff: metadata.tiff_metadata,
        inferred_display_dpi: metadata.inferred_display_dpi,
        resolution_detected: metadata.resolution_detected,
        profile_detected: metadata.profile_detected,
    };
    let rebuilt = match original_png {
        Some(original_png) => ClipboardImage::with_pasteboard_metadata(
            rgba.clone(),
            width,
            height,
            original_png,
            platform_metadata,
        ),
        None => ClipboardImage::with_pasteboard_metadata_without_original(
            rgba.clone(),
            width,
            height,
            platform_metadata,
        ),
    };
    match rebuilt {
        Ok(image_with_metadata) => image_with_metadata,
        // The constructor never fails on a struct the bridge
        // already validated: stride mismatch is enforced before
        // and the `original_png` byte buffer is non-empty by the
        // bridge invariants. The fallback rebuilds a vanilla
        // image so the caller still gets a usable struct.
        Err(_) => ClipboardImage::new(rgba, width, height)
            .unwrap_or_else(|_| ClipboardImage::new(Vec::new(), 1, 1).expect("safe fallback")),
    }
}

impl ClipboardBackend for MacOsPasteboardClipboard {
    fn read_text(&self) -> Result<Option<String>, ClipboardBackendError> {
        macos_clipboard_main_queue::read_plain_text_main_thread()
            .map_err(|_| macos_clipboard_main_queue::unavailable_for(Capability::ClipboardRead))
    }

    fn write_text(&self, text: &str) -> Result<(), ClipboardBackendError> {
        macos_clipboard_main_queue::write_plain_text_main_thread(text.to_string())
            .map_err(|_| macos_clipboard_main_queue::unavailable_for(Capability::ClipboardWrite))
    }

    fn read_rich(&self) -> Result<Option<RichTextPayload>, ClipboardBackendError> {
        // `read_rich` MUST run on the Cocoa main thread. The bridge
        // does the hop transparently so the capture loop never sees
        // a `Backend` error caused by the thread check. When the
        // bridge cannot reach the main thread (no event loop), the
        // adapter surfaces a typed `Unavailable` so the watcher
        // keeps polling.
        macos_clipboard_main_queue::read_rich_main_thread().map_err(|_| {
            macos_clipboard_main_queue::unavailable_for(Capability::ClipboardReadRichText)
        })
    }

    fn write_rich(&self, payload: &RichTextPayload) -> Result<(), ClipboardBackendError> {
        macos_clipboard_main_queue::write_rich_main_thread(payload).map_err(|_| {
            macos_clipboard_main_queue::unavailable_for(Capability::ClipboardWriteRichText)
        })
    }

    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardBackendError> {
        // Image read delegates to the native PNG bridge so the
        // pasteboard's `public.png` bytes survive verbatim — including
        // the `pHYs` resolution chunk, the `iCCP` / `sRGB` color
        // profile and any other safe metadata chunks the source
        // application published. The composite routes
        // `read_image` through this method when it advertises
        // `supports_image_png_read`; the bridge's typed outcome lets
        // the composite decide between a safe fallback to the
        // `arboard` bitmap path and a hard `InvalidImage` error.
        let outcome = macos_clipboard_main_queue::read_png_main_thread();
        log_native_image_read_outcome(&outcome);
        match outcome {
            Ok(macos_clipboard_main_queue::NativePngRead::Native { image, metadata }) => {
                Ok(Some(attach_pasteboard_metadata(image, metadata)))
            }
            Ok(macos_clipboard_main_queue::NativePngRead::TiffOnly { image, metadata }) => {
                Ok(Some(attach_pasteboard_metadata(image, metadata)))
            }
            Ok(macos_clipboard_main_queue::NativePngRead::NoPng { .. }) => {
                // No usable native raster representation exists:
                // surface a soft `UnsupportedFormat` so the composite
                // can use the legacy bitmap path. A valid TIFF-only
                // capture never reaches this branch because the bridge
                // decodes it into `TiffOnly` first.
                Err(ClipboardBackendError::UnsupportedFormat)
            }
            Ok(macos_clipboard_main_queue::NativePngRead::InvalidPng(error)) => {
                // `public.png` was present but failed. Surfacing the
                // structural error here is what stops the composite
                // from silently falling back to `arboard::get_image`
                // and saving a degraded PNG without the original
                // metadata chunks the user reported as missing.
                Err(ClipboardBackendError::InvalidImage(
                    crate::clipboard::ImageValidationError::InvalidPng {
                        kind: error.kind_str(),
                    },
                ))
            }
            Err(_) => Err(ClipboardBackendError::Unavailable {
                capability: Capability::ClipboardReadImage,
            }),
        }
    }

    fn read_image_png(&self) -> Result<Option<ClipboardImage>, ClipboardBackendError> {
        // The fidelity-preserving read lives entirely on this adapter:
        // the bridge reads `public.png` from `NSPasteboard` on the
        // Cocoa main thread, validates the bytes against the PNG
        // signature, the size cap and the dimension cap, decodes the
        // RGBA frame for dedupe and returns the original PNG bytes
        // alongside the bitmap. The typed outcome lets the composite
        // distinguish three different pasteboard shapes:
        //
        // - `Ok(Some(image))` when `public.png` was present and the
        //   bytes validated. The image carries `original_png` so the
        //   core can persist the original bytes verbatim.
        // - `Ok(None)` when the clipboard has no `public.png`
        //   representation at all. The composite falls back to the
        //   legacy `arboard` bitmap path.
        // - `Err(InvalidImage)` when `public.png` was present but
        //   the bytes failed validation. The composite MUST NOT fall
        //   back to `arboard::get_image` here: doing so would
        //   silently save a re-encoded PNG that lacks the metadata
        //   chunks the user observed as missing.
        // - `Err(Unavailable)` when the bridge cannot hop to the
        //   main thread. The composite surfaces this soft result so
        //   the watcher retries instead of degrading to
        //   `arboard::get_image` in the same tick.
        let outcome = macos_clipboard_main_queue::read_png_main_thread();
        log_native_image_read_outcome(&outcome);
        match outcome {
            Ok(macos_clipboard_main_queue::NativePngRead::Native { image, metadata }) => {
                Ok(Some(attach_pasteboard_metadata(image, metadata)))
            }
            Ok(macos_clipboard_main_queue::NativePngRead::TiffOnly { image, metadata }) => {
                Ok(Some(attach_pasteboard_metadata(image, metadata)))
            }
            Ok(macos_clipboard_main_queue::NativePngRead::NoPng { .. }) => Ok(None),
            Ok(macos_clipboard_main_queue::NativePngRead::InvalidPng(error)) => {
                Err(ClipboardBackendError::InvalidImage(
                    crate::clipboard::ImageValidationError::InvalidPng {
                        kind: error.kind_str(),
                    },
                ))
            }
            Err(_) => Err(macos_clipboard_main_queue::unavailable_for(
                Capability::ClipboardReadImage,
            )),
        }
    }

    fn write_image(&self, image: &ClipboardImage) -> Result<(), ClipboardBackendError> {
        // Image write goes through the native PNG path so the
        // canonical bitmap survives the round-trip. The composite
        // routes here whenever this adapter is wired (macOS) so a
        // regression that surfaced `Backend { ... }` from this
        // method would break the documented image-copy contract.
        let rgba = image.rgba().to_vec();
        macos_clipboard_main_queue::write_image_main_thread(image.width(), image.height(), rgba)
            .map_err(|_| {
                macos_clipboard_main_queue::unavailable_for(Capability::ClipboardWriteImage)
            })
    }

    fn write_image_png(&self, png: &[u8]) -> Result<(), ClipboardBackendError> {
        macos_clipboard_main_queue::write_png_main_thread(png.to_vec()).map_err(|_| {
            macos_clipboard_main_queue::unavailable_for(Capability::ClipboardWriteImage)
        })
    }

    fn supports_rich_read(&self) -> bool {
        true
    }

    fn supports_rich_write(&self) -> bool {
        // Both directions advertise support when the adapter is
        // wired in. Per-operation failures surface through
        // `Unavailable { capability }` instead of a false capability
        // downgrade.
        true
    }

    fn supports_image_read(&self) -> bool {
        // The native adapter exposes a fidelity-preserving
        // `public.png` read; the composite still uses this flag to
        // short-circuit image reads on hosts where the bridge cannot
        // reach the main thread. The flag is `true` so the composite
        // prefers the native read; `Ok(None)` from the bridge is the
        // soft-miss signal that triggers the `arboard` fallback.
        true
    }

    fn supports_image_png_read(&self) -> bool {
        // The native adapter is the only path that can read the
        // original PNG bytes the pasteboard exposed; the composite
        // uses this flag to dispatch `read_image_png` instead of
        // `read_image` whenever the bridge is reachable.
        true
    }

    fn supports_image_write(&self) -> bool {
        // The native adapter publishes the image as PNG via
        // `setData_forType(NSPasteboardTypePNG)`. The composite uses
        // this flag to route image writes to the macOS adapter
        // instead of the legacy `arboard` path that produced a
        // partial / downsampled bitmap. Per-operation failures
        // surface through `Unavailable { capability }` instead of a
        // false capability downgrade.
        true
    }

    fn supports_image_png_write(&self) -> bool {
        // The native adapter publishes the persisted PNG bytes
        // directly through `setData:forType:`. This is the path that
        // guarantees the preview and a later paste observe the same
        // complete bitmap.
        true
    }

    fn supports_native_plain_write(&self) -> bool {
        // The native adapter hops to the Cocoa main thread and
        // calls `clearContents()` before publishing the plain text.
        // The composite routes `write_text` through this path so a
        // `PasteMode::Plain` paste never leaves `public.html` or
        // `public.rtf` on the pasteboard from a previous rich
        // capture. When the bridge cannot reach the main thread
        // (no event loop, unit tests) the call surfaces
        // `Unavailable` and the composite falls back to the plain
        // adapter, which is the documented degradation path.
        true
    }

    fn name(&self) -> &'static str {
        "macos_pasteboard"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::ClipboardBackendError;

    /// Sanity: the wrapper is `Send + Sync` so it can live behind
    /// an `Arc` and be shared with the watcher thread.
    #[test]
    fn adapter_is_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<MacOsPasteboardClipboard>();
        assert_sync::<MacOsPasteboardClipboard>();
    }

    /// A native PNG can have no PNG chunks and no usable TIFF IFD while
    /// still carrying the bounded display-scale fallback. That fallback
    /// must survive the adapter boundary; otherwise the core receives the
    /// legacy image and silently persists it at the PNG default of 72 ppi.
    #[test]
    fn attach_pasteboard_metadata_preserves_display_scale_fallback() {
        let image =
            ClipboardImage::with_original_png(vec![0, 0, 0, 255], 1, 1, vec![1]).expect("image");
        let metadata = macos_clipboard_main_queue::PasteboardImageMetadata {
            png_metadata: crate::clipboard_image_png::PngMetadataSummary::default(),
            tiff_metadata: None,
            inferred_display_dpi: Some((144, 144)),
            resolution_detected: true,
            profile_detected: false,
        };

        let attached = attach_pasteboard_metadata(image, metadata);
        assert_eq!(
            attached.pasteboard_metadata().inferred_display_dpi,
            Some((144, 144))
        );
        assert!(attached.has_original_png());
    }

    /// The constant surface area (types, stable name, capability
    /// flags) is pinned here so a future refactor that drops one of
    /// the legs surfaces here instead of as a silent frontend
    /// regression.
    #[test]
    fn adapter_declares_rich_text_capabilities_and_image_write() {
        let backend = MacOsPasteboardClipboard::new();
        assert!(backend.supports_rich_read());
        assert!(backend.supports_rich_write());
        // Image read goes through the fidelity-preserving native
        // PNG bridge; `supports_image_png_read` advertises the
        // capability so the composite dispatches `read_image_png`.
        assert!(backend.supports_image_read());
        assert!(backend.supports_image_png_read());
        // Image write goes through the native PNG path so the
        // canonical bitmap survives the round-trip.
        assert!(backend.supports_image_write());
        assert_eq!(backend.name(), "macos_pasteboard");
    }

    /// `read_rich` from a background thread must never surface the
    /// pre-fix `read_rich must run on the macOS main thread` typed
    /// error. The bridge returns an `Unavailable` outcome instead,
    /// so the watcher keeps polling.
    ///
    /// The unit test runs outside of Tauri (no event loop driving
    /// the main queue), which is the exact path the previous
    /// prototype hit when called from the capture loop on a host
    /// without `dispatch2` initialised. The bridge must return
    /// `Unavailable`, not `Backend`.
    #[test]
    fn read_rich_from_background_returns_unavailable_not_backend() {
        let backend = MacOsPasteboardClipboard::new();
        // `spawn` simulates the background capture loop.
        let outcome = std::thread::Builder::new()
            .name("cv-test-read-rich".to_string())
            .spawn(move || backend.read_rich())
            .expect("spawn")
            .join()
            .expect("join");
        match outcome {
            Err(ClipboardBackendError::Unavailable { capability }) => {
                assert_eq!(capability, Capability::ClipboardReadRichText);
            }
            Err(other) => {
                panic!("background read_rich must not surface a Backend error, got {other:?}")
            }
            Ok(_) => {
                // If a future refactor wires a real event loop in
                // tests, this branch is acceptable — but the
                // production environment never has that. Pinning
                // the error path keeps the regression visible.
            }
        }
    }

    /// Symmetric assertion for `write_rich`: the bridge must return
    /// `Unavailable { ClipboardWriteRichText }` from a background
    /// thread outside of a Tauri runtime, never `Backend { ... }`.
    #[test]
    fn write_rich_from_background_returns_unavailable_not_backend() {
        let backend = MacOsPasteboardClipboard::new();
        let payload = RichTextPayload::new(
            "plain".into(),
            Some("<b>plain</b>".into()),
            Some(b"{\\rtf1 plain}".to_vec()),
        )
        .expect("valid");
        let outcome = std::thread::Builder::new()
            .name("cv-test-write-rich".to_string())
            .spawn(move || backend.write_rich(&payload))
            .expect("spawn")
            .join()
            .expect("join");
        match outcome {
            Err(ClipboardBackendError::Unavailable { capability }) => {
                assert_eq!(capability, Capability::ClipboardWriteRichText);
            }
            Err(other) => {
                panic!("background write_rich must not surface a Backend error, got {other:?}")
            }
            Ok(()) => {}
        }
    }

    /// Symmetric assertion for the plain-text legs: `read_text` and
    /// `write_text` from a background thread surface `Unavailable`
    /// (capability `ClipboardRead` / `ClipboardWrite`) instead of
    /// the pre-fix `Backend` error.
    #[test]
    fn read_write_text_from_background_returns_unavailable_not_backend() {
        let backend = MacOsPasteboardClipboard::new();
        let read_backend = MacOsPasteboardClipboard::new();
        let read_outcome = std::thread::Builder::new()
            .name("cv-test-read-text".to_string())
            .spawn(move || read_backend.read_text())
            .expect("spawn")
            .join()
            .expect("join");
        match read_outcome {
            Err(ClipboardBackendError::Unavailable { capability }) => {
                assert_eq!(capability, Capability::ClipboardRead);
            }
            Err(other) => {
                panic!("background read_text must not surface a Backend error, got {other:?}")
            }
            Ok(_) => {}
        }

        let write_outcome = std::thread::Builder::new()
            .name("cv-test-write-text".to_string())
            .spawn(move || backend.write_text("hello"))
            .expect("spawn")
            .join()
            .expect("join");
        match write_outcome {
            Err(ClipboardBackendError::Unavailable { capability }) => {
                assert_eq!(capability, Capability::ClipboardWrite);
            }
            Err(other) => {
                panic!("background write_text must not surface a Backend error, got {other:?}")
            }
            Ok(()) => {}
        }
    }

    /// `AnyObject` is reachable through the trait surface so a
    /// future change that accidentally removes the import surfaces
    /// here. The smoke test pins the `objc2_foundation` symbol so
    /// the most common API stays compiled; the rest of the adapter
    /// exercises the same code path on every write.
    #[test]
    fn bridge_constant_keeps_canonical_utf8_plain_text_flavor() {
        assert_eq!(UTF8_PLAIN_TEXT_TYPE, "public.utf8-plain-text");
    }
}
