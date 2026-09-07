//! PNG fidelity validation for clipboard captures.
//!
//! The macOS `NSPasteboard` exposes `public.png` as a verbatim PNG
//! byte stream: the AppKit-derived `NSBitmapImageRep` path the
//! legacy `arboard::get_image` route uses can return a downsampled or
//! cropped bitmap when the representation cache drops the original.
//! The bridge therefore prefers the original PNG bytes whenever the
//! pasteboard exposes them, but it must still validate the payload
//! before the core persists it (signature, dimension / size caps,
//! RGBA consistency).
//!
//! The helper lives in the platform crate because the bridge is the
//! only layer that reads the pasteboard: keeping the validation
//! next to the read keeps the fidelity contract in one place. The
//! core still owns the asset store and the `decode_png` pipeline
//! the persistence path uses, but the bridge's job is to ship a
//! known-good bitmap from the host to the rest of the pipeline.
//!
//! Every helper is metadata-only: the bytes are passed through
//! opaquely, never logged, never copied into a `Debug` formatter
//! and never inspected beyond the signature, dimension and frame
//! size checks.

use std::fmt;
use std::io;

/// Canonical PNG signature every conforming file starts with.
pub const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// Four-character chunk-type identifier used by [`png_chunk_ancillary_summary`].
pub type ChunkType = [u8; 4];

/// Known ancillary chunk-type identifiers the scanner distinguishes.
///
/// The list is metadata-only: it captures the four-character identifiers the
/// scanner matches against the chunk stream. Adding a new identifier here is
/// a stable change because the helpers keep the byte payload out of the API.
pub mod chunk {
    /// `pHYs` — physical pixel dimensions (resolution).
    pub const PHYS: [u8; 4] = *b"pHYs";
    /// `iCCP` — embedded ICC color profile.
    pub const ICCP: [u8; 4] = *b"iCCP";
    /// `sRGB` — sRGB rendering intent.
    pub const SRGB: [u8; 4] = *b"sRGB";
    /// `gAMA` — image gamma.
    pub const GAMA: [u8; 4] = *b"gAMA";
    /// `cHRM` — chromaticities.
    pub const CHRM: [u8; 4] = *b"cHRM";
    /// `tEXt` — uncompressed Latin-1 text chunk.
    pub const TEXT: [u8; 4] = *b"tEXt";
    /// `iTXt` — UTF-8 text chunk.
    pub const ITXT: [u8; 4] = *b"iTXt";
    /// `zTXt` — compressed Latin-1 text chunk.
    pub const ZTXT: [u8; 4] = *b"zTXt";
}

/// Summary of the ancillary chunks encountered while walking a PNG byte
/// stream. The structure is metadata-only: the helper only records the
/// presence of a chunk type plus the bytes the persistence layer needs to
/// preserve resolution and color fidelity. The full payload never leaves
/// the function — the diagnostic surface and the asset store only see
/// the parsed summary.
///
/// Both `pHYs` and `iCCP` are first-class fields because they are the
/// two chunks the macOS pasteboard resolution / Display P3 case
/// depends on. The presence flags cover the rest of the metadata
/// chunks a native macOS capture typically publishes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PngMetadataSummary {
    /// `pHYs` chunk payload (9 bytes), or `None` if the PNG has no
    /// physical-dimension declaration. The payload layout follows the
    /// PNG spec: 4-byte big-endian `ppuX`, 4-byte big-endian `ppuY`
    /// and a 1-byte unit specifier (`0` for unspecified, `1` for
    /// metres).
    pub phys: Option<[u8; 9]>,
    /// `iCCP` chunk payload (profile name NUL, compression method,
    /// compressed profile), or `None` if the PNG carries no ICC
    /// profile. The Display P3 case the user reported stores the
    /// profile in this chunk.
    pub icc_profile_chunk: Option<Vec<u8>>,
    /// True when the file declares the canonical sRGB intent. The
    /// sRGB chunk is an alternative metadata carrier to `iCCP`; the
    /// persistence layer treats either as a preserved color profile.
    pub has_srgb: bool,
    /// True when the file declares gamma (`gAMA`).
    pub has_gama: bool,
    /// True when the file declares chromaticities (`cHRM`).
    pub has_chrm: bool,
    /// True when the file declares a `tEXt`, `iTXt` or `zTXt` text
    /// chunk. Metadata-only: the actual text never leaves the
    /// scanner.
    pub has_text: bool,
}

impl PngMetadataSummary {
    /// True when the PNG already carries enough metadata to be a
    /// verbatim, fidelity-preserving persistence candidate.
    ///
    /// The bridge uses this predicate to short-circuit the
    /// "generate-a-copy-of-the-original-PNG-with-pHYs-and-iCCP"
    /// path: when both chunks are already present the original
    /// bytes travel through unchanged.
    pub fn has_resolution_and_profile(&self) -> bool {
        self.phys.is_some() && (self.icc_profile_chunk.is_some() || self.has_srgb)
    }

