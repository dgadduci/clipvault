//! Install / uninstall lifecycle for the bundled KWin script.
//!
//! The script lives at
//! `~/.local/share/kwin/scripts/<SCRIPT_PLUGIN_ID>/` and is
//! declared to KWin over its documented D-Bus interface
//! (`org.kde.KWin /Scripting`) instead of being copied to a global
//! location. The installer is intentionally narrow:
//!
//! * It only ever touches `clipvault-kde-source-app` under the
//!   user's `kwin/scripts/` directory; any pre-existing entry with
//!   a foreign metadata identity is left untouched.
//! * It refuses to write through `kwriteconfig`,
//!   `qdbus`, `kpackagetool` or any other external binary — the
//!   only side-effect channel is KWin's own D-Bus interface.
//! * Activation / deactivation happen via KWin's documented
//!   `Plugins/<id>Enabled` key in `kwinrc` and a follow-up
//!   `reconfigure` call. `kwriteconfig` is forbidden; the
//!   installer writes the file directly so the existing keys stay
//!   intact.
//! * `uninstall` removes only the bundle's own directory and only
//!   after the script has been deactivated; foreign extensions
//!   under `~/.local/share/kwin/scripts` are never deleted.
//!
//! The module is intentionally separate from
//! `linux_kde_kwin_integration`: the integration owns the
//! in-process bridge; this module owns the install lifecycle.

#![cfg(all(target_os = "linux", feature = "linux-kde-kwin-integration"))]

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::runtime::linux_kde_kwin_integration::SCRIPT_PLUGIN_ID;

// Re-export so external callers can reference the bundle id
// without depending on the integration module directly.
pub use crate::runtime::linux_kde_kwin_integration::SCRIPT_PLUGIN_ID as KWIN_SCRIPT_PLUGIN_ID;

/// Errors the installer can surface. Every variant maps to a
/// stable label consumed verbatim by the diagnostics endpoint.
#[derive(Debug, Error)]
pub enum KdeKwinInstallerError {
    /// `$HOME` is unset or empty; the installer cannot compute a
    /// user-writable install target.
    #[error("home directory is not available")]
    HomeUnavailable,
    /// A foreign extension already owns `clipvault-kde-source-app`
    /// under `~/.local/share/kwin/scripts/`. The installer refuses
    /// to overwrite it.
    #[error("refusing to install over a foreign KPackage: {0}")]
    ForeignExtension(String),
    /// The bundled `metadata.json` failed to parse.
    #[error("invalid bundled metadata: {0}")]
    InvalidMetadata(String),
    /// An I/O error happened during install / uninstall.
    #[error("io error: {0}")]
    Io(String),
    /// The KWin reconfig call returned a non-zero status.
    #[error("kwin reconfigure failed: {0}")]
    KwinReconfigure(String),
}

impl KdeKwinInstallerError {
    pub fn stable_label(&self) -> &'static str {
        match self {
            KdeKwinInstallerError::HomeUnavailable => "home_unavailable",
            KdeKwinInstallerError::ForeignExtension(_) => "foreign_extension",
            KdeKwinInstallerError::InvalidMetadata(_) => "invalid_metadata",
            KdeKwinInstallerError::Io(_) => "io_error",
            KdeKwinInstallerError::KwinReconfigure(_) => "kwin_reconfigure_failed",
        }
    }
}

/// Snapshot the installer returns to the UI / diagnostics surface.
/// The fields are deliberately stripped of filesystem metadata so
/// the JSON the Tauri command emits never carries the install
/// target directory or the raw metadata body: the UI needs the
/// lifecycle state and the protocol version, not the host path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KwinInstallation {
    pub installed: bool,
    pub enabled: bool,
    pub plugin_id: String,
    pub version: u32,
}

impl KwinInstallation {
    pub fn not_installed() -> Self {
        Self {
            installed: false,
            enabled: false,
            plugin_id: SCRIPT_PLUGIN_ID.to_string(),
            version: 0,
        }
    }
}

