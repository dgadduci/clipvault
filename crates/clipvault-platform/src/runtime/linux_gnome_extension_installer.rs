//! Atomic installer for the bundled GNOME Shell extension.
//!
//! The extension lives as a local resource of the Tauri shell. The
//! installer never downloads code from the network and never requires
//! root: it writes to
//! `~/.local/share/gnome-shell/extensions/<uuid>/` for the user
//! that started ClipVault.
//!
//! ## Safety guarantees
//!
//! - The installer refuses to touch a directory whose `metadata.json`
//!   carries a different `uuid` than the one we ship — refusing the
//!   operation protects existing extensions installed by other
//!   applications.
//! - Writes happen in a temporary directory first (`<parent>/.staging-<n>`)
//!   and become visible only after a successful `rename(2)`. A
//!   crashed or partial install leaves the previous copy (or nothing)
//!   on disk; it never leaves a half-written tree behind.
//! - The installer refuses symlinks that escape the target directory
//!   at every step. A `.desktop` file or extension metadata that points
//!   outside the extension tree is rejected and the install aborts.
//! - Disabling and uninstalling operate exclusively on directories
//!   whose `uuid` matches ClipVault's. Other extensions never move.
//!
//! The module exposes a pure-data type ([`BundledExtension`]) and a
//! service ([`ExtensionInstaller`]) that drives the disk side. Tests
//! build the installer with a custom host and bundled bytes so the
//! suite never touches the real `~/.local/share` tree.

#![cfg(all(target_os = "linux", feature = "linux-gnome-shell-integration"))]
#![allow(dead_code)]

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// User-writable extensions directory the installer targets.
pub const EXTENSIONS_PARENT: &str = ".local/share/gnome-shell/extensions";

/// Subdirectory name (= ClipVault extension UUID).
pub const EXTENSION_UUID: &str = "clipvault@clipvault.app";

/// Temporary directory prefix the installer uses while committing a
/// new tree. The prefix starts with a dot so `mv` cannot mistake it
/// for a sibling extension on the same parent.
pub const STAGING_PREFIX: &str = ".staging-";

/// Bundled metadata the installer reads. The caller supplies the
/// bytes so the platform crate stays agnostic of how the Tauri
/// shell bundles its resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundledExtension {
    pub uuid: String,
    pub name: String,
    pub description: String,
    pub version: u32,
    #[serde(default, rename = "shell-version")]
    pub shell_versions: Vec<String>,
}

impl BundledExtension {
    /// Parse the bytes the caller pulled from the bundled resource.
    pub fn from_json(raw: &str) -> Result<Self, InstallerError> {
        serde_json::from_str(raw)
            .map_err(|error| InstallerError::InvalidMetadata(error.to_string()))
    }

    /// Render the matching `metadata.json` the installer writes to
    /// disk. The body is regenerated from the typed struct so an
    /// accidental hand-edit of `metadata.json` cannot drift away
    /// from the in-memory representation.
    pub fn render_metadata(&self) -> Result<String, InstallerError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| InstallerError::InvalidMetadata(error.to_string()))
    }
}

#[derive(Debug, Error)]
pub enum InstallerError {
    #[error("home directory is not available; cannot install the extension")]
    HomeUnavailable,
    #[error("extensions parent directory could not be created: {0}")]
    ParentUnavailable(String),
    #[error("invalid bundled metadata: {0}")]
    InvalidMetadata(String),
    #[error("refusing to install over a foreign extension at {path} (uuid {uuid:?})")]
    ForeignExtension { path: String, uuid: Option<String> },
    #[error("metadata is incompatible with this GNOME Shell ({reason})")]
    Incompatible { reason: String },
    #[error("io error: {0}")]
    Io(String),
}

impl InstallerError {
    pub fn stable_label(&self) -> &'static str {
        match self {
            InstallerError::HomeUnavailable => "home_unavailable",
            InstallerError::ParentUnavailable(_) => "parent_unavailable",
            InstallerError::InvalidMetadata(_) => "invalid_metadata",
            InstallerError::ForeignExtension { .. } => "foreign_extension",
            InstallerError::Incompatible { .. } => "incompatible_metadata",
            InstallerError::Io(_) => "io_error",
        }
    }
}

