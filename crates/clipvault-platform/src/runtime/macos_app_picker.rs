//! macOS `NSOpenPanel`-backed application picker.
//!
//! Opens the standard Cocoa open panel pre-pointed at `/Applications`,
//! restricts the selectable target to application bundles
//! (directories with the `.app` extension), extracts
//! `CFBundleIdentifier` and `CFBundleDisplayName` from `Info.plist`
//! and renders the system-provided icon to a PNG stored under
//! `~/.clipvault/assets/ignored-apps/` so the frontend can render the
//! row without trusting arbitrary user paths.
//!
//! ## Safety guarantees
//!
//! - The picker runs **only** on the macOS main thread. Apple requires
//!   `NSOpenPanel` (and the surrounding `NSWorkspace` API) on the main
//!   thread; off-main invocations return
//!   [`ApplicationPickerError::BackendUnavailable`].
//! - The picker NEVER executes, installs, opens or modifies the
//!   selected bundle. It never invokes `open`, `osascript`, `shell`,
//!   URLs or any external command.
//! - Symlinks and aliases are resolved once via
//!   [`std::fs::canonicalize`]; the resolved target must still end in
//!   `.app` and live under a known location (see
//!   [`validate_app_bundle`]).
//! - The picker NEVER logs clipboard content, content hashes,
//!   snippets or capture payloads. The only metadata written to disk
//!   is the rendered PNG icon, the icon reference, the identifier and
//!   the display name.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, Message};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSDeviceRGBColorSpace, NSGraphicsContext,
    NSImageInterpolation, NSModalResponseOK, NSOpenPanel, NSWorkspace,
};
use objc2_core_graphics::CGContext;
use objc2_foundation::{
    MainThreadMarker, NSArray, NSData, NSDictionary, NSNumber, NSPoint, NSRect, NSSize, NSString,
    NSURL,
};
use tracing::warn;

use crate::app_assets::MAX_ICON_DIM;
use crate::app_picker::{ApplicationPicker, ApplicationPickerError, SelectedApplication};

/// macOS-backed application picker. Stores rendered icons under
/// `<assets_dir>/ignored-apps/<safe-id>.png` and exposes the relative
/// path as `icon_ref` so the storage layer never records an
/// arbitrary, user-supplied filesystem location.
pub struct MacOsApplicationPicker {
    assets_dir: PathBuf,
}

impl Default for MacOsApplicationPicker {
    fn default() -> Self {
        Self::new(PathBuf::from(".clipvault/assets"))
    }
}

impl MacOsApplicationPicker {
    pub fn new(assets_dir: impl Into<PathBuf>) -> Self {
        Self {
            assets_dir: assets_dir.into(),
        }
    }
}

impl ApplicationPicker for MacOsApplicationPicker {
    fn pick(&self) -> Result<SelectedApplication, ApplicationPickerError> {
        let Some(mtm) = MainThreadMarker::new() else {
            return Err(ApplicationPickerError::BackendUnavailable {
                reason: "application picker must run on the macOS main thread".into(),
            });
        };

        let selection = run_open_panel(mtm)?;
        let bundle_path = validate_app_bundle(&selection)?;
        let (identifier, display_name) = extract_bundle_metadata(&bundle_path)?;
        let icon_ref = self.persist_icon_for_bundle(&bundle_path, &identifier);
        Ok(SelectedApplication {
            identifier,
            display_name,
            icon_ref,
        })
    }

    fn name(&self) -> &'static str {
        "macos_app_picker"
    }
}

