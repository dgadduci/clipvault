//! Tauri-facing glue for the GNOME Shell integration.
//!
//! The shell layer stores the per-process state on `AppHandle::manage`
//! and exposes a small surface to the Tauri commands. The runtime
//! pieces (listener thread, platform service handle) are kept here
//! because the listener owns an `AppHandle` clone it never uses, and
//! that clone would otherwise leak through the platform crate.

#![cfg(all(target_os = "linux", feature = "linux-gnome-shell-integration"))]

use std::path::PathBuf;
use std::sync::Arc;

use clipvault_core::{
    GnomeConsentDecision, GnomeIntegrationService as CoreIntegrationService, GnomeTechnicalState,
};
use clipvault_platform::{
    spawn_listener_thread_with_socket, GnomeConsentDecision as PlatformConsentDecision,
    GnomeIntegrationService as PlatformIntegrationService, ListenerHandle, SharedGnomeSnapshot,
    UnixListenerTransport, GNOME_BACKEND_NAME, GNOME_EXTENSION_UUID, GNOME_PROTOCOL_VERSION,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::Manager;
use tracing::warn;

const BUNDLE_DIR: &str = "gnome-integration";

/// Resource paths the Tauri shell embeds through the resource bundle.
pub const BUNDLED_METADATA_PATH: &str = "gnome-extension/metadata.json";
pub const BUNDLED_EXTENSION_PATH: &str = "gnome-extension/extension.js";

/// Shared state managed by Tauri's `manage()`.
pub struct GnomeIntegrationState {
    core_service: Arc<CoreIntegrationService>,
    live: Mutex<Option<LiveGnomeHandle>>,
}

/// Bundle the state keeps around the live probe + listener.
pub struct LiveGnomeHandle {
    pub platform_service: Arc<PlatformIntegrationService>,
    pub snapshot: SharedGnomeSnapshot,
    pub probe: Arc<clipvault_platform::GnomeShellActiveApplication>,
    pub listener: Option<ListenerHandle>,
    pub socket_path: Option<PathBuf>,
}

impl Drop for LiveGnomeHandle {
    fn drop(&mut self) {
        if let Some(handle) = self.listener.take() {
            handle.shutdown();
        }
    }
}

impl GnomeIntegrationState {
    pub fn new(core_service: Arc<CoreIntegrationService>) -> Self {
        Self {
            core_service,
            live: Mutex::new(None),
        }
    }

    /// Persist a consent decision through the core service. The helper
    /// also forwards the value to the platform service when a live
    /// handle exists so the listener observes the new decision.
    pub fn record_consent(
        &self,
        context: &clipvault_core::AppContext,
        decision: GnomeConsentDecision,
    ) -> Result<Option<GnomeConsentDecision>, clipvault_core::GnomeIntegrationError> {
        let previous = self.core_service.save_consent(context, decision)?;
        if let Some(handle) = self.live.lock().as_ref() {
            handle
                .platform_service
                .set_consent(convert_consent(decision));
        }
        Ok(previous)
    }

    pub fn record_technical_state(
        &self,
        context: &clipvault_core::AppContext,
        state: GnomeTechnicalState,
    ) -> Result<(), clipvault_core::GnomeIntegrationError> {
        self.core_service.save_technical_state(context, state)
    }

    pub fn load_consent(
        &self,
        context: &clipvault_core::AppContext,
    ) -> Result<GnomeConsentDecision, clipvault_core::GnomeIntegrationError> {
        self.core_service.load_consent(context)
    }

    /// Build (or fetch) the live platform service. Builds a fresh one
    /// on the first call; subsequent calls share the same instance so
    /// the listener and the install state stay in lock-step.
    pub fn ensure_platform_service(
        &self,
        consent: GnomeConsentDecision,
    ) -> Arc<PlatformIntegrationService> {
        if let Some(handle) = self.live.lock().as_ref() {
            return handle.platform_service.clone();
        }
        let service = Arc::new(PlatformIntegrationService::new(convert_consent(consent)));
        service.refresh_session_state();
        let snapshot = service.snapshot().clone();
        let probe = Arc::new(clipvault_platform::GnomeShellActiveApplication::new(
            snapshot.clone(),
        ));
        *self.live.lock() = Some(LiveGnomeHandle {
            platform_service: service.clone(),
            snapshot,
            probe,
            listener: None,
            socket_path: service.socket_path(),
        });
        service
    }

    /// Install the bundled extension. Records `accepted` first so the
    /// audit trail captures the user's choice even if the install
    /// fails.
    pub fn install(
        &self,
        context: &clipvault_core::AppContext,
        bundled: &BundledBytes,
    ) -> Result<InstallResult, InstallError> {
        self.record_consent(context, GnomeConsentDecision::Accepted)
            .map_err(|error| InstallError::Consent(error.to_string()))?;
        let platform_service = self.ensure_platform_service(GnomeConsentDecision::Accepted);
        let outcome = platform_service
            .install(&bundled.metadata_json, &bundled.extension_js)
            .map_err(|error| InstallError::Installer(error.to_string()))?;
        let platform_state = platform_service.snapshot().state();
        let technical_state = convert_technical_state_from_platform(platform_state);
        self.record_technical_state(context, technical_state)
            .map_err(|error| InstallError::Consent(error.to_string()))?;
        // Hot-swap the cached probe to the GNOME-backed one. Until
        // the listener accepts the peer's handshake the new probe
        // surfaces `Unavailable`; once the first `app_id` lands the
        // capture loop observes it without a restart.
        if let Some(handle) = self.live.lock().as_ref() {
            context.swap_active_app_probe(
                handle.probe.clone() as Arc<dyn clipvault_platform::ActiveApplicationProbe>
            );
        }
        Ok(InstallResult {
            installation: outcome.installation,
            status: platform_service.status(),
            diagnostics: platform_service.diagnostics(),
        })
    }

    /// Uninstall the bundled extension. Records `disabled` so the
    /// consent persists even after the directory is removed.
    pub fn uninstall(
        &self,
        context: &clipvault_core::AppContext,
    ) -> Result<clipvault_platform::Installation, InstallError> {
        self.record_consent(context, GnomeConsentDecision::Disabled)
            .map_err(|error| InstallError::Consent(error.to_string()))?;
        let platform_service = match self.live.lock().as_ref() {
            Some(handle) => handle.platform_service.clone(),
            None => self.ensure_platform_service(GnomeConsentDecision::Disabled),
        };
        let outcome = platform_service
            .uninstall()
            .map_err(|error| InstallError::Installer(error.to_string()))?;
        let technical_state =
            convert_technical_state_from_platform(platform_service.snapshot().state());
        self.record_technical_state(context, technical_state)
            .map_err(|error| InstallError::Consent(error.to_string()))?;
        Ok(outcome)
    }

    /// Spawn the listener thread. Returns Ok even if the listener was
    /// already running. The first call to `record_consent(accepted)`
    /// + `install` must already have populated the platform service.
    pub fn start_listener(&self) -> Result<(), String> {
        let mut guard = self.live.lock();
        let Some(handle) = guard.as_mut() else {
            return Err("integration not installed".to_string());
        };
        if handle.listener.is_some() {
            return Ok(());
        }
        let socket_path = match handle.socket_path.clone() {
            Some(path) => path,
            None => return Err("socket path is not available on this session".to_string()),
        };
        let transport = match UnixListenerTransport::bind(&socket_path) {
            Ok(transport) => Arc::new(transport),
            Err(error) => {
                warn!(error = %error, "gnome listener bind failed");
                return Err(format!("bind: {error}"));
            }
        };
        handle.listener = Some(spawn_listener_thread_with_socket(
            handle.snapshot.clone(),
            transport,
            socket_path,
        ));
        Ok(())
    }

    /// Stop the listener thread (if any). Safe to call repeatedly.
    pub fn stop_listener(&self) {
        let mut guard = self.live.lock();
        if let Some(handle) = guard.as_mut() {
            if let Some(listener) = handle.listener.take() {
                listener.shutdown();
            }
        }
    }

    /// Build the public payload every GNOME Tauri command returns.
    /// When the host is not Linux, the helper collapses to
    /// [`GnomeIntegrationPayload::not_applicable`]. On a Linux host
    /// the payload reflects the persisted consent and the session
    /// even when no listener is live: the first launch must already
    /// surface `applicable = true` so the consent prompt is reachable
    /// from the Development modal. The payload never carries absolute
    /// paths or any other identifier that could fingerprint the host.
    pub fn payload(&self) -> GnomeIntegrationPayload {
        let (session, desktop) = clipvault_platform::gnome_detect_session();
        if matches!(session, clipvault_platform::GnomeSessionKind::NonLinux) {
            return GnomeIntegrationPayload::not_applicable();
        }
        let guard = self.live.lock();
        if let Some(handle) = guard.as_ref() {
            let status = handle.platform_service.status();
            let consent = handle.platform_service.consent();
            let technical_state = convert_technical_state_from_platform(handle.snapshot.state());
            return GnomeIntegrationPayload::from_parts(
                session,
                desktop,
                convert_consent_back(consent),
                technical_state,
                status.installed,
                status.identifier,
                status.detail,
            );
        }
        // No live handle yet: surface the session but keep the
        // technical state at the conservative default so the UI
        // renders the consent prompt instead of a misleading
        // "connected" status. The live handle is created on demand
        // the first time the user accepts the integration; the
        // listener and socket stay unbound until then.
        let stored_consent = self.core_service.load_consent_from_cache();
        let technical_state = self.core_service.load_technical_state_from_cache();
        GnomeIntegrationPayload::from_parts(
            session,
            desktop,
            stored_consent,
            technical_state,
            false,
            None,
            None,
        )
    }

    /// Rebuild the live probe, listener thread and probe swap when
    /// the persisted consent is `accepted` AND the extension is
    /// installed. Idempotent — restarting ClipVault lands on this
    /// branch, the live probe goes back behind the cached active-app
    /// probe and the listener keeps the GNOME source authoritative
    /// without re-running the install flow.
    pub fn reactivate_if_consented(
        &self,
        context: &clipvault_core::AppContext,
    ) -> Result<(), InstallError> {
        let consent = self
            .core_service
            .load_consent(context)
            .map_err(|error| InstallError::Consent(error.to_string()))?;
        if !matches!(consent, GnomeConsentDecision::Accepted) {
            return Ok(());
        }
        let platform_service = self.ensure_platform_service(consent);
        let installation = platform_service.installation();
        if !installation.installed {
            self.record_technical_state(context, GnomeTechnicalState::NotInstalled)
                .map_err(|error| InstallError::Consent(error.to_string()))?;
            return Ok(());
        }
        // Start the listener BEFORE swapping the probe so a bind
        // failure never leaves the cached probe pointing at a
        // never-connected backend.
        if let Err(error) = self.start_listener() {
            warn!(error = %error, "gnome listener restart failed");
            return Err(InstallError::Installer(error));
        }
        let probe = {
            let guard = self.live.lock();
            match guard.as_ref() {
                Some(handle) => handle.probe.clone(),
                None => return Ok(()),
            }
        };
        context.swap_active_app_probe(probe);
        let technical_state =
            convert_technical_state_from_platform(platform_service.snapshot().state());
        self.record_technical_state(context, technical_state)
            .map_err(|error| InstallError::Consent(error.to_string()))?;
        Ok(())
    }
}

/// Payload every GNOME-related Tauri command returns. The struct is
/// metadata-only: it never carries absolute filesystem paths,
/// clipboard content, process identifiers, asset references or
/// hashes. The previous prototype leaked the install target
/// directory and the listener socket path; both fields are gone in
/// this revision.
#[derive(Debug, Serialize, Deserialize)]
pub struct GnomeIntegrationPayload {
    pub applicable: bool,
    pub session: String,
    pub desktop: String,
    pub consent: String,
    pub technical_state: String,
    pub installed: bool,
    pub identifier: Option<String>,
    pub detail: Option<String>,
    pub uuid: String,
    pub backend: String,
    pub protocol_version: u16,
}

impl GnomeIntegrationPayload {
    pub fn not_applicable() -> Self {
        Self {
            applicable: false,
            session: "non_linux".to_string(),
            desktop: "unknown".to_string(),
            consent: GnomeConsentDecision::Unknown.as_str().to_string(),
            technical_state: GnomeTechnicalState::NotInstalled.as_str().to_string(),
            installed: false,
            identifier: None,
            detail: None,
            uuid: GNOME_EXTENSION_UUID.to_string(),
            backend: GNOME_BACKEND_NAME.to_string(),
            protocol_version: GNOME_PROTOCOL_VERSION,
        }
    }

    /// Build the payload from explicit parts. Lets the Tauri commands
    /// emit a stable shape without touching the platform service
    /// directly.
    pub fn from_parts(
        session: clipvault_platform::GnomeSessionKind,
        desktop: clipvault_platform::GnomeDesktopEnvironment,
        consent: GnomeConsentDecision,
        technical_state: GnomeTechnicalState,
        installed: bool,
        identifier: Option<String>,
        detail: Option<String>,
    ) -> Self {
        let applicable = matches!(
            (session, desktop),
            (
                clipvault_platform::GnomeSessionKind::LinuxWayland,
                clipvault_platform::GnomeDesktopEnvironment::Gnome
            )
        );
        Self {
            applicable,
            session: session_label(session),
            desktop: desktop_label(desktop),
            consent: consent.as_str().to_string(),
            technical_state: technical_state.as_str().to_string(),
            installed,
            identifier,
            detail,
            uuid: GNOME_EXTENSION_UUID.to_string(),
            backend: GNOME_BACKEND_NAME.to_string(),
            protocol_version: GNOME_PROTOCOL_VERSION,
        }
    }
}

/// Bytes that survived the resource resolution.
#[derive(Debug, Clone)]
pub struct BundledBytes {
    pub metadata_json: String,
    pub extension_js: String,
}

impl BundledBytes {
    pub fn new(metadata_json: String, extension_js: String) -> Self {
        Self {
            metadata_json,
            extension_js,
        }
    }
}

#[derive(Debug)]
pub enum BundledError {
    MetadataMissing,
    ExtensionMissing,
    MetadataRead(String),
    ExtensionRead(String),
}

impl std::fmt::Display for BundledError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BundledError::MetadataMissing => write!(f, "metadata.json was not bundled"),
            BundledError::ExtensionMissing => write!(f, "extension.js was not bundled"),
            BundledError::MetadataRead(details) => write!(f, "metadata read failed: {details}"),
            BundledError::ExtensionRead(details) => write!(f, "extension read failed: {details}"),
        }
    }
}