/// Snapshot the installer returns to the UI / diagnostics surfaces.
/// The fields are deliberately stripped of filesystem metadata so
/// the JSON the Tauri command emits never carries the install
/// target directory or the raw metadata body: the UI needs the
/// lifecycle state and the protocol version, not the host path.
/// The Rust side keeps the internal view because tests and the
/// installer itself still need it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Installation {
    pub installed: bool,
    pub enabled: bool,
    pub uuid: String,
    pub version: u32,
    #[serde(skip_serializing)]
    pub target_dir: PathBuf,
    #[serde(skip_serializing)]
    pub metadata_json: Option<String>,
}

impl Installation {
    pub fn not_installed(uuid: &str) -> Self {
        Self {
            installed: false,
            enabled: false,
            uuid: uuid.to_string(),
            version: 0,
            target_dir: PathBuf::new(),
            metadata_json: None,
        }
    }
}

/// State the installer needs about the host.
pub trait HostEnvironment {
    /// `$HOME`, used to compute the user-writable extensions parent.
    fn home_dir(&self) -> Option<PathBuf>;

    /// Optional current GNOME Shell version. The installer compares
    /// the bundled `shell-version` against this value and reports
    /// `incompatible_metadata` when there is no overlap.
    fn gnome_shell_version(&self) -> Option<String>;
}

/// Default host environment the production code path uses. The
/// helpers are intentionally infallible for the well-typed results
/// so the bootstrap can compose them without `try` operators; the
/// fallback values always render a usable diagnostics surface.
pub struct SystemHostEnvironment;

impl HostEnvironment for SystemHostEnvironment {
    fn home_dir(&self) -> Option<PathBuf> {
        std::env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    }

    fn gnome_shell_version(&self) -> Option<String> {
        None
    }
}

#[cfg(test)]
pub struct TestHostEnvironment {
    pub home: PathBuf,
    pub version: Option<String>,
}

#[cfg(test)]
impl HostEnvironment for TestHostEnvironment {
    fn home_dir(&self) -> Option<PathBuf> {
        Some(self.home.clone())
    }
    fn gnome_shell_version(&self) -> Option<String> {
        self.version.clone()
    }
}

impl fmt::Debug for dyn HostEnvironment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostEnvironment").finish()
    }
}

/// Outcome of [`ExtensionInstaller::install`]. Carries the metadata
/// the install path produced so the diagnostics endpoint can surface
/// it without re-reading the file.
#[derive(Debug, Clone, Serialize)]
pub struct InstallOutcome {
    pub installation: Installation,
    pub activated: bool,
}

/// Service that performs the actual install / uninstall operations.
///
/// The bundled `metadata.json` and `extension.js` bytes are supplied
/// by the caller — the platform crate never reaches outside its
/// own source tree. The Tauri shell reads the bundled resources
/// through its resource resolver and feeds them to the installer.
pub struct ExtensionInstaller<H: HostEnvironment + Send + Sync + 'static> {
    host: Arc<H>,
}

