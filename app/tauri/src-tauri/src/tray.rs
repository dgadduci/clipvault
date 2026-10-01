//! Tray / menu-bar construction.
//!
//! Wires Tauri's built-in [`tauri::tray::TrayIconBuilder`] to the
//! platform-agnostic [`clipvault_platform::TrayAction`] enum. Actions
//! that depend on not-yet-implemented specs emit a thin
//! `clipvault://capability-unavailable` event so the frontend can
//! surface the placeholder.

use std::sync::Arc;

use parking_lot::Mutex;
use tauri::menu::{MenuBuilder, MenuEvent};
use tauri::tray::{TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tracing::warn;

use clipvault_platform::{TrayAction, TrayController, TrayHandle};

/// Tray menu IDs. Stable so the frontend can correlate events.
const ID_OPEN_MAIN: &str = "clipvault://open_main_window";
const ID_OPEN_QUICK: &str = "clipvault://open_quick_search";
const ID_OPEN_FAVORITES: &str = "clipvault://open_favorites";
const ID_CLEAR_HISTORY: &str = "clipvault://clear_history";
const ID_OPEN_SETTINGS: &str = "clipvault://open_settings";
const ID_TOGGLE_CAPTURE: &str = "clipvault://toggle_clipboard_capture";
const ID_QUIT: &str = "clipvault://quit";

/// Tauri-backed tray handle. Implements the platform-agnostic
/// [`TrayHandle`] trait so the rest of the application can keep using
/// the same interface it uses for the [`crate::bootstrap::build_state`]
/// adapters.
pub struct TauriTrayHandle<R: Runtime = tauri::Wry> {
    app: AppHandle<R>,
    menu: Mutex<Vec<TrayAction>>,
    capture_enabled: Mutex<bool>,
    language: Mutex<String>,
}

impl<R: Runtime> TauriTrayHandle<R> {
    fn new(
        app: AppHandle<R>,
        actions: Vec<TrayAction>,
        capture_enabled: bool,
        language: String,
    ) -> Self {
        Self {
            app,
            menu: Mutex::new(actions),
            capture_enabled: Mutex::new(capture_enabled),
            language: Mutex::new(language),
        }
    }
}

impl<R: Runtime> TrayHandle for TauriTrayHandle<R> {
    fn set_menu(
        &self,
        entries: &[clipvault_platform::TrayEntry],
    ) -> Result<(), clipvault_platform::TrayError> {
        let mut menu = self.menu.lock();
        menu.clear();
        menu.extend(entries.iter().map(|e| e.action));
        let capture_enabled = *self.capture_enabled.lock();
        let language = self.language.lock().clone();
        rebuild_menu(&self.app, &menu, capture_enabled, &language)
    }

    fn invoke(
        &self,
        action: TrayAction,
    ) -> Result<clipvault_platform::TrayOutcome, clipvault_platform::TrayError> {
        if !action.is_supported_in_mvp() {
            let _ = self.app.emit(
                "clipvault://capability-unavailable",
                capability_payload(action),
            );
            return Ok(clipboard_platform_unavailable(action));
        }
        match action {
            TrayAction::OpenMainWindow => {
                if let Some(window) = self.app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            TrayAction::OpenQuickSearch => {
                let _ = self.app.emit("clipvault://quick-search", ());
            }
            TrayAction::Quit => {
                self.app.exit(0);
            }
            TrayAction::ToggleClipboardCapture => {
                crate::commands::toggle_capture_from_app(&self.app)
                    .map_err(|error| clipvault_platform::TrayError::backend(error.message))?;
            }
            TrayAction::OpenFavorites | TrayAction::ClearHistory | TrayAction::OpenSettings => {
                let _ = self.app.emit(
                    "clipvault://capability-unavailable",
                    capability_payload(action),
                );
                return Ok(clipboard_platform_unavailable(action));
            }
        }
        Ok(clipvault_platform::TrayOutcome::Delivered)
    }

    fn shutdown(&self) -> Result<(), clipvault_platform::TrayError> {
        Ok(())
    }
}

impl<R: Runtime> TauriTrayHandle<R> {
    fn update_capture_enabled(&self, enabled: bool) -> Result<(), clipvault_platform::TrayError> {
        let menu = self.menu.lock();
        *self.capture_enabled.lock() = enabled;
        let language = self.language.lock().clone();
        rebuild_menu(&self.app, &menu, enabled, &language)
    }

    fn update_language(&self, language: &str) -> Result<(), clipvault_platform::TrayError> {
        let menu = self.menu.lock();
        *self.language.lock() = language.to_string();
        let capture_enabled = *self.capture_enabled.lock();
        rebuild_menu(&self.app, &menu, capture_enabled, language)
    }
}

fn clipboard_platform_unavailable(action: TrayAction) -> clipvault_platform::TrayOutcome {
    clipvault_platform::TrayOutcome::Unavailable {
        action,
        reason: "tray action not implemented yet".into(),
    }
}

fn capability_payload(action: TrayAction) -> serde_json::Value {
    serde_json::json!({
        "action": action.as_str(),
        "capability": action.as_str(),
    })
}

fn rebuild_menu<R: Runtime>(
    app: &AppHandle<R>,
    actions: &[TrayAction],
    capture_enabled: bool,
    language: &str,
) -> Result<(), clipvault_platform::TrayError> {
    let menu = MenuBuilder::new(app)
        .items(&[
            &menu_item(
                app,
                ID_OPEN_MAIN,
                &crate::localization::text(language, "tray.main"),
                actions.contains(&TrayAction::OpenMainWindow),
            ),
            &menu_item(
                app,
                ID_OPEN_QUICK,
                &crate::localization::text(language, "tray.quick_search"),
                actions.contains(&TrayAction::OpenQuickSearch),
            ),
            &menu_item(
                app,
                ID_OPEN_FAVORITES,
                &crate::localization::text(language, "tray.favorites"),
                actions.contains(&TrayAction::OpenFavorites),
            ),
            &menu_item(
                app,
                ID_CLEAR_HISTORY,
                &crate::localization::text(language, "tray.clear_history"),
                actions.contains(&TrayAction::ClearHistory),
            ),
            &menu_item(
                app,
                ID_OPEN_SETTINGS,
                &crate::localization::text(language, "tray.settings"),
                actions.contains(&TrayAction::OpenSettings),
            ),
            &menu_item(
                app,
                ID_TOGGLE_CAPTURE,
                &capture_menu_label(capture_enabled, language),
                actions.contains(&TrayAction::ToggleClipboardCapture),
            ),
            &menu_separator(app),
            &menu_item(
                app,
                ID_QUIT,
                &crate::localization::text(language, "tray.quit"),
                actions.contains(&TrayAction::Quit),
            ),
        ])
        .build()
        .map_err(clipvault_platform::TrayError::backend)?;

    if let Some(tray) = app.tray_by_id("clipvault-tray") {
        tray.set_menu(Some(menu))
            .map_err(clipvault_platform::TrayError::backend)?;
    }
    Ok(())
}

fn capture_menu_label(enabled: bool, language: &str) -> String {
    let key = if enabled {
        "tray.pause_captures"
    } else {
        "tray.resume_captures"
    };
    #[cfg(target_os = "macos")]
    let shortcut = "⌘⌥⇧B";
    #[cfg(not(target_os = "macos"))]
    let shortcut = "Ctrl+Alt+Shift+B";
    crate::localization::text(language, key).replace("{shortcut}", shortcut)
}

fn menu_item<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    text: &str,
    enabled: bool,
) -> tauri::menu::MenuItem<R> {
    tauri::menu::MenuItemBuilder::with_id(id, text)
        .enabled(enabled)
        .build(app)
        .expect("menu item")
}

fn menu_separator<R: Runtime>(app: &AppHandle<R>) -> tauri::menu::PredefinedMenuItem<R> {
    tauri::menu::PredefinedMenuItem::separator(app).expect("separator")
}

/// Tauri-backed tray controller. Implements the platform-agnostic
/// [`TrayController`] trait so it slots in next to the noop controller
/// without changes to the rest of the shell.
pub struct TauriTrayController {
    handle: Arc<TauriTrayHandle>,
    // Tauri removes the native icon when the last `TrayIcon` instance drops.
    // The controller is managed by the app for its lifetime, so retaining it
    // here keeps the tray available after the main window is hidden.
    _icon: TrayIcon<tauri::Wry>,
}

impl TauriTrayController {
    pub fn install(
        app: &AppHandle<tauri::Wry>,
        capture_enabled: bool,
        language: &str,
        on_menu_event: impl Fn(&AppHandle<tauri::Wry>, MenuEvent) + Send + Sync + 'static,
        on_tray_event: impl Fn(&tauri::tray::TrayIcon<tauri::Wry>, TrayIconEvent)
            + Send
            + Sync
            + 'static,
    ) -> Result<Arc<Self>, tauri::Error> {
        let actions = vec![
            TrayAction::OpenMainWindow,
            TrayAction::OpenQuickSearch,
            TrayAction::OpenFavorites,
            TrayAction::ClearHistory,
            TrayAction::OpenSettings,
            TrayAction::ToggleClipboardCapture,
            TrayAction::Quit,
        ];
        let initial_menu = MenuBuilder::new(app)
            .items(&[
                &menu_item(
                    app,
                    ID_OPEN_MAIN,
                    &crate::localization::text(language, "tray.main"),
                    true,
                ),
                &menu_item(
                    app,
                    ID_OPEN_QUICK,
                    &crate::localization::text(language, "tray.quick_search"),
                    true,
                ),
                &menu_item(
                    app,
                    ID_OPEN_FAVORITES,
                    &crate::localization::text(language, "tray.favorites"),
                    true,
                ),
                &menu_item(
                    app,
                    ID_CLEAR_HISTORY,
                    &crate::localization::text(language, "tray.clear_history"),
                    true,
                ),
                &menu_item(
                    app,
                    ID_OPEN_SETTINGS,
                    &crate::localization::text(language, "tray.settings"),
                    true,
                ),
                &menu_item(
                    app,
                    ID_TOGGLE_CAPTURE,
                    &capture_menu_label(capture_enabled, language),
                    true,
                ),
                &menu_separator(app),
                &menu_item(
                    app,
                    ID_QUIT,
                    &crate::localization::text(language, "tray.quit"),
                    true,
                ),
            ])
            .build()?;

        let icon = TrayIconBuilder::with_id("clipvault-tray")
            .icon(
                app.default_window_icon()
                    .cloned()
                    .unwrap_or_else(|| tauri::image::Image::new_owned(vec![0u8; 4], 1, 1)),
            )
            .menu(&initial_menu)
            .on_menu_event(on_menu_event)
            .on_tray_icon_event(on_tray_event)
            .build(app)?;

        Ok(Arc::new(Self {
            handle: Arc::new(TauriTrayHandle::new(
                app.clone(),
                actions,
                capture_enabled,
                language.to_string(),
            )),
            _icon: icon,
        }))
    }

    /// Dispatch a menu action through the same Tauri adapter that owns the
    /// native icon and main window handle.
    pub fn invoke(
        &self,
        action: TrayAction,
    ) -> Result<clipvault_platform::TrayOutcome, clipvault_platform::TrayError> {
        self.handle.invoke(action)
    }

    pub fn set_capture_enabled(&self, enabled: bool) -> Result<(), clipvault_platform::TrayError> {
        self.handle.update_capture_enabled(enabled)
    }

    pub fn set_language(&self, language: &str) -> Result<(), clipvault_platform::TrayError> {
        self.handle.update_language(language)
    }
}

impl TrayController for TauriTrayController {
    fn install(&self) -> Result<Box<dyn TrayHandle>, clipvault_platform::TrayError> {
        Ok(Box::new(TauriTrayHandle {
            app: self.handle.app.clone(),
            menu: Mutex::new(self.handle.menu.lock().clone()),
            capture_enabled: Mutex::new(*self.handle.capture_enabled.lock()),
            language: Mutex::new(self.handle.language.lock().clone()),
        }))
    }

    fn name(&self) -> &'static str {
        clipvault_platform::TrayBackendKind::Tauri.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::{
        test::{mock_app, MockRuntime},
        WebviewUrl, WebviewWindowBuilder,
    };

    #[test]
    fn open_main_window_action_is_delivered_to_a_registered_main_window() {
        let app = mock_app();
        WebviewWindowBuilder::new(&app, "main", WebviewUrl::default())
            .build()
            .expect("mock main window");
        let tray = TauriTrayHandle::<MockRuntime>::new(
            app.handle().clone(),
            Vec::new(),
            true,
            "en".to_string(),
        );

        assert_eq!(
            tray.invoke(TrayAction::OpenMainWindow)
                .expect("open main window action"),
            clipvault_platform::TrayOutcome::Delivered
        );
    }

    #[test]
    fn capture_menu_label_reflects_the_current_state() {
        let active = capture_menu_label(true, "en");
        let paused = capture_menu_label(false, "en");
        assert!(active.starts_with("Pause captures ("));
        assert!(paused.starts_with("Resume captures ("));
        assert!(active.ends_with("B)"));
        assert!(paused.ends_with("B)"));
    }
}

/// Map a Tauri menu event ID back to the platform-agnostic
/// [`TrayAction`].
pub fn menu_event_to_action(id: &str) -> Option<TrayAction> {
    match id {
        ID_OPEN_MAIN => Some(TrayAction::OpenMainWindow),
        ID_OPEN_QUICK => Some(TrayAction::OpenQuickSearch),
        ID_OPEN_FAVORITES => Some(TrayAction::OpenFavorites),
        ID_CLEAR_HISTORY => Some(TrayAction::ClearHistory),
        ID_OPEN_SETTINGS => Some(TrayAction::OpenSettings),
        ID_TOGGLE_CAPTURE => Some(TrayAction::ToggleClipboardCapture),
        ID_QUIT => Some(TrayAction::Quit),
        _ => None,
    }
}

#[allow(dead_code)]
fn _warn_menu_event(error: tauri::Error) {
    warn!(error = %error, "menu event handler error");
}
