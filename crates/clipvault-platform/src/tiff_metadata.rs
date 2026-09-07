//! Minimal TIFF metadata extractor used by the macOS clipboard bridge.
//!
//! The bridge inspects the `public.tiff` leg the AppKit pasteboard
//! publishes alongside `public.png`. The two legs typically share
//! pixel data; the TIFF leg carries the canonical Apple storage of
//! the resolution, the color profile and any other ImageIO-derived
//! metadata a screenshot or screenshot preview publishes. When the
//! accompanying PNG leg lacks a `pHYs` chunk or an embedded
//! `iCCP` profile, the bridge pulls these from the TIFF leg so the
//! persisted asset keeps its native fidelity.
//!
//! The extractor is deliberately narrow: it walks the IFD entries
//! once, collects the four tags the bridge cares about
//! (XResolution / YResolution / ResolutionUnit / ICCProfile) and
//! returns the read-only summary. Any PNG vs TIFF semantic gap
//! (pixel vs colour representation) is left to the caller, which
//! is `read_png_main_thread` in the platform layer.
//!
//! The extractor never decodes the pixel raster. Decoding is the
//! PNG validator's responsibility (because that is the canonical
//! container the asset store persists) and the bridge keeps the
//! pixel and metadata paths separate so a TIFF leg without an
//! accompanying PNG leg can surface a typed "no pixels" outcome
//! instead of masquerading as a successful fidelity-preserving
//! capture.
//!
//! ## Privacy
//!
//! Every helper inspects the byte stream at the metadata boundary
//! only. The ICC profile payload is the only opaque byte buffer the
//! extractor forwards to its caller; the caller must keep that
//! payload inside the trusted bridge boundary, never surface it
//! through a log or `Debug` formatter, and store it only behind a
//! method whose contract forbids payload-level escaping. The
//! [`Debug`](std::fmt::Debug) implementation on
//! [`TiffMetadata`] follows the same metadata-only rule that the
//! rest of the platform crate uses: presence booleans, the
//! resolution numerator/denominator pair and the resolution unit.
//!
//! ## Why hand-rolled and not a third-party TIFF codec
//!
//! The platform crate's dependency graph is intentionally narrow:
//! the only image-related dependency is the `png` codec the bridge
//! already uses for `public.png`. A second codec would have to
//! handle hundreds of TIFF features we never need (BigTIFF, tiles,
//! LZW, JPEG, strip offsets, multi-page, ...). The four tags the
//! bridge consumes are stable, well-documented and small enough to
//! parse with checked arithmetic on the raw byte stream.

/// Four-character chunk-type identifier used by the extractor.
type TagId = [u8; 2];

const TAG_X_RESOLUTION: TagId = *b"\x01\x1a"; // 282
const TAG_Y_RESOLUTION: TagId = *b"\x01\x1b"; // 283
const TAG_RESOLUTION_UNIT: TagId = *b"\x01\x28"; // 296
const TAG_SUB_IFDS: TagId = *b"\x01\x4a"; // 330
const TAG_EXIF_IFD: TagId = *b"\x87\x69"; // 34665
const TAG_ICC_PROFILE: TagId = *b"\x87\x73"; // 34675
const TAG_INTEROPERABILITY_IFD: TagId = *b"\xa0\x05"; // 40965

const TIFF_TYPE_SHORT: u16 = 3;
const TIFF_TYPE_LONG: u16 = 4;
const TIFF_TYPE_RATIONAL: u16 = 5;
const TIFF_TYPE_BYTE: u16 = 1;
const TIFF_TYPE_UNDEFINED: u16 = 7;

const TIFF_MAGIC: u16 = 42;
const BIGTIFF_MAGIC: u16 = 43;

/// TIFF resolution-unit encoding. `None` is the documented
/// behaviour for unit value `1`; a value of `3` collapses to
/// [`TiffResolutionUnit::Centimeter`], a value of `2` to
/// [`TiffResolutionUnit::Inch`]. Any other value surfaces as
/// [`TiffResolutionUnit::None`] so the diagnostic never reports a
/// misleading DPI.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TiffResolutionUnit {
    /// `ResolutionUnit = 1` (no unit) or unparseable value.
    #[default]
    None,
    /// `ResolutionUnit = 2` (inch).
    Inch,
    /// `ResolutionUnit = 3` (centimetre).
    Centimeter,
}

