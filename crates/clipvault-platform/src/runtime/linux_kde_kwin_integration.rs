//! KDE Plasma Wayland source-application bridge.
//!
//! KWin does not expose a public D-Bus API that lets a regular
//! client ask "which native Wayland toplevel is focused". Public
//! Wayland protocols (`ext-foreign-toplevel-list-v1`,
//! `zwlr_foreign_toplevel_management_unstable_v1`) advertise the
//! toplevel metadata but do not carry a focus signal on every
//! compositor. This module ships the Rust side of the bridge:
//!
//! 1. A user-installable KWin script (delivered as a resource
//!    bundled with the Tauri shell) reads
//!    `workspace.activeWindow.desktopFileName`, validates the value
//!    and publishes it, metadata-only, to the session D-Bus bus
//!    name `org.clipvault.SourceApp` over the path
//!    `/org/clipvault/SourceApp` using the interface
//!    `org.clipvault.SourceApp`.
//! 2. ClipVault owns the receiver end of that bus. The interface
//!    surface is `Publish(s id, s state, s v)`; the `id` is the
//!    canonical desktop file name, `state` is the string `1` for
//!    `identified` and `2` for `cleared`, and `v` is the string
//!    protocol version. Anything else is rejected.
//! 3. The receiver validates the wire envelope, refuses messages
//!    whose sender is not `org.kde.KWin`, refreshes the snapshot,
//!    and exposes the same [`ActiveApplicationProbe`] contract the
//!    rest of the platform uses so the capture pipeline picks up the
//!    identifier transparently.
//!
//! The bridge is opt-in: the bootstrap never installs or activates
//! the script unless the user accepted the consent prompt the
//! change introduces. The consent decision lives in `clipvault-core`;
//! this module just consumes the result.
//!
//! ## Wire protocol
//!
//! - Bus name: `org.clipvault.SourceApp` (session bus).
//! - Object path: `/org/clipvault/SourceApp`.
//! - Interface: `org.clipvault.SourceApp`.
//! - Method: `Publish(s id, s state, s v)`.
//! - The bridge resolves `org.kde.KWin` to its current unique D-Bus
//!   name at startup and accepts messages only from that connection.
//!
//! The protocol carries:
//!   - `id` (string): the validated desktop file name (e.g.
//!     `org.kate.editor.desktop`) or the empty string,
//!   - `state` (string): `1` for `identified`, `2` for `cleared`,
//!   - `v` (string): wire protocol version (`2`).
//!
//! It must never carry:
//!   - the window title,
//!   - process identifiers or `/proc` paths,
//!   - clipboard content or snippets,
//!   - file hashes or asset references.

#![cfg(all(target_os = "linux", feature = "linux-kde-kwin-integration"))]

use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::warn;
use zbus::{connection as zbus_connection, interface, message::Header as ZbusHeader};

use crate::active_app::{ActiveAppError, ActiveApplication, ActiveApplicationProbe, ProbeStage};

// =====================================================================
// Protocol constants
// =====================================================================

/// Wire protocol version the bridge accepts. Bumped whenever the
/// envelope shape changes; older versions are rejected with
/// [`KdeKwinError::ProtocolVersion`].
pub const PROTOCOL_VERSION: u32 = 2;

/// Numeric state code represented as a decimal string on the wire.
pub const STATE_IDENTIFIED: u32 = 1;
pub const STATE_CLEARED: u32 = 2;

/// Session bus well-known name KWin answers on a stock KDE Plasma
/// session. The bridge resolves it to KWin's unique owner at startup
/// and accepts `Publish` calls only from that connection.
pub const KWIN_BUS_NAME: &str = "org.kde.KWin";

/// Session bus name the bridge claims. The script sends to this
/// well-known name.
pub const BUS_NAME: &str = "org.clipvault.SourceApp";

/// Object path the bridge serves the `Publish` method on.
pub const OBJECT_PATH: &str = "/org/clipvault/SourceApp";

