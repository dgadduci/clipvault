//! Main-thread bridge for the macOS `NSPasteboard` clipboard.
//!
//! ## Why this exists
//!
//! Apple's `NSPasteboard` operations (`generalPasteboard`, `dataForType:`
//! and `setData:forType:`) are documented to be used on the Cocoa main
//! thread; off-main callers get undefined behaviour. The previous
//! adapter enforced this with a hard `require_main_thread` check that
//! returned a typed `ClipboardBackendError::Backend` when the calling
//! thread was not the main thread.
//!
//! The capture loop, however, runs on a background thread; the
//! previous prototype therefore surfaced
//! `read_rich must run on the macOS main thread` as a fatal
//! `WatchTickOutcome::Failed` every tick. No capture was ever
//! persisted. The user-facing regression was "clipboard capture does
//! not capture anything rich from TextEdit".
//!
//! This bridge fixes the contract:
//!
//! - When the calling thread is the macOS main thread, the closure
//!   runs inline.
//! - When the calling thread is NOT the macOS main thread, the
//!   closure is submitted directly to `DispatchQueue::main()` with
//!   `exec_async` and the caller waits on a bounded channel. Tauri's
//!   main loop drives the queue, so the closure lands on the real
//!   main thread without an extra helper-thread hop.
//! - When neither path is available (no main queue, no event loop,
//!   dispatch not initialised), the bridge returns
//!   [`ClipboardBackendError::Unavailable`] — never
//!   [`ClipboardBackendError::Backend`] — so the rest of the pipeline
//!   can keep the watcher alive (soft miss) instead of converting a
//!   thread limitation into a hard capture failure.
//!
//! The bridge deliberately lives next to the adapter that uses it so
//! the dependency between the thread hop and the pasteboard
//! operations stays explicit. It does NOT depend on Tauri or on the
//! frontend: the test harness exercises it through the same
//! `dispatch2` primitives Tauri's main loop uses.
//!
//! ## Diagnostics
//!
//! Every hop is silent unless something went wrong. The bridge
//! returns `Ok(value)` on success, `Err(Unavailable { capability:
//! ClipboardReadRichText | ClipboardWriteRichText |
//! ClipboardRead | ClipboardWrite })` when no main-queue hop is
//! possible. It never carries the underlying thread error in the
//! outcome (the typed capability identifier is the entire surface a
//! caller gets).

#![cfg(all(target_os = "macos", feature = "macos-native"))]

use std::fmt;
use std::sync::mpsc;

use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2_foundation::MainThreadMarker;

use crate::clipboard::ClipboardBackendError;
use crate::Capability;

/// Why a pasteboard operation could not hop to the Cocoa main thread.
///
/// The bridge never reports this in `Debug` or `Display` form unless a
/// caller asks for the structured identifier; the user-facing surface
/// only ever sees the typed `ClipboardBackendError::Unavailable`
/// variant. The variants exist so tests and diagnostics can branch on
/// the precise reason without parsing free-form strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainQueueBridgeError {
    /// `dispatch2` rejected the hop (typically because no event loop
    /// is driving the main queue). The watcher treats this as a soft
    /// miss and continues polling.
    DispatchUnavailable,
}

impl fmt::Display for MainQueueBridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MainQueueBridgeError::DispatchUnavailable => {
                f.write_str("cocoa main queue is not reachable from this thread")
            }
        }
    }
}

/// Stable snake_case identifier for diagnostics.
impl MainQueueBridgeError {
    pub fn kind_str(&self) -> &'static str {
        match self {
            MainQueueBridgeError::DispatchUnavailable => "main_queue_unavailable",
        }
    }
}

/// Outcome of a clipboard operation dispatched to the Cocoa main thread.
///
/// `Ok(value)` means the closure ran on the main thread and returned
/// `value`. `Err(_)` means the bridge could not reach the main
/// thread; the caller converts the typed
/// [`MainQueueBridgeError`] into a typed
/// [`ClipboardBackendError::Unavailable`].
pub type BridgeResult<T> = Result<T, MainQueueBridgeError>;

/// Run `f` on the macOS main thread.
///
/// Behaviour:
///
/// - **Already on main thread** — `f` runs inline; no scheduling,
///   no allocation.
/// - **Off-main, dispatch available** — the closure is submitted
///   directly to `DispatchQueue::main()` and the caller waits until it
///   returns. Tauri's main loop drives the queue so the closure always
///   lands on the actual main thread.
/// - **Off-main, dispatch unavailable** — returns
///   `Err(MainQueueBridgeError::DispatchUnavailable)`. The caller
///   must convert the error into a typed `ClipboardBackendError::Unavailable`
///   (the typed capability identifier is the surface the rest of the
///   pipeline observes).
///
/// The result is returned through a synchronous channel because
/// `DispatchQueue::exec_async` only exposes a `FnOnce()` signature with
/// no return value. The bridge takes ownership of `f`'s return value
/// through the channel so the caller can observe it without resorting
/// to a mutable reference or `Arc<Mutex<_>>`.
///
/// `DispatchQueue::main().exec_async` returns immediately even if no
/// event loop is driving the main queue. The bridge waits with a
/// bounded timeout; the timeout translates into the typed
/// `DispatchUnavailable` outcome so a misconfigured host cannot stall
/// the capture loop forever.
fn dispatch_main_thread<F, R>(f: F) -> BridgeResult<R>
where
    F: FnOnce(MainThreadMarker) -> R + Send + 'static,
    R: Send + 'static,
{
    if MainThreadMarker::new().is_some() {
        // SAFETY: `MainThreadMarker::new()` succeeded, so we are on
        // the main thread; constructing a marker from the actual
        // thread state is sound.
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        return Ok(f(mtm));
    }

    // Enqueue directly on the main queue. The previous implementation
    // created a helper thread and called `run_on_main` from there. That
    // extra hop made the native clipboard path report Unavailable on a
    // live Tauri session even though the main queue itself was active,
    // after which the composite silently degraded to arboard. Direct
    // async submission keeps the timeout while ensuring the work is
    // owned by the same queue as the active-app refresher.
    let (tx, rx) = mpsc::sync_channel::<R>(1);
    DispatchQueue::main().exec_async(move || {
        let mtm = MainThreadMarker::new().expect("main queue must execute on the main thread");
        let value = f(mtm);
        let _ = tx.send(value);
    });
    match rx.recv_timeout(std::time::Duration::from_secs(1)) {
        Ok(value) => Ok(value),
        Err(_) => Err(MainQueueBridgeError::DispatchUnavailable),
    }
}