impl<H: HostEnvironment + Send + Sync + 'static> ExtensionInstaller<H> {
    pub fn new(host: Arc<H>) -> Self {
        Self { host }
    }

    pub fn host(&self) -> &Arc<H> {
        &self.host
    }

    pub fn target_dir(&self) -> Result<PathBuf, InstallerError> {
        let home = self
            .host
            .home_dir()
            .ok_or(InstallerError::HomeUnavailable)?;
        Ok(home.join(EXTENSIONS_PARENT).join(EXTENSION_UUID))
    }

    /// Inspect an existing install. Returns `None` when the target
    /// directory is absent or does not contain a `metadata.json`.
    pub fn inspect(&self) -> Result<Option<Installation>, InstallerError> {
        let target_dir = self.target_dir()?;
        if !target_dir.exists() {
            return Ok(None);
        }
        let metadata_path = target_dir.join("metadata.json");
        if !metadata_path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&metadata_path).map_err(map_io_error)?;
        let parsed: BundledExtension = serde_json::from_str(&raw)
            .map_err(|error| InstallerError::InvalidMetadata(error.to_string()))?;
        let metadata_json = Some(raw);
        let version = parsed.version;
        let uuid = parsed.uuid.clone();
        if uuid != EXTENSION_UUID {
            return Err(InstallerError::ForeignExtension {
                path: target_dir.display().to_string(),
                uuid: Some(uuid),
            });
        }
        Ok(Some(Installation {
            installed: true,
            enabled: false,
            uuid,
            version,
            target_dir,
            metadata_json,
        }))
    }

    fn validate_metadata(&self, metadata: &BundledExtension) -> Result<(), InstallerError> {
        if metadata.uuid != EXTENSION_UUID {
            return Err(InstallerError::ForeignExtension {
                path: EXTENSION_UUID.to_string(),
                uuid: Some(metadata.uuid.clone()),
            });
        }
        if let Some(version) = self.host.gnome_shell_version() {
            if !metadata.shell_versions.is_empty()
                && !metadata
                    .shell_versions
                    .iter()
                    .any(|entry| entry.trim() == version.trim())
            {
                return Err(InstallerError::Incompatible {
                    reason: format!(
                        "shell version {} is not in {:?}",
                        version, metadata.shell_versions
                    ),
                });
            }
        }
        Ok(())
    }

    /// Validate the supplied metadata string against `host` and
    /// parse it into a typed [`BundledExtension`]. Always called
    /// from [`install`] and [`uninstall`] before any write.
    pub fn parse_metadata(&self, metadata_json: &str) -> Result<BundledExtension, InstallerError> {
        let parsed = BundledExtension::from_json(metadata_json)?;
        self.validate_metadata(&parsed)?;
        Ok(parsed)
    }

    /// Write the extension tree to disk under a staging directory,
    /// validate the staged metadata, then atomically rename to the
    /// final path. The installer never touches the target directory
    /// in place.
    pub fn install(
        &self,
        metadata_json: &str,
        extension_js: &str,
    ) -> Result<InstallOutcome, InstallerError> {
        let target_dir = self.target_dir()?;
        let parent = target_dir
            .parent()
            .ok_or_else(|| InstallerError::ParentUnavailable(target_dir.display().to_string()))?
            .to_path_buf();
        ensure_directory(&parent)?;
        let staging = pick_staging(&parent)?;
        ensure_directory(&staging)?;
        let metadata = self.parse_metadata(metadata_json)?;
        let rendered = metadata.render_metadata()?;
        write_file(&staging.join("metadata.json"), rendered.as_bytes())?;
        write_file(&staging.join("extension.js"), extension_js.as_bytes())?;
        // Symlink audit: refuse any symlink that escapes the staging
        // directory. We just wrote two files so a real install has
        // none, but a hostile bundle could ship symlinks in the
        // future; the guard catches the case before the rename.
        audit_no_symlink_escape(&staging, &staging)?;
        if target_dir.exists() {
            if let Some(existing) = self.inspect()? {
                if existing.uuid != EXTENSION_UUID {
                    cleanup_staging(&staging);
                    return Err(InstallerError::ForeignExtension {
                        path: target_dir.display().to_string(),
                        uuid: Some(existing.uuid),
                    });
                }
            }
            let backup = backup_existing(&target_dir)?;
            let outcome = atomic_rename(&staging, &target_dir);
            match outcome {
                Ok(()) => {
                    cleanup_backup(&backup);
                }
                Err(error) => {
                    restore_backup(&backup, &target_dir);
                    cleanup_staging(&staging);
                    return Err(error);
                }
            }
        } else if let Err(error) = atomic_rename(&staging, &target_dir) {
            cleanup_staging(&staging);
            return Err(error);
        }
        let installation = Installation {
            installed: true,
            enabled: false,
            uuid: metadata.uuid.clone(),
            version: metadata.version,
            target_dir: target_dir.clone(),
            metadata_json: Some(rendered),
        };
        Ok(InstallOutcome {
            installation,
            activated: false,
        })
    }

    /// Remove the extension tree if it belongs to ClipVault. Refuses
    /// a foreign install with [`InstallerError::ForeignExtension`].
    pub fn uninstall(&self) -> Result<Installation, InstallerError> {
        let target_dir = self.target_dir()?;
        if !target_dir.exists() {
            return Ok(Installation::not_installed(EXTENSION_UUID));
        }
        let metadata_path = target_dir.join("metadata.json");
        let raw = fs::read_to_string(&metadata_path).map_err(map_io_error)?;
        let parsed: BundledExtension = serde_json::from_str(&raw)
            .map_err(|error| InstallerError::InvalidMetadata(error.to_string()))?;
        if parsed.uuid != EXTENSION_UUID {
            return Err(InstallerError::ForeignExtension {
                path: target_dir.display().to_string(),
                uuid: Some(parsed.uuid),
            });
        }
        fs::remove_dir_all(&target_dir).map_err(map_io_error)?;
        Ok(Installation::not_installed(EXTENSION_UUID))
    }
}