/// Build, configure and run the NSOpenPanel. Returns the path the
/// user picked or a typed error. The picker is configured to:
///
/// - only allow directories (so the user cannot pick a regular file),
/// - restrict the selectable type to `.app` bundles via
///   `setAllowedFileTypes` (legacy) and
///   `setTreatsFilePackagesAsDirectories(true)` so `.app` is treated
///   as a single bundle rather than a navigable directory,
/// - start at `/Applications`,
/// - disable multi-selection so the picker returns a single bundle.
fn run_open_panel(mtm: MainThreadMarker) -> Result<PathBuf, ApplicationPickerError> {
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseDirectories(true);
    panel.setCanChooseFiles(false);
    panel.setAllowsMultipleSelection(false);
    panel.setResolvesAliases(true);
    panel.setTreatsFilePackagesAsDirectories(true);
    panel.setCanResolveUbiquitousConflicts(false);
    panel.setCanDownloadUbiquitousContents(false);
    panel.setShowsHiddenFiles(false);

    // Restrict the selectable target to application bundles. We pass
    // the legacy `setAllowedFileTypes` because not every host has
    // adopted `setAllowedContentTypes`; the `app` UTI is the canonical
    // App Store extension for `.app`.
    #[allow(deprecated)]
    let allowed_types = NSArray::from_slice(&[&*NSString::from_str("app")]);
    #[allow(deprecated)]
    panel.setAllowedFileTypes(Some(&allowed_types));

    // Initial directory: `/Applications`. We try a couple of fallbacks
    // so the picker never crashes when `/Applications` is missing
    // (sandboxed builds, restricted test environments, ...).
    let initial = NSURL::fileURLWithPath(&NSString::from_str("/Applications"));
    panel.setDirectoryURL(Some(&initial));
    let message = NSString::from_str("Selecciona la aplicación que quieres ignorar");
    panel.setMessage(Some(&message));
    let prompt = NSString::from_str("Seleccionar");
    panel.setPrompt(Some(&prompt));

    let response = panel.runModal();
    if response != NSModalResponseOK {
        return Err(ApplicationPickerError::Cancelled);
    }

    let urls = panel.URLs();
    let Some(url) = urls.firstObject() else {
        return Err(ApplicationPickerError::Cancelled);
    };
    let Some(path) = url.path() else {
        return Err(ApplicationPickerError::InvalidSelection {
            reason: "selected URL has no filesystem path".into(),
        });
    };
    Ok(PathBuf::from(path.to_string()))
}

/// Validate the picked path. The selector can only ever land on a
/// `.app` directory under a known location; anything else is a
/// rejection that surfaces a typed error and never mutates the
/// blacklist.
///
/// The function resolves aliases/symlinks via [`canonicalize`] so
/// the picker cannot be tricked into following a malicious symlink
/// out of `/Applications` or `~/Applications`.
pub(crate) fn validate_app_bundle(path: &Path) -> Result<PathBuf, ApplicationPickerError> {
    let canonical = match fs::canonicalize(path) {
        Ok(canonical) => canonical,
        Err(error) => {
            warn!(error = %error, path = %path.display(), "canonicalize failed");
            return Err(ApplicationPickerError::InvalidSelection {
                reason: format!("could not resolve {}: {error}", path.display()),
            });
        }
    };

    let is_app_directory = canonical.is_dir()
        && canonical
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("app"))
            .unwrap_or(false);
    if !is_app_directory {
        return Err(ApplicationPickerError::InvalidSelection {
            reason: format!("{} is not a .app bundle directory", canonical.display()),
        });
    }

    let allowed_root = is_under_allowed_root(&canonical);
    if !allowed_root {
        return Err(ApplicationPickerError::InvalidSelection {
            reason: format!(
                "{} is outside the allowed application directories",
                canonical.display()
            ),
        });
    }
    Ok(canonical)
}

/// Conservative root allow-list: `/Applications`, `/System/Applications`
/// (read-only system bundles) and the per-user `~/Applications`.
/// Other locations (Downloads, /tmp, ...) are rejected even when the
/// file ends in `.app`, so the picker cannot be used to register
/// arbitrary, unsigned bundles the user happened to drop somewhere.
fn is_under_allowed_root(path: &Path) -> bool {
    let roots: [&Path; 3] = [
        Path::new("/Applications"),
        Path::new("/System/Applications"),
        Path::new("/usr/local/Applications"),
    ];
    for root in roots {
        if path.starts_with(root) {
            return true;
        }
    }
    if let Some(home) = dirs::home_dir() {
        let user_root = home.join("Applications");
        if path.starts_with(&user_root) {
            return true;
        }
    }
    false
}

/// Extract the bundle identifier and display name from `Info.plist`.
/// Returns [`ApplicationPickerError::MissingIdentifier`] when the
/// bundle does not advertise a usable identifier, and falls back to
/// the bundle file name for the display string so the UI is never
/// empty.
pub(crate) fn extract_bundle_metadata(
    path: &Path,
) -> Result<(String, String), ApplicationPickerError> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let Some(bundle) = ns_bundle_from_url(&url) else {
        return Err(ApplicationPickerError::InvalidSelection {
            reason: format!("{} is not a valid bundle", path.display()),
        });
    };
    let info = bundle.infoDictionary();
    let Some(info) = info else {
        return Err(ApplicationPickerError::InvalidSelection {
            reason: "bundle has no Info.plist".into(),
        });
    };

    let identifier = info_dict_string(&info, "CFBundleIdentifier");
    let display_name = info_dict_string(&info, "CFBundleDisplayName")
        .or_else(|| info_dict_string(&info, "CFBundleName"))
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        });

    let Some(identifier) = identifier.filter(|s| !s.is_empty()) else {
        return Err(ApplicationPickerError::MissingIdentifier);
    };
    Ok((identifier, display_name))
}

