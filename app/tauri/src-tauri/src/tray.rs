//! Tray / menu-bar construction.
//!
//! Wires Tauri's built-in [`tauri::tray::TrayIconBuilder`] to the
//! platform-agnostic [`clipvault_platform::TrayAction`] enum. Actions
//! whose application flows are not implemented emit a thin
//! `clipvault://capability-unavailable` event for the frontend.

use std::sync::Arc;

use parking_lot::Mutex;
use tauri::menu::{MenuBuilder, MenuEvent};
use tauri::tray::{TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow, WebviewWindowBuilder};
use tracing::warn;

use clipvault_platform::{TrayAction, TrayController, TrayHandle};

/// Tray menu IDs. Stable so the frontend can correlate events.
const ID_OPEN_MAIN: &str = "clipvault://open_main_window";
const ID_OPEN_QUICK: &str = "clipvault://open_quick_search";
const ID_CLEAR_HISTORY: &str = "clipvault://clear_history";
const ID_OPEN_SETTINGS: &str = "clipvault://open_settings";
const ID_OPEN_ABOUT: &str = "clipvault://open_about";
const ID_TOGGLE_CAPTURE: &str = "clipvault://toggle_clipboard_capture";
const ID_QUIT: &str = "clipvault://quit";
const TRAY_MENU_ACTION_EVENT: &str = "clipvault://tray-menu-action";
const MAIN_WINDOW_LABEL: &str = "main";

/// Tauri-backed tray handle. Implements the platform-agnostic
/// [`TrayHandle`] trait so the rest of the application can keep using
/// the same interface it uses for the [`crate::bootstrap::build_state`]
/// adapters.
pub struct TauriTrayHandle<R: Runtime = tauri::Wry> {
    app: AppHandle<R>,
    menu: Mutex<Vec<TrayAction>>,
    capture_enabled: Mutex<bool>,
    language: Mutex<String>,
    capture_shortcut: Mutex<String>,
}

impl<R: Runtime> TauriTrayHandle<R> {
    fn new(
        app: AppHandle<R>,
        actions: Vec<TrayAction>,
        capture_enabled: bool,
        language: String,
        capture_shortcut: String,
    ) -> Self {
        Self {
            app,
            menu: Mutex::new(actions),
            capture_enabled: Mutex::new(capture_enabled),
            language: Mutex::new(language),
            capture_shortcut: Mutex::new(capture_shortcut),
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
        let shortcut = self.capture_shortcut.lock().clone();
        rebuild_menu(&self.app, &menu, capture_enabled, &language, &shortcut)
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
                bring_main_window_forward(&self.app)?;
            }
            TrayAction::OpenQuickSearch => {
                let _ = self.app.emit("clipvault://quick-search", ());
            }
            TrayAction::ClearHistory | TrayAction::OpenSettings | TrayAction::OpenAbout => {
                bring_main_window_forward(&self.app)?;
                self.app
                    .emit_to("main", TRAY_MENU_ACTION_EVENT, action.as_str())
                    .map_err(clipvault_platform::TrayError::backend)?;
            }
            TrayAction::Quit => {
                self.app.exit(0);
            }
            TrayAction::ToggleClipboardCapture => {
                crate::commands::toggle_capture_from_app(&self.app)
                    .map_err(|error| clipvault_platform::TrayError::backend(error.message))?;
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
        let shortcut = self.capture_shortcut.lock().clone();
        rebuild_menu(&self.app, &menu, enabled, &language, &shortcut)
    }

    fn update_language(&self, language: &str) -> Result<(), clipvault_platform::TrayError> {
        let menu = self.menu.lock();
        *self.language.lock() = language.to_string();
        let capture_enabled = *self.capture_enabled.lock();
        let shortcut = self.capture_shortcut.lock().clone();
        rebuild_menu(&self.app, &menu, capture_enabled, language, &shortcut)
    }

    fn update_capture_shortcut(&self, shortcut: &str) -> Result<(), clipvault_platform::TrayError> {
        let menu = self.menu.lock();
        *self.capture_shortcut.lock() = shortcut.to_string();
        let capture_enabled = *self.capture_enabled.lock();
        let language = self.language.lock().clone();
        rebuild_menu(&self.app, &menu, capture_enabled, &language, shortcut)
    }
}

fn bring_main_window_forward<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<(), clipvault_platform::TrayError> {
    let window = get_or_create_main_window(app)?;

    if window
        .is_minimized()
        .map_err(clipvault_platform::TrayError::backend)?
    {
        window
            .unminimize()
            .map_err(clipvault_platform::TrayError::backend)?;
    }
    window
        .show()
        .map_err(clipvault_platform::TrayError::backend)?;
    window
        .set_focus()
        .map_err(clipvault_platform::TrayError::backend)?;

    // Native tray menus may dismiss after their callback returns. Request
    // focus again on the app thread so the menu cannot leave itself above the
    // main window's activation request.
    let focus_window = window.clone();
    window
        .run_on_main_thread(move || {
            if let Err(error) = focus_window.set_focus() {
                warn!(error = %error, "failed to focus the main window after tray activation");
            }
        })
        .map_err(clipvault_platform::TrayError::backend)?;
    Ok(())
}

fn get_or_create_main_window<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<WebviewWindow<R>, clipvault_platform::TrayError> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        #[cfg(target_os = "linux")]
        apply_linux_main_window_icon(&window);
        return Ok(window);
    }

    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == MAIN_WINDOW_LABEL)
        .ok_or_else(|| {
            clipvault_platform::TrayError::backend("main window configuration unavailable")
        })?;

    let builder = WebviewWindowBuilder::from_config(app, config)
        .map_err(clipvault_platform::TrayError::backend)?;
    let window = match builder.build() {
        Ok(window) => Ok(window),
        Err(error) => app
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| clipvault_platform::TrayError::backend(error)),
    }?;
    #[cfg(target_os = "linux")]
    apply_linux_main_window_icon(&window);
    Ok(window)
}