/// Bundled metadata the installer reads. The caller supplies the
/// bytes so the platform crate stays agnostic of how the Tauri
/// shell bundles its resources.
#[derive(Debug, Clone)]
pub struct BundledKwinScript {
    pub metadata_json: String,
    pub main_qml: String,
    pub main_js: String,
}

impl BundledKwinScript {
    pub fn from_strings(metadata_json: String, main_qml: String, main_js: String) -> Self {
        Self {
            metadata_json,
            main_qml,
            main_js,
        }
    }
}

/// State the installer needs about the host.
pub trait KwinHostEnvironment: Send + Sync {
    fn home_dir(&self) -> Option<PathBuf>;
}

/// Default host environment the production code path uses.
pub struct SystemKwinHostEnvironment;

impl KwinHostEnvironment for SystemKwinHostEnvironment {
    fn home_dir(&self) -> Option<PathBuf> {
        std::env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    }
}

impl fmt::Debug for dyn KwinHostEnvironment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KwinHostEnvironment").finish()
    }
}

/// Concrete kwinrc path the installer should read / write. The
/// installer keeps the path in a [`PathBuf`] rather than calling
/// `std::env` directly so tests can redirect every host-side
/// effect to a temporary directory.
#[derive(Debug, Clone)]
pub struct KwinrcLocation {
    pub home: Option<PathBuf>,
}

impl KwinrcLocation {
    pub fn from_env() -> Self {
        Self {
            home: std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
        }
    }

    pub fn kwinrc_path(&self) -> Option<PathBuf> {
        self.home.as_ref().map(|home| home.join(".config/kwinrc"))
    }
}

/// Outcome of [`KwinInstaller::install`].
#[derive(Debug, Clone, Serialize)]
pub struct KwinInstallOutcome {
    pub installation: KwinInstallation,
    /// Whether the script was activated as part of the install.
    /// Idempotent re-installs leave this value at `false` so the
    /// caller can distinguish a fresh install from a re-apply.
    pub activated: bool,
}

/// Compute the user-writable install target for the bundled KWin
/// script. Lives outside the installer struct so tests can call it
/// without building the full dependency graph.
pub fn default_install_target() -> Option<PathBuf> {
    crate::runtime::linux_kde_kwin_integration::default_package_dir()
}

/// The KWin installer. The struct is intentionally stateless so
/// the shell can build a fresh instance for every Tauri command.
pub struct KwinInstaller {
    host: Arc<dyn KwinHostEnvironment>,
    kwinrc: KwinrcLocation,
}

impl KwinInstaller {
    pub fn new() -> Self {
        Self {
            host: Arc::new(SystemKwinHostEnvironment),
            kwinrc: KwinrcLocation::from_env(),
        }
    }

    pub fn with_host(host: Arc<dyn KwinHostEnvironment>) -> Self {
        Self {
            host,
            kwinrc: KwinrcLocation::from_env(),
        }
    }

    pub fn with_host_and_kwinrc(
        host: Arc<dyn KwinHostEnvironment>,
        kwinrc: KwinrcLocation,
    ) -> Self {
        Self { host, kwinrc }
    }

