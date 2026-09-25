//! Safe writer for the `application-icons/` asset namespace.
//!
//! The store mirrors the [`crate::clipboard_assets::ClipboardAssetStore`]
//! contract but persists under a dedicated namespace so the
//! `peer-source-app-presentation` change can store icon bytes
//! the host delivers without ever accepting a peer-supplied
//! path / filename / reference. The persisted value is always
//! a content-addressed file name (`<lowercase-sha256>.png`) so
//! two peers that ship the same icon share a single on-disk
//! file; the rollback path keeps a reused icon and only
//! removes a freshly written one with zero committed
//! references.
//!
//! ## Layout
//!
//! ```text
//! <data_dir>/assets/application-icons/<lowercase-sha256>.png
//! ```
//!
//! The relative reference (`application-icons/<hash>.png`) is
//! the only string the persistence layer stores in
//! `remote_imports.source_app_icon_ref`. The reader
//! ([`crate::clipboard_assets`] / [`clipvault_platform::app_assets`])
//! enforces the same validation rules the writer honours: an
//! absolute path, a path outside the namespace or a path with a
//! `..` component never reaches disk.
//!
//! ## Atomicity
//!
//! [`ApplicationIconStore::stage`] writes the bytes to a
//! temporary file inside the namespace and then `rename`s it
//! into place. A crash or a failed write can never leave a
//! partially valid asset that a later read would serve. When
//! the target already exists and validates cleanly the bytes
//! are reused instead of rewritten.
//!
//! ## Validation
//!
//! The helper runs the same byte / signature / decode /
//! dimension checks the [`crate::peer_source_app_presentation`]
//! validators pin. The validator is shared so the on-demand
//! route and the explicit import path cannot drift.

use parking_lot::{Mutex, MutexGuard};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};

use thiserror::Error;

use clipvault_platform::APPLICATION_ICONS_DIR;

use crate::peer_source_app_presentation::{validate_source_app_icon, MAX_SOURCE_APP_ICON_BYTES};

/// Sub-directory the writer persists icons under. Mirrors
/// [`clipvault_platform::APPLICATION_ICONS_DIR`] so the writer
/// and reader agree on the prefix byte-for-byte.
pub const APPLICATION_ICONS_ASSET_DIR: &str = APPLICATION_ICONS_DIR;

/// PNG extension the writer always uses. The reader enforces
/// the same extension so a stray `.bin` payload can never
/// reach the resolver.
pub const APPLICATION_ICONS_ASSET_EXTENSION: &str = "png";

/// Outcome of [`ApplicationIconStore::stage`]. Mirrors
/// [`crate::clipboard_assets::StoreOutcome`] so the import
/// rollback path can branch on the same `Written` / `Reused`
/// taxonomy: a `Written` outcome signals a freshly created
/// file the rollback path MAY delete; a `Reused` outcome
/// signals a shared icon the rollback path MUST keep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationIconStageOutcome {
    /// A fresh icon file was created.
    Written {
        /// Relative reference the caller persists in
        /// `remote_imports.source_app_icon_ref`.
        asset_ref: String,
    },
    /// A valid icon with the same hash already existed and
    /// was reused; no second file was created.
    Reused {
        /// Relative reference the caller persists in
        /// `remote_imports.source_app_icon_ref`.
        asset_ref: String,
    },
}

impl ApplicationIconStageOutcome {
    /// Relative reference the import transaction should
    /// persist. Stable across `Written` / `Reused` outcomes.
    pub fn asset_ref(&self) -> &str {
        match self {
            Self::Written { asset_ref } | Self::Reused { asset_ref } => asset_ref,
        }
    }
    /// Whether the staging helper produced a fresh file the
    /// rollback path is allowed to delete. Mirrors
    /// [`crate::clipboard_assets::StoreOutcome::kind`] so the
    /// triage log surfaces the same identifier across the
    /// two namespaces.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Written { .. } => "written",
            Self::Reused { .. } => "reused",
        }
    }
}