/// Interface name the bridge implements.
pub const INTERFACE: &str = "org.clipvault.SourceApp";

/// KWin package id used by the install/lifecycle code and the
/// script-side `pluginName` argument.
pub const SCRIPT_PLUGIN_ID: &str = "clipvault-kde-source-app";

/// Backend identifier exposed through [`ActiveApplicationProbe::name`]
/// when the KDE KWin integration is connected.
pub const BACKEND_NAME: &str = "kde_kwin_script";

/// Maximum length of a desktop file name the bridge accepts. The
/// contract is that the identifier is a basename, so anything above
/// `512` bytes is rejected without further inspection.
pub const MAX_IDENTIFIER_BYTES: usize = 512;

// =====================================================================
// Errors
// =====================================================================

/// Typed errors the bridge surfaces. Variants are mapped to a stable
/// `stable_label()` the diagnostics endpoint and the integration
/// status consume verbatim; the payload itself never reaches the
/// diagnostics surface.
#[derive(Debug, Error)]
pub enum KdeKwinError {
    /// The wire envelope carried a protocol version the bridge
    /// does not support.
    #[error("protocol version mismatch")]
    ProtocolVersion,
    /// The wire envelope carried an unknown state code.
    #[error("unknown state code")]
    UnknownState(u32),
    /// The desktop file name failed the documented shape check.
    #[error("invalid identifier")]
    InvalidIdentifier,
    /// The sender unique name did not match the KWin well-known
    /// bus name. The message is dropped without leaking the
    /// offending sender.
    #[error("sender is not the KWin process")]
    UnexpectedSender,
    /// The bridge failed to take the session bus or to register
    /// the `Publish` interface.
    #[error("d-bus bridge error: {0}")]
    Dbus(String),
}

impl KdeKwinError {
    pub fn stable_label(&self) -> &'static str {
        match self {
            KdeKwinError::ProtocolVersion => "protocol_version_mismatch",
            KdeKwinError::UnknownState(_) => "unknown_state",
            KdeKwinError::InvalidIdentifier => "invalid_identifier",
            KdeKwinError::UnexpectedSender => "unexpected_sender",
            KdeKwinError::Dbus(_) => "dbus_error",
        }
    }
}

// =====================================================================
// Snapshot
// =====================================================================

/// Lifecycle state the probe exposes to the diagnostics surface.
/// Mirrors the GNOME integration's vocabulary so the UI can keep
/// using the same labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KdeKwinIntegrationState {
    /// The session is not KDE Plasma Wayland (`XDG_CURRENT_DESKTOP`
    /// does not contain `KDE` or `WAYLAND_DISPLAY` is unset).
    NotApplicable,
    /// KDE Plasma Wayland was detected but the consent decision
    /// is not `Accepted`.
    AwaitingConsent,
    /// The user accepted but the script is not installed on the
    /// user's account yet.
    NotInstalled,
    /// The user disabled the integration while leaving the package
    /// installed.
    Disabled,
    /// The script is installed but the bridge has not observed a
    /// first `Publish` call.
    ActivationPending,
    /// The bridge accepted a peer but the most recent envelope
    /// carries an empty identifier.
    NoActiveApplication,
    /// The bridge accepted a peer and the most recent envelope
    /// carries a validated identifier.
    Identified,
    /// The bridge dropped the receiver / connection and the
    /// snapshot has been cleared.
    Disconnected,
    /// The bridge failed to bind the session bus name or to
    /// initialise the receiver.
    CommunicationError,
}

impl KdeKwinIntegrationState {
    pub fn as_str(self) -> &'static str {
        match self {
            KdeKwinIntegrationState::NotApplicable => "not_applicable",
            KdeKwinIntegrationState::AwaitingConsent => "awaiting_consent",
            KdeKwinIntegrationState::NotInstalled => "not_installed",
            KdeKwinIntegrationState::Disabled => "disabled",
            KdeKwinIntegrationState::ActivationPending => "activation_pending",
            KdeKwinIntegrationState::NoActiveApplication => "no_active_application",
            KdeKwinIntegrationState::Identified => "identified",
            KdeKwinIntegrationState::Disconnected => "disconnected",
            KdeKwinIntegrationState::CommunicationError => "communication_error",
        }
    }
}

