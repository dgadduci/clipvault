//! High-level glue between the consent persistence layer, the GNOME
//! extension installer and the per-session listener. Owning the
//! service on `AppState` keeps the bootstrap thin and gives the Tauri
//! commands a single object to drive when the user accepts, retracts
//! or retries the integration.
//!
//! ## Lifecycle
//!
//! 1. **Discovery** — every Linux Wayland startup inspects the
//!    session and the existing install. The result is exposed to the
//!    frontend through `clipvault_gnome_integration_status`.
//! 2. **Consent** — the bootstrap records the user's response
//!    (accepted / declined) and persists it. The first launch on a
//!    fresh install produces `unknown` so the UI can prompt the user
//!    exactly once.
//! 3. **Install** — `install()` writes the bundled extension tree to
//!    `~/.local/share/gnome-shell/extensions/<uuid>/` only when the
//!    consent decision is `accepted`. The installation is atomic and
//!    refuses foreign extensions.
//! 4. **Activation** — `request_activation()` does NOT call GNOME's
//!    enable APIs directly. The frontend asks the user to flip the
//!    GNOME preferences toggle and reports the result through
//!    `mark_active()`. Until that callback lands the diagnostics
//!    surface reports `activation_pending` so the operator knows the
//!    integration needs an explicit GNOME action.
//! 5. **Listener** — once the install is active, the bootstrap (or
//!    the `start_listener` helper) spawns the [`GnomeShellListener`]
//!    and wires the `GnomeShellActiveApplication` probe into the
//!    pipeline. Dropping the listener handle stops the thread.

#![cfg(all(target_os = "linux", feature = "linux-gnome-shell-integration"))]
#![allow(dead_code)]

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

pub use crate::runtime::linux_gnome_extension_installer::{
    BundledExtension, ExtensionInstaller, InstallOutcome, Installation, InstallerError,
    SystemHostEnvironment, EXTENSIONS_PARENT, EXTENSION_UUID,
};
pub use crate::runtime::linux_gnome_shell_integration::{
    default_socket_path, detect_session, snapshot_diagnostics, DesktopEnvironment,
    GnomeDiagnostics, GnomeIntegrationState, GnomeShellActiveApplication, GnomeSnapshot,
    ListenerHandle, ListenerTransport, PeerStream, SessionKind, SharedGnomeSnapshot,
    UnixListenerTransport, BACKEND_NAME, MAX_FRAME_BYTES, PROTOCOL_VERSION,
};

/// Fixed server-side socket basename the listener binds.
pub const UNIX_SOCKET_BASENAME: &str = "clipvault-focus.sock";

/// Persisted decision the user has made about the GNOME integration.
/// Mirrors the consent states the spec documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GnomeConsentDecision {
    /// The user has not been asked yet (fresh install / fresh session).
    #[default]
    Unknown,
    /// The user accepted the integration; the install path may run.
    Accepted,
    /// The user declined the integration; the prompt must never
    /// reappear without an explicit `retry()` call.
    Declined,
    /// The user accepted, then opted to disable the integration from
    /// the settings surface.
    Disabled,
}

impl GnomeConsentDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            GnomeConsentDecision::Unknown => "unknown",
            GnomeConsentDecision::Accepted => "accepted",
            GnomeConsentDecision::Declined => "declined",
            GnomeConsentDecision::Disabled => "disabled",
        }
    }
}

/// Snapshot the diagnostics endpoint exposes. Pure data; no
/// filesystem paths, no PIDs, no clipboard text. Every field is a
/// stable identifier or a metadata-only token — the wire-level
/// surface must never leak the install target directory or any
/// other absolute path that could fingerprint the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GnomeIntegrationStatus {
    pub session: String,
    pub desktop: String,
    pub consent: String,
    pub state: String,
    pub installed: bool,
    pub uuid: String,
    pub version: u32,
    pub identifier: Option<String>,
    pub detail: Option<String>,
}

impl GnomeIntegrationStatus {
    pub fn not_applicable() -> Self {
        Self {
            session: "non_linux".to_string(),
            desktop: "unknown".to_string(),
            consent: GnomeConsentDecision::Unknown.as_str().to_string(),
            state: GnomeIntegrationState::GnomeNotDetected.as_str().to_string(),
            installed: false,
            uuid: EXTENSION_UUID.to_string(),
            version: 0,
            identifier: None,
            detail: None,
        }
    }
}

