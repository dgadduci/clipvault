//! Metadata-only diagnostic snapshot for image captures and pastes.
//!
//! The diagnostic surfaces nine pieces of information the operator
//! can use to confirm the fidelity-preserving path is wired
//! correctly without inspecting the persisted bytes:
//!
//! - [`Self::image_source`]: which path the capture / paste took.
//!   `native_png` is the verbatim path (PNG already carried
//!   `pHYs` + profile chunks), `native_png_plus_metadata` is the
//!   rebuild path (PNG carried pixels only and the bridge injected
//!   metadata from a sibling leg), `tiff_metadata` is the
//!   TIFF-only path the bridge surfaces when `public.png` is
//!   absent and `public.tiff` is the only representation, and
//!   `arboard_fallback` is the legacy bitmap path on hosts that
//!   are not macOS-native;
//! - [`Self::representation_source`]: which native pasteboard
//!   flavour produced the resolution / colour profile metadata
//!   the persistence layer used (`png_chunks`,
//!   `tiff_ifd` or `none`);
//! - [`Self::png_chunks_kind`]: the snake_case summary the PNG
//!   chunk scanner produced (metadata-only);
//! - [`Self::tiff_resolution_present`]: whether the TIFF leg
//!   declared X/Y resolution at all;
//! - [`Self::tiff_icc_profile_present`]: whether the TIFF leg
//!   declared an ICC profile at all;
//! - [`Self::resolution_dpi_x`] / [`Self::resolution_dpi_y`]:
//!   the integer DPI the helper resolved, in metadata-only form.
//!   `None` is the documented signal that no metadata carrier
//!   published a usable resolution;
//! - [`Self::profile_kind`]: snake_case identifier for the
//!   profile carrier that produced the colour profile
//!   (`iccp`, `srgb`, `tiff_icc_profile`, `none`);
//! - [`Self::original_png_preserved`]: whether the asset on disk
//!   carries the original `pHYs` / `iCCP` / `sRGB` chunks the
//!   source application published;
//! - [`Self::width`] / [`Self::height`]: the IHDR dimensions of
//!   the persisted PNG.
//!
//! Every field is metadata-only: the diagnostic never carries the
//! asset bytes, the byte length, the content hash, the source-app
//! identifier, the data directory or any absolute path. The
//! [`fmt::Display`] impl renders the fields separated by commas so
//! a single log line stays grep-friendly.
//!
//! The diagnostic is gated on the `CLIPVAULT_DEBUG_IMAGE_CAPTURE`
//! environment variable (capture side) and
//! `CLIPVAULT_DEBUG_IMAGE_PASTE` (paste side) so it stays inert in
//! production and only fires when an operator explicitly opts in.

use std::fmt;

/// Where the bytes the asset store wrote down came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSource {
    /// `public.png` PNG bytes were persisted verbatim: the
    /// original asset already carried `pHYs` + a colour-profile
    /// chunk.
    NativePng,
    /// `public.png` PNG bytes were persisted through the
    /// `[rebuild_png_with_metadata]` helper: the source PNG
    /// carried the same pixels but lacked the metadata the
    /// sibling leg reported. The persistence layer produced a
    /// freshly-encoded PNG that holds the same pixel buffer plus
    /// a `pHYs` chunk (from the TIFF leg's XResolution /
    /// YResolution) and an `iCCP` chunk (from the TIFF leg's
    /// `ICCProfile` IFD entry). This is the documented bridge
    /// for the 144 ppi / Display P3 case the user reported.
    NativePngPlusMetadata,
    /// `public.tiff` was the only representation the pasteboard
    /// exposed. The bridge did not have PNG pixels to persist,
    /// so the persistence layer encoded the asset from the
    /// TIFF-derived RGBA frame plus the sibling TIFF metadata.
    TiffMetadata,
    /// `arboard::get_image` bitmap path produced the asset. The
    /// bytes are the canonical 8-bit RGBA PNG the legacy encoder
    /// produces: no `pHYs`, no `iCCP` / `sRGB`, no other metadata
    /// chunks.
    ArboardFallback,
}