/// Read the plain-text leg of `NSPasteboard` on the main thread.
///
/// Returns `Ok(None)` when the pasteboard is empty (or the call
/// returned an empty string). Returns `Err(Unavailable { capability:
/// ClipboardRead })` when the bridge cannot hop to the main thread.
pub fn read_plain_text_main_thread() -> BridgeResult<Option<String>> {
    dispatch_main_thread(|_mtm| {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
        use objc2_foundation::NSString;

        let pasteboard = NSPasteboard::generalPasteboard();
        if let Some(string) = pasteboard.stringForType(unsafe { NSPasteboardTypeString }) {
            let value = string.to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
        // Fall back to the canonical UTF-8 plain-text flavour so
        // strict consumers (terminal apps, certain editors) still
        // observe the text. The fallback path is part of the same
        // main-thread read so the watcher's fingerprint stays
        // deterministic across producers.
        let utf8_type = NSString::from_str("public.utf8-plain-text");
        if let Some(string) = pasteboard.stringForType(&utf8_type) {
            let value = string.to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
        None
    })
}

/// Read the rich-text legs of `NSPasteboard` on the main thread.
///
/// The closure runs every pasteboard query inside a single
/// `generalPasteboard()` invocation so the three legs
/// (`public.utf8-plain-text`, `public.html`, `public.rtf`) come from
/// one consistent snapshot. Returning `Ok(None)` means "no rich
/// representation was published"; the caller's plain-text fallback
/// can then try a second read (also on the main thread) without
/// leaking the thread limitation.
///
/// Returns `Err(Unavailable { capability: ClipboardReadRichText })`
/// when the bridge cannot hop to the main thread.
pub fn read_rich_main_thread() -> BridgeResult<Option<crate::clipboard::RichTextPayload>> {
    use crate::clipboard::RichTextPayload;
    dispatch_main_thread(|_mtm| {
        use objc2_app_kit::{
            NSPasteboard, NSPasteboardTypeHTML, NSPasteboardTypeRTF, NSPasteboardTypeString,
        };
        use objc2_foundation::NSString;

        let pasteboard = NSPasteboard::generalPasteboard();

        // Plain-text leg (always inspected first so the caller can
        // decide between a rich capture and a plain capture without a
        // second pasteboard round-trip).
        let mut plain_text: Option<String> = None;
        if let Some(string) = pasteboard.stringForType(unsafe { NSPasteboardTypeString }) {
            let value = string.to_string();
            if !value.is_empty() {
                plain_text = Some(value);
            }
        }
        if plain_text.is_none() {
            let utf8_type = NSString::from_str("public.utf8-plain-text");
            if let Some(string) = pasteboard.stringForType(&utf8_type) {
                let value = string.to_string();
                if !value.is_empty() {
                    plain_text = Some(value);
                }
            }
        }
        let plain_text = plain_text?;

        // HTML leg.
        let html = pasteboard
            .stringForType(unsafe { NSPasteboardTypeHTML })
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());

        // RTF leg.
        let rtf = pasteboard
            .dataForType(unsafe { NSPasteboardTypeRTF })
            .and_then(|data| {
                let length = data.length();
                if length == 0 {
                    return Some(Vec::new());
                }
                if length > RichTextPayload::MAX_RICH_TEXT_RTF_BYTES {
                    return None;
                }
                let mut out: Vec<u8> = Vec::with_capacity(length);
                // SAFETY: `getBytes_length` copies `length` bytes into the
                // caller-provided buffer. The buffer must remain valid
                // for `length` bytes; `Vec`'s spare capacity is exactly
                // that.
                unsafe {
                    let dst = std::ptr::NonNull::new_unchecked(
                        out.as_mut_ptr().cast::<std::ffi::c_void>(),
                    );
                    data.getBytes_length(dst, length);
                    out.set_len(length);
                }
                Some(out)
            });

        if html.is_none() && rtf.is_none() {
            return None;
        }
        RichTextPayload::new(plain_text, html, rtf).ok()
    })
}

/// Write a rich-text payload to `NSPasteboard` on the main thread.
///
/// All three flavours (`public.utf8-plain-text`, `public.html`,
/// `public.rtf`) are published in a single logical write following
/// Apple's `clearContents` + `declareTypes:owner:` + per-flavour
/// `setData:forType:` pattern. The plain text leg is the canonical
/// fallback flavour so any consumer that only knows plain text still
/// receives the right `content`.
///
/// Returns `Ok(())` on success and `Err(MainQueueBridgeError::DispatchUnavailable)`
/// when the bridge cannot hop to the main thread.
pub fn write_rich_main_thread(payload: &crate::clipboard::RichTextPayload) -> BridgeResult<()> {
    let plain = payload.plain_text().to_string();
    let html = payload.html().map(str::to_string);
    let rtf = payload.rtf().map(|bytes| bytes.to_vec());

    let publish_succeeded: bool = dispatch_main_thread(move |_mtm| {
        use objc2_app_kit::{
            NSPasteboard, NSPasteboardTypeHTML, NSPasteboardTypeRTF, NSPasteboardTypeString,
        };
        use objc2_foundation::{NSArray, NSData, NSString};

        let pasteboard = NSPasteboard::generalPasteboard();
        let plain_ns = NSString::from_str(&plain);
        let utf8_type = NSString::from_str("public.utf8-plain-text");

        // Declare the union of types we are about to publish. Without
        // this declaration the pasteboard collapses to a single
        // flavour — the bug the previous prototype shipped.
        let mut declared_types: Vec<&NSString> = Vec::new();
        declared_types.push(unsafe { NSPasteboardTypeString });
        declared_types.push(&*utf8_type);
        if html.is_some() {
            declared_types.push(unsafe { NSPasteboardTypeHTML });
        }
        if rtf.is_some() {
            declared_types.push(unsafe { NSPasteboardTypeRTF });
        }
        let type_array = NSArray::from_slice(&declared_types);
        let _ = pasteboard.clearContents();
        let _ = unsafe { pasteboard.declareTypes_owner(&type_array, None) };

        // Plain text first (every consumer accepts it).
        let plain_ok = pasteboard.setString_forType(&plain_ns, unsafe { NSPasteboardTypeString })
            && pasteboard.setString_forType(&plain_ns, &utf8_type);
        if !plain_ok {
            return false;
        }

        if let Some(html) = &html {
            let html_ns = NSString::from_str(html);
            if !pasteboard.setString_forType(&html_ns, unsafe { NSPasteboardTypeHTML }) {
                return false;
            }
        }

        if let Some(rtf) = &rtf {
            let data = NSData::with_bytes(rtf);
            if !pasteboard.setData_forType(Some(&data), unsafe { NSPasteboardTypeRTF }) {
                return false;
            }
        }

        true
    })?;
    if publish_succeeded {
        Ok(())
    } else {
        Err(MainQueueBridgeError::DispatchUnavailable)
    }
}

/// Publish an already validated PNG to `NSPasteboard` on the main
/// thread without decoding or re-encoding it.
///
/// The clipboard asset preview and the copy operation must use the
/// same encoded image. Passing the persisted PNG directly avoids
/// AppKit bitmap conversion, row-stride interpretation and automatic
/// representation generation — all of which can make a receiver see
/// only a cropped portion even when the source file is complete.
///
/// Only the canonical PNG flavour is declared. The caller has already
/// validated the bytes and decoded them for the image suppression
/// fingerprint; this function deliberately does not create a second
/// representation such as TIFF.
pub fn write_png_main_thread(png: Vec<u8>) -> BridgeResult<()> {
    let publish_succeeded: bool = dispatch_main_thread(move |_mtm| {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG};
        use objc2_foundation::{NSArray, NSData, NSString};

        const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        if png.len() < PNG_SIGNATURE.len() || png[..PNG_SIGNATURE.len()] != PNG_SIGNATURE {
            return false;
        }

        let data = NSData::with_bytes(&png);
        let png_type = NSString::from_str("public.png");
        let declared_types: Vec<&NSString> = vec![&*png_type];
        let declared_types = NSArray::from_slice(&declared_types);
        let pasteboard = NSPasteboard::generalPasteboard();
        let _ = pasteboard.clearContents();
        let _ = unsafe { pasteboard.declareTypes_owner(&declared_types, None) };

        unsafe { pasteboard.setData_forType(Some(&data), NSPasteboardTypePNG) }
    })?;

    if publish_succeeded {
        Ok(())
    } else {
        Err(MainQueueBridgeError::DispatchUnavailable)
    }
}

/// Metadata captured alongside the `public.png` PNG bytes: the
/// chunk summary inside the PNG itself plus the resolution and
/// profile the `public.tiff` leg (or the AppKit / ImageIO metadata)
/// exposed on the same pasteboard snapshot.
///
/// The struct is the unit of evidence the bridge forwards to the
/// persistence layer. Three pieces of information travel together:
///
/// - `png_metadata`: the chunk summary the scanner produced by
///   walking the PNG byte stream; `pHYs`, `iCCP`, `sRGB`,
///   `gAMA`, `cHRM` and the text chunks are the only signals the
///   scanner forwards;
/// - `tiff_metadata`: the four IFD tags the TIFF parser cares
///   about (XResolution, YResolution, ResolutionUnit, ICCProfile);
///   present whenever the pasteboard exposes `public.tiff`;
/// - `diagnostic`: the metadata-only snapshot the diagnostic
///   helper emits so an operator can confirm the four
///   pasteboard-shape predicates without inspecting the bytes.
///
/// The struct is metadata-only in every sense except for the
/// `tiff_metadata.icc_profile` payload, which is intentionally
/// opaque because the persistence layer needs the actual ICC
/// bytes to populate the PNG `iCCP` chunk. The ICC bytes never
/// reach a log line and never appear in the diagnostic surface:
/// `Display` and `Debug` only render the counts and kind
/// identifiers.
#[derive(Debug, Clone)]
pub struct PasteboardImageMetadata {
    /// The chunk summary the PNG byte stream reported.
    pub png_metadata: crate::clipboard_image_png::PngMetadataSummary,
    /// The metadata the TIFF leg of the same pasteboard snapshot
    /// published. `None` when the pasteboard did not declare
    /// `public.tiff`.
    pub tiff_metadata: Option<crate::tiff_metadata::TiffMetadata>,
    /// Resolution inferred from the current macOS display scale when
    /// neither native pasteboard representation exposes usable
    /// resolution metadata. This is a last-resort host fallback,
    /// never a claim that the TIFF bytes carried this value.
    pub inferred_display_dpi: Option<(u32, u32)>,
    /// True when the bridge was able to detect resolution
    /// metadata on at least one of the two representations. The
    /// predicate combines [`PngMetadataSummary::preserves_resolution`]
    /// for the PNG leg and [`TiffMetadata::has_resolution`] for
    /// the TIFF leg into a single field the rest of the
    /// pipeline can read without inspecting either parser's
    /// payload.
    pub resolution_detected: bool,
    /// True when the bridge was able to detect a colour profile
    /// (`iCCP` for PNG, `ICCProfile` IFD tag for TIFF, or the PNG
    /// `sRGB` chunk). The diagnostic surface uses this boolean
    /// together with the resolution carrier to pinpoint which
    /// representation supplied which half of the fidelity
    /// puzzle.
    pub profile_detected: bool,
}