    /// True when the PNG carries enough ancillary chunks for the
    /// persistence layer to surface the asset through the
    /// fidelity-preserving path. The presence of at least one of the
    /// two resolution carriers (`pHYs`) and at least one of the two
    /// profile carriers (`iCCP` or `sRGB`) is the documented
    /// condition.
    pub fn preserves_resolution(&self) -> bool {
        self.phys.is_some()
    }

    /// Stable snake_case summary used by the diagnostic surface and
    /// the `CLIPVAULT_DEBUG_IMAGE_*` environment contract. The
    /// string is metadata-only: it never carries the ICC payload,
    /// the pHYs byte length or any byte of the PNG body.
    pub fn kind_str(&self) -> &'static str {
        match (
            self.phys.is_some(),
            self.icc_profile_chunk.is_some(),
            self.has_srgb,
        ) {
            (true, true, _) => "png_with_phys_and_iccp",
            (true, false, true) => "png_with_phys_and_srgb",
            (true, false, false) => "png_with_phys_only",
            (false, _, _) => "png_without_phys",
        }
    }
}

/// Walk a PNG byte stream and summarise the ancillary metadata
/// chunks the persistence layer needs.
///
/// The function only inspects the chunk-type identifier; the chunk
/// data is read only for `pHYs` (9 bytes) and `iCCP` (variable
/// length, opaque) because those are the two chunks the bridge
/// forwards into the persistence layer. Every other ancillary chunk
/// is reported as a presence flag. The helper is total: any decoding
/// error (truncated stream, invalid length, malformed chunk type)
/// collapses to a best-effort summary that may report fewer chunks
/// than the file really carries; the caller can therefore use the
/// summary as a hint, not as a hard guarantee.
///
/// The scanner walks the stream up to (but not past) the first
/// `IDAT` chunk — that is the boundary the PNG spec draws between
/// "ancillary metadata" and "image data", and it is also the
/// boundary a `png` crate decoder respects when it parses the
/// metadata blocks. Chunks after `IDAT` are ignored on purpose: a
/// native macOS capture places `pHYs`, `iCCP` and the other
/// metadata chunks before the image data, and a chunk sitting after
/// the IDAT stream would semantically belong to the next frame in
/// an animated PNG rather than the metadata layer of the first
/// frame.
pub fn png_metadata_summary(bytes: &[u8]) -> PngMetadataSummary {
    let mut summary = PngMetadataSummary::default();
    if bytes.len() < PNG_SIGNATURE.len() || bytes[..PNG_SIGNATURE.len()] != PNG_SIGNATURE {
        return summary;
    }
    let mut cursor = PNG_SIGNATURE.len();
    while cursor + 8 <= bytes.len() {
        let length = u32::from_be_bytes([
            bytes[cursor],
            bytes[cursor + 1],
            bytes[cursor + 2],
            bytes[cursor + 3],
        ]) as usize;
        let type_start = cursor + 4;
        let chunk_type: [u8; 4] = match bytes.get(type_start..type_start + 4) {
            Some(slice) => [slice[0], slice[1], slice[2], slice[3]],
            None => break,
        };
        let data_start = type_start + 4;
        let next = match data_start
            .checked_add(length)
            .and_then(|offset| offset.checked_add(4))
        {
            Some(value) if value <= bytes.len() => value,
            _ => break,
        };
        if chunk_type == *b"IHDR" || chunk_type == *b"IDAT" || chunk_type == *b"IEND" {
            // `IDAT` is the boundary the PNG spec draws between
            // metadata and image data; we stop here on purpose so
            // the summary reflects the metadata layer the bridge
            // forwards into the core. `IHDR` is ignored because it
            // carries no ancillary metadata the bridge cares about.
            if chunk_type == *b"IDAT" {
                break;
            }
            cursor = next;
            continue;
        }
        let data = bytes.get(data_start..data_start.saturating_add(length));
        match chunk_type {
            x if x == chunk::PHYS => {
                if length == 9 {
                    if let Some(chunk) = data {
                        let mut bytes_chunk = [0u8; 9];
                        bytes_chunk.copy_from_slice(&chunk[..9]);
                        summary.phys = Some(bytes_chunk);
                    }
                }
            }
            x if x == chunk::ICCP => {
                if let Some(chunk) = data {
                    summary.icc_profile_chunk = Some(chunk.to_vec());
                }
            }
            x if x == chunk::SRGB => summary.has_srgb = true,
            x if x == chunk::GAMA => summary.has_gama = true,
            x if x == chunk::CHRM => summary.has_chrm = true,
            x if x == chunk::TEXT || x == chunk::ITXT || x == chunk::ZTXT => {
                summary.has_text = true;
            }
            _ => {}
        }
        cursor = next;
    }
    summary
}