impl TiffResolutionUnit {
    /// Stable snake_case identifier for the diagnostic surface.
    pub fn kind_str(self) -> &'static str {
        match self {
            TiffResolutionUnit::None => "none",
            TiffResolutionUnit::Inch => "inch",
            TiffResolutionUnit::Centimeter => "centimeter",
        }
    }

    fn from_u16(value: u16) -> Self {
        match value {
            2 => TiffResolutionUnit::Inch,
            3 => TiffResolutionUnit::Centimeter,
            _ => TiffResolutionUnit::None,
        }
    }
}

/// Summary of the four TIFF tags the macOS bridge consumes.
///
/// Every field is metadata-only: presence booleans, the resolution
/// numerator/denominator pair and the resolution unit. The only
/// byte payload the structure carries is the ICC profile bytes
/// themselves — those are intentionally opaque because the
/// persistence layer needs them to populate the PNG `iCCP` chunk.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TiffMetadata {
    /// XResolution numerator + denominator. The `dpi_x` helper
    /// applies the resolution unit to convert this to a DPI
    /// integer.
    pub x_resolution: Option<(u32, u32)>,
    /// YResolution numerator + denominator.
    pub y_resolution: Option<(u32, u32)>,
    pub resolution_unit: TiffResolutionUnit,
    /// ICC profile bytes, when the TIFF carries the Adobe-style
    /// `ICCProfile` tag (34675). The persistence layer forwards
    /// the bytes into a PNG `iCCP` chunk; the bridge otherwise
    /// forwards them nowhere.
    pub icc_profile: Option<Vec<u8>>,
}

impl TiffMetadata {
    /// True when the TIFF leg carries enough metadata to feed the
    /// bridge's resolution-and-profile path. The bridge uses the
    /// signal to short-circuit between the
    /// `NativePngPlusMetadata` and `NativePng` outcomes.
    pub fn has_resolution(&self) -> bool {
        self.x_resolution.is_some() || self.y_resolution.is_some()
    }

    /// Compute the XResolution DPI as the helper that converts the
    /// RATIONAL pair plus the resolution unit to an integer DPI.
    /// Returns `None` when the TIFF does not declare XResolution or
    /// when the resolution unit is `None`.
    pub fn dpi_x(&self) -> Option<u32> {
        let (numerator_value, denominator_value) = self.x_resolution?;
        rational_to_dpi(numerator_value, denominator_value, self.resolution_unit)
    }

    /// YResolution counterpart of [`Self::dpi_x`].
    pub fn dpi_y(&self) -> Option<u32> {
        let (numerator_value, denominator_value) = self.y_resolution?;
        rational_to_dpi(numerator_value, denominator_value, self.resolution_unit)
    }

    /// Return the exact pixels-per-metre value suitable for a PNG
    /// `pHYs` chunk.  The previous implementation converted the TIFF
    /// rational to a rounded integer DPI and then converted that
    /// integer back to pixels-per-metre.  That is lossy for non-integer
    /// resolutions and made the last-mile metadata path needlessly
    /// dependent on a rounded intermediate value.
    pub fn pixels_per_meter_x(&self) -> Option<u32> {
        let (numerator_value, denominator_value) = self.x_resolution?;
        rational_to_pixels_per_meter(numerator_value, denominator_value, self.resolution_unit)
    }

    /// Y-axis counterpart of [`Self::pixels_per_meter_x`].
    pub fn pixels_per_meter_y(&self) -> Option<u32> {
        let (numerator_value, denominator_value) = self.y_resolution?;
        rational_to_pixels_per_meter(numerator_value, denominator_value, self.resolution_unit)
    }