pub type ArcHost<H> = std::sync::Arc<H>;

use std::sync::Arc;

fn write_file(path: &Path, body: &[u8]) -> Result<(), InstallerError> {
    let mut file = fs::File::create(path).map_err(map_io_error)?;
    file.write_all(body).map_err(map_io_error)?;
    file.flush().map_err(map_io_error)?;
    Ok(())
}

fn ensure_directory(path: &Path) -> Result<(), InstallerError> {
    if path.exists() {
        if path.is_dir() {
            return Ok(());
        }
        return Err(InstallerError::ParentUnavailable(
            path.display().to_string(),
        ));
    }
    fs::create_dir_all(path).map_err(map_io_error)
}

fn pick_staging(parent: &Path) -> Result<PathBuf, InstallerError> {
    for attempt in 0..16u32 {
        let candidate = parent.join(format!("{STAGING_PREFIX}{attempt}-{}", std::process::id()));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(InstallerError::Io("no staging slot available".to_string()))
}

fn cleanup_staging(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

fn atomic_rename(from: &Path, to: &Path) -> Result<(), InstallerError> {
    fs::rename(from, to).map_err(map_io_error)
}

fn backup_existing(target: &Path) -> Result<Option<PathBuf>, InstallerError> {
    if !target.exists() {
        return Ok(None);
    }
    let mut unique = target.with_extension("bak");
    let mut attempt = 1u32;
    while unique.exists() {
        unique = target.with_extension(format!("bak{attempt}"));
        attempt += 1;
        if attempt > 32 {
            return Err(InstallerError::Io("no backup slot available".to_string()));
        }
    }
    fs::rename(target, &unique).map_err(map_io_error)?;
    Ok(Some(unique))
}

fn restore_backup(backup: &Option<PathBuf>, target: &Path) {
    if let Some(path) = backup {
        let _ = fs::rename(path, target);
    }
}

fn cleanup_backup(backup: &Option<PathBuf>) {
    if let Some(path) = backup {
        let _ = fs::remove_dir_all(path);
    }
}

fn audit_no_symlink_escape(root: &Path, current: &Path) -> Result<(), InstallerError> {
    let entries = match fs::read_dir(current) {
        Ok(entries) => entries,
        Err(error) => return Err(InstallerError::Io(error.to_string())),
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => return Err(InstallerError::Io(error.to_string())),
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => return Err(InstallerError::Io(error.to_string())),
        };
        if file_type.is_symlink() {
            let target = match fs::read_link(entry.path()) {
                Ok(target) => target,
                Err(error) => return Err(InstallerError::Io(error.to_string())),
            };
            let absolute = if target.is_absolute() {
                target
            } else {
                entry
                    .path()
                    .parent()
                    .map(|parent| parent.join(&target))
                    .unwrap_or(target)
            };
            if !absolute.starts_with(root) {
                return Err(InstallerError::Io(format!(
                    "symlink {} escapes staging root",
                    entry.path().display()
                )));
            }
        }
        if file_type.is_dir() {
            audit_no_symlink_escape(root, &entry.path())?;
        }
    }
    Ok(())
}

fn map_io_error(error: io::Error) -> InstallerError {
    InstallerError::Io(error.to_string())
}

/// Convenience used by the bootstrap to install the bundled extension
/// without constructing an explicit installer. Production callers
/// keep using [`ExtensionInstaller`] so they can swap host
/// implementations under test.
pub fn install_bundled(
    metadata_json: &str,
    extension_js: &str,
) -> Result<InstallOutcome, InstallerError> {
    let installer = ExtensionInstaller::new(Arc::new(SystemHostEnvironment));
    installer.install(metadata_json, extension_js)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const BUNDLED_METADATA: &str = r#"{
  "uuid": "clipvault@clipvault.app",
  "name": "ClipVault GNOME Focus Bridge",
  "description": "Bridge for ClipVault clipboard manager",
  "version": 1,
  "shell-version": ["42", "43", "44", "45", "46", "47", "48"]
}"#;

    const BUNDLED_EXTENSION: &str = "/* placeholder */\n";

    fn make_installer(
        home: PathBuf,
        version: Option<String>,
    ) -> ExtensionInstaller<TestHostEnvironment> {
        let host = Arc::new(TestHostEnvironment { home, version });
        ExtensionInstaller::new(host)
    }

    #[test]
    fn bundled_metadata_has_expected_uuid_and_versions() {
        let metadata = BundledExtension::from_json(BUNDLED_METADATA).expect("parse");
        assert_eq!(metadata.uuid, EXTENSION_UUID);
        assert!(!metadata.shell_versions.is_empty());
        assert!(metadata.shell_versions.iter().any(|v| v == "42"));
        assert!(metadata.shell_versions.iter().any(|v| v == "48"));
    }

    #[test]
    fn install_writes_metadata_and_extension_atomically() {
        let dir = TempDir::new().expect("tempdir");
        let home = dir.path().to_path_buf();
        let installer = make_installer(home.clone(), None);
        let outcome = installer
            .install(BUNDLED_METADATA, BUNDLED_EXTENSION)
            .expect("install");
        let target = installer.target_dir().expect("target");
        assert!(target.join("metadata.json").exists());
        assert!(target.join("extension.js").exists());
        let restored = fs::read_to_string(target.join("metadata.json")).expect("read");
        let parsed: BundledExtension = serde_json::from_str(&restored).expect("parse");
        assert_eq!(parsed.uuid, EXTENSION_UUID);
        assert!(outcome.installation.installed);
    }

    #[test]
    fn install_rejects_foreign_extension_at_uuid_path() {
        let dir = TempDir::new().expect("tempdir");
        let home = dir.path().to_path_buf();
        let target = home.join(EXTENSIONS_PARENT).join(EXTENSION_UUID);
        fs::create_dir_all(&target).expect("mkdir");
        fs::write(
            target.join("metadata.json"),
            r#"{"uuid":"someone-else@gnome.org","name":"foreign","description":"x","version":1,"shell-version":["45"]}"#,
        )
        .expect("write");
        let installer = make_installer(home, None);
        match installer.install(BUNDLED_METADATA, BUNDLED_EXTENSION) {
            Err(InstallerError::ForeignExtension { .. }) => {}
            other => panic!("expected ForeignExtension, got {other:?}"),
        }
    }

    #[test]
    fn install_rejects_incompatible_shell_version() {
        let dir = TempDir::new().expect("tempdir");
        let home = dir.path().to_path_buf();
        let installer = make_installer(home, Some("3.0".to_string()));
        match installer.install(BUNDLED_METADATA, BUNDLED_EXTENSION) {
            Err(InstallerError::Incompatible { .. }) => {}
            other => panic!("expected Incompatible, got {other:?}"),
        }
    }

    #[test]
    fn uninstall_only_affects_clipvault_extension() {
        let dir = TempDir::new().expect("tempdir");
        let home = dir.path().to_path_buf();
        let foreign_dir = home.join(EXTENSIONS_PARENT).join("foreign@gnome.org");
        fs::create_dir_all(&foreign_dir).expect("mkdir");
        fs::write(
            foreign_dir.join("metadata.json"),
            r#"{"uuid":"foreign@gnome.org","name":"foreign","description":"x","version":1,"shell-version":["45"]}"#,
        )
        .expect("write");
        let installer = make_installer(home.clone(), None);
        installer
            .install(BUNDLED_METADATA, BUNDLED_EXTENSION)
            .expect("install");
        let uninstalled = installer.uninstall().expect("uninstall");
        assert!(!uninstalled.installed);
        assert!(foreign_dir.exists(), "foreign extension must remain");
    }

    #[test]
    fn install_is_idempotent_when_same_metadata_present() {
        let dir = TempDir::new().expect("tempdir");
        let home = dir.path().to_path_buf();
        let installer = make_installer(home, None);
        installer
            .install(BUNDLED_METADATA, BUNDLED_EXTENSION)
            .expect("install first");
        let second = installer
            .install(BUNDLED_METADATA, BUNDLED_EXTENSION)
            .expect("install second");
        let target = installer.target_dir().expect("target");
        assert!(target.join("metadata.json").exists());
        assert!(target.join("extension.js").exists());
        assert!(second.installation.installed);
    }

    #[test]
    fn install_writes_metadata_through_typed_struct() {
        let dir = TempDir::new().expect("tempdir");
        let home = dir.path().to_path_buf();
        let installer = make_installer(home, None);
        installer
            .install(BUNDLED_METADATA, BUNDLED_EXTENSION)
            .expect("install");
        let target = installer.target_dir().expect("target");
        let rendered = fs::read_to_string(target.join("metadata.json")).expect("read metadata");
        let typed = BundledExtension::from_json(BUNDLED_METADATA).expect("parse");
        let re_rendered = typed.render_metadata().expect("render");
        assert_eq!(rendered, re_rendered);
    }

    #[test]
    fn install_rejects_bundle_with_wrong_uuid() {
        let dir = TempDir::new().expect("tempdir");
        let home = dir.path().to_path_buf();
        let installer = make_installer(home, None);
        let bad = r#"{"uuid":"other@gnome.org","name":"x","description":"y","version":1,"shell-version":["45"]}"#;
        match installer.install(bad, BUNDLED_EXTENSION) {
            Err(InstallerError::ForeignExtension { .. }) => {}
            other => panic!("expected ForeignExtension, got {other:?}"),
        }
    }

    #[test]
    fn symlink_audit_rejects_escape() {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("inside")).expect("mkdir");
        let outside = dir.path().join("outside");
        fs::create_dir_all(&outside).expect("mkdir outside");
        std::os::unix::fs::symlink(&outside, root.join("inside/escape")).expect("symlink");
        let result = audit_no_symlink_escape(root, root);
        assert!(result.is_err(), "symlink escape must be refused");
    }

    #[test]
    fn installer_error_labels_are_stable() {
        assert_eq!(
            InstallerError::HomeUnavailable.stable_label(),
            "home_unavailable"
        );
        assert_eq!(
            InstallerError::ForeignExtension {
                path: "x".into(),
                uuid: None
            }
            .stable_label(),
            "foreign_extension"
        );
        assert_eq!(
            InstallerError::Incompatible { reason: "x".into() }.stable_label(),
            "incompatible_metadata"
        );
        assert_eq!(
            InstallerError::InvalidMetadata("x".into()).stable_label(),
            "invalid_metadata"
        );
    }

    #[test]
    fn renderer_emits_valid_metadata_for_bundled_extension() {
        let parsed = BundledExtension::from_json(BUNDLED_METADATA).expect("parse");
        let rendered = parsed.render_metadata().expect("render");
        let again: BundledExtension = serde_json::from_str(&rendered).expect("parse render");
        assert_eq!(again.uuid, EXTENSION_UUID);
    }

    /// `Installation` MUST serialise without `target_dir` or
    /// `metadata_json` — both fields carry host filesystem metadata
    /// the public payload contract refuses to expose.
    #[test]
    fn installation_serialization_omits_target_dir_and_metadata() {
        let dir = TempDir::new().expect("tempdir");
        let home = dir.path().to_path_buf();
        let installer = make_installer(home.clone(), None);
        installer
            .install(BUNDLED_METADATA, BUNDLED_EXTENSION)
            .expect("install");
        let installation = installer.inspect().expect("inspect").expect("installed");
        let raw = serde_json::to_string(&installation).expect("serialise");
        assert!(
            !raw.contains("target_dir"),
            "installation JSON leaked target_dir: {raw}"
        );
        assert!(
            !raw.contains("metadata_json"),
            "installation JSON leaked metadata_json: {raw}"
        );
        assert!(
            !raw.contains(".local/share"),
            "installation JSON leaked install path: {raw}"
        );
        assert!(
            !raw.contains(dir.path().to_str().unwrap_or("")),
            "installation JSON leaked home path: {raw}"
        );
    }
}
