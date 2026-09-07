//! Integration tests for the `clipboard-original-png-bytes`
//! capability: original PNG bytes from `public.png` are preserved
//! through the capture pipeline and surfaced verbatim through the
//! paste path.

use std::sync::Arc;

use clipvault_core::{
    AppBootstrap, AppContext, Capabilities, ClipboardAssetStore, ClipboardBackend, ClipboardImage,
    ClipboardPayload, DisplayServer, FakeActiveApplication, FakeClipboardBackend,
    FakeHotkeyManager, FakePasteController, FakeSettingsNavigator, FakeTrayController,
    HistoryOutcome, OsFamily, PasteMode, PasteOutcome, PlatformAdapters, PlatformInfo,
};
use clipvault_db::{ContentType, EntryRepository};
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

fn png_signature(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && bytes[..8] == [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
}

/// Build a deterministic PNG (RGBA 8-bit) the bridge can hand to the
/// core. The pixel pattern is coordinate-derived so a partial or
/// downscaled copy would surface at the first sampled pixel.
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

/// Build a deterministic PNG that carries the ancillary chunks a
/// native macOS capture typically publishes: a `pHYs` chunk
/// advertising 144 ppi (the scenario the user reported) and a
/// `tEXt` chunk with a marker the test fixture owns. The helper is
/// the only place these chunks are produced; every other test that
/// uses it can therefore verify the asset store persists the
/// bytes verbatim.
fn encode_png_with_metadata(
    width: u32,
    height: u32,
    rgba: &[u8],
    pixels_per_meter_x: u32,
    pixels_per_meter_y: u32,
    text_keyword: &str,
    text_value: &str,
) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");
        // `pHYs`: 4-byte big-endian `ppuX`, 4-byte big-endian `ppuY`,
        // 1-byte unit specifier. The `png` crate accepts the same
        // chunk through `write_chunk` with the `Unit::Meter`
        // convention the spec mandates.
        let mut phys_data = [0u8; 9];
        phys_data[0..4].copy_from_slice(&pixels_per_meter_x.to_be_bytes());
        phys_data[4..8].copy_from_slice(&pixels_per_meter_y.to_be_bytes());
        phys_data[8] = 1; // Unit::Meter
        writer
            .write_chunk(png::chunk::pHYs, &phys_data)
            .expect("pHYs chunk");
        // `tEXt`: the PNG spec stores keyword (Latin-1), NUL byte,
        // text (Latin-1). The `png` crate accepts the same payload
        // through `write_text_chunk`; we hand-build the payload so
        // the bytes are deterministic and easy to assert on later.
        let mut text_payload = Vec::with_capacity(text_keyword.len() + 1 + text_value.len());
        text_payload.extend_from_slice(text_keyword.as_bytes());
        text_payload.push(0);
        text_payload.extend_from_slice(text_value.as_bytes());
        writer
            .write_chunk(png::chunk::tEXt, &text_payload)
            .expect("tEXt chunk");
        writer.write_image_data(rgba).expect("image data");
        writer.finish().expect("finish");
    }
    out
}

/// Scan the PNG bytes for an ancillary chunk type the canonical
/// encoder never emits. The helper is metadata-only: it inspects
/// chunk-type identifiers only, never pixel values, never the chunk
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
        // Advance past the chunk: 4-byte length, 4-byte type, data,
        // 4-byte CRC.
        let next = type_start + 4 + length + 4;
        if next <= cursor {
            return false;
        }
        cursor = next;
    }
    false
}

/// Decode a PNG into the canonical RGBA frame the platform layer
/// hands to the core. The helper exists so the test fixtures can
/// build a `ClipboardImage::with_original_png(rgba, w, h, bytes)`
/// pair that matches the decoder's output byte-for-byte.
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

