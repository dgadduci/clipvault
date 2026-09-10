//! Pure-Rust SVG → PNG rasterizer used by the Linux application
//! metadata provider to honour `.desktop` entries whose `Icon=`
//! value points at an SVG file.
//!
//! The rasterizer is intentionally narrow:
//!
//! - It parses the SVG bytes with [`usvg`], then rasterizes the
//!   resulting tree onto a [`tiny_skia::Pixmap`] and encodes the
//!   pixmap as PNG through `tiny-skia`'s built-in encoder. No
//!   external helper, no font database, no system font enumeration
//!   and no network resource are ever loaded — the resolver's
//!   `ImageHrefResolver` is wired to always return `None` so `<image>`
//!   `xlink:href` references that resolve to files or remote URLs
//!   are silently dropped. SVG `<script>` elements are not supported
//!   by `usvg` to begin with, so a malicious SVG cannot execute
//!   JavaScript through this path either.
//! - The rasterizer caps the byte length, the source dimensions
//!   and the target dimensions so a hostile `.desktop` file cannot
//!   push the renderer to consume unbounded memory. Outputs are
//!   clamped to `MAX_ICON_DIM` × `MAX_ICON_DIM` so the asset store
//!   keeps the same upper bound the macOS picker enforces.
//! - The rasterizer never logs the SVG payload, the file path or
//!   the matched identifier — every failure is reduced to a typed
//!   [`SvgRasterError`] the diagnostic sink surfaces verbatim.
//!
//! The module is gated behind the `linux-svg-raster` feature so
//! builds that do not need the rasterizer (CI smoke checks, the
//! macOS dev host) do not pull `resvg` / `usvg` / `tiny-skia` into
//! the dependency graph.

use crate::app_assets::MAX_ICON_DIM;

/// Maximum byte length the rasterizer will accept for a single SVG
/// payload. The cap mirrors the byte ceiling
/// [`crate::app_assets::MAX_ICON_BYTES_LEGACY`] enforces on the
/// read side so a tampered SVG cannot push the asset store over
/// the documented ceiling.
pub const MAX_SVG_BYTES: usize = 4 * 1024 * 1024;

/// Maximum source dimensions (`width` × `height`) the rasterizer
/// honours. SVGs that declare larger viewports are rejected with
/// [`SvgRasterError::TooLarge`] so the renderer cannot allocate an
/// unbounded pixmap.
pub const MAX_SVG_SOURCE_DIM: u32 = 1024;

/// Output of a successful rasterization. The PNG bytes are already
/// validated by `tiny-skia`'s encoder (the encoder refuses to
/// emit anything that does not start with the canonical PNG
/// signature) and clamped to [`MAX_ICON_DIM`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterizedSvg {
    /// PNG bytes the writer should persist under the
    /// `application-icons/` namespace.
    pub png_bytes: Vec<u8>,
    /// Width of the rasterized PNG in pixels.
    pub width: u32,
    /// Height of the rasterized PNG in pixels.
    pub height: u32,
}

/// Stable error the rasterizer surfaces for every failure mode.
/// The variants are part of the `linux-source-app-metadata`
/// contract: the capture diagnostic enumerates them verbatim so
/// the operator can distinguish a missing icon, an oversized SVG,
/// a malformed SVG and a rasterization failure without parsing
/// free-form strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SvgRasterError {
    /// The SVG payload exceeded [`MAX_SVG_BYTES`].
    TooLarge,
    /// The SVG declared `width` / `height` larger than
    /// [`MAX_SVG_SOURCE_DIM`].
    SourceTooLarge,
    /// The rasterized output would exceed [`MAX_ICON_DIM`]
    /// pixels on either axis after the aspect-preserving scale.
    TargetTooLarge,
    /// `usvg` rejected the bytes as malformed SVG.
    InvalidSvg,
    /// The PNG encoder refused to emit a payload (should not
    /// happen with valid pixmaps, but kept as a typed variant so
    /// the diagnostic surface never has to fall back to
    /// free-form strings).
    EncodeFailed,
}

