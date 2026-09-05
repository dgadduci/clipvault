//! Safe resolution of application-icon asset references.
//!
//! The macOS application picker stores rendered icons under
//! `<data_dir>/assets/ignored-apps/<safe-id>.png` and records the
//! relative path (e.g. `ignored-apps/com.apple.textedit.png`) as the
//! `icon_ref` field on every persisted row. The frontend never sees
//! an absolute path on disk; the only payload that crosses the Tauri
//! boundary is the opaque `icon_ref` string. This module is the
//! single owner of the rules that turn that string back into a
//! readable path inside the assets directory.
//!
//! ## Safety guarantees
//!
//! - [`resolve_icon_path`] accepts a reference only when it is
//!   non-empty, relative, does not contain any `..` component and
//!   starts with the `ignored-apps/` prefix. Anything else is
//!   rejected with a typed [`IconRefError`].
//! - The function refuses to return an absolute path the frontend
//!   could load directly; the caller must read the file and serve
//!   the bytes through a Tauri command.
//! - Once the candidate path is built, it is canonicalised through
//!   [`std::fs::canonicalize`] and verified to live underneath the
//!   `<data_dir>/assets/ignored-apps/` canonical root. A malicious
//!   symlink that points outside the allowed directory is detected
//!   by the canonical comparison and rejected.
//! - The function never logs, returns or forwards clipboard content,
//!   hashes or snippets. The only metadata it touches is the
//!   `icon_ref` string the caller supplied.

use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Sub-directory under `<data_dir>/assets` where application icons
/// for the blacklisted-apps table live. The picker persists this
/// prefix in the `icon_ref` column so the resolved path stays under
/// `<data_dir>/assets/ignored-apps/`.
pub const IGNORED_APPS_DIR: &str = "ignored-apps";

/// Sub-directory under `<data_dir>/assets` where source-application
/// icons extracted for the history card layout live. The
/// `history-card-layout` capability persists this prefix in the
/// `source_app_icon_ref` column so the resolved path stays under
/// `<data_dir>/assets/application-icons/`. The namespace is
/// deliberately distinct from [`IGNORED_APPS_DIR`] so the two
/// feature areas can evolve their asset shapes independently and the
/// read-side validator can keep the contracts minimal.
pub const APPLICATION_ICONS_DIR: &str = "application-icons";

/// Asset directory immediately under [`PlatformInfo::data_dir`].
pub const ASSETS_DIR: &str = "assets";

/// Errors that [`resolve_icon_path`] can surface. The shell maps each
/// variant to a stable string identifier so the frontend never has to
/// inspect the free-form display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconRefError {
    /// The reference was an empty string.
    Empty,
    /// The reference was an absolute path. Absolute references can
    /// never satisfy the "must live under the assets directory"
    /// guarantee, so they are rejected before any filesystem access.
    Absolute,
    /// The reference contains a `..` component. The validator
    /// rejects every form of traversal — `..`, `./..`, `a/../b`, ...
    Traversal,
    /// The reference does not start with the `ignored-apps/` prefix.
    /// Anything else is out of scope (the picker only writes icons
    /// under `ignored-apps/`).
    OutOfScope,
    /// The reference is well-formed but the underlying file does not
    /// exist or the parent directory is missing. The frontend falls
    /// back to the letter render in this case.
    NotFound,
    /// The resolved canonical path escapes the assets directory.
    /// This catches symlinks that point outside the allowed root.
    Escaped,
}

impl IconRefError {
    /// Stable snake_case identifier the frontend consumes. The shell
    /// never inspects the free-form [`Display`](fmt::Display) string
    /// to make routing decisions; it only renders it after picking
    /// the matching copy.
    pub fn kind_str(&self) -> &'static str {
        match self {
            IconRefError::Empty => "empty",
            IconRefError::Absolute => "absolute",
            IconRefError::Traversal => "traversal",
            IconRefError::OutOfScope => "out_of_scope",
            IconRefError::NotFound => "not_found",
            IconRefError::Escaped => "escaped",
        }
    }
}