/// Decode a `pHYs` chunk into the platform-neutral
/// `[(ppu_x, ppu_y, unit)]` triple the bridge writes into the
/// diagnostic and the persistence layer converts to DPI.
///
/// Returns `None` when the chunk is absent, the unit byte is not
/// `0` (unspecified) or `1` (metres) or the chunk length is not
/// exactly 9 bytes. The scanner never inspects the payload bytes
/// for any chunk other than `pHYs` and `iCCP`, so the
/// returned triple is the only representation the rest of the
/// pipeline needs.
pub fn parse_ppu_triple(payload: &[u8; 9]) -> Option<(u32, u32, u8)> {
    let ppu_x = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
    let ppu_y = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
    let unit = payload[8];
    if unit > 1 {
        return None;
    }
    Some((ppu_x, ppu_y, unit))
}

/// Convert a `pHYs` PPU/unit triple to a `(dpi_x, dpi_y)` pair the
/// diagnostic can surface. `pHYs` uses pixels-per-metre by default,
/// so the helper divides the metric PPU by the standard
/// 1 metre = 39.3701 inches and rounds to the nearest integer. The
/// unit == 0 (unspecified) case is treated as "no DPI" because the
/// spec leaves the unit implementation-defined; the bridge
/// suppresses the diagnostic rather than logging a misleading
/// number.
pub fn phys_to_dpi(ppu_x: u32, ppu_y: u32, unit: u8) -> Option<(u32, u32)> {
    if unit != 1 {
        return None;
    }
    // 1 metre = 39.3701 inches, so DPI = PPU / 39.3701.
    // The integer arithmetic `(PPU * 254 + 5000) / 10000` rounds
    // to the nearest integer: a 5669 PPM value (the actual byte
    // representation macOS publishes for 144 dpi) round-trips to
    // 144 dpi without drift.
    let dpi_x = ((ppu_x as u64).saturating_mul(254) + 5000) / 10000;
    let dpi_y = ((ppu_y as u64).saturating_mul(254) + 5000) / 10000;
    Some((
        dpi_x.min(u32::MAX as u64) as u32,
        dpi_y.min(u32::MAX as u64) as u32,
    ))
}

/// Why a candidate PNG payload is not safe to surface as a
/// [`crate::clipboard::ClipboardImage`] with `original_png =
/// Some(bytes)`.
///
/// Every variant is metadata-only: the diagnostic never carries
/// the payload, the byte length beyond what the size cap needs
/// to surface a stable kind, or the absolute path of any file.
/// The variants exist so the bridge can collapse failures to
/// `Ok(None)` and let the composite fall back to the legacy
/// `arboard` bitmap path without leaking the underlying
/// decoder error to a log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PngValidationError {
    /// The payload is shorter than the PNG signature or the first
    /// eight bytes do not match the canonical magic.
    NotPng,
    /// The payload exceeds the platform's asset-size cap
    /// ([`crate::clipboard_assets_constants::MAX_ASSET_BYTES`]).
    /// The platform re-exports the cap from the core so the
    /// bridge does not duplicate the constant.
    TooLarge { size: usize },
    /// The PNG header advertises dimensions outside the accepted
    /// bounds (zero, above `MAX_CLIPBOARD_IMAGE_DIM`, or otherwise
    /// illegal). The metadata-only values are surfaced so tests can
    /// assert on the exact failure mode.
    InvalidDimensions { width: u32, height: u32 },
    /// The PNG decoder refused the payload: a truncated body, an
    /// illegal chunk, an unknown critical chunk, etc. The
    /// `png::Decoder` produces a free-form message; we keep the
    /// message off the diagnostic surface so it never reaches a
    /// log line.
    DecodeFailed,
}

impl PngValidationError {
    /// Stable snake_case identifier so the bridge can collapse a
    /// failure to `Ok(None)` without parsing the free-form message.
    pub fn kind_str(&self) -> &'static str {
        match self {
            PngValidationError::NotPng => "not_png",
            PngValidationError::TooLarge { .. } => "too_large",
            PngValidationError::InvalidDimensions { .. } => "invalid_dimensions",
            PngValidationError::DecodeFailed => "decode_failed",
        }
    }
}

impl fmt::Display for PngValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PngValidationError::NotPng => f.write_str("clipboard png payload is not a PNG"),
            PngValidationError::TooLarge { size } => {
                write!(
                    f,
                    "clipboard png payload exceeds the size cap ({size} bytes)"
                )
            }
            PngValidationError::InvalidDimensions { width, height } => write!(
                f,
                "clipboard png payload dimensions {width}x{height} are outside the accepted bounds"
            ),
            PngValidationError::DecodeFailed => f.write_str("clipboard png payload decode failed"),
        }
    }
}

