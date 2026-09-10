//! Application metadata provider used by the `history-card-layout`
//! capability.
//!
//! The capture pipeline calls this provider for every permitted text
//! capture to resolve a user-visible name and a controlled icon
//! reference for the source application. The provider lives behind a
//! trait so the core layer can be exercised without a real platform
//! adapter and so the macOS / Linux implementations can plug into the
//! same shell bootstrap. The interface is deliberately small:
//!
//! - Given a normalised application identifier, return a
//!   [`ApplicationMetadata`] describing the user-visible name and an
//!   opaque, locally-controlled icon reference (or `None` when the
//!   metadata could not be resolved);
//! - The icon reference MUST live under
//!   [`crate::app_assets::APPLICATION_ICONS_DIR`] so the existing
//!   safe icon bridge can serve the bytes to the webview without
//!   re-validating the path;
//! - Failures are non-fatal: the capture pipeline falls back to a
//!   generic application icon and skips the metadata enrichment.

use std::fmt;
use std::path::Path;

use thiserror::Error;

/// Sub-directory under `<data_dir>/assets` where the provider
/// persists icons. Re-exported from [`crate::app_assets`] so the
/// provider can reference it without depending on the asset module
/// layout directly.
pub use crate::app_assets::APPLICATION_ICONS_DIR;

/// Metadata returned by [`ApplicationMetadataProvider::lookup`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationMetadata {
    /// User-visible application name (for example "Terminal" or
    /// "Firefox"). The provider MUST return a non-empty string.
    pub display_name: String,
    /// Opaque, locally-controlled icon reference the frontend can
    /// pass back to the icon bridge. The reference is relative to
    /// `<data_dir>/assets/` and starts with `application-icons/`.
    pub icon_ref: Option<String>,
}

/// Errors the provider can surface. Every variant is non-fatal: the
/// caller falls back to a generic metadata shape and continues the
/// capture flow.
#[derive(Debug, Clone, Error)]
pub enum ApplicationMetadataError {
    /// The provider backend cannot run on the current session (for
    /// example no `NSWorkspace` access on macOS or a Wayland
    /// compositor without an active-app protocol on Linux).
    #[error("application metadata lookup is unavailable on this platform")]
    Unavailable,
    /// The backend failed for a non-fatal reason. The capture
    /// pipeline keeps running; the icon simply stays absent.
    #[error("application metadata lookup failed: {details}")]
    Backend { details: String },
}

/// Strategy the provider used to match the most recent
/// [`ApplicationMetadataProvider::lookup`] call. The values are the
/// stable, snake_case identifiers the
/// `linux-source-app-metadata` capture diagnostic reads so the user
/// can confirm whether the resolver hit
/// [`MatchStrategy::StartupWmClass`],
/// [`MatchStrategy::XGnomeWmClass`] or the file-name fallback.
///
/// Renaming a variant is a breaking change for the diagnostic; tests
/// pin the strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchStrategy {
    /// No lookup has been performed yet, or the lookup produced no
    /// match. The default for adapters that do not track the field.
    None,
    /// The entry matched because its `StartupWMClass=` key equals the
    /// identifier.
    StartupWmClass,
    /// The entry matched because its `X-GNOME-WMClass=` key equals the
    /// identifier.
    XGnomeWmClass,
    /// The entry matched because the `.desktop` file basename (without
    /// the extension) equals the identifier.
    DesktopFilename,
}

impl MatchStrategy {
    /// Stable snake_case identifier consumed by the diagnostic sink and
    /// the front-end label. Renaming the strings is a breaking change.
    pub fn as_str(self) -> &'static str {
        match self {
            MatchStrategy::None => "none",
            MatchStrategy::StartupWmClass => "startup_wm_class",
            MatchStrategy::XGnomeWmClass => "x_gnome_wm_class",
            MatchStrategy::DesktopFilename => "desktop_filename",
        }
    }
}

