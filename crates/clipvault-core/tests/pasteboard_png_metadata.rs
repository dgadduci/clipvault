//! Integration tests for the `native_png_with_tiff_metadata`
//! fidelity path the macOS bridge surfaces when `public.png` lacks
//! `pHYs` and `iCCP` chunks but `public.tiff` carries the canonical
//! 144 ppi / Display P3 metadata.
//!
//! Regression guard for the issue the user reported on macOS: the
//! image they screenshot reports 144 ppi and a Display P3 profile,
//! yet the asset ClipVault persisted reports 72 ppi and no colour
//! profile. The root cause (documented in `proposal.md`) is that
//! `public.png` does not always carry the resolution / colour
//! profile chunks; the metadata lives on the sibling `public.tiff`
//! leg.
//!
//! Each scenario below builds a synthetic fixture that mirrors the
//! exact byte layout the bridge is documented to surface, then
//! feeds it through `normalize_image_with_original`. The
//! assertions inspect the resulting PNG byte buffer to confirm the
//! metadata the bridge resolved actually reached disk.
//!
//! No real macOS screenshot is read in any of these tests: the
//! fixtures are deterministic and constructed in a `tempfile`
//! directory. The real-world validation lives in the manual test
//! harness the design document mandates.

use std::sync::Arc;

use clipvault_core::{
    AppBootstrap, AppContext, Capabilities, ClipboardAssetStore, ClipboardBackend, ClipboardImage,
    ClipboardPayload, DisplayServer, FakeActiveApplication, FakeClipboardBackend,
    FakeHotkeyManager, FakePasteController, FakeSettingsNavigator, FakeTrayController,
    HistoryOutcome, OsFamily, PlatformAdapters, PlatformInfo, MAX_CLIPBOARD_IMAGE_DIM,
};
use clipvault_core::{
    ColorProfileKind, ImageCaptureDiagnostic, ImageSource, PasteboardImageMetadata,
    RepresentationSource,
};
use clipvault_db::EntryRepository;
use clipvault_platform::{
    parse_tiff_metadata, png_metadata_summary, PngMetadataSummary, TiffMetadata,
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use time::macros::datetime;

// ---------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------

#[derive(Debug, Clone)]
struct FixedClock {
    instant: time::OffsetDateTime,
}

impl clipvault_core::Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        self.instant
    }
}

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

fn png_signature(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && bytes[..8] == PNG_SIGNATURE
}

/// Build a deterministic PNG (RGBA 8-bit) the bridge can hand to
/// the core. The pixel pattern is coordinate-derived so a partial
/// or downscaled copy surfaces at the first sampled pixel.
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

/// Build a deterministic PNG with an explicit `pHYs` payload. This
/// models the macOS case where `public.png` contains a default 72
/// ppi value while the sibling TIFF carries the actual 144 ppi
/// resolution.
fn encode_png_with_phys(
    width: u32,
    height: u32,
    rgba: &[u8],
    pixels_per_meter_x: u32,
    pixels_per_meter_y: u32,
) -> Vec<u8> {
    use png::chunk::ChunkType;
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");
        let mut phys = [0u8; 9];
        phys[0..4].copy_from_slice(&pixels_per_meter_x.to_be_bytes());
        phys[4..8].copy_from_slice(&pixels_per_meter_y.to_be_bytes());
        phys[8] = 1;
        writer
            .write_chunk(ChunkType(*b"pHYs"), &phys)
            .expect("pHYs");
        writer.write_image_data(rgba).expect("image data");
        writer.finish().expect("finish");
    }
    out
}

/// Build a deterministic PNG that carries the canonical
/// `pHYs` (144 ppi = 5669 ppm) and `iCCP` (Display P3
/// fixture profile) metadata chunks. The marker text inside
/// the `iCCP` profile name is fixture-only.
#[allow(non_snake_case)]
fn encode_png_with_pHYs_and_iCCP(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    use png::chunk::ChunkType;
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");

        // `pHYs` chunk: 144 ppi = 5669 ppm.
        let mut phys = [0u8; 9];
        phys[0..4].copy_from_slice(&5669u32.to_be_bytes());
        phys[4..8].copy_from_slice(&5669u32.to_be_bytes());
        phys[8] = 1;
        writer
            .write_chunk(ChunkType(*b"pHYs"), &phys)
            .expect("pHYs");

        // `iCCP` chunk: profile name, NUL, compression method
        // (0 = deflate), compressed profile. The fixture
        // profile is a synthetic ASCII string so the test
        // never relies on a real Display P3 payload.
        let profile_name = "Display P3";
        let profile_payload = b"clipvault-fixture-display-p3-icc";
        let compressed = {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            use std::io::Write as _;
            encoder.write_all(profile_payload).expect("deflate");
            encoder.finish().expect("deflate finish")
        };
        let mut iccp = Vec::with_capacity(profile_name.len() + 1 + 1 + compressed.len());
        iccp.extend_from_slice(profile_name.as_bytes());
        iccp.push(0);
        iccp.push(0);
        iccp.extend_from_slice(&compressed);
        writer
            .write_chunk(ChunkType(*b"iCCP"), &iccp)
            .expect("iCCP");

        writer.write_image_data(rgba).expect("image data");
        writer.finish().expect("finish");
    }
    out
}