    /// Return the PNG `pHYs` value using the macOS pasteboard
    /// convention for a TIFF whose `ResolutionUnit` is omitted.
    ///
    /// A number of AppKit/ImageIO producers publish X/Y resolution
    /// but omit tag 296.  macOS still presents those values as pixels
    /// per inch (the same values visible in Preview's inspector).  A
    /// strict TIFF reader must treat unit `1` as unspecified, so the
    /// regular `pixels_per_meter_x/y` methods keep returning `None` in
    /// that case.  The native macOS bridge uses these explicit
    /// compatibility methods so it does not silently turn a real 144
    /// ppi capture into the PNG default of 72 ppi.
    pub fn macos_pixels_per_meter_x(&self) -> Option<u32> {
        let (numerator_value, denominator_value) = self.x_resolution?;
        rational_to_pixels_per_meter(
            numerator_value,
            denominator_value,
            match self.resolution_unit {
                TiffResolutionUnit::None => TiffResolutionUnit::Inch,
                unit => unit,
            },
        )
    }

    /// Y-axis counterpart of [`Self::macos_pixels_per_meter_x`].
    pub fn macos_pixels_per_meter_y(&self) -> Option<u32> {
        let (numerator_value, denominator_value) = self.y_resolution?;
        rational_to_pixels_per_meter(
            numerator_value,
            denominator_value,
            match self.resolution_unit {
                TiffResolutionUnit::None => TiffResolutionUnit::Inch,
                unit => unit,
            },
        )
    }
}

/// Rational-to-DPI conversion shared by both axes.
///
/// The TIFF spec stores resolutions as `RATIONAL` (numerator +
/// denominator) in `ResolutionUnit` units. To turn the pair into a
/// DPI integer the helper divides by the unit's inch value:
/// - `Inch` (ResolutionUnit = 2) → straight integer divide, rounded;
/// - `Centimeter` (ResolutionUnit = 3) → multiply by 2.54, rounded;
/// - `None` (ResolutionUnit = 1 or unknown) → `None`.
///
/// The function uses saturating arithmetic on `u64` because the
/// spec allows rationals up to `u32::MAX / u32::MAX`; the result
/// saturates to `u32::MAX` so a pathological TIFF cannot panic.
fn rational_to_dpi(
    numerator_value: u32,
    denominator_value: u32,
    unit: TiffResolutionUnit,
) -> Option<u32> {
    if denominator_value == 0 {
        return None;
    }
    let numerator = numerator_value as u64;
    let denominator = denominator_value as u64;
    let dpi = match unit {
        TiffResolutionUnit::Inch => round_div(numerator, denominator),
        TiffResolutionUnit::Centimeter => {
            let scaled = numerator.saturating_mul(254);
            let divisor = denominator.saturating_mul(100);
            round_div(scaled, divisor)
        }
        TiffResolutionUnit::None => return None,
    };
    Some(dpi.unwrap_or(u32::MAX as u64).min(u32::MAX as u64) as u32)
}

/// Convert a TIFF resolution rational directly to PNG pixels per
/// metre, without first rounding it to an integer DPI.
fn rational_to_pixels_per_meter(
    numerator_value: u32,
    denominator_value: u32,
    unit: TiffResolutionUnit,
) -> Option<u32> {
    if denominator_value == 0 {
        return None;
    }
    let numerator = numerator_value as u64;
    let denominator = denominator_value as u64;
    let ppm = match unit {
        // inches → metres: 100 / 2.54 = 5000 / 127 exactly.
        TiffResolutionUnit::Inch => {
            let scaled_numerator = numerator.saturating_mul(5000);
            let scaled_denominator = denominator.saturating_mul(127);
            round_div(scaled_numerator, scaled_denominator)
        }
        // centimetres → metres: multiply by exactly 100.
        TiffResolutionUnit::Centimeter => round_div(numerator.saturating_mul(100), denominator),
        TiffResolutionUnit::None => return None,
    }?;
    Some(ppm.min(u32::MAX as u64) as u32)
}