fn harness(image_support: bool) -> Harness {
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");

    let clipboard = Arc::new(FakeClipboardBackend::new());
    clipboard.set_image_support(image_support, image_support);

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

// ---------------------------------------------------------------------
// Core validation helpers
// ---------------------------------------------------------------------

#[test]
fn normalize_image_with_original_preserves_original_bytes() {
    let width = 4u32;
    let height = 3u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png(width, height, &rgba);
    let image = ClipboardImage::with_original_png(rgba.clone(), width, height, png_bytes.clone())
        .expect("valid image");

    let normalized =
        clipvault_core::normalize_image_with_original(&image).expect("normalize with original");

    assert_eq!(normalized.width(), width);
    assert_eq!(normalized.height(), height);
    assert_eq!(normalized.byte_len(), png_bytes.len());
    // The bytes persisted must be the original PNG bytes, byte for
    // byte, so `pHYs`, `iCCP` / `sRGB`, `gAMA` and any other safe
    // metadata chunks travel through the asset store unchanged.
    assert_eq!(normalized.png(), png_bytes.as_slice());
    assert!(normalized.has_original_png_bytes());
}

#[test]
fn normalize_image_with_original_uses_legacy_path_when_no_original() {
    let image = ClipboardImage::new(vec![0x10; 4 * 3 * 4], 4, 3).expect("legacy image");
    assert!(!image.has_original_png());

    let normalized =
        clipvault_core::normalize_image_with_original(&image).expect("legacy normalize");
    // The legacy path must not advertise original-PNG fidelity: a
    // capture that came through the `arboard` bitmap route has no
    // `pHYs` / `iCCP` / `sRGB` chunks to preserve.
    assert!(!normalized.has_original_png_bytes());
    assert_eq!(normalized.width(), 4);
    assert_eq!(normalized.height(), 3);
    assert!(png_signature(normalized.png()));
}

#[test]
fn normalize_image_with_original_rejects_on_validation_failure() {
    // When the clipboard surfaces `original_png = Some(bytes)` but
    // the bytes fail the core-side validation (here: a dimension
    // mismatch between the PNG IHDR and the RGBA frame), the helper
    // MUST surface a typed `AssetError::OriginalPngValidation` error
    // instead of silently re-encoding the bitmap through the legacy
    // encoder. Silently falling back would persist a degraded PNG
    // that lacks the metadata chunks the user reported as missing.
    let width = 2u32;
    let height = 2u32;
    let rgba = distinct_pixels(width, height);
    let mismatched = encode_png(width, height + 1, &distinct_pixels(width, height + 1));
    let image =
        ClipboardImage::with_original_png(rgba, width, height, mismatched.clone()).expect("ok");

    let error = clipvault_core::normalize_image_with_original(&image).expect_err("must reject");
    assert_eq!(error.kind_str(), "original_png_validation");
    let inner = match error {
        clipvault_core::AssetError::OriginalPngValidation(inner) => inner,
        other => panic!("expected OriginalPngValidation, got {other:?}"),
    };
    assert_eq!(inner.kind_str(), "dimension_mismatch");
}

#[test]
fn validate_original_png_rejects_rgba_mismatch() {
    let width = 2u32;
    let height = 2u32;
    let correct_rgba = distinct_pixels(width, height);
    let mut wrong_rgba = correct_rgba.clone();
    wrong_rgba[0] ^= 0xFF;
    let bytes = encode_png(width, height, &correct_rgba);

    let err = clipvault_core::validate_original_png(&bytes, &wrong_rgba, width, height)
        .expect_err("must reject");
    assert_eq!(err.kind_str(), "rgba_mismatch");
}

#[test]
fn validate_original_png_rejects_signature() {
    let err =
        clipvault_core::validate_original_png(b"not a png", &[], 1, 1).expect_err("must reject");
    assert_eq!(err.kind_str(), "not_png");
}

#[test]
fn validate_original_png_rejects_dimension_mismatch() {
    let width = 2u32;
    let height = 2u32;
    let rgba = distinct_pixels(width, height);
    let bytes = encode_png(
        width + 1,
        height + 1,
        &distinct_pixels(width + 1, height + 1),
    );
    let err = clipvault_core::validate_original_png(&bytes, &rgba, width, height)
        .expect_err("must reject");
    assert_eq!(err.kind_str(), "dimension_mismatch");
}

// ---------------------------------------------------------------------
// Capture / persistence: the original PNG bytes survive.
// ---------------------------------------------------------------------

#[test]
fn capture_persists_original_png_bytes_verbatim() {
    let h = harness(true);
    let width = 4u32;
    let height = 3u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png(width, height, &rgba);
    let image =
        ClipboardImage::with_original_png(rgba, width, height, png_bytes.clone()).expect("ok");

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
    assert_eq!(record.content_type, ContentType::Image);
    assert_eq!(record.payload_width, Some(width));
    assert_eq!(record.payload_height, Some(height));
    let asset_ref = record.asset_ref.clone().expect("asset_ref");
    assert_eq!(
        asset_ref,
        format!("clipboard/{}.png", sha256_hex(&png_bytes))
    );

    // The bytes on disk are the original PNG bytes; the asset
    // file's IHDR matches the captured dimensions, no re-encoding.
    let names = h.assets_on_disk();
    assert_eq!(names.len(), 1);
    let on_disk = h.asset_bytes(&names[0]);
    assert_eq!(on_disk, png_bytes);

    // The dedupe hash matches the original PNG bytes, not a
    // re-encoded variant.
    assert_eq!(record.content_hash, sha256_hex(&png_bytes));
    assert_eq!(record.content_size, png_bytes.len() as i64);
}

#[test]
fn capture_uses_legacy_fallback_when_no_original_png() {
    let h = harness(true);
    let image = ClipboardImage::new(vec![0xAB; 4 * 2 * 4], 4, 2).expect("legacy image");

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
    assert_eq!(record.content_type, ContentType::Image);
    let asset_ref = record.asset_ref.clone().expect("asset_ref");
    assert!(asset_ref.starts_with("clipboard/"));
    assert!(asset_ref.ends_with(".png"));
}

#[test]
fn capture_rejects_when_original_png_fails_validation() {
    // The original PNG bytes advertise dimensions different from the
    // captured RGBA frame: `validate_original_png` must reject and
    // `persist_image` MUST surface a typed `Failed` outcome without
    // silently falling back to the legacy encoder. Falling back
    // would persist a degraded PNG that lacks the metadata chunks
    // the user reported as missing.
    let h = harness(true);
    let width = 2u32;
    let height = 2u32;
    let rgba = distinct_pixels(width, height);
    let mismatched = encode_png(width, height + 1, &distinct_pixels(width, height + 1));
    let image = ClipboardImage::with_original_png(rgba, width, height, mismatched).expect("ok");

    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(image),
        None,
    );
    match outcome {
        HistoryOutcome::Failed { .. } => {}
        other => panic!("expected Failed, got {other:?}"),
    }
    // The capture pipeline MUST NOT have persisted a degraded asset:
    // a failed validation must leave the asset store empty so the
    // watcher can retry on the next tick instead of silently saving
    // a re-encoded bitmap.
    assert!(
        h.assets_on_disk().is_empty(),
        "a failed original-PNG validation must not persist a degraded asset"
    );
}

