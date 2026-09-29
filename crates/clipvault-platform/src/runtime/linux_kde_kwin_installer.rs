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
//! * It refuses to write through `kwriteconfig`, `qdbus`,
//!   `kpackagetool` or any other external binary. The Tauri
//!   integration asks KWin to reload through the typed `zbus`
//!   client after the filesystem/configuration transaction.
//! * Activation / deactivation change only KWin's documented
//!   `<id>Enabled` key in the `[Plugins]` group in `kwinrc`. `kwriteconfig` is
//!   forbidden; the installer writes the file directly so the
//!   existing keys stay intact. The caller then asks KWin to reload
//!   over the session bus.
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
    pub main_js: String,
}

impl BundledKwinScript {
    pub fn from_strings(metadata_json: String, main_js: String) -> Self {
        Self {
            metadata_json,
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
    /// package and activates it. The caller asks KWin to reload over
    /// the session bus after this method returns.
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
        let code_dir = contents_dir.join("code");
        fs::create_dir_all(&code_dir)
            .map_err(|error| KdeKwinInstallerError::Io(error.to_string()))?;

        // Parse metadata up-front so a malformed bundle surfaces
        // before we touch the filesystem.
        let parsed = parse_metadata(&bundled.metadata_json)?;
        if parsed.id != SCRIPT_PLUGIN_ID {
            return Err(KdeKwinInstallerError::ForeignExtension(parsed.id));
        }

        self.migrate_legacy_enabled_key()?;

        write_atomic(&metadata_path, bundled.metadata_json.as_bytes())?;
        write_atomic(&code_dir.join("main.js"), bundled.main_js.as_bytes())?;

        // Previous ClipVault releases installed a declarative QML
        // wrapper that loaded the JavaScript file as a component.
        // Remove only that obsolete entrypoint after package ownership
        // has been checked above, so an upgraded package cannot leave
        // KWin with both old and new script entrypoints.
        let legacy_qml = contents_dir.join("ui").join("main.qml");
        match fs::remove_file(legacy_qml) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(KdeKwinInstallerError::Io(error.to_string())),
        }

        // Activate only on the first install. Re-installs leave
        // the existing `enabled` flag untouched so the call is
        // truly idempotent.
        let first_install = !self.is_enabled_in_kwinrc();
        if first_install {
            self.enable_in_kwinrc()?;
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

    /// Disable the ClipVault plugin without removing its package.
    /// The caller asks KWin to reload after this write.
    pub fn disable(&self) -> Result<KwinInstallation, KdeKwinInstallerError> {
        self.disable_in_kwinrc()?;
        Ok(self
            .inspect()?
            .unwrap_or_else(KwinInstallation::not_installed))
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
        // Disable first. The caller reloads KWin before invoking
        // this method so the script can no longer publish envelopes.
        self.disable_in_kwinrc()?;
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
        read_plugin_enabled_value(&raw, &plugin_enabled_key()).unwrap_or(false)
    }

    /// Migrate the malformed key written by previous ClipVault
    /// versions. It was stored as `Plugins/<id>Enabled` inside the
    /// `[Plugins]` group, so KWin ignored it. The migration is scoped
    /// to this plugin and preserves a correctly written value if one
    /// already exists.
    pub fn migrate_legacy_enabled_key(&self) -> Result<(), KdeKwinInstallerError> {
        let Some(path) = self.kwinrc.kwinrc_path() else {
            return Ok(());
        };
        let raw = match fs::read_to_string(&path) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(KdeKwinInstallerError::Io(error.to_string())),
        };
        let Some(legacy_value) = read_plugin_enabled_value(&raw, &legacy_plugin_enabled_key())
        else {
            return Ok(());
        };
        let enabled =
            read_plugin_enabled_value(&raw, &plugin_enabled_key()).unwrap_or(legacy_value);
        self.write_plugin_enabled_value(&path, &raw, enabled)
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
        self.write_plugin_enabled_value(&path, &raw, enabled)
    }

    fn write_plugin_enabled_value(
        &self,
        path: &Path,
        raw: &str,
        enabled: bool,
    ) -> Result<(), KdeKwinInstallerError> {
        let key = plugin_enabled_key();
        let legacy_key = legacy_plugin_enabled_key();
        let value = if enabled { "true" } else { "false" };
        let mut lines = Vec::new();
        let mut group = String::new();
        let mut found_group = false;
        let mut replaced = false;
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                group = trimmed[1..trimmed.len() - 1].to_string();
                found_group |= group == "Plugins";
                lines.push(line.to_string());
                continue;
            }
            if group == "Plugins" {
                if let Some((name, _)) = trimmed.split_once('=') {
                    match name.trim() {
                        name if name == legacy_key => continue,
                        name if name == key => {
                            lines.push(format!("{key}={value}"));
                            replaced = true;
                            continue;
                        }
                        _ => {}
                    }
                }
            }
            lines.push(line.to_string());
        }
        if !replaced {
            if found_group {
                let group_start = lines
                    .iter()
                    .position(|line| line.trim() == "[Plugins]")
                    .expect("found Plugins group");
                let insert_at = lines
                    .iter()
                    .enumerate()
                    .skip(group_start + 1)
                    .find(|(_, line)| {
                        let trimmed = line.trim();
                        trimmed.starts_with('[') && trimmed.ends_with(']')
                    })
                    .map(|(index, _)| index)
                    .unwrap_or(lines.len());
                lines.insert(insert_at, format!("{key}={value}"));
            } else {
                if !lines.is_empty() && !lines.last().is_some_and(String::is_empty) {
                    lines.push(String::new());
                }
                lines.push("[Plugins]".to_string());
                lines.push(format!("{key}={value}"));
            }
        }
        let body = lines.join("\n") + "\n";
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| KdeKwinInstallerError::Io(error.to_string()))?;
        }
        fs::write(path, body).map_err(|error| KdeKwinInstallerError::Io(error.to_string()))
    }
}