    /// Inspect the install target. Returns `Ok(None)` when the
    /// package directory is absent or belongs to a foreign agent.
    pub fn inspect(&self) -> Result<Option<KwinInstallation>, KdeKwinInstallerError> {
        let target = match self.target_dir() {
            Some(target) => target,
            None => return Ok(None),
        };
        let metadata = match fs::read_to_string(target.join("metadata.json")) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(KdeKwinInstallerError::Io(error.to_string())),
        };
        let parsed = parse_metadata(&metadata)?;
        if parsed.id != SCRIPT_PLUGIN_ID {
            return Err(KdeKwinInstallerError::ForeignExtension(parsed.id));
        }
        let enabled = self.is_enabled_in_kwinrc();
        Ok(Some(KwinInstallation {
            installed: true,
            enabled,
            plugin_id: SCRIPT_PLUGIN_ID.to_string(),
            version: parsed.version,
        }))
    }

    /// Idempotent install. The first call writes the bundled
    /// package, activates it and asks KWin to reload; subsequent
    /// calls leave the directory untouched and return the live
    /// `enabled` flag.
    pub fn install(
        &self,
        bundled: &BundledKwinScript,
    ) -> Result<KwinInstallOutcome, KdeKwinInstallerError> {
        let target = self
            .target_dir()
            .ok_or(KdeKwinInstallerError::HomeUnavailable)?;

        // Refuse to install over a foreign package that already
        // owns the directory. Foreign packages include any entry
        // whose `Id` is not `clipvault-kde-source-app`; the
        // `metadata.json` parser already enforces this.
        let metadata_path = target.join("metadata.json");
        if metadata_path.exists() {
            let raw = fs::read_to_string(&metadata_path)
                .map_err(|error| KdeKwinInstallerError::Io(error.to_string()))?;
            let parsed = parse_metadata(&raw)?;
            if parsed.id != SCRIPT_PLUGIN_ID {
                return Err(KdeKwinInstallerError::ForeignExtension(parsed.id));
            }
        }

        let contents_dir = target.join("contents");
        let ui_dir = contents_dir.join("ui");
        let code_dir = contents_dir.join("code");
        fs::create_dir_all(&ui_dir)
            .map_err(|error| KdeKwinInstallerError::Io(error.to_string()))?;
        fs::create_dir_all(&code_dir)
            .map_err(|error| KdeKwinInstallerError::Io(error.to_string()))?;

        // Parse metadata up-front so a malformed bundle surfaces
        // before we touch the filesystem.
        let parsed = parse_metadata(&bundled.metadata_json)?;
        if parsed.id != SCRIPT_PLUGIN_ID {
            return Err(KdeKwinInstallerError::ForeignExtension(parsed.id));
        }

        write_atomic(&metadata_path, bundled.metadata_json.as_bytes())?;
        write_atomic(&ui_dir.join("main.qml"), bundled.main_qml.as_bytes())?;
        write_atomic(&code_dir.join("main.js"), bundled.main_js.as_bytes())?;

        // Activate only on the first install. Re-installs leave
        // the existing `enabled` flag untouched so the call is
        // truly idempotent.
        let first_install = !self.is_enabled_in_kwinrc();
        if first_install {
            self.enable_in_kwinrc()?;
            request_kwin_reconfigure()?;
        }

        Ok(KwinInstallOutcome {
            installation: KwinInstallation {
                installed: true,
                enabled: true,
                plugin_id: SCRIPT_PLUGIN_ID.to_string(),
                version: parsed.version,
            },
            activated: first_install,
        })
    }

    /// Idempotent uninstall. Removes the bundled package
    /// directory and disables the plugin. Returns the post-uninstall
    /// `KwinInstallation`, which is `not_installed` when the
    /// directory was present and the operation succeeded, or
    /// `not_installed` when nothing was there to begin with.
    pub fn uninstall(&self) -> Result<KwinInstallation, KdeKwinInstallerError> {
        let target = match self.target_dir() {
            Some(target) => target,
            None => {
                self.disable_in_kwinrc()?;
                return Ok(KwinInstallation::not_installed());
            }
        };
        if !target.exists() {
            self.disable_in_kwinrc()?;
            return Ok(KwinInstallation::not_installed());
        }
        if let Some(metadata) = fs::read_to_string(target.join("metadata.json"))
            .ok()
            .and_then(|raw| parse_metadata(&raw).ok())
        {
            if metadata.id != SCRIPT_PLUGIN_ID {
                return Err(KdeKwinInstallerError::ForeignExtension(metadata.id));
            }
        }
        // Disable first, then ask KWin to reload so the script
        // can no longer publish envelopes while we delete the
        // directory.
        self.disable_in_kwinrc()?;
        request_kwin_reconfigure()?;
        fs::remove_dir_all(&target)
            .map_err(|error| KdeKwinInstallerError::Io(error.to_string()))?;
        Ok(KwinInstallation::not_installed())
    }

    fn target_dir(&self) -> Option<PathBuf> {
        let home = self.host.home_dir()?;
        Some(
            home.join(".local/share/kwin/scripts")
                .join(SCRIPT_PLUGIN_ID),
        )
    }

    fn is_enabled_in_kwinrc(&self) -> bool {
        let Some(path) = self.kwinrc.kwinrc_path() else {
            return false;
        };
        let Ok(raw) = fs::read_to_string(&path) else {
            return false;
        };
        let key = format!("Plugins/{SCRIPT_PLUGIN_ID}Enabled");
        for line in raw.lines() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with(&key) {
                continue;
            }
            let Some((_, value)) = trimmed.split_once('=') else {
                continue;
            };
            let value = value.trim();
            return value.eq_ignore_ascii_case("true");
        }
        false
    }

    fn enable_in_kwinrc(&self) -> Result<(), KdeKwinInstallerError> {
        self.set_enabled_in_kwinrc(true)
    }

    fn disable_in_kwinrc(&self) -> Result<(), KdeKwinInstallerError> {
        self.set_enabled_in_kwinrc(false)
    }

    fn set_enabled_in_kwinrc(&self, enabled: bool) -> Result<(), KdeKwinInstallerError> {
        let path = match self.kwinrc.kwinrc_path() {
            Some(path) => path,
            None => return Ok(()),
        };
        let raw = match fs::read_to_string(&path) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(KdeKwinInstallerError::Io(error.to_string())),
        };
        let key = format!("Plugins/{SCRIPT_PLUGIN_ID}Enabled");
        let value = if enabled { "true" } else { "false" };
        let mut lines: Vec<String> = raw.lines().map(|line| line.to_string()).collect();
        let mut replaced = false;
        for line in lines.iter_mut() {
            let trimmed = line.trim_start();
            if trimmed.starts_with(&key) {
                *line = format!("{key}={value}");
                replaced = true;
            }
        }
        if !replaced {
            if !lines.is_empty() && !lines.last().unwrap().is_empty() {
                lines.push(String::new());
            }
            // Find / insert the [Plugins] group.
            let mut inserted = false;
            for index in 0..lines.len() {
                if lines[index].trim() == "[Plugins]" {
                    lines.insert(index + 1, format!("{key}={value}"));
                    inserted = true;
                    break;
                }
            }
            if !inserted {
                lines.push("[Plugins]".to_string());
                lines.push(format!("{key}={value}"));
            }
        }
        let body = lines.join("\n") + "\n";
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| KdeKwinInstallerError::Io(error.to_string()))?;
        }
        fs::write(&path, body).map_err(|error| KdeKwinInstallerError::Io(error.to_string()))?;
        Ok(())
    }
}

