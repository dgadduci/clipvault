//! Concrete adapters wired by the bootstrap when the corresponding
//! feature is enabled. Each submodule is feature-gated so the crate
//! still compiles in environments where the backing crate is not
//! available (notably on `Other` hosts and in unit tests).
//!
//! ## Active-application detection on Wayland
//!
//! The `privacy-settings` capability asks the platform probe for the
//! currently focused application so the blacklist can match a stable
//! identifier. On Wayland there is no portable `wl_app_id` query that
//! works across compositors without `wayland-client`; the MVP therefore
//! resolves the Wayland branch to the `NoopActiveApplicationProbe`,
//! which always returns `Ok(None)`. That keeps the blacklist benign
//! (no source → no match → event flows through) until a future change
//! adds a per-compositor adapter.

#[cfg(feature = "clipboard-arboard")]
pub mod clipboard_arboard;

pub mod composite_clipboard;

#[cfg(feature = "hotkey-global")]
pub mod hotkey_global;

#[cfg(all(target_os = "macos", feature = "macos-native"))]
pub mod macos_active_app;
#[cfg(all(target_os = "macos", feature = "macos-native"))]
pub mod macos_app_metadata;
#[cfg(all(target_os = "macos", feature = "macos-native"))]
pub mod macos_app_picker;
#[cfg(all(target_os = "macos", feature = "macos-native"))]
pub mod macos_clipboard;
#[cfg(all(target_os = "macos", feature = "macos-native"))]
pub mod macos_clipboard_main_queue;
#[cfg(all(target_os = "macos", feature = "macos-native"))]
pub mod macos_main_queue_refresher;
#[cfg(all(target_os = "macos", feature = "macos-native"))]
pub mod macos_paste;
#[cfg(all(target_os = "macos", feature = "macos-native"))]
pub mod macos_settings;

#[cfg(target_os = "linux")]
pub mod linux_app_picker;
#[cfg(target_os = "linux")]
pub mod linux_settings;
#[cfg(all(target_os = "linux", feature = "linux-x11"))]
pub mod linux_x11_active_app;
#[cfg(all(target_os = "linux", feature = "linux-x11"))]
pub mod linux_x11_paste;