/// Build a minimal TIFF carrying the four tags the bridge
/// consumes: XResolution, YResolution, ResolutionUnit, ICCProfile.
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
        tag: *b"\x01\x28",
        type_id: 3,
        count: 1,
        inline_value: {
            let pair = resolution_unit.to_le_bytes();
            [pair[0], pair[1], 0, 0]
        },
        trailing: Vec::new(),
    });
    pending.push(Pending {
        tag: *b"\x01\x1a",
        type_id: 5,
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
        tag: *b"\x01\x1b",
        type_id: 5,
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
            // TIFF tag 34675 (0x8773) is the ICC profile tag.
            tag: *b"\x87\x73",
            type_id: 7,
            count: profile.len() as u32,
            inline_value: [0; 4],
            trailing: profile.to_vec(),
        });
    }
    pending.sort_by(|a, b| u16::from_le_bytes(a.tag).cmp(&u16::from_le_bytes(b.tag)));

    let entries_offset = out.len();
    for _ in 0..pending.len() {
        out.extend_from_slice(&[0u8; 12]);
    }
    out.extend_from_slice(&0u32.to_le_bytes());

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

/// Build the pasteboard metadata the bridge produces for a
/// copy that has only PNG pixels + a sibling TIFF metadata
/// leg. The summary mirrors the documented macOS pasteboard
/// shape the user reported.
fn build_pasteboard_metadata(
    png_bytes: &[u8],
    tiff_metadata: Option<TiffMetadata>,
) -> PasteboardImageMetadata {
    let png_summary = png_metadata_summary(png_bytes);
    let resolution_detected = png_summary.preserves_resolution()
        || tiff_metadata.as_ref().is_some_and(|m| m.has_resolution());
    let profile_detected = png_summary.icc_profile_chunk.is_some()
        || png_summary.has_srgb
        || tiff_metadata
            .as_ref()
            .is_some_and(|m| m.icc_profile.is_some());
    PasteboardImageMetadata {
        png_chunks: png_summary,
        tiff: tiff_metadata,
        inferred_display_dpi: None,
        resolution_detected,
        profile_detected,
    }
}

/// Scan the PNG bytes for an ancillary chunk type the canonical
/// encoder never emits. The helper is metadata-only: it inspects
/// chunk-type identifiers only, never pixel values, never chunk
/// data.
fn png_contains_chunk(bytes: &[u8], chunk_type: &[u8; 4]) -> bool {
    if !png_signature(bytes) {
        return false;
    }
    let mut cursor = 8usize;
    while cursor + 8 <= bytes.len() {
        let length = u32::from_be_bytes([
            bytes[cursor],
            bytes[cursor + 1],
            bytes[cursor + 2],
            bytes[cursor + 3],
        ]) as usize;
        let type_start = cursor + 4;
        if type_start + 4 > bytes.len() {
            return false;
        }
        if &bytes[type_start..type_start + 4] == chunk_type {
            return true;
        }
        let next = type_start + 4 + length + 4;
        if next <= cursor {
            return false;
        }
        cursor = next;
    }
    false
}

/// Decode a PNG into the canonical RGBA frame the platform layer
/// hands to the core.
fn decode_png_rgba(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("png decode info");
    let info = reader.info();
    let (width, height) = (info.width, info.height);
    let mut buffer = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buffer).expect("png decode frame");
    buffer.truncate(frame.buffer_size());
    assert_eq!(frame.color_type, png::ColorType::Rgba);
    assert_eq!(frame.bit_depth, png::BitDepth::Eight);
    (width, height, buffer)
}

/// Distinct pixel pattern so partial copies are visible.
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

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{:02x}", byte));
    }
    out
}

// ---------------------------------------------------------------------
// `normalize_image_with_original` direct unit-level tests
// ---------------------------------------------------------------------

/// Build a `ClipboardImage` with the pasteboard metadata the
/// bridge would attach to a TIFF-metadata-bearing capture.
fn image_with_metadata(
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    png_bytes: Vec<u8>,
    metadata: PasteboardImageMetadata,
) -> ClipboardImage {
    ClipboardImage::with_pasteboard_metadata(rgba, width, height, png_bytes, metadata)
        .expect("image with metadata is valid")
}

// ---------------------------------------------------------------------
// Rebuilt PNG: TIFF metadata only, PNG pixels only
// ---------------------------------------------------------------------