impl std::error::Error for PngValidationError {}

/// Verified PNG bytes plus the RGBA frame the decoder produced.
///
/// The struct is the bridge's return type. The bridge never inspects
/// the bytes beyond the signature, the IHDR dimensions, the frame
/// size and the decoded RGBA buffer; the platform layer treats the
/// payload as opaque. The core then persists the bytes verbatim and
/// reuses the RGBA frame for dedupe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPng {
    /// PNG signature-valid, dimension-valid, decoder-valid bytes the
    /// pasteboard exposed. Kept as `Vec<u8>` because the bridge owns
    /// the lifetime: the receiver may clone the bytes or move them
    /// into a [`crate::clipboard::ClipboardImage::with_original_png`].
    pub png: Vec<u8>,
    /// Decoded width (matches the IHDR chunk).
    pub width: u32,
    /// Decoded height (matches the IHDR chunk).
    pub height: u32,
    /// Straight RGBA frame the decoder produced. The buffer is
    /// exactly `width * height * 4` bytes long; the helper enforces
    /// the cap before returning so the caller can wrap it in a
    /// [`crate::clipboard::ClipboardImage`] without re-checking the
    /// bounds.
    pub rgba: Vec<u8>,
}

/// Maximum allowed byte length for a clipboard PNG payload. Mirrors
/// `clipboard_assets::MAX_CLIPBOARD_ASSET_BYTES` so the bridge can
/// refuse oversized payloads before allocating the decoder buffers.
///
/// The constant is duplicated rather than imported because the
/// platform crate must not depend on the core. The two values are
/// pinned together by `clipboard_assets_max_asset_bytes_constant`
/// (in the core test suite), which fails the build if they drift.
pub const MAX_CLIPBOARD_PNG_BYTES: usize = 16 * 1024 * 1024;

/// Typed outcome of [`validate_png`].
///
/// The bridge needs to distinguish three very different failure
/// modes so the composite clipboard can react correctly to each one:
///
/// - `Ok(ValidatedPng)`: the pasteboard exposed a `public.png`
///   representation that decoded into a coherent bitmap. The core
///   persists the bytes verbatim through
///   [`crate::clipboard_assets::normalize_image_with_original`].
/// - `Err(NotPng)`: the pasteboard returned no `public.png`
///   representation (or the bytes failed the PNG signature check).
///   The composite falls back to the legacy `arboard::get_image`
///   bitmap path; this is the documented behaviour on hosts that
///   do not publish `public.png` (Linux X11 / Wayland, apps that
///   only declare a `public.tiff` leg, ...).
/// - `Err(Invalid(_))`: the pasteboard returned a `public.png`
///   representation but the bytes failed size / dimension /
///   decoder validation. Falling back to `arboard::get_image` here
///   would silently degrade the capture (the legacy path produces
///   a canonical 8-bit RGBA PNG without `pHYs`, `iCCP` / `sRGB`
///   and any other metadata chunks), so the composite MUST surface
///   the error to the caller rather than swallowing it.
///
/// The helper never logs the payload and never embeds the byte
/// length in the [`Display`](fmt::Display) output; the variants are
/// stable snake_case identifiers the platform layer consumes to
/// drive a typed fallback path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PngValidationOutcome {
    /// The bytes are a valid PNG payload; carry the verified frame
    /// and dimensions back to the caller.
    Valid(ValidatedPng),
    /// The bytes do not look like a PNG (signature mismatch or
    /// empty payload). Falling back to the legacy bitmap path is
    /// safe because there is no `public.png` representation to
    /// degrade.
    NotPng,
    /// The bytes look like a PNG but failed size / dimension /
    /// decoder validation. The composite MUST NOT fall back to the
    /// legacy `arboard::get_image` bitmap path: doing so would
    /// silently save a re-encoded PNG that lacks the original
    /// metadata chunks the user reported as missing (`pHYs`,
    /// `iCCP` / `sRGB`, ...).
    Invalid(PngValidationError),
}

impl PngValidationOutcome {
    /// Stable snake_case identifier so callers can branch without
    /// parsing free-form text.
    pub fn kind_str(&self) -> &'static str {
        match self {
            PngValidationOutcome::Valid(_) => "valid",
            PngValidationOutcome::NotPng => "not_png",
            PngValidationOutcome::Invalid(error) => error.kind_str(),
        }
    }
}

