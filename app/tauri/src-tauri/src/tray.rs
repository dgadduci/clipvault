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
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tracing::warn;

use clipvault_platform::{TrayAction, TrayController, TrayHandle};

/// Tray menu IDs. Stable so the frontend can correlate events.
const ID_OPEN_MAIN: &str = "clipvault://open_main_window";
const ID_OPEN_QUICK: &str = "clipvault://open_quick_search";
const ID_OPEN_FAVORITES: &str = "clipvault://open_favorites";
const ID_CLEAR_HISTORY: &str = "clipvault://clear_history";
const ID_OPEN_SETTINGS: &str = "clipvault://open_settings";
const ID_QUIT: &str = "clipvault://quit";

/// Tauri-backed tray handle. Implements the platform-agnostic
/// [`TrayHandle`] trait so the rest of the application can keep using
/// the same interface it uses for the [`crate::bootstrap::build_state`]
/// adapters.
pub struct TauriTrayHandle {
    app: AppHandle<tauri::Wry>,
    menu: Mutex<Vec<TrayAction>>,
}

impl TauriTrayHandle {
    fn new(app: AppHandle<tauri::Wry>, actions: Vec<TrayAction>) -> Self {
        Self {
            app,
            menu: Mutex::new(actions),
        }
    }
}

impl TrayHandle for TauriTrayHandle {
    fn set_menu(
        &self,
        entries: &[clipvault_platform::TrayEntry],
    ) -> Result<(), clipvault_platform::TrayError> {
        let mut menu = self.menu.lock();
        menu.clear();
        menu.extend(entries.iter().map(|e| e.action));
        rebuild_menu(&self.app, &menu)
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
) -> Result<(), clipvault_platform::TrayError> {
    let menu = MenuBuilder::new(app)
        .items(&[
            &menu_item(
                app,
                ID_OPEN_MAIN,
                "Open ClipVault",
                actions.contains(&TrayAction::OpenMainWindow),
            ),
            &menu_item(
                app,
                ID_OPEN_QUICK,
                "Open quick search",
                actions.contains(&TrayAction::OpenQuickSearch),
            ),
            &menu_item(
                app,
                ID_OPEN_FAVORITES,
                "Favorites",
                actions.contains(&TrayAction::OpenFavorites),
            ),
            &menu_item(
                app,
                ID_CLEAR_HISTORY,
                "Clear history…",
                actions.contains(&TrayAction::ClearHistory),
            ),
            &menu_item(
                app,
                ID_OPEN_SETTINGS,
                "Settings",
                actions.contains(&TrayAction::OpenSettings),
            ),
            &menu_separator(app),
            &menu_item(
                app,
                ID_QUIT,
                "Quit ClipVault",
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
}

impl TauriTrayController {
    pub fn install(
        app: &AppHandle<tauri::Wry>,
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
            TrayAction::Quit,
        ];
        let initial_menu = MenuBuilder::new(app)
            .items(&[
                &menu_item(app, ID_OPEN_MAIN, "Open ClipVault", true),
                &menu_item(app, ID_OPEN_QUICK, "Open quick search", true),
                &menu_item(app, ID_OPEN_FAVORITES, "Favorites", true),
                &menu_item(app, ID_CLEAR_HISTORY, "Clear history…", true),
                &menu_item(app, ID_OPEN_SETTINGS, "Settings", true),
                &menu_separator(app),
                &menu_item(app, ID_QUIT, "Quit ClipVault", true),
            ])
            .build()?;

        TrayIconBuilder::with_id("clipvault-tray")
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
            handle: Arc::new(TauriTrayHandle::new(app.clone(), actions)),
        }))
    }
}

impl TrayController for TauriTrayController {
    fn install(&self) -> Result<Box<dyn TrayHandle>, clipvault_platform::TrayError> {
        Ok(Box::new(TauriTrayHandle {
            app: self.handle.app.clone(),
            menu: Mutex::new(self.handle.menu.lock().clone()),
        }))
    }

    fn name(&self) -> &'static str {
        clipvault_platform::TrayBackendKind::Tauri.as_str()
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
        ID_QUIT => Some(TrayAction::Quit),
        _ => None,
    }
}

#[allow(dead_code)]
fn _warn_menu_event(error: tauri::Error) {
    warn!(error = %error, "menu event handler error");
}