/// `public.png` carries pixels + `pHYs` + `iCCP`. The TIFF leg is
/// also present but carries the same metadata verbatim. The
/// persistence layer MUST keep the original bytes verbatim
/// because the source PNG already contains the resolution /
/// profile chunks — the rebuild is a no-op.
#[test]
fn png_with_native_metadata_persists_verbatim() {
    let width = 4u32;
    let height = 3u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png_with_pHYs_and_iCCP(width, height, &rgba);
    let summary = png_metadata_summary(&png_bytes);
    assert!(summary.phys.is_some());
    assert!(summary.icc_profile_chunk.is_some());

    let image = image_with_metadata(
        rgba.clone(),
        width,
        height,
        png_bytes.clone(),
        build_pasteboard_metadata(&png_bytes, None),
    );
    let normalized =
        clipvault_core::normalize_image_with_original(&image).expect("normalize with original");
    assert_eq!(
        normalized.png(),
        png_bytes.as_slice(),
        "the source PNG already carries pHYs + iCCP; the rebuild must be a no-op"
    );
    assert!(!normalized.was_rebuilt_with_metadata());
    assert!(normalized.has_original_png_bytes());
}

/// `public.png` carries pixels only (no `pHYs`, no `iCCP`). The
/// TIFF leg declares 144 ppi and a Display P3 ICC profile. The
/// persistence layer MUST rebuild the PNG, inject a `pHYs`
/// chunk and an `iCCP` chunk, and persist the same pixels.
/// The output bytes are NOT byte-identical to the source
/// because the rebuild adds the metadata chunks the source
/// lacked. This is the documented behaviour for the failure
/// mode the user reported.
#[test]
fn png_without_metadata_rebuilds_with_tiff_metadata() {
    let width = 4u32;
    let height = 3u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png(width, height, &rgba);
    // Sanity: the fixture lacks pHYs and iCCP.
    let summary = png_metadata_summary(&png_bytes);
    assert!(summary.phys.is_none());
    assert!(summary.icc_profile_chunk.is_none());

    let fixture_icc = b"clipvault-fixture-display-p3-icc";
    let tiff_bytes = encode_minimal_tiff(5669, 100, 5669, 100, 3, Some(fixture_icc));
    let tiff_metadata = parse_tiff_metadata(&tiff_bytes);
    assert!(tiff_metadata.has_resolution());
    assert_eq!(
        tiff_metadata.icc_profile.as_deref(),
        Some(fixture_icc.as_slice())
    );

    let image = image_with_metadata(
        rgba.clone(),
        width,
        height,
        png_bytes.clone(),
        build_pasteboard_metadata(&png_bytes, Some(tiff_metadata.clone())),
    );

    let normalized =
        clipvault_core::normalize_image_with_original(&image).expect("normalize with original");

    // The persisted PNG carries `pHYs` and `iCCP` chunks.
    let rebuilt_summary = png_metadata_summary(normalized.png());
    assert!(
        rebuilt_summary.phys.is_some(),
        "the rebuilt PNG must carry a `pHYs` chunk"
    );
    assert!(
        rebuilt_summary.icc_profile_chunk.is_some(),
        "the rebuilt PNG must carry an `iCCP` chunk"
    );

    // The `pHYs` payload is at 144 ppi (5669 ppm).
    let phys = rebuilt_summary.phys.expect("pHYs");
    assert_eq!(phys[0..4], 5669u32.to_be_bytes());
    assert_eq!(phys[4..8], 5669u32.to_be_bytes());
    assert_eq!(phys[8], 1);

    // The pixels survive byte-for-byte.
    let (decoded_w, decoded_h, decoded_rgba) = decode_png_rgba(normalized.png());
    assert_eq!(decoded_w, width);
    assert_eq!(decoded_h, height);
    assert_eq!(decoded_rgba, rgba);

    // The normalized source is rebuilt-with-metadata.
    assert!(normalized.was_rebuilt_with_metadata());
    assert!(normalized.has_original_png_bytes());
}