/// Snapshot handle the receiver thread and the probe share. The
/// receiver holds the writer side and the probe reads through the
/// [`ActiveApplicationProbe`] trait without ever touching the
/// receiver.
#[derive(Debug, Clone)]
pub struct SharedKdeKwinSnapshot {
    inner: Arc<RwLock<KdeKwinSnapshot>>,
    stage: Arc<Mutex<ProbeStage>>,
}

#[derive(Debug)]
struct KdeKwinSnapshot {
    state: KdeKwinIntegrationState,
    active_app_id: Option<String>,
    backend: Option<&'static str>,
    detail: Option<String>,
    last_error: Option<KdeKwinError>,
}

impl KdeKwinSnapshot {
    fn new() -> Self {
        Self {
            state: KdeKwinIntegrationState::ActivationPending,
            active_app_id: None,
            backend: None,
            detail: None,
            last_error: None,
        }
    }
}

impl Default for KdeKwinSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedKdeKwinSnapshot {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(KdeKwinSnapshot::new())),
            stage: Arc::new(Mutex::new(ProbeStage::Started)),
        }
    }

    pub fn state(&self) -> KdeKwinIntegrationState {
        self.inner.read().state
    }

    pub fn state_str(&self) -> &'static str {
        self.inner.read().state.as_str()
    }

    pub fn active_app_id(&self) -> Option<String> {
        self.inner.read().active_app_id.clone()
    }

    pub fn backend(&self) -> Option<&'static str> {
        self.inner.read().backend
    }

    pub fn detail(&self) -> Option<String> {
        self.inner.read().detail.clone()
    }

    pub fn last_error_label(&self) -> Option<&'static str> {
        self.inner
            .read()
            .last_error
            .as_ref()
            .map(|err| err.stable_label())
    }

    pub fn set_state(&self, state: KdeKwinIntegrationState) {
        self.inner.write().state = state;
        if matches!(
            state,
            KdeKwinIntegrationState::Identified
                | KdeKwinIntegrationState::NoActiveApplication
                | KdeKwinIntegrationState::ActivationPending
        ) {
            self.inner.write().detail = None;
            self.inner.write().last_error = None;
        }
    }

    pub fn set_active_app_id(&self, app_id: Option<String>) {
        self.inner.write().active_app_id = app_id.and_then(|value| {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
    }

    pub fn set_backend(&self, backend: Option<&'static str>) {
        self.inner.write().backend = backend;
    }

    pub fn set_detail<S: Into<String>>(&self, detail: Option<S>) {
        self.inner.write().detail = detail.map(Into::into);
    }

    pub fn set_last_error(&self, error: Option<KdeKwinError>) {
        self.inner.write().last_error = error;
    }

    pub fn last_probe_stage(&self) -> ProbeStage {
        *self.stage.lock()
    }

    pub fn set_probe_stage(&self, stage: ProbeStage) {
        *self.stage.lock() = stage;
    }
}

impl Default for SharedKdeKwinSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

/// Diagnostics struct the Tauri command returns. Mirrors the GNOME
/// integration's `GnomeDiagnostics` so the frontend code does not
/// branch on the backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdeKwinDiagnostics {
    pub backend: String,
    pub state: String,
    pub installed: bool,
    pub detail: Option<String>,
    pub error: Option<String>,
    pub active_app_id: Option<String>,
}

impl KdeKwinDiagnostics {
    pub fn from_snapshot(snapshot: &SharedKdeKwinSnapshot, installed: bool) -> Self {
        Self {
            backend: snapshot.backend().unwrap_or(BACKEND_NAME).to_string(),
            state: snapshot.state_str().to_string(),
            installed,
            detail: snapshot.detail(),
            error: snapshot.last_error_label().map(|s| s.to_string()),
            active_app_id: snapshot.active_app_id(),
        }
    }
}