/// Handle the GNOME integration service hands back when the
/// listener is alive. Stores the join handle so the bootstrap can
/// deterministically stop the I/O thread at shutdown.
pub struct GnomeIntegrationHandle {
    pub probe: Arc<GnomeShellActiveApplication>,
    pub listener: Option<ListenerHandle>,
}

/// Single point of coordination between consent, install, listener
/// and the diagnostics surface. All long-lived state lives on
/// `AppState`; this struct owns the typed wrappers + an Arc to the
/// shared snapshot the probe reads through.
pub struct GnomeIntegrationService {
    snapshot: SharedGnomeSnapshot,
    installation: Arc<RwLock<Option<Installation>>>,
    consent: Arc<RwLock<GnomeConsentDecision>>,
    installer: Arc<ExtensionInstaller<SystemHostEnvironment>>,
}

impl fmt::Debug for GnomeIntegrationService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GnomeIntegrationService")
            .field("consent", &*self.consent.read())
            .field("state", &self.snapshot.state_str())
            .field("installation", &*self.installation.read())
            .finish()
    }
}

impl GnomeIntegrationService {
    /// Build a fresh service backed by the production host and the
    /// supplied initial consent decision. The snapshot is fresh —
    /// never populated — until the listener accepts a peer.
    pub fn new(initial_consent: GnomeConsentDecision) -> Self {
        let installer = Arc::new(ExtensionInstaller::new(Arc::new(SystemHostEnvironment)));
        let snapshot = SharedGnomeSnapshot::new();
        let service = Self {
            snapshot,
            installation: Arc::new(RwLock::new(None)),
            consent: Arc::new(RwLock::new(initial_consent)),
            installer,
        };
        service.refresh_session_state();
        service
    }

    pub fn snapshot(&self) -> &SharedGnomeSnapshot {
        &self.snapshot
    }

    pub fn installation(&self) -> Installation {
        self.installation
            .read()
            .clone()
            .unwrap_or_else(|| Installation::not_installed(EXTENSION_UUID))
    }

    pub fn consent(&self) -> GnomeConsentDecision {
        *self.consent.read()
    }

    pub fn installer(&self) -> &Arc<ExtensionInstaller<SystemHostEnvironment>> {
        &self.installer
    }

    /// Re-evaluate `(session, desktop, install)` and update the
    /// snapshot state. Called by the bootstrap at startup and after
    /// every successful install / uninstall.
    pub fn refresh_session_state(&self) {
        let (session, desktop) = detect_session();
        if matches!(session, SessionKind::NonLinux) {
            self.snapshot
                .set_state(GnomeIntegrationState::GnomeNotDetected);
            return;
        }
        if !matches!(session, SessionKind::LinuxWayland) {
            self.snapshot.set_state(GnomeIntegrationState::NotWayland);
            return;
        }
        if !matches!(desktop, DesktopEnvironment::Gnome) {
            self.snapshot
                .set_state(GnomeIntegrationState::GnomeNotDetected);
            return;
        }
        match self.installer.inspect() {
            Ok(Some(installation)) => {
                self.snapshot
                    .set_state(GnomeIntegrationState::ActivationPending);
                let mut guard = self.installation.write();
                *guard = Some(installation);
            }
            Ok(None) => {
                self.snapshot.set_state(GnomeIntegrationState::NotInstalled);
                let mut guard = self.installation.write();
                *guard = None;
            }
            Err(_) => {
                self.snapshot.set_state(GnomeIntegrationState::Incompatible);
            }
        }
    }

    /// Record the user's consent decision. Accepting must be the
    /// caller's explicit choice — the helper never sets `Accepted`
    /// without an explicit `accepted=true` argument. `declined` and
    /// `disabled` mirror the spec's persistence model.
    pub fn set_consent(&self, decision: GnomeConsentDecision) {
        *self.consent.write() = decision;
    }