/// Profile-only fallback: `public.png` carries pixels + `pHYs`
/// but lacks any colour-profile chunk. The TIFF leg declares
/// an ICCProfile IFD entry but no resolution. The persistence
/// layer adds an `iCCP` chunk for the profile but leaves the
/// PNG's existing `pHYs` chunk alone.
#[test]
fn png_with_phys_but_no_profile_keeps_phys_and_injects_icc() {
    let width = 4u32;
    let height = 3u32;
    let rgba = distinct_pixels(width, height);

    // Source PNG carries `pHYs` only (no `iCCP`).
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");
        let mut phys = [0u8; 9];
        phys[0..4].copy_from_slice(&5669u32.to_be_bytes());
        phys[4..8].copy_from_slice(&5669u32.to_be_bytes());
        phys[8] = 1;
        writer
            .write_chunk(png::chunk::ChunkType(*b"pHYs"), &phys)
            .expect("pHYs");
        writer.write_image_data(&rgba).expect("image data");
        writer.finish().expect("finish");
    }
    let source_summary = png_metadata_summary(&png_bytes);
    assert!(source_summary.phys.is_some());
    assert!(source_summary.icc_profile_chunk.is_none());

    // TIFF carries an ICCProfile IFD entry but no resolution.
    let fixture_icc = b"clipvault-fixture-display-p3-iccp";
    let tiff_bytes = encode_minimal_tiff(72, 1, 72, 1, 1, Some(fixture_icc));
    let tiff_metadata = parse_tiff_metadata(&tiff_bytes);
    // XResolution exists but unit=1 collapses to None DPI: a
    // `dpi_x` call returns `None`, so the persistence layer
    // cannot compute a `pHYs` chunk from this metadata. The
    // helper still sees `has_resolution()` returning `true`
    // because the tag is present — but the resolution is not
    // usable so the rebuild path picks up the PNG's existing
    // `pHYs` chunk verbatim.
    assert!(tiff_metadata.has_resolution());
    assert!(tiff_metadata.dpi_x().is_none());
    assert_eq!(
        tiff_metadata.icc_profile.as_deref(),
        Some(fixture_icc.as_slice())
    );

    let image = image_with_metadata(
        rgba.clone(),
        width,
        height,
        png_bytes.clone(),
        build_pasteboard_metadata(&png_bytes, Some(tiff_metadata)),
    );

    let normalized = clipvault_core::normalize_image_with_original(&image).expect("normalize");

    let rebuilt_summary = png_metadata_summary(normalized.png());
    assert!(rebuilt_summary.phys.is_some(), "pHYs survives the rebuild");
    assert!(
        rebuilt_summary.icc_profile_chunk.is_some(),
        "the rebuild adds an iCCP chunk because the TIFF leg declared one"
    );

    let (decoded_w, decoded_h, decoded_rgba) = decode_png_rgba(normalized.png());
    assert_eq!(decoded_w, width);
    assert_eq!(decoded_h, height);
    assert_eq!(decoded_rgba, rgba);
}

/// Regression for the actual macOS pasteboard shape: the PNG leg
/// already contains a default 72 ppi `pHYs`, while the TIFF leg
/// carries the 144 ppi resolution shown by Preview. The TIFF value
/// is authoritative for this paired pasteboard snapshot; accepting
/// the PNG value first would make every newly captured asset report
/// 72 ppi even though the source image reports 144 ppi.
#[test]
fn tiff_resolution_overrides_default_png_phys() {
    let width = 4u32;
    let height = 3u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png_with_phys(width, height, &rgba, 2835, 2835);
    let png_phys = png_metadata_summary(&png_bytes)
        .phys
        .expect("PNG fixture must carry pHYs");
    assert_eq!(&png_phys[0..4], &2835u32.to_be_bytes());

    let tiff_metadata = parse_tiff_metadata(&encode_minimal_tiff(144, 1, 144, 1, 2, None));
    let metadata = build_pasteboard_metadata(&png_bytes, Some(tiff_metadata));
    let image = image_with_metadata(rgba.clone(), width, height, png_bytes, metadata);
    let normalized =
        clipvault_core::normalize_image_with_original(&image).expect("normalize with TIFF ppi");

    let output_phys = png_metadata_summary(normalized.png())
        .phys
        .expect("rebuilt PNG must carry the TIFF pHYs");
    assert_eq!(&output_phys[0..4], &5669u32.to_be_bytes());
    assert_eq!(&output_phys[4..8], &5669u32.to_be_bytes());
    assert!(normalized.was_rebuilt_with_metadata());
    let (decoded_w, decoded_h, decoded_rgba) = decode_png_rgba(normalized.png());
    assert_eq!((decoded_w, decoded_h), (width, height));
    assert_eq!(decoded_rgba, rgba);
}

/// No metadata case: `public.png` carries pixels only and the
/// TIFF leg either does not exist or declares no useful
/// metadata. The persistence layer keeps the source PNG
/// verbatim because there is no metadata to inject.
#[test]
fn png_without_sibling_metadata_persists_verbatim() {
    let width = 4u32;
    let height = 3u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png(width, height, &rgba);
    let image = image_with_metadata(
        rgba,
        width,
        height,
        png_bytes.clone(),
        build_pasteboard_metadata(&png_bytes, None),
    );

    let normalized = clipvault_core::normalize_image_with_original(&image).expect("normalize");
    assert_eq!(normalized.png(), png_bytes.as_slice());
    assert!(!normalized.was_rebuilt_with_metadata());
    assert!(normalized.has_original_png_bytes());
}