/// Typed error the writer surfaces. Every variant collapses
/// to a stable reason the caller can branch on without
/// inspecting the underlying payload.
#[derive(Debug, Error)]
pub enum ApplicationIconError {
    /// The bytes failed the source-app presentation
    /// validation (signature, decode, dimensions, byte cap).
    #[error("invalid source-application icon bytes")]
    InvalidBytes,
    /// The bytes exceeded the documented
    /// [`crate::peer_source_app_presentation::MAX_SOURCE_APP_ICON_BYTES`]
    /// cap. The variant collapses to a stable identifier; the
    /// caller MUST NOT inspect the offending byte length.
    #[error("source-application icon exceeds the byte cap")]
    TooLarge,
    /// Filesystem I/O failed during staging. The variant
    /// keeps the path off its surface so a log line never
    /// exposes a peer-relative or absolute location.
    #[error("source-application icon staging failed")]
    Io,
}

impl ApplicationIconError {
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::InvalidBytes => "invalid_bytes",
            Self::TooLarge => "too_large",
            Self::Io => "io",
        }
    }
}

impl From<ApplicationIconError> for fmt::Error {
    fn from(_: ApplicationIconError) -> Self {
        fmt::Error
    }
}

/// Safe writer for the `application-icons/` namespace. Cheap
/// to clone: every clone shares the resolved data directory
/// and the mutation lock so concurrent imports cannot race on
/// the same hash.
#[derive(Debug, Default)]
struct ApplicationIconStoreState {
    staged_leases: HashMap<String, usize>,
}

static APPLICATION_ICON_STATES: OnceLock<
    Mutex<HashMap<PathBuf, Arc<Mutex<ApplicationIconStoreState>>>>,
> = OnceLock::new();

fn shared_state(data_dir: &Path) -> Arc<Mutex<ApplicationIconStoreState>> {
    let registry = APPLICATION_ICON_STATES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut registry = registry.lock();
    Arc::clone(
        registry
            .entry(data_dir.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(ApplicationIconStoreState::default()))),
    )
}

#[derive(Debug, Clone)]
pub struct ApplicationIconStore {
    data_dir: PathBuf,
    state: Arc<Mutex<ApplicationIconStoreState>>,
}

