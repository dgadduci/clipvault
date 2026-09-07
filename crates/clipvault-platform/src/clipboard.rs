//! Clipboard backend trait and the neutral payload it transports.
//!
//! The trait stays intentionally small: it exposes the operations the
//! rest of ClipVault needs (read/write text, read/write rich text,
//! read/write a raster image) and nothing else. Format negotiation,
//! pasteboard flavours and platform quirks live behind the concrete
//! implementations.
//!
//! ## Payload neutrality
//!
//! [`ClipboardPayload`] is the single transport type between the
//! platform layer and the core. It deliberately knows nothing about
//! `arboard`, Tauri, AppKit, X11 or the webview:
//!
//! - a textual capture is a plain [`String`];
//! - a rich textual capture is [`RichTextPayload`]: the canonical
//!   `plain_text` plus the available HTML and RTF representations;
//! - an image capture is a validated RGBA bitmap plus its dimensions.
//!
//! Bytes only live in memory for the duration of a read or a write.
//! The core is responsible for normalising both HTML/RTF previews and
//! PNG payloads and for persisting them; the platform layer never
//! touches the filesystem.
//!
//! ## Read priority is deterministic
//!
//! [`ClipboardBackend::read_payload`] has a default implementation
//! that encodes the documented priority once, for every backend:
//!
//! 1. try rich text; if the session exposes a non-empty `plain_text`
//!    plus at least one rich representation, capture it as `RichText`;
//! 2. only when no usable rich capture exists, try plain text;
//! 3. only when no usable text exists, try a supported raster image;
//! 4. anything else is `Ok(None)` — the caller treats it as
//!    "ignored", creates no row and keeps polling.
//!
//! The text-first guarantee (rich or plain) preserves the pre-existing
//! textual behaviour byte for byte when the source application publishes
//! a plain-text flavour alongside its rich or raster representation.
//!
//! Backends that only support text inherit the default
//! [`ClipboardBackend::read_image`] / [`ClipboardBackend::write_image`]
//! implementations, which report the matching capability as
//! unavailable, and the default [`ClipboardBackend::read_rich`] /
//! [`ClipboardBackend::write_rich`] implementations, which likewise
//! report the matching capability as unavailable. Existing text-only
//! fakes and adapters therefore keep compiling and behaving exactly as
//! before.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::tiff_metadata::TiffMetadata;
use crate::{Capability, PngMetadataSummary};

/// Upper bound on either pixel dimension of a captured image.
///
/// The cap is checked before any allocation so a hostile or corrupt
/// pasteboard entry cannot make ClipVault reserve an unbounded
/// buffer. 8192 covers every realistic screenshot (including 8K
/// displays) while keeping `width * height * 4` inside
/// [`MAX_CLIPBOARD_IMAGE_RGBA_BYTES`] for sane aspect ratios.
pub const MAX_CLIPBOARD_IMAGE_DIM: u32 = 8192;

/// Upper bound on the in-memory RGBA buffer of a captured image
/// (64 MiB ≈ a 4096×4096 bitmap). Anything above this is rejected
/// with [`ImageValidationError::TooLarge`] before the buffer is
/// copied or encoded.
pub const MAX_CLIPBOARD_IMAGE_RGBA_BYTES: usize = 64 * 1024 * 1024;

/// Number of bytes per pixel in the neutral RGBA representation.
pub const RGBA_BYTES_PER_PIXEL: usize = 4;

/// Why a candidate bitmap cannot become a [`ClipboardImage`].
///
/// Every variant is metadata-only: the error never carries pixels,
/// clipboard content or a hash, so it is safe to log and to surface
/// through a typed outcome.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ImageValidationError {
    /// Width or height is zero. A zero-sized bitmap carries no image.
    #[error("clipboard image has a zero dimension")]
    ZeroDimension,
    /// Width or height exceeds [`MAX_CLIPBOARD_IMAGE_DIM`].
    #[error("clipboard image dimension {dim} exceeds the {max} pixel cap")]
    DimensionTooLarge { dim: u32, max: u32 },
    /// `width * height * 4` overflowed `usize`. Checked explicitly so
    /// a crafted pasteboard entry cannot wrap the multiplication and
    /// pass the size cap.
    #[error("clipboard image size computation overflowed")]
    SizeOverflow,
    /// The computed RGBA size exceeds [`MAX_CLIPBOARD_IMAGE_RGBA_BYTES`].
    #[error("clipboard image needs {bytes} bytes, above the {max} byte cap")]
    TooLarge { bytes: usize, max: usize },
    /// The supplied buffer length does not match `width * height * 4`.
    #[error("clipboard image buffer is {actual} bytes, expected {expected}")]
    StrideMismatch { expected: usize, actual: usize },
    /// The pasteboard published a `public.png` representation but the
    /// bytes failed the platform-layer validator (signature,
    /// dimension / size cap, decoder failure). The composite MUST
    /// surface this error and MUST NOT fall back to the legacy
    /// `arboard::get_image` bitmap path: doing so would silently save
    /// a re-encoded PNG that lacks the `pHYs` / `iCCP` / `sRGB`
    /// metadata chunks the source application published. The
    /// `kind` field carries the snake_case reason the validator
    /// reported (never the payload, never the byte length).
    #[error("clipboard public.png payload is invalid: {kind}")]
    InvalidPng { kind: &'static str },
}

impl ImageValidationError {
    /// Stable snake_case identifier so callers can branch without
    /// parsing the free-form [`fmt::Display`] output.
    pub fn kind_str(&self) -> &'static str {
        match self {
            ImageValidationError::ZeroDimension => "zero_dimension",
            ImageValidationError::DimensionTooLarge { .. } => "dimension_too_large",
            ImageValidationError::SizeOverflow => "size_overflow",
            ImageValidationError::TooLarge { .. } => "too_large",
            ImageValidationError::StrideMismatch { .. } => "stride_mismatch",
            ImageValidationError::InvalidPng { kind } => kind,
        }
    }
}

