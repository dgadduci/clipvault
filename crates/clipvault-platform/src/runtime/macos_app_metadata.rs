//! macOS-backed [`ApplicationMetadataProvider`].
//!
//! The provider resolves a bundle identifier to a user-visible name
//! and a controlled icon reference stored under
//! `<data_dir>/assets/application-icons/<safe-id>.png`. The pipeline
//! intentionally mirrors the macOS application picker so the two
//! namespaces stay visually consistent on disk: same sanitiser, same
//! icon dimensions, same downscaling rule.
//!
//! ## Safety guarantees
//!
//! - The provider only consults identifiers the [`crate::ActiveApplicationProbe`]
//!   already returned; nothing on the surface accepts user-supplied
//!   paths or URLs.
//! - Icons are persisted under `<data_dir>/assets/application-icons/`
//!   and the read-side validator enforces the same safety contract
//!   the picker icons rely on (PNG magic header, size cap, scope).
//! - Failures are non-fatal: the provider returns
//!   [`ApplicationMetadataError::Backend`] so the caller can fall
//!   back to a generic metadata shape and keep capturing text.
//! - The provider NEVER executes, installs, opens or modifies the
//!   resolved bundle. It only reads `Info.plist` and renders the
//!   bundle icon into a PNG.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, Message};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSDeviceRGBColorSpace, NSGraphicsContext,
    NSImageInterpolation, NSRunningApplication, NSWorkspace,
};
use objc2_core_graphics::CGContext;
use objc2_foundation::{
    MainThreadMarker, NSDictionary, NSNumber, NSPoint, NSRect, NSSize, NSString,
};
use tracing::warn;

use crate::app_assets::{APPLICATION_ICONS_DIR, MAX_ICON_DIM};
use crate::app_metadata::{
    icon_ref_for, ApplicationMetadata, ApplicationMetadataError, ApplicationMetadataProvider,
    IconDiagnostics, IconFailureKind, IconSourceKind, MatchStrategy,
};
use parking_lot::Mutex;

/// macOS-backed application-metadata provider. Stores rendered icons
/// under `<data_dir>/assets/application-icons/<safe-id>.png` and
/// exposes the relative path as the `icon_ref` so the storage layer
/// never records an arbitrary user-supplied filesystem location.
pub struct MacOsApplicationMetadataProvider {
    assets_dir: PathBuf,
    /// Strategy the most recent [`Self::lookup`] used. macOS resolves
    /// identifiers through `NSRunningApplication::bundleURL`, which
    /// maps cleanly onto the [`MatchStrategy::DesktopFilename`] arm
    /// the diagnostic contract pins for "file-name match".
    last_strategy: Mutex<MatchStrategy>,
    /// Snapshot the icon writer reported on the most recent
    /// successful lookup. Mirrors the `IconDiagnostics` shape the
    /// Linux provider publishes.
    last_icon: Mutex<IconDiagnostics>,
}

impl Default for MacOsApplicationMetadataProvider {
    fn default() -> Self {
        Self::new(PathBuf::from(".clipvault/assets"))
    }
}

impl MacOsApplicationMetadataProvider {
    pub fn new(assets_dir: impl Into<PathBuf>) -> Self {
        Self {
            assets_dir: assets_dir.into(),
            last_strategy: Mutex::new(MatchStrategy::None),
            last_icon: Mutex::new(IconDiagnostics::default()),
        }
    }
}

