//! Local raster-image decoding for the explicit file-import flow.
//!
//! File paths stay in the core/Tauri process. The decoder only returns the
//! platform-neutral RGBA image that the existing history pipeline converts
//! into ClipVault's canonical PNG asset.

use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
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
    decode_image_file_with_progress(path, |_| {})
}

/// Decode one supported raster file and report cumulative encoded bytes read.
/// The callback receives counters only; callers must not attach file metadata
/// or image content to a progress event.
pub fn decode_image_file_with_progress<F>(
    path: &Path,
    on_bytes_read: F,
) -> Result<ClipboardImage, ImageFileImportError>
where
    F: FnMut(u64),
{
    let file = File::open(path).map_err(|_| ImageFileImportError::Open)?;
    let mut reader = ImageReader::new(BufReader::new(CountingReader::new(file, on_bytes_read)))
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

struct CountingReader<R, F> {
    inner: R,
    bytes_read: u64,
    on_bytes_read: F,
}

impl<R, F> CountingReader<R, F> {
    fn new(inner: R, on_bytes_read: F) -> Self {
        Self {
            inner,
            bytes_read: 0,
            on_bytes_read,
        }
    }
}

impl<R, F> Read for CountingReader<R, F>
where
    R: Read,
    F: FnMut(u64),
{
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let bytes_read = self.inner.read(buffer)?;
        if bytes_read > 0 {
            self.bytes_read = self.bytes_read.saturating_add(bytes_read as u64);
            (self.on_bytes_read)(self.bytes_read);
        }
        Ok(bytes_read)
    }
}

impl<R, F> Seek for CountingReader<R, F>
where
    R: Seek,
    F: FnMut(u64),
{
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.inner.seek(position)
    }
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