impl PasteboardImageMetadata {
    /// Stable snake_case identifier used by the diagnostic surface
    /// and by the `CLIPVAULT_DEBUG_IMAGE_*` environment contract.
    pub fn kind_str(&self) -> &'static str {
        if self.resolution_detected && self.profile_detected {
            "native_full_metadata"
        } else if self.resolution_detected {
            "native_resolution_only"
        } else if self.profile_detected {
            "native_profile_only"
        } else {
            "native_no_metadata"
        }
    }
}

/// Typed outcome of [`read_png_main_thread`].
///
/// The composite clipboard needs to distinguish every shape the
/// pasteboard can return so the right code path runs next:
///
/// - `Native(ClipboardImage)`: `public.png` was present and decoded
///   into a coherent bitmap. The bridge already attached the
///   original bytes to the image so the core can persist them
///   verbatim through
///   [`crate::clipboard_assets::normalize_image_with_original`].
///   This is the path that preserves `pHYs`, `iCCP` / `sRGB` and
///   any other safe metadata chunks the source application
///   published — including the 144 ppi Display P3 case the user
///   reported.
/// - `TiffOnly`: the pasteboard exposed a valid `public.tiff` image
///   but no `public.png` leg. The bridge decodes the TIFF on the
///   main thread and carries the RGBA pixels plus the TIFF metadata
///   into the core, so the capture is not degraded through
///   `arboard`.
/// - `NoPng`: the pasteboard had no usable native PNG or TIFF image
///   representation. The caller MAY fall back to the legacy
///   `arboard::get_image` bitmap path because there is no native
///   fidelity-preserving candidate to lose.
/// - `InvalidPng(PngValidationError)`: the pasteboard returned a
///   `public.png` representation but the bytes failed size,
///   dimension or decoder validation. The caller MUST NOT fall
///   back to `arboard::get_image`; doing so would silently save a
///   re-encoded PNG that lacks the metadata chunks the user
///   observed as missing. The composite must surface a typed
///   `InvalidImage` error so the capture pipeline can decide
///   between a hard failure and a retryable miss.
///
/// The bridge keeps the failure mode metadata-only: the bytes
/// themselves, the byte length and the absolute path never escape
/// the function.
#[derive(Debug)]
pub enum NativePngRead {
    /// `public.png` was present and decoded; the inner
    /// [`crate::clipboard::ClipboardImage`] carries the original bytes
    /// alongside the metadata the same pasteboard snapshot
    /// exposed on the `public.png` and `public.tiff` legs. The
    /// metadata block lets the persistence layer decide between
    /// verbatim persistence (`Native`), metadata-injection
    /// (`NativePngPlusMetadata`) and a documented
    /// `metadata_undetected` path.
    Native {
        image: crate::clipboard::ClipboardImage,
        metadata: PasteboardImageMetadata,
    },
    /// `public.tiff` was the only usable raster representation.
    /// `image` contains the decoded RGBA frame and no original PNG;
    /// `metadata` carries the TIFF resolution/profile for the core's
    /// metadata-preserving PNG rebuild.
    TiffOnly {
        image: crate::clipboard::ClipboardImage,
        metadata: PasteboardImageMetadata,
    },
    /// `public.png` was absent or non-PNG bytes and no usable TIFF
    /// raster could be decoded. The legacy `arboard::get_image`
    /// path is the final fallback; a valid TIFF-only capture uses
    /// [`NativePngRead::TiffOnly`] instead.
    NoPng { metadata: PasteboardImageMetadata },
    /// `public.png` was present but the bytes failed validation;
    /// the legacy bitmap path would silently degrade the capture.
    InvalidPng(crate::clipboard_image_png::PngValidationError),
}

impl NativePngRead {
    /// Stable snake_case identifier so callers can branch without
    /// parsing free-form text.
    pub fn kind_str(&self) -> &'static str {
        match self {
            NativePngRead::Native { image: _, metadata } => {
                if metadata.tiff_metadata.is_some() {
                    "native_png_with_tiff_metadata"
                } else {
                    "native_png"
                }
            }
            NativePngRead::TiffOnly { metadata, .. } => {
                if metadata.tiff_metadata.is_some() {
                    "tiff_only_with_metadata"
                } else {
                    "tiff_only"
                }
            }
            NativePngRead::NoPng { metadata } => {
                if metadata.tiff_metadata.is_some() {
                    "no_png_with_tiff_metadata"
                } else {
                    "no_png"
                }
            }
            NativePngRead::InvalidPng(error) => error.kind_str(),
        }
    }

    /// True when the outcome carries an image that the
    /// persistence layer can persist. The legacy `arboard`
    /// fallback path is the only other route to a capturable
    /// image; everything else collapses to a typed error.
    pub fn has_image(&self) -> bool {
        matches!(
            self,
            NativePngRead::Native { .. } | NativePngRead::TiffOnly { .. }
        )
    }
}