/// Validate `width` / `height` and return the exact RGBA byte length
/// the bitmap requires.
///
/// The function is the single owner of the arithmetic guard rails:
/// zero dimensions, per-dimension caps, `usize` overflow and the
/// total-size cap are all checked *before* any allocation happens.
pub fn checked_rgba_len(width: u32, height: u32) -> Result<usize, ImageValidationError> {
    if width == 0 || height == 0 {
        return Err(ImageValidationError::ZeroDimension);
    }
    for dim in [width, height] {
        if dim > MAX_CLIPBOARD_IMAGE_DIM {
            return Err(ImageValidationError::DimensionTooLarge {
                dim,
                max: MAX_CLIPBOARD_IMAGE_DIM,
            });
        }
    }
    let width = usize::try_from(width).map_err(|_| ImageValidationError::SizeOverflow)?;
    let height = usize::try_from(height).map_err(|_| ImageValidationError::SizeOverflow)?;
    let pixels = width
        .checked_mul(height)
        .ok_or(ImageValidationError::SizeOverflow)?;
    let bytes = pixels
        .checked_mul(RGBA_BYTES_PER_PIXEL)
        .ok_or(ImageValidationError::SizeOverflow)?;
    if bytes > MAX_CLIPBOARD_IMAGE_RGBA_BYTES {
        return Err(ImageValidationError::TooLarge {
            bytes,
            max: MAX_CLIPBOARD_IMAGE_RGBA_BYTES,
        });
    }
    Ok(bytes)
}

/// Optional metadata the bridge exposes alongside a
/// [`ClipboardImage`]: the chunk summary the scanner produced for
/// the source PNG and the four IFD tags the TIFF parser cares
/// about. The struct is metadata-only except for the
/// `tiff_metadata.icc_profile` payload — that one byte buffer is
/// intentionally opaque because the persistence layer feeds it
/// into a PNG `iCCP` chunk without inspecting its content.
///
/// The bridge carrying this metadata lives in
/// [`crate::runtime::macos_clipboard_main_queue`]; every other
/// adapter leaves both fields `None` / empty so the persistence
/// layer can safely fall back to the legacy `arboard::get_image`
/// path on Linux X11 without surfacing the empty metadata as an
/// error.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct PasteboardImageMetadata {
    /// The chunk summary the PNG byte stream reported.
    pub png_chunks: PngMetadataSummary,
    /// The metadata the TIFF leg of the same pasteboard snapshot
    /// published. `None` when the pasteboard did not declare
    /// `public.tiff`.
    pub tiff: Option<TiffMetadata>,
    /// Resolution inferred from the macOS display scale when the
    /// pasteboard exposed a native PNG but neither image leg exposed
    /// usable resolution metadata. This is deliberately separate
    /// from `tiff`: it is a host fallback, not a claim about bytes
    /// present in the TIFF representation.
    pub inferred_display_dpi: Option<(u32, u32)>,
    /// True when the bridge was able to detect resolution
    /// metadata on at least one of the two representations.
    pub resolution_detected: bool,
    /// True when the bridge was able to detect a colour profile.
    pub profile_detected: bool,
}

impl PasteboardImageMetadata {
    /// Stable snake_case identifier the diagnostic surface and the
    /// `CLIPVAULT_DEBUG_IMAGE_*` environment contract consume.
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

    pub fn has_png(&self) -> bool {
        self.png_chunks != PngMetadataSummary::default()
    }
}

impl fmt::Debug for PasteboardImageMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PasteboardImageMetadata")
            .field("png_chunks_kind", &self.png_chunks.kind_str())
            .field("tiff_resolution_present", &self.tiff.is_some())
            .field("resolution_detected", &self.resolution_detected)
            .field("profile_detected", &self.profile_detected)
            .finish()
    }
}

/// A validated, platform-neutral raster bitmap read from (or destined
/// for) the operating-system clipboard.
///
/// # Invariants
///
/// A value of this type can only be built through
/// [`ClipboardImage::new`] or [`ClipboardImage::with_original_png`],
/// which guarantee:
///
/// - `width > 0` and `height > 0`;
/// - `width <= MAX_CLIPBOARD_IMAGE_DIM` and likewise for `height`;
/// - `rgba.len() == width * height * 4`, computed with checked
///   arithmetic;
/// - `rgba.len() <= MAX_CLIPBOARD_IMAGE_RGBA_BYTES`;
/// - when `original_png` is `Some(bytes)`, the bytes form a
///   decodable PNG whose IHDR dimensions match `width` and `height`
///   and whose RGBA frame matches `rgba`. The validation is the core's
///   `validate_original_png` responsibility; the constructor only
///   enforces the structural invariants so a malformed payload is
///   rejected as early as possible.
///
/// The pixel order is straight (non-premultiplied) RGBA, top-left
/// origin, no row padding — the same layout every supported backend
/// exposes.
///
/// The [`fmt::Debug`] implementation is hand-written on purpose: it
/// prints the dimensions, the buffer length and a boolean indicating
/// whether the original PNG bytes are present, but **never** the
/// pixels, the original PNG payload or its byte length. An accidental
/// `{:?}` in a log line therefore cannot leak the captured image.
#[derive(Clone, PartialEq, Eq)]
pub struct ClipboardImage {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    original_png: Option<Vec<u8>>,
    /// Metadata the host pasteboard exposed alongside the PNG
    /// leg. Optional: only the macOS bridge populates this field,
    /// every other adapter leaves it as the default
    /// [`PasteboardImageMetadata`]. The struct participates in the
    /// equality contract only by surface-level marker (presence of
    /// `original_png`) so legacy tests keep passing without the
    /// equality semantics bringing in ICC bytes.
    pasteboard_metadata: PasteboardImageMetadata,
}

impl ClipboardImage {
    /// Build a validated image from a straight-RGBA buffer.
    pub fn new(rgba: Vec<u8>, width: u32, height: u32) -> Result<Self, ImageValidationError> {
        let expected = checked_rgba_len(width, height)?;
        if rgba.len() != expected {
            return Err(ImageValidationError::StrideMismatch {
                expected,
                actual: rgba.len(),
            });
        }
        Ok(Self {
            rgba,
            width,
            height,
            original_png: None,
            pasteboard_metadata: PasteboardImageMetadata::default(),
        })
    }

