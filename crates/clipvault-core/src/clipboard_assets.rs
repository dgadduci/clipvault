//! Local asset store for non-textual clipboard payloads.
//!
//! The store is the only component allowed to turn an in-memory
//! [`ClipboardImage`] into bytes on disk, and the only component
//! allowed to turn a persisted `asset_ref` back into bytes. It lives in
//! the core (not in the shell, not in the platform crate) so it can be
//! exercised without Tauri, Svelte, a real clipboard or a graphical
//! session: every entry point takes an explicit `data_dir`.
//!
//! ## Layout and reference shape
//!
//! ```text
//! <data_dir>/assets/clipboard/<lowercase-sha256>.png
//! ```
//!
//! The value persisted in SQLite is only the relative reference:
//!
//! ```text
//! clipboard/<lowercase-sha256>.png
//! ```
//!
//! An absolute path is never stored and never returned across the
//! Tauri boundary. The `clipboard/` namespace is deliberately
//! independent from `ignored-apps/` (blacklist picker icons) and
//! `application-icons/` (source-app icons): a reference from one
//! namespace can never resolve inside another.
//!
//! ## Safety guarantees on read
//!
//! [`ClipboardAssetStore::read_bytes`] mirrors the validator the
//! `application-icons` bridge uses and adds the image-specific bounds:
//!
//! - the reference must be non-empty, relative and free of any `..`
//!   component;
//! - it must start with the `clipboard/` prefix;
//! - the canonicalised path must stay under
//!   `<data_dir>/assets/clipboard/`, which rejects a symlink pointing
//!   outside the allowed root;
//! - the file must be a decodable PNG whose byte length stays under
//!   [`MAX_CLIPBOARD_ASSET_BYTES`] and whose dimensions stay under
//!   [`clipvault_platform::MAX_CLIPBOARD_IMAGE_DIM`].
//!
//! ## Atomicity
//!
//! [`ClipboardAssetStore::store_image`] writes to a temporary file in
//! the *same* directory and then `rename`s it into place, so a crash or
//! a failed write can never leave a partially valid asset that a later
//! read would serve. When the target already exists and passes
//! validation the bytes are reused instead of rewritten.
//!
//! ## Privacy
//!
//! No function in this module logs, returns or embeds pixels, the
//! captured text, the content hash or an absolute path. Errors carry a
//! stable snake_case kind plus non-sensitive metadata only.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use clipvault_platform::{checked_rgba_len, ClipboardImage, MAX_CLIPBOARD_IMAGE_DIM};
use sha2::{Digest, Sha256};

/// Sub-directory under `<data_dir>/assets` that holds persisted
/// clipboard payloads. Independent from the application-icon
/// namespaces so the three feature areas can evolve separately.
pub const CLIPBOARD_ASSETS_DIR: &str = "clipboard";

/// File extension and the only asset format in this phase.
pub const CLIPBOARD_ASSET_EXTENSION: &str = "png";

/// Upper bound on the byte length of a persisted PNG asset (16 MiB).
///
/// A losslessly encoded screenshot of a 5K display stays well under
/// this bound; the cap exists so a tampered assets directory cannot
/// hand the webview an arbitrarily large buffer.
pub const MAX_CLIPBOARD_ASSET_BYTES: usize = 16 * 1024 * 1024;

/// PNG signature every conforming file starts with.
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// Errors surfaced while validating a reference or reading an asset.
///
/// Every variant is metadata-only and maps to a stable snake_case
/// identifier so the frontend can pick the matching fallback copy
/// without parsing the free-form message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// The reference was an empty string.
    Empty,
    /// The reference was an absolute path.
    Absolute,
    /// The reference contains a `..` component.
    Traversal,
    /// The reference does not start with the `clipboard/` prefix, or it
    /// nests a sub-directory the store never writes.
    OutOfScope,
    /// The reference is well-formed but the file (or the namespace
    /// directory) does not exist.
    NotFound,
    /// The canonical path escapes the allowed root — typically a
    /// symlink pointing outside `assets/clipboard/`.
    Escaped,
    /// The file exists but cannot be read or written.
    Io { reason: String },
    /// The payload exceeds [`MAX_CLIPBOARD_ASSET_BYTES`].
    TooLarge { size: usize },
    /// The payload is not a decodable PNG.
    NotPng,
    /// The PNG decodes but its dimensions are outside the accepted
    /// bounds.
    InvalidDimensions { width: u32, height: u32 },
    /// The bitmap could not be encoded to PNG.
    Encode { reason: String },
    /// The original PNG bytes the clipboard surfaced failed the
    /// core-side coherence check (signature, dimension mismatch,
    /// decoder failure, RGBA mismatch). The capture pipeline MUST
    /// surface this error rather than silently re-encoding the
    /// bitmap through the legacy encoder — doing so would persist
    /// a degraded PNG that lacks the `pHYs` / `iCCP` / `sRGB`
    /// metadata chunks the source application published. The
    /// inner [`OriginalPngValidationError`] is metadata-only and
    /// `kind_str` returns the stable snake_case identifier.
    OriginalPngValidation(OriginalPngValidationError),
}

impl AssetError {
    pub fn kind_str(&self) -> &'static str {
        match self {
            AssetError::Empty => "empty",
            AssetError::Absolute => "absolute",
            AssetError::Traversal => "traversal",
            AssetError::OutOfScope => "out_of_scope",
            AssetError::NotFound => "not_found",
            AssetError::Escaped => "escaped",
            AssetError::Io { .. } => "io",
            AssetError::TooLarge { .. } => "too_large",
            AssetError::NotPng => "not_png",
            AssetError::InvalidDimensions { .. } => "invalid_dimensions",
            AssetError::Encode { .. } => "encode",
            AssetError::OriginalPngValidation(_) => "original_png_validation",
        }
    }
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AssetError::Empty => f.write_str("clipboard asset reference is empty"),
            AssetError::Absolute => {
                f.write_str("clipboard asset reference must be a relative path")
            }
            AssetError::Traversal => {
                f.write_str("clipboard asset reference must not contain '..' components")
            }
            AssetError::OutOfScope => f.write_str(
                "clipboard asset reference must be 'clipboard/<name>.png' inside the assets directory",
            ),
            AssetError::NotFound => f.write_str("clipboard asset is missing on disk"),
            AssetError::Escaped => {
                f.write_str("clipboard asset reference resolves outside the assets directory")
            }
            AssetError::Io { reason } => write!(f, "clipboard asset io failed: {reason}"),
            AssetError::TooLarge { size } => write!(
                f,
                "clipboard asset exceeds the {MAX_CLIPBOARD_ASSET_BYTES} byte cap (got {size})"
            ),
            AssetError::NotPng => f.write_str("clipboard asset is not a valid PNG"),
            AssetError::InvalidDimensions { width, height } => write!(
                f,
                "clipboard asset dimensions {width}x{height} are outside the accepted bounds"
            ),
            AssetError::Encode { reason } => {
                write!(f, "clipboard asset encoding failed: {reason}")
            }
            AssetError::OriginalPngValidation(error) => write!(
                f,
                "clipboard original png failed validation: {error}"
            ),
        }
    }
}

impl std::error::Error for AssetError {}

fn io_error(error: io::Error) -> AssetError {
    // `io::Error`'s Display can include a path on some platforms, so we
    // only keep the kind — never the path, never the payload.
    AssetError::Io {
        reason: error.kind().to_string(),
    }
}

/// Metadata-only outcome of [`ClipboardAssetStore::diagnose`].
///
/// The variant lets the diagnostics surface differentiate the failure
/// modes the user-facing error message collapses: a stale PNG that is
/// decodable by a wider decoder than the one shipped today, a file
/// that is missing on disk, an asset that landed in another data
/// directory, a namespace violation, a PNG that the decoder still
/// rejects, an oversized payload, a network drive hiccup or a
/// well-formed image the bridge can serve verbatim.
///
/// Every variant is metadata-only: the diagnostic never carries the
/// reference, the payload, an absolute path or the content hash. The
/// accompanying `kind_str` returns the stable snake_case identifier
/// the frontend consumes to drive the matching fallback copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetDiagnosticKind {
    /// The asset resolved to a decodable bitmap that the bridge can
    /// serve verbatim. The metadata block reports the dimensions the
    /// PNG header advertises and the byte length the store wrote; the
    /// frontend can use these to log a "loaded" diagnostic without
    /// inspecting pixels.
    Loaded,
    /// The reference failed the path-level validation: empty, absolute,
    /// traversal, foreign namespace, … The accompanying metadata is
    /// empty.
    InvalidReference,
    /// The file (or its containing directory) is missing on disk.
    /// The metadata is empty.
    NotFound,
    /// The asset is readable but the directory it lives in differs
    /// from the data directory the running process resolved. This is
    /// the canonical "wrong data_dir" signal the diagnostic surfaces
    /// so the user knows where the bytes really went. The metadata
    /// is empty by design: the absolute paths never leave the
    /// backend.
    WrongDataDir,
    /// The reference starts with the `clipboard/` prefix but the
    /// canonical path the resolver returns sits outside the
    /// `<data_dir>/assets/clipboard/` root — typically a symlink
    /// escape. The metadata is empty.
    WrongNamespace,
    /// The file is larger than [`MAX_CLIPBOARD_ASSET_BYTES`]. The
    /// `size` field carries the byte length the metadata observed
    /// (no payload bytes — only the count).
    TooLarge { size: usize },
    /// The asset is on disk but the PNG decoder refused it. The
    /// `color_type` and `bit_depth` fields report the values the
    /// PNG header advertised so the diagnostic can confirm the
    /// "wrong format" hypothesis without inspecting pixels.
    InvalidPng {
        color_type: &'static str,
        bit_depth: &'static str,
    },
    /// The asset decoded but the dimensions are outside the accepted
    /// bounds. The `width` and `height` fields report the values the
    /// PNG header advertised so the diagnostic can confirm the
    /// "wrong size" hypothesis without inspecting pixels.
    InvalidDimensions { width: u32, height: u32 },
    /// The asset is on disk but the read failed at the I/O layer. The
    /// `reason` field is a stable `io::ErrorKind` string and never
    /// carries the path.
    Io { reason: String },
}

impl AssetDiagnosticKind {
    /// Stable snake_case identifier the frontend consumes. The shell
    /// never inspects the free-form [`Display`](fmt::Display) string
    /// to make routing decisions; it only renders it after picking
    /// the matching copy.
    pub fn kind_str(&self) -> &'static str {
        match self {
            AssetDiagnosticKind::Loaded => "loaded",
            AssetDiagnosticKind::InvalidReference => "invalid_reference",
            AssetDiagnosticKind::NotFound => "not_found",
            AssetDiagnosticKind::WrongDataDir => "wrong_data_dir",
            AssetDiagnosticKind::WrongNamespace => "wrong_namespace",
            AssetDiagnosticKind::TooLarge { .. } => "too_large",
            AssetDiagnosticKind::InvalidPng { .. } => "invalid_png",
            AssetDiagnosticKind::InvalidDimensions { .. } => "invalid_dimensions",
            AssetDiagnosticKind::Io { .. } => "io_error",
        }
    }

    fn color_type_label(color_type: png::ColorType) -> &'static str {
        match color_type {
            png::ColorType::Grayscale => "grayscale",
            png::ColorType::Rgb => "rgb",
            png::ColorType::Indexed => "indexed",
            png::ColorType::GrayscaleAlpha => "grayscale_alpha",
            png::ColorType::Rgba => "rgba",
        }
    }

    fn bit_depth_label(bit_depth: png::BitDepth) -> &'static str {
        match bit_depth {
            png::BitDepth::One => "1",
            png::BitDepth::Two => "2",
            png::BitDepth::Four => "4",
            png::BitDepth::Eight => "8",
            png::BitDepth::Sixteen => "16",
        }
    }
}

/// Output of [`ClipboardAssetStore::diagnose`]: a stable identifier
/// plus the metadata the diagnostic helper already gathered.
///
/// The struct intentionally avoids payload bytes, content hashes and
/// absolute paths: the frontend receives the variant and the
/// accompanying counts / labels to drive the matching fallback copy
/// (or, in the `Loaded` case, to confirm the bridge will serve bytes
/// when asked).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetDiagnostic {
    pub kind: AssetDiagnosticKind,
}

impl AssetDiagnostic {
    pub fn kind_str(&self) -> &'static str {
        self.kind.kind_str()
    }
}

/// Relative reference for a normalised-PNG hash.
pub fn asset_ref_for_hash(hash: &str) -> String {
    format!("{CLIPBOARD_ASSETS_DIR}/{hash}.{CLIPBOARD_ASSET_EXTENSION}")
}

/// How the asset was persisted. The capture pipeline converts
/// this identifier into the corresponding [`crate::ImageSource`]
/// diagnostic value; the renaming keeps the persistence layer
/// free of GUI-facing types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NormalizedSource {
    /// `original_png = None` legacy encoder: the canonical 8-bit
    /// RGBA PNG without any safe metadata chunks.
    #[default]
    LegacyEncoded,
    /// `original_png = Some(bytes)` and the bytes were kept
    /// verbatim because the source PNG already carried the
    /// metadata the bridge consumed.
    NativeVerbatim,
    /// `original_png = Some(bytes)` but the rebuild path ran:
    /// the persistence layer encoded the PNG with the same
    /// pixels plus the metadata injected from the sibling leg.
    /// The output bytes are not byte-identical to the source.
    NativeRebuiltWithMetadata,
    /// Only the TIFF leg was available. The persistence layer
    /// kept the asset by combining the sibling-leg metadata with
    /// the RGBA frame the bridge recovered.
    TiffMetadataOnly,
}