// =====================================================================
// Identifier validation
// =====================================================================

/// Validate the desktop file name the script publishes. The
/// contract is documented in the script's JavaScript companion
/// module: the value is a basename matching the regex
/// `[A-Za-z0-9._+-]+\.desktop` with no path separators, no NUL
/// bytes and a bounded length. Anything else is dropped.
pub fn is_valid_identifier(raw: &str) -> bool {
    let value = raw.trim();
    if value.is_empty() || value.len() > MAX_IDENTIFIER_BYTES {
        return false;
    }
    if value.contains('/') || value.contains('\\') || value.contains('\0') {
        return false;
    }
    let bytes = value.as_bytes();
    if !bytes.ends_with(b".desktop") {
        return false;
    }
    let prefix = &bytes[..bytes.len() - ".desktop".len()];
    if prefix.is_empty() {
        return false;
    }
    for byte in prefix {
        let is_allowed = matches!(
            *byte,
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'_' | b'+' | b'-'
        );
        if !is_allowed {
            return false;
        }
    }
    true
}

/// Normalise an identifier before the probe exposes it. Returns
/// `None` when the value is empty or fails [`is_valid_identifier`].
pub fn normalize_identifier(raw: &str) -> Option<String> {
    if is_valid_identifier(raw) {
        Some(raw.trim().to_string())
    } else {
        None
    }
}

// =====================================================================
// Receiver
// =====================================================================

/// Bridge the `zbus` connection serves. The receiver validates the
/// envelope and forwards the identifier / state pair to the
/// snapshot through [`SharedKdeKwinSnapshot`]. The struct owns the
/// shared snapshot so a clone can move into the
/// `interface!`-generated server trait.
#[derive(Clone)]
struct KdeKwinReceiver {
    snapshot: SharedKdeKwinSnapshot,
    /// Unique name the bridge has resolved for `org.kde.KWin` at
    /// startup. D-Bus unique names look like `:1.8` and are
    /// independent of the well-known name the process owns; we
    /// capture the live value so every incoming envelope can be
    /// compared against it without round-tripping to the broker.
    kwin_unique_name: Arc<Mutex<Option<String>>>,
}