impl Default for KwinInstaller {
    fn default() -> Self {
        Self::new()
    }
}

use std::sync::Arc;

#[derive(Debug)]
struct ParsedMetadata {
    id: String,
    version: u32,
}

fn parse_metadata(raw: &str) -> Result<ParsedMetadata, KdeKwinInstallerError> {
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|error| KdeKwinInstallerError::InvalidMetadata(error.to_string()))?;
    let id = value
        .get("KPlugin")
        .and_then(|plugin| plugin.get("Id"))
        .and_then(|id| id.as_str())
        .ok_or_else(|| KdeKwinInstallerError::InvalidMetadata("missing KPlugin.Id".to_string()))?
        .to_string();
    let version = value
        .get("KPlugin")
        .and_then(|plugin| plugin.get("Version"))
        .and_then(|version| version.as_str())
        .and_then(|raw| raw.parse::<u32>().ok())
        .unwrap_or(0);
    Ok(ParsedMetadata { id, version })
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), KdeKwinInstallerError> {
    // KPackage metadata files are tiny; a single `write` is atomic
    // enough for the contract — the install path never produces
    // half-written bytes because every write replaces the file
    // outright.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| KdeKwinInstallerError::Io(error.to_string()))?;
    }
    fs::write(path, bytes).map_err(|error| KdeKwinInstallerError::Io(error.to_string()))?;
    Ok(())
}