    /// Build a validated image that also carries the original PNG
    /// bytes the clipboard exposed (typically `public.png` on macOS).
    ///
    /// `original_png` is the verbatim byte stream ClipVault will
    /// persist when the core's [`crate::clipboard_assets::normalize_image_with_original`]
    /// accepts it; the persistence layer keeps `pHYs`, `iCCP`/`sRGB`,
    /// `gAMA`, `cHRM` and any other safe metadata chunks intact.
    ///
    /// The constructor enforces the structural invariants only:
    /// - `original_png` must be non-empty;
    /// - the supplied RGBA buffer must match the declared dimensions.
    ///
    /// Semantic validation (PNG signature, decoder success, dimension
    /// coherence, RGBA consistency) lives in the core so the platform
    /// layer never owns the spec interpretation. The constructor
    /// returns an empty `original_png` rejection as
    /// [`ImageValidationError::ZeroDimension`] because there is no
    /// pixel data behind an empty PNG; the persistent zero-length
    /// case is a programming error.
    pub fn with_original_png(
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        original_png: Vec<u8>,
    ) -> Result<Self, ImageValidationError> {
        if original_png.is_empty() {
            return Err(ImageValidationError::ZeroDimension);
        }
        let image = Self::new(rgba, width, height)?;
        Ok(Self {
            original_png: Some(original_png),
            pasteboard_metadata: PasteboardImageMetadata::default(),
            ..image
        })
    }

    /// Build a validated image that carries the original PNG bytes
    /// **and** the metadata the pasteboard exposed alongside them.
    ///
    /// The bridge uses this constructor for the
    /// `NativePng` outcome: the constructor stashes the
    /// [`PasteboardImageMetadata`] so the persistence layer can
    /// later decide between the verbatim path, the
    /// `NativePngPlusMetadata` chunk-injection path and the
    /// `TiffMetadata` TIFF-only path. The struct participates in
    /// `PartialEq` only through `original_png`, so tests that
    /// pin the bytes do not need to recompute the metadata
    /// fingerprint every time.
    pub fn with_pasteboard_metadata(
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        original_png: Vec<u8>,
        pasteboard_metadata: PasteboardImageMetadata,
    ) -> Result<Self, ImageValidationError> {
        let mut image = Self::with_original_png(rgba, width, height, original_png)?;
        image.pasteboard_metadata = pasteboard_metadata;
        Ok(image)
    }

    /// Build a validated image carrying pasteboard metadata when the
    /// native clipboard exposed a raster representation other than
    /// PNG (currently the macOS `public.tiff` leg).
    ///
    /// The TIFF-only capture path still needs to transport the
    /// resolution/profile metadata into the core, but it has no
    /// original PNG byte stream to attach. Keeping this constructor
    /// separate from [`Self::with_pasteboard_metadata`] makes that
    /// distinction explicit and prevents an empty byte vector from
    /// being mistaken for a valid original PNG.
    pub fn with_pasteboard_metadata_without_original(
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        pasteboard_metadata: PasteboardImageMetadata,
    ) -> Result<Self, ImageValidationError> {
        let mut image = Self::new(rgba, width, height)?;
        image.pasteboard_metadata = pasteboard_metadata;
        Ok(image)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Straight-RGBA pixels, `width * height * 4` bytes long.
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// Consume the image and return the owned pixel buffer.
    pub fn into_rgba(self) -> Vec<u8> {
        self.rgba
    }

    /// Byte length of the pixel buffer. Cheap metadata a caller can
    /// log without touching the pixels.
    pub fn byte_len(&self) -> usize {
        self.rgba.len()
    }

    /// Original PNG bytes the clipboard exposed, when the capture
    /// pipeline surfaced them. `Some` only when the platform adapter
    /// read `public.png` (or equivalent) and the bytes validated; the
    /// `None` case is the legacy `arboard` path or a fallback.
    pub fn original_png(&self) -> Option<&[u8]> {
        self.original_png.as_deref()
    }

    /// Owned original PNG bytes, consuming the image.
    pub fn into_original_png(self) -> Option<Vec<u8>> {
        self.original_png
    }

    /// Whether the image carries original PNG bytes. Metadata-only:
    /// safe to log without inspecting the payload.
    pub fn has_original_png(&self) -> bool {
        self.original_png.is_some()
    }

    /// Read-only view of the pasteboard metadata the bridge
    /// attached to this image. Defaults to an empty metadata
    /// block when the producing adapter did not populate one.
    pub fn pasteboard_metadata(&self) -> &PasteboardImageMetadata {
        &self.pasteboard_metadata
    }
}

impl fmt::Debug for ClipboardImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Metadata only: never print the pixel buffer, the original
        // PNG payload or its byte length. A boolean presence flag is
        // enough for diagnostics; the actual bytes never reach a log
        // line.
        f.debug_struct("ClipboardImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("rgba_len", &self.rgba.len())
            .field("has_original_png", &self.original_png.is_some())
            .field(
                "pasteboard_metadata_kind",
                &self.pasteboard_metadata.kind_str(),
            )
            .finish()
    }
}

/// Neutral transport for a textual clipboard payload that also exposes
/// at least one rich representation (HTML or RTF).
///
/// The structure carries the canonical `plain_text` plus whatever rich
/// flavours the source application exposed. The platform layer never
/// inspects or sanitises the rich bytes — that work belongs to the
/// core, which can decide what to persist and what to render.
///
/// # Invariants
///
/// - `plain_text` is always non-empty; the constructor rejects an
///   empty canonical text so a `RichText` payload is never reduced to
///   a `Text` payload by accident;
/// - at least one of `html` / `rtf` is `Some` — a textual clipboard
///   without any rich flavour falls into the `Text` variant;
/// - `html` is owned (not borrowed) so the core can persist and
///   serialise it without lifetime gymnastics;
/// - `rtf` is a raw byte buffer; the platform layer only validates
///   its size cap, never its RTF syntax.
#[derive(Clone, PartialEq, Eq)]
pub struct RichTextPayload {
    plain_text: String,
    html: Option<String>,
    rtf: Option<Vec<u8>>,
}

impl RichTextPayload {
    /// Upper bound on the size of an HTML payload accepted from the
    /// platform layer (1 MiB). The cap protects the core from a
    /// hostile pasteboard entry; the rich-text asset store enforces
    /// its own per-file cap and rejects oversized bytes before
    /// writing them to disk.
    pub const MAX_RICH_TEXT_HTML_BYTES: usize = 1024 * 1024;