impl fmt::Display for IconRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IconRefError::Empty => f.write_str("icon reference is empty"),
            IconRefError::Absolute => {
                f.write_str("icon reference must be a relative path")
            }
            IconRefError::Traversal => {
                f.write_str("icon reference must not contain '..' components")
            }
            IconRefError::OutOfScope => f.write_str(
                "icon reference must start with 'ignored-apps/' and stay under the assets directory",
            ),
            IconRefError::NotFound => f.write_str("icon file is missing on disk"),
            IconRefError::Escaped => f.write_str(
                "icon reference resolves outside the assets directory",
            ),
        }
    }
}

impl std::error::Error for IconRefError {}

/// Resolve a relative `icon_ref` to its canonical absolute path
/// inside the platform's assets directory.
///
/// The function refuses to operate on anything that is not a
/// relative, traversal-free reference prefixed with
/// [`IGNORED_APPS_DIR`]. Once the reference passes the format
/// checks, the resolved path is canonicalised so symlinks that point
/// outside the allowed root are rejected by the final
/// `starts_with(canonical_root)` comparison.
pub fn resolve_icon_path(icon_ref: &str, data_dir: &Path) -> Result<PathBuf, IconRefError> {
    resolve_icon_path_in_namespace(icon_ref, data_dir, IGNORED_APPS_DIR)
}

/// Resolve a relative source-application `icon_ref` to its
/// canonical absolute path inside the platform's assets directory.
///
/// Mirrors [`resolve_icon_path`] but accepts references prefixed with
/// [`APPLICATION_ICONS_DIR`] instead of [`IGNORED_APPS_DIR`]. The
/// namespaces are independent — a blacklist icon reference cannot
/// resolve under the source-application namespace and vice versa.
#[allow(dead_code)]
pub fn resolve_source_app_icon_path(
    icon_ref: &str,
    data_dir: &Path,
) -> Result<PathBuf, IconRefError> {
    resolve_icon_path_in_namespace(icon_ref, data_dir, APPLICATION_ICONS_DIR)
}

/// Shared validation routine used by both
/// [`resolve_icon_path`] and [`resolve_source_app_icon_path`]. The
/// function enforces the same safety contract: relative path,
/// traversal-free, scope-prefixed and canonicalised under the
/// assets namespace.
fn resolve_icon_path_in_namespace(
    icon_ref: &str,
    data_dir: &Path,
    namespace: &str,
) -> Result<PathBuf, IconRefError> {
    if icon_ref.is_empty() {
        return Err(IconRefError::Empty);
    }

    let candidate = Path::new(icon_ref);
    if candidate.is_absolute() {
        return Err(IconRefError::Absolute);
    }
    if candidate
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(IconRefError::Traversal);
    }
    if !icon_ref.starts_with(&format!("{namespace}/")) {
        return Err(IconRefError::OutOfScope);
    }

    let assets_root = data_dir.join(ASSETS_DIR);
    let allowed_root = assets_root.join(namespace);
    let canonical_root = fs::canonicalize(&allowed_root).map_err(|_| IconRefError::NotFound)?;
    let relative = candidate
        .strip_prefix(namespace)
        .map_err(|_| IconRefError::OutOfScope)?;
    let full_path = allowed_root.join(relative);
    let canonical = fs::canonicalize(&full_path).map_err(|_| IconRefError::NotFound)?;

    if !canonical.starts_with(&canonical_root) {
        return Err(IconRefError::Escaped);
    }
    Ok(canonical)
}

/// Read the PNG bytes for an `icon_ref` through the same validation
/// pipeline as [`resolve_icon_path`]. The function caps the size so a
/// tampered file cannot exhaust the renderer memory, and validates
/// the PNG magic header so the frontend can never receive garbage
/// that fails to decode in the webview.
///
/// The function returns [`IconReadError::Ref`] when the reference is
/// malformed and [`IconReadError::Io`] when the file exists but
/// cannot be read. The frontend surfaces both as a recoverable
/// fallback, never as a fatal error.
pub fn read_icon_bytes(icon_ref: &str, data_dir: &Path) -> Result<Vec<u8>, IconReadError> {
    let path = resolve_icon_path(icon_ref, data_dir)?;
    read_validated_png(&path)
}

