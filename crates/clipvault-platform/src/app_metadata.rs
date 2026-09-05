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
}