    /// Upper bound on the size of an RTF payload accepted from the
    /// platform layer (4 MiB). RTF is a token-heavy format so the cap
    /// is more generous than the HTML one.
    pub const MAX_RICH_TEXT_RTF_BYTES: usize = 4 * 1024 * 1024;

    /// Build a `RichTextPayload` from already-validated bytes.
    ///
    /// Returns `Err(ClipboardBackendError::Empty)` when `plain_text`
    /// is empty or when neither HTML nor RTF is supplied — the
    /// invariant keeps the rich-text variant distinguishable from a
    /// plain `Text` payload. Per-representation size caps live with
    /// the asset store and the paste pipeline; this constructor stays
    /// a pure invariant checker so tests can build fixtures freely.
    pub fn new(
        plain_text: String,
        html: Option<String>,
        rtf: Option<Vec<u8>>,
    ) -> Result<Self, ClipboardBackendError> {
        if plain_text.is_empty() {
            return Err(ClipboardBackendError::Empty);
        }
        let html = match html {
            Some(value) if value.is_empty() => None,
            Some(value) => Some(value),
            None => None,
        };
        let rtf = match rtf {
            Some(value) if value.is_empty() => None,
            Some(value) => Some(value),
            None => None,
        };
        if html.is_none() && rtf.is_none() {
            return Err(ClipboardBackendError::Empty);
        }
        Ok(Self {
            plain_text,
            html,
            rtf,
        })
    }

    /// Canonical plain-text flavour.
    pub fn plain_text(&self) -> &str {
        &self.plain_text
    }

    /// HTML representation, if the source exposed one.
    pub fn html(&self) -> Option<&str> {
        self.html.as_deref()
    }

    /// Owned HTML representation, for callers that need to persist it.
    pub fn into_html(self) -> Option<String> {
        self.html
    }

    /// RTF representation as raw bytes, if the source exposed one.
    pub fn rtf(&self) -> Option<&[u8]> {
        self.rtf.as_deref()
    }

    /// Owned RTF bytes, for callers that need to persist them.
    pub fn into_rtf(self) -> Option<Vec<u8>> {
        self.rtf
    }

    /// Consume the payload and return the inner parts without
    /// allocating.
    pub fn into_parts(self) -> (String, Option<String>, Option<Vec<u8>>) {
        (self.plain_text, self.html, self.rtf)
    }

    /// Whether at least one rich representation is present.
    pub fn has_rich(&self) -> bool {
        self.html.is_some() || self.rtf.is_some()
    }
}

impl fmt::Debug for RichTextPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Metadata only: never the HTML body, the RTF bytes or the
        // canonical plain text — those never reach a log line.
        f.debug_struct("RichTextPayload")
            .field("plain_len", &self.plain_text.len())
            .field("has_html", &self.html.is_some())
            .field("has_rtf", &self.rtf.is_some())
            .finish()
    }
}

/// Neutral transport for whatever the clipboard currently holds.
///
/// The [`fmt::Debug`] implementation prints the variant and its
/// metadata only — never the captured text nor the pixels.
#[derive(Clone, PartialEq, Eq)]
pub enum ClipboardPayload {
    /// Non-empty textual content with no rich representation.
    Text(String),
    /// Validated raster bitmap.
    Image(ClipboardImage),
    /// Non-empty plain text with at least one supported rich
    /// representation. The core never receives HTML/RTF bytes for
    /// arbitrary clipboard content without a plain-text companion.
    RichText(RichTextPayload),
}

impl ClipboardPayload {
    /// Stable snake_case discriminator used by diagnostics.
    pub fn kind(&self) -> &'static str {
        match self {
            ClipboardPayload::Text(_) => "text",
            ClipboardPayload::Image(_) => "image",
            ClipboardPayload::RichText(_) => "rich_text",
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            ClipboardPayload::Text(text) => Some(text),
            ClipboardPayload::RichText(payload) => Some(payload.plain_text()),
            ClipboardPayload::Image(_) => None,
        }
    }

    pub fn as_image(&self) -> Option<&ClipboardImage> {
        match self {
            ClipboardPayload::Image(image) => Some(image),
            ClipboardPayload::Text(_) | ClipboardPayload::RichText(_) => None,
        }
    }

    pub fn as_rich_text(&self) -> Option<&RichTextPayload> {
        match self {
            ClipboardPayload::RichText(payload) => Some(payload),
            ClipboardPayload::Text(_) | ClipboardPayload::Image(_) => None,
        }
    }
}

impl fmt::Debug for ClipboardPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Length only: the captured text never reaches a log line.
            ClipboardPayload::Text(text) => f
                .debug_struct("ClipboardPayload::Text")
                .field("len", &text.len())
                .finish(),
            ClipboardPayload::Image(image) => f
                .debug_tuple("ClipboardPayload::Image")
                .field(image)
                .finish(),
            ClipboardPayload::RichText(payload) => f
                .debug_struct("ClipboardPayload::RichText")
                .field("plain_len", &payload.plain_text().len())
                .field("has_html", &payload.html().is_some())
                .field("has_rtf", &payload.rtf().is_some())
                .finish(),
        }
    }
}

/// Typed error returned by every clipboard backend.
#[derive(Debug, Error)]
pub enum ClipboardBackendError {
    /// The backend reported that no usable text is on the clipboard.
    /// The pipeline treats this as `HistoryOutcome::Ignored`.
    #[error("clipboard returned no text")]
    Empty,
    /// The host-side clipboard could not fulfil the request. The
    /// `details` field MUST NOT contain clipboard contents (only
    /// categories, error codes and non-sensitive context).
    #[error("clipboard backend failed: {details}")]
    Backend { details: String },
    /// The platform layer declared this capability unavailable for the
    /// current session (for example Wayland without `wl-clipboard`).
    #[error("clipboard capability unavailable: {capability}")]
    Unavailable { capability: Capability },
    /// The clipboard holds a representation ClipVault does not
    /// support (a file list, audio, video, RTF-only payload, ...).
    /// The caller creates no row and keeps polling.
    #[error("clipboard holds an unsupported representation")]
    UnsupportedFormat,
    /// The clipboard exposed an image whose geometry or buffer failed
    /// validation. Carries the metadata-only reason.
    #[error("clipboard image is invalid: {0}")]
    InvalidImage(#[from] ImageValidationError),
}

impl ClipboardBackendError {
    /// Convenience constructor for [`ClipboardBackendError::Backend`].
    /// Accepts any `fmt::Display` so adapters can pass through
    /// underlying errors without leaking sensitive content.
    pub fn backend(details: impl fmt::Display) -> Self {
        ClipboardBackendError::Backend {
            details: details.to_string(),
        }
    }

