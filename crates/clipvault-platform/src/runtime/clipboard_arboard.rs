//! `arboard`-backed clipboard implementation.
//!
//! `arboard` is already the text clipboard adapter for macOS and
//! Linux X11, and its `image-data` feature (enabled by default and
//! confirmed present in `Cargo.lock` through the transitive `image`
//! dependency) exposes exactly the two operations the
//! `clipboard-rich-content` capability needs: `get_image` and
//! `set_image`, both in straight RGBA. Reusing it avoids adding a
//! second clipboard dependency and keeps a single code path for
//! the pasteboard flavour negotiation on each host.
//!
//! For rich text the adapter rides on `arboard`'s HTML support
//! (`get().html()` / `set().html(html, alt)`), which maps to
//! `NSPasteboardTypeHTML` on macOS and the `text/html` selection
//! target on X11. RTF is **not** exposed by `arboard`, so this
//! adapter cannot publish an RTF leg even when the rest of the
//! pipeline has the bytes. The contract distinguishes three
//! outcomes:
//!
//! 1. The payload carries HTML only. The adapter publishes the HTML
//!    leg through `arboard::set_html`; the result is a successful
//!    rich write.
//! 2. The payload carries RTF only. The adapter returns
//!    `ClipboardBackendError::Unavailable { capability:
//!    ClipboardWriteRichText }` so the pipeline can surface a typed
//!    capability outcome. Writing plain text here would falsify a
//!    rich paste and is explicitly forbidden by the contract.
//! 3. The payload carries both HTML and RTF. The adapter publishes
//!    the HTML leg (the only one `arboard` exposes) and the
//!    pipeline keeps the original RTF bytes on the row so a future
//!    platform adapter that exposes RTF can paste them.
//!
//! macOS hosts with the `macos-native` feature enabled use a
//! dedicated `MacOsPasteboardClipboard` adapter that publishes
//! `public.utf8-plain-text`, `public.rtf` and `public.html` through
//! `NSPasteboard` directly. The bootstrap wires both adapters:
//! `arboard` for images, `NSPasteboard` for rich text. This module
//! is the Linux X11 / non-`macos-native` fallback.
//!
//! `supports_rich_write()` returns `true` only when at least the
//! HTML leg is supported. The HTML leg is supported on every
//! `arboard` build, so the answer is `true` whenever the
//! `clipboard-arboard` feature is on; the per-operation refusal for
//! RTF-only payloads surfaces through the typed `Unavailable` error
//! instead of a false capability downgrade.
//!
//! The adapter converts between `arboard::ImageData` and the neutral
//! [`ClipboardImage`] and nothing else: no filesystem access, no PNG
//! encoding, no hashing. Those belong to the core.

use std::borrow::Cow;

use arboard::{Clipboard as Arboard, Error as ArboardError, ImageData};

use crate::clipboard::{
    checked_rgba_len, ClipboardBackend, ClipboardBackendError, ClipboardImage,
    ImageValidationError, RichTextPayload,
};
use crate::Capability;

/// Thin wrapper around [`arboard::Clipboard`]. Each method takes the
/// global lock lazily so the backend stays cheap to share across
/// threads.
pub struct ArboardClipboard;

impl Default for ArboardClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl ArboardClipboard {
    pub fn new() -> Self {
        Self
    }
}

fn map_error(error: ArboardError) -> ClipboardBackendError {
    match error {
        ArboardError::ContentNotAvailable | ArboardError::ConversionFailure => {
            ClipboardBackendError::Empty
        }
        other => ClipboardBackendError::backend(other),
    }
}

/// Map an image-read failure. `ContentNotAvailable` means "no image on
/// the clipboard" and `ConversionFailure` means "the representation
/// exists but cannot be decoded into RGBA" — both are soft outcomes
/// that must leave the watcher running, so they collapse into
/// [`ClipboardBackendError::UnsupportedFormat`] instead of a hard
/// backend failure.
fn map_image_error(error: ArboardError) -> ClipboardBackendError {
    match error {
        ArboardError::ContentNotAvailable | ArboardError::ConversionFailure => {
            ClipboardBackendError::UnsupportedFormat
        }
        other => ClipboardBackendError::backend(other),
    }
}