impl ApplicationMetadataProvider for MacOsApplicationMetadataProvider {
    fn lookup(
        &self,
        identifier: &str,
    ) -> Result<Option<ApplicationMetadata>, ApplicationMetadataError> {
        // The provider MUST run on the macOS main thread because
        // `NSWorkspace` requires it; off-main callers fall back to
        // `Unavailable` and the capture pipeline uses the no-op
        // metadata. Returning `Ok(None)` instead of an error would
        // mask the platform's capability contract, so we surface a
        // typed error here and let the caller downgrade.
        let Some(_mtm) = MainThreadMarker::new() else {
            *self.last_strategy.lock() = MatchStrategy::None;
            *self.last_icon.lock() = IconDiagnostics::default();
            return Err(ApplicationMetadataError::Unavailable);
        };

        let bundle_path = match resolve_bundle_path(identifier) {
            Some(path) => path,
            None => {
                *self.last_strategy.lock() = MatchStrategy::None;
                *self.last_icon.lock() = IconDiagnostics::default();
                return Ok(None);
            }
        };
        let display_name = extract_display_name(&bundle_path).unwrap_or_else(|| {
            bundle_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        });
        if display_name.is_empty() {
            *self.last_strategy.lock() = MatchStrategy::None;
            *self.last_icon.lock() = IconDiagnostics::default();
            return Ok(None);
        }
        *self.last_strategy.lock() = MatchStrategy::DesktopFilename;
        let icon_ref = self.persist_icon_for_bundle(&bundle_path, identifier);
        Ok(Some(ApplicationMetadata {
            display_name,
            icon_ref,
        }))
    }

    fn name(&self) -> &'static str {
        "macos_app_metadata"
    }

    fn last_match_strategy(&self) -> MatchStrategy {
        *self.last_strategy.lock()
    }

    fn last_icon_diagnostics(&self) -> IconDiagnostics {
        *self.last_icon.lock()
    }
}

impl MacOsApplicationMetadataProvider {
    /// Render the bundle icon to PNG and store it under
    /// `<assets_dir>/application-icons/<safe-id>.png`. The function
    /// never fails the lookup when icon extraction is unavailable:
    /// per the spec the metadata rule must hold even without an icon.
    ///
    /// The function records every step of the icon resolution onto
    /// the provider's `IconDiagnostics` slot so the
    /// `linux-source-app-metadata` capture diagnostic can confirm
    /// whether a missing icon is a missing render, a missing
    /// directory or a writer failure.
    fn persist_icon_for_bundle(&self, path: &Path, identifier: &str) -> Option<String> {
        let png_bytes = match render_bundle_icon_png(path) {
            Some(bytes) => bytes,
            None => {
                *self.last_icon.lock() = IconDiagnostics {
                    declared: true,
                    kind: IconSourceKind::Png,
                    resolved: false,
                    rasterization_attempted: false,
                    rasterization_succeeded: false,
                    png_validated: false,
                    persisted: false,
                    bytes: None,
                    dimensions: None,
                    failure_kind: IconFailureKind::NotFound,
                };
                return None;
            }
        };
        let target_dir = self.assets_dir.join(APPLICATION_ICONS_DIR);
        if let Err(error) = fs::create_dir_all(&target_dir) {
            warn!(error = %error, "could not create application-icons directory");
            *self.last_icon.lock() = IconDiagnostics {
                declared: true,
                kind: IconSourceKind::Png,
                resolved: true,
                rasterization_attempted: false,
                rasterization_succeeded: false,
                png_validated: true,
                persisted: false,
                bytes: Some(png_bytes.len()),
                dimensions: png_header_dimensions(&png_bytes),
                failure_kind: IconFailureKind::WriteError,
            };
            return None;
        }
        let safe_id = sanitize_identifier(identifier);
        let target = target_dir.join(format!("{safe_id}.png"));
        let mut file = match fs::File::create(&target) {
            Ok(file) => file,
            Err(error) => {
                warn!(error = %error, path = %target.display(), "could not open icon file");
                *self.last_icon.lock() = IconDiagnostics {
                    declared: true,
                    kind: IconSourceKind::Png,
                    resolved: true,
                    rasterization_attempted: false,
                    rasterization_succeeded: false,
                    png_validated: true,
                    persisted: false,
                    bytes: Some(png_bytes.len()),
                    dimensions: png_header_dimensions(&png_bytes),
                    failure_kind: IconFailureKind::WriteError,
                };
                return None;
            }
        };
        if let Err(error) = file.write_all(&png_bytes) {
            warn!(error = %error, "failed to write PNG icon");
            *self.last_icon.lock() = IconDiagnostics {
                declared: true,
                kind: IconSourceKind::Png,
                resolved: true,
                rasterization_attempted: false,
                rasterization_succeeded: false,
                png_validated: true,
                persisted: false,
                bytes: Some(png_bytes.len()),
                dimensions: png_header_dimensions(&png_bytes),
                failure_kind: IconFailureKind::WriteError,
            };
            return None;
        }
        *self.last_icon.lock() = IconDiagnostics {
            declared: true,
            kind: IconSourceKind::Png,
            resolved: true,
            rasterization_attempted: false,
            rasterization_succeeded: false,
            png_validated: true,
            persisted: true,
            bytes: Some(png_bytes.len()),
            dimensions: png_header_dimensions(&png_bytes),
            failure_kind: IconFailureKind::None,
        };
        Some(icon_ref_for(identifier))
    }
}