/// Metadata the icon writer reported on the most recent successful
/// lookup. The fields are metadata-only — never the icon bytes or an
/// absolute path. The default is the "nothing happened yet" snapshot
/// adapters that do not track icons return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IconDiagnostics {
    /// `true` when the matched `.desktop` entry declared an `Icon=`
    /// key the resolver could interpret (either an absolute PNG path
    /// under an allowed root or a theme-name lookup).
    pub declared: bool,
    /// Source format the resolver identified for the matched icon.
    /// `Unknown` covers formats the provider refuses to handle (XPM,
    /// ICO, raw bitmaps). The diagnostic never reports the actual
    /// path or filename.
    pub kind: IconSourceKind,
    /// `true` when the resolver produced a canonical file path on
    /// disk. `false` when the icon was missing, escaped the allowed
    /// roots or failed the format check.
    pub resolved: bool,
    /// `true` when the resolver attempted to rasterize a SVG source
    /// into a PNG payload. Always `false` for PNG and pixmap
    /// sources — the value only flips when the resolver picked an
    /// `.svg` candidate from the theme or `pixmaps/` namespace.
    pub rasterization_attempted: bool,
    /// `true` when the SVG rasterizer produced a valid PNG payload
    /// the writer accepted. Only meaningful when
    /// `rasterization_attempted` is `true`; stays `false` for every
    /// PNG / pixmap source.
    pub rasterization_succeeded: bool,
    /// `true` when the PNG signature the resolver expected matched
    /// the bytes on disk. Mirrors the
    /// `read_validated_png` validator the Linux provider runs.
    pub png_validated: bool,
    /// `true` when the writer successfully renamed a temporary file
    /// over the destination.
    pub persisted: bool,
    /// Byte length of the validated PNG payload. `None` when no
    /// payload reached the writer.
    pub bytes: Option<usize>,
    /// IHDR dimensions the validator parsed from the PNG header.
    /// `None` when the payload never reached the validator.
    pub dimensions: Option<(u32, u32)>,
    /// Stable identifier the resolver associates with the most
    /// recent failure. The string never carries a path, the icon
    /// payload or any user-supplied text; the variants the capture
    /// diagnostic enumerates cover every documented failure mode.
    pub failure_kind: IconFailureKind,
}

/// Source format the resolver detected on the last successful icon
/// lookup. The variants are part of the diagnostic contract
/// `linux-source-app-metadata` pins: every test reads back the
/// `as_str` value to confirm the resolver landed on the right
/// source. Renaming a variant is a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconSourceKind {
    /// The resolver has not picked a source yet (default).
    #[default]
    None,
    /// The resolver identified a PNG file (`<theme>/<size>x<size>/apps/<name>.png`
    /// or a `.png` pixmap).
    Png,
    /// The resolver identified an SVG file that the rasterizer
    /// converted to a PNG payload before validation.
    Svg,
    /// The resolver identified a file living under a `pixmaps/`
    /// namespace (typically a freedesktop legacy layout).
    Pixmap,
    /// The resolver encountered an `Icon=` value that does not match
    /// any supported format. The diagnostic keeps the variant
    /// generic so a tampered `.desktop` file cannot smuggle a
    /// filename or extension into the logs.
    Unknown,
}

impl IconSourceKind {
    /// Stable snake_case identifier the capture diagnostic consumes.
    pub fn as_str(self) -> &'static str {
        match self {
            IconSourceKind::None => "none",
            IconSourceKind::Png => "png",
            IconSourceKind::Svg => "svg",
            IconSourceKind::Pixmap => "pixmap",
            IconSourceKind::Unknown => "unknown",
        }
    }
}

/// Stable identifier the icon resolver attaches to the most recent
/// failure. The string never carries a path, an icon payload or any
/// user-supplied text — every variant is a typed category the
/// `linux-source-app-metadata` capture diagnostic surfaces verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconFailureKind {
    /// The resolver has not surfaced a failure yet (default).
    #[default]
    None,
    /// The `.desktop` file did not declare an `Icon=` key.
    NotDeclared,
    /// The resolver could not find any candidate that lived under
    /// an allowed XDG root.
    NotFound,
    /// The candidate path the resolver identified lives outside the
    /// allowed XDG roots. Symlinks that escape the root land here.
    OutOfRoots,
    /// The candidate bytes are not a valid PNG payload.
    InvalidPng,
    /// The candidate bytes are not a valid SVG payload.
    InvalidSvg,
    /// The SVG was valid but exceeded one of the safety caps
    /// (`MAX_SVG_BYTES`, source dimensions, target dimensions).
    SvgRejected,
    /// The SVG rasterizer could not produce a PNG payload (parse
    /// failure, rasterizer internal failure or PNG encode failure).
    RasterizationFailed,
    /// The PNG writer rejected the payload (invalid signature,
    /// dimensions above the cap, IO error during the atomic
    /// rename).
    WriteError,
}