impl SvgRasterError {
    /// Stable snake_case identifier the capture diagnostic
    /// consumes. Renaming a string is a breaking change.
    pub fn kind_str(&self) -> &'static str {
        match self {
            SvgRasterError::TooLarge => "too_large",
            SvgRasterError::SourceTooLarge => "source_too_large",
            SvgRasterError::TargetTooLarge => "target_too_large",
            SvgRasterError::InvalidSvg => "invalid_svg",
            SvgRasterError::EncodeFailed => "encode_failed",
        }
    }
}

impl std::fmt::Display for SvgRasterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SvgRasterError::TooLarge => f.write_str("svg payload exceeds the size cap"),
            SvgRasterError::SourceTooLarge => f.write_str("svg source dimensions exceed the cap"),
            SvgRasterError::TargetTooLarge => {
                f.write_str("svg target dimensions exceed the icon cap")
            }
            SvgRasterError::InvalidSvg => f.write_str("svg payload is malformed"),
            SvgRasterError::EncodeFailed => f.write_str("svg rasterizer failed to encode png"),
        }
    }
}

impl std::error::Error for SvgRasterError {}

/// Rasterize an SVG payload into a PNG of at most
/// `MAX_ICON_DIM × MAX_ICON_DIM` pixels.
///
/// The function is deterministic: same bytes always produce the
/// same PNG. The output dimensions preserve the source aspect
/// ratio and never exceed [`MAX_ICON_DIM`] on the longest side.
pub fn rasterize_svg_to_png(svg_bytes: &[u8]) -> Result<RasterizedSvg, SvgRasterError> {
    if svg_bytes.len() > MAX_SVG_BYTES {
        return Err(SvgRasterError::TooLarge);
    }

    let mut options = resvg::usvg::Options::default();
    // Disable every external resource resolver so a tampered SVG
    // cannot read files from the host filesystem, embed network
    // resources or trigger any decoder besides the one the
    // rasterizer uses internally. `usvg` does not run JavaScript
    // or honour any scripting element, so the parser itself stays
    // inert.
    options.image_href_resolver = resvg::usvg::ImageHrefResolver {
        resolve_data: Box::new(|_mime, _data, _opts| None),
        resolve_string: Box::new(|_href, _opts| None),
    };

    let tree = resvg::usvg::Tree::from_data(svg_bytes, &options)
        .map_err(|_| SvgRasterError::InvalidSvg)?;
    let size = tree.size();
    let source_width = size.width().ceil();
    let source_height = size.height().ceil();
    if !source_width.is_finite()
        || !source_height.is_finite()
        || source_width <= 0.0
        || source_height <= 0.0
    {
        return Err(SvgRasterError::InvalidSvg);
    }
    let source_width_u32 = source_width as u32;
    let source_height_u32 = source_height as u32;
    if source_width_u32 == 0
        || source_height_u32 == 0
        || source_width_u32 > MAX_SVG_SOURCE_DIM
        || source_height_u32 > MAX_SVG_SOURCE_DIM
    {
        return Err(SvgRasterError::SourceTooLarge);
    }

    let (target_width, target_height) =
        scale_dimensions(source_width_u32, source_height_u32, MAX_ICON_DIM)
            .ok_or(SvgRasterError::TargetTooLarge)?;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(target_width, target_height)
        .ok_or(SvgRasterError::TargetTooLarge)?;
    let scale_x = target_width as f32 / source_width;
    let scale_y = target_height as f32 / source_height;
    let transform = resvg::tiny_skia::Transform::from_scale(scale_x, scale_y);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let png_bytes = pixmap
        .encode_png()
        .map_err(|_| SvgRasterError::EncodeFailed)?;
    Ok(RasterizedSvg {
        png_bytes,
        width: target_width,
        height: target_height,
    })
}