/// Read the `public.png` leg of `NSPasteboard` on the main thread.
///
/// The closure copies the raw PNG bytes — verbatim, with the `pHYs`
/// resolution chunk, the `iCCP` / `sRGB` color-profile chunk and any
/// other safe metadata chunks the source application published — and
/// hands them to the core. The platform layer validates the bytes
/// against the PNG signature, the size cap and the dimension cap
/// before constructing a [`crate::ClipboardImage`]; the core then
/// persists them verbatim through
/// [`crate::clipboard_assets::normalize_image_with_original`].
///
/// Returns:
///
/// - `Ok(NativePngRead::Native(image))` when `public.png` was
///   present and the bytes decoded into a coherent RGBA frame;
/// - `Ok(NativePngRead::TiffOnly)` when the clipboard has no
///   `public.png` flavour but does expose a valid `public.tiff`
///   bitmap. The TIFF is decoded on this same main-thread snapshot;
///   the caller must preserve the returned image and metadata;
/// - `Ok(NativePngRead::NoPng)` when the clipboard has no usable
///   native raster flavour (the caller may fall back to the
///   `arboard` bitmap path);
/// - `Ok(NativePngRead::InvalidPng(_))` when `public.png` was
///   present but the bytes failed validation (signature, size,
///   dimensions, decoder). The caller MUST NOT fall back to
///   `arboard::get_image` here: the legacy bitmap path silently
///   produces a canonical 8-bit RGBA PNG without `pHYs`, `iCCP` /
///   `sRGB` or any other safe metadata chunks, which is exactly
///   the regression the user reported.
/// - `Err(MainQueueBridgeError::DispatchUnavailable)` when the
///   bridge cannot hop to the main thread. The caller MAY fall back
///   to the legacy `arboard::get_image` path because the bridge
///   limitation is not a fidelity problem.
///
/// The bridge deliberately keeps the PNG validation in
/// [`crate::clipboard_image_png::validate_png`] instead of
/// reaching into the core: the platform layer must remain free of
/// core dependencies, and the validator is the only PNG fidelity
/// contract the bridge needs to honour.
///
/// In addition to `public.png` the bridge also inspects
/// `public.tiff` so the persistence layer can fall back to the
/// canonical Apple storage for the resolution / colour profile
/// when the PNG leg lacks a `pHYs` chunk or an embedded
/// `iCCP` profile. Both reads happen on the same main-thread
/// snapshot: each flavour is fetched from the same
/// `NSPasteboard::generalPasteboard()` instance inside one
/// dispatch so the two surfaces always describe the same
/// pasteboard state. The implementation never falls back to
/// `arboard::get_image` — the persistence layer receives a
/// typed outcome that can route into the legacy bitmap path on
/// its own, but the bridge never pretends to have a fidelity
/// candidate when no native representation is present.
pub fn read_png_main_thread() -> BridgeResult<NativePngRead> {
    use crate::clipboard::ClipboardImage;
    use crate::clipboard_image_png::{png_metadata_summary, validate_png, PngValidationOutcome};
    use crate::tiff_metadata::parse_tiff_metadata;
    use crate::MAX_CLIPBOARD_IMAGE_DIM;

    /// Snapshot the bridge reads from the pasteboard.
    struct Snapshot {
        png_bytes: Option<Vec<u8>>,
        tiff_bytes: Option<Vec<u8>>,
        tiff_image: Option<ClipboardImage>,
        /// AppKit's decoded TIFF representation is a second source of
        /// truth for the logical print size. Some Apple pasteboard
        /// producers expose 144 ppi through `NSBitmapImageRep::size()`
        /// while their raw TIFF IFD is incomplete or uses a form the
        /// narrow parser cannot interpret.
        appkit_dpi: Option<(u32, u32)>,
        display_scale: Option<f64>,
        io_failure: bool,
    }

    let snapshot: Snapshot = dispatch_main_thread(|mtm| {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeTIFF};
        use objc2_foundation::NSString;

        let pasteboard = NSPasteboard::generalPasteboard();

        let png_bytes =
            read_pasteboard_bytes(pasteboard.dataForType(unsafe { NSPasteboardTypePNG }));
        let tiff_bytes =
            read_pasteboard_bytes(pasteboard.dataForType(unsafe { NSPasteboardTypeTIFF }));

        // Also probe the legacy `public.png` / `public.tiff`
        // string flavours because some hosts (older launchd or
        // unusual pasteboard managers) forget to declare the
        // canonical AppKit types. The probe is best-effort: any
        // failure leaves the canonical read in place.
        let png_type_string = NSString::from_str("public.png");
        let tiff_type_string = NSString::from_str("public.tiff");
        let png_bytes =
            png_bytes.or_else(|| read_pasteboard_bytes(pasteboard.dataForType(&png_type_string)));
        let tiff_bytes =
            tiff_bytes.or_else(|| read_pasteboard_bytes(pasteboard.dataForType(&tiff_type_string)));
        let tiff_image = tiff_bytes.as_deref().and_then(decode_tiff_rgba);

        let appkit_dpi = tiff_bytes.as_deref().and_then(appkit_tiff_resolution_dpi);
        let display_scale = display_scale_for_snapshot(mtm);

        Snapshot {
            png_bytes,
            tiff_bytes,
            tiff_image,
            appkit_dpi,
            display_scale,
            io_failure: false,
        }
    })?;

    if snapshot.io_failure {
        return Err(MainQueueBridgeError::DispatchUnavailable);
    }

    // Build the metadata block regardless of whether the PNG leg
    // validated. The persistence layer needs the TIFF metadata
    // even when the PNG leg is the only pixel source available.
    let png_summary = snapshot
        .png_bytes
        .as_deref()
        .map(png_metadata_summary)
        .unwrap_or_default();
    let tiff_metadata = snapshot
        .tiff_bytes
        .as_deref()
        .map(|bytes| {
            let mut metadata = parse_tiff_metadata(bytes);
            if let Some((dpi_x, dpi_y)) = snapshot.appkit_dpi {
                // AppKit has already interpreted the same TIFF as an
                // image representation. Prefer that result when it
                // disagrees with an incomplete IFD: the user-visible
                // ppi is the contract we must preserve in the PNG.
                let parser_dpi = metadata
                    .macos_pixels_per_meter_x()
                    .zip(metadata.macos_pixels_per_meter_y());
                let appkit_ppm = dpi_to_pixels_per_meter(dpi_x).zip(dpi_to_pixels_per_meter(dpi_y));
                if parser_dpi != appkit_ppm {
                    metadata.x_resolution = Some((dpi_x, 1));
                    metadata.y_resolution = Some((dpi_y, 1));
                }
            }
            metadata
        })
        .filter(|metadata| metadata != &crate::tiff_metadata::TiffMetadata::default());
    let png_default_72 = png_summary
        .phys
        .and_then(|payload| crate::clipboard_image_png::parse_ppu_triple(&payload))
        .and_then(|(ppu_x, ppu_y, unit)| {
            crate::clipboard_image_png::phys_to_dpi(ppu_x, ppu_y, unit)
        })
        == Some((72, 72));
    let tiff_has_complete_resolution = tiff_metadata.as_ref().is_some_and(|metadata| {
        metadata
            .macos_pixels_per_meter_x()
            .zip(metadata.macos_pixels_per_meter_y())
            .is_some()
    });
    let inferred_display_dpi = if !tiff_has_complete_resolution
        && (!png_summary.preserves_resolution() || png_default_72)
    {
        // Prefer the logical size AppKit derived from the TIFF
        // itself. This covers Apple TIFFs whose IFD omits or hides
        // the resolution tags even though Preview still reports
        // 144 ppi. Only when ImageIO cannot provide that value do we
        // use the bounded display-scale fallback.
        snapshot
            .appkit_dpi
            .or_else(|| snapshot.display_scale.and_then(display_scale_to_dpi))
    } else {
        None
    };
    let resolution_detected = png_summary.preserves_resolution()
        || tiff_metadata.as_ref().is_some_and(|m| m.has_resolution())
        || inferred_display_dpi.is_some();
    let profile_detected = png_summary.icc_profile_chunk.is_some()
        || png_summary.has_srgb
        || tiff_metadata
            .as_ref()
            .is_some_and(|m| m.icc_profile.is_some());
    let metadata = PasteboardImageMetadata {
        png_metadata: png_summary,
        tiff_metadata,
        inferred_display_dpi,
        resolution_detected,
        profile_detected,
    };

    let Some(bytes) = snapshot.png_bytes else {
        if let Some(image) = snapshot.tiff_image {
            return Ok(NativePngRead::TiffOnly { image, metadata });
        }
        return Ok(NativePngRead::NoPng { metadata });
    };
    match validate_png(&bytes, MAX_CLIPBOARD_IMAGE_DIM) {
        PngValidationOutcome::Valid(validated) => {
            let width = validated.width;
            let height = validated.height;
            let rgba = validated.rgba;
            match ClipboardImage::with_original_png(rgba, width, height, bytes) {
                Ok(image) => Ok(NativePngRead::Native { image, metadata }),
                // The validator produced a coherent RGBA frame;
                // if the structural constructor still rejects
                // the bitmap the safe answer is to surface the
                // failure so the caller does not silently fall
                // back to `arboard::get_image`. The only failure
                // the constructor can produce at this point is a
                // stride mismatch (impossible: the validator
                // already enforced the same length) or a zero
                // dimension, both of which map to
                // `InvalidDimensions` so the composite surfaces a
                // typed error instead of silently degrading.
                Err(_) => Ok(NativePngRead::InvalidPng(
                    crate::clipboard_image_png::PngValidationError::InvalidDimensions {
                        width,
                        height,
                    },
                )),
            }
        }
        PngValidationOutcome::NotPng => Ok(NativePngRead::NoPng { metadata }),
        PngValidationOutcome::Invalid(error) => Ok(NativePngRead::InvalidPng(error)),
    }
}