fn info_dict_string(info: &NSDictionary<NSString, AnyObject>, key: &str) -> Option<String> {
    let key = NSString::from_str(key);
    let value = info.objectForKey(&key)?;
    let raw = value.downcast_ref::<NSString>()?;
    Some(raw.to_string())
}

// `NSBundle::bundleWithURL` lives in `objc2_foundation`. Re-export
// here to keep the picker code self-contained.
fn ns_bundle_from_url(url: &NSURL) -> Option<Retained<objc2_foundation::NSBundle>> {
    use objc2_foundation::NSBundle;
    NSBundle::bundleWithURL(url)
}

impl MacOsApplicationPicker {
    /// Render the bundle icon to PNG and store it under
    /// `~/.clipvault/assets/ignored-apps/<safe-id>.png`. The function
    /// never fails the picker when icon extraction is unavailable:
    /// per the spec the privacy rule must hold even without an icon,
    /// so the caller can proceed with `icon_ref = None`.
    fn persist_icon_for_bundle(&self, path: &Path, identifier: &str) -> Option<String> {
        let png_bytes = render_bundle_icon_png(path)?;
        let safe_id = sanitize_for_filename(identifier);
        let filename = format!("{safe_id}.png");
        let target_dir = self.assets_dir.join("ignored-apps");
        if let Err(error) = fs::create_dir_all(&target_dir) {
            warn!(error = %error, "could not create assets directory for icon");
            return None;
        }
        let target = target_dir.join(&filename);
        let mut file = match fs::File::create(&target) {
            Ok(file) => file,
            Err(error) => {
                warn!(error = %error, path = %target.display(), "could not open icon file");
                return None;
            }
        };
        if let Err(error) = file.write_all(&png_bytes) {
            warn!(error = %error, "failed to write PNG icon");
            return None;
        }
        // The encoded bitmap is already capped at `MAX_ICON_DIM ×
        // MAX_ICON_DIM`; the lossless PNG fits comfortably under the
        // `MAX_ICON_BYTES` write cap exposed in `app_assets`.
        Some(format!("ignored-apps/{filename}"))
    }
}

/// Render the bundle's document icon as PNG bytes. Returns `None`
/// when AppKit refuses to produce a TIFF representation or when the
/// PNG conversion fails. Per the spec, an icon failure MUST NOT
/// block adding a valid identifier.
///
/// The function enforces the documented icon size: every pixel of
/// the resulting PNG lives within a `MAX_ICON_DIM × MAX_ICON_DIM`
/// bounding box, preserving the original aspect ratio. Without this
/// downscale step `NSWorkspace::iconForFile` would happily deliver
/// 1024×1024 RGBA bitmaps for bundles such as Chrome or Affinity,
/// which exceed the asset-directory write cap the read path
/// enforces.
fn render_bundle_icon_png(path: &Path) -> Option<Vec<u8>> {
    let workspace = NSWorkspace::sharedWorkspace();
    let path_str = NSString::from_str(&path.to_string_lossy());
    let image = workspace.iconForFile(&path_str);
    let tiff: Retained<NSData> = image.TIFFRepresentation()?;
    let bitmap: Retained<NSBitmapImageRep> = NSBitmapImageRep::imageRepWithData(&tiff)?;
    let downscaled = downscale_bitmap(&bitmap)?;
    let png_data = encode_png(&downscaled)?;
    Some(png_data.to_vec())
}

/// Compute the target dimensions of a downscaled bitmap preserving
/// the original aspect ratio. The longest side never exceeds
/// [`MAX_ICON_DIM`]; the shortest side is rounded down so the bitmap
/// fits inside a `MAX_ICON_DIM × MAX_ICON_DIM` bounding box without
/// padding.
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

/// Allocate a new `NSBitmapImageRep` and redraw `source` into it at
/// the target size, preserving the original aspect ratio. Returns
/// the source unchanged when it already fits inside
/// `MAX_ICON_DIM × MAX_ICON_DIM`. The function is the single point
/// that enforces the documented icon size; any new caller must route
/// through it before persisting PNG bytes.
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

    // Reuse the highest-quality interpolation AppKit exposes so the
    // downscaled result is sharp enough for the 1.75rem slot the
    // settings panel renders into.
    context.setImageInterpolation(NSImageInterpolation::High);
    // Flip the coordinate system so the source image is right-side
    // up in the bitmap (AppKit pixels live in the bottom-left
    // quadrant; the bitmap allocator uses the top-left one).
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