/// Rounded integer division on `u64`. Returns `None` when the
/// divisor is zero so the caller can choose to surface a typed
/// error.
fn round_div(numerator: u64, denominator: u64) -> Option<u64> {
    if denominator == 0 {
        return None;
    }
    let half = denominator / 2;
    numerator
        .checked_add(half)
        .and_then(|n| n.checked_div(denominator))
}

/// Parse the four tags the bridge cares about out of a TIFF byte
/// stream. Returns an empty [`TiffMetadata`] on any structural
/// failure: the caller treats the empty value as "no TIFF
/// resolution" rather than a hard failure.
pub fn parse_tiff_metadata(bytes: &[u8]) -> TiffMetadata {
    if bytes.len() < 8 {
        return TiffMetadata::default();
    }
    let big_endian = match bytes[0] {
        b'I' => false,
        b'M' => true,
        _ => return TiffMetadata::default(),
    };
    let read_u16 = |offset: usize| -> Option<u16> {
        if offset.checked_add(2)? > bytes.len() {
            return None;
        }
        let pair = [bytes[offset], bytes[offset + 1]];
        Some(if big_endian {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        })
    };
    let read_u32 = |offset: usize| -> Option<u32> {
        if offset.checked_add(4)? > bytes.len() {
            return None;
        }
        let quad = [
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ];
        Some(if big_endian {
            u32::from_be_bytes(quad)
        } else {
            u32::from_le_bytes(quad)
        })
    };
    let magic = match read_u16(2) {
        Some(value) => value,
        None => return TiffMetadata::default(),
    };
    if magic != TIFF_MAGIC && magic != BIGTIFF_MAGIC {
        return TiffMetadata::default();
    }
    // BigTIFF uses 8-byte offsets and a larger header; this is the
    // first place where a refusal-to-handle lives. The macOS
    // pasteboard only ever publishes classic TIFF (magic = 42),
    // so a BigTIFF refusal collapses to "no TIFF resolution"
    // rather than a hard failure.
    if magic == BIGTIFF_MAGIC {
        return TiffMetadata::default();
    }

    let ifd_offset = match read_u32(4) {
        Some(value) => value as usize,
        None => return TiffMetadata::default(),
    };
    let mut metadata = TiffMetadata::default();
    // A classic TIFF can chain multiple IFDs.  macOS normally puts
    // resolution in the first IFD, but walking the chain avoids
    // silently dropping it for pasteboard producers that place the
    // image metadata in a later directory.  The fixed bound and the
    // monotonic/visited checks keep malformed input bounded and avoid
    // loops without introducing a payload-sized allocation.
    // Resolution is normally in the first IFD, but ImageIO can place
    // it behind an ExifIFD/SubIFD pointer. Walk a bounded queue of
    // linked directories so the result does not depend on which
    // TIFF writer produced the pasteboard leg. The queue and visited
    // set are fixed-size: malformed metadata can never turn this
    // parser into an unbounded traversal or allocation.
    let mut ifd_queue = [0usize; 32];
    let mut queue_len = 1usize;
    let mut queue_index = 0usize;
    ifd_queue[0] = ifd_offset;
    let mut visited = [0usize; 32];
    let mut visited_len = 0usize;
    while queue_index < queue_len && visited_len < visited.len() {
        let current_ifd = ifd_queue[queue_index];
        queue_index += 1;
        if current_ifd >= bytes.len() || visited[..visited_len].contains(&current_ifd) {
            continue;
        }
        visited[visited_len] = current_ifd;
        visited_len += 1;

        let entry_count = match read_u16(current_ifd) {
            Some(value) => value as usize,
            None => break,
        };
        let entries_start = match current_ifd.checked_add(2) {
            Some(value) => value,
            None => break,
        };
        // 12 bytes per IFD entry, plus `entries_start` for the block.
        let entries_end = match entry_count
            .checked_mul(12)
            .and_then(|offset| entries_start.checked_add(offset))
        {
            Some(value) => value,
            None => break,
        };
        if !matches!(
            entries_end.checked_add(4),
            Some(value) if value <= bytes.len()
        ) {
            break;
        }

        for index in 0..entry_count {
            let entry_offset = match entries_start.checked_add(index * 12) {
                Some(value) => value,
                None => break,
            };
            let tag_id = [bytes[entry_offset], bytes[entry_offset + 1]];
            let type_id = match read_u16(entry_offset + 2) {
                Some(value) => value,
                None => continue,
            };
            let count = match read_u32(entry_offset + 4) {
                Some(value) => value as usize,
                None => continue,
            };
            let value_offset = entry_offset + 8;
            match tag_id {
                x if x == TAG_X_RESOLUTION => {
                    metadata.x_resolution =
                        read_rational(bytes, type_id, count, value_offset, big_endian);
                }
                x if x == TAG_Y_RESOLUTION => {
                    metadata.y_resolution =
                        read_rational(bytes, type_id, count, value_offset, big_endian);
                }
                x if x == TAG_RESOLUTION_UNIT => {
                    metadata.resolution_unit = TiffResolutionUnit::from_u16(read_u16_at(
                        bytes,
                        type_id,
                        value_offset,
                        big_endian,
                    ));
                }
                x if x == TAG_ICC_PROFILE => {
                    metadata.icc_profile =
                        read_long_payload(bytes, type_id, count, value_offset, big_endian);
                }
                x if x == TAG_SUB_IFDS || x == TAG_EXIF_IFD || x == TAG_INTEROPERABILITY_IFD => {
                    if let Some(offsets) =
                        read_ifd_offsets(bytes, type_id, count, value_offset, big_endian)
                    {
                        for offset in offsets {
                            if offset != 0 && queue_len < ifd_queue.len() {
                                ifd_queue[queue_len] = offset;
                                queue_len += 1;
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let next_ifd = match read_u32(entries_end) {
            Some(value) => value as usize,
            None => break,
        };
        if next_ifd == 0 {
            continue;
        }
        if queue_len < ifd_queue.len() {
            ifd_queue[queue_len] = next_ifd;
            queue_len += 1;
        }
    }

    metadata
}

fn read_u16_at(bytes: &[u8], type_id: u16, value_offset: usize, big_endian: bool) -> u16 {
    if type_id == TIFF_TYPE_SHORT && value_offset + 2 <= bytes.len() {
        let pair = [bytes[value_offset], bytes[value_offset + 1]];
        if big_endian {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        }
    } else if type_id == TIFF_TYPE_LONG && value_offset + 4 <= bytes.len() {
        let quad = [
            bytes[value_offset],
            bytes[value_offset + 1],
            bytes[value_offset + 2],
            bytes[value_offset + 3],
        ];
        let long = if big_endian {
            u32::from_be_bytes(quad)
        } else {
            u32::from_le_bytes(quad)
        };
        long as u16
    } else {
        0
    }
}

fn read_rational(
    bytes: &[u8],
    type_id: u16,
    count: usize,
    value_offset: usize,
    big_endian: bool,
) -> Option<(u32, u32)> {
    if type_id != TIFF_TYPE_RATIONAL || count == 0 {
        return None;
    }
    let offset_end = value_offset.checked_add(4)?;
    let offset_bytes: [u8; 4] = bytes.get(value_offset..offset_end)?.try_into().ok()?;
    let offset = if big_endian {
        u32::from_be_bytes(offset_bytes)
    } else {
        u32::from_le_bytes(offset_bytes)
    } as usize;
    if offset.checked_add(8)? > bytes.len() {
        return None;
    }
    let num_bytes: [u8; 4] = bytes[offset..offset + 4].try_into().ok()?;
    let den_bytes: [u8; 4] = bytes[offset + 4..offset + 8].try_into().ok()?;
    let num = if big_endian {
        u32::from_be_bytes(num_bytes)
    } else {
        u32::from_le_bytes(num_bytes)
    };
    let den = if big_endian {
        u32::from_be_bytes(den_bytes)
    } else {
        u32::from_le_bytes(den_bytes)
    };
    if den == 0 {
        return None;
    }
    Some((num, den))
}

/// Read offsets from an IFD pointer tag (`SubIFDs`, `ExifIFD` or
/// `InteroperabilityIFD`). TIFF permits the pointer values to be
/// encoded as either SHORT or LONG values and stores them inline
/// when the complete value fits in the four-byte IFD field.
fn read_ifd_offsets(
    bytes: &[u8],
    type_id: u16,
    count: usize,
    value_offset: usize,
    big_endian: bool,
) -> Option<Vec<usize>> {
    if count == 0 || count > 32 || (type_id != TIFF_TYPE_SHORT && type_id != TIFF_TYPE_LONG) {
        return None;
    }
    let item_size = if type_id == TIFF_TYPE_SHORT { 2 } else { 4 };
    let byte_len = count.checked_mul(item_size)?;
    let value_start = if byte_len <= 4 {
        value_offset
    } else {
        let offset_bytes: [u8; 4] = bytes.get(value_offset..value_offset + 4)?.try_into().ok()?;
        (if big_endian {
            u32::from_be_bytes(offset_bytes)
        } else {
            u32::from_le_bytes(offset_bytes)
        }) as usize
    };
    let value_end = value_start.checked_add(byte_len)?;
    if value_end > bytes.len() {
        return None;
    }
    let mut offsets = Vec::with_capacity(count);
    for index in 0..count {
        let start = value_start.checked_add(index.checked_mul(item_size)?)?;
        let offset = if item_size == 2 {
            let pair: [u8; 2] = bytes.get(start..start + 2)?.try_into().ok()?;
            if big_endian {
                u16::from_be_bytes(pair) as usize
            } else {
                u16::from_le_bytes(pair) as usize
            }
        } else {
            let quad: [u8; 4] = bytes.get(start..start + 4)?.try_into().ok()?;
            if big_endian {
                u32::from_be_bytes(quad) as usize
            } else {
                u32::from_le_bytes(quad) as usize
            }
        };
        offsets.push(offset);
    }
    Some(offsets)
}

fn read_long_payload(
    bytes: &[u8],
    type_id: u16,
    count: usize,
    value_offset: usize,
    big_endian: bool,
) -> Option<Vec<u8>> {
    if type_id != TIFF_TYPE_UNDEFINED && type_id != TIFF_TYPE_BYTE {
        return None;
    }
    // The value fits in the 4-byte value field when `count <= 4`;
    // otherwise the 4 bytes are an offset to the payload. We
    // accept either, in line with the TIFF spec.
    if count <= 4 {
        let end = value_offset.checked_add(count)?;
        if end > bytes.len() {
            return None;
        }
        return Some(bytes[value_offset..end].to_vec());
    }
    let offset_quad: [u8; 4] = [
        bytes[value_offset],
        bytes[value_offset + 1],
        bytes[value_offset + 2],
        bytes[value_offset + 3],
    ];
    let offset = if big_endian {
        u32::from_be_bytes(offset_quad)
    } else {
        u32::from_le_bytes(offset_quad)
    } as usize;
    let end = offset.checked_add(count)?;
    if end > bytes.len() {
        return None;
    }
    Some(bytes[offset..end].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a minimal TIFF carrying the four tags we want, with
    /// inline data when the value fits in the 4-byte field and an
    /// out-of-line pointer for the rationals (8 bytes) and the ICC
    /// profile (variable length).
    fn encode_minimal_tiff(
        x_numerator: u32,
        x_denominator: u32,
        y_numerator: u32,
        y_denominator: u32,
        resolution_unit: u16,
        icc_profile: Option<&[u8]>,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"II");
        out.extend_from_slice(&42u16.to_le_bytes());
        out.extend_from_slice(&8u32.to_le_bytes());

        // Decide how many entries will be present, build the
        // 12-byte entries inline, and accumulate trailing
        // payload bytes into one blob we append after the IFD.
        let num_entries: u16 = if icc_profile.is_some() { 4 } else { 3 };
        out.extend_from_slice(&num_entries.to_le_bytes());

        struct Pending {
            tag: [u8; 2],
            type_id: u16,
            count: u32,
            inline_value: [u8; 4],
            trailing: Vec<u8>,
        }
        let mut pending: Vec<Pending> = Vec::new();
        pending.push(Pending {
            tag: TAG_RESOLUTION_UNIT,
            type_id: TIFF_TYPE_SHORT,
            count: 1,
            inline_value: {
                let pair = resolution_unit.to_le_bytes();
                [pair[0], pair[1], 0, 0]
            },
            trailing: Vec::new(),
        });
        // X resolution rational → 8 bytes trailing.
        pending.push(Pending {
            tag: TAG_X_RESOLUTION,
            type_id: TIFF_TYPE_RATIONAL,
            count: 1,
            inline_value: [0; 4],
            trailing: {
                let mut v = Vec::new();
                v.extend_from_slice(&x_numerator.to_le_bytes());
                v.extend_from_slice(&x_denominator.to_le_bytes());
                v
            },
        });
        pending.push(Pending {
            tag: TAG_Y_RESOLUTION,
            type_id: TIFF_TYPE_RATIONAL,
            count: 1,
            inline_value: [0; 4],
            trailing: {
                let mut v = Vec::new();
                v.extend_from_slice(&y_numerator.to_le_bytes());
                v.extend_from_slice(&y_denominator.to_le_bytes());
                v
            },
        });
        if let Some(profile) = icc_profile {
            pending.push(Pending {
                tag: TAG_ICC_PROFILE,
                type_id: TIFF_TYPE_UNDEFINED,
                count: profile.len() as u32,
                inline_value: [0; 4],
                trailing: profile.to_vec(),
            });
        }
        // Sort entries by tag id (canonical IFD ordering).
        pending.sort_by(|a, b| u16::from_le_bytes(a.tag).cmp(&u16::from_le_bytes(b.tag)));

        // Reserve the entry block.
        let entries_offset = out.len();
        for _ in 0..pending.len() {
            out.extend_from_slice(&[0u8; 12]);
        }
        // No next IFD.
        out.extend_from_slice(&0u32.to_le_bytes());

        // Append the trailing payload.
        let mut trailing_offset = out.len();
        let mut trailing_offsets = Vec::new();
        for entry in &pending {
            if entry.trailing.is_empty() {
                trailing_offsets.push(0);
            } else {
                trailing_offsets.push(trailing_offset);
                out.extend_from_slice(&entry.trailing);
                trailing_offset = out.len();
            }
        }

        // Back-fill the value field for each entry: the inline
        // 4-byte value for short payloads, the trailing-data
        // offset for the rest.
        for (i, entry) in pending.iter().enumerate() {
            let entry_offset = entries_offset + i * 12;
            out[entry_offset..entry_offset + 2].copy_from_slice(&entry.tag);
            out[entry_offset + 2..entry_offset + 4].copy_from_slice(&entry.type_id.to_le_bytes());
            out[entry_offset + 4..entry_offset + 8].copy_from_slice(&entry.count.to_le_bytes());
            let value_bytes = if entry.trailing.is_empty() {
                entry.inline_value
            } else {
                (trailing_offsets[i] as u32).to_le_bytes()
            };
            out[entry_offset + 8..entry_offset + 12].copy_from_slice(&value_bytes);
        }
        out
    }

    /// Build a classic TIFF whose resolution lives in a secondary
    /// IFD reached through `SubIFDs` rather than in the first IFD.
    /// ImageIO is allowed to emit this shape, and it must remain
    /// equivalent to the ordinary first-IFD representation.
    fn encode_secondary_ifd_tiff() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"II");
        out.extend_from_slice(&42u16.to_le_bytes());
        out.extend_from_slice(&8u32.to_le_bytes());

        // First IFD at offset 8: one SubIFDs pointer and no next IFD.
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&TAG_SUB_IFDS);
        out.extend_from_slice(&TIFF_TYPE_LONG.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&26u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());

        // Secondary IFD at offset 26: X/Y resolution and inches.
        out.extend_from_slice(&3u16.to_le_bytes());
        out.extend_from_slice(&TAG_X_RESOLUTION);
        out.extend_from_slice(&TIFF_TYPE_RATIONAL.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&68u32.to_le_bytes());
        out.extend_from_slice(&TAG_Y_RESOLUTION);
        out.extend_from_slice(&TIFF_TYPE_RATIONAL.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&76u32.to_le_bytes());
        out.extend_from_slice(&TAG_RESOLUTION_UNIT);
        out.extend_from_slice(&TIFF_TYPE_SHORT.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&[0u8; 2]);
        out.extend_from_slice(&0u32.to_le_bytes());

        // X/Y RATIONAL payloads.
        out.extend_from_slice(&144u32.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&144u32.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out
    }

    #[test]
    fn parses_144_dpi_x_resolution_in_inches() {
        let bytes = encode_minimal_tiff(144, 1, 144, 1, 2, None);
        let metadata = parse_tiff_metadata(&bytes);
        assert_eq!(metadata.x_resolution, Some((144, 1)));
        assert_eq!(metadata.y_resolution, Some((144, 1)));
        assert_eq!(metadata.resolution_unit, TiffResolutionUnit::Inch);
        assert_eq!(metadata.dpi_x(), Some(144));
        assert_eq!(metadata.dpi_y(), Some(144));
    }

    #[test]
    fn parses_resolution_from_a_secondary_ifd() {
        let metadata = parse_tiff_metadata(&encode_secondary_ifd_tiff());
        assert_eq!(metadata.x_resolution, Some((144, 1)));
        assert_eq!(metadata.y_resolution, Some((144, 1)));
        assert_eq!(metadata.resolution_unit, TiffResolutionUnit::Inch);
        assert_eq!(metadata.dpi_x(), Some(144));
        assert_eq!(metadata.dpi_y(), Some(144));
    }

    #[test]
    fn parses_centimeter_resolution_into_dpi() {
        let bytes = encode_minimal_tiff(5669, 100, 5669, 100, 3, None);
        let metadata = parse_tiff_metadata(&bytes);
        assert_eq!(metadata.x_resolution, Some((5669, 100)));
        assert_eq!(metadata.resolution_unit, TiffResolutionUnit::Centimeter);
        assert!(metadata.dpi_x().unwrap_or(0) >= 143);
    }

    #[test]
    fn parses_inline_icc_profile_payload() {
        let profile = b"display-p3-fixture-icc-profile-bytes";
        let bytes = encode_minimal_tiff(144, 1, 144, 1, 2, Some(profile));
        let metadata = parse_tiff_metadata(&bytes);
        assert_eq!(metadata.icc_profile.as_deref(), Some(profile.as_slice()));
    }

    #[test]
    fn rejects_short_buffers() {
        let metadata = parse_tiff_metadata(b"II");
        assert_eq!(metadata, TiffMetadata::default());
    }

    #[test]
    fn rejects_invalid_magic_number() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II");
        bytes.extend_from_slice(&99u16.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes());
        let metadata = parse_tiff_metadata(&bytes);
        assert_eq!(metadata, TiffMetadata::default());
    }

    #[test]
    fn empty_dpi_when_unit_is_none() {
        let bytes = encode_minimal_tiff(72, 1, 72, 1, 1, None);
        let metadata = parse_tiff_metadata(&bytes);
        assert_eq!(metadata.x_resolution, Some((72, 1)));
        assert_eq!(metadata.resolution_unit, TiffResolutionUnit::None);
        assert!(metadata.dpi_x().is_none());
    }

    #[test]
    fn macos_compatibility_treats_unqualified_resolution_as_inches() {
        let bytes = encode_minimal_tiff(144, 1, 144, 1, 1, None);
        let metadata = parse_tiff_metadata(&bytes);

        // Generic TIFF semantics remain strict: ResolutionUnit=1
        // does not promise inches.
        assert!(metadata.dpi_x().is_none());

        // The macOS pasteboard/ImageIO compatibility path preserves
        // the ppi values Preview exposes even when tag 296 is absent
        // or set to the unspecified value.
        assert_eq!(metadata.macos_pixels_per_meter_x(), Some(5669));
        assert_eq!(metadata.macos_pixels_per_meter_y(), Some(5669));
    }
}