impl ImageSource {
    /// Stable snake_case identifier used by the diagnostic surface
    /// and by the `CLIPVAULT_DEBUG_IMAGE_*` environment contract.
    pub fn as_str(self) -> &'static str {
        match self {
            ImageSource::NativePng => "native_png",
            ImageSource::NativePngPlusMetadata => "native_png_plus_metadata",
            ImageSource::TiffMetadata => "tiff_metadata",
            ImageSource::ArboardFallback => "arboard_fallback",
        }
    }
}

impl fmt::Display for ImageSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which native pasteboard flavour produced the resolution /
/// colour-profile metadata the persistence layer used to keep the
/// image fidelity intact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepresentationSource {
    /// The PNG chunk scanner found the metadata (`pHYs`,
    /// `iCCP`, `sRGB`) inside the source `public.png` bytes.
    PngChunks,
    /// The TIFF IFD parser found the metadata on the sibling
    /// `public.tiff` leg.
    TiffIfd,
    /// The native pasteboard omitted resolution metadata and the
    /// macOS display scale supplied the bounded last-resort value.
    DisplayScaleFallback,
    /// No native metadata was available; the persistence layer
    /// can only persist the canonical 8-bit RGBA PNG.
    None,
}

impl RepresentationSource {
    pub fn as_str(self) -> &'static str {
        match self {
            RepresentationSource::PngChunks => "png_chunks",
            RepresentationSource::TiffIfd => "tiff_ifd",
            RepresentationSource::DisplayScaleFallback => "display_scale_fallback",
            RepresentationSource::None => "none",
        }
    }
}

impl fmt::Display for RepresentationSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which colour-profile carrier the persistence layer consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorProfileKind {
    /// PNG `iCCP` chunk carrying the raw ICC profile bytes.
    Iccp,
    /// PNG `sRGB` rendering intent marker.
    Srgb,
    /// TIFF `ICCProfile` IFD entry.
    TiffIccProfile,
    /// No carrier was found.
    None,
}

impl ColorProfileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ColorProfileKind::Iccp => "iccp",
            ColorProfileKind::Srgb => "srgb",
            ColorProfileKind::TiffIccProfile => "tiff_icc_profile",
            ColorProfileKind::None => "none",
        }
    }
}

impl fmt::Display for ColorProfileKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Metadata-only snapshot of a successful image capture. The
/// diagnostic surfaces the pieces of information that explain the
/// fidelity path the capture took without leaking the asset's
/// bytes, content hash or path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageCaptureDiagnostic {
    /// Which path produced the asset.
    pub image_source: ImageSource,
    /// Which native pasteboard flavour produced the metadata the
    /// persistence layer used.
    pub representation_source: RepresentationSource,
    /// Snake_case summary of the PNG chunks the scanner
    /// encountered. `none` means the scanner did not run (the
    /// bridge did not see a `public.png` PNG).
    pub png_chunks_kind: &'static str,
    /// True when the TIFF leg declared X / Y resolution.
    pub tiff_resolution_present: bool,
    /// True when the TIFF leg declared an ICC profile.
    pub tiff_icc_profile_present: bool,
    /// Integer DPI the helper resolved for the X axis. `None`
    /// when no metadata carrier reported a usable value.
    pub resolution_dpi_x: Option<u32>,
    /// Integer DPI the helper resolved for the Y axis. `None`
    /// when no metadata carrier reported a usable value.
    pub resolution_dpi_y: Option<u32>,
    /// Colour profile carrier the persistence layer used.
    pub profile_kind: ColorProfileKind,
    /// Whether the persisted asset carries the original PNG bytes
    /// the host clipboard exposed. `true` for the
    /// fidelity-preserving paths; `false` for the legacy bitmap
    /// fallback.
    pub original_png_preserved: bool,
    /// IHDR width of the persisted PNG.
    pub width: u32,
    /// IHDR height of the persisted PNG.
    pub height: u32,
}

impl ImageCaptureDiagnostic {
    /// Build a diagnostic from the normalised image the asset store
    /// just persisted. The helper reads only the metadata the asset
    /// already exposed; it never inspects the bytes themselves.
    #[allow(clippy::too_many_arguments)]
    pub fn from_normalized(
        image_source: ImageSource,
        representation_source: RepresentationSource,
        png_chunks_kind: &'static str,
        tiff_resolution_present: bool,
        tiff_icc_profile_present: bool,
        resolution_dpi_x: Option<u32>,
        resolution_dpi_y: Option<u32>,
        profile_kind: ColorProfileKind,
        has_original_png_bytes: bool,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            image_source,
            representation_source,
            png_chunks_kind,
            tiff_resolution_present,
            tiff_icc_profile_present,
            resolution_dpi_x,
            resolution_dpi_y,
            profile_kind,
            original_png_preserved: has_original_png_bytes,
            width,
            height,
        }
    }
}