/// When macOS exposes a native PNG but omits both the TIFF leg and
/// the PNG `pHYs` chunk, the platform bridge may provide the host's
/// bounded display-scale fallback. The core must use it rather than
/// silently persisting a 72 ppi PNG, while preserving every pixel.
#[test]
fn inferred_display_resolution_rebuilds_png_without_changing_pixels() {
    let width = 4u32;
    let height = 3u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png(width, height, &rgba);
    let mut metadata = build_pasteboard_metadata(&png_bytes, None);
    metadata.inferred_display_dpi = Some((144, 144));
    metadata.resolution_detected = true;
    let image = image_with_metadata(rgba.clone(), width, height, png_bytes, metadata);
    let normalized =
        clipvault_core::normalize_image_with_original(&image).expect("normalize fallback");

    let output_phys = png_metadata_summary(normalized.png())
        .phys
        .expect("inferred pHYs");
    assert_eq!(&output_phys[0..4], &5669u32.to_be_bytes());
    assert_eq!(&output_phys[4..8], &5669u32.to_be_bytes());
    let (decoded_w, decoded_h, decoded_rgba) = decode_png_rgba(normalized.png());
    assert_eq!((decoded_w, decoded_h), (width, height));
    assert_eq!(decoded_rgba, rgba);
}

#[test]
fn inferred_display_resolution_replaces_generic_png_72_ppi() {
    let width = 4u32;
    let height = 3u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png_with_phys(width, height, &rgba, 2835, 2835);
    let mut metadata = build_pasteboard_metadata(&png_bytes, None);
    metadata.inferred_display_dpi = Some((144, 144));
    metadata.resolution_detected = true;
    let image = image_with_metadata(rgba.clone(), width, height, png_bytes, metadata);
    let normalized =
        clipvault_core::normalize_image_with_original(&image).expect("normalize fallback");
    let output_phys = png_metadata_summary(normalized.png())
        .phys
        .expect("inferred pHYs");
    assert_eq!(&output_phys[0..4], &5669u32.to_be_bytes());
    assert_eq!(&output_phys[4..8], &5669u32.to_be_bytes());
    let (decoded_w, decoded_h, decoded_rgba) = decode_png_rgba(normalized.png());
    assert_eq!((decoded_w, decoded_h), (width, height));
    assert_eq!(decoded_rgba, rgba);
}

// ---------------------------------------------------------------------
// `rebuild_png_with_metadata` lower-level direct call
// ---------------------------------------------------------------------

/// Direct exercise of the rebuild helper. The PNG the helper
/// produces MUST carry the supplied `pHYs` payload and `iCCP`
/// payload verbatim, with the same pixels as the input.
#[allow(non_snake_case)]
#[test]
fn rebuild_helper_emits_pHYs_and_iCCP() {
    let width = 3u32;
    let height = 2u32;
    let rgba = distinct_pixels(width, height);
    let mut phys = [0u8; 9];
    phys[0..4].copy_from_slice(&5669u32.to_be_bytes());
    phys[4..8].copy_from_slice(&5669u32.to_be_bytes());
    phys[8] = 1;
    let fixture_icc = b"clipvault-fixture-srgb";
    let compressed = clipvault_core::deflate_icc_profile(fixture_icc).expect("deflate");
    let icc = clipvault_core::IccChunk {
        profile_name: "srgb".to_string(),
        compression_method: 0,
        compressed_profile: compressed,
    };
    let rebuilt = clipvault_core::rebuild_png_with_metadata(
        width,
        height,
        &rgba,
        Some(phys),
        Some(icc),
        None,
    )
    .expect("rebuild");
    assert!(png_signature(&rebuilt));
    assert!(png_contains_chunk(&rebuilt, b"pHYs"));
    assert!(png_contains_chunk(&rebuilt, b"iCCP"));
    let (decoded_w, decoded_h, decoded_rgba) = decode_png_rgba(&rebuilt);
    assert_eq!(decoded_w, width);
    assert_eq!(decoded_h, height);
    assert_eq!(decoded_rgba, rgba);
}

/// Rebuild without metadata produces a 8-bit RGBA PNG that
/// still satisfies the decoder and reads with the right
/// dimensions and pixels. Mirrors the legacy encoder path.
#[test]
fn rebuild_helper_without_metadata_emits_canonical_png() {
    let width = 5u32;
    let height = 4u32;
    let rgba = distinct_pixels(width, height);
    let rebuilt = clipvault_core::rebuild_png_with_metadata(width, height, &rgba, None, None, None)
        .expect("rebuild canonical");
    assert!(png_signature(&rebuilt));
    assert!(!png_contains_chunk(&rebuilt, b"pHYs"));
    assert!(!png_contains_chunk(&rebuilt, b"iCCP"));
    let (decoded_w, decoded_h, decoded_rgba) = decode_png_rgba(&rebuilt);
    assert_eq!(decoded_w, width);
    assert_eq!(decoded_h, height);
    assert_eq!(decoded_rgba, rgba);
}

// ---------------------------------------------------------------------
// Capture / persistence end-to-end through `record_clipboard_payload`
// ---------------------------------------------------------------------

struct Harness {
    _dir: TempDir,
    context: AppContext,
    clipboard: Arc<FakeClipboardBackend>,
    store: ClipboardAssetStore,
}

impl Harness {
    fn record(&self, id: i64) -> clipvault_db::EntryRecord {
        let mut db = self.context.database().lock();
        let repo = EntryRepository::new(db.connection_mut());
        repo.find_by_id(id).expect("query").expect("row present")
    }