    /// Stable snake_case identifier for diagnostics and typed
    /// outcomes. Never derived from the payload.
    pub fn kind_str(&self) -> &'static str {
        match self {
            ClipboardBackendError::Empty => "empty",
            ClipboardBackendError::Backend { .. } => "backend",
            ClipboardBackendError::Unavailable { .. } => "unavailable",
            ClipboardBackendError::UnsupportedFormat => "unsupported_format",
            ClipboardBackendError::InvalidImage(_) => "invalid_image",
        }
    }

    /// Whether the error means "nothing usable to capture" rather than
    /// "the backend is broken".
    ///
    /// The payload reader uses this to decide between falling through
    /// to the next representation (soft) and surfacing a typed failure
    /// (hard). Either way the watcher keeps running.
    pub fn is_soft(&self) -> bool {
        matches!(
            self,
            ClipboardBackendError::Empty
                | ClipboardBackendError::Unavailable { .. }
                | ClipboardBackendError::UnsupportedFormat
        )
    }
}

/// Minimal trait every clipboard backend must implement.
///
/// The trait is `Send + Sync` so a single backend instance can be shared
/// across threads (capture thread + paste thread) via `Arc<dyn ...>`.
///
/// Only [`Self::read_text`], [`Self::write_text`] and [`Self::name`]
/// are required. The image and rich-text methods have conservative
/// defaults so a text-only backend (the no-op adapter, a legacy fake,
/// a future platform without rich or image transport) keeps compiling
/// and reports the matching capability as unavailable instead of
/// pretending to support it.
pub trait ClipboardBackend: Send + Sync {
    /// Returns the current clipboard contents if they are textual.
    ///
    /// `Ok(None)` is the canonical "nothing to capture" signal.
    /// `Err(ClipboardBackendError::Empty)` is reserved for backends that
    /// can distinguish between "no clipboard contents" and "clipboard
    /// contents but no text". Most implementations collapse both into
    /// `Ok(None)`.
    fn read_text(&self) -> Result<Option<String>, ClipboardBackendError>;

    /// Writes `text` to the clipboard so that subsequent paste actions
    /// from the OS receive it.
    fn write_text(&self, text: &str) -> Result<(), ClipboardBackendError>;

    /// Returns the current clipboard contents if they are a textual
    /// payload with at least one rich representation.
    ///
    /// The default implementation reports
    /// [`Capability::ClipboardReadRichText`] as unavailable so a
    /// text-only backend never claims rich-text support. The method
    /// MUST NOT degrade silently to plain text: the read-priority
    /// helper below relies on the explicit `RichText` vs `Text`
    /// distinction.
    fn read_rich(&self) -> Result<Option<RichTextPayload>, ClipboardBackendError> {
        Err(ClipboardBackendError::Unavailable {
            capability: Capability::ClipboardReadRichText,
        })
    }

    /// Writes a rich-text payload to the clipboard. The default
    /// reports [`Capability::ClipboardWriteRichText`] as unavailable.
    ///
    /// Implementations are expected to write the available rich
    /// representations first and the `plain_text` as a fallback
    /// flavour in the same operation.
    fn write_rich(&self, payload: &RichTextPayload) -> Result<(), ClipboardBackendError> {
        let _ = payload;
        Err(ClipboardBackendError::Unavailable {
            capability: Capability::ClipboardWriteRichText,
        })
    }

    /// Returns the current clipboard contents if they are a supported
    /// raster image.
    ///
    /// The default implementation reports
    /// [`Capability::ClipboardReadImage`] as unavailable so a
    /// text-only backend never claims image support.
    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardBackendError> {
        Err(ClipboardBackendError::Unavailable {
            capability: Capability::ClipboardReadImage,
        })
    }

    /// Returns the current clipboard contents through the native
    /// fidelity-preserving image path. When the host exposes
    /// `public.png`, the result carries those original PNG bytes;
    /// when macOS exposes only `public.tiff`, the result carries the
    /// complete decoded RGBA frame plus its pasteboard metadata.
    ///
    /// The default implementation returns
    /// [`ClipboardBackendError::UnsupportedFormat`] (soft) so a backend
    /// that does not know how to transport original PNG bytes keeps
    /// compiling and reports the missing capability honestly instead
    /// of pretending to support it.
    ///
    /// Backends that implement the PNG leg MUST populate
    /// [`ClipboardImage::original_png`] so the core can persist the
    /// PNG verbatim. A TIFF-only native leg may leave that field
    /// empty, but MUST preserve its metadata through
    /// [`ClipboardImage::pasteboard_metadata`] so the core can
    /// rebuild the persisted PNG without degrading it to `arboard`.
    fn read_image_png(&self) -> Result<Option<ClipboardImage>, ClipboardBackendError> {
        Err(ClipboardBackendError::UnsupportedFormat)
    }

    /// Writes `image` to the clipboard so a subsequent paste action
    /// receives it. The default reports
    /// [`Capability::ClipboardWriteImage`] as unavailable.
    fn write_image(&self, image: &ClipboardImage) -> Result<(), ClipboardBackendError> {
        let _ = image;
        Err(ClipboardBackendError::Unavailable {
            capability: Capability::ClipboardWriteImage,
        })
    }

    /// Writes the canonical PNG bytes of an image to the clipboard.
    ///
    /// This optional fast path exists for platforms whose native
    /// clipboard can publish an encoded image without decoding and
    /// re-encoding it. The core uses it for persisted image copies so
    /// the bytes shown by the preview and the bytes handed to the
    /// receiving application are identical. A backend that does not
    /// implement the path keeps the historical `write_image` route.
    fn write_image_png(&self, png: &[u8]) -> Result<(), ClipboardBackendError> {
        let _ = png;
        Err(ClipboardBackendError::Unavailable {
            capability: Capability::ClipboardWriteImage,
        })
    }