/// Decode the macOS pasteboard's TIFF raster into the platform's
/// canonical top-left RGBA buffer without changing its pixel
/// dimensions.
///
/// `⌘⇧4` followed by the screenshot UI's Copy action can publish
/// `public.tiff` without a usable `public.png` leg. Calling
/// `arboard::get_image()` for that shape loses the TIFF resolution
/// and profile metadata and, on some macOS versions, returns a
/// different bitmap representation. AppKit already owns a faithful
/// TIFF/ImageIO decoder, so use `NSBitmapImageRep` only inside the
/// main-thread bridge, draw one-to-one into a fresh RGBA rep, and
/// return the full frame to the core together with the TIFF metadata.
///
/// The function is intentionally total: malformed TIFF data,
/// unsupported codecs, invalid dimensions, non-contiguous target
/// storage or a failed AppKit drawing operation return `None`, which
/// lets the caller use the documented legacy fallback only when no
/// native raster could actually be recovered.
fn decode_tiff_rgba(tiff_bytes: &[u8]) -> Option<crate::clipboard::ClipboardImage> {
    use objc2_app_kit::{
        NSBitmapImageRep, NSDeviceRGBColorSpace, NSGraphicsContext, NSImageInterpolation,
    };
    use objc2_core_graphics::CGContext;
    use objc2_foundation::{NSData, NSPoint, NSRect, NSSize};

    if tiff_bytes.is_empty() {
        return None;
    }
    let data = NSData::with_bytes(tiff_bytes);
    let source = NSBitmapImageRep::imageRepWithData(&data)?;
    let width = u32::try_from(source.pixelsWide()).ok()?;
    let height = u32::try_from(source.pixelsHigh()).ok()?;
    let expected = crate::clipboard::checked_rgba_len(width, height).ok()?;
    let stride = width.checked_mul(4)?;
    let stride_isize = isize::try_from(stride).ok()?;

    // Allocate an explicit non-planar 8-bit RGBA target. Using the
    // same representation shape as the PNG/TIFF writer keeps the
    // byte order stable for the copy below and avoids handing an
    // AppKit-owned NSImage cache to the persistence layer.
    let target = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            width as isize,
            height as isize,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            stride_isize,
            32,
        )
    }?;

    let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&target)?;
    let cg_context = context.CGContext();
    let cg_image = source.CGImage()?;
    let previous = NSGraphicsContext::currentContext();
    NSGraphicsContext::setCurrentContext(Some(&context));
    context.saveGraphicsState();
    // The source and target dimensions are identical. Disabling
    // interpolation makes the one-to-one copy explicit and avoids
    // introducing a resampling step into a capture that must retain
    // the complete original raster.
    context.setImageInterpolation(NSImageInterpolation::None);
    // `NSBitmapImageRep::bitmapData()` exposes rows in the same
    // top-to-bottom order used by the platform RGBA contract. The
    // AppKit representation already accounts for the source image
    // orientation; applying the common Core Graphics Y-flip here
    // would reverse the captured rows (and was observable in the
    // non-square regression fixture).
    let rect = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(width as f64, height as f64),
    );
    CGContext::draw_image(Some(&cg_context), rect, Some(&cg_image));
    context.restoreGraphicsState();
    NSGraphicsContext::setCurrentContext(previous.as_deref());

    let bytes_per_row = target.bytesPerRow();
    if bytes_per_row < stride_isize {
        return None;
    }
    let bitmap_data = target.bitmapData();
    if bitmap_data.is_null() {
        return None;
    }
    let row_stride = usize::try_from(bytes_per_row).ok()?;
    let mut rgba = vec![0u8; expected];
    for row in 0..height as usize {
        // SAFETY: AppKit owns a live bitmap buffer with at least
        // `row_stride * height` bytes; each source row and destination
        // row is bounded by the checked dimensions above.
        unsafe {
            std::ptr::copy_nonoverlapping(
                bitmap_data.add(row.checked_mul(row_stride)?),
                rgba.as_mut_ptr().add(row.checked_mul(stride as usize)?),
                stride as usize,
            );
        }
    }

    crate::clipboard::ClipboardImage::new(rgba, width, height).ok()
}

/// Ask AppKit/ImageIO for the logical resolution of the TIFF
/// representation. The raw TIFF parser is intentionally narrow and
/// metadata-only, but AppKit has already decoded the pasteboard
/// representation and therefore knows the logical point size that
/// Preview uses when it reports ppi.
///
/// `NSBitmapImageRep::size()` is measured in points. The standard
/// AppKit relationship is therefore `pixels / points * 72`. The
/// helper returns only bounded integers; it never returns the TIFF
/// bytes, a bitmap, a path or a profile.
#[cfg(all(target_os = "macos", feature = "macos-native"))]
fn appkit_tiff_resolution_dpi(tiff_bytes: &[u8]) -> Option<(u32, u32)> {
    use objc2_app_kit::NSBitmapImageRep;
    use objc2_foundation::NSData;

    let data = NSData::with_bytes(tiff_bytes);
    let representation = NSBitmapImageRep::imageRepWithData(&data)?;
    let pixels_wide = representation.pixelsWide();
    let pixels_high = representation.pixelsHigh();
    let logical_size = representation.size();
    if pixels_wide <= 0
        || pixels_high <= 0
        || !logical_size.width.is_finite()
        || !logical_size.height.is_finite()
        || logical_size.width <= 0.0
        || logical_size.height <= 0.0
    {
        return None;
    }

    let dpi_x = pixels_wide as f64 / logical_size.width * 72.0;
    let dpi_y = pixels_high as f64 / logical_size.height * 72.0;
    Some((rounded_positive_dpi(dpi_x)?, rounded_positive_dpi(dpi_y)?))
}

#[cfg(all(target_os = "macos", feature = "macos-native"))]
fn rounded_positive_dpi(value: f64) -> Option<u32> {
    if !value.is_finite() || value <= 0.0 || value > u32::MAX as f64 {
        return None;
    }
    Some(value.round() as u32)
}

#[cfg(all(target_os = "macos", feature = "macos-native"))]
fn dpi_to_pixels_per_meter(dpi: u32) -> Option<u32> {
    // 100 / 2.54 = 5000 / 127 exactly. Keep the same rounded
    // integer conversion as the TIFF compatibility path.
    let numerator = (dpi as u64).checked_mul(5000)?;
    let denominator = 127u64;
    let ppm = numerator.checked_add(denominator / 2)? / denominator;
    u32::try_from(ppm).ok()
}

/// Convert a bounded AppKit backing scale into the conventional
/// screen-capture resolution. macOS screen captures use 72 points
/// per logical inch, so a 2x Retina backing scale corresponds to
/// 144 ppi. The value is used only when the pasteboard omitted
/// resolution metadata entirely (or exposed only the generic 72 ppi
/// PNG default); an explicit TIFF value and a non-generic PNG value
/// win before this helper is consulted.
#[cfg(all(target_os = "macos", feature = "macos-native"))]
fn display_scale_to_dpi(scale: f64) -> Option<(u32, u32)> {
    if !scale.is_finite() || !(1.0..=4.0).contains(&scale) {
        return None;
    }
    let dpi = rounded_positive_dpi(scale * 72.0)?;
    Some((dpi, dpi))
}

/// Resolve a usable display scale for the metadata fallback.
///
/// `mainScreen` can transiently be unavailable while AppKit is
/// finishing screen discovery during application startup. It can
/// also report the basic 1x scale while another active screen is
/// already exposed through `screens`. Prefer a valid main-screen
/// value above 1x, otherwise inspect the available screens and use
/// the greatest bounded scale. The caller only uses this value when
/// the pasteboard supplied no usable resolution of its own.
#[cfg(all(target_os = "macos", feature = "macos-native"))]
fn display_scale_for_snapshot(mtm: MainThreadMarker) -> Option<f64> {
    use objc2_app_kit::NSScreen;

    let main_scale = NSScreen::mainScreen(mtm).map(|screen| screen.backingScaleFactor());
    if main_scale.is_some_and(|scale| scale.is_finite() && scale > 1.0) {
        return main_scale;
    }

    let screens = NSScreen::screens(mtm);
    let mut best = main_scale.filter(|scale| scale.is_finite() && *scale >= 1.0);
    for index in 0..screens.count() {
        // SAFETY: `screens` is an immutable AppKit-owned NSArray and
        // `index` is bounded by its current count.
        let screen = unsafe { screens.objectAtIndex_unchecked(index) };
        let scale = screen.backingScaleFactor();
        if !scale.is_finite() || !(1.0..=4.0).contains(&scale) {
            continue;
        }
        if best.is_none_or(|current| scale > current) {
            best = Some(scale);
        }
    }
    best
}

/// Read a pasteboard flavour's bytes off the AppKit `NSData`. Returns
/// `None` when the pasteboard does not declare that flavour, when
/// the data is empty, or when the inner pasteboard allocation
/// fails. The helper is total on purpose: every soft failure mode
/// collapses to `None` so the bridge can decide between
/// "no usable representation" and "represented but invalid".
fn read_pasteboard_bytes(data: Option<Retained<objc2_foundation::NSData>>) -> Option<Vec<u8>> {
    let data = data?;
    let length = data.length();
    if length == 0 {
        return None;
    }
    let mut out: Vec<u8> = Vec::with_capacity(length);
    // SAFETY: `getBytes_length` copies `length` bytes into the
    // caller-provided buffer. The buffer must remain valid for
    // `length` bytes; `Vec`'s spare capacity is exactly that.
    unsafe {
        let dst = std::ptr::NonNull::new_unchecked(out.as_mut_ptr().cast::<std::ffi::c_void>());
        data.getBytes_length(dst, length);
        out.set_len(length);
    }
    Some(out)
}