/// A bitmap normalised to a canonical PNG, ready to be persisted.
///
/// `hash` is the lowercase hex SHA-256 of `png` — the same value the
/// capture pipeline uses as `content_hash`, so dedupe and the asset
/// name are derived from exactly the same bytes.
///
/// When the original PNG bytes the clipboard exposed were valid, the
/// struct also carries a clone of those bytes in `original_png_bytes`
/// so callers (the asset store, the diagnostic surface) can confirm
/// the fidelity-preserving path was used without inspecting the
/// payload. The field is metadata-only: it is never surfaced
/// through `Debug` and never reaches a log line.
///
/// The `source` field records which fidelity path produced the
/// payload. The diagnostic surface uses it to render the
/// `image_source` identifier an operator expects, so the capture
/// pipeline never has to inspect the byte buffer to tell whether
/// the rebuild path ran.
#[derive(Clone, PartialEq, Eq)]
pub struct NormalizedImage {
    png: Vec<u8>,
    hash: String,
    width: u32,
    height: u32,
    original_png_bytes: Option<Vec<u8>>,
    /// Which fidelity path produced the PNG. The default is
    /// [`NormalizedSource::LegacyEncoded`] because every helper
    /// that did not preserve the original bytes produces an
    /// identical value.
    source: NormalizedSource,
}

impl NormalizedImage {
    /// Canonical PNG bytes. For the fidelity-preserving path these
    /// are the original bytes the clipboard exposed (with `pHYs`,
    /// `iCCP` / `sRGB`, ...); for the legacy encoder these are the
    /// canonical 8-bit RGBA PNG the encoder produced.
    pub fn png(&self) -> &[u8] {
        &self.png
    }

    /// Lowercase hex SHA-256 of [`Self::png`].
    pub fn hash(&self) -> &str {
        &self.hash
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Byte length of the encoded PNG. This is the value persisted as
    /// `content_size`.
    pub fn byte_len(&self) -> usize {
        self.png.len()
    }

    /// Whether the asset was persisted from the original PNG bytes
    /// the host clipboard exposed, rather than from a re-encoded
    /// bitmap. Metadata-only: safe to log without inspecting the
    /// payload.
    pub fn has_original_png_bytes(&self) -> bool {
        self.original_png_bytes.is_some()
    }

    /// Owned original PNG bytes, when the fidelity-preserving path
    /// was used. Returns `None` for the legacy encoder fallback.
    pub fn into_original_png_bytes(self) -> Option<Vec<u8>> {
        self.original_png_bytes
    }

    /// Which fidelity path produced the persisted asset.
    pub fn source(&self) -> NormalizedSource {
        self.source
    }

    /// Relative reference this asset will be persisted under.
    pub fn asset_ref(&self) -> String {
        crate::clipboard_assets::asset_ref_for_hash(&self.hash)
    }

    /// True when the persistence layer rebuilt a PNG that
    /// preserved the metadata the bridge consumed. The diagnostic
    /// surface uses this flag to render the
    /// `native_png_plus_metadata` identifier the operator expects.
    pub fn was_rebuilt_with_metadata(&self) -> bool {
        self.source == NormalizedSource::NativeRebuiltWithMetadata
    }
}

impl fmt::Debug for NormalizedImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Metadata only: neither the PNG bytes nor the hash (which is
        // derived from the payload) may reach a log line.
        f.debug_struct("NormalizedImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("png_len", &self.png.len())
            .field("has_original_png_bytes", &self.original_png_bytes.is_some())
            .field("source", &self.source)
            .finish()
    }
}

/// Encode a validated bitmap to a canonical PNG and hash the result.
///
/// The bitmap is already bounded by [`ClipboardImage`]'s invariants, so
/// this function only has to guarantee determinism: the same pixels
/// always produce byte-identical PNG output, which is what makes the
/// hash a usable dedupe key across restarts.
pub fn normalize_image(image: &ClipboardImage) -> Result<NormalizedImage, AssetError> {
    let width = image.width();
    let height = image.height();
    if width == 0
        || height == 0
        || width > MAX_CLIPBOARD_IMAGE_DIM
        || height > MAX_CLIPBOARD_IMAGE_DIM
    {
        return Err(AssetError::InvalidDimensions { width, height });
    }

    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        // Fixed compression settings keep the output deterministic:
        // the same pixels must always hash to the same value.
        encoder.set_compression(png::Compression::Default);
        let mut writer = encoder.write_header().map_err(|error| AssetError::Encode {
            reason: error.to_string(),
        })?;
        writer
            .write_image_data(image.rgba())
            .map_err(|error| AssetError::Encode {
                reason: error.to_string(),
            })?;
        writer.finish().map_err(|error| AssetError::Encode {
            reason: error.to_string(),
        })?;
    }

    if png.len() > MAX_CLIPBOARD_ASSET_BYTES {
        return Err(AssetError::TooLarge { size: png.len() });
    }

    let hash = sha256_hex(&png);
    Ok(NormalizedImage {
        png,
        hash,
        width,
        height,
        original_png_bytes: None,
        source: NormalizedSource::LegacyEncoded,
    })
}

/// Persist an image through the fidelity-preserving path when the
/// clipboard surfaced the original PNG bytes; otherwise fall back to
/// [`normalize_image`].
///
/// The original-bytes path preserves the `pHYs` resolution chunk,
/// the `iCCP` / `sRGB` color-profile chunk, the `gAMA` / `cHRM`
/// chunks and any other safe metadata chunks the source application
/// published. The decoded RGBA frame the bridge produced is used as
/// the dedupe fingerprint so the same image captured twice still
/// collapses to a single row.
///
/// The validation pipeline matches the platform-layer helper:
///
/// 1. **Signature**: the first eight bytes must be the canonical
///    PNG magic.
/// 2. **Size cap**: `bytes.len() <= MAX_CLIPBOARD_ASSET_BYTES`.
/// 3. **Dimension cap**: the IHDR dimensions must be non-zero, must
///    stay within [`clipvault_platform::MAX_CLIPBOARD_IMAGE_DIM`],
///    and must match the bitmap the capture produced.
/// 4. **RGBA consistency**: the decoded RGBA frame must match the
///    RGBA frame the capture pipeline holds.
///
/// The legacy `normalize_image()` path is reserved for two cases:
///
/// - the clipboard did not surface `original_png` (the legacy
///   `arboard::get_image` bitmap path on Linux X11 / Wayland or on
///   macOS hosts where the native bridge is unreachable);
/// - the platform validation already rejected the bytes via the
///   typed `InvalidImage` outcome, in which case the rich adapter
///   surfaces the error and the capture pipeline never reaches
///   this helper.
///
/// When `original_png` is `Some(bytes)` but
/// [`validate_original_png`] rejects them (signature, dimension
/// mismatch, decoder failure, RGBA mismatch), the helper MUST NOT
/// silently re-encode the bitmap through the legacy encoder:
/// doing so would persist a degraded PNG that lacks the metadata
/// chunks the user reported as missing. The helper surfaces the
/// typed [`OriginalPngValidationError`] wrapped in
/// [`AssetError::OriginalPngValidation`] so the capture pipeline
/// can surface the typed failure without re-encoding the bitmap.
pub fn normalize_image_with_original(
    image: &ClipboardImage,
) -> Result<NormalizedImage, AssetError> {
    let width = image.width();
    let height = image.height();
    let original_bytes = image.original_png();
    let pasteboard_metadata = image.pasteboard_metadata();
    if let Some(bytes) = original_bytes {
        match validate_original_png(bytes, image.rgba(), width, height) {
            Ok(()) => {
                // The original PNG decoded cleanly. Decide which
                // fidelity path to surface:
                //
                // - **`NativePng`**: the PNG bytes already carry
                //   resolution + profile chunks. Persist verbatim.
                // - **`NativePngPlusMetadata`**: the PNG bytes are
                //   pixel-valid but the pasteboard reports
                //   resolution / profile metadata on a sibling
                //   leg. Rebuild the PNG with `pHYs` / `iCCP`
                //   injected from the TIFF leg. The output bytes
                //   differ from the source — this branch is the
                //   only one that cannot preserve the original
                //   byte stream byte-for-byte.
                // - **`NativePngNoMetadata`**: neither the PNG nor
                //   the TIFF leg expose resolution / profile. The
                //   original PNG is still pixel-valid; persist it
                //   verbatim with no metadata to lose.
                if let Some(rebuilt) = rebuild_for_pasteboard_metadata(
                    bytes,
                    width,
                    height,
                    image.rgba(),
                    pasteboard_metadata,
                )? {
                    let png = rebuilt;
                    if png.len() > MAX_CLIPBOARD_ASSET_BYTES {
                        return Err(AssetError::TooLarge { size: png.len() });
                    }
                    let hash = sha256_hex(&png);
                    return Ok(NormalizedImage {
                        png,
                        hash,
                        width,
                        height,
                        original_png_bytes: Some(bytes.to_vec()),
                        source: NormalizedSource::NativeRebuiltWithMetadata,
                    });
                }
                // No sibling-leg metadata to inject: persist the
                // original PNG bytes verbatim.
                if bytes.len() > MAX_CLIPBOARD_ASSET_BYTES {
                    return Err(AssetError::TooLarge { size: bytes.len() });
                }
                let png = bytes.to_vec();
                let hash = sha256_hex(&png);
                return Ok(NormalizedImage {
                    png,
                    hash,
                    width,
                    height,
                    original_png_bytes: Some(bytes.to_vec()),
                    source: NormalizedSource::NativeVerbatim,
                });
            }
            Err(error) => {
                // The platform bridge guarantees that an image with
                // `original_png: Some(bytes)` has already cleared the
                // signature / dimension / decoder validation, so the
                // failures the core helper can still produce are
                // dimension mismatches and RGBA mismatches between
                // the decoded frame and the bitmap the capture
                // pipeline holds. Either way: do not silently fall
                // back to the legacy encoder. The capture pipeline
                // surfaces the typed failure instead.
                return Err(AssetError::OriginalPngValidation(error));
            }
        }
    }

    // A macOS screenshot copied through the screenshot UI can expose
    // only `public.tiff`. In that case there is no original PNG to
    // validate or persist, but the bridge still gives us the complete
    // RGBA frame and the TIFF resolution/profile metadata. Start from
    // the deterministic canonical PNG, then inject that metadata so
    // the TIFF-only path retains the 144 ppi contract instead of
    // silently falling back to a metadata-free 72 ppi PNG.
    let normalized = normalize_image(image)?;
    if let Some(rebuilt) = rebuild_for_pasteboard_metadata(
        normalized.png(),
        width,
        height,
        image.rgba(),
        pasteboard_metadata,
    )? {
        if rebuilt.len() > MAX_CLIPBOARD_ASSET_BYTES {
            return Err(AssetError::TooLarge {
                size: rebuilt.len(),
            });
        }
        let hash = sha256_hex(&rebuilt);
        return Ok(NormalizedImage {
            png: rebuilt,
            hash,
            width,
            height,
            original_png_bytes: None,
            source: NormalizedSource::TiffMetadataOnly,
        });
    }
    Ok(normalized)
}

/// Decide whether the persistence layer can rebuild a higher-fidelity
/// PNG out of an existing pixel-valid source and the metadata the
/// bridge surfaced.
///
/// Returns `Ok(Some(bytes))` when the helper produced a new payload.
/// Returns `Ok(None)` when the original PNG already carries the same
/// metadata the sibling leg reports, or when no sibling-leg metadata
/// applies; in either case the persistence layer should fall back to
/// the verbatim path.
///
/// Returns `Err(AssetError::OriginalPngValidation(_))` when the
/// sibling metadata disagrees with the pixels at the structural
/// level: a malformed `pHYs` payload, an `iCCP` profile name longer
/// than the PNG spec allows, or an `iCCP` profile that the deflate
/// encoder refused to compress. The error is typed so the caller can
/// surface the failure without falling back to the legacy 8-bit RGBA
/// encoder.
fn dpi_to_pixels_per_meter(dpi: u32) -> Option<u32> {
    // 100 / 2.54 = 5000 / 127 exactly. Match the platform bridge's
    // bounded integer conversion so the inferred fallback produces
    // the same pHYs payload as an explicit macOS TIFF value.
    let numerator = (dpi as u64).checked_mul(5000)?;
    let ppm = numerator.checked_add(63)? / 127;
    u32::try_from(ppm).ok()
}