impl ApplicationIconStore {
    /// Build a new writer rooted at the supplied data
    /// directory. The namespace `<data_dir>/assets/application-icons/`
    /// is created on demand the first time [`Self::stage`]
    /// runs; the constructor itself stays side-effect-free so
    /// tests that point the writer at a tempdir never observe
    /// an unexpected directory creation.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        let state = shared_state(&data_dir);
        Self { data_dir, state }
    }

    /// `<data_dir>/assets/application-icons`. The path the
    /// writer materialises; the caller MUST NOT persist the
    /// absolute value — only the relative reference the
    /// staging outcome returns.
    pub fn root(&self) -> PathBuf {
        self.data_dir
            .join(clipvault_platform::ASSETS_DIR)
            .join(APPLICATION_ICONS_ASSET_DIR)
    }

    /// Serialize a larger operation that must keep an asset
    /// in place across staging and a subsequent SQLite
    /// commit. The caller MUST keep the guard alive until the
    /// transaction has committed or failed.
    fn lock_mutations(&self) -> MutexGuard<'_, ApplicationIconStoreState> {
        self.state.lock()
    }

    /// Stage a peer-supplied PNG under the local
    /// `application-icons/` namespace. The helper validates the
    /// bytes (signature, decode, dimensions, byte cap),
    /// computes a content-addressed file name and writes
    /// atomically.
    ///
    /// The helper NEVER accepts a peer-supplied path /
    /// filename / reference; the persisted value is always
    /// `<lowercase-sha256>.png` and the returned outcome
    /// carries the relative reference (`application-icons/<hash>.png`).
    pub fn stage(&self, bytes: &[u8]) -> Result<ApplicationIconStageOutcome, ApplicationIconError> {
        // The validation helper enforces signature + decode +
        // dimensions + byte cap and collapses to a typed error
        // for every failure mode so the caller never inspects
        // the offending bytes.
        validate_source_app_icon(bytes).map_err(|error| match error {
            crate::peer_source_app_presentation::SourceAppPresentationError::InvalidIcon => {
                if bytes.len() > MAX_SOURCE_APP_ICON_BYTES {
                    ApplicationIconError::TooLarge
                } else {
                    ApplicationIconError::InvalidBytes
                }
            }
            crate::peer_source_app_presentation::SourceAppPresentationError::InvalidName => {
                // The icon path never touches the name validator;
                // reaching this branch means the validator
                // surfaces the wrong variant, which the type
                // system already prevents. The mapping stays
                // here so a future refactor cannot silently lose
                // the failure mode.
                ApplicationIconError::InvalidBytes
            }
        })?;
        let hash = content_hash(bytes);
        let asset_ref =
            format!("{APPLICATION_ICONS_ASSET_DIR}/{hash}.{APPLICATION_ICONS_ASSET_EXTENSION}");
        let mut guard = self.lock_mutations();
        let root = self.checked_root(true)?;
        let target = root.join(format!("{hash}.{APPLICATION_ICONS_ASSET_EXTENSION}"));
        match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(ApplicationIconError::InvalidBytes);
            }
            Ok(_) if existing_icon_matches(&target, bytes) => {
                // Reuse only a regular, valid file whose bytes match
                // the content-addressed input. A pre-existing
                // corrupt file is atomically replaced below; a
                // symlink is always rejected above.
                *guard.staged_leases.entry(asset_ref.clone()).or_insert(0) += 1;
                return Ok(ApplicationIconStageOutcome::Reused { asset_ref });
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(ApplicationIconError::Io),
        }
        let temp = root.join(format!(".{hash}.tmp"));
        let _ = fs::remove_file(&temp);
        fs::write(&temp, bytes).map_err(io_to_error)?;
        let rename = fs::rename(&temp, &target);
        if rename.is_err() {
            let _ = fs::remove_file(&temp);
            return Err(ApplicationIconError::Io);
        }
        // Re-validate the renamed file to confirm the
        // validator's promise survives the rename; the check
        // is cheap (signature + first frame) and catches any
        // platform that mishandled the temp-file round-trip.
        if !validate_existing_icon(&target) {
            let _ = fs::remove_file(&target);
            return Err(ApplicationIconError::InvalidBytes);
        }
        *guard.staged_leases.entry(asset_ref.clone()).or_insert(0) += 1;
        Ok(ApplicationIconStageOutcome::Written { asset_ref })
    }

    /// Finish a successful import lease. This keeps the file and
    /// releases the process-wide reservation shared by every store
    /// rooted at this data directory.
    pub fn commit_staged(&self, asset_ref: &str) {
        let mut state = self.lock_mutations();
        decrement_lease(&mut state, asset_ref);
    }

    /// Roll back a staged icon. A reused icon is always preserved.
    /// A newly written icon is removed only when it has no other
    /// staging leases and the caller confirms SQLite has no committed
    /// provenance reference. The shared lock spans the lease update,
    /// reference query and unlink so another stage cannot slip in.
    pub fn rollback_staged<F>(
        &self,
        asset_ref: &str,
        kind: &ApplicationIconStageOutcome,
        has_references: F,
    ) -> Result<(), ApplicationIconError>
    where
        F: FnOnce(&str) -> Result<bool, ()>,
    {
        let mut state = self.lock_mutations();
        let remaining = decrement_lease(&mut state, asset_ref);
        if matches!(kind, ApplicationIconStageOutcome::Reused { .. }) || remaining > 0 {
            return Ok(());
        }
        if has_references(asset_ref).map_err(|_| ApplicationIconError::Io)? {
            return Ok(());
        }
        let Some(filename) = asset_ref.strip_prefix("application-icons/") else {
            return Err(ApplicationIconError::InvalidBytes);
        };
        if filename.is_empty()
            || filename.contains('/')
            || filename.contains('\\')
            || !filename.ends_with(".png")
            || !is_safe_icon_ref(asset_ref)
        {
            return Err(ApplicationIconError::InvalidBytes);
        }
        let root = self.checked_root(false)?;
        let candidate = root.join(filename);
        match fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(ApplicationIconError::Io),
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ApplicationIconError::InvalidBytes);
            }
            Ok(_) => {}
        }
        let path = self.icon_path(asset_ref)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(ApplicationIconError::Io),
        }
    }

    /// Read a locally generated source-app icon after validating the
    /// reference namespace, canonical path and PNG limits.
    pub fn read_bytes(&self, asset_ref: &str) -> Result<Vec<u8>, ApplicationIconError> {
        let path = self.icon_path(asset_ref)?;
        let bytes = fs::read(path).map_err(|_| ApplicationIconError::Io)?;
        validate_source_app_icon(&bytes).map_err(|_| ApplicationIconError::InvalidBytes)?;
        Ok(bytes)
    }

    fn icon_path(&self, asset_ref: &str) -> Result<PathBuf, ApplicationIconError> {
        let Some(filename) = asset_ref.strip_prefix("application-icons/") else {
            return Err(ApplicationIconError::InvalidBytes);
        };
        if filename.is_empty()
            || filename.contains('/')
            || filename.contains('\\')
            || !filename.ends_with(".png")
            || !is_safe_icon_ref(asset_ref)
        {
            return Err(ApplicationIconError::InvalidBytes);
        }
        let root = self.checked_root(false)?;
        let path = root.join(filename);
        let canonical = fs::canonicalize(&path).map_err(|_| ApplicationIconError::Io)?;
        if !canonical.starts_with(&root)
            || fs::symlink_metadata(&path)
                .map_err(|_| ApplicationIconError::Io)?
                .file_type()
                .is_symlink()
        {
            return Err(ApplicationIconError::InvalidBytes);
        }
        Ok(canonical)
    }

    fn checked_root(&self, create: bool) -> Result<PathBuf, ApplicationIconError> {
        let canonical_data = fs::canonicalize(&self.data_dir).map_err(io_to_error)?;
        let assets = self.data_dir.join(clipvault_platform::ASSETS_DIR);
        ensure_directory(&assets, create)?;
        let canonical_assets = fs::canonicalize(&assets).map_err(io_to_error)?;
        if !canonical_assets.starts_with(&canonical_data) {
            return Err(ApplicationIconError::InvalidBytes);
        }
        let root = assets.join(APPLICATION_ICONS_ASSET_DIR);
        ensure_directory(&root, create)?;
        let canonical_root = fs::canonicalize(&root).map_err(io_to_error)?;
        if !canonical_root.starts_with(&canonical_assets)
            || canonical_root != canonical_assets.join(APPLICATION_ICONS_ASSET_DIR)
        {
            return Err(ApplicationIconError::InvalidBytes);
        }
        Ok(canonical_root)
    }
}