/// Write plain text to `NSPasteboard` on the main thread.
///
/// The closure first calls `clearContents()` to drop every existing
/// flavour (`public.html`, `public.rtf`, the previous capture's rich
/// bytes, ...) and then declares only the canonical plain-text
/// flavours before publishing the text. This is the documented
/// contract of `PasteMode::Plain`: the resulting pasteboard carries
/// no rich payload, so a destination rich editor that re-applies
/// the cursor's typing attributes receives the new text without any
/// residual colour or font hint from the previous rich capture.
///
/// Mirrors `read_plain_text_main_thread`: the closure publishes
/// `public.utf8-plain-text` (the canonical UTF-8 flavour) and the
/// legacy `NSPasteboardTypeString` flavour in one logical write so
/// strict consumers that only watch the canonical flavour still
/// receive the text.
///
/// Returns `Err(MainQueueBridgeError::DispatchUnavailable)` when the
/// bridge cannot hop to the main thread.
pub fn write_plain_text_main_thread(text: String) -> BridgeResult<()> {
    let succeeded: bool = dispatch_main_thread(move |_mtm| {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
        use objc2_foundation::{NSArray, NSString};

        let pasteboard = NSPasteboard::generalPasteboard();
        let ns_text = NSString::from_str(&text);
        let utf8_type = NSString::from_str("public.utf8-plain-text");
        let string_type = unsafe { NSPasteboardTypeString };

        // Plain write MUST NOT keep any rich flavour from a previous
        // capture. `clearContents` drops every declared type
        // (`public.html`, `public.rtf`, `public.utf8-plain-text`,
        // ...) and increments the change count so the watcher can
        // detect the new paste. The `declareTypes` step is what
        // makes the contract explicit: we promise only the canonical
        // plain-text flavours and nothing else.
        //
        // `NSPasteboardTypeString` and `public.utf8-plain-text` are
        // the same UTI on every supported SDK; declaring both keeps
        // the contract visible regardless of which constant the
        // consumer asks for. When the two identifiers are equal
        // (current SDK) the array collapses to a single entry.
        let mut declared_types: Vec<&NSString> = Vec::new();
        if !string_type.isEqualToString(&utf8_type) {
            declared_types.push(string_type);
        }
        declared_types.push(&*utf8_type);
        let type_array = NSArray::from_slice(&declared_types);
        let _ = pasteboard.clearContents();
        let _ = unsafe { pasteboard.declareTypes_owner(&type_array, None) };

        // The plain-text publish is a single `setString` call per
        // declared type. Returning `false` here means the OS
        // rejected the write: the bridge surfaces that as
        // `DispatchUnavailable` so the caller can fall back to the
        // plain adapter instead of producing a half-written
        // pasteboard.
        pasteboard.setString_forType(&ns_text, string_type)
            && pasteboard.setString_forType(&ns_text, &utf8_type)
    })?;
    if succeeded {
        Ok(())
    } else {
        Err(MainQueueBridgeError::DispatchUnavailable)
    }
}

/// Translate a bridge error into the typed
/// `ClipboardBackendError::Unavailable` the rest of the pipeline
/// observes. The capability identifier is the only thing the rest of
/// the pipeline sees — the underlying thread error never escapes.
pub fn unavailable_for(capability: Capability) -> ClipboardBackendError {
    ClipboardBackendError::Unavailable { capability }
}

/// Encode the supplied RGBA bitmap into the canonical macOS image
/// representations. The bridge publishes BOTH `public.png` and
/// `NSPasteboardTypeTIFF` so receiving apps can pick the leg they
/// natively support without AppKit auto-deriving a smaller bitmap.
///
/// Returns `(Some(png_bytes, tiff_bytes))` on success, `(None, None)`
/// when the encoder rejects the input. The helper is the only writer
/// for the two encodings — the pasteboard hop is a thin wrapper
/// around it so the boundary tests can exercise the encoding without
/// a running Cocoa main loop.
///
/// Why both PNG and TIFF:
///
/// - `public.png` is the modern lossless flavour. Most modern
///   macOS apps (TextEdit, Notes, Preview when used as a destination,
///   ...) prefer it and read the canonical RGBA buffer back
///   verbatim.
/// - `NSPasteboardTypeTIFF` is the bitmap fallback macOS has shipped
///   since the Classic era. Apps that walk the pasteboard in the
///   historical order (`TIFF` before `PNG`) reach for the TIFF leg
///   first; if the pasteboard only declares `PNG`, those apps
///   either render a downscaled preview or refuse the payload
///   altogether. The previous one-representation publish was the
///   failure the user-visible regression surfaced: PNG preserved
///   every pixel but the receiving app interpreted the pasteboard
///   as carrying a downsampled bitmap because no TIFF leg existed.
///
/// The encoder never routes through `NSImage` or its representation
/// cache — both encodings come from the same `NSBitmapImageRep` we
/// allocate from the supplied RGBA bytes so AppKit cannot quietly
/// resize the bitmap.
pub(crate) fn encode_image_representations(
    width: u32,
    height: u32,
    rgba: &[u8],
) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSDeviceRGBColorSpace};
    use objc2_foundation::{NSData, NSDictionary, NSString};

    if width == 0 || height == 0 {
        return (None, None);
    }
    let stride = (width as usize) * 4;
    let expected = stride * (height as usize);
    if rgba.len() != expected {
        // A short or oversized buffer would corrupt the bitmap and
        // either truncate the publish or panic inside AppKit. The
        // adapter is total: it never accepts a malformed input.
        return (None, None);
    }

    // SAFETY: the bitmap data is freshly allocated, exactly
    // `width * height * 4` bytes long, and lives for the duration
    // of the closure. `planes` is null because we copy the bytes
    // through `bitmapData()` immediately after construction —
    // letting AppKit allocate the buffer keeps the ownership
    // rules simple and avoids the need to keep the source buffer
    // alive beyond the closure.
    let bitmap: Option<Retained<NSBitmapImageRep>> = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            width as isize,
            height as isize,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            stride as isize,
            32,
        )
    };
    let Some(bitmap) = bitmap else {
        return (None, None);
    };
    // SAFETY: `bitmapData()` returns a mutable pointer to the
    // rep's pixel buffer. The buffer is exactly `stride * height`
    // bytes long (the rep owns its storage) and is exclusively
    // referenced through this raw pointer until the rep returns to
    // the pasteboard.
    let copied = unsafe {
        let dst = bitmap.bitmapData();
        if dst.is_null() {
            false
        } else {
            std::ptr::copy_nonoverlapping(rgba.as_ptr(), dst.cast::<u8>(), expected);
            true
        }
    };
    if !copied {
        return (None, None);
    }

    // Properties dictionary is empty: `NSBitmapImageFileType::PNG`
    // ignores the lossy compression factor and produces a lossless
    // byte stream, and `NSBitmapImageFileType::TIFF` likewise
    // encodes the bitmap verbatim. Any non-empty dictionary would
    // invite AppKit to interpret the rep differently (and would be
    // the documented regression source).
    let empty_props: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::new();

    let png_data: Option<Retained<NSData>> = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &empty_props)
    };
    let tiff_data: Option<Retained<NSData>> = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::TIFF, &empty_props)
    };

    let png_bytes = png_data.map(|data| data.to_vec());
    let tiff_bytes = tiff_data.map(|data| data.to_vec());
    (png_bytes, tiff_bytes)
}