fn rebuild_for_pasteboard_metadata(
    bytes: &[u8],
    width: u32,
    height: u32,
    rgba: &[u8],
    pasteboard_metadata: &clipvault_platform::PasteboardImageMetadata,
) -> Result<Option<Vec<u8>>, AssetError> {
    let png_summary = &pasteboard_metadata.png_chunks;

    // Prefer the TIFF rational converted directly to pixels per metre.
    // Going through rounded integer DPI first can move a non-integer
    // source resolution by a measurable amount and, more importantly,
    // makes the persisted `pHYs` value depend on a lossy intermediate
    // representation.  The PNG metadata contract is pixels/metre,
    // so keep that unit all the way to the chunk writer.
    let tiff_pixels_per_meter = pasteboard_metadata.tiff.as_ref().and_then(|tiff| {
        // `public.tiff` is an Apple pasteboard representation.
        // ImageIO can publish X/Y resolution without tag 296;
        // its UI still reports those values as ppi.  Use the
        // explicitly macOS-compatible conversion here, while
        // keeping the strict TIFF helpers available for generic
        // callers.
        tiff.macos_pixels_per_meter_x()
            .zip(tiff.macos_pixels_per_meter_y())
    });
    let inferred_display_pixels_per_meter =
        pasteboard_metadata
            .inferred_display_dpi
            .and_then(|(dpi_x, dpi_y)| {
                Some((
                    dpi_to_pixels_per_meter(dpi_x)?,
                    dpi_to_pixels_per_meter(dpi_y)?,
                ))
            });
    let tiff_icc_profile_bytes = pasteboard_metadata
        .tiff
        .as_ref()
        .and_then(|tiff| tiff.icc_profile.clone());

    // Resolution candidates: the TIFF leg is authoritative whenever
    // it is available. macOS may publish a stale/default 72 ppi
    // `pHYs` chunk in `public.png` while the sibling TIFF describes
    // the actual 144 ppi representation shown by Preview. Choosing
    // the PNG value first would preserve that wrong default forever.
    let phys_payload = tiff_pixels_per_meter
        .map(|(ppu_x, ppu_y)| {
            let mut payload = [0u8; 9];
            payload[0..4].copy_from_slice(&ppu_x.to_be_bytes());
            payload[4..8].copy_from_slice(&ppu_y.to_be_bytes());
            payload[8] = 1;
            payload
        })
        .or_else(|| {
            inferred_display_pixels_per_meter.map(|(ppu_x, ppu_y)| {
                let mut payload = [0u8; 9];
                payload[0..4].copy_from_slice(&ppu_x.to_be_bytes());
                payload[4..8].copy_from_slice(&ppu_y.to_be_bytes());
                payload[8] = 1;
                payload
            })
        })
        .or(png_summary.phys);

    // Profile candidates: existing PNG `iCCP` bytes (already
    // deflate-compressed), a fresh `iCCP` chunk wrapping the TIFF
    // ICCProfile IFD bytes, or `sRGB` rendering intent. The PNG
    // spec requires iCCP name 1..=79 chars and a non-zero profile
    // payload; we reject anything outside the bounds as a typed
    // validation failure so the capture pipeline can surface it.
    let icc_chunk: Option<IccChunk> = if let Some(existing) = &png_summary.icc_profile_chunk {
        // An existing iCCP chunk payload travels through the
        // rebuild path unchanged. The scanner already validated
        // that it lives before IDAT so the bytes are part of the
        // metadata layer the PNG decoder already accepted.
        if existing.is_empty() {
            return Err(AssetError::OriginalPngValidation(
                OriginalPngValidationError::NotPng,
            ));
        }
        // Distinguish the canonical "Display P3" name and use it
        // verbatim; otherwise fall back to "icc".
        let (name, body) = split_iccp_chunk(existing).ok_or(AssetError::OriginalPngValidation(
            OriginalPngValidationError::DecodeFailed,
        ))?;
        Some(IccChunk {
            profile_name: name,
            compression_method: body.0,
            compressed_profile: body.1,
        })
    } else if let Some(profile_bytes) = &tiff_icc_profile_bytes {
        if profile_bytes.is_empty() {
            return Err(AssetError::OriginalPngValidation(
                OriginalPngValidationError::NotPng,
            ));
        }
        let compressed = deflate_icc_profile(profile_bytes)?;
        Some(IccChunk {
            profile_name: "icc".to_string(),
            compression_method: 0,
            compressed_profile: compressed,
        })
    } else {
        None
    };
    let srgb_intent: Option<u8> = if png_summary.icc_profile_chunk.is_some() {
        None
    } else if png_summary.has_srgb {
        // Match `png::SrgbRenderingIntent::Perceptual` (= 0). The
        // spec lets the rendering intent be any of four values; we
        // pin Perceptual because that is the legacy encoder's
        // default and the field is opaque for sRGB PNGs.
        Some(0)
    } else {
        None
    };

    let resolution_needs_rebuild = phys_payload != png_summary.phys;
    let profile_needs_rebuild =
        tiff_icc_profile_bytes.is_some() && png_summary.icc_profile_chunk.is_none();
    if !resolution_needs_rebuild && !profile_needs_rebuild {
        // The source PNG already carries every metadata field the
        // current pasteboard snapshot can improve. Keep it verbatim.
        // This also avoids turning a metadata-free source into a
        // freshly encoded 72 ppi PNG merely because the rebuild
        // helper was called.
        return Ok(None);
    }

    // The rebuild produces a fresh PNG with the same pixels and
    // the injected metadata. This is the
    // `NativePngPlusMetadata` path the user reported: the source
    // pixels travel through; only the chunk layout changes.
    let png = rebuild_png_with_metadata(width, height, rgba, phys_payload, icc_chunk, srgb_intent)?;
    if png == bytes.to_vec() {
        // Defensive: the rebuild should never be byte-identical
        // unless the source was already perfect. Avoid the round
        // trip just in case.
        return Ok(None);
    }
    Ok(Some(png))
}

/// Split an iCCP chunk payload into `(profile_name, (compression,
/// compressed_profile))`. The helper exists so the rebuild path
/// can forward an existing `iCCP` chunk's deflate-compressed
/// profile without re-deflating it.
fn split_iccp_chunk(payload: &[u8]) -> Option<(String, (u8, Vec<u8>))> {
    let nul = payload.iter().position(|&b| b == 0)?;
    if nul == 0 || nul > 79 {
        return None;
    }
    let name = std::str::from_utf8(&payload[..nul]).ok()?.to_string();
    let compression = *payload.get(nul + 1)?;
    let profile = payload.get(nul + 2..)?;
    Some((name, (compression, profile.to_vec())))
}

/// Validate a candidate PNG payload against the bitmap the capture
/// pipeline holds.
///
/// Returns `Ok(())` when:
///
/// - the payload starts with the canonical PNG signature;
/// - the payload size stays within [`MAX_CLIPBOARD_ASSET_BYTES`];
/// - the IHDR dimensions are non-zero, stay within
///   [`clipvault_platform::MAX_CLIPBOARD_IMAGE_DIM`] and match
///   `expected_width` / `expected_height`;
/// - the decoded RGBA frame matches `expected_rgba` byte for byte.
///
/// On any failure the helper returns a typed
/// [`OriginalPngValidationError`] whose `kind_str` is metadata-only.
/// The helper never logs the payload and never echoes it through a
/// free-form message.
pub fn validate_original_png(
    bytes: &[u8],
    expected_rgba: &[u8],
    expected_width: u32,
    expected_height: u32,
) -> Result<(), OriginalPngValidationError> {
    if !looks_like_png(bytes) {
        return Err(OriginalPngValidationError::NotPng);
    }
    if bytes.len() > MAX_CLIPBOARD_ASSET_BYTES {
        return Err(OriginalPngValidationError::TooLarge { size: bytes.len() });
    }
    let decoder = png::Decoder::new(io::Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|_| OriginalPngValidationError::DecodeFailed)?;
    let info = reader.info();
    let (decoded_width, decoded_height) = (info.width, info.height);
    if decoded_width != expected_width || decoded_height != expected_height {
        return Err(OriginalPngValidationError::DimensionMismatch {
            declared: (expected_width, expected_height),
            decoded: (decoded_width, decoded_height),
        });
    }
    let palette: Option<Vec<u8>> = info.palette.as_deref().map(|slice| slice.to_vec());
    let trns: Option<Vec<u8>> = info.trns.as_deref().map(|slice| slice.to_vec());
    let mut buffer = vec![0; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut buffer)
        .map_err(|_| OriginalPngValidationError::DecodeFailed)?;
    buffer.truncate(frame.buffer_size());
    let decoded = expand_png_frame_to_rgba8(
        frame.color_type,
        frame.bit_depth,
        &buffer,
        palette.as_deref(),
        trns.as_deref(),
    )
    .map_err(|_| OriginalPngValidationError::DecodeFailed)?;
    if decoded.as_slice() != expected_rgba {
        return Err(OriginalPngValidationError::RgbaMismatch);
    }
    Ok(())
}

/// Why a candidate PNG payload did not pass
/// [`validate_original_png`].
///
/// The variants are metadata-only: no payload bytes, no path, no
/// hash. The free-form `Display` impl never embeds the offending
/// bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OriginalPngValidationError {
    /// The payload is shorter than the PNG signature or the first
    /// eight bytes do not match the canonical magic.
    NotPng,
    /// The payload exceeds [`MAX_CLIPBOARD_ASSET_BYTES`].
    TooLarge { size: usize },
    /// The decoded dimensions do not match the bitmap the capture
    /// pipeline holds. The tuple pairs the declared `(width,
    /// height)` with the decoded `(width, height)` so the diagnostic
    /// can pinpoint the divergence without echoing the payload.
    DimensionMismatch {
        declared: (u32, u32),
        decoded: (u32, u32),
    },
    /// The PNG decoder refused the payload: a truncated body, an
    /// illegal chunk, an unknown critical chunk, an unsupported
    /// `(ColorType, BitDepth)` combination, etc. The `png::Decoder`
    /// produces a free-form message; the diagnostic keeps the message
    /// off its surface so it never reaches a log line.
    DecodeFailed,
    /// The decoded RGBA frame does not match the bitmap the capture
    /// pipeline holds. The mismatch is the canonical signal that the
    /// bridge derived the bitmap from a different source than the
    /// pasteboard's PNG.
    RgbaMismatch,
}

impl OriginalPngValidationError {
    /// Stable snake_case identifier so callers can collapse a failure
    /// to a soft miss without parsing free-form text.
    pub fn kind_str(&self) -> &'static str {
        match self {
            OriginalPngValidationError::NotPng => "not_png",
            OriginalPngValidationError::TooLarge { .. } => "too_large",
            OriginalPngValidationError::DimensionMismatch { .. } => "dimension_mismatch",
            OriginalPngValidationError::DecodeFailed => "decode_failed",
            OriginalPngValidationError::RgbaMismatch => "rgba_mismatch",
        }
    }
}

impl fmt::Display for OriginalPngValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OriginalPngValidationError::NotPng => f.write_str("original png is not a png"),
            OriginalPngValidationError::TooLarge { size } => {
                write!(f, "original png exceeds the size cap ({size} bytes)")
            }
            OriginalPngValidationError::DimensionMismatch { declared, decoded } => write!(
                f,
                "original png dimensions {}x{} differ from the captured {}x{}",
                decoded.0, decoded.1, declared.0, declared.1,
            ),
            OriginalPngValidationError::DecodeFailed => {
                f.write_str("original png decoder refused the payload")
            }
            OriginalPngValidationError::RgbaMismatch => {
                f.write_str("original png decoded to an unexpected rgba frame")
            }
        }
    }
}

impl std::error::Error for OriginalPngValidationError {}

/// Mandatory ICC profile chunks the rebuilder can attach to a
/// freshly encoded PNG. The values are pre-compressed (deflate)
/// inside the chunk payload; the persistence layer treats the
/// chunk as opaque bytes.
///
/// The bridge constructs an [`IccChunk`] from a TIFF
/// `ICCProfile` IFD entry or from a PNG `iCCP` chunk the original
/// PNG carried. Either way the helper produces a single
/// `iCCP`-shaped chunk the persistence layer can splice into the
/// freshly encoded PNG without inspecting the bytes.
///
/// `compression_method` is `0` ("deflate") in every Profile the
/// spec allows; the field is exposed for the rare case where a
/// host publishes a future, non-deflate encoder.
#[derive(Debug, Clone)]
pub struct IccChunk {
    pub profile_name: String,
    pub compression_method: u8,
    pub compressed_profile: Vec<u8>,
}

/// Rebuild a PNG asset from the source RGBA frame plus the metadata
/// the bridge recovered.
///
/// The function is the persistence layer's last-mile response to
/// the documented failure mode the user reported: a 1104×396 px
/// screenshot from a Retina-class display carries the
/// resolution and the colour profile on the **TIFF leg** of the
/// pasteboard, not on the PNG leg. The PNG leg the macOS bridge
/// reads can be a pixel-identical bitmap that lacks `pHYs`,
/// `iCCP` / `sRGB` and any other ancillary chunk, so the asset
/// stored verbatim would report the canonical 72 ppi and no
/// profile. This helper rebuilds a new PNG that:
///
/// - keeps every pixel the source PNG delivered (the RGBA bytes
///   the bridge validated are the input here);
/// - inserts a `pHYs` chunk with the pixels-per-metre resolution
///   the bridge resolved;
/// - inserts an `iCCP` chunk for the ICC profile when the bridge
///   found one on the TIFF leg, or an `sRGB` chunk when the
///   bridge found an `sRGB` PNG marker;
/// - leaves the dimensions, the colour interpretation and the
///   alpha channel untouched.
///
/// When the source PNG already carried the same metadata the
/// bridge uses [`rebuild_png_for_native_png`] with an empty
/// metadata argument set, which produces a fresh 8-bit RGBA PNG
/// exactly like the legacy encoder. This is the deliberate
/// bridge behaviour: the asset store only calls this helper
/// when the source PNG's metadata disagrees with the
/// pasteboard-reported metadata, so the scenario "fidelity was
/// already preserved verbatim" never goes through the rebuild
/// path.
///
/// The helper returns [`AssetError::Encode`] for any encoder
/// failure. The ICC profile and the pHYs payloads are kept
/// metadata-only at the call site: the helper never logs the
/// payload bytes or echoes them through a free-form `Display`
/// formatter.
pub fn rebuild_png_with_metadata(
    width: u32,
    height: u32,
    rgba: &[u8],
    phys_payload: Option<[u8; 9]>,
    icc_chunk: Option<IccChunk>,
    srgb_intent: Option<u8>,
) -> Result<Vec<u8>, AssetError> {
    let expected = checked_rgba_len(width, height)
        .map_err(|_| AssetError::InvalidDimensions { width, height })?;
    if rgba.len() != expected {
        return Err(AssetError::InvalidDimensions { width, height });
    }

    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| AssetError::Encode {
            reason: error.to_string(),
        })?;

        if let Some(payload) = phys_payload {
            writer
                .write_chunk(png::chunk::ChunkType(*b"pHYs"), &payload)
                .map_err(|error| AssetError::Encode {
                    reason: error.to_string(),
                })?;
        }
        if let Some(icc) = &icc_chunk {
            let mut payload: Vec<u8> =
                Vec::with_capacity(icc.profile_name.len() + 1 + 1 + icc.compressed_profile.len());
            payload.extend_from_slice(icc.profile_name.as_bytes());
            payload.push(0);
            payload.push(icc.compression_method);
            payload.extend_from_slice(&icc.compressed_profile);
            writer
                .write_chunk(png::chunk::ChunkType(*b"iCCP"), &payload)
                .map_err(|error| AssetError::Encode {
                    reason: error.to_string(),
                })?;
        }
        if let Some(intent) = srgb_intent {
            writer
                .write_chunk(png::chunk::ChunkType(*b"sRGB"), &[intent])
                .map_err(|error| AssetError::Encode {
                    reason: error.to_string(),
                })?;
        }

        writer
            .write_image_data(rgba)
            .map_err(|error| AssetError::Encode {
                reason: error.to_string(),
            })?;
        writer.finish().map_err(|error| AssetError::Encode {
            reason: error.to_string(),
        })?;
    }

    if out.len() > MAX_CLIPBOARD_ASSET_BYTES {
        return Err(AssetError::TooLarge { size: out.len() });
    }

    Ok(out)
}