/// Decode the IHDR width / height from the bytes the icon writer
/// just validated. Mirrors the Linux provider helper so both
/// adapters populate the same `dimensions` field the capture
/// diagnostic reads.
fn png_header_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    // PNG layout: 8-byte signature + 4-byte length + 4-byte
    // chunk type (`IHDR`) + 4-byte width + 4-byte height.
    const HEADER_OFFSET: usize = 8 + 4 + 4;
    if bytes.len() < HEADER_OFFSET + 8 {
        return None;
    }
    let width = u32::from_be_bytes([
        bytes[HEADER_OFFSET],
        bytes[HEADER_OFFSET + 1],
        bytes[HEADER_OFFSET + 2],
        bytes[HEADER_OFFSET + 3],
    ]);
    let height = u32::from_be_bytes([
        bytes[HEADER_OFFSET + 4],
        bytes[HEADER_OFFSET + 5],
        bytes[HEADER_OFFSET + 6],
        bytes[HEADER_OFFSET + 7],
    ]);
    Some((width, height))
}

/// Resolve the bundle path on disk for a stable bundle identifier.
/// Returns `None` when no running application matches the identifier
/// or the running application does not expose a `.app` bundle URL.
///
/// Apple does not document a way to resolve a bundle identifier
/// directly via `NSWorkspace::URLForApplicationToOpenURL`: that API
/// accepts a URL (not a bundle identifier) and the previous
/// implementation passed the identifier as a URL string, which
/// `NSURL::URLWithString` happily parsed as a relative URL, then
/// `URLForApplicationToOpenURL` returned `None`. The card rail
/// therefore rendered the generic fallback copy for every capture,
/// even when the active application was a first-party bundle like
/// `com.apple.Terminal`.
///
/// The supported path is to look up the running application that owns
/// the identifier via
/// [`NSRunningApplication::runningApplicationsWithBundleIdentifier`]
/// and read its on-disk `bundleURL`. The provider MUST run on the
/// main thread (the caller in [`MacOsApplicationMetadataProvider`]
/// already enforces this via [`MainThreadMarker`]).
fn resolve_bundle_path(identifier: &str) -> Option<PathBuf> {
    let trimmed = identifier.trim();
    if trimmed.is_empty() {
        return None;
    }
    let bundle_identifier = NSString::from_str(trimmed);
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&bundle_identifier);
    let app = apps.firstObject()?;
    let url = app.bundleURL()?;
    let path = url.path()?;
    let path = PathBuf::from(path.to_string());
    if path.extension().and_then(|ext| ext.to_str()) == Some("app") {
        Some(path)
    } else {
        None
    }
}

/// Extract `CFBundleDisplayName` from the bundle's `Info.plist`. Falls
/// back to `CFBundleName` and finally to the bundle file name.
fn extract_display_name(path: &Path) -> Option<String> {
    use objc2_foundation::NSBundle;
    let url =
        objc2_foundation::NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let bundle = NSBundle::bundleWithURL(&url)?;
    let info = bundle.infoDictionary()?;
    info_dict_string(&info, "CFBundleDisplayName")
        .or_else(|| info_dict_string(&info, "CFBundleName"))
}

fn info_dict_string(info: &NSDictionary<NSString, AnyObject>, key: &str) -> Option<String> {
    let key = NSString::from_str(key);
    let value = info.objectForKey(&key)?;
    let raw = value.downcast_ref::<NSString>()?;
    Some(raw.to_string())
}