/// Convert an `arboard` bitmap into the neutral, validated payload.
///
/// The dimensions are validated *before* the pixel buffer is copied so
/// a hostile pasteboard entry cannot make ClipVault allocate an
/// unbounded `Vec`.
fn to_clipboard_image(data: ImageData<'_>) -> Result<ClipboardImage, ClipboardBackendError> {
    let width = u32::try_from(data.width).map_err(|_| {
        ClipboardBackendError::InvalidImage(ImageValidationError::DimensionTooLarge {
            dim: u32::MAX,
            max: crate::clipboard::MAX_CLIPBOARD_IMAGE_DIM,
        })
    })?;
    let height = u32::try_from(data.height).map_err(|_| {
        ClipboardBackendError::InvalidImage(ImageValidationError::DimensionTooLarge {
            dim: u32::MAX,
            max: crate::clipboard::MAX_CLIPBOARD_IMAGE_DIM,
        })
    })?;
    // Validate the geometry (zero dimensions, per-dimension cap,
    // multiplication overflow, total-size cap) before touching bytes.
    let expected = checked_rgba_len(width, height)?;
    if data.bytes.len() != expected {
        return Err(ClipboardBackendError::InvalidImage(
            ImageValidationError::StrideMismatch {
                expected,
                actual: data.bytes.len(),
            },
        ));
    }
    let rgba = data.bytes.into_owned();
    Ok(ClipboardImage::new(rgba, width, height)?)
}

impl ClipboardBackend for ArboardClipboard {
    fn read_text(&self) -> Result<Option<String>, ClipboardBackendError> {
        let mut clipboard = Arboard::new().map_err(ClipboardBackendError::backend)?;
        match clipboard.get_text() {
            Ok(text) if text.is_empty() => Ok(None),
            Ok(text) => Ok(Some(text)),
            Err(error) => Err(map_error(error)),
        }
    }

    fn write_text(&self, text: &str) -> Result<(), ClipboardBackendError> {
        let mut clipboard = Arboard::new().map_err(ClipboardBackendError::backend)?;
        clipboard
            .set_text(text.to_string())
            .map_err(ClipboardBackendError::backend)
    }

    fn read_rich(&self) -> Result<Option<RichTextPayload>, ClipboardBackendError> {
        let mut clipboard = Arboard::new().map_err(ClipboardBackendError::backend)?;
        // arboard exposes only the HTML leg of a rich-text payload;
        // the plain-text companion comes from the standard `get_text`
        // call and is required by the `RichTextPayload` invariant.
        let plain_text = match clipboard.get_text() {
            Ok(text) if !text.is_empty() => text,
            // No usable plain text: this is not a rich capture even if
            // the HTML leg happens to exist. The watcher can still try
            // the image pipeline afterwards.
            Ok(_) => return Ok(None),
            Err(ArboardError::ContentNotAvailable) | Err(ArboardError::ConversionFailure) => {
                return Ok(None);
            }
            Err(error) => return Err(map_error(error)),
        };
        let html = match clipboard.get().html() {
            Ok(html) if !html.is_empty() => Some(html),
            Ok(_) => None,
            Err(ArboardError::ContentNotAvailable | ArboardError::ConversionFailure) => None,
            // `get_html` returns `ContentNotAvailable` for an absent
            // leg; anything else surfaces as a hard backend failure so
            // the caller knows the rich capture is unreliable.
            Err(error) => return Err(map_image_error(error)),
        };
        // RTF is not exposed by arboard: keep the leg explicit so a
        // future adapter that does expose it can fill it in without a
        // contract change.
        RichTextPayload::new(plain_text, html, None)
            .map(Some)
            .or_else(|error| {
                if matches!(error, ClipboardBackendError::Empty) {
                    // The HTML leg disappeared between the two arboard
                    // reads (a concurrent writer): not a rich capture.
                    Ok(None)
                } else {
                    Err(error)
                }
            })
    }

    fn write_rich(&self, payload: &RichTextPayload) -> Result<(), ClipboardBackendError> {
        let mut clipboard = Arboard::new().map_err(ClipboardBackendError::backend)?;
        // The contract forbids silently writing the plain text when
        // only RTF is available: doing so would falsify a rich paste
        // and the user would observe plain text in the receiving
        // application without any indication that the rich flavours
        // were dropped. The correct outcome is a typed capability
        // error so the pipeline can decide between a plain fallback
        // and a hard failure.
        if payload.html().is_none() && payload.rtf().is_some() {
            return Err(ClipboardBackendError::Unavailable {
                capability: Capability::ClipboardWriteRichText,
            });
        }
        if let Some(html) = payload.html() {
            // `arboard`'s `set_html` writes the HTML leg together
            // with the canonical plain text as the alt-text
            // fallback in a single operation. The plain text is
            // always published as the `alt` parameter so a
            // consumer that only knows about plain text still
            // receives the canonical `content` (the `arboard`
            // contract pins the alt-text behaviour).
            return clipboard
                .set()
                .html(html.to_string(), Some(payload.plain_text().to_string()))
                .map_err(ClipboardBackendError::backend);
        }
        // The payload carries neither HTML nor RTF. The
        // `RichTextPayload` constructor already rejects an empty
        // rich payload, so reaching this branch means the
        // `RichTextPayload` invariant was bypassed (a refactor
        // that relaxes the invariant would be a regression).
        // Surface the same typed `Unavailable` outcome so the
        // pipeline can degrade instead of silently losing the
        // rich claim.
        Err(ClipboardBackendError::Unavailable {
            capability: Capability::ClipboardWriteRichText,
        })
    }

    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardBackendError> {
        let mut clipboard = Arboard::new().map_err(ClipboardBackendError::backend)?;
        match clipboard.get_image() {
            Ok(data) => to_clipboard_image(data).map(Some),
            Err(error) => Err(map_image_error(error)),
        }
    }