fn ensure_directory(path: &Path, create: bool) -> Result<(), ApplicationIconError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(ApplicationIconError::InvalidBytes)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound && create => {
            fs::create_dir(path).map_err(io_to_error)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(ApplicationIconError::Io),
        Err(_) => Err(ApplicationIconError::Io),
    }
}

fn decrement_lease(state: &mut ApplicationIconStoreState, asset_ref: &str) -> usize {
    let Some(leases) = state.staged_leases.get_mut(asset_ref) else {
        return 0;
    };
    if *leases <= 1 {
        state.staged_leases.remove(asset_ref);
        0
    } else {
        *leases -= 1;
        *leases
    }
}

fn io_to_error(_error: io::Error) -> ApplicationIconError {
    ApplicationIconError::Io
}

/// SHA-256 hex digest of the supplied bytes. Lowercased so
/// the persisted file name stays ASCII.
fn content_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Validate a file on disk without copying its bytes into
/// memory. The helper mirrors the signature / first-frame
/// validation the source-app presentation helper performs
/// so a future platform quirk that mishandles the temp-file
/// rename surfaces as a typed error before the caller persists
/// the relative reference.
fn validate_existing_icon(path: &Path) -> bool {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    if bytes.is_empty() {
        return false;
    }
    validate_source_app_icon(&bytes).is_ok()
}

