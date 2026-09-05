//! No-op implementations used in tests and on hosts where a real
//! adapter cannot be constructed (Windows in v0.1, `Other` OS family,
//! missing display servers, missing session secrets, ...).
//!
//! Every method returns a typed `*Error` variant instead of panicking
//! so the rest of the application can stay running and surface the
//! limitation.

use std::sync::Mutex;

use crate::active_app::{
    ActiveAppBackendKind, ActiveAppError, ActiveApplication, ActiveApplicationProbe,
};
use crate::clipboard::{ClipboardBackend, ClipboardBackendError};
use crate::guidance::{SettingsNavigator, SettingsOpenOutcome};
use crate::hotkey::{HotkeyBinding, HotkeyError, HotkeyManager, HotkeyOutcome};
use crate::paste::{PasteController, PasteError};
use crate::tray::{TrayController, TrayError, TrayHandle};

/// In-memory clipboard used by tests; the runtime no-op falls back to
/// the [`crate::clipboard::UnavailableBackend`] when the OS-level
/// adapter cannot be built.
pub struct NoopClipboardBackend;

impl ClipboardBackend for NoopClipboardBackend {
    fn read_text(&self) -> Result<Option<String>, ClipboardBackendError> {
        Err(ClipboardBackendError::Unavailable {
            capability: crate::Capability::ClipboardRead,
        })
    }

    fn write_text(&self, _text: &str) -> Result<(), ClipboardBackendError> {
        Err(ClipboardBackendError::Unavailable {
            capability: crate::Capability::ClipboardWrite,
        })
    }

    fn name(&self) -> &'static str {
        "unavailable"
    }
}

/// Hotkey manager stub. Reports every binding as `Unsupported` so the
/// bootstrap can carry on without a session.
pub struct NoopHotkeyManager;

impl HotkeyManager for NoopHotkeyManager {
    fn register(
        &self,
        _binding: &HotkeyBinding,
        _on_activate: Box<dyn Fn() + Send + Sync + 'static>,
    ) -> Result<HotkeyOutcome, HotkeyError> {
        Ok(HotkeyOutcome::Unsupported {
            reason: "no hotkey backend available on this host".into(),
        })
    }

    fn unregister_all(&self) -> Result<(), HotkeyError> {
        Ok(())
    }

    fn name(&self) -> &'static str {
        "unavailable"
    }
}

/// Active-application probe stub.
pub struct NoopActiveApplicationProbe;

impl ActiveApplicationProbe for NoopActiveApplicationProbe {
    fn active_application(&self) -> Result<Option<ActiveApplication>, ActiveAppError> {
        Err(ActiveAppError::Unavailable)
    }

    fn name(&self) -> &'static str {
        ActiveAppBackendKind::Unavailable.as_str()
    }
}

/// Paste controller stub.
pub struct NoopPasteController;

impl PasteController for NoopPasteController {
    fn paste(&self) -> Result<(), PasteError> {
        Err(PasteError::Unavailable)
    }

    fn name(&self) -> &'static str {
        "unavailable"
    }
}

/// Tray controller stub. Returns a [`NoopTrayHandle`] whose
/// `shutdown` is a no-op.
pub struct NoopTrayController;

impl TrayController for NoopTrayController {
    fn install(&self) -> Result<Box<dyn TrayHandle>, TrayError> {
        Ok(Box::new(NoopTrayHandle::default()))
    }

    fn name(&self) -> &'static str {
        "unavailable"
    }
}

/// Settings navigator stub. Always reports
/// [`SettingsOpenOutcome::FallbackRequired`] with a conservative set
/// of manual steps. Used in tests and on platforms where no real
/// navigator is wired in.
pub struct NoopSettingsNavigator;

impl SettingsNavigator for NoopSettingsNavigator {
    fn open(&self, _target: crate::guidance::PlatformSettingsTarget) -> SettingsOpenOutcome {
        SettingsOpenOutcome::FallbackRequired {
            manual_steps: vec![
                "Open the operating system settings manually.".into(),
                "Refer to ClipVault documentation for the precise pane.".into(),
            ],
        }
    }

    fn name(&self) -> &'static str {
        "unavailable"
    }
}

#[derive(Debug, Default)]
pub struct NoopTrayHandle {
    shutdown_called: Mutex<bool>,
}

impl TrayHandle for NoopTrayHandle {
    fn set_menu(&self, _entries: &[crate::tray::TrayEntry]) -> Result<(), TrayError> {
        Ok(())
    }