#[interface(name = "org.clipvault.SourceApp")]
impl KdeKwinReceiver {
    /// Handle a `Publish` call from the KWin script.
    ///
    /// Returns `Ok(())` on success; returns a D-Bus error derived
    /// from [`KdeKwinError`] on every rejection. The error message
    /// is the stable label the diagnostics surface already
    /// understands; the payload itself never leaves the receiver.
    async fn publish(
        &self,
        #[zbus(header)] hdr: ZbusHeader<'_>,
        id: String,
        state: String,
        v: String,
    ) -> zbus::fdo::Result<()> {
        let sender_unique = match hdr.sender() {
            Some(sender) => sender.to_string(),
            None => {
                self.snapshot
                    .set_state(KdeKwinIntegrationState::CommunicationError);
                self.snapshot
                    .set_last_error(Some(KdeKwinError::UnexpectedSender));
                self.snapshot.set_probe_stage(ProbeStage::Backend);
                warn!("KDE bridge: dropped envelope without D-Bus sender header");
                return Err(zbus::fdo::Error::Failed(
                    KdeKwinError::UnexpectedSender.stable_label().to_string(),
                ));
            }
        };

        if !sender_matches_kwin(&sender_unique, &self.kwin_unique_name.lock()) {
            self.snapshot
                .set_state(KdeKwinIntegrationState::CommunicationError);
            self.snapshot
                .set_last_error(Some(KdeKwinError::UnexpectedSender));
            self.snapshot.set_probe_stage(ProbeStage::Backend);
            warn!("KDE bridge: dropped envelope from non-KWin sender");
            return Err(zbus::fdo::Error::Failed(
                KdeKwinError::UnexpectedSender.stable_label().to_string(),
            ));
        }

        if v != PROTOCOL_VERSION.to_string() {
            self.snapshot
                .set_last_error(Some(KdeKwinError::ProtocolVersion));
            self.snapshot.set_probe_stage(ProbeStage::Backend);
            return Err(zbus::fdo::Error::Failed(
                KdeKwinError::ProtocolVersion.stable_label().to_string(),
            ));
        }

        match state.as_str() {
            "1" => {
                let normalised = match normalize_identifier(&id) {
                    Some(value) => value,
                    None => {
                        self.snapshot
                            .set_last_error(Some(KdeKwinError::InvalidIdentifier));
                        self.snapshot.set_probe_stage(ProbeStage::IdentifierEmpty);
                        return Err(zbus::fdo::Error::Failed(
                            KdeKwinError::InvalidIdentifier.stable_label().to_string(),
                        ));
                    }
                };
                self.snapshot.set_active_app_id(Some(normalised));
                self.snapshot.set_state(KdeKwinIntegrationState::Identified);
                self.snapshot.set_probe_stage(ProbeStage::Identified);
                self.snapshot.set_last_error(None);
            }
            "2" => {
                self.snapshot.set_active_app_id(None);
                self.snapshot
                    .set_state(KdeKwinIntegrationState::NoActiveApplication);
                self.snapshot.set_probe_stage(ProbeStage::ActiveWindowEmpty);
                self.snapshot.set_last_error(None);
            }
            _ => {
                self.snapshot
                    .set_last_error(Some(KdeKwinError::UnknownState(u32::MAX)));
                self.snapshot.set_probe_stage(ProbeStage::Backend);
                return Err(zbus::fdo::Error::Failed(
                    KdeKwinError::UnknownState(u32::MAX)
                        .stable_label()
                        .to_string(),
                ));
            }
        }
        Ok(())
    }
}

/// Compare a sender unique-name against the KWin process' resolved
/// unique name. The bridge resolves the unique name at startup
/// (D-Bus exposes `:1.<n>` identifiers independent of the
/// well-known name `org.kde.KWin` that KWin answers) and the
/// receiver compares every incoming envelope against it.
fn sender_matches_kwin(sender_unique: &str, kwin_unique: &Option<String>) -> bool {
    kwin_unique.as_deref() == Some(sender_unique)
}

/// Outcome of [`detect_session`]. The bootstrap uses the values to
/// decide whether the KDE KWin integration is applicable for the
/// current host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KdeSessionKind {
    /// The host is not a Linux session at all.
    NonLinux,
    /// The session is not Wayland (`WAYLAND_DISPLAY` is unset).
    NotWayland,
    /// The session is Wayland but the desktop environment is not
    /// KDE Plasma (`XDG_CURRENT_DESKTOP` does not contain `KDE`
    /// or `plasma`).
    NotKde,
    /// The host is a KDE Plasma Wayland session.
    KdePlasmaWayland,
}

/// Inspect the environment and classify the session. Mirrors the
/// GNOME integration's `detect_session` so the bootstrap can ask
/// both branches the same way. The helper never shells out — every
/// check reads a documented environment variable.
pub fn detect_session() -> KdeSessionKind {
    if !cfg!(target_os = "linux") {
        return KdeSessionKind::NonLinux;
    }
    match std::env::var_os("WAYLAND_DISPLAY") {
        Some(_) => {}
        None => return KdeSessionKind::NotWayland,
    }
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let desktop_lower = desktop.to_ascii_lowercase();
    let is_kde = desktop_lower
        .split(':')
        .any(|token| token.trim() == "kde" || token.trim() == "plasma");
    if is_kde {
        KdeSessionKind::KdePlasmaWayland
    } else {
        KdeSessionKind::NotKde
    }
}