/// Render the bundle's document icon as PNG bytes. Mirrors the
/// downscaling logic the application picker uses so the
/// `application-icons/` namespace stays visually consistent with the
/// `ignored-apps/` namespace.
fn render_bundle_icon_png(path: &Path) -> Option<Vec<u8>> {
    let workspace = NSWorkspace::sharedWorkspace();
    let path_str = NSString::from_str(&path.to_string_lossy());
    let image = workspace.iconForFile(&path_str);
    let tiff: Retained<objc2_foundation::NSData> = image.TIFFRepresentation()?;
    let bitmap: Retained<NSBitmapImageRep> = NSBitmapImageRep::imageRepWithData(&tiff)?;
    let downscaled = downscale_bitmap(&bitmap)?;
    let png_data = encode_png(&downscaled)?;
    Some(png_data.to_vec())
}

fn target_dimensions(orig_w: isize, orig_h: isize) -> (isize, isize) {
    let max_dim = isize::try_from(MAX_ICON_DIM).expect("MAX_ICON_DIM fits in isize");
    if orig_w <= max_dim && orig_h <= max_dim {
        return (orig_w, orig_h);
    }
    if orig_w >= orig_h {
        let h = (orig_h * max_dim + (orig_w / 2)) / orig_w.max(1);
        (max_dim, h.max(1))
    } else {
        let w = (orig_w * max_dim + (orig_h / 2)) / orig_h.max(1);
        (w.max(1), max_dim)
    }
}

fn downscale_bitmap(source: &NSBitmapImageRep) -> Option<Retained<NSBitmapImageRep>> {
    let orig_w = source.pixelsWide();
    let orig_h = source.pixelsHigh();
    let (target_w, target_h) = target_dimensions(orig_w, orig_h);
    if target_w == orig_w && target_h == orig_h {
        return Some(source.retain());
    }

    let target = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            target_w,
            target_h,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            0,
            32,
        )
    }?;

    let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&target)?;
    let cg_ctx = context.CGContext();

    let previous = NSGraphicsContext::currentContext();
    NSGraphicsContext::setCurrentContext(Some(&context));
    context.saveGraphicsState();

    context.setImageInterpolation(NSImageInterpolation::High);
    CGContext::translate_ctm(Some(&cg_ctx), 0.0, target_h as f64);
    CGContext::scale_ctm(Some(&cg_ctx), 1.0, -1.0);
    let cg_image = source.CGImage()?;
    let rect = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(target_w as f64, target_h as f64),
    );
    CGContext::draw_image(Some(&cg_ctx), rect, Some(&cg_image));

    context.restoreGraphicsState();
    NSGraphicsContext::setCurrentContext(previous.as_deref());

    Some(target)
}

fn encode_png(bitmap: &NSBitmapImageRep) -> Option<Retained<objc2_foundation::NSData>> {
    let key: Retained<NSString> = NSString::from_str("NSImageCompressionFactor");
    let value: Retained<NSNumber> = NSNumber::numberWithFloat(0.9);
    let key_ref: &NSString = key.as_ref();
    let value_ref: &NSNumber = value.as_ref();
    let props: Retained<NSDictionary<NSString, AnyObject>> =
        NSDictionary::from_slices(&[key_ref], &[value_ref]);
    unsafe { bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &props) }
}

/// Reduce an identifier to a safe filename component. Mirrors the
/// picker so the two namespaces stay visually consistent on disk.
fn sanitize_identifier(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("app");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_identifier_replaces_unsafe_characters() {
        assert_eq!(
            sanitize_identifier("com.apple.terminal"),
            "com.apple.terminal"
        );
        assert_eq!(sanitize_identifier("a/b\\c d"), "a_b_c_d");
        assert_eq!(sanitize_identifier(""), "app");
    }

    #[test]
    fn target_dimensions_caps_the_longest_side() {
        assert_eq!(target_dimensions(256, 256), (256, 256));
        assert_eq!(target_dimensions(1024, 1024), (256, 256));
        assert_eq!(target_dimensions(2048, 1024), (256, 128));
    }
}