    fn invoke(
        &self,
        action: crate::tray::TrayAction,
    ) -> Result<crate::tray::TrayOutcome, TrayError> {
        if action.is_supported_in_mvp() {
            Ok(crate::tray::TrayOutcome::Delivered)
        } else {
            Ok(crate::tray::TrayOutcome::Unavailable {
                action,
                reason: "tray action not implemented yet".into(),
            })
        }
    }

    fn shutdown(&self) -> Result<(), TrayError> {
        *self.shutdown_called.lock().expect("poisoned") = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray::{TrayAction, TrayEntry};

    #[test]
    fn noop_clipboard_returns_unavailable() {
        let backend = NoopClipboardBackend;
        match backend.read_text() {
            Err(ClipboardBackendError::Unavailable { capability }) => {
                assert_eq!(capability.as_str(), "clipboard_read");
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
        match backend.write_text("x") {
            Err(ClipboardBackendError::Unavailable { capability }) => {
                assert_eq!(capability.as_str(), "clipboard_write");
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn noop_clipboard_reports_image_capabilities_as_unavailable() {
        // The no-op backend inherits the trait defaults. The test pins
        // the contract so a future refactor cannot make an adapter
        // without image transport claim `clipboard_*_image` support.
        let backend = NoopClipboardBackend;
        assert!(!backend.supports_image_read());
        assert!(!backend.supports_image_write());
        match backend.read_image() {
            Err(ClipboardBackendError::Unavailable { capability }) => {
                assert_eq!(capability.as_str(), "clipboard_read_image");
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
        // `read_payload` must collapse to `Ignored` (Ok(None)) rather
        // than propagate the unavailable text capability as a failure,
        // so a host without a clipboard keeps the watcher alive.
        assert!(backend.read_payload().expect("soft outcome").is_none());
    }

    #[test]
    fn noop_clipboard_reports_rich_text_capabilities_as_unavailable() {
        // Rich-text support rides on the same default as image
        // support: the no-op backend never claims rich-text
        // capabilities. A host without rich-text transport falls back
        // to plain text without surfacing false promises.
        let backend = NoopClipboardBackend;
        assert!(!backend.supports_rich_read());
        assert!(!backend.supports_rich_write());
        match backend.read_rich() {
            Err(ClipboardBackendError::Unavailable { capability }) => {
                assert_eq!(capability.as_str(), "clipboard_read_rich_text");
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
        match backend.write_rich(
            &crate::clipboard::RichTextPayload::new(
                "plain".into(),
                Some("<p>html</p>".into()),
                None,
            )
            .expect("valid"),
        ) {
            Err(ClipboardBackendError::Unavailable { capability }) => {
                assert_eq!(capability.as_str(), "clipboard_write_rich_text");
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn noop_hotkey_reports_unsupported() {
        let manager = NoopHotkeyManager;
        let binding = HotkeyBinding {
            id: "quick_search".into(),
            modifiers: crate::hotkey::HotkeyModifiers::CMD_SHIFT,
            key: crate::hotkey::HotkeyKey::V,
        };
        let outcome = manager
            .register(&binding, Box::new(|| {}))
            .expect("register does not error");
        assert!(matches!(outcome, HotkeyOutcome::Unsupported { .. }));
        manager.unregister_all().expect("unregister_all");
    }

    #[test]
    fn noop_active_application_is_unavailable() {
        let probe = NoopActiveApplicationProbe;
        let result = probe.active_application();
        assert!(matches!(result, Err(ActiveAppError::Unavailable)));
    }

    #[test]
    fn noop_paste_is_unavailable() {
        let controller = NoopPasteController;
        assert!(matches!(controller.paste(), Err(PasteError::Unavailable)));
    }

    #[test]
    fn noop_tray_reports_supported_actions_as_delivered() {
        let controller = NoopTrayController;
        let handle = controller.install().expect("install");
        let entries = vec![TrayEntry {
            label: "Open quick search".into(),
            action: TrayAction::OpenQuickSearch,
        }];
        handle.set_menu(&entries).expect("set_menu");
        let outcome = handle.invoke(TrayAction::OpenQuickSearch).expect("invoke");
        assert!(matches!(outcome, crate::tray::TrayOutcome::Delivered));
        let outcome = handle.invoke(TrayAction::OpenFavorites).expect("invoke");
        assert!(matches!(
            outcome,
            crate::tray::TrayOutcome::Unavailable { .. }
        ));
        handle.shutdown().expect("shutdown");
    }

    #[test]
    fn noop_settings_navigator_falls_back() {
        let nav = NoopSettingsNavigator;
        let outcome = nav.open(crate::guidance::PlatformSettingsTarget::MacosAccessibility);
        assert!(matches!(
            outcome,
            SettingsOpenOutcome::FallbackRequired { .. }
        ));
    }
}