    fn assets_on_disk(&self) -> Vec<String> {
        let root = self.store.root();
        let Ok(entries) = std::fs::read_dir(root) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }

    fn asset_bytes(&self, name: &str) -> Vec<u8> {
        let path = self.store.root().join(name);
        std::fs::read(&path).expect("asset bytes")
    }
}

fn harness() -> Harness {
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");

    let clipboard = Arc::new(FakeClipboardBackend::new());
    clipboard.set_image_support(true, true);

    let adapters = PlatformAdapters::new(
        Arc::clone(&clipboard) as Arc<dyn ClipboardBackend>,
        Arc::new(FakeHotkeyManager::new()),
        Arc::new(FakeActiveApplication::new()),
        Arc::new(FakePasteController::new()),
        Arc::new(FakeTrayController::new()),
        Arc::new(FakeSettingsNavigator::new()),
        Arc::new(clipvault_core::NoopApplicationMetadataProvider)
            as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
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

    Harness {
        _dir: dir,
        context,
        clipboard,
        store: ClipboardAssetStore::new(data_dir),
    }
}

/// End-to-end: PNG without `pHYs` + sibling TIFF with 144 ppi
/// + ICC profile → asset on disk preserves resolution and
/// profile.
#[test]
#[allow(clippy::doc_lazy_continuation)]
fn capture_persists_rebuilt_png_with_tiff_metadata_end_to_end() {
    let h = harness();
    let width = 4u32;
    let height = 3u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png(width, height, &rgba);
    let fixture_icc = b"clipvault-fixture-display-p3-icc";
    let tiff_bytes = encode_minimal_tiff(5669, 100, 5669, 100, 3, Some(fixture_icc));
    let tiff_metadata = parse_tiff_metadata(&tiff_bytes);
    let metadata = build_pasteboard_metadata(&png_bytes, Some(tiff_metadata));
    let image = ClipboardImage::with_pasteboard_metadata(
        rgba.clone(),
        width,
        height,
        png_bytes.clone(),
        metadata,
    )
    .expect("image with metadata");

    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(image),
        None,
    );
    let id = match outcome {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };
    let record = h.record(id);
    assert_eq!(record.content_type, clipvault_db::ContentType::Image);
    let names = h.assets_on_disk();
    assert_eq!(names.len(), 1);
    let on_disk = h.asset_bytes(&names[0]);
    assert!(png_signature(&on_disk));
    assert!(png_contains_chunk(&on_disk, b"pHYs"));
    assert!(png_contains_chunk(&on_disk, b"iCCP"));

    // Pixel-perfect roundtrip.
    let (decoded_w, decoded_h, decoded_rgba) = decode_png_rgba(&on_disk);
    assert_eq!(decoded_w, width);
    assert_eq!(decoded_h, height);
    assert_eq!(decoded_rgba, rgba);
}

/// Preview + paste use the same asset after a rebuild-with-
/// metadata capture. Reproduces the user-visible promise:
/// "Quick Paste publishes the same bytes the preview paints".
#[test]
fn preview_and_copy_share_the_rebuilt_asset() {
    let h = harness();
    h.clipboard.set_image_png_support(true);

    let width = 3u32;
    let height = 2u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png(width, height, &rgba);
    let fixture_icc = b"clipvault-fixture-display-p3-icc";
    let tiff_bytes = encode_minimal_tiff(5669, 100, 5669, 100, 3, Some(fixture_icc));
    let tiff_metadata = parse_tiff_metadata(&tiff_bytes);
    let metadata = build_pasteboard_metadata(&png_bytes, Some(tiff_metadata));
    let image =
        ClipboardImage::with_pasteboard_metadata(rgba, width, height, png_bytes.clone(), metadata)
            .expect("image with metadata");

    let id = match h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(image),
        None,
    ) {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };

    let paste = h
        .context
        .paste()
        .paste_entry(&h.context, id, clipvault_core::PasteMode::Plain);
    assert!(matches!(paste, clipvault_core::PasteOutcome::Pasted { .. }));
    let writes = h.clipboard.written_image_pngs();
    assert_eq!(writes.len(), 1);
    let names = h.assets_on_disk();
    let on_disk = h.asset_bytes(&names[0]);
    assert_eq!(
        writes[0], on_disk,
        "Quick Paste must publish exactly the bytes the asset store persisted"
    );
}