/// Ask KWin to reconfigure by sending `reconfigure` over the
/// session bus. Returns `Ok(())` even when KWin is offline so a
/// future `KDE` session picks up the freshly written `kwinrc`
/// automatically; the only error the helper raises is when the
/// D-Bus call itself fails.
fn request_kwin_reconfigure() -> Result<(), KdeKwinInstallerError> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut command = Command::new("dbus-send");
    command
        .args([
            "--session",
            "--print-reply=literal",
            "--dest=org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting.start",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    // `start` is the documented entry point on KWin's scripting
    // bus; sending it asks KWin to reload its script registry and
    // honour the freshly written `kwinrc` keys. We only check the
    // exit code; the reply payload is irrelevant.
    match command.spawn().and_then(|mut child| child.wait()) {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => {
            let _ = writeln!(std::io::stderr(), "kwin reconfigure returned {status}");
            Err(KdeKwinInstallerError::KwinReconfigure(format!(
                "exit status {status}"
            )))
        }
        Err(error) => {
            // dbus-send missing is a transient environment issue
            // and never fatal: KWin will pick the keys up at the
            // next start.
            if error.kind() == io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(KdeKwinInstallerError::KwinReconfigure(error.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use tempfile::TempDir;

    struct FakeHost {
        home: PathBuf,
    }

    impl KwinHostEnvironment for FakeHost {
        fn home_dir(&self) -> Option<PathBuf> {
            Some(self.home.clone())
        }
    }

    fn bundled_fixture() -> BundledKwinScript {
        BundledKwinScript::from_strings(
            r#"{
    "KPackageStructure": "KWin/Script",
    "KPlugin": {
        "Authors": [{ "Name": "ClipVault" }],
        "Description": "Publishes the focused application's desktop file name to the ClipVault clipboard manager.",
        "Icon": "preferences-system-windows-script-test",
        "Id": "clipvault-kde-source-app",
        "License": "MIT",
        "Name": "ClipVault KDE Source App",
        "Version": "1.0"
    },
    "X-Plasma-API": "declarativescript"
}"#
            .to_string(),
            "import QtQuick\nLoader {}\n".to_string(),
            "// js\n".to_string(),
        )
    }

    fn make_installer(temp: &TempDir) -> KwinInstaller {
        let host = Arc::new(FakeHost {
            home: temp.path().to_path_buf(),
        });
        KwinInstaller::with_host_and_kwinrc(
            host,
            KwinrcLocation {
                home: Some(temp.path().to_path_buf()),
            },
        )
    }

    #[test]
    fn install_writes_files_and_reports_first_activation() {
        let temp = TempDir::new().unwrap();
        let installer = make_installer(&temp);
        let outcome = installer.install(&bundled_fixture()).expect("install");
        assert!(outcome.installation.installed);
        assert!(outcome.activated);
        assert_eq!(outcome.installation.plugin_id, SCRIPT_PLUGIN_ID);
        let target = temp
            .path()
            .join(".local/share/kwin/scripts")
            .join(SCRIPT_PLUGIN_ID);
        assert!(target.join("metadata.json").is_file());
        assert!(target.join("contents/ui/main.qml").is_file());
        assert!(target.join("contents/code/main.js").is_file());
    }

    #[test]
    fn install_is_idempotent() {
        let temp = TempDir::new().unwrap();
        let installer = make_installer(&temp);
        installer
            .install(&bundled_fixture())
            .expect("first install");
        let second = installer
            .install(&bundled_fixture())
            .expect("second install");
        assert!(second.installation.installed);
        assert!(!second.activated, "second install must not re-activate");
    }

    #[test]
    fn install_refuses_foreign_metadata() {
        let temp = TempDir::new().unwrap();
        let installer = make_installer(&temp);
        let foreign = BundledKwinScript::from_strings(
            r#"{
    "KPackageStructure": "KWin/Script",
    "KPlugin": { "Id": "some-other-script", "Version": "1.0" },
    "X-Plasma-API": "declarativescript"
}"#
            .to_string(),
            String::new(),
            String::new(),
        );
        let error = installer
            .install(&foreign)
            .expect_err("foreign metadata must be rejected");
        assert_eq!(error.stable_label(), "foreign_extension");
    }

    #[test]
    fn install_rejects_existing_foreign_directory() {
        let temp = TempDir::new().unwrap();
        let host = Arc::new(FakeHost {
            home: temp.path().to_path_buf(),
        });
        let target = temp
            .path()
            .join(".local/share/kwin/scripts")
            .join(SCRIPT_PLUGIN_ID);
        std::fs::create_dir_all(target.join("contents/ui")).unwrap();
        std::fs::write(
            target.join("metadata.json"),
            r#"{"KPlugin": {"Id": "another-script", "Version": "1.0"}}"#,
        )
        .unwrap();
        let installer = KwinInstaller::with_host_and_kwinrc(
            host,
            KwinrcLocation {
                home: Some(temp.path().to_path_buf()),
            },
        );
        let error = installer
            .install(&bundled_fixture())
            .expect_err("foreign existing metadata must be rejected");
        assert_eq!(error.stable_label(), "foreign_extension");
    }

    #[test]
    fn uninstall_removes_bundle_only() {
        let temp = TempDir::new().unwrap();
        // Seed a foreign KWin script the installer must NOT touch.
        let foreign = temp
            .path()
            .join(".local/share/kwin/scripts/some-other-script");
        std::fs::create_dir_all(foreign.join("contents/ui")).unwrap();
        std::fs::write(foreign.join("metadata.json"), "{}").unwrap();

        let installer = make_installer(&temp);
        installer.install(&bundled_fixture()).expect("install");
        let uninstalled = installer.uninstall().expect("uninstall");
        assert!(!uninstalled.installed);
        let target = temp
            .path()
            .join(".local/share/kwin/scripts")
            .join(SCRIPT_PLUGIN_ID);
        assert!(!target.exists(), "bundle directory must be removed");
        assert!(
            foreign.join("metadata.json").exists(),
            "foreign extension must remain untouched"
        );
    }

    #[test]
    fn inspect_reports_not_installed_when_target_missing() {
        let temp = TempDir::new().unwrap();
        let host = Arc::new(FakeHost {
            home: temp.path().to_path_buf(),
        });
        let installer = KwinInstaller::with_host(host);
        let inspection = installer.inspect().expect("inspect");
        assert!(inspection.is_none());
    }

    #[test]
    fn inspect_detects_foreign_directory() {
        let temp = TempDir::new().unwrap();
        let host = Arc::new(FakeHost {
            home: temp.path().to_path_buf(),
        });
        let target = temp
            .path()
            .join(".local/share/kwin/scripts")
            .join(SCRIPT_PLUGIN_ID);
        std::fs::create_dir_all(target.join("contents/ui")).unwrap();
        std::fs::write(
            target.join("metadata.json"),
            r#"{"KPlugin": {"Id": "foreign", "Version": "1.0"}}"#,
        )
        .unwrap();
        let installer = KwinInstaller::with_host(host);
        let error = installer.inspect().expect_err("foreign detected");
        assert_eq!(error.stable_label(), "foreign_extension");
    }

    #[test]
    fn stable_label_is_deterministic() {
        let cases = [
            (KdeKwinInstallerError::HomeUnavailable, "home_unavailable"),
            (
                KdeKwinInstallerError::ForeignExtension("x".into()),
                "foreign_extension",
            ),
            (
                KdeKwinInstallerError::InvalidMetadata("x".into()),
                "invalid_metadata",
            ),
            (KdeKwinInstallerError::Io("x".into()), "io_error"),
            (
                KdeKwinInstallerError::KwinReconfigure("x".into()),
                "kwin_reconfigure_failed",
            ),
        ];
        for (error, label) in cases {
            assert_eq!(error.stable_label(), label);
        }
    }
}