/// Compress an ICC profile payload with the canonical deflate
/// method `iCCP` uses. The helper exists because the bridge may
/// surface a raw ICC payload (`ICCProfile` IFD tag in TIFF,
/// [`crate::clipboard_image_png::PngMetadataSummary::icc_profile_chunk`]
/// in PNG) that the persistence layer still has to wrap into an
/// `iCCP`-shaped chunk.
///
/// The output is `zlib`-wrapped deflate so the PNG encoder can
/// write the chunk without inspecting the byte stream. The fn
/// traps any I/O error from the deflate encoder and surfaces it
/// as [`AssetError::Encode`] — the upstream contract does not
/// admit an `io::Error` because every PNG encoder failure must
/// already collapse to that variant.
pub fn deflate_icc_profile(profile: &[u8]) -> Result<Vec<u8>, AssetError> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    use std::io::Write as _;
    encoder
        .write_all(profile)
        .map_err(|error| AssetError::Encode {
            reason: error.to_string(),
        })?;
    encoder.finish().map_err(|error| AssetError::Encode {
        reason: error.to_string(),
    })
}

/// Decode a PNG asset back into a platform-neutral bitmap.
///
/// Used by the paste pipeline so a persisted image can be written back
/// to the clipboard without ever being converted to text. The decoder
/// enforces the same dimension and size bounds as the read validator.
///
/// The store writes 8-bit RGBA today, but a PNG captured by an
/// external tool or by an older ClipVault build can arrive in any of
/// the legal combinations the PNG spec permits — palette, grayscale,
/// grayscale + alpha, RGB and RGBA at the depths the standard allows —
/// and every valid variant must be readable through the bridge so the
/// card surface does not regress after a recompile. The decoder
/// therefore accepts every `(color_type, bit_depth)` pair the
/// standard considers valid and normalises the raw frame buffer to a
/// platform-neutral 8-bit RGBA bitmap that the paste pipeline can hand
/// straight to the clipboard adapter.
pub fn decode_png(bytes: &[u8]) -> Result<ClipboardImage, AssetError> {
    if bytes.len() > MAX_CLIPBOARD_ASSET_BYTES {
        return Err(AssetError::TooLarge { size: bytes.len() });
    }
    if !looks_like_png(bytes) {
        return Err(AssetError::NotPng);
    }
    let decoder = png::Decoder::new(io::Cursor::new(bytes));
    let mut reader = decoder.read_info().map_err(|_| AssetError::NotPng)?;
    let info = reader.info();
    let (width, height) = (info.width, info.height);
    if width == 0
        || height == 0
        || width > MAX_CLIPBOARD_IMAGE_DIM
        || height > MAX_CLIPBOARD_IMAGE_DIM
    {
        return Err(AssetError::InvalidDimensions { width, height });
    }
    // Copy the palette / transparency tables out of the reader so the
    // mutable borrow on `reader.next_frame` below does not conflict
    // with the immutable borrows the helper needs.
    let palette: Option<Vec<u8>> = info.palette.as_deref().map(|slice| slice.to_vec());
    let trns: Option<Vec<u8>> = info.trns.as_deref().map(|slice| slice.to_vec());
    let mut buffer = vec![0; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut buffer)
        .map_err(|_| AssetError::NotPng)?;
    buffer.truncate(frame.buffer_size());
    let rgba = expand_png_frame_to_rgba8(
        frame.color_type,
        frame.bit_depth,
        &buffer,
        palette.as_deref(),
        trns.as_deref(),
    )?;
    ClipboardImage::new(rgba, width, height)
        .map_err(|_| AssetError::InvalidDimensions { width, height })
}

/// Expand one decoded PNG frame into an 8-bit RGBA buffer.
///
/// The function accepts every `(ColorType, BitDepth)` combination the
/// PNG standard considers legal and converts the raw frame buffer
/// produced by the `png` crate into an 8-bit RGBA layout the paste
/// pipeline can forward to the clipboard adapter without inspecting
/// pixels. The implementation favours clarity over micro-optimisation:
/// each branch is a small, self-contained loop and the only metadata
/// the helper returns is the typed [`AssetError`] enum.
///
/// The conversion rules follow the PNG specification:
/// - 8/16-bit grayscale expand to RGBA by replicating the sample and
///   setting the alpha to opaque (`0xFF`);
/// - 8/16-bit grayscale + alpha become two-channel sources that are
///   mapped straight onto RGBA;
/// - 8/16-bit RGB samples are replicated as opaque red/green/blue;
/// - 8/16-bit RGBA is already the canonical layout;
/// - palette (1/2/4/8-bit indexed) samples are expanded through the
///   `PLTE` palette the `png` crate exposes via the reader; a `tRNS`
///   chunk provides the per-entry alpha values that the spec mandates
///   for partially transparent palettes.
///
/// 16-bit samples are downscaled to 8 bits by keeping the high byte;
/// the conversion is symmetric across every channel and never inspects
/// the pixel values, so a stale PNG with a different palette produces
/// the same byte layout a freshly captured one does.
fn expand_png_frame_to_rgba8(
    color_type: png::ColorType,
    bit_depth: png::BitDepth,
    raw: &[u8],
    palette: Option<&[u8]>,
    trns: Option<&[u8]>,
) -> Result<Vec<u8>, AssetError> {
    use png::ColorType::{Grayscale, GrayscaleAlpha, Indexed, Rgb, Rgba};

    let pixel_count =
        pixel_count_for_frame(raw.len(), color_type, bit_depth).ok_or(AssetError::NotPng)?;

    let mut rgba = vec![0u8; pixel_count.checked_mul(4).ok_or(AssetError::NotPng)?];

    match (color_type, bit_depth) {
        (Rgba, png::BitDepth::Eight) => rgba.copy_from_slice(raw),
        (Rgba, png::BitDepth::Sixteen) => {
            for (pixel, chunk) in rgba.chunks_exact_mut(4).zip(raw.chunks_exact(8)) {
                pixel[0] = chunk[0];
                pixel[1] = chunk[2];
                pixel[2] = chunk[4];
                pixel[3] = chunk[6];
            }
        }
        (Rgb, png::BitDepth::Eight) => {
            for (pixel, chunk) in rgba.chunks_exact_mut(4).zip(raw.chunks_exact(3)) {
                pixel[0] = chunk[0];
                pixel[1] = chunk[1];
                pixel[2] = chunk[2];
                pixel[3] = 0xFF;
            }
        }
        (Rgb, png::BitDepth::Sixteen) => {
            for (pixel, chunk) in rgba.chunks_exact_mut(4).zip(raw.chunks_exact(6)) {
                pixel[0] = chunk[0];
                pixel[1] = chunk[2];
                pixel[2] = chunk[4];
                pixel[3] = 0xFF;
            }
        }
        (Grayscale, png::BitDepth::Eight) => {
            for (pixel, sample) in rgba.chunks_exact_mut(4).zip(raw.iter()) {
                pixel[0] = *sample;
                pixel[1] = *sample;
                pixel[2] = *sample;
                pixel[3] = 0xFF;
            }
        }
        (Grayscale, png::BitDepth::Sixteen) => {
            for (pixel, chunk) in rgba.chunks_exact_mut(4).zip(raw.chunks_exact(2)) {
                pixel[0] = chunk[0];
                pixel[1] = chunk[0];
                pixel[2] = chunk[0];
                pixel[3] = 0xFF;
            }
        }
        (Grayscale, png::BitDepth::One)
        | (Grayscale, png::BitDepth::Two)
        | (Grayscale, png::BitDepth::Four) => {
            expand_packed_grayscale(bit_depth, raw, &mut rgba)?;
        }
        (GrayscaleAlpha, png::BitDepth::Eight) => {
            for (pixel, chunk) in rgba.chunks_exact_mut(4).zip(raw.chunks_exact(2)) {
                pixel[0] = chunk[0];
                pixel[1] = chunk[0];
                pixel[2] = chunk[0];
                pixel[3] = chunk[1];
            }
        }
        (GrayscaleAlpha, png::BitDepth::Sixteen) => {
            for (pixel, chunk) in rgba.chunks_exact_mut(4).zip(raw.chunks_exact(4)) {
                pixel[0] = chunk[0];
                pixel[1] = chunk[0];
                pixel[2] = chunk[0];
                pixel[3] = chunk[2];
            }
        }
        (Indexed, _) => {
            let palette = palette.ok_or(AssetError::NotPng)?;
            // The PLTE chunk must carry a multiple of three bytes so
            // the palette can be sliced into RGB triplets. A file
            // shorter than that is malformed: surface the same typed
            // rejection the old single-format decoder raised so the
            // failure surface does not change for foreign payloads.
            if palette.len() % 3 != 0 {
                return Err(AssetError::NotPng);
            }
            expand_packed_indexed(bit_depth, raw, palette, trns, &mut rgba)?;
        }
        // The remaining combinations (grayscale-alpha at sub-byte
        // depths, RGB at sub-byte depths) are illegal under the
        // PNG spec and are rejected by the decoder before reaching
        // here; surface them as a typed `NotPng` so a malformed
        // foreign payload cannot sneak through.
        _ => return Err(AssetError::NotPng),
    }
    Ok(rgba)
}

/// Return the number of pixels the supplied raw buffer covers, given
/// the colour type and bit depth of the frame the decoder produced.
///
/// The decoder hands us whole bytes: sub-byte depths (1/2/4) are
/// already packed one sample per bit/half-byte. Returning `None`
/// instead of `0` keeps `pixel_count == 0` for a malformed payload,
/// which the caller can then turn into the typed `NotPng` rejection.
fn pixel_count_for_frame(
    raw_len: usize,
    color_type: png::ColorType,
    bit_depth: png::BitDepth,
) -> Option<usize> {
    use png::ColorType::{Grayscale, GrayscaleAlpha, Indexed, Rgb, Rgba};

    let samples_per_pixel = match color_type {
        Grayscale | Indexed => 1u64,
        GrayscaleAlpha => 2u64,
        Rgb => 3u64,
        Rgba => 4u64,
    };
    let bits = u64::from(bit_depth as u8);
    if bits == 0 || bits > 16 {
        return None;
    }
    let len = u64::try_from(raw_len).ok()?;
    let pixels = if bits <= 8 {
        // Sub-byte depths pack multiple samples per byte. Each input
        // byte therefore carries `8 / bits` samples; dividing the
        // total sample count by `samples_per_pixel` gives the number
        // of pixels the frame covers.
        let samples_per_byte = 8u64 / bits;
        let total_samples = samples_per_byte.checked_mul(len)?;
        total_samples / samples_per_pixel
    } else {
        // 16-bit: each sample is two bytes; samples_per_pixel samples
        // therefore take `samples_per_pixel * 2` input bytes.
        let bytes_per_pixel = samples_per_pixel.checked_mul(2)?;
        len / bytes_per_pixel
    };
    usize::try_from(pixels).ok()
}

/// Expand a packed 1/2/4-bit grayscale frame into RGBA8.
///
/// The PNG spec packs the sub-byte samples one after another, MSB
/// first, so each input byte carries `8 / bit_depth` pixels. The
/// expansion is lossless — the sample is scaled to 8 bits by
/// replicating the high bits into the low bits, the same operation a
/// typical PNG decoder performs.
fn expand_packed_grayscale(
    bit_depth: png::BitDepth,
    raw: &[u8],
    rgba: &mut [u8],
) -> Result<(), AssetError> {
    let bits = bit_depth as u8;
    if bits == 0 || bits > 8 || 8 % bits != 0 {
        return Err(AssetError::NotPng);
    }
    let pixels_per_byte = 8 / bits;
    let max_value = (1u16 << bits) - 1;
    let mut out_index = 0;
    for byte in raw {
        for offset in 0..pixels_per_byte {
            if out_index + 4 > rgba.len() {
                return Err(AssetError::NotPng);
            }
            let shift = 8 - bits - offset * bits;
            let sample = ((*byte >> shift) as u16) & max_value;
            let scaled = scale_sample_to_u8(sample, max_value);
            rgba[out_index] = scaled;
            rgba[out_index + 1] = scaled;
            rgba[out_index + 2] = scaled;
            rgba[out_index + 3] = 0xFF;
            out_index += 4;
        }
    }
    if out_index != rgba.len() {
        return Err(AssetError::NotPng);
    }
    Ok(())
}

/// Expand a packed 1/2/4/8-bit indexed (palette) frame into RGBA8.
///
/// Each packed byte carries `8 / bit_depth` palette indices (one
/// index per byte for the 8-bit variant). The palette is sliced into
/// RGB triplets and a `tRNS` chunk — when present — supplies one
/// alpha byte per palette entry. Indices that fall outside the
/// declared palette are treated as a malformed payload and surface
/// the typed `NotPng` rejection the rest of the decoder already
/// raises, so the surface is consistent for foreign files.
fn expand_packed_indexed(
    bit_depth: png::BitDepth,
    raw: &[u8],
    palette: &[u8],
    trns: Option<&[u8]>,
    rgba: &mut [u8],
) -> Result<(), AssetError> {
    let bits = bit_depth as u8;
    if bits == 0 || bits > 8 || 8 % bits != 0 {
        return Err(AssetError::NotPng);
    }
    let palette_entries = palette.len() / 3;
    if palette_entries == 0 {
        return Err(AssetError::NotPng);
    }
    let pixels_per_byte = 8 / bits;
    let max_value = (1u16 << bits) - 1;
    let mut out_index = 0;
    let mut pixel_index = 0usize;
    for byte in raw {
        for offset in 0..pixels_per_byte {
            if out_index + 4 > rgba.len() {
                return Err(AssetError::NotPng);
            }
            let shift = 8 - bits - offset * bits;
            let sample = ((*byte >> shift) as u16) & max_value;
            if (sample as usize) >= palette_entries {
                return Err(AssetError::NotPng);
            }
            let base = sample as usize * 3;
            rgba[out_index] = palette[base];
            rgba[out_index + 1] = palette[base + 1];
            rgba[out_index + 2] = palette[base + 2];
            rgba[out_index + 3] = trns
                .and_then(|bytes| bytes.get(pixel_index).copied())
                .unwrap_or(0xFF);
            out_index += 4;
            pixel_index += 1;
        }
    }
    if out_index != rgba.len() {
        return Err(AssetError::NotPng);
    }
    Ok(())
}