/// Restart preserves the rebuilt PNG bytes. Regression guard
/// for the documented lifetime: the asset store stores what
/// the helper produced, and a fresh bootstrap reads the same
/// bytes back.
#[test]
fn restart_loads_rebuilt_png_asset() {
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    let db_path = dir.path().join("clipvault.db");

    let build = |data_dir: std::path::PathBuf| {
        let clipboard = Arc::new(FakeClipboardBackend::with_image_support());
        let adapters = PlatformAdapters::new(
            clipboard as Arc<dyn ClipboardBackend>,
            Arc::new(FakeHotkeyManager::new()),
            Arc::new(FakeActiveApplication::new()),
            Arc::new(FakePasteController::new()),
            Arc::new(FakeTrayController::new()),
            Arc::new(FakeSettingsNavigator::new()),
            Arc::new(clipvault_core::NoopApplicationMetadataProvider)
                as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
            Capabilities::ALL_AVAILABLE,
            PlatformInfo {
                home_dir: dir.path().to_path_buf(),
                data_dir,
                os_family: OsFamily::Macos,
                display_server: DisplayServer::Unknown,
            },
        );
        AppBootstrap::new()
            .with_clock(Arc::new(FixedClock {
                instant: datetime!(2026-01-02 03:04:05 UTC),
            }))
            .with_platform_adapters(adapters)
            .bootstrap_at(&db_path)
            .expect("bootstrap")
    };

    let width = 5u32;
    let height = 3u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png(width, height, &rgba);
    let fixture_icc = b"clipvault-fixture-display-p3-icc";
    let tiff_bytes = encode_minimal_tiff(5669, 100, 5669, 100, 3, Some(fixture_icc));
    let tiff_metadata = parse_tiff_metadata(&tiff_bytes);
    let metadata = build_pasteboard_metadata(&png_bytes, Some(tiff_metadata));
    let image =
        ClipboardImage::with_pasteboard_metadata(rgba, width, height, png_bytes.clone(), metadata)
            .expect("image with metadata");

    let asset_ref = {
        let context = build(data_dir.clone());
        let id = match context.history().record_clipboard_payload(
            &context,
            ClipboardPayload::Image(image),
            None,
        ) {
            HistoryOutcome::Stored { id } => id,
            other => panic!("expected Stored, got {other:?}"),
        };
        let mut db = context.database().lock();
        let repo = EntryRepository::new(db.connection_mut());
        let record = repo.find_by_id(id).expect("query").expect("row");
        record.asset_ref.expect("asset_ref")
    };

    // Restart: a fresh context, same SQLite file, same data dir.
    let _context = build(data_dir.clone());
    let store = ClipboardAssetStore::new(data_dir);
    let bytes = store.read_bytes(&asset_ref).expect("asset bytes");
    assert!(png_contains_chunk(&bytes, b"pHYs"));
    assert!(png_contains_chunk(&bytes, b"iCCP"));
    let (decoded_w, decoded_h, decoded_rgba) = decode_png_rgba(&bytes);
    assert_eq!(decoded_w, width);
    assert_eq!(decoded_h, height);
    assert_eq!(decoded_rgba, distinct_pixels(width, height));
    // The asset_ref SHA-256 matches the persisted bytes, not the
    // original PNG. A future regression that re-encodes would
    // surface as a hash mismatch.
    assert_eq!(asset_ref, format!("clipboard/{}.png", sha256_hex(&bytes)));
}

// ---------------------------------------------------------------------
// Diagnostic / metadata-only
// ---------------------------------------------------------------------

/// `ImageCaptureDiagnostic::Display` never echoes the asset
/// bytes, the byte length, the asset reference or any absolute
/// path. The diagnostic is metadata-only at every level.
#[test]
fn capture_diagnostic_never_leaks_payload_metadata() {
    let diagnostic = ImageCaptureDiagnostic::from_normalized(
        ImageSource::NativePngPlusMetadata,
        RepresentationSource::TiffIfd,
        "png_with_phys_only",
        true,
        true,
        Some(144),
        Some(144),
        ColorProfileKind::TiffIccProfile,
        true,
        1104,
        396,
    );
    let rendered = format!("{diagnostic}");
    for forbidden in [
        "bytes",
        "len=",
        "hash",
        "asset_ref",
        "/Users",
        "/home",
        ".clipvault",
        "data_dir",
    ] {
        assert!(
            !rendered.contains(forbidden),
            "diagnostic must not contain {forbidden:?}, got {rendered:?}"
        );
    }
    assert!(rendered.contains("image_source=native_png_plus_metadata"));
    assert!(rendered.contains("representation_source=tiff_ifd"));
    assert!(rendered.contains("dpi_x=144"));
    assert!(rendered.contains("dpi_y=144"));
}

/// `PngMetadataSummary::kind_str` returns the documented
/// snake_case strings the bridge and the persistence layer
/// rely on.
#[test]
fn png_metadata_summary_kind_strings_are_stable() {
    let only_phys = PngMetadataSummary {
        phys: Some([0; 9]),
        ..Default::default()
    };
    assert_eq!(only_phys.kind_str(), "png_with_phys_only");

    let phys_and_iccp = PngMetadataSummary {
        phys: Some([0; 9]),
        icc_profile_chunk: Some(b"profile".to_vec()),
        ..Default::default()
    };
    assert_eq!(phys_and_iccp.kind_str(), "png_with_phys_and_iccp");

    let phys_and_srgb = PngMetadataSummary {
        phys: Some([0; 9]),
        has_srgb: true,
        ..Default::default()
    };
    assert_eq!(phys_and_srgb.kind_str(), "png_with_phys_and_srgb");

    let none = PngMetadataSummary::default();
    assert_eq!(none.kind_str(), "png_without_phys");
}

