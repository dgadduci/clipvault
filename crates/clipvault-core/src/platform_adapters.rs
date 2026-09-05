//! Aggregation of every platform adapter that the core consumes.
//!
//! The struct is the single argument the bootstrap receives to wire
//! ClipVault against a host. Production builds build a
//! [`PlatformAdapters`] populated with real adapters (see
//! `clipvault-app::bootstrap`); tests build one filled with fakes.

use std::sync::Arc;

use clipvault_platform::{
    refresh_capabilities as refresh_capabilities_for, ActiveApplicationProbe,
    ApplicationMetadataProvider, Capabilities, ClipboardBackend, HotkeyManager, PasteController,
    PlatformInfo, SettingsNavigator, TrayController,
};

/// All the platform adapters the core needs at runtime.
///
/// Cloning is cheap: every field is `Arc<dyn ...>`. The struct also
/// carries the [`Capabilities`] matrix so callers can short-circuit
/// when a feature is unsupported instead of hitting the adapter and
/// receiving a typed `Unavailable` error.
#[derive(Clone)]
pub struct PlatformAdapters {
    clipboard: Arc<dyn ClipboardBackend>,
    hotkey: Arc<dyn HotkeyManager>,
    active_app: Arc<dyn ActiveApplicationProbe>,
    paste: Arc<dyn PasteController>,
    tray: Arc<dyn TrayController>,
    settings_navigator: Arc<dyn SettingsNavigator>,
    /// Source-application metadata provider used by the
    /// `history-card-layout` capability to enrich permitted captures
    /// with a user-visible name and a controlled icon reference.
    app_metadata: Arc<dyn ApplicationMetadataProvider>,
    capabilities: Capabilities,
    info: PlatformInfo,
}

impl PlatformAdapters {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        clipboard: Arc<dyn ClipboardBackend>,
        hotkey: Arc<dyn HotkeyManager>,
        active_app: Arc<dyn ActiveApplicationProbe>,
        paste: Arc<dyn PasteController>,
        tray: Arc<dyn TrayController>,
        settings_navigator: Arc<dyn SettingsNavigator>,
        app_metadata: Arc<dyn ApplicationMetadataProvider>,
        capabilities: Capabilities,
        info: PlatformInfo,
    ) -> Self {
        Self {
            clipboard,
            hotkey,
            active_app,
            paste,
            tray,
            settings_navigator,
            app_metadata,
            capabilities,
            info,
        }
    }

    pub fn clipboard(&self) -> Arc<dyn ClipboardBackend> {
        Arc::clone(&self.clipboard)
    }

    pub fn hotkey(&self) -> Arc<dyn HotkeyManager> {
        Arc::clone(&self.hotkey)
    }

    pub fn active_app(&self) -> Arc<dyn ActiveApplicationProbe> {
        Arc::clone(&self.active_app)
    }

    pub fn paste(&self) -> Arc<dyn PasteController> {
        Arc::clone(&self.paste)
    }

    pub fn tray(&self) -> Arc<dyn TrayController> {
        Arc::clone(&self.tray)
    }

    pub fn settings_navigator(&self) -> Arc<dyn SettingsNavigator> {
        Arc::clone(&self.settings_navigator)
    }

    pub fn app_metadata(&self) -> Arc<dyn ApplicationMetadataProvider> {
        Arc::clone(&self.app_metadata)
    }

    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    pub fn info(&self) -> &PlatformInfo {
        &self.info
    }

    /// Re-evaluate the capability matrix from the cached [`PlatformInfo`].
    /// Used after the user returns from system settings so the UI can
    /// show the updated support without restarting ClipVault.
    ///
    /// The matrix is recomputed through the runtime detection entry
    /// point so the `synthetic_paste` capability reflects the current
    /// macOS Accessibility grant.
    pub fn refresh_capabilities(&self) -> Capabilities {
        refresh_capabilities_for(&self.info)
    }
}

impl std::fmt::Debug for PlatformAdapters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlatformAdapters")
            .field("clipboard", &self.clipboard.name())
            .field("hotkey", &self.hotkey.name())
            .field("active_app", &self.active_app.name())
            .field("paste", &self.paste.name())
            .field("tray", &self.tray.name())
            .field("settings_navigator", &self.settings_navigator.name())
            .field("capabilities", &self.capabilities)
            .finish()
    }
}

impl PlatformAdapters {
    /// Build a stub bundle populated with no-op adapters. Used by the
    /// bootstrap when the shell did not inject a real adapter set.
    pub fn stub(info: &PlatformInfo, capabilities: Capabilities) -> Self {
        use clipvault_platform::{
            NoopActiveApplicationProbe, NoopApplicationMetadataProvider, NoopClipboardBackend,
            NoopHotkeyManager, NoopPasteController, NoopSettingsNavigator, NoopTrayController,
        };
        Self {
            clipboard: Arc::new(NoopClipboardBackend),
            hotkey: Arc::new(NoopHotkeyManager),
            active_app: Arc::new(NoopActiveApplicationProbe),
            paste: Arc::new(NoopPasteController),
            tray: Arc::new(NoopTrayController),
            settings_navigator: Arc::new(NoopSettingsNavigator),
            app_metadata: Arc::new(NoopApplicationMetadataProvider),
            capabilities,
            info: info.clone(),
        }
    }
}