/// Apply the bundled square brand icon directly to the native Linux window.
/// KWin uses the app ID and its desktop entry for grouping, but the native
/// window icon is still needed by X11 and by shells that fall back to it.
#[cfg(target_os = "linux")]
pub(crate) fn apply_linux_main_window_icon<R: Runtime>(window: &WebviewWindow<R>) {
    let result = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png"))
        .and_then(|icon| window.set_icon(icon));
    if let Err(error) = result {
        warn!(error = %error, "failed to apply the ClipVault icon to the main window");
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
    capture_shortcut: &str,
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
                ID_OPEN_ABOUT,
                &crate::localization::text(language, "tray.about"),
                actions.contains(&TrayAction::OpenAbout),
            ),
            &menu_item(
                app,
                ID_TOGGLE_CAPTURE,
                &capture_menu_label(capture_enabled, language, capture_shortcut),
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

fn capture_menu_label(enabled: bool, language: &str, shortcut: &str) -> String {
    let key = if enabled {
        "tray.pause_captures"
    } else {
        "tray.resume_captures"
    };
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
        capture_shortcut: &str,
        on_menu_event: impl Fn(&AppHandle<tauri::Wry>, MenuEvent) + Send + Sync + 'static,
        on_tray_event: impl Fn(&tauri::tray::TrayIcon<tauri::Wry>, TrayIconEvent)
            + Send
            + Sync
            + 'static,
    ) -> Result<Arc<Self>, tauri::Error> {
        let actions = vec![
            TrayAction::OpenMainWindow,
            TrayAction::OpenQuickSearch,
            TrayAction::ClearHistory,
            TrayAction::OpenSettings,
            TrayAction::OpenAbout,
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
                    ID_OPEN_ABOUT,
                    &crate::localization::text(language, "tray.about"),
                    true,
                ),
                &menu_item(
                    app,
                    ID_TOGGLE_CAPTURE,
                    &capture_menu_label(capture_enabled, language, capture_shortcut),
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

        let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))?;
        let icon = TrayIconBuilder::with_id("clipvault-tray")
            .icon(tray_icon)
            .icon_as_template(cfg!(target_os = "macos"))
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
                capture_shortcut.to_string(),
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

    pub fn set_capture_shortcut(
        &self,
        shortcut: &str,
    ) -> Result<(), clipvault_platform::TrayError> {
        self.handle.update_capture_shortcut(shortcut)
    }
}

impl TrayController for TauriTrayController {
    fn install(&self) -> Result<Box<dyn TrayHandle>, clipvault_platform::TrayError> {
        Ok(Box::new(TauriTrayHandle {
            app: self.handle.app.clone(),
            menu: Mutex::new(self.handle.menu.lock().clone()),
            capture_enabled: Mutex::new(*self.handle.capture_enabled.lock()),
            language: Mutex::new(self.handle.language.lock().clone()),
            capture_shortcut: Mutex::new(self.handle.capture_shortcut.lock().clone()),
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
    fn bundled_main_window_icon_is_a_decodable_square_png() {
        let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png"))
            .expect("decode bundled main window icon");

        assert_eq!(icon.width(), 512);
        assert_eq!(icon.height(), 512);
        assert_eq!(icon.rgba().len(), 512 * 512 * 4);
    }

    #[test]
    fn open_main_window_action_is_delivered_to_a_registered_main_window() {
        let app = mock_app();
        let window = WebviewWindowBuilder::new(&app, "main", WebviewUrl::default())
            .build()
            .expect("mock main window");
        let tray = TauriTrayHandle::<MockRuntime>::new(
            app.handle().clone(),
            Vec::new(),
            true,
            "en".to_string(),
            "Ctrl+Alt+Shift+B".to_string(),
        );

        window.hide().expect("hide main window");
        assert_eq!(
            tray.invoke(TrayAction::OpenMainWindow)
                .expect("restore hidden main window"),
            clipvault_platform::TrayOutcome::Delivered
        );
        assert!(window.is_visible().expect("main window visibility"));

        window.minimize().expect("minimize main window");
        assert_eq!(
            tray.invoke(TrayAction::OpenMainWindow)
                .expect("restore minimized main window"),
            clipvault_platform::TrayOutcome::Delivered
        );
        assert!(!window.is_minimized().expect("main window minimized state"));
        assert_eq!(app.webview_windows().len(), 1);
    }

    #[test]
    fn implemented_ui_menu_actions_are_delivered_to_main_window() {
        let app = mock_app();
        WebviewWindowBuilder::new(&app, "main", WebviewUrl::default())
            .build()
            .expect("mock main window");
        let tray = TauriTrayHandle::<MockRuntime>::new(
            app.handle().clone(),
            Vec::new(),
            true,
            "en".to_string(),
            "Ctrl+Alt+Shift+B".to_string(),
        );

        for action in [
            TrayAction::ClearHistory,
            TrayAction::OpenSettings,
            TrayAction::OpenAbout,
        ] {
            assert_eq!(
                tray.invoke(action).expect("tray action"),
                clipvault_platform::TrayOutcome::Delivered
            );
        }
    }

    #[test]
    fn tray_menu_ids_cover_working_actions_and_do_not_dispatch_favorites() {
        assert_eq!(
            menu_event_to_action(ID_OPEN_MAIN),
            Some(TrayAction::OpenMainWindow)
        );
        assert_eq!(
            menu_event_to_action(ID_CLEAR_HISTORY),
            Some(TrayAction::ClearHistory)
        );
        assert_eq!(
            menu_event_to_action(ID_OPEN_SETTINGS),
            Some(TrayAction::OpenSettings)
        );
        assert_eq!(
            menu_event_to_action(ID_OPEN_ABOUT),
            Some(TrayAction::OpenAbout)
        );
        assert_eq!(menu_event_to_action("clipvault://open_favorites"), None);
    }

    #[test]
    fn capture_menu_label_reflects_the_current_state() {
        let active = capture_menu_label(true, "en", "Ctrl+Alt+Shift+B");
        let paused = capture_menu_label(false, "en", "Ctrl+Alt+Shift+B");
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
        ID_CLEAR_HISTORY => Some(TrayAction::ClearHistory),
        ID_OPEN_SETTINGS => Some(TrayAction::OpenSettings),
        ID_OPEN_ABOUT => Some(TrayAction::OpenAbout),
        ID_TOGGLE_CAPTURE => Some(TrayAction::ToggleClipboardCapture),
        ID_QUIT => Some(TrayAction::Quit),
        _ => None,
    }
}

#[allow(dead_code)]
fn _warn_menu_event(error: tauri::Error) {
    warn!(error = %error, "menu event handler error");
}