fn existing_icon_matches(path: &Path, expected: &[u8]) -> bool {
    validate_existing_icon(path)
        && fs::read(path)
            .map(|bytes| bytes == expected)
            .unwrap_or(false)
}

/// Reject every path component that escapes the namespace.
/// The helper is metadata-only and never echoes the supplied
/// reference.
pub fn is_safe_icon_ref(reference: &str) -> bool {
    let path = std::path::Path::new(reference);
    let mut components = path.components();
    let starts_with_namespace = match components.next() {
        Some(Component::Normal(first)) => {
            first == std::ffi::OsStr::new(APPLICATION_ICONS_ASSET_DIR)
        }
        _ => false,
    };
    starts_with_namespace && components.all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer_source_app_presentation::MAX_SOURCE_APP_ICON_LONGEST_SIDE;

    fn encode_png(width: u32, height: u32) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut buf, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("png header");
            let frame = vec![0x80u8; (width as usize) * (height as usize) * 4];
            writer.write_image_data(&frame).expect("png data");
            writer.finish().expect("png finish");
        }
        buf
    }

    #[test]
    fn stage_rejects_non_png_payload() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ApplicationIconStore::new(dir.path().to_path_buf());
        let err = store.stage(b"not a png").expect_err("invalid bytes");
        assert_eq!(err.kind_str(), "invalid_bytes");
    }

    #[test]
    fn stage_rejects_oversized_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ApplicationIconStore::new(dir.path().to_path_buf());
        let mut bytes = encode_png(8, 8);
        // Append filler to exceed the byte cap while keeping
        // the dimensions within the documented limits.
        bytes.extend(std::iter::repeat(0u8).take(MAX_SOURCE_APP_ICON_BYTES + 1));
        let err = store.stage(&bytes).expect_err("too large");
        assert_eq!(err.kind_str(), "too_large");
    }

    #[test]
    fn stage_rejects_truncated_signature() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ApplicationIconStore::new(dir.path().to_path_buf());
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend(std::iter::repeat(0u8).take(32));
        let err = store.stage(&bytes).expect_err("invalid bytes");
        assert_eq!(err.kind_str(), "invalid_bytes");
    }

    #[test]
    fn stage_rejects_oversized_dimensions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ApplicationIconStore::new(dir.path().to_path_buf());
        let bytes = encode_png(
            MAX_SOURCE_APP_ICON_LONGEST_SIDE + 1,
            MAX_SOURCE_APP_ICON_LONGEST_SIDE + 1,
        );
        let err = store.stage(&bytes).expect_err("invalid bytes");
        assert_eq!(err.kind_str(), "invalid_bytes");
    }

    #[test]
    fn stage_writes_content_addressed_file_and_reuses_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ApplicationIconStore::new(dir.path().to_path_buf());
        let bytes = encode_png(32, 32);
        let first = store.stage(&bytes).expect("stage");
        let expected_ref = first.asset_ref().to_string();
        assert!(expected_ref.starts_with("application-icons/"));
        assert!(expected_ref.ends_with(".png"));
        let root = store.root();
        let on_disk = root.join(
            expected_ref
                .strip_prefix("application-icons/")
                .expect("application-icons/ prefix"),
        );
        assert!(on_disk.exists(), "icon file must be persisted on disk");
        let persisted = std::fs::read(&on_disk).expect("read icon");
        assert_eq!(persisted, bytes);

        // A second call reuses the file: the helper does NOT
        // rewrite a valid existing icon even though the
        // mutation lock would allow it. The rollback path
        // MUST keep the file in both cases because some other
        // peer may already reference it.
        let second = store.stage(&bytes).expect("stage again");
        assert_eq!(second.asset_ref(), first.asset_ref());
        assert_eq!(second.kind(), "reused");
        assert_eq!(first.kind(), "written");
    }

    #[cfg(unix)]
    #[test]
    fn stage_rejects_symlinked_icon_namespace_and_target() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().join("data");
        let outside = dir.path().join("outside");
        fs::create_dir_all(data_dir.join(clipvault_platform::ASSETS_DIR)).expect("assets dir");
        fs::create_dir(&outside).expect("outside dir");
        symlink(
            &outside,
            data_dir
                .join(clipvault_platform::ASSETS_DIR)
                .join(APPLICATION_ICONS_ASSET_DIR),
        )
        .expect("namespace symlink");
        let store = ApplicationIconStore::new(&data_dir);
        assert_eq!(
            store.stage(&encode_png(8, 8)).unwrap_err().kind_str(),
            "invalid_bytes"
        );
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);

        fs::remove_file(
            data_dir
                .join(clipvault_platform::ASSETS_DIR)
                .join(APPLICATION_ICONS_ASSET_DIR),
        )
        .expect("remove namespace symlink");
        let root = store.root();
        fs::create_dir_all(&root).expect("icon root");
        let bytes = encode_png(9, 9);
        let target_ref = match store.stage(&bytes).expect("first valid stage") {
            ApplicationIconStageOutcome::Written { asset_ref } => asset_ref,
            ApplicationIconStageOutcome::Reused { .. } => panic!("fresh temporary root"),
        };
        store.commit_staged(&target_ref);
        let target = root.join(target_ref.trim_start_matches("application-icons/"));
        fs::remove_file(&target).expect("remove target");
        let outside_target = outside.join("target.png");
        fs::write(&outside_target, b"keep").expect("outside target");
        symlink(&outside_target, &target).expect("target symlink");
        assert_eq!(store.stage(&bytes).unwrap_err().kind_str(), "invalid_bytes");
        assert_eq!(fs::read(&outside_target).unwrap(), b"keep");
    }

    #[test]
    fn rollback_of_written_icon_preserves_concurrent_reuse_lease() {
        let dir = tempfile::tempdir().expect("tempdir");
        let first_store = ApplicationIconStore::new(dir.path());
        let second_store = ApplicationIconStore::new(dir.path());
        let bytes = encode_png(24, 24);
        let first = first_store.stage(&bytes).expect("first stage");
        let second = second_store.stage(&bytes).expect("concurrent reused stage");
        assert_eq!(first.kind(), "written");
        assert_eq!(second.kind(), "reused");

        first_store
            .rollback_staged(first.asset_ref(), &first, |_| Ok(false))
            .expect("rollback first stage");
        assert_eq!(second_store.read_bytes(second.asset_ref()).unwrap(), bytes);
        second_store.commit_staged(second.asset_ref());
        assert_eq!(
            second_store.read_bytes(second.asset_ref()).unwrap().len(),
            bytes.len()
        );
    }

    #[test]
    fn rollback_removes_only_unreferenced_new_icon_after_last_lease() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ApplicationIconStore::new(dir.path());
        let bytes = encode_png(12, 12);
        let staged = store.stage(&bytes).expect("stage");
        let path = store
            .root()
            .join(staged.asset_ref().trim_start_matches("application-icons/"));
        store
            .rollback_staged(staged.asset_ref(), &staged, |_| Ok(false))
            .expect("rollback");
        assert!(!path.exists());
    }

    #[test]
    fn is_safe_icon_ref_rejects_traversal_and_absolute_paths() {
        assert!(is_safe_icon_ref("application-icons/foo.png"));
        assert!(is_safe_icon_ref("application-icons/sub/foo.png"));
        assert!(!is_safe_icon_ref("../application-icons/foo.png"));
        assert!(!is_safe_icon_ref("/etc/passwd"));
        assert!(!is_safe_icon_ref("clipboard/foo.png"));
        assert!(!is_safe_icon_ref(""));
    }
}