impl IconFailureKind {
    /// Stable snake_case identifier the capture diagnostic consumes.
    pub fn as_str(self) -> &'static str {
        match self {
            IconFailureKind::None => "none",
            IconFailureKind::NotDeclared => "not_declared",
            IconFailureKind::NotFound => "not_found",
            IconFailureKind::OutOfRoots => "out_of_roots",
            IconFailureKind::InvalidPng => "invalid_png",
            IconFailureKind::InvalidSvg => "invalid_svg",
            IconFailureKind::SvgRejected => "svg_rejected",
            IconFailureKind::RasterizationFailed => "rasterization_failed",
            IconFailureKind::WriteError => "write_error",
        }
    }
}

impl ApplicationMetadataError {
    pub fn backend(details: impl fmt::Display) -> Self {
        ApplicationMetadataError::Backend {
            details: details.to_string(),
        }
    }
}

/// Stable, platform-agnostic interface every implementation must
/// satisfy. Cheap to clone via `Arc` so the capture pipeline can keep
/// a handle in [`clipvault_core::TextHistoryService`].
pub trait ApplicationMetadataProvider: Send + Sync {
    /// Resolve `identifier` to metadata the history card can render.
    ///
    /// The implementation MUST:
    /// - return `Ok(Some(...))` when the platform can answer;
    /// - return `Ok(None)` when the platform recognises the
    ///   identifier but has no metadata to share (for example the
    ///   identifier points to a CLI that ships no bundle);
    /// - return an [`ApplicationMetadataError::Unavailable`] variant
    ///   when the backend cannot run on the current session;
    /// - never panic.
    fn lookup(
        &self,
        identifier: &str,
    ) -> Result<Option<ApplicationMetadata>, ApplicationMetadataError>;

    /// Stable identifier for diagnostics.
    fn name(&self) -> &'static str;

    /// Strategy the implementation used to match the most recent
    /// [`Self::lookup`] call. Defaults to [`MatchStrategy::None`] for
    /// adapters that do not track the field — every production
    /// provider overrides this helper so the
    /// `linux-source-app-metadata` capture diagnostic can confirm the
    /// `_NET_ACTIVE_WINDOW → WM_CLASS → source identifier` chain on
    /// every iteration. Reading the value MUST NOT mutate provider
    /// state.
    fn last_match_strategy(&self) -> MatchStrategy {
        MatchStrategy::None
    }

    /// Metadata the icon writer reported on the most recent
    /// [`Self::lookup`] call. Defaults to the empty snapshot for
    /// adapters that do not track icons (the macOS picker reads the
    /// field so the diagnostic can confirm the persistence step ran).
    fn last_icon_diagnostics(&self) -> IconDiagnostics {
        IconDiagnostics::default()
    }
}

/// No-op provider. The capture pipeline installs this provider when
/// no real adapter is available so the rest of the core layer can
/// rely on a populated [`Arc<dyn ApplicationMetadataProvider>`].
#[derive(Debug, Default, Clone)]
pub struct NoopApplicationMetadataProvider;

impl ApplicationMetadataProvider for NoopApplicationMetadataProvider {
    fn lookup(
        &self,
        _identifier: &str,
    ) -> Result<Option<ApplicationMetadata>, ApplicationMetadataError> {
        Ok(None)
    }

    fn name(&self) -> &'static str {
        "noop"
    }

    fn last_match_strategy(&self) -> MatchStrategy {
        MatchStrategy::None
    }
}

/// Compute the icon reference the storage layer should persist for a
/// given source identifier. The reference lives under
/// `<data_dir>/assets/application-icons/<safe-id>.png` so the icon
/// bridge can serve the bytes through the existing
/// `application-icons/` namespace validator.
///
/// The function performs no filesystem work: it just builds the
/// deterministic reference. Persistence is the caller's
/// responsibility so the provider can stay synchronous and testable.
pub fn icon_ref_for(identifier: &str) -> String {
    let safe_id = sanitize_identifier(identifier);
    format!("{APPLICATION_ICONS_DIR}/{safe_id}.png")
}

/// Reduce an identifier to a safe filename component. Mirrors the
/// sanitiser the macOS picker uses so the two namespaces stay
/// visually consistent on disk.
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