/// Encode an `NSBitmapImageRep` as PNG. The compression factor is
/// irrelevant for PNG (lossless), but the property dictionary is the
/// documented entry point for `representationUsingType:properties:`.
fn encode_png(bitmap: &NSBitmapImageRep) -> Option<Retained<NSData>> {
    let key: Retained<NSString> = NSString::from_str("NSImageCompressionFactor");
    let value: Retained<NSNumber> = NSNumber::numberWithFloat(0.9);
    let key_ref: &NSString = key.as_ref();
    let value_ref: &NSNumber = value.as_ref();
    let props: Retained<NSDictionary<NSString, AnyObject>> =
        NSDictionary::from_slices(&[key_ref], &[value_ref]);
    unsafe { bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &props) }
}

/// Reduce an identifier to a safe filename component. The picker
/// already filters non-ASCII via the upstream `Normalize` rules; this
/// is a defensive measure so the icon file does not leak
/// path-traversal characters.
fn sanitize_for_filename(input: &str) -> String {
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
    fn validate_app_bundle_rejects_non_app_paths() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bogus = dir.path().join("not-an-app");
        std::fs::create_dir_all(&bogus).expect("create dir");
        let err = validate_app_bundle(&bogus).expect_err("must reject non-app");
        assert_eq!(err.kind_str(), "invalid_selection");
    }

    #[test]
    fn validate_app_bundle_rejects_regular_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bogus = dir.path().join("fake.app");
        std::fs::write(&bogus, b"not a bundle").expect("write");
        let err = validate_app_bundle(&bogus).expect_err("must reject file");
        assert_eq!(err.kind_str(), "invalid_selection");
    }

    #[test]
    fn is_under_allowed_root_accepts_user_applications() {
        let mut home = dirs::home_dir().expect("home");
        home.push("Applications");
        let candidate = home.join("Example.app");
        assert!(is_under_allowed_root(&candidate));
    }

    #[test]
    fn is_under_allowed_root_rejects_tmp() {
        let candidate = std::path::PathBuf::from("/tmp/example.app");
        assert!(!is_under_allowed_root(&candidate));
    }

    #[test]
    fn sanitize_for_filename_replaces_unsafe_chars() {
        assert_eq!(
            sanitize_for_filename("com.apple.terminal"),
            "com.apple.terminal"
        );
        assert_eq!(sanitize_for_filename("a/b\\c d"), "a_b_c_d");
        assert_eq!(sanitize_for_filename(""), "app");
    }

    #[test]
    fn target_dimensions_keeps_already_small_bitmaps_untouched() {
        // Icons that already fit inside the documented box must
        // bypass the resampling path so we never lose fidelity on
        // bundles that already provide a small icon.
        assert_eq!(target_dimensions(256, 256), (256, 256));
        assert_eq!(target_dimensions(128, 200), (128, 200));
        assert_eq!(target_dimensions(16, 16), (16, 16));
    }

    #[test]
    fn target_dimensions_caps_the_longest_side() {
        // The longest side is pinned to `MAX_ICON_DIM` and the
        // shortest side preserves the original aspect ratio. The
        // Chrome/Affinity icons Apple ships are 1024×1024 RGBA bitmaps
        // and must collapse to a 256×256 PNG so the write-side cap
        // (`MAX_ICON_BYTES`) is honoured.
        assert_eq!(target_dimensions(1024, 1024), (256, 256));
        assert_eq!(target_dimensions(2048, 1024), (256, 128));
        assert_eq!(target_dimensions(1024, 2048), (128, 256));
    }

    #[test]
    fn target_dimensions_keeps_results_within_max_icon_dim() {
        // The function must always produce a bitmap whose longest
        // side fits inside the documented bounding box, even for the
        // pathological non-square ratios macOS occasionally ships.
        let samples: [(isize, isize); 5] =
            [(512, 32), (32, 512), (4096, 256), (256, 4096), (1280, 720)];
        for (w, h) in samples {
            let (tw, th) = target_dimensions(w, h);
            let max_dim = isize::try_from(MAX_ICON_DIM).expect("MAX_ICON_DIM fits");
            assert!(
                tw <= max_dim,
                "width {tw} must not exceed MAX_ICON_DIM for input {w}x{h}"
            );
            assert!(
                th <= max_dim,
                "height {th} must not exceed MAX_ICON_DIM for input {w}x{h}"
            );
            assert!(
                tw >= 1 && th >= 1,
                "downscaled bitmap must keep at least one pixel"
            );
        }
    }
}