/// `TiffMetadata::dpi_x` / `dpi_y` produce the integer DPI the
/// bridge surfaces for the display-card diagnostic surface.
#[test]
fn tiff_metadata_dpi_helpers_match_expected() {
    let bytes = encode_minimal_tiff(5669, 100, 5669, 100, 3, None);
    let metadata = parse_tiff_metadata(&bytes);
    // 5669/100 ppm → ~143.99 dpi. The helper rounds to 144.
    assert!(metadata.dpi_x().unwrap_or(0) >= 143);
    assert!(metadata.dpi_y().unwrap_or(0) >= 143);

    let bytes = encode_minimal_tiff(144, 1, 144, 1, 2, None);
    let metadata = parse_tiff_metadata(&bytes);
    assert_eq!(metadata.dpi_x(), Some(144));
    assert_eq!(metadata.dpi_y(), Some(144));
}

/// Regression for Apple/ImageIO TIFFs that carry X/Y resolution but
/// omit a usable `ResolutionUnit`.  Preview still reports the values
/// as ppi; the macOS compatibility conversion must therefore inject a
/// 144 ppi `pHYs` chunk instead of silently accepting the PNG default
/// of 72 ppi.
#[test]
fn tiff_resolution_without_unit_rebuilds_png_at_144_ppi() {
    let width = 4u32;
    let height = 3u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png(width, height, &rgba);
    let tiff_metadata = parse_tiff_metadata(&encode_minimal_tiff(144, 1, 144, 1, 1, None));
    assert!(tiff_metadata.has_resolution());
    assert!(tiff_metadata.dpi_x().is_none());
    assert_eq!(tiff_metadata.macos_pixels_per_meter_x(), Some(5669));

    let metadata = build_pasteboard_metadata(&png_bytes, Some(tiff_metadata));
    let image = image_with_metadata(rgba.clone(), width, height, png_bytes, metadata);
    let normalized =
        clipvault_core::normalize_image_with_original(&image).expect("normalize with TIFF ppi");
    let phys = png_metadata_summary(normalized.png())
        .phys
        .expect("rebuilt PNG must carry pHYs");
    assert_eq!(&phys[0..4], &5669u32.to_be_bytes());
    assert_eq!(&phys[4..8], &5669u32.to_be_bytes());
    assert_eq!(phys[8], 1);
}

/// Regression for `⌘⇧4` followed by the screenshot UI's Copy
/// action when macOS publishes a TIFF raster but no usable PNG
/// leg. The platform bridge decodes the TIFF and the core must
/// rebuild the canonical PNG with the TIFF's 144 ppi metadata,
/// rather than taking the arboard path (which always writes 72 ppi
/// or no resolution at all).
#[test]
fn tiff_only_capture_rebuilds_png_at_144_ppi_without_original_png() {
    let width = 17u32;
    let height = 9u32;
    let rgba = distinct_pixels(width, height);
    let tiff_metadata = parse_tiff_metadata(&encode_minimal_tiff(144, 1, 144, 1, 2, None));
    assert_eq!(tiff_metadata.dpi_x(), Some(144));
    assert_eq!(tiff_metadata.dpi_y(), Some(144));

    let metadata = build_pasteboard_metadata(&[], Some(tiff_metadata));
    let image = ClipboardImage::with_pasteboard_metadata_without_original(
        rgba.clone(),
        width,
        height,
        metadata,
    )
    .expect("TIFF-only image is structurally valid");

    let normalized = clipvault_core::normalize_image_with_original(&image)
        .expect("TIFF-only capture must normalize");
    let phys = png_metadata_summary(normalized.png())
        .phys
        .expect("TIFF-only capture must carry pHYs");
    assert_eq!(&phys[0..4], &5669u32.to_be_bytes());
    assert_eq!(&phys[4..8], &5669u32.to_be_bytes());
    assert_eq!(phys[8], 1);
    assert_eq!(
        normalized.source(),
        clipvault_core::NormalizedSource::TiffMetadataOnly
    );
    assert!(!normalized.has_original_png_bytes());

    let (decoded_w, decoded_h, decoded_rgba) = decode_png_rgba(normalized.png());
    assert_eq!((decoded_w, decoded_h), (width, height));
    assert_eq!(decoded_rgba, rgba);
}

// ---------------------------------------------------------------------
// Composite clipboard surface (no regressions)
// ---------------------------------------------------------------------

/// The platform's `MAX_CLIPBOARD_IMAGE_DIM` cap is reachable
/// through the public re-exports so test fixtures can stay
/// synchronised with the production cap.
#[test]
fn max_image_dim_constant_matches_public_surface() {
    let _ = MAX_CLIPBOARD_IMAGE_DIM;
}