/// Resolve a relative `icon_ref` against `<data_dir>/assets/`. The
/// helper is the read-side counterpart to [`icon_ref_for`]; the
/// caller passes a ref the provider produced earlier and the helper
/// returns the absolute path inside the asset directory.
///
/// The path is *not* canonicalised here: callers that need the
/// traversal / symlink guarantees must route through
/// [`crate::app_assets::resolve_source_app_icon_path`].
pub fn path_for_icon_ref(icon_ref: &str, data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(crate::app_assets::ASSETS_DIR).join(icon_ref)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn icon_ref_for_returns_application_icons_namespace() {
        let reference = icon_ref_for("com.apple.Terminal");
        assert!(reference.starts_with("application-icons/"));
        assert!(reference.ends_with(".png"));
    }

    #[test]
    fn icon_ref_for_sanitises_unsafe_characters() {
        let reference = icon_ref_for("a/b\\c d");
        // All non-alphanumeric characters become `_`, so `a/b\c d`
        // normalises to `a_b_c_d`.
        assert_eq!(reference, "application-icons/a_b_c_d.png");
    }

    #[test]
    fn icon_ref_for_falls_back_to_app_when_input_is_empty() {
        let reference = icon_ref_for("");
        assert_eq!(reference, "application-icons/app.png");
    }

    #[test]
    fn path_for_icon_ref_joins_under_assets_dir() {
        let data_dir = PathBuf::from("/data");
        let resolved = path_for_icon_ref("application-icons/x.png", &data_dir);
        assert_eq!(
            resolved,
            PathBuf::from("/data/assets/application-icons/x.png")
        );
    }

    #[test]
    fn noop_provider_returns_none_for_any_identifier() {
        let provider = NoopApplicationMetadataProvider;
        let result = provider.lookup("anything").expect("ok");
        assert_eq!(result, None);
        assert_eq!(provider.name(), "noop");
    }

    #[test]
    fn icon_source_kind_strings_are_stable() {
        // The capture diagnostic renders the snake_case identifier
        // verbatim; renaming any string is a breaking change for
        // `linux-source-app-metadata`.
        assert_eq!(IconSourceKind::None.as_str(), "none");
        assert_eq!(IconSourceKind::Png.as_str(), "png");
        assert_eq!(IconSourceKind::Svg.as_str(), "svg");
        assert_eq!(IconSourceKind::Pixmap.as_str(), "pixmap");
        assert_eq!(IconSourceKind::Unknown.as_str(), "unknown");
    }

    #[test]
    fn icon_failure_kind_strings_are_stable() {
        assert_eq!(IconFailureKind::None.as_str(), "none");
        assert_eq!(IconFailureKind::NotDeclared.as_str(), "not_declared");
        assert_eq!(IconFailureKind::NotFound.as_str(), "not_found");
        assert_eq!(IconFailureKind::OutOfRoots.as_str(), "out_of_roots");
        assert_eq!(IconFailureKind::InvalidPng.as_str(), "invalid_png");
        assert_eq!(IconFailureKind::InvalidSvg.as_str(), "invalid_svg");
        assert_eq!(IconFailureKind::SvgRejected.as_str(), "svg_rejected");
        assert_eq!(
            IconFailureKind::RasterizationFailed.as_str(),
            "rasterization_failed"
        );
        assert_eq!(IconFailureKind::WriteError.as_str(), "write_error");
    }

    #[test]
    fn icon_diagnostics_default_is_empty_snapshot() {
        // The default snapshot the bootstrap installs before any
        // lookup ran MUST carry every field the diagnostic sinks
        // read; missing fields break the JSON shape the user
        // already relies on.
        let diagnostics = IconDiagnostics::default();
        assert!(!diagnostics.declared);
        assert_eq!(diagnostics.kind, IconSourceKind::None);
        assert!(!diagnostics.resolved);
        assert!(!diagnostics.rasterization_attempted);
        assert!(!diagnostics.rasterization_succeeded);
        assert!(!diagnostics.png_validated);
        assert!(!diagnostics.persisted);
        assert!(diagnostics.bytes.is_none());
        assert!(diagnostics.dimensions.is_none());
        assert_eq!(diagnostics.failure_kind, IconFailureKind::None);
    }
}