/// Scale a sub-byte sample to its 8-bit equivalent.
///
/// The PNG spec packs a `bits`-wide sample into the high bits of a
/// byte; the conversion replicates those high bits into the low bits
/// so a 1-bit `0b1` maps to `0xFF` and a 4-bit `0b1010` maps to
/// `0xAA`. The implementation is identical to the standard PNG
/// decoder behaviour and never inspects pixel content.
fn scale_sample_to_u8(sample: u16, max_value: u16) -> u8 {
    if max_value == 0 {
        return 0;
    }
    let mut value = sample;
    let mut source_bits = log2_u16(max_value) + 1;
    let target_bits = 8u32;
    while source_bits < target_bits {
        value = (value << source_bits) | value;
        source_bits *= 2;
    }
    value as u8
}

fn log2_u16(value: u16) -> u32 {
    if value == 0 {
        return 0;
    }
    let mut bits = 0u32;
    let mut shifted = value;
    while shifted > 1 {
        shifted >>= 1;
        bits += 1;
    }
    bits
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use fmt::Write;
        // `write!` into a `String` cannot fail.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Verify the PNG magic header. A full parse happens in
/// [`decode_png`]; this cheap check short-circuits obvious garbage
/// before the decoder allocates.
fn looks_like_png(bytes: &[u8]) -> bool {
    bytes.len() >= PNG_SIGNATURE.len() && bytes[..PNG_SIGNATURE.len()] == PNG_SIGNATURE
}

/// Outcome of persisting a normalised image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreOutcome {
    /// A new asset file was created.
    Written { asset_ref: String },
    /// A valid asset with the same hash already existed and was
    /// reused; no second file was created.
    Reused { asset_ref: String },
}

impl StoreOutcome {
    pub fn asset_ref(&self) -> &str {
        match self {
            StoreOutcome::Written { asset_ref } | StoreOutcome::Reused { asset_ref } => asset_ref,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            StoreOutcome::Written { .. } => "written",
            StoreOutcome::Reused { .. } => "reused",
        }
    }
}

/// Filesystem-backed store for clipboard payload assets.
///
/// Cheap to clone: it only holds the resolved data directory.
#[derive(Debug, Clone)]
pub struct ClipboardAssetStore {
    data_dir: PathBuf,
}

impl ClipboardAssetStore {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    /// `<data_dir>/assets/clipboard`.
    pub fn root(&self) -> PathBuf {
        self.data_dir
            .join(clipvault_platform::ASSETS_DIR)
            .join(CLIPBOARD_ASSETS_DIR)
    }

    /// Persist `image` and return the relative reference.
    ///
    /// The write is atomic (temp file + `rename` inside the same
    /// directory) and idempotent: a second call with the same bytes
    /// reuses the existing file and reports [`StoreOutcome::Reused`].
    pub fn store_image(&self, image: &NormalizedImage) -> Result<StoreOutcome, AssetError> {
        let root = self.root();
        fs::create_dir_all(&root).map_err(io_error)?;
        let file_name = format!("{}.{CLIPBOARD_ASSET_EXTENSION}", image.hash());
        let target = root.join(&file_name);
        let asset_ref = image.asset_ref();

        // Reuse an existing, valid asset: the hash already proves the
        // bytes match, so re-encoding would only churn the disk.
        if target.exists() && read_validated_png(&target).is_ok() {
            return Ok(StoreOutcome::Reused { asset_ref });
        }

        // Temp file in the *same* directory so the rename stays on one
        // filesystem and is therefore atomic.
        let temp = root.join(format!(".{}.tmp", image.hash()));
        // A leftover temp file from an interrupted run must not make
        // this attempt fail.
        let _ = fs::remove_file(&temp);
        if let Err(error) = fs::write(&temp, image.png()) {
            let _ = fs::remove_file(&temp);
            return Err(io_error(error));
        }
        if let Err(error) = fs::rename(&temp, &target) {
            // Never leave a partial file behind.
            let _ = fs::remove_file(&temp);
            return Err(io_error(error));
        }
        Ok(StoreOutcome::Written { asset_ref })
    }

    /// Resolve a relative `asset_ref` to its canonical absolute path
    /// inside the clipboard asset namespace.
    ///
    /// The absolute path never leaves the backend: callers use it to
    /// read bytes and serve them through a command.
    pub fn resolve(&self, asset_ref: &str) -> Result<PathBuf, AssetError> {
        if asset_ref.is_empty() {
            return Err(AssetError::Empty);
        }
        let candidate = Path::new(asset_ref);
        if candidate.is_absolute() {
            return Err(AssetError::Absolute);
        }
        if candidate
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(AssetError::Traversal);
        }
        let prefix = format!("{CLIPBOARD_ASSETS_DIR}/");
        if !asset_ref.starts_with(&prefix) {
            return Err(AssetError::OutOfScope);
        }
        let relative = candidate
            .strip_prefix(CLIPBOARD_ASSETS_DIR)
            .map_err(|_| AssetError::OutOfScope)?;
        // The store writes flat file names only; a nested reference
        // would mean the caller invented a shape we never produce.
        if relative.components().count() != 1 {
            return Err(AssetError::OutOfScope);
        }

        let allowed_root = self.root();
        let canonical_root = fs::canonicalize(&allowed_root).map_err(|_| AssetError::NotFound)?;
        let full_path = allowed_root.join(relative);

        // Reject a symlink *before* canonicalising. The store only ever
        // creates regular files, so a link inside the namespace is a
        // tampering signal even when its target happens to be a
        // legitimate asset — and canonicalisation would otherwise hide
        // it by resolving to the real file.
        let metadata = fs::symlink_metadata(&full_path).map_err(|_| AssetError::NotFound)?;
        if metadata.file_type().is_symlink() {
            return Err(AssetError::Escaped);
        }
        if !metadata.is_file() {
            return Err(AssetError::NotFound);
        }