impl fmt::Display for ImageCaptureDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "image_source={}, representation_source={}, png_chunks={}, \
             tiff_resolution={}, tiff_icc_profile={}, dpi_x={}, dpi_y={}, \
             profile={}, original_png_preserved={}, width={}, height={}",
            self.image_source,
            self.representation_source,
            self.png_chunks_kind,
            self.tiff_resolution_present,
            self.tiff_icc_profile_present,
            self.resolution_dpi_x
                .map(|v| v.to_string())
                .unwrap_or_else(|| "none".to_string()),
            self.resolution_dpi_y
                .map(|v| v.to_string())
                .unwrap_or_else(|| "none".to_string()),
            self.profile_kind,
            self.original_png_preserved,
            self.width,
            self.height,
        )
    }
}

/// Emit the capture diagnostic through `tracing::debug!` when the
/// `CLIPVAULT_DEBUG_IMAGE_CAPTURE` environment variable is exactly
/// `1`. The helper is intentionally narrow: it never logs the
/// asset bytes, the content hash, the source-app identifier or the
/// data directory.
pub fn log_image_capture_diagnostic(diagnostic: ImageCaptureDiagnostic) {
    let enabled = match std::env::var_os("CLIPVAULT_DEBUG_IMAGE_CAPTURE") {
        Some(value) => value == "1",
        None => return,
    };
    if !enabled {
        return;
    }
    tracing::debug!(
        image_source = diagnostic.image_source.as_str(),
        representation_source = diagnostic.representation_source.as_str(),
        png_chunks_kind = diagnostic.png_chunks_kind,
        tiff_resolution_present = diagnostic.tiff_resolution_present,
        tiff_icc_profile_present = diagnostic.tiff_icc_profile_present,
        resolution_dpi_x = ?diagnostic.resolution_dpi_x,
        resolution_dpi_y = ?diagnostic.resolution_dpi_y,
        profile_kind = diagnostic.profile_kind.as_str(),
        original_png_preserved = diagnostic.original_png_preserved,
        width = diagnostic.width,
        height = diagnostic.height,
        "image capture diagnostic",
    );
}

