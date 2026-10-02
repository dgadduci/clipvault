//! Tauri lifecycle and consent adapter for the KDE KWin bridge.

#![cfg(all(target_os = "linux", feature = "linux-kde-kwin-integration"))]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clipvault_core::{AppContext, KdeKwinConsentDecision, KdeKwinIntegrationService};
use clipvault_platform::{
    request_kwin_reconfigure, ActiveApplicationProbe, BundledKwinScript, KdeKwinActiveApplication,
    KdeKwinBridgeHandle, KdeKwinCaptureToggleSink, KdeKwinError, KdeKwinInstallerError,
    KdeKwinIntegrationState as ProbeState, KdeKwinQuickPasteSink, KdeKwinShortcutStatus,
    KwinInstaller, ProbeStage, SharedKdeKwinSnapshot, KDE_KWIN_BACKEND_NAME,
    KDE_KWIN_PROTOCOL_VERSION,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::Manager;
use thiserror::Error;
use tokio::sync::Mutex as AsyncMutex;

const BUNDLED_METADATA: &str = "metadata.json";
const BUNDLED_JS: &str = "contents/code/main.js";

pub struct KdeKwinIntegrationState {
    core: Arc<KdeKwinIntegrationService>,
    installer: KwinInstaller,
    snapshot: SharedKdeKwinSnapshot,
    probe: Arc<KdeKwinActiveApplication>,
    lifecycle: AsyncMutex<()>,
    bridge: Mutex<Option<KdeKwinBridgeHandle>>,
    capture_toggle_sink: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync + 'static>>>>,
    quick_paste_sink: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync + 'static>>>>,
    shortcut_status: KdeKwinShortcutStatus,
}

#[derive(Debug, Error)]
pub enum KdeKwinShellError {
    #[error("integration is not applicable to this session")]
    NotApplicable,
    #[error("KWin integration is not consented, installed and enabled")]
    NotReady,
    #[error("consent storage failed")]
    Consent,
    #[error("installer failed")]
    Installer(#[from] KdeKwinInstallerError),
    #[error("bridge failed")]
    Bridge(#[from] KdeKwinError),
    #[error("KWin reports a shortcut conflict")]
    ShortcutConflict,
    #[error("KWin did not confirm shortcut registration")]
    ShortcutRegistration,
}

impl KdeKwinShellError {
    pub fn stable_kind(&self) -> &'static str {
        match self {
            Self::NotApplicable => "not_applicable",
            Self::NotReady => "not_ready",
            Self::Consent => "consent_error",
            Self::Installer(error) => error.stable_label(),
            Self::Bridge(error) => error.stable_label(),
            Self::ShortcutConflict => "shortcut_conflict",
            Self::ShortcutRegistration => "shortcut_registration_failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdeKwinIntegrationPayload {
    pub applicable: bool,
    pub session: String,
    pub consent: String,
    pub technical_state: String,
    pub installed: bool,
    pub enabled: bool,
    pub backend: String,
    pub protocol_version: u32,
    pub detail: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Error)]
pub enum BundledKwinScriptError {
    #[error("bundle resource missing")]
    Missing,
    #[error("bundle resource could not be read")]
    Read,
}

impl KdeKwinIntegrationState {
    pub fn new(core: Arc<KdeKwinIntegrationService>) -> Self {
        let snapshot = SharedKdeKwinSnapshot::new();
        snapshot.set_state(ProbeState::AwaitingConsent);
        Self {
            core,
            installer: KwinInstaller::new(),
            probe: Arc::new(KdeKwinActiveApplication::new(snapshot.clone())),
            snapshot,
            lifecycle: AsyncMutex::new(()),
            bridge: Mutex::new(None),
            capture_toggle_sink: Arc::new(Mutex::new(None)),
            quick_paste_sink: Arc::new(Mutex::new(None)),
            shortcut_status: Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Attach the Tauri callback after the app handle becomes available.
    /// The KWin bridge can start earlier during bootstrap, so it closes over
    /// this shared slot instead of requiring the handle during construction.
    pub fn set_capture_toggle_sink(&self, sink: Arc<dyn Fn() + Send + Sync + 'static>) {
        *self.capture_toggle_sink.lock() = Some(sink);
    }

    pub fn set_quick_paste_sink(&self, sink: Arc<dyn Fn() + Send + Sync + 'static>) {
        *self.quick_paste_sink.lock() = Some(sink);
    }

    pub fn shortcut_status(&self, action: &str) -> Option<String> {
        self.shortcut_status.lock().get(action).cloned()
    }

    /// Replace the installed KWin script's global shortcut set. The complete
    /// candidate package is written before KWin reloads it, and the script
    /// reports both registrations over the authenticated bridge. Any failure
    /// reinstalls and reloads the previous package before returning.
    pub async fn replace_shortcuts(
        &self,
        previous: &BundledKwinScript,
        candidate: &BundledKwinScript,
    ) -> Result<(), KdeKwinShellError> {
        self.ensure_applicable()?;
        let _guard = self.lifecycle.lock().await;
        let installation = self
            .installer
            .inspect()?
            .filter(|value| value.installed && value.enabled)
            .ok_or(KdeKwinShellError::NotReady)?;
        let _ = installation;
        self.shortcut_status.lock().clear();
        let result = async {
            self.installer.install(candidate)?;
            self.prepare_script_reload();
            request_kwin_reconfigure().await?;
            self.wait_for_shortcut_status().await
        }
        .await;
        if result.is_ok() {
            return Ok(());
        }

        self.shortcut_status.lock().clear();
        if let Err(error) = self.installer.install(previous) {
            self.record_installer_error(&error);
            return Err(error.into());
        }
        self.prepare_script_reload();
        if let Err(error) = request_kwin_reconfigure().await {
            self.record_reconfigure_error();
            return Err(error.into());
        }
        let _ = self.wait_for_shortcut_status().await;
        result
    }

    async fn wait_for_shortcut_status(&self) -> Result<(), KdeKwinShellError> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let status = self.shortcut_status.lock().clone();
            if status.values().any(|value| value == "conflict") {
                return Err(KdeKwinShellError::ShortcutConflict);
            }
            if status
                .get("open_quick_paste")
                .is_some_and(|value| value == "registered")
                && status
                    .get("toggle_clipboard_capture")
                    .is_some_and(|value| value == "registered")
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(KdeKwinShellError::ShortcutRegistration);
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    pub fn load_consent(&self, context: &AppContext) {
        if self.core.load_consent(context).is_err() {
            self.snapshot.set_detail(Some("consent_storage_error"));
        }
    }

    pub fn payload(&self) -> KdeKwinIntegrationPayload {
        use clipvault_platform::runtime::linux_kde_kwin_integration::{
            detect_session, KdeSessionKind,
        };

        let session_kind = detect_session();
        let applicable = matches!(session_kind, KdeSessionKind::KdePlasmaWayland);
        let session = match session_kind {
            KdeSessionKind::KdePlasmaWayland => "kde_plasma_wayland",
            KdeSessionKind::NotWayland => "not_wayland",
            KdeSessionKind::NotKde => "not_kde_wayland",
            KdeSessionKind::NonLinux => "non_linux",
        };
        let consent = self.core.cached_consent();
        let installation = self.installer.inspect();
        let (installed, enabled, inspect_error) = match installation {
            Ok(Some(value)) => (value.installed, value.enabled, None),
            Ok(None) => (false, false, None),
            Err(error) => (false, false, Some(error.stable_label().to_string())),
        };

        let technical_state = if !applicable {
            ProbeState::NotApplicable.as_str()
        } else if !matches!(consent, KdeKwinConsentDecision::Accepted) {
            if matches!(consent, KdeKwinConsentDecision::Disabled) {
                ProbeState::Disabled.as_str()
            } else {
                ProbeState::AwaitingConsent.as_str()
            }
        } else if !installed {
            ProbeState::NotInstalled.as_str()
        } else if !enabled {
            ProbeState::Disabled.as_str()
        } else {
            self.snapshot.state_str()
        };

        KdeKwinIntegrationPayload {
            applicable,
            session: session.to_string(),
            consent: consent.as_str().to_string(),
            technical_state: technical_state.to_string(),
            installed,
            enabled,
            backend: KDE_KWIN_BACKEND_NAME.to_string(),
            protocol_version: KDE_KWIN_PROTOCOL_VERSION,
            detail: self.snapshot.detail(),
            error: inspect_error.or_else(|| self.snapshot.last_error_label().map(str::to_string)),
        }
    }

    pub async fn activate(
        &self,
        context: &AppContext,
        fallback: Arc<dyn ActiveApplicationProbe>,
        bundled: &BundledKwinScript,
    ) -> Result<KdeKwinIntegrationPayload, KdeKwinShellError> {
        self.ensure_applicable()?;
        let _guard = self.lifecycle.lock().await;
        self.core
            .save_consent(context, KdeKwinConsentDecision::Accepted)
            .map_err(|_| KdeKwinShellError::Consent)?;

        if let Err(error) = self.installer.install(bundled) {
            self.record_installer_error(&error);
            return Err(error.into());
        }
        if let Err(error) = self.start_bridge().await {
            context.swap_active_app_probe(fallback);
            return Err(error);
        }
        self.prepare_script_reload();
        if let Err(error) = request_kwin_reconfigure().await {
            self.stop_bridge();
            context.swap_active_app_probe(fallback);
            self.record_reconfigure_error();
            return Err(error.into());
        }
        context.swap_active_app_probe(self.probe.clone());
        Ok(self.payload())
    }

    pub async fn decline(
        &self,
        context: &AppContext,
        fallback: Arc<dyn ActiveApplicationProbe>,
    ) -> Result<KdeKwinIntegrationPayload, KdeKwinShellError> {
        self.deactivate(context, fallback, KdeKwinConsentDecision::Declined, false)
            .await
    }

    pub async fn disable(
        &self,
        context: &AppContext,
        fallback: Arc<dyn ActiveApplicationProbe>,
    ) -> Result<KdeKwinIntegrationPayload, KdeKwinShellError> {
        self.deactivate(context, fallback, KdeKwinConsentDecision::Disabled, false)
            .await
    }

    pub async fn uninstall(
        &self,
        context: &AppContext,
        fallback: Arc<dyn ActiveApplicationProbe>,
    ) -> Result<KdeKwinIntegrationPayload, KdeKwinShellError> {
        self.deactivate(context, fallback, KdeKwinConsentDecision::Disabled, true)
            .await
    }

    pub async fn retry(
        &self,
        context: &AppContext,
        fallback: Arc<dyn ActiveApplicationProbe>,
        bundled: &BundledKwinScript,
    ) -> Result<KdeKwinIntegrationPayload, KdeKwinShellError> {
        self.ensure_applicable()?;
        let _guard = self.lifecycle.lock().await;
        if !matches!(self.core.cached_consent(), KdeKwinConsentDecision::Accepted) {
            return Err(KdeKwinShellError::NotApplicable);
        }
        let installation = self
            .installer
            .inspect()?
            .filter(|value| value.installed)
            .ok_or(KdeKwinShellError::NotReady)?;
        self.installer.migrate_legacy_enabled_key()?;
        let installation = self.installer.inspect()?.unwrap_or(installation);
        if !installation.enabled {
            return Err(KdeKwinShellError::NotReady);
        }
        if let Err(error) = self.installer.install(bundled) {
            self.record_installer_error(&error);
            return Err(error.into());
        }
        if let Err(error) = self.start_bridge().await {
            context.swap_active_app_probe(fallback);
            return Err(error);
        }
        self.prepare_script_reload();
        if let Err(error) = request_kwin_reconfigure().await {
            self.stop_bridge();
            context.swap_active_app_probe(fallback);
            self.record_reconfigure_error();
            return Err(error.into());
        }
        context.swap_active_app_probe(self.probe.clone());
        Ok(self.payload())
    }

    pub async fn reactivate_if_consented(
        &self,
        context: &AppContext,
        fallback: Arc<dyn ActiveApplicationProbe>,
        bundled: &BundledKwinScript,
    ) -> Result<(), KdeKwinShellError> {
        if !self.is_applicable() {
            return Ok(());
        }
        let _guard = self.lifecycle.lock().await;
        let decision = self
            .core
            .load_consent(context)
            .map_err(|_| KdeKwinShellError::Consent)?;
        if !matches!(decision, KdeKwinConsentDecision::Accepted) {
            return Ok(());
        }
        let Some(installation) = self.installer.inspect()? else {
            self.snapshot.set_state(ProbeState::NotInstalled);
            return Ok(());
        };
        self.installer.migrate_legacy_enabled_key()?;
        let installation = self.installer.inspect()?.unwrap_or(installation);
        if !installation.enabled {
            self.snapshot.set_state(ProbeState::Disabled);
            return Ok(());
        }
        if let Err(error) = self.installer.install(bundled) {
            self.record_installer_error(&error);
            return Err(error.into());
        }
        if let Err(error) = self.start_bridge().await {
            context.swap_active_app_probe(fallback);
            return Err(error);
        }
        self.prepare_script_reload();
        if let Err(error) = request_kwin_reconfigure().await {
            self.stop_bridge();
            context.swap_active_app_probe(fallback);
            self.record_reconfigure_error();
            return Err(error.into());
        }
        context.swap_active_app_probe(self.probe.clone());
        Ok(())
    }

    fn ensure_applicable(&self) -> Result<(), KdeKwinShellError> {
        if self.is_applicable() {
            Ok(())
        } else {
            Err(KdeKwinShellError::NotApplicable)
        }
    }

    fn is_applicable(&self) -> bool {
        use clipvault_platform::runtime::linux_kde_kwin_integration::{
            detect_session, KdeSessionKind,
        };
        matches!(detect_session(), KdeSessionKind::KdePlasmaWayland)
    }

    async fn start_bridge(&self) -> Result<(), KdeKwinShellError> {
        if self.bridge.lock().is_some() {
            return Ok(());
        }
        self.snapshot.set_state(ProbeState::ActivationPending);
        let sink_slot = Arc::clone(&self.capture_toggle_sink);
        let sink: KdeKwinCaptureToggleSink = Arc::new(move || {
            let callback = sink_slot.lock().clone();
            if let Some(callback) = callback {
                callback();
            }
        });
        let quick_paste_slot = Arc::clone(&self.quick_paste_sink);
        let quick_paste: KdeKwinQuickPasteSink = Arc::new(move || {
            let callback = quick_paste_slot.lock().clone();
            if let Some(callback) = callback {
                callback();
            }
        });
        match clipvault_platform::kde_kwin_start_bridge_with_shortcuts(
            self.snapshot.clone(),
            Some(sink),
            Some(quick_paste),
            self.shortcut_status.clone(),
        )
        .await
        {
            Ok(handle) => {
                *self.bridge.lock() = Some(handle);
                Ok(())
            }
            Err(error) => {
                self.snapshot.set_state(ProbeState::CommunicationError);
                self.snapshot
                    .set_last_error(Some(KdeKwinError::Dbus(error.stable_label().to_string())));
                Err(error.into())
            }
        }
    }

    fn prepare_script_reload(&self) {
        self.snapshot.set_active_app_id(None);
        self.snapshot.set_probe_stage(ProbeStage::Started);
        self.snapshot.set_state(ProbeState::ActivationPending);
    }

    fn stop_bridge(&self) {
        self.bridge.lock().take();
        self.snapshot.set_active_app_id(None);
    }

    async fn deactivate(
        &self,
        context: &AppContext,
        fallback: Arc<dyn ActiveApplicationProbe>,
        decision: KdeKwinConsentDecision,
        remove_package: bool,
    ) -> Result<KdeKwinIntegrationPayload, KdeKwinShellError> {
        self.ensure_applicable()?;
        let _guard = self.lifecycle.lock().await;
        self.core
            .save_consent(context, decision)
            .map_err(|_| KdeKwinShellError::Consent)?;

        let previous = match self.installer.inspect() {
            Ok(previous) => previous,
            Err(error) => {
                self.stop_bridge();
                context.swap_active_app_probe(fallback);
                self.record_installer_error(&error);
                return Err(error.into());
            }
        };
        if previous.is_some() {
            if let Err(error) = self.installer.disable() {
                self.stop_bridge();
                context.swap_active_app_probe(fallback);
                self.record_installer_error(&error);
                return Err(error.into());
            }
        }
        self.stop_bridge();
        context.swap_active_app_probe(fallback);

        if previous.is_some() {
            if let Err(error) = request_kwin_reconfigure().await {
                self.record_reconfigure_error();
                return Err(error.into());
            }
        }
        if remove_package && previous.is_some() {
            if let Err(error) = self.installer.uninstall() {
                self.record_installer_error(&error);
                return Err(error.into());
            }
        }
        self.snapshot.set_state(if remove_package {
            ProbeState::NotInstalled
        } else {
            ProbeState::Disabled
        });
        Ok(self.payload())
    }

    fn record_installer_error(&self, error: &KdeKwinInstallerError) {
        self.snapshot.set_state(ProbeState::CommunicationError);
        self.snapshot
            .set_detail(Some(error.stable_label().to_string()));
        self.snapshot
            .set_last_error(Some(KdeKwinError::Dbus(error.stable_label().to_string())));
    }

    fn record_reconfigure_error(&self) {
        self.snapshot.set_state(ProbeState::CommunicationError);
        self.snapshot.set_detail(Some("kwin_reconfigure_failed"));
        self.snapshot.set_last_error(Some(KdeKwinError::Dbus(
            "kwin_reconfigure_failed".to_string(),
        )));
    }
}

pub fn read_bundled_script(
    handle: &tauri::AppHandle,
) -> Result<BundledKwinScript, BundledKwinScriptError> {
    let resolver = handle.path();
    let resource_dir = resolver
        .resource_dir()
        .map_err(|_| BundledKwinScriptError::Missing)?;
    let metadata =
        find_resource(&resource_dir, BUNDLED_METADATA).ok_or(BundledKwinScriptError::Missing)?;
    let js = find_resource(&resource_dir, BUNDLED_JS).ok_or(BundledKwinScriptError::Missing)?;
    let read =
        |path: PathBuf| std::fs::read_to_string(path).map_err(|_| BundledKwinScriptError::Read);
    Ok(BundledKwinScript::from_strings(read(metadata)?, read(js)?))
}

/// Apply the persisted global bindings to the bundled KWin script. KWin owns
/// the global grabs, so it receives the complete two-action set whenever a
/// script reload is needed.
pub fn configure_bundled_shortcuts(
    bundled: &BundledKwinScript,
    shortcuts: &[clipvault_core::HotkeySpec],
    language: &str,
) -> Result<BundledKwinScript, String> {
    let open = shortcuts
        .iter()
        .find(|binding| binding.id == "open_quick_paste")
        .ok_or_else(|| "open shortcut missing".to_string())?;
    let toggle = shortcuts
        .iter()
        .find(|binding| binding.id == "toggle_clipboard_capture")
        .ok_or_else(|| "capture shortcut missing".to_string())?;
    let open_sequence = kwin_key_sequence(open)?;
    let toggle_sequence = kwin_key_sequence(toggle)?;
    let mut main_js = bundled.main_js.clone();
    main_js = replace_shortcut_constant(
        &main_js,
        "CLIPVAULT_QUICK_PASTE_ACCELERATOR",
        "Ctrl+Shift+V",
        &open_sequence,
    )?;
    main_js = replace_shortcut_constant(
        &main_js,
        "CLIPVAULT_CAPTURE_TOGGLE_ACCELERATOR",
        "Ctrl+Alt+Shift+B",
        &toggle_sequence,
    )?;
    main_js = replace_string_constant(
        &main_js,
        "CLIPVAULT_QUICK_PASTE_DESCRIPTION",
        "Open ClipVault QuickVault",
        &crate::localization::text(language, "keyboard_shortcuts.action.open_quick_paste"),
    )?;
    main_js = replace_string_constant(
        &main_js,
        "CLIPVAULT_CAPTURE_TOGGLE_DESCRIPTION",
        "Toggle ClipVault local clipboard capture",
        &crate::localization::text(language, "keyboard_shortcuts.action.toggle_capture"),
    )?;
    Ok(BundledKwinScript::from_strings(
        bundled.metadata_json.clone(),
        main_js,
    ))
}

fn replace_string_constant(
    source: &str,
    name: &str,
    default: &str,
    replacement: &str,
) -> Result<String, String> {
    let needle = format!("const {name} = {default:?};");
    if !source.contains(&needle) {
        return Err("KWin shortcut template is incompatible".to_string());
    }
    let value = serde_json::to_string(replacement).map_err(|_| "invalid shortcut description")?;
    Ok(source.replace(&needle, &format!("const {name} = {value};")))
}

fn replace_shortcut_constant(
    source: &str,
    name: &str,
    default: &str,
    replacement: &str,
) -> Result<String, String> {
    let needle = format!("const {name} = \"{default}\";");
    if !source.contains(&needle) {
        return Err("KWin shortcut template is incompatible".to_string());
    }
    Ok(source.replace(&needle, &format!("const {name} = \"{replacement}\";")))
}

fn kwin_key_sequence(binding: &clipvault_core::HotkeySpec) -> Result<String, String> {
    let mut parts = Vec::new();
    if binding.cmd_or_ctrl {
        parts.push("Ctrl".to_string());
    }
    if binding.meta {
        parts.push("Meta".to_string());
    }
    if binding.alt {
        parts.push("Alt".to_string());
    }
    if binding.shift {
        parts.push("Shift".to_string());
    }
    let key = match binding.key.to_ascii_lowercase().as_str() {
        "enter" => "Return".to_string(),
        "escape" => "Esc".to_string(),
        "space" => "Space".to_string(),
        value if value.len() == 1 && value.as_bytes()[0].is_ascii_alphanumeric() => {
            value.to_ascii_uppercase()
        }
        _ => return Err("unsupported KWin key".to_string()),
    };
    parts.push(key);
    Ok(parts.join("+"))
}

#[cfg(test)]
mod keyboard_shortcut_tests {
    use super::*;

    #[test]
    fn bundled_shortcuts_apply_bindings_and_selected_locale_descriptions() {
        let template = BundledKwinScript::from_strings(
            "{}".to_string(),
            include_str!("../resources/kde-kwin-script/contents/code/main.js").to_string(),
        );
        let mut shortcuts = clipvault_core::keyboard_shortcuts::default_keyboard_shortcuts();
        let open = shortcuts
            .iter_mut()
            .find(|binding| binding.id == "open_quick_paste")
            .expect("open QuickVault shortcut");
        open.key = "q".to_string();
        let configured = configure_bundled_shortcuts(&template, &shortcuts, "es")
            .expect("valid configured KWin script");

        assert!(configured
            .main_js
            .contains("const CLIPVAULT_QUICK_PASTE_ACCELERATOR = \"Ctrl+Shift+Q\";"));
        assert!(configured
            .main_js
            .contains("const CLIPVAULT_QUICK_PASTE_DESCRIPTION = \"Abrir QuickVault\";"));
        assert!(configured.main_js.contains(
            "const CLIPVAULT_CAPTURE_TOGGLE_DESCRIPTION = \"Pausar o reanudar la captura del portapapeles\";"
        ));
    }
}

fn find_resource(resource_dir: &std::path::Path, relative: &str) -> Option<PathBuf> {
    [
        resource_dir.join("kde-kwin-script").join(relative),
        resource_dir.join(relative),
        resource_dir
            .join("resources/kde-kwin-script")
            .join(relative),
        resource_dir.join("resources").join(relative),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}