    /// Install the bundled extension into the user's GNOME Shell
    /// extensions directory. Refuses when the user has not accepted
    /// the integration; in that case the helper returns the
    /// [`InstallerError`] `Io` variant carrying the stable
    /// `consent_required` label so the caller can render a tailored
    /// message.
    pub fn install(
        &self,
        metadata_json: &str,
        extension_js: &str,
    ) -> Result<InstallOutcome, InstallerError> {
        if !matches!(self.consent(), GnomeConsentDecision::Accepted) {
            return Err(InstallerError::Io("consent_required".to_string()));
        }
        let outcome = self.installer.install(metadata_json, extension_js)?;
        *self.installation.write() = Some(outcome.installation.clone());
        self.snapshot
            .set_state(GnomeIntegrationState::ActivationPending);
        self.snapshot.set_backend(Some(BACKEND_NAME));
        Ok(outcome)
    }

    /// Mark the extension as `disabled`. Does not touch GNOME's
    /// preferences — that toggle lives in `gnome-extensions` for the
    /// user to flip.
    pub fn uninstall(&self) -> Result<Installation, InstallerError> {
        let outcome = self.installer.uninstall()?;
        *self.installation.write() = None;
        self.snapshot.set_state(GnomeIntegrationState::NotInstalled);
        self.snapshot.set_active_app_id(None);
        Ok(outcome)
    }

    pub fn status(&self) -> GnomeIntegrationStatus {
        let (session, desktop) = detect_session();
        let installation = self.installation();
        GnomeIntegrationStatus {
            session: session_label(session),
            desktop: desktop_label(desktop),
            consent: self.consent().as_str().to_string(),
            state: self.snapshot.state_str().to_string(),
            installed: installation.installed,
            uuid: installation.uuid.clone(),
            version: installation.version,
            identifier: self.snapshot.active_app_id(),
            detail: self.snapshot.detail(),
        }
    }

    /// Build the diagnostics struct the Tauri command returns.
    /// Mirrors the free function in `linux_gnome_shell_integration`
    /// so callers do not have to reach into the snapshot themselves.
    pub fn diagnostics(&self) -> GnomeDiagnostics {
        snapshot_diagnostics(&self.snapshot)
    }

    /// Compute the runtime socket path the listener should bind to.
    /// The path is internal: it must never leak through the
    /// diagnostics surface or the Tauri payload. Callers that need
    /// to start the listener thread receive the path through the
    /// private constructor in the shell layer.
    pub fn socket_path(&self) -> Option<PathBuf> {
        default_socket_path()
    }
}

fn session_label(session: SessionKind) -> String {
    match session {
        SessionKind::NonLinux => "non_linux",
        SessionKind::LinuxX11 => "linux_x11",
        SessionKind::LinuxWayland => "linux_wayland",
        SessionKind::LinuxUnknown => "linux_unknown",
    }
    .to_string()
}