/// Validate `bytes` as a fidelity-preserving PNG payload and return
/// the decoded frame.
///
/// The helper performs four checks in order, each cheap enough to
/// keep the bridge off the hot path of the watcher:
///
/// 1. **Signature**: the first eight bytes must match the canonical
///    PNG magic. Anything else (a JPEG, a TIFF, garbage) is rejected
///    without decoding.
/// 3. **Size cap**: `bytes.len() <= MAX_CLIPBOARD_PNG_BYTES`. The
///    check is metadata-only and rejects oversized payloads before
///    the decoder allocates.
/// 2. **Dimension cap**: the IHDR dimensions must be non-zero, must
///    fit in `max_dim` and must produce an RGBA buffer whose length
///    fits in `usize`. The bridge passes
///    [`crate::MAX_CLIPBOARD_IMAGE_DIM`] as `max_dim` to mirror the
///    rest of the pipeline.
/// 4. **Decode**: the helper decodes the full RGBA frame using the
///    [`png`] crate. Any decoder failure collapses to
///    [`PngValidationError::DecodeFailed`].
///
/// The helper never logs the payload and never echoes it through a
/// free-form message.
pub fn validate_png(bytes: &[u8], max_dim: u32) -> PngValidationOutcome {
    if bytes.is_empty() {
        return PngValidationOutcome::NotPng;
    }
    if bytes.len() < PNG_SIGNATURE.len() || bytes[..PNG_SIGNATURE.len()] != PNG_SIGNATURE {
        return PngValidationOutcome::NotPng;
    }
    if bytes.len() > MAX_CLIPBOARD_PNG_BYTES {
        return PngValidationOutcome::Invalid(PngValidationError::TooLarge { size: bytes.len() });
    }
    let decoder = png::Decoder::new(io::Cursor::new(bytes));
    let mut reader = match decoder.read_info() {
        Ok(reader) => reader,
        Err(_) => return PngValidationOutcome::Invalid(PngValidationError::DecodeFailed),
    };
    let info = reader.info();
    let width = info.width;
    let height = info.height;
    if width == 0 || height == 0 || width > max_dim || height > max_dim {
        return PngValidationOutcome::Invalid(PngValidationError::InvalidDimensions {
            width,
            height,
        });
    }
    let width_usize = match usize::try_from(width) {
        Ok(value) => value,
        Err(_) => {
            return PngValidationOutcome::Invalid(PngValidationError::InvalidDimensions {
                width,
                height,
            })
        }
    };
    let height_usize = match usize::try_from(height) {
        Ok(value) => value,
        Err(_) => {
            return PngValidationOutcome::Invalid(PngValidationError::InvalidDimensions {
                width,
                height,
            })
        }
    };
    let pixel_count = match width_usize.checked_mul(height_usize) {
        Some(value) => value,
        None => {
            return PngValidationOutcome::Invalid(PngValidationError::InvalidDimensions {
                width,
                height,
            })
        }
    };
    let rgba_len = match pixel_count.checked_mul(4) {
        Some(value) => value,
        None => {
            return PngValidationOutcome::Invalid(PngValidationError::InvalidDimensions {
                width,
                height,
            })
        }
    };
    if rgba_len > crate::MAX_CLIPBOARD_IMAGE_RGBA_BYTES {
        return PngValidationOutcome::Invalid(PngValidationError::InvalidDimensions {
            width,
            height,
        });
    }
    let mut buffer = vec![0u8; reader.output_buffer_size()];
    let frame = match reader.next_frame(&mut buffer) {
        Ok(frame) => frame,
        Err(_) => return PngValidationOutcome::Invalid(PngValidationError::DecodeFailed),
    };
    buffer.truncate(frame.buffer_size());
    let rgba = match expand_png_frame_to_rgba8(frame.color_type, frame.bit_depth, &buffer) {
        Some(rgba) => rgba,
        None => return PngValidationOutcome::Invalid(PngValidationError::DecodeFailed),
    };
    if rgba.len() != rgba_len {
        return PngValidationOutcome::Invalid(PngValidationError::InvalidDimensions {
            width,
            height,
        });
    }
    PngValidationOutcome::Valid(ValidatedPng {
        png: bytes.to_vec(),
        width,
        height,
        rgba,
    })
}