        let canonical = fs::canonicalize(&full_path).map_err(|_| AssetError::NotFound)?;
        if !canonical.starts_with(&canonical_root) {
            return Err(AssetError::Escaped);
        }
        Ok(canonical)
    }

    /// Read the validated PNG bytes behind `asset_ref`.
    ///
    /// This is the single entry point the Tauri asset command uses.
    /// Every rejection path returns a typed error and **no** bytes.
    pub fn read_bytes(&self, asset_ref: &str) -> Result<Vec<u8>, AssetError> {
        let path = self.resolve(asset_ref)?;
        read_validated_png(&path)
    }

    /// Run the full validation pipeline against `asset_ref` and report
    /// the metadata-only diagnostic that explains the outcome.
    ///
    /// Unlike [`Self::read_bytes`] this helper is intentionally
    /// never called from the production thumbnail surface: it is a
    /// diagnostic / triage entry point that distinguishes the failure
    /// modes the user-facing error message collapses. The returned
    /// [`AssetDiagnostic`] never carries bytes, a content hash or an
    /// absolute path; the metadata the diagnostic surfaces is enough
    /// for the frontend to drive a typed fallback ("wrong data_dir",
    /// "invalid PNG", "too large", …) without widening the privacy
    /// surface.
    pub fn diagnose(&self, asset_ref: &str) -> AssetDiagnostic {
        // Path-level validation: empty, absolute, traversal, foreign
        // namespace or symlink escape all collapse to a single
        // `InvalidReference` kind so the frontend surfaces a clear
        // "this row points at something the namespace will never
        // serve" message instead of guessing between the four
        // sub-cases.
        if let Err(error) = self.resolve(asset_ref) {
            match error {
                AssetError::Empty
                | AssetError::Absolute
                | AssetError::Traversal
                | AssetError::OutOfScope => {
                    return AssetDiagnostic {
                        kind: AssetDiagnosticKind::InvalidReference,
                    };
                }
                AssetError::Escaped => {
                    // The validator rejects symlinks inside the
                    // clipboard namespace with `Escaped`; for the
                    // diagnostic this is a "wrong namespace" signal
                    // (a foreign reference tried to escape the
                    // allowed root) rather than a malformed
                    // reference. The frontend can then drive a
                    // distinct fallback copy.
                    return AssetDiagnostic {
                        kind: AssetDiagnosticKind::WrongNamespace,
                    };
                }
                AssetError::NotFound => {
                    // Differentiate "no namespace directory" (which is
                    // a WrongDataDir signal — the store has never seen
                    // this data_dir) from "namespace exists, this
                    // file is missing". The check is metadata-only: it
                    // never logs or returns the path itself.
                    if !self.root().exists() {
                        return AssetDiagnostic {
                            kind: AssetDiagnosticKind::WrongDataDir,
                        };
                    }
                    return AssetDiagnostic {
                        kind: AssetDiagnosticKind::NotFound,
                    };
                }
                AssetError::Io { reason } => {
                    return AssetDiagnostic {
                        kind: AssetDiagnosticKind::Io { reason },
                    };
                }
                // Path-level validation only ever surfaces the
                // variants above. The remaining cases
                // (size / PNG / dimensions / encode / original
                // validation) require an actual file read or a
                // capture-time validation step and cannot be
                // produced by `resolve`, so a future addition that
                // introduces a new variant here would surface as a
                // compiler error — exactly what we want.
                AssetError::TooLarge { .. }
                | AssetError::NotPng
                | AssetError::InvalidDimensions { .. }
                | AssetError::Encode { .. }
                | AssetError::OriginalPngValidation(_) => {
                    return AssetDiagnostic {
                        kind: AssetDiagnosticKind::InvalidReference,
                    };
                }
            }
        }

        // Path-level validation succeeded, so the file lives inside
        // the allowed root. Re-resolve to obtain the canonical path;
        // a `NotFound` between the first resolve and the read would
        // indicate the namespace was deleted mid-call.
        let path = match self.resolve(asset_ref) {
            Ok(path) => path,
            Err(_) => {
                return AssetDiagnostic {
                    kind: AssetDiagnosticKind::NotFound,
                };
            }
        };
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => {
                return AssetDiagnostic {
                    kind: AssetDiagnosticKind::NotFound,
                };
            }
        };
        if metadata.file_type().is_symlink() {
            return AssetDiagnostic {
                kind: AssetDiagnosticKind::WrongNamespace,
            };
        }
        if !metadata.is_file() {
            return AssetDiagnostic {
                kind: AssetDiagnosticKind::NotFound,
            };
        }
        let size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
        if size > MAX_CLIPBOARD_ASSET_BYTES {
            return AssetDiagnostic {
                kind: AssetDiagnosticKind::TooLarge { size },
            };
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return AssetDiagnostic {
                    kind: AssetDiagnosticKind::Io {
                        reason: error.kind().to_string(),
                    },
                };
            }
        };
        if bytes.len() > MAX_CLIPBOARD_ASSET_BYTES {
            return AssetDiagnostic {
                kind: AssetDiagnosticKind::TooLarge { size: bytes.len() },
            };
        }
        if !looks_like_png(&bytes) {
            return AssetDiagnostic {
                kind: AssetDiagnosticKind::InvalidPng {
                    color_type: "unknown",
                    bit_depth: "unknown",
                },
            };
        }
        let decoder = png::Decoder::new(io::Cursor::new(&bytes));
        let mut reader = match decoder.read_info() {
            Ok(reader) => reader,
            Err(_) => {
                return AssetDiagnostic {
                    kind: AssetDiagnosticKind::InvalidPng {
                        color_type: "unknown",
                        bit_depth: "unknown",
                    },
                };
            }
        };
        let info = reader.info();
        let (width, height) = (info.width, info.height);
        // Snapshot the labels the diagnostic surfaces into `'static`
        // strings before `reader.next_frame` mutably borrows the
        // reader; otherwise the immutable borrow on `info` would
        // conflict with the mutable one on `next_frame` in the error
        // arms.
        let color_type_label = AssetDiagnosticKind::color_type_label(info.color_type);
        let bit_depth_label = AssetDiagnosticKind::bit_depth_label(info.bit_depth);
        if width == 0
            || height == 0
            || width > MAX_CLIPBOARD_IMAGE_DIM
            || height > MAX_CLIPBOARD_IMAGE_DIM
        {
            return AssetDiagnostic {
                kind: AssetDiagnosticKind::InvalidDimensions { width, height },
            };
        }
        // Copy the palette / transparency tables out of the reader so
        // the mutable borrow on `reader.next_frame` below does not
        // conflict with the immutable borrows the helper needs.
        let palette: Option<Vec<u8>> = info.palette.as_deref().map(|slice| slice.to_vec());
        let trns: Option<Vec<u8>> = info.trns.as_deref().map(|slice| slice.to_vec());
        let mut buffer = vec![0; reader.output_buffer_size()];
        let frame = match reader.next_frame(&mut buffer) {
            Ok(frame) => frame,
            Err(_) => {
                return AssetDiagnostic {
                    kind: AssetDiagnosticKind::InvalidPng {
                        color_type: color_type_label,
                        bit_depth: bit_depth_label,
                    },
                };
            }
        };
        buffer.truncate(frame.buffer_size());
        match expand_png_frame_to_rgba8(
            frame.color_type,
            frame.bit_depth,
            &buffer,
            palette.as_deref(),
            trns.as_deref(),
        ) {
            Ok(_) => AssetDiagnostic {
                kind: AssetDiagnosticKind::Loaded,
            },
            Err(_) => AssetDiagnostic {
                kind: AssetDiagnosticKind::InvalidPng {
                    color_type: color_type_label,
                    bit_depth: bit_depth_label,
                },
            },
        }
    }

    /// Delete every file in the clipboard namespace whose name is not
    /// referenced by `referenced`.
    ///
    /// The caller is responsible for computing `referenced` from
    /// SQLite *after* the data mutation has committed, so an asset that
    /// is still referenced by another row is never a candidate.
    ///
    /// The collector is idempotent, never recurses out of the
    /// namespace and never touches the `ignored-apps` or
    /// `application-icons` directories. A file it does not recognise
    /// (a directory, a leftover `.tmp`, a foreign extension) is left
    /// alone rather than deleted, except for stale temporaries which
    /// are safe to reclaim because the store only creates them inside
    /// a single `store_image` call.
    ///
    /// Returns the number of assets removed.
    pub fn collect_unreferenced(&self, referenced: &BTreeSet<String>) -> Result<usize, AssetError> {
        let root = self.root();
        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            // No namespace directory yet: nothing to collect. This is a
            // success, not an error, so retention can always run.
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(io_error(error)),
        };

        let mut removed = 0usize;
        for entry in entries {
            let entry = entry.map_err(io_error)?;
            // `file_type` does not follow symlinks, so a symlink placed
            // in the namespace is not treated as a regular file and is
            // therefore never followed nor deleted.
            let file_type = entry.file_type().map_err(io_error)?;
            if !file_type.is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            // Reclaim our own stale temporaries; leave anything else.
            if name.starts_with('.') && name.ends_with(".tmp") {
                if fs::remove_file(entry.path()).is_ok() {
                    removed += 1;
                }
                continue;
            }
            if !name.ends_with(&format!(".{CLIPBOARD_ASSET_EXTENSION}")) {
                continue;
            }
            let candidate_ref = format!("{CLIPBOARD_ASSETS_DIR}/{name}");
            if referenced.contains(&candidate_ref) {
                continue;
            }
            if fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

fn read_validated_png(path: &Path) -> Result<Vec<u8>, AssetError> {
    // `symlink_metadata` does not follow links: a symlink inside the
    // namespace is rejected before it is ever opened. The canonical
    // comparison in `resolve` already rejects links that escape the
    // root; this check additionally refuses links that stay inside it,
    // because the store never creates one.
    let metadata = fs::symlink_metadata(path).map_err(|_| AssetError::NotFound)?;
    if metadata.file_type().is_symlink() {
        return Err(AssetError::Escaped);
    }
    if !metadata.is_file() {
        return Err(AssetError::NotFound);
    }
    let size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
    if size > MAX_CLIPBOARD_ASSET_BYTES {
        // Reject on metadata so an oversized file is never read into
        // memory in the first place.
        return Err(AssetError::TooLarge { size });
    }
    let bytes = fs::read(path).map_err(io_error)?;
    if bytes.len() > MAX_CLIPBOARD_ASSET_BYTES {
        return Err(AssetError::TooLarge { size: bytes.len() });
    }
    if !looks_like_png(&bytes) {
        return Err(AssetError::NotPng);
    }
    // Full decode: a truncated or corrupt PNG must not reach the
    // webview, and the dimensions have to stay inside the bounds.
    decode_png(&bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bitmap(width: u32, height: u32, fill: u8) -> ClipboardImage {
        let len = (width as usize) * (height as usize) * 4;
        ClipboardImage::new(vec![fill; len], width, height).expect("valid bitmap")
    }

    fn store() -> (tempfile::TempDir, ClipboardAssetStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ClipboardAssetStore::new(dir.path());
        (dir, store)
    }

    #[test]
    fn normalize_produces_a_canonical_png_with_a_sha256_name() {
        let normalized = normalize_image(&bitmap(4, 3, 0x40)).expect("normalize");
        assert!(looks_like_png(normalized.png()));
        assert_eq!(normalized.width(), 4);
        assert_eq!(normalized.height(), 3);
        assert_eq!(normalized.hash().len(), 64, "sha256 hex is 64 chars");
        assert!(
            normalized
                .hash()
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "hash must be lowercase hex"
        );
        assert_eq!(
            normalized.asset_ref(),
            format!("clipboard/{}.png", normalized.hash())
        );
        assert_eq!(normalized.byte_len(), normalized.png().len());
    }

    #[test]
    fn normalize_is_deterministic_for_identical_pixels() {
        // This is what makes the hash a usable dedupe key: the same
        // bitmap must always produce byte-identical PNG output, across
        // calls and across restarts.
        let first = normalize_image(&bitmap(8, 8, 0x11)).expect("first");
        let second = normalize_image(&bitmap(8, 8, 0x11)).expect("second");
        assert_eq!(first.png(), second.png());
        assert_eq!(first.hash(), second.hash());
    }

    #[test]
    fn normalize_distinguishes_different_pixels_and_geometry() {
        let base = normalize_image(&bitmap(8, 8, 0x11)).expect("base");
        let other_pixels = normalize_image(&bitmap(8, 8, 0x22)).expect("pixels");
        let other_geometry = normalize_image(&bitmap(4, 16, 0x11)).expect("geometry");
        assert_ne!(base.hash(), other_pixels.hash());
        assert_ne!(base.hash(), other_geometry.hash());
    }

    #[test]
    fn normalized_debug_never_leaks_bytes_or_hash() {
        let normalized = normalize_image(&bitmap(2, 2, 0xAB)).expect("normalize");
        let rendered = format!("{normalized:?}");
        assert!(rendered.contains("width"));
        assert!(rendered.contains("png_len"));
        assert!(
            !rendered.contains(normalized.hash()),
            "the hash must not appear in Debug output"
        );
    }

    #[test]
    fn round_trip_encode_then_decode_preserves_pixels() {
        let original = bitmap(5, 7, 0x7F);
        let normalized = normalize_image(&original).expect("normalize");
        let decoded = decode_png(normalized.png()).expect("decode");
        assert_eq!(decoded.width(), original.width());
        assert_eq!(decoded.height(), original.height());
        assert_eq!(decoded.rgba(), original.rgba());
    }

    #[test]
    fn decode_rejects_non_png_and_corrupt_payloads() {
        assert_eq!(decode_png(b"definitely not a png"), Err(AssetError::NotPng));
        // Valid signature, truncated body: the decoder must refuse
        // instead of handing partial pixels to the webview.
        let mut truncated = PNG_SIGNATURE.to_vec();
        truncated.extend_from_slice(b"\x00\x00\x00\rIHDR-broken");
        assert_eq!(decode_png(&truncated), Err(AssetError::NotPng));
    }

    #[test]
    fn store_writes_then_reuses_the_same_asset() {
        let (_dir, store) = store();
        let normalized = normalize_image(&bitmap(6, 6, 0x33)).expect("normalize");

        let first = store.store_image(&normalized).expect("write");
        assert_eq!(first.kind(), "written");
        assert_eq!(first.asset_ref(), normalized.asset_ref());

        let second = store.store_image(&normalized).expect("reuse");
        assert_eq!(second.kind(), "reused");
        assert_eq!(second.asset_ref(), first.asset_ref());

        // Exactly one file on disk.
        let files: Vec<_> = fs::read_dir(store.root())
            .expect("read_dir")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(files.len(), 1, "reuse must not create a second file");
    }

    #[test]
    fn store_leaves_no_temporary_behind_on_success() {
        let (_dir, store) = store();
        let normalized = normalize_image(&bitmap(3, 3, 0x55)).expect("normalize");
        store.store_image(&normalized).expect("write");
        let temporaries: Vec<_> = fs::read_dir(store.root())
            .expect("read_dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(temporaries.is_empty(), "atomic write must clean up");
    }

    #[test]
    fn store_recovers_from_a_leftover_temporary() {
        // Simulate a crash between `write` and `rename`: the stale temp
        // file must not block the next attempt.
        let (_dir, store) = store();
        let normalized = normalize_image(&bitmap(3, 3, 0x66)).expect("normalize");
        fs::create_dir_all(store.root()).expect("mkdir");
        let stale = store.root().join(format!(".{}.tmp", normalized.hash()));
        fs::write(&stale, b"partial garbage").expect("stale temp");

        let outcome = store.store_image(&normalized).expect("write");
        assert_eq!(outcome.kind(), "written");
        assert!(!stale.exists(), "stale temp must be replaced");
        let bytes = store.read_bytes(outcome.asset_ref()).expect("read back");
        assert_eq!(bytes, normalized.png());
    }

    #[test]
    fn store_replaces_a_corrupt_existing_asset_instead_of_serving_it() {
        let (_dir, store) = store();
        let normalized = normalize_image(&bitmap(4, 4, 0x77)).expect("normalize");
        fs::create_dir_all(store.root()).expect("mkdir");
        let target = store.root().join(format!("{}.png", normalized.hash()));
        fs::write(&target, b"not a png at all").expect("corrupt file");

        let outcome = store.store_image(&normalized).expect("rewrite");
        assert_eq!(
            outcome.kind(),
            "written",
            "a corrupt asset must be rewritten, not reused"
        );
        assert_eq!(
            store.read_bytes(outcome.asset_ref()).expect("read"),
            normalized.png()
        );
    }

    #[test]
    fn read_bytes_round_trips_a_persisted_asset() {
        let (_dir, store) = store();
        let normalized = normalize_image(&bitmap(9, 2, 0x21)).expect("normalize");
        let outcome = store.store_image(&normalized).expect("write");
        let bytes = store.read_bytes(outcome.asset_ref()).expect("read");
        assert_eq!(bytes, normalized.png());
    }

    #[test]
    fn read_bytes_rejects_an_empty_reference() {
        let (_dir, store) = store();
        assert_eq!(store.read_bytes("").unwrap_err(), AssetError::Empty);
    }

    #[test]
    fn read_bytes_rejects_absolute_paths() {
        let (_dir, store) = store();
        for reference in ["/etc/passwd", "/tmp/clipboard/x.png"] {
            assert_eq!(
                store.read_bytes(reference).unwrap_err(),
                AssetError::Absolute,
                "{reference} must be rejected as absolute"
            );
        }
    }

    #[test]
    fn read_bytes_rejects_traversal() {
        let (_dir, store) = store();
        for reference in [
            "clipboard/../../etc/passwd",
            "clipboard/sub/../../escape.png",
            "../clipboard/x.png",
        ] {
            assert_eq!(
                store.read_bytes(reference).unwrap_err(),
                AssetError::Traversal,
                "{reference} must be rejected as traversal"
            );
        }
    }

    #[test]
    fn read_bytes_rejects_foreign_namespaces() {
        // Namespace isolation: a blacklist-icon or source-app-icon
        // reference must never resolve through the clipboard bridge.
        let (_dir, store) = store();
        for reference in [
            "ignored-apps/com.apple.textedit.png",
            "application-icons/com.apple.Terminal.png",
            "x.png",
            "clipboardx/y.png",
            "clipboard/nested/y.png",
        ] {
            assert_eq!(
                store.read_bytes(reference).unwrap_err(),
                AssetError::OutOfScope,
                "{reference} must be rejected as out of scope"
            );
        }
    }

    #[test]
    fn read_bytes_reports_a_missing_asset() {
        let (_dir, store) = store();
        fs::create_dir_all(store.root()).expect("mkdir");
        assert_eq!(
            store.read_bytes("clipboard/ghost.png").unwrap_err(),
            AssetError::NotFound
        );
    }

    #[test]
    fn read_bytes_reports_not_found_when_the_namespace_is_absent() {
        let (_dir, store) = store();
        assert_eq!(
            store.read_bytes("clipboard/ghost.png").unwrap_err(),
            AssetError::NotFound
        );
    }

    #[cfg(unix)]
    #[test]
    fn read_bytes_rejects_a_symlink_escaping_the_namespace() {
        let (dir, store) = store();
        fs::create_dir_all(store.root()).expect("mkdir");
        let outside = dir.path().join("outside.png");
        let normalized = normalize_image(&bitmap(2, 2, 0x01)).expect("normalize");
        fs::write(&outside, normalized.png()).expect("write outside");
        std::os::unix::fs::symlink(&outside, store.root().join("escape.png")).expect("symlink");
        let error = store.read_bytes("clipboard/escape.png").unwrap_err();
        assert!(
            matches!(error, AssetError::Escaped),
            "expected Escaped, got {error:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn read_bytes_rejects_a_symlink_that_stays_inside_the_namespace() {
        // The store never creates a symlink, so one inside the
        // namespace is a tampering signal even when its target is a
        // legitimate asset.
        let (_dir, store) = store();
        let normalized = normalize_image(&bitmap(2, 2, 0x02)).expect("normalize");
        let outcome = store.store_image(&normalized).expect("write");
        let real = store.root().join(format!("{}.png", normalized.hash()));
        std::os::unix::fs::symlink(&real, store.root().join("alias.png")).expect("symlink");
        assert_eq!(
            store.read_bytes("clipboard/alias.png").unwrap_err(),
            AssetError::Escaped
        );
        // The genuine reference still works.
        assert!(store.read_bytes(outcome.asset_ref()).is_ok());
    }

    #[test]
    fn read_bytes_rejects_a_non_png_payload_in_the_namespace() {
        let (_dir, store) = store();
        fs::create_dir_all(store.root()).expect("mkdir");
        fs::write(store.root().join("fake.png"), b"not a png").expect("write");
        assert_eq!(
            store.read_bytes("clipboard/fake.png").unwrap_err(),
            AssetError::NotPng
        );
    }

    #[test]
    fn read_bytes_rejects_a_corrupt_png_before_serving_bytes() {
        let (_dir, store) = store();
        fs::create_dir_all(store.root()).expect("mkdir");
        let normalized = normalize_image(&bitmap(4, 4, 0x03)).expect("normalize");
        // Valid signature and header, truncated data chunk.
        let truncated = &normalized.png()[..normalized.png().len() / 2];
        fs::write(store.root().join("corrupt.png"), truncated).expect("write");
        assert_eq!(
            store.read_bytes("clipboard/corrupt.png").unwrap_err(),
            AssetError::NotPng
        );
    }

    #[test]
    fn read_bytes_enforces_the_size_cap_without_reading_the_file() {
        let (_dir, store) = store();
        fs::create_dir_all(store.root()).expect("mkdir");
        let path = store.root().join("huge.png");
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.resize(MAX_CLIPBOARD_ASSET_BYTES + 1, b'X');
        fs::write(&path, &bytes).expect("write");
        match store.read_bytes("clipboard/huge.png").unwrap_err() {
            AssetError::TooLarge { size } => assert_eq!(size, bytes.len()),
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    #[test]
    fn collect_removes_only_unreferenced_assets() {
        let (_dir, store) = store();
        let kept = normalize_image(&bitmap(4, 4, 0x0A)).expect("kept");
        let dropped = normalize_image(&bitmap(4, 4, 0x0B)).expect("dropped");
        let kept_ref = store
            .store_image(&kept)
            .expect("write kept")
            .asset_ref()
            .to_string();
        let dropped_ref = store
            .store_image(&dropped)
            .expect("write dropped")
            .asset_ref()
            .to_string();

        let mut referenced = BTreeSet::new();
        referenced.insert(kept_ref.clone());

        let removed = store.collect_unreferenced(&referenced).expect("collect");
        assert_eq!(removed, 1);
        assert!(store.read_bytes(&kept_ref).is_ok(), "referenced asset kept");
        assert_eq!(
            store.read_bytes(&dropped_ref).unwrap_err(),
            AssetError::NotFound
        );
    }

    #[test]
    fn collect_keeps_an_asset_shared_by_two_references() {
        // A single file referenced under the same name by two rows must
        // survive as long as the reference is in the live set.
        let (_dir, store) = store();
        let shared = normalize_image(&bitmap(4, 4, 0x0C)).expect("shared");
        let shared_ref = store
            .store_image(&shared)
            .expect("write")
            .asset_ref()
            .to_string();
        let mut referenced = BTreeSet::new();
        referenced.insert(shared_ref.clone());
        assert_eq!(store.collect_unreferenced(&referenced).expect("collect"), 0);
        assert!(store.read_bytes(&shared_ref).is_ok());
    }

    #[test]
    fn collect_is_idempotent_and_safe_on_an_empty_namespace() {
        let (_dir, store) = store();
        let referenced = BTreeSet::new();
        // No directory yet: not an error.
        assert_eq!(store.collect_unreferenced(&referenced).expect("first"), 0);

        let orphan = normalize_image(&bitmap(2, 2, 0x0D)).expect("orphan");
        store.store_image(&orphan).expect("write");
        assert_eq!(store.collect_unreferenced(&referenced).expect("second"), 1);
        // Running again removes nothing and still succeeds.
        assert_eq!(store.collect_unreferenced(&referenced).expect("third"), 0);
    }

    #[test]
    fn collect_reclaims_stale_temporaries_and_leaves_foreign_files_alone() {
        let (_dir, store) = store();
        fs::create_dir_all(store.root()).expect("mkdir");
        fs::write(store.root().join(".abc.tmp"), b"partial").expect("temp");
        fs::write(store.root().join("notes.txt"), b"foreign").expect("foreign");

        let removed = store
            .collect_unreferenced(&BTreeSet::new())
            .expect("collect");
        assert_eq!(removed, 1, "only the stale temporary is reclaimed");
        assert!(
            store.root().join("notes.txt").exists(),
            "an unrecognised file must be left alone"
        );
    }

    #[test]
    fn collect_never_touches_the_sibling_asset_namespaces() {
        // `ignored-apps/` and `application-icons/` belong to other
        // capabilities; the clipboard collector must not walk into them.
        let (dir, store) = store();
        let assets = dir.path().join(clipvault_platform::ASSETS_DIR);
        for namespace in ["ignored-apps", "application-icons"] {
            let path = assets.join(namespace);
            fs::create_dir_all(&path).expect("mkdir");
            fs::write(path.join("keep.png"), b"\x89PNG\r\n\x1a\n icon").expect("write");
        }
        let orphan = normalize_image(&bitmap(2, 2, 0x0E)).expect("orphan");
        store.store_image(&orphan).expect("write");

        assert_eq!(
            store
                .collect_unreferenced(&BTreeSet::new())
                .expect("collect"),
            1
        );
        for namespace in ["ignored-apps", "application-icons"] {
            assert!(
                assets.join(namespace).join("keep.png").exists(),
                "{namespace} must be untouched"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn collect_does_not_follow_or_delete_symlinks() {
        let (dir, store) = store();
        fs::create_dir_all(store.root()).expect("mkdir");
        let outside = dir.path().join("precious.png");
        fs::write(&outside, b"\x89PNG\r\n\x1a\n precious").expect("write");
        std::os::unix::fs::symlink(&outside, store.root().join("link.png")).expect("symlink");

        store
            .collect_unreferenced(&BTreeSet::new())
            .expect("collect");
        assert!(outside.exists(), "the symlink target must survive");
    }

    #[test]
    fn error_kind_strings_are_stable() {
        let cases: [(AssetError, &str); 12] = [
            (AssetError::Empty, "empty"),
            (AssetError::Absolute, "absolute"),
            (AssetError::Traversal, "traversal"),
            (AssetError::OutOfScope, "out_of_scope"),
            (AssetError::NotFound, "not_found"),
            (AssetError::Escaped, "escaped"),
            (AssetError::Io { reason: "x".into() }, "io"),
            (AssetError::TooLarge { size: 1 }, "too_large"),
            (AssetError::NotPng, "not_png"),
            (
                AssetError::InvalidDimensions {
                    width: 0,
                    height: 0,
                },
                "invalid_dimensions",
            ),
            (AssetError::Encode { reason: "x".into() }, "encode"),
            (
                AssetError::OriginalPngValidation(OriginalPngValidationError::NotPng),
                "original_png_validation",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(error.kind_str(), expected);
        }
    }

    #[test]
    fn error_messages_never_include_an_absolute_path() {
        // Privacy contract: an error surfaced to the user or a log must
        // not disclose where the data directory lives.
        let (_dir, store) = store();
        let errors = [
            store.read_bytes("clipboard/ghost.png").unwrap_err(),
            store.read_bytes("/etc/passwd").unwrap_err(),
            store.read_bytes("clipboard/../x.png").unwrap_err(),
        ];
        for error in errors {
            let rendered = error.to_string();
            assert!(!rendered.contains('/') || !rendered.contains("/Users"));
            assert!(!rendered.contains(&store.root().display().to_string()));
        }
    }

    #[test]
    fn sha256_hex_matches_the_known_digest_of_the_empty_input() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn namespace_constants_are_stable_and_distinct() {
        assert_eq!(CLIPBOARD_ASSETS_DIR, "clipboard");
        assert_eq!(CLIPBOARD_ASSET_EXTENSION, "png");
        assert_ne!(CLIPBOARD_ASSETS_DIR, clipvault_platform::IGNORED_APPS_DIR);
        assert_ne!(
            CLIPBOARD_ASSETS_DIR,
            clipvault_platform::APPLICATION_ICONS_DIR
        );
    }

    // -----------------------------------------------------------------
    // `decode_png` compatibility coverage.
    //
    // The PNG spec permits a wide range of `(ColorType, BitDepth)`
    // combinations. Older ClipVault builds or external tools can
    // write any of them; the bridge must therefore accept every
    // combination the standard considers legal and normalise it to a
    // platform-neutral RGBA8 bitmap the paste pipeline can hand to
    // the clipboard adapter without inspecting pixels.
    //
    // The tests below cover each legal combination explicitly:
    // RGB8, RGBA8, indexed (1/2/4/8-bit palette), grayscale (1/2/4/8
    // /16-bit) and grayscale + alpha (8/16-bit). They round-trip
    // through the asset store, prove the bytes the decoder produces
    // match the canonical 8-bit RGBA layout, and pin the failure
    // surface for the truly malformed inputs.
    // -----------------------------------------------------------------

    /// Build a PNG with the requested color type, bit depth and a
    /// 2x2 bitmap of identical RGBA values. The PNG crate's encoder
    /// accepts the same set of combinations the decoder does; the
    /// helper makes the test inputs self-describing.
    fn build_png(color_type: png::ColorType, bit_depth: png::BitDepth, rgba: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 2, 2);
            encoder.set_color(color_type);
            encoder.set_depth(bit_depth);
            let mut writer = encoder
                .write_header()
                .expect("write header for legal color type / bit depth");
            let bytes = match (color_type, bit_depth) {
                (png::ColorType::Grayscale, png::BitDepth::Eight) => {
                    vec![rgba[0]; 4]
                }
                (png::ColorType::GrayscaleAlpha, png::BitDepth::Eight) => {
                    // 2x2 image, 2 bytes per pixel = 8 bytes total.
                    std::iter::repeat_n(&[rgba[0], rgba[3]][..], 4)
                        .flatten()
                        .copied()
                        .collect()
                }
                (png::ColorType::Rgb, png::BitDepth::Eight) => {
                    // 2x2 image, 3 bytes per pixel = 12 bytes total.
                    std::iter::repeat_n(&[rgba[0], rgba[1], rgba[2]][..], 4)
                        .flatten()
                        .copied()
                        .collect()
                }
                (png::ColorType::Rgba, png::BitDepth::Eight) => {
                    // 2x2 image, 4 bytes per pixel = 16 bytes total.
                    std::iter::repeat_n(&[rgba[0], rgba[1], rgba[2], rgba[3]][..], 4)
                        .flatten()
                        .copied()
                        .collect()
                }
                _ => panic!("unsupported encoder combination for fixture"),
            };
            writer.write_image_data(&bytes).expect("write image data");
            writer.finish().expect("finish encoder");
        }
        out
    }

    fn expected_rgba(width: u32, height: u32, fill: [u8; 4]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            bytes.extend_from_slice(&fill);
        }
        bytes
    }

    #[test]
    fn decode_png_reads_legacy_rgba8_assets() {
        let normalized = normalize_image(&bitmap(4, 4, 0x55)).expect("normalize");
        let bytes = normalized.png();
        let image = decode_png(bytes).expect("decode");
        assert_eq!(image.width(), 4);
        assert_eq!(image.height(), 4);
        assert_eq!(image.rgba(), bytes_to_rgba_repeat(0x55, 16));
    }

    #[test]
    fn decode_png_reads_legacy_rgb8_assets() {
        // Encode a 2x2 RGB8 PNG manually so the fixture is decoupled
        // from the rest of the store.
        let png = build_png(
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            [0x10, 0x20, 0x30, 0xFF],
        );
        let image = decode_png(&png).expect("decode RGB8");
        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 2);
        assert_eq!(image.rgba(), expected_rgba(2, 2, [0x10, 0x20, 0x30, 0xFF]));
    }

    #[test]
    fn decode_png_reads_indexed_palette_assets() {
        // Two-entry palette: index 0 = red, index 1 = blue.
        let mut palette_bytes = Vec::new();
        palette_bytes.extend_from_slice(&[0xFF, 0x00, 0x00]); // entry 0
        palette_bytes.extend_from_slice(&[0x00, 0x00, 0xFF]); // entry 1
                                                              // 2x2 image, row-major: [0, 1, 0, 1] → red, blue, red, blue.
        let indices = [0u8, 1, 0, 1];
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 2, 2);
            encoder.set_color(png::ColorType::Indexed);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_palette(palette_bytes.clone());
            let mut writer = encoder.write_header().expect("header");
            writer.write_image_data(&indices).expect("data");
            writer.finish().expect("finish");
        }
        let image = decode_png(&out).expect("decode indexed");
        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 2);
        // Index 0 (red) at pixel 0 → [0xFF, 0, 0, 0xFF].
        assert_eq!(&image.rgba()[0..4], &[0xFF, 0x00, 0x00, 0xFF]);
        // Index 1 (blue) at pixel 1 → [0, 0, 0xFF, 0xFF].
        assert_eq!(&image.rgba()[4..8], &[0x00, 0x00, 0xFF, 0xFF]);
        // Index 0 (red) at pixel 2 → [0xFF, 0, 0, 0xFF].
        assert_eq!(&image.rgba()[8..12], &[0xFF, 0x00, 0x00, 0xFF]);
        // Index 1 (blue) at pixel 3 → [0, 0, 0xFF, 0xFF].
        assert_eq!(&image.rgba()[12..16], &[0x00, 0x00, 0xFF, 0xFF]);
    }

    #[test]
    fn decode_png_reads_grayscale_8bit_assets() {
        let png = build_png(
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            [0x80, 0, 0, 0xFF],
        );
        let image = decode_png(&png).expect("decode grayscale 8");
        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 2);
        assert_eq!(image.rgba(), expected_rgba(2, 2, [0x80, 0x80, 0x80, 0xFF]));
    }

    #[test]
    fn decode_png_reads_grayscale_alpha_8bit_assets() {
        // The PNG fixture above for grayscale + alpha writes
        // `[gray, alpha]` so the decoded RGBA pixels must carry the
        // alpha byte verbatim.
        let png = build_png(
            png::ColorType::GrayscaleAlpha,
            png::BitDepth::Eight,
            [0x40, 0, 0, 0x80],
        );
        let image = decode_png(&png).expect("decode gray + alpha 8");
        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 2);
        assert_eq!(image.rgba(), expected_rgba(2, 2, [0x40, 0x40, 0x40, 0x80]));
    }

    #[test]
    fn decode_png_reads_grayscale_4bit_assets() {
        // 4-bit grayscale is illegal through the encoder helper
        // (the encoder requires a byte per sample); emit the PNG by
        // hand with two rows of `[0x10, 0x10]` (two samples packed
        // per byte, both equal to 0x1).
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 4, 2);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Four);
            let mut writer = encoder.write_header().expect("header");
            // 4 pixels per row, 2 rows: 2 packed bytes per row.
            writer
                .write_image_data(&[0x11, 0x11, 0x11, 0x11])
                .expect("data");
            writer.finish().expect("finish");
        }
        let image = decode_png(&out).expect("decode grayscale 4");
        assert_eq!(image.width(), 4);
        assert_eq!(image.height(), 2);
        // 4-bit sample `0x1` scales to 8-bit `0x11` through the
        // standard decoder scaling. Every pixel must carry that
        // value on every channel plus opaque alpha.
        let expected = expected_rgba(4, 2, [0x11, 0x11, 0x11, 0xFF]);
        assert_eq!(image.rgba(), expected);
    }

    #[test]
    fn decode_png_reads_grayscale_2bit_assets() {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 4, 2);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Two);
            let mut writer = encoder.write_header().expect("header");
            // 4 samples per byte; pixel 0..3 packed as 0b10 10 10 10.
            writer
                .write_image_data(&[0b1010_1010, 0b1010_1010])
                .expect("data");
            writer.finish().expect("finish");
        }
        let image = decode_png(&out).expect("decode grayscale 2");
        assert_eq!(image.width(), 4);
        assert_eq!(image.height(), 2);
        // 2-bit sample `0b10` (= 2) scales to 8-bit `0b10101010` = 0xAA.
        assert_eq!(image.rgba(), expected_rgba(4, 2, [0xAA, 0xAA, 0xAA, 0xFF]));
    }

    #[test]
    fn decode_png_reads_grayscale_1bit_assets() {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 8, 2);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::One);
            let mut writer = encoder.write_header().expect("header");
            // 8 samples per byte; alternating 0/1 produces
            // 0b01010101.
            writer
                .write_image_data(&[0b01010101, 0b01010101])
                .expect("data");
            writer.finish().expect("finish");
        }
        let image = decode_png(&out).expect("decode grayscale 1");
        assert_eq!(image.width(), 8);
        assert_eq!(image.height(), 2);
        // Pixel 0 = 0b0 -> 0x00, pixel 1 = 0b1 -> 0xFF, ...
        let expected: Vec<u8> = (0..16)
            .map(|i| match i % 2 {
                0 => [0x00, 0x00, 0x00, 0xFF],
                _ => [0xFF, 0xFF, 0xFF, 0xFF],
            })
            .flat_map(|px| px.into_iter())
            .collect();
        assert_eq!(image.rgba(), expected);
    }

    #[test]
    fn decode_png_rejects_oversized_assets() {
        // Build a valid-looking PNG header then pad the rest with a
        // payload that crosses the cap.
        let normalized = normalize_image(&bitmap(2, 2, 0x11)).expect("normalize");
        let mut bytes = normalized.png().to_vec();
        bytes.resize(MAX_CLIPBOARD_ASSET_BYTES + 1, b'X');
        assert_eq!(
            decode_png(&bytes).unwrap_err(),
            AssetError::TooLarge {
                size: MAX_CLIPBOARD_ASSET_BYTES + 1
            }
        );
    }

    #[test]
    fn decode_png_rejects_a_corrupt_png_payload() {
        // Valid signature, truncated body — the same shape the
        // existing regression in `decode_rejects_non_png_and_corrupt_payloads`
        // covers, but the test now lives next to the new fixtures so
        // a future change that touches the corrupt-input path
        // updates both contracts together.
        let mut truncated = PNG_SIGNATURE.to_vec();
        truncated.extend_from_slice(b"\x00\x00\x00\rIHDR-broken");
        assert_eq!(decode_png(&truncated), Err(AssetError::NotPng));
    }

    #[test]
    fn store_serves_a_legacy_rgba_asset_after_a_restart() {
        // Mirror of `read_bytes_round_trips_a_persisted_asset` for
        // the diagnostics: the store's read path must hand back the
        // exact bytes the writer recorded even when the bytes came
        // from an older build that wrote a different PNG shape.
        let (_dir, store) = store();
        let png_bytes = build_png(
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            [0xAA, 0xBB, 0xCC, 0xFF],
        );
        let reference = format!("{}/{}.png", CLIPBOARD_ASSETS_DIR, "deadbeef".repeat(8));
        let target = store
            .root()
            .join("deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef.png");
        std::fs::create_dir_all(store.root()).expect("mkdir");
        std::fs::write(&target, &png_bytes).expect("write");
        let read = store.read_bytes(&reference).expect("read");
        assert_eq!(read, png_bytes);
    }

    #[test]
    fn store_serves_a_legacy_grayscale_asset_after_a_restart() {
        let (_dir, store) = store();
        let png_bytes = build_png(
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            [0x42, 0, 0, 0xFF],
        );
        let target = store
            .root()
            .join("c0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffee.png");
        std::fs::create_dir_all(store.root()).expect("mkdir");
        std::fs::write(&target, &png_bytes).expect("write");
        let read = store
            .read_bytes(
                "clipboard/c0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffee.png",
            )
            .expect("read grayscale");
        assert_eq!(read, png_bytes);
    }

    #[test]
    fn store_serves_a_legacy_palette_asset_after_a_restart() {
        let (_dir, store) = store();
        let mut palette_bytes = Vec::new();
        palette_bytes.extend_from_slice(&[0x12, 0x34, 0x56]);
        palette_bytes.extend_from_slice(&[0xAB, 0xCD, 0xEF]);
        let indices = [0u8, 1, 1, 0];
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 2, 2);
            encoder.set_color(png::ColorType::Indexed);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_palette(palette_bytes);
            let mut writer = encoder.write_header().expect("header");
            writer.write_image_data(&indices).expect("data");
            writer.finish().expect("finish");
        }
        let target = store
            .root()
            .join("cafecafecafecafecafecafecafecafecafecafecafecafecafecafecafecafe.png");
        std::fs::create_dir_all(store.root()).expect("mkdir");
        std::fs::write(&target, &out).expect("write");
        let read = store
            .read_bytes(
                "clipboard/cafecafecafecafecafecafecafecafecafecafecafecafecafecafecafecafe.png",
            )
            .expect("read palette");
        assert_eq!(read, out);
    }

    #[test]
    fn collector_keeps_a_legacy_grayscale_asset_after_a_delete() {
        // The garbage collector must not delete an asset that an
        // existing row references just because the on-disk format is
        // a grayscale PNG the current encoder never produces.
        let (_dir, store) = store();
        let png_bytes = build_png(
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            [0x11, 0, 0, 0xFF],
        );
        let file_name = "abcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcd.png";
        let reference = format!("{CLIPBOARD_ASSETS_DIR}/{file_name}");
        std::fs::create_dir_all(store.root()).expect("mkdir");
        std::fs::write(store.root().join(file_name), &png_bytes).expect("write");
        let mut referenced = BTreeSet::new();
        referenced.insert(reference.clone());
        let removed = store.collect_unreferenced(&referenced).expect("collect");
        assert_eq!(removed, 0, "a referenced legacy asset must survive");
        assert!(store.read_bytes(&reference).is_ok());
    }

    // -----------------------------------------------------------------
    // `diagnose` metadata-only diagnostic coverage.
    //
    // The diagnostic is the helper the user-facing surface calls when
    // it needs to know *why* an asset did not render: missing,
    // decodable, foreign namespace, oversized, … Each test pins a
    // single kind so the surface cannot drift back to the single
    // "invalid_asset_ref" string the previous implementation
    // surfaced for every failure mode.
    // -----------------------------------------------------------------

    #[test]
    fn diagnostic_returns_loaded_for_a_persisted_rgba_asset() {
        let (_dir, store) = store();
        let normalized = normalize_image(&bitmap(4, 4, 0x11)).expect("normalize");
        let outcome = store.store_image(&normalized).expect("write");
        let diagnostic = store.diagnose(outcome.asset_ref());
        assert_eq!(diagnostic.kind_str(), "loaded");
        assert_eq!(diagnostic.kind, AssetDiagnosticKind::Loaded);
    }

    #[test]
    fn diagnostic_returns_loaded_for_a_legacy_palette_asset() {
        let (_dir, store) = store();
        // Two-entry palette: index 0 = red, index 1 = blue.
        let mut palette_bytes = Vec::new();
        palette_bytes.extend_from_slice(&[0x10, 0x20, 0x30]);
        palette_bytes.extend_from_slice(&[0xAB, 0xCD, 0xEF]);
        let indices = [0u8, 1, 1, 0];
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 2, 2);
            encoder.set_color(png::ColorType::Indexed);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_palette(palette_bytes);
            let mut writer = encoder.write_header().expect("header");
            writer.write_image_data(&indices).expect("data");
            writer.finish().expect("finish");
        }
        let file_name = "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899.png";
        std::fs::create_dir_all(store.root()).expect("mkdir");
        std::fs::write(store.root().join(file_name), &out).expect("write");
        let diagnostic = store.diagnose(&format!("{CLIPBOARD_ASSETS_DIR}/{file_name}"));
        assert_eq!(diagnostic.kind_str(), "loaded");
    }

    #[test]
    fn diagnostic_returns_invalid_reference_for_a_foreign_reference() {
        let (_dir, store) = store();
        let cases = [
            "",
            "/etc/passwd",
            "../escape.png",
            "ignored-apps/x.png",
            "clipboard/nested/y.png",
        ];
        for reference in cases {
            let diagnostic = store.diagnose(reference);
            assert_eq!(
                diagnostic.kind_str(),
                "invalid_reference",
                "{reference} must classify as invalid_reference, got {diagnostic:?}"
            );
        }
    }

    #[test]
    fn diagnostic_returns_not_found_when_the_file_is_missing() {
        let (_dir, store) = store();
        std::fs::create_dir_all(store.root()).expect("mkdir");
        let diagnostic = store.diagnose("clipboard/ghost.png");
        assert_eq!(diagnostic.kind_str(), "not_found");
        assert_eq!(diagnostic.kind, AssetDiagnosticKind::NotFound);
    }

    #[test]
    fn diagnostic_returns_wrong_data_dir_when_the_namespace_is_absent() {
        // A data_dir that never saw a capture must surface
        // `wrong_data_dir` so the user can confirm the running
        // process is pointing at a different location than the
        // one the rows were persisted against.
        let (dir, _store) = store();
        // Point a fresh store at the directory but never write to
        // it; the namespace therefore does not exist.
        let isolated = ClipboardAssetStore::new(dir.path().join("empty"));
        let diagnostic = isolated.diagnose("clipboard/ghost.png");
        assert_eq!(diagnostic.kind_str(), "wrong_data_dir");
        assert_eq!(diagnostic.kind, AssetDiagnosticKind::WrongDataDir);
    }

    #[test]
    fn diagnostic_returns_wrong_namespace_for_a_symlink_inside_the_namespace() {
        let (dir, store) = store();
        let outside = dir.path().join("outside.png");
        std::fs::write(&outside, b"\x89PNG\r\n\x1a\n outside").expect("write");
        std::fs::create_dir_all(store.root()).expect("mkdir");
        std::os::unix::fs::symlink(&outside, store.root().join("alias.png")).expect("symlink");
        let diagnostic = store.diagnose("clipboard/alias.png");
        assert_eq!(diagnostic.kind_str(), "wrong_namespace");
        assert_eq!(diagnostic.kind, AssetDiagnosticKind::WrongNamespace);
    }

    #[test]
    fn diagnostic_returns_too_large_for_an_oversized_asset() {
        let (_dir, store) = store();
        let path = store.root().join("huge.png");
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.resize(MAX_CLIPBOARD_ASSET_BYTES + 1, b'X');
        std::fs::create_dir_all(store.root()).expect("mkdir");
        std::fs::write(&path, &bytes).expect("write");
        let diagnostic = store.diagnose("clipboard/huge.png");
        assert_eq!(diagnostic.kind_str(), "too_large");
        match diagnostic.kind {
            AssetDiagnosticKind::TooLarge { size } => {
                assert_eq!(size, MAX_CLIPBOARD_ASSET_BYTES + 1)
            }
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    #[test]
    fn diagnostic_returns_invalid_png_for_a_corrupt_payload() {
        let (_dir, store) = store();
        std::fs::create_dir_all(store.root()).expect("mkdir");
        std::fs::write(
            store.root().join("corrupt.png"),
            b"\x89PNG\r\n\x1a\n broken",
        )
        .expect("write");
        let diagnostic = store.diagnose("clipboard/corrupt.png");
        assert_eq!(diagnostic.kind_str(), "invalid_png");
        match diagnostic.kind {
            AssetDiagnosticKind::InvalidPng {
                color_type,
                bit_depth,
            } => {
                assert_eq!(color_type, "unknown");
                assert_eq!(bit_depth, "unknown");
            }
            other => panic!("expected InvalidPng, got {other:?}"),
        }
    }

    #[test]
    fn diagnostic_never_carries_absolute_paths_or_payload_bytes() {
        let (dir, store) = store();
        let data_dir = dir.path().display().to_string();
        let (_id, reference) = {
            let normalized = normalize_image(&bitmap(2, 2, 0x22)).expect("normalize");
            let outcome = store.store_image(&normalized).expect("write");
            (
                outcome.asset_ref().to_string(),
                outcome.asset_ref().to_string(),
            )
        };
        let diagnostic = store.diagnose(&reference);
        let rendered = format!("{diagnostic:?}");
        assert!(
            !rendered.contains(&data_dir),
            "absolute data_dir leaked into a diagnostic: {rendered}"
        );
        assert!(
            !rendered.contains("/Users/"),
            "absolute path leaked: {rendered}"
        );
        assert!(
            !rendered.contains(&reference),
            "asset reference leaked into a diagnostic: {rendered}"
        );
    }

    fn bytes_to_rgba_repeat(fill: u8, pixels: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(pixels * 4);
        for _ in 0..pixels {
            out.extend_from_slice(&[fill, fill, fill, fill]);
        }
        out
    }
}