/// Read the PNG bytes for a source-application `icon_ref`. Mirrors
/// [`read_icon_bytes`] but resolves references under the
/// [`APPLICATION_ICONS_DIR`] namespace.
#[allow(dead_code)]
pub fn read_source_app_icon_bytes(
    icon_ref: &str,
    data_dir: &Path,
) -> Result<Vec<u8>, IconReadError> {
    let path = resolve_source_app_icon_path(icon_ref, data_dir)?;
    read_validated_png(&path)
}

fn read_validated_png(path: &Path) -> Result<Vec<u8>, IconReadError> {
    let bytes = fs::read(path).map_err(|error| IconReadError::Io {
        reason: error.to_string(),
    })?;
    if bytes.len() > MAX_ICON_BYTES_LEGACY {
        return Err(IconReadError::TooLarge { size: bytes.len() });
    }
    if !looks_like_png(&bytes) {
        return Err(IconReadError::NotPng);
    }
    Ok(bytes)
}

/// Upper bound on the pixel dimensions (width and height) of an icon
/// persisted by the macOS picker. The picker renders the bundle icon
/// into a bitmap whose dimensions never exceed this constant, then
/// encodes the bitmap as PNG. A 256×256 lossless PNG stays well under
/// [`MAX_ICON_BYTES`] even for the most detailed bundles (Chrome,
/// Affinity), so the on-disk asset directory stays predictable.
pub const MAX_ICON_DIM: u32 = 256;

/// Upper bound on the byte length of an icon produced by the picker.
/// The picker encodes a bitmap at most [`MAX_ICON_DIM`] × [`MAX_ICON_DIM`]
/// pixels, so a well-formed icon stays well under this bound. The cap
/// exists so a malicious or accidental megabyte-sized file cannot
/// exhaust the renderer memory once it reaches the webview.
pub const MAX_ICON_BYTES: usize = 512 * 1024;

/// Higher read-side cap used by [`read_icon_bytes`] to keep legacy
/// icons readable. Earlier versions of the picker wrote 1024×1024 PNGs
/// that exceeded [`MAX_ICON_BYTES`] but stayed under this looser cap.
/// The picker now downsamples every new icon to the documented
/// [`MAX_ICON_DIM`] so re-selecting the same application rewrites the
/// asset at the smaller size; until then the read path accepts the
/// legacy files so the row does not render as a broken icon forever.
///
/// The legacy cap is still an order of magnitude below what a
/// tampered asset directory can deliver, so the renderer memory
/// guarantee is preserved. Anything larger than this cap is rejected
/// with [`IconReadError::TooLarge`].
pub const MAX_ICON_BYTES_LEGACY: usize = 4 * 1024 * 1024;

/// Errors that [`read_icon_bytes`] can surface in addition to the
/// format errors from [`resolve_icon_path`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconReadError {
    /// The reference failed the path-level validation.
    Ref(IconRefError),
    /// The file is on disk but cannot be read.
    Io { reason: String },
    /// The file is larger than [`MAX_ICON_BYTES_LEGACY`].
    TooLarge { size: usize },
    /// The file does not start with the PNG magic header.
    NotPng,
}

impl From<IconRefError> for IconReadError {
    fn from(error: IconRefError) -> Self {
        IconReadError::Ref(error)
    }
}

impl fmt::Display for IconReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IconReadError::Ref(error) => write!(f, "{error}"),
            IconReadError::Io { reason } => write!(f, "icon read failed: {reason}"),
            IconReadError::TooLarge { size } => {
                write!(
                    f,
                    "icon exceeds the {} byte cap (got {size})",
                    MAX_ICON_BYTES_LEGACY
                )
            }
            IconReadError::NotPng => f.write_str("icon file is not a valid PNG"),
        }
    }
}

impl std::error::Error for IconReadError {}