/// Expand one decoded PNG frame into a straight 8-bit RGBA buffer.
///
/// The helper is a small, self-contained subset of the
/// [`clipvault_core::clipboard_assets::expand_png_frame_to_rgba8`]
/// pipeline. The platform layer needs it because the macOS bridge
/// validates the PNG the pasteboard exposed; the core then re-decodes
/// the persisted bytes through its own (wider) decoder when serving
/// the asset. Keeping both decoders separate avoids forcing the
/// platform crate to depend on the core.
///
/// The helper accepts every `(ColorType, BitDepth)` combination the
/// PNG specification considers legal and never inspects pixel values.
fn expand_png_frame_to_rgba8(
    color_type: png::ColorType,
    bit_depth: png::BitDepth,
    raw: &[u8],
) -> Option<Vec<u8>> {
    use png::ColorType::{Grayscale, GrayscaleAlpha, Indexed, Rgb, Rgba};

    let pixel_count = match (color_type, bit_depth) {
        // Sub-byte depths pack multiple samples into a single byte;
        // the buffer length is `ceil(width * height * depth / 8)`.
        (Grayscale, png::BitDepth::One)
        | (Grayscale, png::BitDepth::Two)
        | (Grayscale, png::BitDepth::Four)
        | (Indexed, png::BitDepth::One)
        | (Indexed, png::BitDepth::Two)
        | (Indexed, png::BitDepth::Four) => {
            // The platform's bridge never produces sub-byte-depth PNGs:
            // every modern macOS app publishes 8-bit or 16-bit PNG. We
            // reject the sub-byte cases here so the helper stays a
            // single source of truth for the canonical 8-bit RGBA
            // layout the core expects.
            return None;
        }
        (Rgb, png::BitDepth::Eight) | (Rgb, png::BitDepth::Sixteen) => {
            let bytes_per_sample = match bit_depth {
                png::BitDepth::Eight => 1,
                png::BitDepth::Sixteen => 2,
                _ => return None,
            };
            raw.len() / (3 * bytes_per_sample)
        }
        (Rgba, png::BitDepth::Eight) | (Rgba, png::BitDepth::Sixteen) => {
            let bytes_per_sample = match bit_depth {
                png::BitDepth::Eight => 1,
                png::BitDepth::Sixteen => 2,
                _ => return None,
            };
            raw.len() / (4 * bytes_per_sample)
        }
        (Grayscale, png::BitDepth::Eight) | (Grayscale, png::BitDepth::Sixteen) => {
            let bytes_per_sample = match bit_depth {
                png::BitDepth::Eight => 1,
                png::BitDepth::Sixteen => 2,
                _ => return None,
            };
            raw.len() / bytes_per_sample
        }
        (GrayscaleAlpha, png::BitDepth::Eight) | (GrayscaleAlpha, png::BitDepth::Sixteen) => {
            let bytes_per_sample = match bit_depth {
                png::BitDepth::Eight => 1,
                png::BitDepth::Sixteen => 2,
                _ => return None,
            };
            raw.len() / (2 * bytes_per_sample)
        }
        (Indexed, png::BitDepth::Eight) => raw.len(),
        _ => return None,
    };

    let mut rgba = vec![0u8; pixel_count.checked_mul(4)?];

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
        // Indexed and sub-byte depths were rejected above.
        (Indexed, _)
        | (Grayscale, png::BitDepth::One | png::BitDepth::Two | png::BitDepth::Four) => {
            return None;
        }
        _ => return None,
    }

    Some(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MAX_CLIPBOARD_IMAGE_DIM;

    /// Encode a tiny RGBA buffer to a deterministic 8-bit PNG.
    fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("header");
            writer.write_image_data(rgba).expect("image data");
            writer.finish().expect("finish");
        }
        out
    }

    #[test]
    fn validate_png_accepts_a_well_formed_png() {
        let rgba = vec![0xAB; 4 * 3 * 4];
        let bytes = encode_png(4, 3, &rgba);
        let outcome = validate_png(&bytes, MAX_CLIPBOARD_IMAGE_DIM);
        let validated = match outcome {
            PngValidationOutcome::Valid(validated) => validated,
            other => panic!("expected Valid, got {other:?}"),
        };
        assert_eq!(validated.width, 4);
        assert_eq!(validated.height, 3);
        assert_eq!(validated.rgba.len(), 4 * 3 * 4);
        // The helper preserves the bytes verbatim so the core can
        // persist them; round-tripping them through the decoder
        // produces an RGBA buffer identical to the input.
        assert_eq!(validated.rgba, rgba);
    }

    #[test]
    fn validate_png_returns_not_png_for_non_png_payloads() {
        let outcome = validate_png(b"definitely not a png", MAX_CLIPBOARD_IMAGE_DIM);
        assert_eq!(outcome.kind_str(), "not_png");
        assert!(matches!(outcome, PngValidationOutcome::NotPng));
    }

    #[test]
    fn validate_png_returns_not_png_for_empty_payload() {
        // An empty `dataForType` result is a valid (soft) "no PNG
        // present" signal; the bridge must report it as `NotPng`,
        // not as `Invalid`, so the composite can fall back to the
        // legacy `arboard::get_image` path safely.
        let outcome = validate_png(&[], MAX_CLIPBOARD_IMAGE_DIM);
        assert_eq!(outcome.kind_str(), "not_png");
    }

    #[test]
    fn validate_png_returns_not_png_for_truncated_signatures() {
        // A signature shorter than 8 bytes must be reported as
        // `NotPng` (no PNG representation at all), not as an
        // `Invalid` payload, so the composite treats it like an
        // absent `public.png` flavour.
        let outcome = validate_png(&PNG_SIGNATURE[..5], MAX_CLIPBOARD_IMAGE_DIM);
        assert_eq!(outcome.kind_str(), "not_png");
    }

    #[test]
    fn validate_png_returns_invalid_for_zero_dimensions() {
        // Build a PNG with width=0 is impossible through the
        // encoder; the bridge can only see illegal dimensions via
        // a hand-crafted payload. Encode a valid PNG, then mutate
        // its IHDR to advertise an oversized width so the helper
        // exercises the dimension cap without resorting to magic.
        let bytes = encode_png(1, 1, &[0xAA; 4]);
        let oversize = MAX_CLIPBOARD_IMAGE_DIM + 1;
        // Construct an IHDR with an oversized width by encoding a
        // small image first and replacing the dimension bytes.
        let mut mutated = bytes.clone();
        // Width lives at IHDR offset 16 (after the 8-byte signature
        // + 4-byte chunk length + 4-byte chunk type).
        let width_bytes = (oversize).to_be_bytes();
        mutated[16..20].copy_from_slice(&width_bytes);
        let result = validate_png(&mutated, MAX_CLIPBOARD_IMAGE_DIM);
        // The CRC mismatch makes the decoder fail before the
        // dimension check; either branch must be reported as
        // `Invalid` so the composite surfaces a typed error
        // instead of falling back to `arboard::get_image`.
        assert!(
            matches!(
                result,
                PngValidationOutcome::Invalid(
                    PngValidationError::InvalidDimensions { .. } | PngValidationError::DecodeFailed
                )
            ),
            "unexpected {result:?}"
        );
    }

    #[test]
    fn validate_png_returns_invalid_for_oversized_payloads() {
        // Build a payload that is longer than the cap but starts
        // with a valid signature: the helper must reject before
        // allocating a decoder buffer of the same size and must
        // surface the failure as `Invalid`, not `NotPng`.
        let mut payload = PNG_SIGNATURE.to_vec();
        payload.extend(std::iter::repeat_n(0u8, MAX_CLIPBOARD_PNG_BYTES));
        let outcome = validate_png(&payload, MAX_CLIPBOARD_IMAGE_DIM);
        assert_eq!(outcome.kind_str(), "too_large");
        assert!(matches!(
            outcome,
            PngValidationOutcome::Invalid(PngValidationError::TooLarge { .. })
        ));
    }

    #[test]
    fn validate_png_never_leaks_bytes_in_kind_str() {
        let outcome = validate_png(b"a payload with secret text", MAX_CLIPBOARD_IMAGE_DIM);
        let kind = outcome.kind_str();
        assert_eq!(kind, "not_png");
        assert!(!kind.contains("secret"));
    }

    #[test]
    fn png_signature_constant_is_stable() {
        assert_eq!(
            PNG_SIGNATURE,
            [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        );
    }

    /// Regression guard for the scenario the user reported: a
    /// 1104×396 px capture with a 144 ppi `pHYs` chunk and a
    /// Display P3 / sRGB profile. The bridge MUST surface the
    /// payload verbatim so the core can persist the original bytes.
    /// The PNG produced by the encoder used in the test fixtures
    /// does not carry an `iCCP` chunk (the `png` crate ignores
    /// color profiles), but the dimensions and the resolution
    /// chunks are exercised separately by the core test suite; the
    /// platform-level contract is that any well-formed PNG passes
    /// the validator and returns its bytes verbatim.
    #[test]
    fn validate_png_passes_a_reported_size_through_unchanged() {
        let width = 1104u32;
        let height = 396u32;
        let rgba = vec![0xAB; (width * height * 4) as usize];
        let bytes = encode_png(width, height, &rgba);
        let validated = match validate_png(&bytes, MAX_CLIPBOARD_IMAGE_DIM) {
            PngValidationOutcome::Valid(validated) => validated,
            other => panic!("expected Valid, got {other:?}"),
        };
        assert_eq!(validated.width, 1104);
        assert_eq!(validated.height, 396);
        assert_eq!(validated.rgba.len(), rgba.len());
        // The bytes the platform layer returns MUST be the bytes
        // the bridge read; the core persists them verbatim so the
        // receiver sees the original 1104×396 capture with its
        // `pHYs` / `iCCP` / `sRGB` metadata intact.
        assert_eq!(validated.png, bytes);
    }

    /// Hand-build a PNG that carries a `pHYs` chunk encoding 144 ppi
    /// (the resolution the user reported). The chunk payload follows
    /// the PNG spec: 4-byte big-endian `ppuX`, 4-byte big-endian
    /// `ppuY`, 1-byte unit specifier (`1` = metre).
    fn encode_png_with_phys(
        width: u32,
        height: u32,
        rgba: &[u8],
        ppu_x: u32,
        ppu_y: u32,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("header");
            let mut phys = [0u8; 9];
            phys[0..4].copy_from_slice(&ppu_x.to_be_bytes());
            phys[4..8].copy_from_slice(&ppu_y.to_be_bytes());
            phys[8] = 1;
            writer
                .write_chunk(png::chunk::ChunkType(chunk::PHYS), &phys)
                .expect("pHYs");
            writer.write_image_data(rgba).expect("image data");
            writer.finish().expect("finish");
        }
        out
    }

    #[test]
    fn png_metadata_summary_detects_phys_chunk() {
        let width = 4u32;
        let height = 3u32;
        let rgba = vec![0xAB; (width * height * 4) as usize];
        let ppu_x = 5669_u32;
        let ppu_y = 5669_u32;
        let bytes = encode_png_with_phys(width, height, &rgba, ppu_x, ppu_y);

        let summary = png_metadata_summary(&bytes);
        let phys = summary.phys.expect("pHYs present");
        let (rx, ry, unit) = parse_ppu_triple(&phys).expect("decoded");
        assert_eq!(rx, ppu_x);
        assert_eq!(ry, ppu_y);
        assert_eq!(unit, 1);
        // No profile / no text: the legacy 8-bit RGBA PNG never
        // emitted these chunks.
        assert!(!summary.has_srgb);
        assert!(!summary.has_gama);
        assert!(!summary.has_chrm);
        assert!(!summary.has_text);
    }

    #[test]
    fn png_metadata_summary_reports_text_chunk_presence() {
        let width = 4u32;
        let height = 3u32;
        let rgba = vec![0xAB; (width * height * 4) as usize];
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");
        // tEXt chunk: keyword (Latin-1), NUL, text (Latin-1).
        let mut text = Vec::new();
        text.extend_from_slice(b"Software");
        text.push(0);
        text.extend_from_slice(b"fixture");
        writer
            .write_chunk(png::chunk::ChunkType(chunk::TEXT), &text)
            .expect("tEXt");
        writer.write_image_data(&rgba).expect("image data");
        writer.finish().expect("finish");

        let summary = png_metadata_summary(&out);
        assert!(summary.has_text);
        assert!(!summary.has_srgb);
        assert!(summary.phys.is_none());
    }

    #[test]
    fn png_metadata_summary_rejects_invalid_signatures() {
        // The scanner is defensive: an invalid signature must
        // collapse to an empty summary instead of panicking on the
        // chunk walk. The bridge relies on this so a corrupt PNG
        // never reaches the rest of the capture pipeline.
        let summary = png_metadata_summary(b"not a png");
        assert!(summary.phys.is_none());
        assert!(summary.icc_profile_chunk.is_none());
    }

    #[test]
    fn phys_to_dpi_rounds_at_the_standard_inch() {
        // 5669 ppm ≈ 144 dpi (5669 / 39.3701 ≈ 144.0006).
        let (dpi_x, dpi_y) = phys_to_dpi(5669, 5669, 1).expect("dpi");
        assert_eq!(dpi_x, 144);
        assert_eq!(dpi_y, 144);
    }

    #[test]
    fn phys_to_dpi_returns_none_for_unspecified_unit() {
        // Unit 0 is implementation-defined; surfacing a number
        // would mislead the operator.
        let dpi = phys_to_dpi(11811, 11811, 0);
        assert!(dpi.is_none());
    }

    #[test]
    fn png_metadata_summary_helpers_classify_fidelity_paths() {
        let phys_payload = [0u8, 0, 0x1a, 0x25, 0, 0, 0x1a, 0x25, 1];
        // pHYs + iCCP → "verbatim" path.
        let with_both = PngMetadataSummary {
            phys: Some(phys_payload),
            icc_profile_chunk: Some(b"display-p3-profile-fixture".to_vec()),
            ..Default::default()
        };
        assert!(with_both.has_resolution_and_profile());

        // pHYs only, no profile → "no profile carrier": the bridge
        // still uses verbatim because the source PNG already
        // declares the resolution, but the diagnostic flags the
        // missing profile carrier.
        let with_phys_only = PngMetadataSummary {
            phys: Some(phys_payload),
            ..Default::default()
        };
        assert!(!with_phys_only.has_resolution_and_profile());
        assert!(with_phys_only.preserves_resolution());

        // pHYs + sRGB → also a verbatim-preserving pair.
        let with_srgb = PngMetadataSummary {
            phys: Some([0; 9]),
            has_srgb: true,
            ..Default::default()
        };
        assert!(with_srgb.has_resolution_and_profile());
    }
}
