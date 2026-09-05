//! Re-export the platform-layer traits and concrete adapters to the
//! rest of the crate and the Tauri shell. Keeping the re-exports
//! centralised here avoids leaking the `clipvault_platform` crate name
//! across the application surface.

pub use clipvault_platform::{
    default_linux_binding, default_macos_binding, ActiveAppBackendKind, ActiveAppError,
    ActiveApplication, ActiveApplicationProbe, Capabilities, Capability, ClipboardBackend,
    ClipboardBackendError, ClipboardBackendKind, DefaultPlatform, DisplayServer, HotkeyBackendKind,
    HotkeyBinding, HotkeyError, HotkeyKey, HotkeyManager, HotkeyModifiers, HotkeyOutcome,
    NoopActiveApplicationProbe, NoopClipboardBackend, NoopHotkeyManager, NoopPasteController,
    NoopSettingsNavigator, NoopTrayController, NoopTrayHandle, OsFamily, PasteBackendKind,
    PasteController, PasteError, PlatformError, PlatformGuidance, PlatformIssueKind,
    PlatformSettingsTarget, SettingsNavigator, SettingsOpenOutcome, TrayAction, TrayBackendKind,
    TrayController, TrayEntry, TrayError, TrayHandle, TrayOutcome,
};