/// Compute the target dimensions for an aspect-preserving scale
/// that fits the source inside `max_dim` × `max_dim`. Returns
/// `None` when either dimension is zero or the result overflows
/// `u32`.
fn scale_dimensions(source_w: u32, source_h: u32, max_dim: u32) -> Option<(u32, u32)> {
    if source_w == 0 || source_h == 0 {
        return None;
    }
    let (target_w, target_h) = if source_w <= max_dim && source_h <= max_dim {
        (source_w, source_h)
    } else {
        let ratio_w = max_dim as f32 / source_w as f32;
        let ratio_h = max_dim as f32 / source_h as f32;
        let ratio = ratio_w.min(ratio_h);
        let target_w = (source_w as f32 * ratio).round() as u32;
        let target_h = (source_h as f32 * ratio).round() as u32;
        if target_w == 0 || target_h == 0 {
            return None;
        }
        (target_w.max(1), target_h.max(1))
    };
    if target_w > max_dim || target_h > max_dim {
        return None;
    }
    Some((target_w, target_h))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_SVG: &[u8] =
        b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\"><rect width=\"16\" height=\"16\" fill=\"#336699\"/></svg>";

    #[test]
    fn scale_dimensions_preserves_aspect_ratio_for_landscape() {
        let (w, h) = scale_dimensions(256, 128, 64).expect("ok");
        assert_eq!(w, 64);
        assert_eq!(h, 32);
    }

    #[test]
    fn scale_dimensions_preserves_aspect_ratio_for_portrait() {
        let (w, h) = scale_dimensions(128, 256, 64).expect("ok");
        assert_eq!(w, 32);
        assert_eq!(h, 64);
    }

    #[test]
    fn scale_dimensions_returns_source_when_smaller_than_cap() {
        let (w, h) = scale_dimensions(48, 48, 256).expect("ok");
        assert_eq!((w, h), (48, 48));
    }

    #[test]
    fn scale_dimensions_rejects_zero_dimensions() {
        assert!(scale_dimensions(0, 16, 64).is_none());
        assert!(scale_dimensions(16, 0, 64).is_none());
    }

    #[test]
    fn rasterize_minimal_svg_produces_png() {
        let result = rasterize_svg_to_png(MINIMAL_SVG).expect("ok");
        assert_eq!(result.width, 16);
        assert_eq!(result.height, 16);
        assert!(result
            .png_bytes
            .starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]));
    }

    #[test]
    fn rasterize_rejects_payload_above_max_svg_bytes() {
        let oversized = vec![b'x'; MAX_SVG_BYTES + 1];
        let err = rasterize_svg_to_png(&oversized).expect_err("too large");
        assert_eq!(err.kind_str(), "too_large");
    }

    #[test]
    fn rasterize_rejects_malformed_svg() {
        let malformed = b"<<this is not svg>>";
        let err = rasterize_svg_to_png(malformed).expect_err("invalid");
        assert_eq!(err.kind_str(), "invalid_svg");
    }

    #[test]
    fn rasterize_caps_output_to_max_icon_dim() {
        // 1024 × 1024 SVG is the maximum the rasterizer accepts; it
        // scales down to `MAX_ICON_DIM` × `MAX_ICON_DIM`.
        let svg = format!(
            "<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\"><rect width=\"{w}\" height=\"{h}\" fill=\"#336699\"/></svg>",
            w = MAX_SVG_SOURCE_DIM,
            h = MAX_SVG_SOURCE_DIM,
        );
        let result = rasterize_svg_to_png(svg.as_bytes()).expect("ok");
        assert!(result.width <= MAX_ICON_DIM);
        assert!(result.height <= MAX_ICON_DIM);
    }

    #[test]
    fn rasterize_rejects_oversized_source_dimensions() {
        let svg = format!(
            "<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"16\"><rect width=\"{w}\" height=\"16\" fill=\"#336699\"/></svg>",
            w = MAX_SVG_SOURCE_DIM + 1
        );
        let err = rasterize_svg_to_png(svg.as_bytes()).expect_err("source too large");
        assert_eq!(err.kind_str(), "source_too_large");
    }

    #[test]
    fn rasterize_drops_external_image_href_resources() {
        // The SVG below references a local file via `xlink:href`.
        // With our resolver wired to always return `None`, the
        // image is silently dropped — the rasterizer still emits a
        // PNG, but the rectangle is the only visible content (the
        // <image> is treated as missing).
        let svg = b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"32\" height=\"32\"><rect width=\"32\" height=\"32\" fill=\"#336699\"/><image xlink:href=\"/etc/passwd\" width=\"32\" height=\"32\"/></svg>";
        let result = rasterize_svg_to_png(svg).expect("ok");
        assert!(result
            .png_bytes
            .starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]));
    }
}