/// Emit the paste diagnostic through `tracing::debug!` when the
/// `CLIPVAULT_DEBUG_IMAGE_PASTE` environment variable is exactly
/// `1`. Symmetric to [`log_image_capture_diagnostic`] so an
/// operator can compare the two snapshots to confirm a paste
/// round-trips through the same fidelity-preserving path the
/// capture took.
pub fn log_image_paste_diagnostic(diagnostic: ImageCaptureDiagnostic) {
    let enabled = match std::env::var_os("CLIPVAULT_DEBUG_IMAGE_PASTE") {
        Some(value) => value == "1",
        None => return,
    };
    if !enabled {
        return;
    }
    tracing::debug!(
        image_source = diagnostic.image_source.as_str(),
        representation_source = diagnostic.representation_source.as_str(),
        png_chunks_kind = diagnostic.png_chunks_kind,
        tiff_resolution_present = diagnostic.tiff_resolution_present,
        tiff_icc_profile_present = diagnostic.tiff_icc_profile_present,
        resolution_dpi_x = ?diagnostic.resolution_dpi_x,
        resolution_dpi_y = ?diagnostic.resolution_dpi_y,
        profile_kind = diagnostic.profile_kind.as_str(),
        original_png_preserved = diagnostic.original_png_preserved,
        width = diagnostic.width,
        height = diagnostic.height,
        "image paste diagnostic",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_source_as_str_is_stable() {
        assert_eq!(ImageSource::NativePng.as_str(), "native_png");
        assert_eq!(
            ImageSource::NativePngPlusMetadata.as_str(),
            "native_png_plus_metadata"
        );
        assert_eq!(ImageSource::TiffMetadata.as_str(), "tiff_metadata");
        assert_eq!(ImageSource::ArboardFallback.as_str(), "arboard_fallback");
    }

    #[test]
    fn image_source_display_matches_as_str() {
        assert_eq!(format!("{}", ImageSource::NativePng), "native_png");
        assert_eq!(
            format!("{}", ImageSource::NativePngPlusMetadata),
            "native_png_plus_metadata"
        );
        assert_eq!(format!("{}", ImageSource::TiffMetadata), "tiff_metadata");
        assert_eq!(
            format!("{}", ImageSource::ArboardFallback),
            "arboard_fallback"
        );
    }

    #[test]
    fn representation_source_as_str_is_stable() {
        assert_eq!(RepresentationSource::PngChunks.as_str(), "png_chunks");
        assert_eq!(RepresentationSource::TiffIfd.as_str(), "tiff_ifd");
        assert_eq!(
            RepresentationSource::DisplayScaleFallback.as_str(),
            "display_scale_fallback"
        );
        assert_eq!(RepresentationSource::None.as_str(), "none");
    }

    #[test]
    fn color_profile_kind_as_str_is_stable() {
        assert_eq!(ColorProfileKind::Iccp.as_str(), "iccp");
        assert_eq!(ColorProfileKind::Srgb.as_str(), "srgb");
        assert_eq!(
            ColorProfileKind::TiffIccProfile.as_str(),
            "tiff_icc_profile"
        );
        assert_eq!(ColorProfileKind::None.as_str(), "none");
    }

    #[test]
    fn diagnostic_display_contains_only_metadata_fields() {
        let diagnostic = ImageCaptureDiagnostic {
            image_source: ImageSource::NativePng,
            representation_source: RepresentationSource::PngChunks,
            png_chunks_kind: "png_with_phys_and_iccp",
            tiff_resolution_present: false,
            tiff_icc_profile_present: false,
            resolution_dpi_x: Some(144),
            resolution_dpi_y: Some(144),
            profile_kind: ColorProfileKind::Iccp,
            original_png_preserved: true,
            width: 1104,
            height: 396,
        };
        let rendered = format!("{diagnostic}");
        assert!(rendered.contains("image_source=native_png"));
        assert!(rendered.contains("representation_source=png_chunks"));
        assert!(rendered.contains("png_chunks=png_with_phys_and_iccp"));
        assert!(rendered.contains("tiff_resolution=false"));
        assert!(rendered.contains("tiff_icc_profile=false"));
        assert!(rendered.contains("dpi_x=144"));
        assert!(rendered.contains("dpi_y=144"));
        assert!(rendered.contains("profile=iccp"));
        assert!(rendered.contains("original_png_preserved=true"));
        assert!(rendered.contains("width=1104"));
        assert!(rendered.contains("height=396"));
        // The diagnostic MUST NOT carry any of the privacy-sensitive
        // fields the user-visible surface forbids.
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
    }

    #[test]
    fn diagnostic_display_marks_none_dpi_when_missing() {
        let diagnostic = ImageCaptureDiagnostic {
            image_source: ImageSource::ArboardFallback,
            representation_source: RepresentationSource::None,
            png_chunks_kind: "png_without_phys",
            tiff_resolution_present: false,
            tiff_icc_profile_present: false,
            resolution_dpi_x: None,
            resolution_dpi_y: None,
            profile_kind: ColorProfileKind::None,
            original_png_preserved: false,
            width: 17,
            height: 9,
        };
        let rendered = format!("{diagnostic}");
        assert!(rendered.contains("dpi_x=none"));
        assert!(rendered.contains("dpi_y=none"));
        assert!(rendered.contains("profile=none"));
    }

    #[test]
    fn diagnostic_from_normalized_reads_metadata_only() {
        let diagnostic = ImageCaptureDiagnostic::from_normalized(
            ImageSource::ArboardFallback,
            RepresentationSource::None,
            "png_without_phys",
            false,
            false,
            None,
            None,
            ColorProfileKind::None,
            false,
            17,
            9,
        );
        assert_eq!(diagnostic.image_source, ImageSource::ArboardFallback);
        assert!(!diagnostic.original_png_preserved);
        assert_eq!(diagnostic.width, 17);
        assert_eq!(diagnostic.height, 9);
    }
}