    /// Whether this backend can read rich text at all. Independent
    /// from [`Self::supports_rich_write`] because a session may allow
    /// one direction and not the other. The default keeps the
    /// historical text-only behaviour.
    fn supports_rich_read(&self) -> bool {
        false
    }

    /// Whether this backend can write rich text at all.
    fn supports_rich_write(&self) -> bool {
        false
    }

    /// Whether this backend can read images at all. Independent from
    /// [`Self::supports_image_write`] because a session may allow one
    /// direction and not the other.
    fn supports_image_read(&self) -> bool {
        false
    }

    /// Whether this backend can write images at all.
    fn supports_image_write(&self) -> bool {
        false
    }

    /// Whether this backend can publish a PNG without changing its
    /// encoded bytes. This is deliberately separate from
    /// [`Self::supports_image_write`], because a backend may support
    /// image writes only through its decoded bitmap API.
    fn supports_image_png_write(&self) -> bool {
        false
    }

    /// Whether this backend can read the original PNG bytes the host
    /// clipboard exposed (typically `public.png` on macOS). This is
    /// deliberately separate from
    /// [`Self::supports_image_read`], because a backend may support
    /// bitmap reads without a fidelity-preserving PNG read path.
    fn supports_image_png_read(&self) -> bool {
        false
    }

    /// Whether this backend can perform a plain-text write that
    /// **clears the pasteboard first**. The composite routes
    /// `write_text` through the backend that advertises this flag so
    /// a `PasteMode::Plain` invocation drops every residual rich
    /// flavour (HTML, RTF, the previous capture's bytes) before
    /// publishing the canonical plain text.
    ///
    /// The default is `false`: a backend that does not override the
    /// method stays on the plain adapter, which is the documented
    /// path on Linux X11 and on every non-macOS target the
    /// composite is wired on. macOS' native `NSPasteboard` adapter
    /// overrides the flag because it has direct access to
    /// `clearContents()`; the `arboard` adapter is intentionally
    /// `false` so the composite's plain-write fallback never
    /// re-enters arboard's plain path on macOS when the native
    /// bridge is unavailable.
    fn supports_native_plain_write(&self) -> bool {
        false
    }

    /// Read whatever the clipboard currently holds, applying the
    /// documented deterministic priority.
    ///
    /// The default implementation is the single source of truth for
    /// the priority rule and should not be overridden without a
    /// documented reason:
    ///
    /// - rich text (non-empty plain text + at least one rich
    ///   representation) wins, always;
    /// - plain text wins over image and over unsupported payloads;
    /// - an image is only considered when no usable text exists;
    /// - soft errors (unavailable capability, empty, format not
    ///   supported) collapse to `Ok(None)` so the caller reports
    ///   "ignored" and the watcher keeps running;
    /// - hard errors (backend failure, invalid geometry) surface so
    ///   the caller can report a typed failure.
    fn read_payload(&self) -> Result<Option<ClipboardPayload>, ClipboardBackendError> {
        if self.supports_rich_read() {
            match self.read_rich() {
                Ok(Some(payload)) => return Ok(Some(ClipboardPayload::RichText(payload))),
                // A soft miss collapses to the plain-text attempt.
                Ok(None) => {}
                Err(error) if error.is_soft() => {}
                Err(error) => return Err(error),
            }
        }

        match self.read_text() {
            Ok(Some(text)) if !text.is_empty() => {
                return Ok(Some(ClipboardPayload::Text(text)));
            }
            // No usable text: fall through to the image attempt.
            Ok(_) => {}
            // A text read that reports "nothing textual" must not
            // hide an image that is present on the clipboard.
            Err(error) if error.is_soft() => {}
            Err(error) => return Err(error),
        }

        if !self.supports_image_read() {
            return Ok(None);
        }

        match self.read_image() {
            Ok(Some(image)) => Ok(Some(ClipboardPayload::Image(image))),
            Ok(None) => Ok(None),
            Err(error) if error.is_soft() => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Write a payload back to the clipboard, dispatching on its
    /// variant. Never converts an image to text or vice versa.
    ///
    /// A `RichText` payload is routed through [`Self::write_rich`]
    /// when the backend advertises rich support; otherwise the call
    /// surfaces the `clipboard_write_rich_text` capability so the
    /// paste pipeline can decide between a hard failure and a plain
    /// fallback.
    fn write_payload(&self, payload: &ClipboardPayload) -> Result<(), ClipboardBackendError> {
        match payload {
            ClipboardPayload::Text(text) => self.write_text(text),
            ClipboardPayload::Image(image) => self.write_image(image),
            ClipboardPayload::RichText(rich) => {
                if self.supports_rich_write() {
                    self.write_rich(rich)
                } else {
                    Err(ClipboardBackendError::Unavailable {
                        capability: Capability::ClipboardWriteRichText,
                    })
                }
            }
        }
    }

    /// Stable identifier for diagnostics. Backends must return a
    /// non-empty, human-readable string (e.g. `"arboard"`).
    fn name(&self) -> &'static str;
}

/// Identifier for the default clipboard backend implementation
/// selected at runtime. Useful for logging and for the frontend to
/// display which backend is in use.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardBackendKind {
    Arboard,
    InMemory,
    Unavailable,
}

impl ClipboardBackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ClipboardBackendKind::Arboard => "arboard",
            ClipboardBackendKind::InMemory => "in_memory",
            ClipboardBackendKind::Unavailable => "unavailable",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal text-only backend: proves the default image methods
    /// keep a legacy adapter compiling and honest about its support.
    struct TextOnlyBackend {
        text: Option<String>,
    }

    impl ClipboardBackend for TextOnlyBackend {
        fn read_text(&self) -> Result<Option<String>, ClipboardBackendError> {
            Ok(self.text.clone())
        }

        fn write_text(&self, _text: &str) -> Result<(), ClipboardBackendError> {
            Ok(())
        }