/// Verify the PNG magic header. The check rejects any non-PNG payload
/// so the frontend never receives bytes it cannot decode. The full
/// PNG signature lives at the start of every conforming file; chunks
/// past the signature are not validated here because the webview's
/// image decoder performs a complete parse before rendering.
fn looks_like_png(bytes: &[u8]) -> bool {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    bytes.len() >= SIGNATURE.len() && bytes[..SIGNATURE.len()] == SIGNATURE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_icon(tempdir: &tempfile::TempDir, relative: &str) -> PathBuf {
        let data_dir = tempdir.path().to_path_buf();
        let assets = data_dir.join("assets");
        let icons = assets.join("ignored-apps");
        std::fs::create_dir_all(&icons).expect("mkdir");
        let path = icons.join(relative.trim_start_matches("ignored-apps/"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir parent");
        }
        std::fs::write(&path, b"\x89PNG\r\n\x1a\n fixture").expect("write");
        path
    }

    #[test]
    fn resolve_icon_path_accepts_relative_path_under_ignored_apps() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        build_icon(&dir, "ignored-apps/com.apple.textedit.png");
        let resolved =
            resolve_icon_path("ignored-apps/com.apple.textedit.png", &data_dir).expect("ok");
        assert!(resolved.ends_with("ignored-apps/com.apple.textedit.png"));
        assert!(resolved.starts_with(fs::canonicalize(&data_dir).expect("canon")));
    }

    #[test]
    fn resolve_icon_path_rejects_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        let err = resolve_icon_path("", &data_dir).expect_err("must reject");
        assert_eq!(err.kind_str(), "empty");
    }

    #[test]
    fn resolve_icon_path_rejects_absolute_paths() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        let err = resolve_icon_path("/etc/passwd", &data_dir).expect_err("absolute");
        assert_eq!(err.kind_str(), "absolute");
    }

    #[test]
    fn resolve_icon_path_rejects_traversal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        let cases = [
            "ignored-apps/../etc/passwd",
            "ignored-apps/sub/../../escape",
            "../ignored-apps/com.apple.textedit.png",
        ];
        for case in cases {
            let err = resolve_icon_path(case, &data_dir).expect_err("traversal");
            assert_eq!(
                err.kind_str(),
                "traversal",
                "case {case} must reject traversal"
            );
        }
    }

    #[test]
    fn resolve_icon_path_rejects_out_of_scope() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        let cases = [
            "snapshots/com.apple.textedit.png",
            "com.apple.textedit.png",
            "ignored-apps-other/x.png",
        ];
        for case in cases {
            let err = resolve_icon_path(case, &data_dir).expect_err("scope");
            assert_eq!(
                err.kind_str(),
                "out_of_scope",
                "case {case} must reject scope"
            );
        }
    }

    #[test]
    fn resolve_icon_path_rejects_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        std::fs::create_dir_all(data_dir.join("assets").join("ignored-apps")).expect("mkdir");
        let err = resolve_icon_path("ignored-apps/ghost.png", &data_dir).expect_err("missing");
        assert_eq!(err.kind_str(), "not_found");
    }

    #[cfg(unix)]
    #[test]
    fn resolve_icon_path_rejects_symlink_escape() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        let icons = data_dir.join("assets").join("ignored-apps");
        std::fs::create_dir_all(&icons).expect("mkdir");
        // A symlink inside `ignored-apps/` that points to a file
        // outside the allowed root. Without the canonical
        // comparison the validator would happily resolve it.
        let target = dir.path().join("outside.png");
        std::fs::write(&target, b"\x89PNG\r\n\x1a\n outside").expect("write");
        std::fs::remove_dir_all(&icons).ok();
        std::fs::create_dir_all(&icons).expect("mkdir");
        std::os::unix::fs::symlink(&target, icons.join("escape.png")).expect("symlink");
        let err = resolve_icon_path("ignored-apps/escape.png", &data_dir).expect_err("escape");
        assert_eq!(err.kind_str(), "escaped");
    }

    #[test]
    fn resolve_icon_path_surfaces_error_when_assets_dir_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        // No `assets/ignored-apps` directory at all.
        let err = resolve_icon_path("ignored-apps/ghost.png", &data_dir).expect_err("missing");
        assert_eq!(err.kind_str(), "not_found");
    }

    #[test]
    fn read_icon_bytes_round_trips_a_valid_png() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        let bytes = b"\x89PNG\r\n\x1a\n fixture payload".to_vec();
        std::fs::create_dir_all(data_dir.join("assets").join("ignored-apps")).expect("mkdir");
        std::fs::write(
            data_dir.join("assets").join("ignored-apps").join("x.png"),
            &bytes,
        )
        .expect("write");
        let read = read_icon_bytes("ignored-apps/x.png", &data_dir).expect("read");
        assert_eq!(read, bytes);
    }

    #[test]
    fn read_icon_bytes_accepts_legacy_files_above_write_cap() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        std::fs::create_dir_all(data_dir.join("assets").join("ignored-apps")).expect("mkdir");
        // Pre-existing 1024×1024 PNGs (e.g. Chrome, Affinity) used to
        // exceed [`MAX_ICON_BYTES`]. The picker now downsamples to
        // [`MAX_ICON_DIM`], but legacy files on disk must remain
        // readable until the user re-selects the application and the
        // picker overwrites them. Anything above the legacy cap is
        // still rejected.
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        bytes.extend(std::iter::repeat_n(b'X', MAX_ICON_BYTES + 1024));
        let target = data_dir
            .join("assets")
            .join("ignored-apps")
            .join("legacy.png");
        std::fs::write(&target, &bytes).expect("write");
        let read = read_icon_bytes("ignored-apps/legacy.png", &data_dir).expect("legacy read");
        assert_eq!(read, bytes);
    }

    #[test]
    fn read_icon_bytes_rejects_oversized_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        std::fs::create_dir_all(data_dir.join("assets").join("ignored-apps")).expect("mkdir");
        // Build a PNG-signature-prefixed buffer followed by a payload
        // that exceeds the legacy cap. The validator must reject it
        // before sending the bytes to the webview.
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x0a];
        bytes.extend(std::iter::repeat_n(b'X', MAX_ICON_BYTES_LEGACY + 1));
        std::fs::write(
            data_dir
                .join("assets")
                .join("ignored-apps")
                .join("huge.png"),
            &bytes,
        )
        .expect("write");
        let err = read_icon_bytes("ignored-apps/huge.png", &data_dir).expect_err("too large");
        match err {
            IconReadError::TooLarge { size } => assert_eq!(size, bytes.len()),
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    #[test]
    fn read_icon_bytes_rejects_non_png_payloads() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        std::fs::create_dir_all(data_dir.join("assets").join("ignored-apps")).expect("mkdir");
        std::fs::write(
            data_dir
                .join("assets")
                .join("ignored-apps")
                .join("not-png.bin"),
            b"not a real png",
        )
        .expect("write");
        let err = read_icon_bytes("ignored-apps/not-png.bin", &data_dir).expect_err("not png");
        assert_eq!(err, IconReadError::NotPng);
    }

    #[test]
    fn read_icon_bytes_surfaces_invalid_reference() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        let err = read_icon_bytes("/etc/passwd", &data_dir).expect_err("absolute");
        assert_eq!(err, IconReadError::Ref(IconRefError::Absolute));
    }

    #[test]
    fn icon_ref_error_kind_str_is_stable() {
        assert_eq!(IconRefError::Empty.kind_str(), "empty");
        assert_eq!(IconRefError::Absolute.kind_str(), "absolute");
        assert_eq!(IconRefError::Traversal.kind_str(), "traversal");
        assert_eq!(IconRefError::OutOfScope.kind_str(), "out_of_scope");
        assert_eq!(IconRefError::NotFound.kind_str(), "not_found");
        assert_eq!(IconRefError::Escaped.kind_str(), "escaped");
    }

    #[test]
    fn icon_ref_error_display_describes_each_variant() {
        assert_eq!(IconRefError::Empty.to_string(), "icon reference is empty");
        assert_eq!(
            IconRefError::Absolute.to_string(),
            "icon reference must be a relative path"
        );
        assert_eq!(
            IconRefError::Traversal.to_string(),
            "icon reference must not contain '..' components"
        );
        assert!(IconRefError::OutOfScope
            .to_string()
            .contains("ignored-apps"));
        assert_eq!(
            IconRefError::NotFound.to_string(),
            "icon file is missing on disk"
        );
        assert!(IconRefError::Escaped.to_string().contains("outside"));
    }

    #[test]
    fn max_icon_dim_matches_the_picker_documented_target() {
        // The picker renders at most `MAX_ICON_DIM × MAX_ICON_DIM`
        // pixels and the read path enforces `MAX_ICON_BYTES`. Keep
        // both constants aligned with the OpenSpec change so a future
        // tweak that drifts one side without the other surfaces here.
        assert_eq!(MAX_ICON_DIM, 256);
        // A lossless PNG of 256×256 RGBA pixels fits comfortably under
        // the write cap; if the picker ever starts emitting a different
        // pixel format these constants become the canary.
        const { assert!(MAX_ICON_BYTES < MAX_ICON_BYTES_LEGACY) };
        const { assert!(MAX_ICON_BYTES_LEGACY <= 4 * 1024 * 1024) };
    }

    // -----------------------------------------------------------------
    // `application-icons/` namespace — `history-card-layout` regression.
    //
    // The user-reported bug was that the card rail rendered a generic
    // fallback icon for every capture. The end-to-end fix relies on
    // the macOS metadata provider persisting a PNG under
    // `application-icons/<safe-id>.png` and the shell-side bridge
    // reading it back through `read_source_app_icon_bytes`. The
    // tests below pin the namespace contract so a refactor that
    // drifts one side without the other surfaces here instead of as
    // a regression in production.
    // -----------------------------------------------------------------

    fn build_source_app_icon(tempdir: &tempfile::TempDir, relative: &str) -> std::path::PathBuf {
        let data_dir = tempdir.path().to_path_buf();
        let icons = data_dir.join("assets").join("application-icons");
        std::fs::create_dir_all(&icons).expect("mkdir");
        let path = icons.join(relative.trim_start_matches("application-icons/"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir parent");
        }
        std::fs::write(&path, b"\x89PNG\r\n\x1a\n source-app-icon").expect("write");
        path
    }

    #[test]
    fn resolve_source_app_icon_path_accepts_relative_path_under_application_icons() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        build_source_app_icon(&dir, "application-icons/com.apple.Terminal.png");
        let resolved =
            resolve_source_app_icon_path("application-icons/com.apple.Terminal.png", &data_dir)
                .expect("ok");
        assert!(resolved.ends_with("application-icons/com.apple.Terminal.png"));
    }

    #[test]
    fn resolve_source_app_icon_path_rejects_ignored_apps_namespace() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        let err = resolve_source_app_icon_path("ignored-apps/com.apple.Terminal.png", &data_dir)
            .expect_err("namespace must not bleed across");
        assert_eq!(err.kind_str(), "out_of_scope");
    }

    #[test]
    fn resolve_icon_path_rejects_application_icons_namespace() {
        // Symmetric guarantee: the legacy blacklist bridge MUST NOT
        // serve files from the `application-icons/` namespace.
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        let err = resolve_icon_path("application-icons/com.apple.Terminal.png", &data_dir)
            .expect_err("namespace must not bleed across");
        assert_eq!(err.kind_str(), "out_of_scope");
    }

    #[test]
    fn read_source_app_icon_bytes_round_trips_a_valid_png() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        let bytes = b"\x89PNG\r\n\x1a\n fixture payload".to_vec();
        let target = data_dir
            .join("assets")
            .join("application-icons")
            .join("com.apple.Terminal.png");
        std::fs::create_dir_all(target.parent().unwrap()).expect("mkdir");
        std::fs::write(&target, &bytes).expect("write");
        let read =
            read_source_app_icon_bytes("application-icons/com.apple.Terminal.png", &data_dir)
                .expect("read");
        assert_eq!(read, bytes);
    }

    #[test]
    fn read_source_app_icon_bytes_rejects_traversal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        let cases = [
            "application-icons/../etc/passwd",
            "application-icons/sub/../../escape",
        ];
        for case in cases {
            let err =
                read_source_app_icon_bytes(case, &data_dir).expect_err("traversal must reject");
            match err {
                IconReadError::Ref(IconRefError::Traversal) => {}
                other => panic!("case {case} must reject as traversal, got {other:?}"),
            }
        }
    }

    #[test]
    fn application_icons_and_ignored_apps_directories_are_distinct() {
        // The two namespaces MUST stay separate so a backfill can
        // safely reason about them independently and so the read
        // path can apply the right scope check.
        assert_ne!(APPLICATION_ICONS_DIR, IGNORED_APPS_DIR);
        assert_eq!(IGNORED_APPS_DIR, "ignored-apps");
        assert_eq!(APPLICATION_ICONS_DIR, "application-icons");
    }
}