    fn write_image(&self, image: &ClipboardImage) -> Result<(), ClipboardBackendError> {
        let mut clipboard = Arboard::new().map_err(ClipboardBackendError::backend)?;
        let data = ImageData {
            width: image.width() as usize,
            height: image.height() as usize,
            bytes: Cow::Borrowed(image.rgba()),
        };
        clipboard
            .set_image(data)
            .map_err(ClipboardBackendError::backend)
    }

    fn supports_rich_read(&self) -> bool {
        true
    }

    fn supports_rich_write(&self) -> bool {
        true
    }

    fn supports_image_read(&self) -> bool {
        true
    }

    fn supports_image_write(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "arboard"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_clipboard_image_accepts_a_well_formed_bitmap() {
        let data = ImageData {
            width: 2,
            height: 2,
            bytes: Cow::Owned(vec![0x11; 2 * 2 * 4]),
        };
        let image = to_clipboard_image(data).expect("valid bitmap");
        assert_eq!((image.width(), image.height()), (2, 2));
        assert_eq!(image.byte_len(), 16);
    }

    #[test]
    fn to_clipboard_image_rejects_zero_dimensions_before_allocating() {
        for (w, h) in [(0usize, 4usize), (4, 0)] {
            let data = ImageData {
                width: w,
                height: h,
                bytes: Cow::Owned(Vec::new()),
            };
            let err = to_clipboard_image(data).expect_err("must reject");
            assert_eq!(err.kind_str(), "invalid_image");
        }
    }

    #[test]
    fn to_clipboard_image_rejects_a_buffer_that_does_not_match_the_geometry() {
        let data = ImageData {
            width: 4,
            height: 4,
            // A truncated buffer: the adapter must not trust the
            // reported geometry.
            bytes: Cow::Owned(vec![0; 8]),
        };
        let err = to_clipboard_image(data).expect_err("must reject");
        match err {
            ClipboardBackendError::InvalidImage(ImageValidationError::StrideMismatch {
                expected,
                actual,
            }) => {
                assert_eq!(expected, 64);
                assert_eq!(actual, 8);
            }
            other => panic!("expected StrideMismatch, got {other:?}"),
        }
    }

    #[test]
    fn to_clipboard_image_rejects_dimensions_above_the_cap() {
        let data = ImageData {
            width: crate::clipboard::MAX_CLIPBOARD_IMAGE_DIM as usize + 1,
            height: 1,
            bytes: Cow::Owned(Vec::new()),
        };
        let err = to_clipboard_image(data).expect_err("must reject");
        assert_eq!(err.kind_str(), "invalid_image");
    }

    #[test]
    fn image_read_errors_map_to_soft_outcomes() {
        // "No image available" and "cannot convert the available
        // representation" are both `Ignored`, not failures: the
        // watcher has to keep polling.
        assert!(map_image_error(ArboardError::ContentNotAvailable).is_soft());
        assert!(map_image_error(ArboardError::ConversionFailure).is_soft());
        assert_eq!(
            map_image_error(ArboardError::ContentNotAvailable).kind_str(),
            "unsupported_format"
        );
    }

    #[test]
    fn adapter_declares_both_image_directions() {
        let backend = ArboardClipboard::new();
        assert!(backend.supports_image_read());
        assert!(backend.supports_image_write());
        assert_eq!(backend.name(), "arboard");
    }

    // -----------------------------------------------------------------
    // `clipboard-rich-text`: rich-text capability surface.
    //
    // The adapter does not own any test-only clipboard state, but the
    // capability flags are part of the public contract. Pin them here
    // so a future refactor that drops rich support surfaces here
    // instead of as a frontend regression.
    // -----------------------------------------------------------------

    #[test]
    fn adapter_declares_both_rich_text_directions() {
        let backend = ArboardClipboard::new();
        assert!(backend.supports_rich_read());
        assert!(backend.supports_rich_write());
    }
}