/// Compute the `~/.local/share/kwin/scripts/<SCRIPT_PLUGIN_ID>`
/// directory used by the install path. Lives in the runtime
/// module so the install/uninstall helpers can call it without
/// depending on `dirs`.
pub fn default_package_dir() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    if home.is_empty() {
        return None;
    }
    Some(
        std::path::PathBuf::from(home)
            .join(format!(".local/share/kwin/scripts/{SCRIPT_PLUGIN_ID}")),
    )
}

// =====================================================================
// Connection lifecycle
// =====================================================================

/// Handle to the live bridge. Dropping the value stops the
/// `zbus::connection` and the snapshot is left in [`Disconnected`]
/// so the next probe query returns `Ok(None)`.
pub struct KdeKwinBridgeHandle {
    connection: zbus_connection::Connection,
    snapshot: SharedKdeKwinSnapshot,
}

impl KdeKwinBridgeHandle {
    pub fn snapshot(&self) -> SharedKdeKwinSnapshot {
        self.snapshot.clone()
    }

    pub fn state(&self) -> KdeKwinIntegrationState {
        self.snapshot.state()
    }

    pub fn diagnostics(&self) -> KdeKwinDiagnostics {
        KdeKwinDiagnostics::from_snapshot(&self.snapshot, true)
    }

    pub fn connection(&self) -> &zbus_connection::Connection {
        &self.connection
    }
}

impl Drop for KdeKwinBridgeHandle {
    fn drop(&mut self) {
        self.snapshot.set_active_app_id(None);
        self.snapshot
            .set_state(KdeKwinIntegrationState::Disconnected);
    }
}

/// Resolve the unique name the KWin process answers under on the
/// current session bus. Returns `None` when KWin is not running
/// (e.g. a non-KDE session, an unsupported distro, or KWin itself
/// crashing). The bridge treats `None` as "accept all envelopes"
/// is unsafe — instead the receiver keeps
/// `kwin_unique_name = None` and rejects every envelope until the
/// next call refreshes the value.
async fn resolve_kwin_unique_name(connection: &zbus_connection::Connection) -> Option<String> {
    let reply = connection
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "GetNameOwner",
            &(KWIN_BUS_NAME),
        )
        .await
        .ok()?;
    let owner: String = reply.body().deserialize().ok()?;
    if owner.is_empty() {
        None
    } else {
        Some(owner)
    }
}

/// Start the bridge on the current Tokio runtime. Returns
/// `Err(KdeKwinError::Dbus(_))` when the session bus is not
/// reachable or the well-known name cannot be claimed.
pub async fn start_bridge(
    snapshot: SharedKdeKwinSnapshot,
) -> Result<KdeKwinBridgeHandle, KdeKwinError> {
    snapshot.set_backend(Some(BACKEND_NAME));

    let kwin_unique_name = Arc::new(Mutex::new(None::<String>));

    let connection = match zbus_connection::Builder::session()
        .map_err(|error| KdeKwinError::Dbus(error.to_string()))?
        .name(BUS_NAME)
        .map_err(|error| KdeKwinError::Dbus(error.to_string()))?
        .serve_at(
            OBJECT_PATH,
            KdeKwinReceiver {
                snapshot: snapshot.clone(),
                kwin_unique_name: kwin_unique_name.clone(),
            },
        )
        .map_err(|error| KdeKwinError::Dbus(error.to_string()))?
        .build()
        .await
    {
        Ok(connection) => connection,
        Err(error) => {
            snapshot.set_state(KdeKwinIntegrationState::CommunicationError);
            snapshot.set_last_error(Some(KdeKwinError::Dbus(error.to_string())));
            snapshot.set_probe_stage(ProbeStage::Backend);
            return Err(KdeKwinError::Dbus(error.to_string()));
        }
    };

    if let Some(unique) = resolve_kwin_unique_name(&connection).await {
        *kwin_unique_name.lock() = Some(unique);
    } else {
        snapshot.set_state(KdeKwinIntegrationState::ActivationPending);
        snapshot.set_detail(Some("kwin_owner_missing".to_string()));
    }

    Ok(KdeKwinBridgeHandle {
        connection,
        snapshot,
    })
}