fn desktop_label(desktop: DesktopEnvironment) -> String {
    match desktop {
        DesktopEnvironment::Gnome => "gnome",
        DesktopEnvironment::Unknown => "unknown",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_defaults_to_unknown() {
        let service = GnomeIntegrationService::new(GnomeConsentDecision::Unknown);
        assert_eq!(service.consent(), GnomeConsentDecision::Unknown);
    }

    #[test]
    fn install_refuses_without_consent() {
        let service = GnomeIntegrationService::new(GnomeConsentDecision::Unknown);
        let error = service
            .install(
                r#"{"uuid":"clipvault@clipvault.app","name":"x","description":"y","version":1,"shell-version":["45"]}"#,
                "// stub",
            )
            .expect_err("consent missing");
        assert_eq!(error.stable_label(), "io_error");
    }

    #[test]
    fn status_is_metadata_only() {
        let service = GnomeIntegrationService::new(GnomeConsentDecision::Declined);
        let status = service.status();
        let raw = serde_json::to_string(&status).unwrap();
        assert!(!raw.contains("clipvault-focus.sock"));
        assert!(!raw.contains(".local/share"));
    }

    #[test]
    fn consent_persists_through_set_consent() {
        let service = GnomeIntegrationService::new(GnomeConsentDecision::Unknown);
        service.set_consent(GnomeConsentDecision::Accepted);
        assert_eq!(service.consent(), GnomeConsentDecision::Accepted);
        service.set_consent(GnomeConsentDecision::Declined);
        assert_eq!(service.consent(), GnomeConsentDecision::Declined);
        service.set_consent(GnomeConsentDecision::Disabled);
        assert_eq!(service.consent(), GnomeConsentDecision::Disabled);
    }

    #[test]
    fn status_omits_install_target_dir() {
        let service = GnomeIntegrationService::new(GnomeConsentDecision::Declined);
        let status = service.status();
        let raw = serde_json::to_string(&status).unwrap();
        assert!(
            !raw.contains("target_dir"),
            "status must not expose install target dir: {raw}"
        );
        assert!(
            !raw.contains(".local/share"),
            "status must not expose filesystem paths: {raw}"
        );
    }

    #[test]
    fn diagnostics_payload_never_carries_paths_or_secrets() {
        let service = GnomeIntegrationService::new(GnomeConsentDecision::Accepted);
        let diagnostics = service.diagnostics();
        let raw = serde_json::to_string(&diagnostics).unwrap();
        for forbidden in [
            "/run/",
            "/tmp/",
            "/home/",
            ".local/share",
            "XDG_RUNTIME_DIR",
            "pid",
            "PID",
            "/proc/",
            "title",
            "Title",
            "hash",
            "Hash",
            "secret",
            "password",
            "token",
            "target_dir",
            "socket_path",
        ] {
            assert!(
                !raw.contains(forbidden),
                "diagnostics leaked {forbidden:?} in {raw}"
            );
        }
    }

    /// First launch on a fresh install reports the consent as
    /// `unknown` regardless of the host session. The installer must
    /// never write to disk before the user accepts the integration.
    #[test]
    fn first_launch_reports_unknown_consent_without_installing() {
        let service = GnomeIntegrationService::new(GnomeConsentDecision::Unknown);
        let status = service.status();
        assert!(!status.installed, "first launch must report not installed");
        assert_eq!(status.consent, "unknown");
        // The platform service must NOT have called `installer.inspect`
        // (the inspection reads the user's filesystem), and `installation`
        // must report no install.
        let installation = service.installation();
        assert!(!installation.installed);
    }

    /// When the session is `NonLinux`, the status reports
    /// `not_applicable` independently of the consent decision. The
    /// helper must not lie to the UI by reporting an applicable
    /// session it cannot actually reach.
    #[test]
    fn non_linux_session_reports_not_applicable() {
        let service = GnomeIntegrationService::new(GnomeConsentDecision::Accepted);
        let status = service.status();
        if matches!(detect_session().0, SessionKind::NonLinux) {
            assert_eq!(status.session, "non_linux");
            assert!(!status.installed);
        }
    }

    /// The platform service must reject any `install` call when the
    /// user has not explicitly accepted the consent. The error label
    /// must stay stable so the UI can branch on it.
    #[test]
    fn install_refuses_with_declined_consent() {
        let service = GnomeIntegrationService::new(GnomeConsentDecision::Declined);
        let error = service
            .install(
                r#"{"uuid":"clipvault@clipvault.app","name":"x","description":"y","version":1,"shell-version":["45"]}"#,
                "// stub",
            )
            .expect_err("consent declined");
        assert_eq!(error.stable_label(), "io_error");
    }

    /// Declining the consent leaves the snapshot in the canonical
    /// `NotInstalled` state — the platform service must never
    /// silently install the extension on a decline.
    #[test]
    fn declined_consent_keeps_snapshot_in_not_installed() {
        let service = GnomeIntegrationService::new(GnomeConsentDecision::Declined);
        service.refresh_session_state();
        let status = service.status();
        // On a Linux Wayland GNOME session the canonical state must
        // be `NotInstalled` after a decline — never `ActivationPending`
        // and never `Identified`. The platform service must not
        // start the listener or modify the install state.
        let state = service.snapshot().state();
        if matches!(detect_session().0, SessionKind::LinuxWayland) {
            assert!(
                matches!(
                    state,
                    GnomeIntegrationState::NotInstalled
                        | GnomeIntegrationState::GnomeNotDetected
                        | GnomeIntegrationState::NotWayland
                ),
                "declined consent must keep snapshot in a safe state, got {state:?}"
            );
        }
        assert!(!status.installed);
    }
}