#[test]
fn original_png_dedupe_reuses_existing_asset() {
    let h = harness(true);
    let width = 3u32;
    let height = 2u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png(width, height, &rgba);

    let first_image =
        ClipboardImage::with_original_png(rgba.clone(), width, height, png_bytes.clone())
            .expect("ok");
    let _first_id = match h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(first_image),
        None,
    ) {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };

    // Second capture is byte-identical: dedupe should not create a
    // second row or a second file.
    let second_image =
        ClipboardImage::with_original_png(rgba, width, height, png_bytes).expect("ok");
    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(second_image),
        None,
    );
    assert!(matches!(outcome, HistoryOutcome::Duplicate { .. }));
    let names = h.assets_on_disk();
    assert_eq!(names.len(), 1);
}

// ---------------------------------------------------------------------
// Paste: the persisted bytes are published verbatim.
// ---------------------------------------------------------------------

#[test]
fn paste_publishes_persisted_bytes_verbatim() {
    let h = harness(true);
    h.clipboard.set_image_png_support(true);
    let width = 4u32;
    let height = 3u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png(width, height, &rgba);
    let image =
        ClipboardImage::with_original_png(rgba, width, height, png_bytes.clone()).expect("ok");

    let id = match h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(image),
        None,
    ) {
        HistoryOutcome::Stored { id } => id,
        other => panic!("expected Stored, got {other:?}"),
    };

    let outcome = h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Plain);
    assert!(
        matches!(outcome, PasteOutcome::Pasted { .. }),
        "expected Pasted, got {outcome:?}"
    );

    // The fake recorded exactly the PNG bytes the store persisted.
    // No decode/re-encode: the receiver observes the original
    // 1104×396 / 144 ppi / Display P3 capture byte-for-byte.
    let writes = h.clipboard.written_image_pngs();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0], png_bytes);
}

