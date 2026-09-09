//! Platform adapters and trait boundaries for ClipVault.
//!
//! The crate is split into one module per adapter. Each module exposes
//! a small, platform-agnostic trait plus the typed errors the rest of
//! the application uses to react to failures. Real implementations
//! live behind feature flags so the crate still compiles in
//! environments where the backing native crate is not available.

#![deny(unsafe_op_in_unsafe_fn)]

mod active_app;
mod app_assets;
pub mod app_metadata;
mod app_picker;
mod capabilities;
mod clipboard;
mod clipboard_image_png;
pub mod guidance;
mod hotkey;
mod info;
mod noop;
pub mod paste;
pub mod runtime;
mod stub;
mod tiff_metadata;
mod tray;

pub use active_app::{
    ActiveAppBackendKind, ActiveAppError, ActiveApplication, ActiveApplicationProbe,
    CachedActiveApplication,
};
pub use app_assets::{
    read_icon_bytes, read_source_app_icon_bytes, resolve_icon_path, IconReadError, IconRefError,
    ASSETS_DIR, IGNORED_APPS_DIR, MAX_ICON_BYTES, MAX_ICON_BYTES_LEGACY, MAX_ICON_DIM,
};
pub use app_metadata::{
    icon_ref_for, path_for_icon_ref, ApplicationMetadata, ApplicationMetadataError,
    ApplicationMetadataProvider, NoopApplicationMetadataProvider, APPLICATION_ICONS_DIR,
};
pub use app_picker::{ApplicationPicker, ApplicationPickerError, SelectedApplication};
pub use capabilities::{
    detect_capabilities, detect_capabilities_runtime, detect_capabilities_with_preflight,
    detect_capabilities_with_probes, macos_preflight_event_access, probe_image_clipboard,
    probe_rich_text_clipboard, refresh_capabilities, Capabilities, Capability,
    ImageClipboardSupport, RichTextClipboardSupport,
};
pub use clipboard::{
    checked_rgba_len, ClipboardBackend, ClipboardBackendError, ClipboardBackendKind,
    ClipboardImage, ClipboardPayload, ImageValidationError, PasteboardImageMetadata,
    RichTextPayload, MAX_CLIPBOARD_IMAGE_DIM, MAX_CLIPBOARD_IMAGE_RGBA_BYTES, RGBA_BYTES_PER_PIXEL,
};
pub use clipboard_image_png::{
    chunk, parse_ppu_triple, phys_to_dpi, png_metadata_summary, validate_png, ChunkType,
    PngMetadataSummary, PngValidationError, PngValidationOutcome, ValidatedPng,
    MAX_CLIPBOARD_PNG_BYTES, PNG_SIGNATURE,
};
pub use guidance::{
    backend_unavailable_guidance, linux_unknown_session_guidance,
    linux_wayland_unsupported_guidance, linux_x11_backend_unavailable_guidance,
    macos_accessibility_guidance, unknown_guidance, PlatformGuidance, PlatformIssueKind,
    PlatformSettingsTarget, SettingsNavigator, SettingsOpenOutcome,
};
pub use hotkey::{
    default_linux_binding, default_macos_binding, HotkeyBackendKind, HotkeyBinding, HotkeyError,
    HotkeyKey, HotkeyManager, HotkeyModifiers, HotkeyOutcome,
};
pub use info::{DisplayServer, OsFamily, PlatformInfo};
pub use noop::{
    NoopActiveApplicationProbe, NoopClipboardBackend, NoopHotkeyManager, NoopPasteController,
    NoopSettingsNavigator, NoopTrayController, NoopTrayHandle,
};
pub use paste::{PasteBackendKind, PasteController, PasteError};
pub use stub::{data_dir, DefaultPlatform, PlatformError};
pub use tiff_metadata::{parse_tiff_metadata, TiffMetadata, TiffResolutionUnit};
pub use tray::{
    TrayAction, TrayBackendKind, TrayController, TrayEntry, TrayError, TrayHandle, TrayOutcome,
};

#[cfg(all(target_os = "macos", feature = "macos-native"))]
pub use runtime::macos_main_queue_refresher::{
    outcome as active_app_refresh_outcome, MainQueueActiveAppRefresher, MainQueueRefresherError,
    DEFAULT_REFRESH_INTERVAL,
};

/// Platform-neutral handle the shell stores on `AppState` to keep the
/// installed active-app refresher alive for the lifetime of the
/// application.
///
/// On macOS with the `macos-native` feature enabled the alias
/// resolves to [`MainQueueActiveAppRefresher`], the real
/// `dispatch2`-backed timer that periodically re-evaluates the
/// cached `NSWorkspace` probe on the Cocoa main thread. On every
/// other platform — Linux X11, Linux Wayland, unsupported hosts,
/// builds compiled without the `macos-native` feature — the alias
/// resolves to the unit type `()`, which makes the slot
/// `Option<ActiveAppRefresherHandle>` zero-cost on the non-macOS
/// side without forcing the shell to repeat the
/// `cfg(target_os = "macos", feature = "macos-native")` gate at every
/// call site.
///
/// Dropping the value bound to this alias cancels the timer on
/// macOS and is a no-op on every other platform. The shell relies
/// on that contract to manage the lifetime of the refresher through
/// field ownership on `AppState`, independent of the host platform.
#[cfg(all(target_os = "macos", feature = "macos-native"))]
pub type ActiveAppRefresherHandle = MainQueueActiveAppRefresher;

/// Non-macOS / no-`macos-native` fallback for
/// [`ActiveAppRefresherHandle`]. Resolves to `()` so the shell can
/// carry an `Option<ActiveAppRefresherHandle>` slot without
/// referencing the macOS-only symbol. See the docstring on the
/// macOS-side alias for the full reasoning.
#[cfg(not(all(target_os = "macos", feature = "macos-native")))]
pub type ActiveAppRefresherHandle = ();