fn plugin_enabled_key() -> String {
    format!("{SCRIPT_PLUGIN_ID}Enabled")
}

fn legacy_plugin_enabled_key() -> String {
    format!("Plugins/{SCRIPT_PLUGIN_ID}Enabled")
}

fn read_plugin_enabled_value(raw: &str, key: &str) -> Option<bool> {
    let mut group = String::new();
    let mut value = None;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            group = trimmed[1..trimmed.len() - 1].to_string();
            continue;
        }
        if group != "Plugins" {
            continue;
        }
        let Some((name, raw_value)) = trimmed.split_once('=') else {
            continue;
        };
        if name.trim() != key {
            continue;
        }
        value = match raw_value.trim().to_ascii_lowercase().as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        };
    }
    value
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

/// Reload ClipVault's KWin script and reconfigure the enabled-script
/// list over the session bus. KWin keeps enabled scripts loaded across
/// `reconfigure`, so unload the owned plugin first to make disk updates
/// take effect. If KWin is offline, a future session picks up `kwinrc`
/// automatically; D-Bus errors are returned to the caller.
pub async fn request_kwin_reconfigure() -> Result<(), KdeKwinInstallerError> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| KdeKwinInstallerError::KwinReconfigure(error.to_string()))?;
    connection
        .call_method(
            Some("org.kde.KWin"),
            "/Scripting",
            Some("org.kde.kwin.Scripting"),
            "unloadScript",
            &(SCRIPT_PLUGIN_ID,),
        )
        .await
        .map_err(|error| KdeKwinInstallerError::KwinReconfigure(error.to_string()))?;
    connection
        .call_method(
            Some("org.kde.KWin"),
            "/KWin",
            Some("org.kde.KWin"),
            "reconfigure",
            &(),
        )
        .await
        .map_err(|error| KdeKwinInstallerError::KwinReconfigure(error.to_string()))?;
    Ok(())
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
    "X-Plasma-API": "javascript",
    "X-Plasma-MainScript": "code/main.js"
}"#
            .to_string(),
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
    "X-Plasma-API": "javascript",
    "X-Plasma-MainScript": "code/main.js"
}"#
            .to_string(),
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