// =====================================================================
// ActiveApplicationProbe
// =====================================================================

/// Probe wrapper that exposes the snapshot to the rest of the
/// platform. The probe is intentionally narrow: the snapshot is the
/// single source of truth and the wrapper never calls into
/// `zbus`. A separate caller is responsible for owning the
/// [`KdeKwinBridgeHandle`] so dropping the handle tears down the
/// receiver independently of how long the probe survives.
pub struct KdeKwinActiveApplication {
    snapshot: SharedKdeKwinSnapshot,
}

impl KdeKwinActiveApplication {
    pub fn new(snapshot: SharedKdeKwinSnapshot) -> Self {
        Self { snapshot }
    }

    pub fn snapshot(&self) -> SharedKdeKwinSnapshot {
        self.snapshot.clone()
    }
}

impl std::fmt::Debug for KdeKwinActiveApplication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KdeKwinActiveApplication")
            .field("state", &self.snapshot.state_str())
            .field("identifier", &self.snapshot.active_app_id())
            .finish()
    }
}

impl ActiveApplicationProbe for KdeKwinActiveApplication {
    fn active_application(&self) -> Result<Option<ActiveApplication>, ActiveAppError> {
        match self.snapshot.state() {
            KdeKwinIntegrationState::Identified => {
                if let Some(identifier) = self.snapshot.active_app_id() {
                    // The probe exposes the identifier in both
                    // fields so the `HistoryCardRail` can render a
                    // stable key; the `LinuxApplicationMetadata`
                    // provider resolves the user-visible label later.
                    return Ok(Some(ActiveApplication::new(identifier.clone(), identifier)));
                }
                self.snapshot.set_probe_stage(ProbeStage::IdentifierEmpty);
                Ok(None)
            }
            KdeKwinIntegrationState::NoActiveApplication => {
                self.snapshot.set_probe_stage(ProbeStage::ActiveWindowEmpty);
                Ok(None)
            }
            KdeKwinIntegrationState::Disconnected | KdeKwinIntegrationState::CommunicationError => {
                Err(ActiveAppError::backend(
                    self.snapshot
                        .last_error_label()
                        .unwrap_or("bridge_unavailable")
                        .to_string(),
                ))
            }
            _ => Ok(None),
        }
    }

