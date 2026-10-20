//! Local raster-image decoding for the explicit file-import flow.
//!
//! File paths stay in the core/Tauri process. The decoder only returns the
//! platform-neutral RGBA image that the existing history pipeline converts
//! into ClipVault's canonical PNG asset.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use image::{ImageDecoder, ImageFormat, ImageReader, Limits};
use thiserror::Error;

use clipvault_platform::{
    checked_rgba_len, ClipboardImage, MAX_CLIPBOARD_IMAGE_DIM, MAX_CLIPBOARD_IMAGE_RGBA_BYTES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ImageFileImportError {
    #[error("image file could not be opened")]
    Open,
    #[error("image file format is not supported")]
    UnsupportedFormat,
    #[error("image file could not be decoded")]
    Decode,
    #[error("image file exceeds the supported image limits")]
    TooLarge,
}

/// Decode one supported raster file into the bounded RGBA representation used
/// by clipboard captures. Format detection uses file contents, not its name.
pub fn decode_image_file(path: &Path) -> Result<ClipboardImage, ImageFileImportError> {
    let file = File::open(path).map_err(|_| ImageFileImportError::Open)?;
    let mut reader = ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .map_err(|_| ImageFileImportError::Decode)?;

    if !reader.format().is_some_and(is_supported_format) {
        return Err(ImageFileImportError::UnsupportedFormat);
    }

    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_CLIPBOARD_IMAGE_DIM);
    limits.max_image_height = Some(MAX_CLIPBOARD_IMAGE_DIM);
    limits.max_alloc = Some(MAX_CLIPBOARD_IMAGE_RGBA_BYTES as u64);
    reader.limits(limits);

    let mut decoder = reader
        .into_decoder()
        .map_err(|_| ImageFileImportError::Decode)?;
    if decoder.total_bytes() > MAX_CLIPBOARD_IMAGE_RGBA_BYTES as u64 {
        return Err(ImageFileImportError::TooLarge);
    }
    let orientation = decoder
        .orientation()
        .map_err(|_| ImageFileImportError::Decode)?;
    let mut decoded =
        image::DynamicImage::from_decoder(decoder).map_err(|_| ImageFileImportError::Decode)?;
    decoded.apply_orientation(orientation);

    let (width, height) = (decoded.width(), decoded.height());
    checked_rgba_len(width, height).map_err(|_| ImageFileImportError::TooLarge)?;
    let rgba = decoded.into_rgba8().into_raw();
    ClipboardImage::new(rgba, width, height).map_err(|_| ImageFileImportError::TooLarge)
}

fn is_supported_format(format: ImageFormat) -> bool {
    matches!(
        format,
        ImageFormat::Bmp
            | ImageFormat::Gif
            | ImageFormat::Jpeg
            | ImageFormat::Png
            | ImageFormat::Tiff
            | ImageFormat::WebP
    )
}