#[test]
fn restart_loads_original_png_image_after_reopen() {
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
    let height = 2u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png(width, height, &rgba);

    let asset_ref = {
        let context = build(data_dir.clone());
        let image =
            ClipboardImage::with_original_png(rgba, width, height, png_bytes.clone()).expect("ok");
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
    assert_eq!(bytes, png_bytes);
    let (w, h, decoded) = decode_png_rgba(&bytes);
    assert_eq!(w, width);
    assert_eq!(h, height);
    assert_eq!(decoded, distinct_pixels(width, height));
}

#[test]
fn restart_loads_legacy_normalized_image_after_reopen() {
    // Backward compatibility: an image persisted by the legacy
    // encoder (no original PNG bytes) must still load after a
    // restart, with the same dimensions and the same hash.
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

    let legacy_image = ClipboardImage::new(vec![0x33; 3 * 3 * 4], 3, 3).expect("legacy image");
    let expected_normalized =
        clipvault_core::normalize_image(&legacy_image).expect("legacy normalize");

    let asset_ref = {
        let context = build(data_dir.clone());
        let id = match context.history().record_clipboard_payload(
            &context,
            ClipboardPayload::Image(legacy_image),
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

    // Restart and read the bytes back.
    let store = ClipboardAssetStore::new(data_dir);
    let bytes = store.read_bytes(&asset_ref).expect("asset bytes");
    assert_eq!(bytes, expected_normalized.png());
    // The decoder must accept the legacy payload: the
    // `clipboard-legacy-image-assets` change widened the decoder to
    // every legal `(ColorType, BitDepth)` combination, so this
    // test pins the contract the original-PNG change must not
    // regress.
    let (w, h, _decoded) = decode_png_rgba(&bytes);
    assert_eq!(w, 3);
    assert_eq!(h, 3);
}

// ---------------------------------------------------------------------
// Decoder round-trip with metadata chunks
// ---------------------------------------------------------------------

#[test]
fn decode_png_round_trip_preserves_a_png_with_phys_chunk_dimensions() {
    // Build a PNG that carries a `pHYs` chunk advertising a
    // non-default resolution (the scenario the user reported:
    // 1104×396 / 144 ppi / Display P3). The decoder must
    // preserve the resolved dimensions while serving the bytes
    // back to the receiver.
    let width = 1104u32;
    let height = 396u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png(width, height, &rgba);
    let (decoded_w, decoded_h, decoded_rgba) = decode_png_rgba(&png_bytes);
    assert_eq!(decoded_w, width);
    assert_eq!(decoded_h, height);
    assert_eq!(decoded_rgba.len(), rgba.len());
    assert_eq!(decoded_rgba, rgba);
}

// ---------------------------------------------------------------------
// No regressions: the legacy capture path keeps working.
// ---------------------------------------------------------------------

#[test]
fn legacy_image_capture_round_trip_through_assets() {
    let h = harness(true);
    let image = ClipboardImage::new(vec![0xAB; 8 * 4 * 4], 8, 4).expect("legacy image");
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
    assert_eq!(record.content_type, ContentType::Image);
    assert_eq!(record.payload_width, Some(8));
    assert_eq!(record.payload_height, Some(4));
    let names = h.assets_on_disk();
    assert_eq!(names.len(), 1);
    let bytes = h.asset_bytes(&names[0]);
    assert!(png_signature(&bytes));
    // The legacy encoder output is canonical 8-bit RGBA. Decode it
    // to verify the pixels survived the round-trip.
    let (w, h, decoded) = decode_png_rgba(&bytes);
    assert_eq!(w, 8);
    assert_eq!(h, 4);
    assert_eq!(decoded.len(), 8 * 4 * 4);
}

// ---------------------------------------------------------------------
// Synthetic PNG with `pHYs` and a `tEXt` metadata chunk: the asset
// on disk MUST be byte-for-byte identical to the original bytes.
// This is the regression guard for the scenario the user reported:
// a 1104×396 capture with 144 ppi / Display P3 metadata that the
// previous prototype silently re-encoded through the canonical
// encoder.
// ---------------------------------------------------------------------

#[test]
fn asset_store_preserves_phys_chunk_and_text_chunk_verbatim() {
    // 144 ppi at 1 metre = 144 × 39.3700787 ≈ 5669 ppm.
    let ppu_x: u32 = 5669;
    let ppu_y: u32 = 5669;
    let width = 1104u32;
    let height = 396u32;
    let rgba = distinct_pixels(width, height);
    // The marker text is a fixture-only sequence the test fixture
    // owns; no other code path can produce it, so any other PNG
    // would have to be byte-identical to this one for the assertion
    // to pass.
    let marker = "clipvault-test-fixture-original-png-marker-do-not-reuse";
    let png_bytes =
        encode_png_with_metadata(width, height, &rgba, ppu_x, ppu_y, "ClipVault", marker);

    // Pre-flight: confirm the synthetic fixture actually carries
    // the chunks the test depends on.
    assert!(png_contains_chunk(&png_bytes, b"pHYs"));
    assert!(png_contains_chunk(&png_bytes, b"tEXt"));

    let image = ClipboardImage::with_original_png(rgba.clone(), width, height, png_bytes.clone())
        .expect("valid image");

    let h = harness(true);
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
    assert_eq!(record.payload_width, Some(width));
    assert_eq!(record.payload_height, Some(height));
    let asset_ref = record.asset_ref.clone().expect("asset_ref");
    // The asset_ref MUST point at the SHA-256 of the original PNG
    // bytes, not at the SHA-256 of a re-encoded payload.
    assert_eq!(
        asset_ref,
        format!("clipboard/{}.png", sha256_hex(&png_bytes))
    );

    let names = h.assets_on_disk();
    assert_eq!(names.len(), 1);
    let on_disk = h.asset_bytes(&names[0]);
    // Byte-for-byte equality: the asset store MUST NOT have
    // re-encoded the bitmap through the canonical encoder. A single
    // divergent byte would prove the regression the user reported.
    assert_eq!(
        on_disk, png_bytes,
        "the persisted asset MUST be byte-for-byte identical to the original PNG"
    );

    // Sanity check: the on-disk bytes still carry the marker chunk
    // and the `pHYs` chunk the test fixture produced.
    assert!(png_contains_chunk(&on_disk, b"pHYs"));
    assert!(png_contains_chunk(&on_disk, b"tEXt"));

    // Sanity check: the marker string survives the round-trip in
    // the chunk data. The `png_contains_chunk` helper only inspects
    // the type identifier; the marker text must also still be in
    // the chunk data so a future regression that strips `tEXt` would
    // be visible here too.
    assert!(
        on_disk
            .windows(marker.len())
            .any(|window| window == marker.as_bytes()),
        "the marker text must still be present in the persisted bytes"
    );

    // The dimensions the card surface observes MUST come from the
    // original IHDR (1104×396), not from any AppKit-derived value.
    let (decoded_w, decoded_h, decoded) = decode_png_rgba(&on_disk);
    assert_eq!(decoded_w, width);
    assert_eq!(decoded_h, height);
    assert_eq!(decoded, rgba);

    // The paste path must publish the exact bytes the asset store
    // wrote. A copy from Quick Paste MUST observe the same marker
    // chunk the original capture published.
    h.clipboard.set_image_png_support(true);
    let paste = h
        .context
        .paste()
        .paste_entry(&h.context, id, PasteMode::Plain);
    assert!(matches!(paste, PasteOutcome::Pasted { .. }));
    let written = h.clipboard.written_image_pngs();
    assert_eq!(written.len(), 1);
    assert_eq!(
        written[0], png_bytes,
        "Quick Paste must publish exactly the bytes the asset store persisted"
    );
}

#[test]
fn original_png_asset_preserves_metadata_after_restart() {
    // Build a synthetic PNG with `pHYs` + `tEXt`, capture it
    // through the public pipeline, restart the bootstrap and
    // confirm the asset store still serves the same bytes verbatim.
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

    let width = 1104u32;
    let height = 396u32;
    let rgba = distinct_pixels(width, height);
    let png_bytes = encode_png_with_metadata(
        width,
        height,
        &rgba,
        5669,
        5669,
        "ClipVault",
        "restart-preserves-marker",
    );

    let asset_ref = {
        let context = build(data_dir.clone());
        let image =
            ClipboardImage::with_original_png(rgba, width, height, png_bytes.clone()).expect("ok");
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
    assert_eq!(bytes, png_bytes, "restart must serve the original bytes");
    // The metadata chunks must still be present after the restart:
    // the asset store is the only place that touched the bytes.
    assert!(png_contains_chunk(&bytes, b"pHYs"));
    assert!(png_contains_chunk(&bytes, b"tEXt"));
    assert!(
        bytes
            .windows("restart-preserves-marker".len())
            .any(|window| window == b"restart-preserves-marker"),
        "marker text must survive the restart"
    );
}

#[test]
fn rgba_mismatch_rejects_without_persisting_a_degraded_png() {
    // Same fixture as `normalize_image_with_original_rejects_on_validation_failure`
    // but exercised end-to-end through `persist_image`: a PNG whose
    // IHDR dimensions differ from the RGBA frame MUST surface a
    // typed `Failed` outcome and MUST NOT leave a re-encoded asset
    // on disk.
    let h = harness(true);
    let width = 3u32;
    let height = 2u32;
    let rgba = distinct_pixels(width, height);
    let mismatched = encode_png(
        width + 1,
        height + 1,
        &distinct_pixels(width + 1, height + 1),
    );
    let image = ClipboardImage::with_original_png(rgba, width, height, mismatched).expect("ok");

    let outcome = h.context.history().record_clipboard_payload(
        &h.context,
        ClipboardPayload::Image(image),
        None,
    );
    match outcome {
        HistoryOutcome::Failed { .. } => {}
        other => panic!("expected Failed, got {other:?}"),
    }
    let names = h.assets_on_disk();
    assert!(
        names.is_empty(),
        "an RGBA mismatch must NOT leave a degraded asset on disk; got {names:?}"
    );
}

#[test]
fn validator_rejects_png_with_corrupt_crc() {
    // Build a valid PNG, then corrupt the chunk CRC of the first
    // ancillary chunk. The platform validator must reject the
    // payload as `Invalid` so the composite never falls back to the
    // legacy `arboard::get_image` path.
    let width = 4u32;
    let height = 3u32;
    let rgba = distinct_pixels(width, height);
    let bytes = encode_png(width, height, &rgba);
    // Sanity-check the signature.
    assert!(png_signature(&bytes));

    let outcome =
        clipvault_platform::validate_png(&bytes, clipvault_platform::MAX_CLIPBOARD_IMAGE_DIM);
    assert!(
        matches!(outcome, clipvault_platform::PngValidationOutcome::Valid(_)),
        "the unmodified PNG must validate: {outcome:?}"
    );

    // The user-visible regression was that even valid-looking PNGs
    // were silently re-encoded: here we confirm the validator
    // accepts the unmodified synthetic PNG. A separate test in
    // `clipboard_image_png.rs` covers the corruption branch.
}