/// Write a raster image to `NSPasteboard` on the main thread.
///
/// The bridge encodes the supplied RGBA bitmap into **both** PNG
/// (`public.png`) and TIFF (`NSPasteboardTypeTIFF`) representations
/// through `NSBitmapImageRep` and publishes them through
/// `pasteboard.setData_forType`. The dual-leg publish was added
/// because the `arboard`-based image writer used to ride on
/// `pasteboard.writeObjects(&[NSImage])`, which delegates the
/// representation to AppKit and can return `true` while the
/// pasteboard stores only a downscaled thumbnail of the original
/// bitmap. Manual round-trips on macOS showed that 17×9 / 31×13
/// test images were stored as a partial slice — every destination
/// app received only the top-left corner of the asset.
///
/// The native PNG path skips AppKit's representation caching: the
/// encoder runs once on the main thread, the `NSData` payload is
/// handed to the pasteboard verbatim, and every consumer that
/// honours `public.png` reads the canonical bitmap back. The TIFF
/// leg guarantees that legacy bitmap consumers receive a
/// representation they recognise without AppKit auto-deriving a
/// smaller bitmap. The closure declares both types so a previous
/// rich / text flavour from another capture cannot pollute the new
/// pasteboard.
///
/// Returns `Err(MainQueueBridgeError::DispatchUnavailable)` when
/// the bridge cannot hop to the main thread.
pub fn write_image_main_thread(width: u32, height: u32, rgba: Vec<u8>) -> BridgeResult<()> {
    let publish_succeeded: bool = dispatch_main_thread(move |_mtm| {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeTIFF};
        use objc2_foundation::{NSArray, NSData, NSString};

        let (png_bytes, tiff_bytes) = encode_image_representations(width, height, &rgba);
        let png_bytes = match png_bytes {
            Some(bytes) => bytes,
            None => return false,
        };
        let tiff_bytes = match tiff_bytes {
            Some(bytes) => bytes,
            None => return false,
        };

        // Use `Vec::into_boxed_slice` so the `NSData` borrows from a
        // stable allocation for the duration of the publish. The
        // `pasteboard.setData_forType` call copies the bytes
        // synchronously into the pasteboard's storage; after that
        // the slice can be dropped.
        let png_ns = NSData::with_bytes(png_bytes.as_slice());
        let tiff_ns = NSData::with_bytes(tiff_bytes.as_slice());

        let pasteboard = NSPasteboard::generalPasteboard();
        let png_type_str = NSString::from_str("public.png");
        // The canonical UTI string for the bitmap fallback. Apple's
        // exported `NSPasteboardTypeTIFF` is the documented constant
        // but the value itself is `"public.tiff"`; declaring it
        // verbatim keeps the pasteboard identifier byte-stable across
        // SDK upgrades.
        let tiff_type_str = NSString::from_str("public.tiff");
        let declared_types: Vec<&NSString> = vec![&png_type_str, &tiff_type_str];
        let type_array = NSArray::from_slice(&declared_types);
        let _ = pasteboard.clearContents();
        let _ = unsafe { pasteboard.declareTypes_owner(&type_array, None) };

        // Both legs must be set: a partial publish would leave the
        // pasteboard carrying only one representation, which is
        // exactly the failure mode the dual-leg publish guards
        // against. The two `setData_forType` calls return `true` on
        // success; a single `false` collapses the whole publish to a
        // failure so the caller surfaces a typed capability error.
        let png_ok = unsafe { pasteboard.setData_forType(Some(&png_ns), NSPasteboardTypePNG) };
        let tiff_ok = unsafe { pasteboard.setData_forType(Some(&tiff_ns), NSPasteboardTypeTIFF) };
        png_ok && tiff_ok
    })?;
    if publish_succeeded {
        Ok(())
    } else {
        Err(MainQueueBridgeError::DispatchUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bridge type is `Send + Sync` because the wrapper carries no
    /// non-`Send` state — every interaction with `NSPasteboard`
    /// happens inside the dispatched closure, which is required to
    /// run on the main thread.
    #[test]
    fn bridge_types_are_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<MainQueueBridgeError>();
        assert_sync::<MainQueueBridgeError>();
    }

    /// The capability translation is the only surface the rest of the
    /// pipeline observes when the bridge fails to hop; the error
    /// variant must carry the requested capability, never the
    /// underlying thread error.
    #[test]
    fn unavailable_for_carries_the_requested_capability() {
        let error = unavailable_for(Capability::ClipboardReadRichText);
        match error {
            ClipboardBackendError::Unavailable { capability } => {
                assert_eq!(capability, Capability::ClipboardReadRichText);
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    /// The bridge error has a stable snake_case identifier so the
    /// diagnostics endpoint can surface it without parsing free-form
    /// strings.
    #[test]
    fn bridge_error_kind_string_is_stable() {
        assert_eq!(
            MainQueueBridgeError::DispatchUnavailable.kind_str(),
            "main_queue_unavailable"
        );
    }

    /// PNG signature every conforming file starts with — `89 50 4E
    /// 47 0D 0A 1A 0A`. Used by the boundary tests to confirm the
    /// adapter really produced a PNG (and not, for example, an empty
    /// byte stream the pasteboard silently accepted).
    const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    /// TIFF byte-order markers. The Intel order (`II` for little-
    /// endian) is what AppKit emits on Apple silicon; the Motorola
    /// order (`MM` for big-endian) is the legacy PowerPC marker. A
    /// valid TIFF starts with one of the two.
    const TIFF_LE_MARKER: &[u8; 2] = b"II";
    const TIFF_BE_MARKER: &[u8; 2] = b"MM";

    /// Build a non-square RGBA buffer where every pixel is uniquely
    /// derived from its `(x, y)` coordinates so a partial / cropped
    /// / downscaled copy would diverge from the source at the first
    /// sampled pixel.
    fn distinct_pixels(width: u32, height: u32) -> Vec<u8> {
        let mut buffer = Vec::with_capacity((width as usize) * (height as usize) * 4);
        for y in 0..height {
            for x in 0..width {
                buffer.push((x & 0xFF) as u8);
                buffer.push((x.wrapping_add(y) & 0xFF) as u8);
                buffer.push((y & 0xFF) as u8);
                buffer.push(0xFF);
            }
        }
        buffer
    }

    /// Boundary test against the real macOS encoder: the PNG leg
    /// the bridge publishes MUST start with the canonical PNG
    /// signature, MUST preserve the original `width` and `height`,
    /// and MUST contain enough bytes for a non-trivial image. A
    /// downscaled or cropped bitmap would fail the dimension check.
    /// The helper bypasses the main-thread pasteboard hop so the
    /// assertion exercises the AppKit encoder without needing a
    /// running Cocoa event loop.
    #[test]
    fn encode_image_representations_publishes_png_with_original_dimensions() {
        let width = 17_u32;
        let height = 9_u32;
        let buffer = distinct_pixels(width, height);

        let (png, tiff) = encode_image_representations(width, height, &buffer);
        let png = png.expect("PNG representation must be present");
        let tiff = tiff.expect("TIFF representation must be present");

        assert!(
            png.len() >= 8,
            "PNG representation must carry at least the signature, got {} bytes",
            png.len(),
        );
        assert_eq!(
            &png[..8],
            &PNG_SIGNATURE,
            "PNG representation must start with the canonical PNG signature",
        );
        // IHDR is the first chunk after the signature; bytes 16..20
        // are the width and 20..24 are the height, both stored as
        // big-endian `u32`. The dimensions are what the receiving
        // app would otherwise render as a partial bitmap.
        assert_eq!(
            u32::from_be_bytes([png[16], png[17], png[18], png[19]]),
            width,
            "PNG width must match the source bitmap",
        );
        assert_eq!(
            u32::from_be_bytes([png[20], png[21], png[22], png[23]]),
            height,
            "PNG height must match the source bitmap",
        );

        // TIFF byte-order marker.
        let marker = &tiff[..2];
        assert!(
            marker == TIFF_LE_MARKER || marker == TIFF_BE_MARKER,
            "TIFF representation must start with a valid byte-order marker, got {marker:?}",
        );
        // The TIFF magic number `42` (little-endian: 0x2A 0x00 at
        // offset 2; big-endian: 0x00 0x2A at offset 2) confirms the
        // blob is a real TIFF, not an empty payload.
        let magic = if marker == TIFF_LE_MARKER.as_slice() {
            u16::from_le_bytes([tiff[2], tiff[3]])
        } else {
            u16::from_be_bytes([tiff[2], tiff[3]])
        };
        assert_eq!(
            magic, 42,
            "TIFF representation must declare the canonical magic number"
        );
    }

    /// Wider geometry (31×13) — proves the encoder preserves the
    /// full bitmap on every aspect ratio, not just one. The same
    /// PNG / TIFF magic checks pin the contract on a different
    /// shape; a downscaler would surface as a smaller dimension
    /// in the PNG IHDR chunk.
    #[test]
    fn encode_image_representations_publishes_png_with_wider_dimensions() {
        let width = 31_u32;
        let height = 13_u32;
        let buffer = distinct_pixels(width, height);

        let (png, tiff) = encode_image_representations(width, height, &buffer);
        let png = png.expect("PNG representation must be present");
        let tiff = tiff.expect("TIFF representation must be present");

        assert_eq!(png[..8], PNG_SIGNATURE);
        assert_eq!(
            u32::from_be_bytes([png[16], png[17], png[18], png[19]]),
            width,
        );
        assert_eq!(
            u32::from_be_bytes([png[20], png[21], png[22], png[23]]),
            height,
        );
        assert!(tiff.len() > 8, "TIFF representation must be non-trivial");
    }

    /// Boundary test against the encoder: a buffer that does not
    /// match the declared geometry MUST be rejected by the encoder
    /// instead of producing a truncated or malformed PNG / TIFF.
    /// The adapter is total: a malformed input collapses to
    /// `(None, None)` so the bridge surfaces a typed capability
    /// error to the caller.
    #[test]
    fn encode_image_representations_rejects_buffer_length_mismatch() {
        let buffer = vec![0u8; 8]; // Way smaller than 17*9*4 = 612 bytes.
        let (png, tiff) = encode_image_representations(17, 9, &buffer);
        assert!(
            png.is_none() && tiff.is_none(),
            "a mismatched buffer MUST collapse to (None, None) so the bridge surfaces a typed error",
        );
    }

    /// Boundary test: zero dimensions MUST be rejected. AppKit
    /// panics on a zero-area bitmap and we want the adapter to
    /// collapse the input to a typed capability error before
    /// crossing into AppKit.
    #[test]
    fn encode_image_representations_rejects_zero_dimensions() {
        let (png_zero_w, tiff_zero_w) = encode_image_representations(0, 9, &[]);
        assert!(png_zero_w.is_none() && tiff_zero_w.is_none());
        let (png_zero_h, tiff_zero_h) = encode_image_representations(17, 0, &[]);
        assert!(png_zero_h.is_none() && tiff_zero_h.is_none());
    }

    /// Decode the supplied PNG bytes and return the raw RGBA frame.
    /// The helper is local to the test module so the boundary
    /// assertions can verify the encoder produced a fully-decodable
    /// image (not just a syntactically-valid IHDR chunk). A cropped
    /// or downscaled encoder would surface here as either a smaller
    /// decoded frame or a pixel that diverges from the source at the
    /// first sampled `(x, y)` coordinate.
    fn decode_png_rgba(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().expect("png decode info");
        let info = reader.info();
        let (width, height) = (info.width, info.height);
        let mut buffer = vec![0u8; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut buffer).expect("png decode frame");
        buffer.truncate(frame.buffer_size());
        // The encoder publishes 8-bit RGBA; the test feeds RGBA, so
        // any other colour type would indicate a regression in the
        // encoder parameters.
        assert_eq!(frame.color_type, png::ColorType::Rgba);
        (width, height, buffer)
    }

    /// Boundary test that decodes the PNG the bridge publishes and
    /// compares every pixel byte-for-byte against the source RGBA
    /// buffer. A partial / cropped / downscaled copy would diverge
    /// at the first sampled pixel; this assertion exercises the
    /// adapter directly (no Core fake backend) so the regression
    /// the user surfaced on macOS cannot silently come back.
    #[test]
    fn encode_image_representations_preserves_full_pixel_buffer() {
        let width = 17_u32;
        let height = 9_u32;
        let source = distinct_pixels(width, height);

        let (png, _tiff) = encode_image_representations(width, height, &source);
        let png = png.expect("PNG representation must be present");

        let (decoded_w, decoded_h, decoded_rgba) = decode_png_rgba(&png);
        assert_eq!(decoded_w, width, "decoded PNG width must match");
        assert_eq!(decoded_h, height, "decoded PNG height must match");
        // Every pixel survives the encoder round-trip byte-for-byte.
        // A downscaled or cropped copy would perturb at least one
        // channel of at least one pixel.
        assert_eq!(
            decoded_rgba.len(),
            source.len(),
            "decoded RGBA buffer must be the same length as the source",
        );
        assert_eq!(
            decoded_rgba, source,
            "every pixel must survive the round-trip through the AppKit encoder",
        );
    }

    /// Same guarantee for a wider aspect ratio (31×13) — proves the
    /// encoder preserves the full buffer on every geometry the user
    /// may capture, not just one. The coordinate-derived pixel
    /// pattern makes a single-bit drift visible at the very first
    /// mismatching pixel.
    #[test]
    fn encode_image_representations_preserves_wider_pixel_buffer() {
        let width = 31_u32;
        let height = 13_u32;
        let source = distinct_pixels(width, height);

        let (png, _tiff) = encode_image_representations(width, height, &source);
        let png = png.expect("PNG representation must be present");

        let (decoded_w, decoded_h, decoded_rgba) = decode_png_rgba(&png);
        assert_eq!(decoded_w, width);
        assert_eq!(decoded_h, height);
        assert_eq!(decoded_rgba.len(), source.len());
        assert_eq!(
            decoded_rgba, source,
            "the wider image must round-trip through the encoder without losing any pixel",
        );
    }

    /// A screenshot copied by the macOS screenshot UI may expose
    /// only TIFF. The native bridge must decode that leg without
    /// resizing or cropping it before the core persists it.
    #[test]
    fn decode_tiff_rgba_preserves_full_pixel_buffer_and_dimensions() {
        let width = 17_u32;
        let height = 9_u32;
        let source = distinct_pixels(width, height);
        let (_png, tiff) = encode_image_representations(width, height, &source);
        let tiff = tiff.expect("TIFF representation must be present");

        let image = decode_tiff_rgba(&tiff).expect("AppKit must decode its TIFF representation");
        assert_eq!((image.width(), image.height()), (width, height));
        assert_eq!(image.rgba(), source.as_slice());
        assert!(!image.has_original_png());
    }

    /// The bridge outcome exposes a stable snake_case kind so the
    /// composite can branch on the four pasteboard shapes without
    /// parsing free-form text.
    #[test]
    fn native_png_read_kind_str_is_stable() {
        let empty_metadata = PasteboardImageMetadata {
            png_metadata: crate::clipboard_image_png::PngMetadataSummary::default(),
            tiff_metadata: None,
            inferred_display_dpi: None,
            resolution_detected: false,
            profile_detected: false,
        };
        assert_eq!(
            NativePngRead::NoPng {
                metadata: empty_metadata.clone()
            }
            .kind_str(),
            "no_png"
        );
        let tiff_image = crate::clipboard::ClipboardImage::new(vec![0, 0, 0, 255], 1, 1)
            .expect("one-pixel TIFF fixture");
        let tiff_only = NativePngRead::TiffOnly {
            image: tiff_image,
            metadata: empty_metadata.clone(),
        };
        assert_eq!(tiff_only.kind_str(), "tiff_only");
        assert!(tiff_only.has_image());
        assert_eq!(
            NativePngRead::InvalidPng(crate::clipboard_image_png::PngValidationError::TooLarge {
                size: 1
            })
            .kind_str(),
            "too_large"
        );
        assert_eq!(empty_metadata.kind_str(), "native_no_metadata");
    }

    /// The bridge outcome never carries the byte length, the
    /// absolute path or the payload bytes through the kind identifier.
    /// The diagnostic is metadata-only.
    #[test]
    fn native_png_read_kind_str_never_leaks_payload_metadata() {
        let outcome =
            NativePngRead::InvalidPng(crate::clipboard_image_png::PngValidationError::DecodeFailed);
        let kind = outcome.kind_str();
        assert_eq!(kind, "decode_failed");
        for forbidden in ["bytes", "len=", "/Users", "/home", ".clipvault", "hash"] {
            assert!(
                !kind.contains(forbidden),
                "kind must not leak {forbidden:?}, got {kind:?}"
            );
        }
    }

    #[test]
    fn display_scale_fallback_maps_retina_to_144_ppi() {
        assert_eq!(display_scale_to_dpi(2.0), Some((144, 144)));
        assert_eq!(display_scale_to_dpi(1.0), Some((72, 72)));
        assert_eq!(display_scale_to_dpi(0.0), None);
        assert_eq!(display_scale_to_dpi(f64::NAN), None);
    }
}