        fn name(&self) -> &'static str {
            "text-only"
        }
    }

    fn image(width: u32, height: u32) -> ClipboardImage {
        let len = checked_rgba_len(width, height).expect("valid geometry");
        ClipboardImage::new(vec![0x20; len], width, height).expect("valid image")
    }

    #[test]
    fn backend_kind_strings_are_stable() {
        assert_eq!(ClipboardBackendKind::Arboard.as_str(), "arboard");
        assert_eq!(ClipboardBackendKind::InMemory.as_str(), "in_memory");
        assert_eq!(ClipboardBackendKind::Unavailable.as_str(), "unavailable");
    }

    #[test]
    fn clipboard_image_enforces_its_invariants() {
        let img = image(2, 3);
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 3);
        assert_eq!(img.byte_len(), 2 * 3 * 4);
        assert_eq!(img.rgba().len(), img.byte_len());
    }

    #[test]
    fn checked_rgba_len_rejects_zero_dimensions() {
        assert_eq!(
            checked_rgba_len(0, 8),
            Err(ImageValidationError::ZeroDimension)
        );
        assert_eq!(
            checked_rgba_len(8, 0),
            Err(ImageValidationError::ZeroDimension)
        );
    }

    #[test]
    fn checked_rgba_len_rejects_oversized_dimensions() {
        let err = checked_rgba_len(MAX_CLIPBOARD_IMAGE_DIM + 1, 1).expect_err("must reject");
        assert_eq!(err.kind_str(), "dimension_too_large");
    }

    #[test]
    fn checked_rgba_len_rejects_total_size_above_the_cap() {
        // Both dimensions are individually legal, but the product
        // exceeds the RGBA byte cap. Without the total-size check the
        // pipeline would try to allocate 256 MiB.
        let err = checked_rgba_len(MAX_CLIPBOARD_IMAGE_DIM, MAX_CLIPBOARD_IMAGE_DIM)
            .expect_err("must reject");
        assert_eq!(err.kind_str(), "too_large");
    }

    #[test]
    fn checked_rgba_len_guards_against_multiplication_overflow() {
        // `u32::MAX * u32::MAX * 4` would wrap on a 64-bit `usize`
        // only after the dimension cap, so the cap fires first. The
        // explicit assertion documents that no input reaches the
        // allocation path with a wrapped length.
        for (w, h) in [(u32::MAX, u32::MAX), (u32::MAX, 1), (1, u32::MAX)] {
            let err = checked_rgba_len(w, h).expect_err("must reject");
            assert!(
                matches!(
                    err,
                    ImageValidationError::DimensionTooLarge { .. }
                        | ImageValidationError::SizeOverflow
                ),
                "unexpected {err:?}"
            );
        }
    }

    #[test]
    fn clipboard_image_rejects_stride_mismatch() {
        let err = ClipboardImage::new(vec![0; 3], 2, 2).expect_err("must reject");
        assert_eq!(
            err,
            ImageValidationError::StrideMismatch {
                expected: 16,
                actual: 3
            }
        );
    }

    #[test]
    fn clipboard_image_with_original_png_keeps_invariants() {
        let rgba = vec![0xAB; 4];
        let png = b"\x89PNG\r\n\x1a\nfake-png".to_vec();
        let image =
            ClipboardImage::with_original_png(rgba.clone(), 1, 1, png.clone()).expect("valid");
        assert_eq!(image.width(), 1);
        assert_eq!(image.height(), 1);
        assert_eq!(image.byte_len(), 4);
        assert_eq!(image.rgba(), rgba.as_slice());
        assert!(image.has_original_png());
        assert_eq!(image.original_png(), Some(png.as_slice()));
        // `into_original_png` consumes the image and returns the
        // owned PNG bytes; the rgba is dropped together with the
        // rest of the struct.
        let recovered = image.into_original_png();
        assert_eq!(recovered.as_deref(), Some(png.as_slice()));
    }

    #[test]
    fn clipboard_image_with_original_png_rejects_empty_bytes() {
        let err = ClipboardImage::with_original_png(vec![0; 4], 1, 1, Vec::new())
            .expect_err("must reject");
        assert_eq!(err, ImageValidationError::ZeroDimension);
    }

    #[test]
    fn clipboard_image_with_original_png_rejects_stride_mismatch() {
        // The constructor must reject a mismatched RGBA buffer even
        // when the original PNG bytes look plausible. This keeps the
        // structural invariants symmetric with `ClipboardImage::new`.
        let err = ClipboardImage::with_original_png(vec![0; 3], 2, 2, b"PNG".to_vec())
            .expect_err("must reject");
        assert_eq!(
            err,
            ImageValidationError::StrideMismatch {
                expected: 16,
                actual: 3
            }
        );
    }

    #[test]
    fn clipboard_image_default_has_no_original_png() {
        // The legacy `new` constructor is the path arboard uses:
        // it must produce an image with `original_png = None` so the
        // core can detect the legacy fallback and persist through
        // `normalize_image`.
        let image = ClipboardImage::new(vec![0x10; 4], 1, 1).expect("image");
        assert!(!image.has_original_png());
        assert!(image.original_png().is_none());
        assert!(image.into_original_png().is_none());
    }

    #[test]
    fn debug_output_never_contains_payload_bytes() {
        // Regression guard for the privacy contract: an accidental
        // `{:?}` in a log line must not print pixels or text.
        let img = ClipboardImage::new(vec![0xAB; 4], 1, 1).expect("image");
        let rendered = format!("{img:?}");
        assert!(rendered.contains("width"));
        assert!(!rendered.contains("171"), "pixel values must not appear");
        assert!(!rendered.contains("ab"), "pixel values must not appear");

        let payload = ClipboardPayload::Text("super-secret-token".into());
        let rendered = format!("{payload:?}");
        assert!(!rendered.contains("super-secret-token"));
        assert!(rendered.contains("len"));

        let rich = ClipboardPayload::RichText(
            RichTextPayload::new("secret".into(), Some("<b>secret</b>".into()), None)
                .expect("valid"),
        );
        let rendered = format!("{rich:?}");
        assert!(!rendered.contains("secret"));
        assert!(!rendered.contains("<b>"));
        assert!(rendered.contains("plain_len"));
    }

    #[test]
    fn debug_output_never_leaks_original_png_bytes() {
        // The `Debug` impl for an image that carries the original
        // PNG bytes MUST NOT print the payload, the byte length or
        // any substring of the PNG. The only diagnostic that may
        // reach a log line is the presence flag and the standard
        // dimensions / rgba length.
        let secret_png = b"\x89PNG\r\n\x1a\nsecret-payload-content".to_vec();
        let image =
            ClipboardImage::with_original_png(vec![0xAB; 4], 1, 1, secret_png.clone()).expect("ok");
        let rendered = format!("{image:?}");
        assert!(rendered.contains("has_original_png"));
        assert!(rendered.contains("width"));
        assert!(rendered.contains("rgba_len"));
        assert!(
            !rendered.contains("secret"),
            "the original PNG bytes must never appear in Debug output"
        );
        assert!(
            !rendered.contains("89 50 4E 47") && !rendered.contains("89504e47"),
            "PNG signature markers must not appear"
        );
    }

    #[test]
    fn payload_kind_strings_are_stable() {
        assert_eq!(ClipboardPayload::Text("x".into()).kind(), "text");
        assert_eq!(ClipboardPayload::Image(image(1, 1)).kind(), "image");
        let rich = ClipboardPayload::RichText(
            RichTextPayload::new("x".into(), Some("<i>x</i>".into()), None).expect("ok"),
        );
        assert_eq!(rich.kind(), "rich_text");
    }

    // -----------------------------------------------------------------
    // `clipboard-rich-text`: rich payload construction and invariants.
    // -----------------------------------------------------------------

    #[test]
    fn rich_text_payload_rejects_empty_plain_text() {
        // A `RichText` payload without canonical plain text would be
        // indistinguishable from `Text("")` and is meaningless: the
        // invariant keeps the priority chain deterministic.
        let error = RichTextPayload::new(String::new(), Some("<p/>".into()), None)
            .expect_err("must reject");
        assert_eq!(error.kind_str(), "empty");
    }

    #[test]
    fn rich_text_payload_requires_at_least_one_representation() {
        let error = RichTextPayload::new("hello".into(), None, None).expect_err("must reject");
        assert_eq!(error.kind_str(), "empty");
    }

    #[test]
    fn rich_text_payload_rejects_empty_representations() {
        // Empty inputs are collapsed to `None`. The variant requires
        // at least one rich representation, so a payload whose only
        // representations are empty must be rejected — the caller is
        // expected to fall back to the `Text` variant instead.
        let error = RichTextPayload::new("hello".into(), Some(String::new()), Some(Vec::new()))
            .expect_err("must reject");
        assert_eq!(error.kind_str(), "empty");
    }

    #[test]
    fn rich_text_payload_exposes_html_and_rtf_accessors() {
        let payload = RichTextPayload::new(
            "hello".into(),
            Some("<b>hello</b>".into()),
            Some(b"{\\rtf1 hello}".to_vec()),
        )
        .expect("valid");
        assert_eq!(payload.plain_text(), "hello");
        assert_eq!(payload.html(), Some("<b>hello</b>"));
        assert_eq!(payload.rtf(), Some(&b"{\\rtf1 hello}".to_vec()[..]));
        assert!(payload.has_rich());

        let (plain, html, rtf) = payload.clone().into_parts();
        assert_eq!(plain, "hello");
        assert_eq!(html.as_deref(), Some("<b>hello</b>"));
        assert_eq!(rtf.as_deref(), Some(&b"{\\rtf1 hello}".to_vec()[..]));
    }

    #[test]
    fn rich_text_payload_accepts_html_only_and_rtf_only() {
        let html_only =
            RichTextPayload::new("hello".into(), Some("<b>hello</b>".into()), None).expect("valid");
        assert!(html_only.html().is_some());
        assert!(html_only.rtf().is_none());

        let rtf_only = RichTextPayload::new("hello".into(), None, Some(b"{\\rtf1 hi}".to_vec()))
            .expect("valid");
        assert!(rtf_only.html().is_none());
        assert!(rtf_only.rtf().is_some());
    }

    #[test]
    fn text_only_backend_reports_image_support_as_unavailable() {
        let backend = TextOnlyBackend { text: None };
        assert!(!backend.supports_image_read());
        assert!(!backend.supports_image_write());
        match backend.read_image() {
            Err(ClipboardBackendError::Unavailable { capability }) => {
                assert_eq!(capability.as_str(), "clipboard_read_image");
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
        match backend.write_image(&image(1, 1)) {
            Err(ClipboardBackendError::Unavailable { capability }) => {
                assert_eq!(capability.as_str(), "clipboard_write_image");
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn default_read_payload_returns_text_when_available() {
        let backend = TextOnlyBackend {
            text: Some("hello".into()),
        };
        let payload = backend.read_payload().expect("read").expect("payload");
        assert_eq!(payload.as_text(), Some("hello"));
    }

    #[test]
    fn default_read_payload_ignores_empty_text_on_a_text_only_backend() {
        for text in [None, Some(String::new())] {
            let backend = TextOnlyBackend { text };
            assert!(backend.read_payload().expect("read").is_none());
        }
    }

    #[test]
    fn error_soft_classification_is_stable() {
        assert!(ClipboardBackendError::Empty.is_soft());
        assert!(ClipboardBackendError::UnsupportedFormat.is_soft());
        assert!(ClipboardBackendError::Unavailable {
            capability: Capability::ClipboardReadImage
        }
        .is_soft());
        assert!(!ClipboardBackendError::backend("boom").is_soft());
        assert!(
            !ClipboardBackendError::InvalidImage(ImageValidationError::ZeroDimension).is_soft()
        );
    }

    #[test]
    fn error_kind_strings_are_stable() {
        assert_eq!(ClipboardBackendError::Empty.kind_str(), "empty");
        assert_eq!(ClipboardBackendError::backend("x").kind_str(), "backend");
        assert_eq!(
            ClipboardBackendError::UnsupportedFormat.kind_str(),
            "unsupported_format"
        );
        assert_eq!(
            ClipboardBackendError::InvalidImage(ImageValidationError::ZeroDimension).kind_str(),
            "invalid_image"
        );
    }

    #[test]
    fn backend_error_display_never_includes_payload() {
        // The `details` string comes from the adapter; the contract is
        // that adapters pass categories and error codes only. This
        // test pins the shape of the rendered message so a future
        // adapter change that appends content is visible in review.
        let error = ClipboardBackendError::backend("ContentNotAvailable");
        assert_eq!(
            error.to_string(),
            "clipboard backend failed: ContentNotAvailable"
        );
    }
}
