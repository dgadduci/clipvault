//! Tauri shell for ClipVault.
//!
//! This crate is intentionally a thin adapter: every command is at
//! most a few lines long and delegates to `clipvault-core`. No
//! business logic lives here.

mod bootstrap;
mod commands;
mod main_window_layout;
mod metadata_scheduler;
mod state;
mod tray;

use std::sync::Arc;

use clipvault_core::{RedactingMakeWriter, WatchTickOutcome};
use tauri::menu::MenuEvent;
use tauri::tray::{TrayIcon, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, EnvFilter};

use crate::bootstrap::{
    build_state, install_capture_loop, refresh_active_app_cached, register_default_hotkey,
};
use crate::commands::run_retention;
use crate::state::SharedState;
use crate::tray::{menu_event_to_action, TauriTrayController};

fn main() {
    init_tracing();

    tauri::Builder::default()
        .setup(|app| {
            // Resize the main window to the available work area before
            // anything else so the first paint already shows a desktop
            // that fills the monitor horizontally. The operation only
            // runs once during `setup`; afterwards the OS tracks the
            // user's manual resize and we MUST NOT loop on
            // `ResizeObserver` events. A monitor query failure falls
            // back to the conf-file defaults declared in
            // `tauri.conf.json` so the setup never blocks startup.
            resize_main_window_to_monitor(app);

            let state = match build_state() {
                Ok(state) => state,
                Err(error) => {
                    error!(error = %error, "ClipVault bootstrap failed");
                    return Err(error);
                }
            };

            // Install the Tauri-backed tray. The tray controller takes
            // ownership of the menu wiring; the rest of the app reads
            // it through `state.adapters.tray()`.
            let menu_handler = move |handle: &AppHandle<tauri::Wry>, event: MenuEvent| {
                on_menu_event(handle.clone(), event);
            };
            let tray_handler = move |tray: &TrayIcon<tauri::Wry>, event: TrayIconEvent| {
                on_tray_event(tray, event);
            };
            match TauriTrayController::install(app.handle(), menu_handler, tray_handler) {
                Ok(controller) => {
                    info!("tray installed");
                    let _ = controller;
                }
                Err(error) => {
                    warn!(error = %error, "tray installation failed; running without tray");
                }
            }

            // Register the default global hotkey.
            let outcome = register_default_hotkey(&state, app.handle());
            info!(kind = outcome.kind(), "default hotkey outcome");

            // Apply the configured retention policy as part of the
            // startup pass. Failures are logged but never block the
            // first paint of the UI.
            run_retention(&state.context);

            // Warm the active-app cache synchronously on the main
            // thread before the background loop starts so the very
            // first capture tick already sees a non-empty cache.
            // `refresh_active_app_cached` skips the synchronous
            // helper when called from the main thread (the typical
            // Tauri-command path) and goes straight to the inner
            // probe, so this warm-up is a single, fast call.
            let _ = refresh_active_app_cached(&state.context, Some(app.handle()));

            // Register the managed state BEFORE spinning up the
            // capture loop. The loop's first iteration runs in
            // parallel with the rest of the setup callback; if the
            // state is not yet managed when the loop fires, the
            // diagnostics endpoint can race the loop and report
            // `pending` forever even though the loop is alive.
            // The macOS main-queue refresher is owned by `state`
            // (installed exactly once inside `build_state`) and
            // lives for the application's lifetime through this
            // `SharedState` — the setup callback MUST NOT touch it.
            app.manage(SharedState::new(state));

            // Schedule the main-thread refresh of the active-app
            // cache and start polling the clipboard from a background
            // thread. The capture loop relies on the cached probe to
            // resolve the source identifier at every tick so the
            // blacklist can block content from ignored applications.
            if let Some(shared) = app.try_state::<SharedState>() {
                install_capture_loop(shared.app_state(), app.handle());
            } else {
                warn!("shared state not available; capture loop not installed");
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Hide instead of close: the application keeps running
                // from the tray. Quit is initiated through the tray.
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::clipvault_diagnostics,
            commands::clipvault_database_path,
            commands::clipvault_migrations_applied,
            commands::clipvault_capture_text,
            commands::clipvault_recent_entries,
            commands::clipvault_recent_entries_filtered,
            commands::clipvault_history_count,
            commands::clipvault_search_entries,
            commands::clipvault_capture_tick,
            commands::clipvault_platform_capabilities,
            commands::clipvault_paste_entry,
            commands::clipvault_active_application,
            commands::clipvault_active_app_diagnostics,
            commands::clipvault_refresh_active_app_diagnostics,
            commands::clipvault_register_hotkey,
            commands::clipvault_open_platform_settings,
            commands::clipvault_refresh_capabilities,
            commands::clipvault_shutdown,
            commands::clipvault_set_favorite,
            commands::clipvault_delete_entry,
            commands::clipvault_clear_history,
            commands::clipvault_unorganized_clearable_count,
            commands::clipvault_clear_unorganized_history,
            commands::clipvault_apply_retention,
            commands::clipvault_retention_preview,
            commands::clipvault_settings_get,
            commands::clipvault_settings_set,
            commands::clipvault_ignored_apps_list,
            commands::clipvault_ignored_apps_add,
            commands::clipvault_ignored_apps_remove,
            commands::clipvault_ignored_app_pick_and_add,
            commands::clipvault_ignored_apps_list_with_metadata,
            commands::clipvault_ignored_app_icon,
            commands::clipvault_set_entry_title,
            commands::clipvault_source_app_icon,
            commands::clipvault_clipboard_asset,
            commands::clipvault_rich_text_preview,
            commands::clipvault_organization_snapshot,
            commands::clipvault_collections_create,
            commands::clipvault_collections_rename,
            commands::clipvault_collections_delete,
            commands::clipvault_tags_create,
            commands::clipvault_tags_rename,
            commands::clipvault_tags_delete,
            commands::clipvault_entry_collections,
            commands::clipvault_entry_tags,
            commands::clipvault_entry_collections_set,
            commands::clipvault_entry_tags_set,
            commands::clipvault_entry_upsert_tag,
            commands::clipvault_entry_remove_from_collection,
            commands::clipvault_history_collection_id,
            commands::clipvault_source_applications,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ClipVault")
        .run(handle_run_event);
}

fn handle_run_event<R: tauri::Runtime>(app: &AppHandle<R>, event: RunEvent) {
    if let RunEvent::ExitRequested { .. } = event {
        cleanup(app);
        info!("ClipVault exiting cleanly");
    }
}

fn cleanup<R: tauri::Runtime>(app: &AppHandle<R>) {
    let shared = app.try_state::<SharedState>();
    let Some(shared) = shared else {
        return;
    };
    // Stop the background capture loop so it does not race with
    // the retention pass that follows.
    shared.signal_capture_loop_stop();
    let _ = shared.adapters().hotkey().unregister_all();
    if let Some(tray) = app.tray_by_id("clipvault-tray") {
        let _ = tray.set_menu(None::<tauri::menu::Menu<R>>);
    }
    // Best-effort retention pass on shutdown so a long-running
    // session cleans up expired rows before the next start.
    run_retention(shared.context());
}

fn on_menu_event(handle: AppHandle<tauri::Wry>, event: MenuEvent) {
    let Some(action) = menu_event_to_action(event.id().as_ref()) else {
        warn!("unknown tray menu id: {}", event.id().as_ref());
        return;
    };
    if let Some(shared) = handle.try_state::<SharedState>() {
        if let Ok(handle) = shared.app_state().adapters.tray().install() {
            let _ = handle.invoke(action);
        }
    }
}

fn on_tray_event(_tray: &TrayIcon<tauri::Wry>, _event: TrayIconEvent) {
    // Left-click toggling is the OS default; we keep the default.
}

#[allow(dead_code)]
fn _unused_watcher_outcome(outcome: &WatchTickOutcome) -> &'static str {
    match outcome {
        WatchTickOutcome::Captured(_) => "captured",
        WatchTickOutcome::Unchanged => "unchanged",
        WatchTickOutcome::Suppressed => "suppressed",
        WatchTickOutcome::Ignored => "ignored",
        WatchTickOutcome::Failed { .. } => "failed",
    }
}

fn init_tracing() {
    let _ = fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .with_writer(RedactingMakeWriter::new(std::io::stderr))
        .try_init();
}

#[allow(dead_code)]
fn _ensure_arc(_: &Arc<()>) {}

/// Position the main window once during the Tauri `setup` callback.
///
/// The window is sized to fill the primary monitor's work area
/// horizontally and aligned to the top edge so the first paint
/// already lands in the documented product position. The helper is
/// intentionally a one-shot pass:
///
/// - the size and position are only ever written during the `setup`
///   callback, so a later manual user move/resize is preserved by
///   the OS and never clobbered by a `ResizeObserver` loop;
/// - a missing monitor (headless, RDP, X11 without `RANDR`, …) keeps
///   the `tauri.conf.json` defaults, which the conf already picked
///   for that case;
/// - the helper never enters the path that resizes or moves the
///   transient `quick-paste` window: only the `main` label is
///   touched.
///
/// The math lives in [`crate::main_window_layout`] so the centring
/// logic is unit tested without a Tauri runtime.
fn resize_main_window_to_monitor(app: &mut tauri::App) {
    use crate::main_window_layout::compute_main_window_layout;
    use tauri::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize};

    let Some(window) = app.get_webview_window("main") else {
        warn!("main window not present at setup; skipping initial layout");
        return;
    };

    let monitor = match window.primary_monitor() {
        Ok(Some(monitor)) => monitor,
        Ok(None) => {
            warn!("no primary monitor reported; keeping conf defaults");
            return;
        }
        Err(error) => {
            warn!(error = %error, "primary monitor query failed; keeping conf defaults");
            return;
        }
    };

    let scale = monitor.scale_factor();
    let work_area = monitor.work_area();
    let work_tuple = (
        work_area.position.x as f64,
        work_area.position.y as f64,
        work_area.size.width.max(1) as f64,
        work_area.size.height.max(1) as f64,
    );
    let layout = compute_main_window_layout(work_tuple, scale);

    let logical_size = LogicalSize::new(layout.logical_size.0, layout.logical_size.1);
    let physical_size = PhysicalSize::new(layout.physical_size.0, layout.physical_size.1);

    if let Err(error) = window.set_size(logical_size) {
        warn!(error = %error, "logical resize failed; falling back to physical");
        if let Err(physical_error) = window.set_size(physical_size) {
            warn!(error = %physical_error, "physical resize failed; keeping conf defaults");
        }
    }

    let logical_position =
        LogicalPosition::new(layout.logical_position.0, layout.logical_position.1);
    let physical_position =
        PhysicalPosition::new(layout.physical_position.0, layout.physical_position.1);

    if let Err(error) = window.set_position(logical_position) {
        warn!(error = %error, "logical position failed; falling back to physical");
        if let Err(physical_error) = window.set_position(physical_position) {
            warn!(error = %physical_error, "physical position failed; keeping conf defaults");
        }
    }

    info!(
        logical_width = layout.logical_size.0,
        logical_height = layout.logical_size.1,
        logical_x = layout.logical_position.0,
        logical_y = layout.logical_position.1,
        scale = layout.scale_factor,
        "main window positioned at top center of the primary monitor"
    );
}