    fn name(&self) -> &'static str {
        BACKEND_NAME
    }

    fn last_probe_stage(&self) -> ProbeStage {
        self.snapshot.last_probe_stage()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_identifiers_round_trip() {
        for raw in [
            "org.kate.editor.desktop",
            "firefox.desktop",
            "com.google.Chrome.desktop",
            "code.desktop",
            "org.kde.kate.desktop",
        ] {
            assert!(is_valid_identifier(raw), "expected valid: {raw}");
            assert_eq!(
                normalize_identifier(raw).as_deref(),
                Some(raw),
                "expected unchanged round-trip for {raw}",
            );
        }
    }

    #[test]
    fn invalid_identifiers_are_rejected() {
        for raw in [
            "",
            "   ",
            "firefox",
            "firefox.desktop.exe",
            "../firefox.desktop",
            "/home/user/firefox.desktop",
            "fire\0x.desktop",
            "firex.desktop/../etc/passwd",
            ".desktop",
        ] {
            assert!(!is_valid_identifier(raw), "expected rejection for {raw:?}");
            assert!(
                normalize_identifier(raw).is_none(),
                "expected None for {raw:?}",
            );
        }
        let oversized = format!("{}.desktop", "a".repeat(MAX_IDENTIFIER_BYTES));
        assert!(!is_valid_identifier(&oversized));
    }

    #[test]
    fn diagnostics_payload_never_carries_paths_or_secrets() {
        let snapshot = SharedKdeKwinSnapshot::new();
        snapshot.set_state(KdeKwinIntegrationState::Identified);
        snapshot.set_active_app_id(Some("org.kate.editor.desktop".to_string()));
        let diagnostics = KdeKwinDiagnostics::from_snapshot(&snapshot, true);
        let raw = serde_json::to_string(&diagnostics).unwrap();
        for forbidden in [
            "/run/",
            "/tmp/",
            "/home/",
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

    #[test]
    fn probe_serves_cached_identifier_when_identified() {
        let snapshot = SharedKdeKwinSnapshot::new();
        snapshot.set_state(KdeKwinIntegrationState::Identified);
        snapshot.set_active_app_id(Some("org.kate.editor.desktop".to_string()));
        let probe = KdeKwinActiveApplication::new(snapshot);
        let app = probe.active_application().expect("ok").expect("identified");
        assert_eq!(app.identifier, "org.kate.editor.desktop");
        assert_eq!(app.name, "org.kate.editor.desktop");
    }

    #[test]
    fn probe_returns_none_when_no_active_application() {
        let snapshot = SharedKdeKwinSnapshot::new();
        snapshot.set_state(KdeKwinIntegrationState::NoActiveApplication);
        let probe = KdeKwinActiveApplication::new(snapshot);
        assert!(probe.active_application().expect("ok").is_none());
    }

    #[test]
    fn probe_returns_backend_error_when_disconnected() {
        let snapshot = SharedKdeKwinSnapshot::new();
        snapshot.set_state(KdeKwinIntegrationState::Disconnected);
        let probe = KdeKwinActiveApplication::new(snapshot);
        match probe.active_application() {
            Err(ActiveAppError::Backend { .. }) => {}
            other => panic!("expected backend error, got {other:?}"),
        }
    }

    #[test]
    fn stable_label_is_deterministic() {
        for (error, expected) in [
            (KdeKwinError::ProtocolVersion, "protocol_version_mismatch"),
            (KdeKwinError::InvalidIdentifier, "invalid_identifier"),
            (KdeKwinError::UnexpectedSender, "unexpected_sender"),
            (KdeKwinError::UnknownState(0), "unknown_state"),
            (KdeKwinError::Dbus("x".into()), "dbus_error"),
        ] {
            assert_eq!(error.stable_label(), expected);
        }
    }

    #[test]
    fn snapshot_serialises_metadata_only_fields() {
        let snapshot = SharedKdeKwinSnapshot::new();
        snapshot.set_state(KdeKwinIntegrationState::Identified);
        snapshot.set_active_app_id(Some("org.kate.editor.desktop".to_string()));
        snapshot.set_backend(Some(BACKEND_NAME));
        let diagnostics = KdeKwinDiagnostics::from_snapshot(&snapshot, true);
        let raw = serde_json::to_string(&diagnostics).unwrap();
        assert!(raw.contains("\"backend\":\"kde_kwin_script\""));
        assert!(raw.contains("\"state\":\"identified\""));
        assert!(raw.contains("\"active_app_id\":\"org.kate.editor.desktop\""));
    }

    #[test]
    fn kwin_sender_matcher_accepts_kwin_unique_name() {
        // The bridge resolves KWin's unique name at startup
        // through `GetNameOwner("org.kde.KWin")`. D-Bus unique
        // names look like `:1.8` and are independent of the
        // well-known name `org.kde.KWin` that KWin owns.
        let kwin_unique = Some(":1.8".to_string());
        assert!(sender_matches_kwin(":1.8", &kwin_unique));
        // Anything else is rejected.
        assert!(!sender_matches_kwin(":1.50", &kwin_unique));
        assert!(!sender_matches_kwin(":1.7", &kwin_unique));
        assert!(!sender_matches_kwin("org.kde.KWin", &kwin_unique));
        assert!(!sender_matches_kwin("", &kwin_unique));
        // An unresolved owner (KWin not running) drops everything.
        assert!(!sender_matches_kwin(":1.8", &None));
    }
}