impl std::error::Error for BundledError {}

/// Resolve the bundled extension path inside Tauri's resource dir.
pub fn bundled_resource_path(handle: &tauri::AppHandle, resource: &str) -> Option<PathBuf> {
    let resolver = handle.path();
    let resource_dir = resolver.resource_dir().ok()?;
    bundled_resource_path_from(&resource_dir, resource)
}

/// Find a resource inside one of Tauri's supported on-disk layouts.
///
/// `cargo tauri dev` copies a configured path such as
/// `resources/gnome-extension/metadata.json` beneath
/// `<resource_dir>/resources/`. Linux bundles can expose configured
/// resources directly below `<resource_dir>`. Neither layout may fall
/// back to the source checkout: release binaries must only execute the
/// bytes that were embedded in their own resource directory.
fn bundled_resource_path_from(resource_dir: &std::path::Path, resource: &str) -> Option<PathBuf> {
    [
        resource_dir.join(BUNDLE_DIR).join(resource),
        resource_dir.join(resource),
        resource_dir.join("resources").join(resource),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

/// Read the bundled extension resources from disk.
pub fn read_bundled_extension(handle: &tauri::AppHandle) -> Result<BundledBytes, BundledError> {
    let metadata = bundled_resource_path(handle, BUNDLED_METADATA_PATH)
        .ok_or(BundledError::MetadataMissing)?;
    let extension = bundled_resource_path(handle, BUNDLED_EXTENSION_PATH)
        .ok_or(BundledError::ExtensionMissing)?;
    let metadata_bytes = std::fs::read_to_string(&metadata)
        .map_err(|error| BundledError::MetadataRead(format!("{}: {error}", metadata.display())))?;
    let extension_bytes = std::fs::read_to_string(&extension).map_err(|error| {
        BundledError::ExtensionRead(format!("{}: {error}", extension.display()))
    })?;
    Ok(BundledBytes::new(metadata_bytes, extension_bytes))
}

/// Outcome the install command returns to the Tauri command.
#[derive(Debug, Serialize)]
pub struct InstallResult {
    pub installation: clipvault_platform::Installation,
    pub status: clipvault_platform::GnomeIntegrationStatus,
    pub diagnostics: clipvault_platform::GnomeDiagnostics,
}

#[derive(Debug)]
pub enum InstallError {
    Consent(String),
    Installer(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::Consent(message) => write!(f, "consent: {message}"),
            InstallError::Installer(message) => write!(f, "installer: {message}"),
        }
    }
}

impl std::error::Error for InstallError {}

fn session_label(session: clipvault_platform::GnomeSessionKind) -> String {
    match session {
        clipvault_platform::GnomeSessionKind::NonLinux => "non_linux",
        clipvault_platform::GnomeSessionKind::LinuxX11 => "linux_x11",
        clipvault_platform::GnomeSessionKind::LinuxWayland => "linux_wayland",
        clipvault_platform::GnomeSessionKind::LinuxUnknown => "linux_unknown",
    }
    .to_string()
}

fn desktop_label(desktop: clipvault_platform::GnomeDesktopEnvironment) -> String {
    match desktop {
        clipvault_platform::GnomeDesktopEnvironment::Gnome => "gnome",
        clipvault_platform::GnomeDesktopEnvironment::Unknown => "unknown",
    }
    .to_string()
}

fn convert_consent(decision: GnomeConsentDecision) -> PlatformConsentDecision {
    match decision {
        GnomeConsentDecision::Unknown => PlatformConsentDecision::Unknown,
        GnomeConsentDecision::Accepted => PlatformConsentDecision::Accepted,
        GnomeConsentDecision::Declined => PlatformConsentDecision::Declined,
        GnomeConsentDecision::Disabled => PlatformConsentDecision::Disabled,
    }
}

fn convert_consent_back(decision: PlatformConsentDecision) -> GnomeConsentDecision {
    match decision {
        PlatformConsentDecision::Unknown => GnomeConsentDecision::Unknown,
        PlatformConsentDecision::Accepted => GnomeConsentDecision::Accepted,
        PlatformConsentDecision::Declined => GnomeConsentDecision::Declined,
        PlatformConsentDecision::Disabled => GnomeConsentDecision::Disabled,
    }
}

fn convert_technical_state_from_platform(
    state: clipvault_platform::GnomeIntegrationState,
) -> GnomeTechnicalState {
    use clipvault_platform::GnomeIntegrationState as PlatformState;
    match state {
        PlatformState::GnomeNotDetected | PlatformState::NotWayland => {
            GnomeTechnicalState::Unavailable
        }
        PlatformState::NotInstalled => GnomeTechnicalState::NotInstalled,
        PlatformState::Disabled => GnomeTechnicalState::Disabled,
        PlatformState::Incompatible => GnomeTechnicalState::Incompatible,
        PlatformState::ActivationPending => GnomeTechnicalState::ActivationPending,
        PlatformState::Connected => GnomeTechnicalState::Connected,
        PlatformState::NoActiveApplication => GnomeTechnicalState::NoActiveApplication,
        PlatformState::Identified => GnomeTechnicalState::Identified,
        PlatformState::Disconnected => GnomeTechnicalState::Disconnected,
        PlatformState::CommunicationError => GnomeTechnicalState::CommunicationError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn not_applicable_payload_carries_protocol_constants() {
        let payload = GnomeIntegrationPayload::not_applicable();
        assert_eq!(payload.protocol_version, GNOME_PROTOCOL_VERSION);
        assert_eq!(payload.backend, GNOME_BACKEND_NAME);
        assert!(!payload.applicable);
        assert!(payload.identifier.is_none());
        assert!(payload.detail.is_none());
    }

    #[test]
    fn payload_does_not_leak_absolute_paths() {
        let payload = GnomeIntegrationPayload::not_applicable();
        let raw = serde_json::to_string(&payload).unwrap();
        for forbidden in [
            "install_target_dir",
            "socket_path",
            "target_dir",
            "/run/",
            "/tmp/",
            "XDG_RUNTIME_DIR",
            "/home/",
            "pid",
            "PID",
            "/proc/",
        ] {
            assert!(
                !raw.contains(forbidden),
                "payload leaked {forbidden:?} in {raw}"
            );
        }
    }

    #[test]
    fn bundled_bytes_round_trip() {
        let bundle = BundledBytes::new("meta".to_string(), "code".to_string());
        assert_eq!(bundle.metadata_json, "meta");
        assert_eq!(bundle.extension_js, "code");
    }

    #[test]
    fn bundled_resource_resolution_supports_tauri_dev_resources_layout() {
        let temp = TempDir::new().expect("tempdir");
        let resource = temp.path().join("resources").join(BUNDLED_METADATA_PATH);
        std::fs::create_dir_all(resource.parent().expect("resource parent"))
            .expect("create resource parent");
        std::fs::write(&resource, "{}\n").expect("write bundled metadata");

        assert_eq!(
            bundled_resource_path_from(temp.path(), BUNDLED_METADATA_PATH),
            Some(resource)
        );
    }

    #[test]
    fn from_parts_reflects_session() {
        let payload = GnomeIntegrationPayload::from_parts(
            clipvault_platform::GnomeSessionKind::NonLinux,
            clipvault_platform::GnomeDesktopEnvironment::Unknown,
            GnomeConsentDecision::Unknown,
            GnomeTechnicalState::NotInstalled,
            false,
            None,
            None,
        );
        assert!(!payload.applicable);
        assert_eq!(payload.session, "non_linux");
        assert_eq!(payload.desktop, "unknown");
    }

    #[test]
    fn from_parts_marks_linux_wayland_gnome_as_applicable() {
        let payload = GnomeIntegrationPayload::from_parts(
            clipvault_platform::GnomeSessionKind::LinuxWayland,
            clipvault_platform::GnomeDesktopEnvironment::Gnome,
            GnomeConsentDecision::Accepted,
            GnomeTechnicalState::ActivationPending,
            true,
            Some("firefox.desktop".to_string()),
            None,
        );
        assert!(payload.applicable);
        assert_eq!(payload.session, "linux_wayland");
        assert_eq!(payload.desktop, "gnome");
        assert_eq!(payload.identifier.as_deref(), Some("firefox.desktop"));
    }
}
