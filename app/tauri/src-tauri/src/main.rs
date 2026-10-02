//! Tauri shell for ClipVault.
//!
//! This crate is intentionally a thin adapter: every command is at
//! most a few lines long and delegates to `clipvault-core`. No
//! business logic lives here.

mod bootstrap;
mod commands;
#[cfg(all(target_os = "linux", feature = "linux-gnome-shell-integration"))]
mod gnome_integration;
#[cfg(all(target_os = "linux", feature = "linux-kde-kwin-integration"))]
mod kde_kwin_integration;
mod keyboard_shortcuts;
mod localization;
mod main_window_layout;
mod metadata_scheduler;
mod state;
mod tray;

#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clipvault_core::{RedactingMakeWriter, WatchTickOutcome};
use tauri::menu::MenuEvent;
use tauri::tray::{TrayIcon, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, EnvFilter};

use crate::bootstrap::{
    build_state, install_capture_loop, platform_hotkey_binding, refresh_active_app_cached,
    register_global_shortcut, stop_network_subsystems, QUICK_SEARCH_EVENT,
};
use crate::commands::run_retention;
use crate::state::SharedState;
use crate::tray::{menu_event_to_action, TauriTrayController};

const WINDOW_LIFECYCLE_DEBUG_ENV: &str = "CLIPVAULT_DEBUG_WINDOW_LIFECYCLE";

fn main() {
    // X11 thread-safety preflight: must run before Tauri/GTK or
    // the global-hotkey backend open Xlib so the rule
    // `XInitThreads` enables is in place for the whole process.
    // The helper is a no-op on every platform other than Linux
    // builds with the `linux-xlib-init` feature enabled.
    #[cfg(all(target_os = "linux", feature = "linux-xlib-init"))]
    {
        use clipvault_platform::xlib_init_once;
        let _ = xlib_init_once();
    }

    init_tracing();

    let builder = tauri::Builder::default().plugin(tauri_plugin_process::init());

    // The updater public key is a public GitHub Actions variable. It is
    // compiled into release builds by the release workflow and deliberately
    // omitted from local development builds, where the frontend also disables
    // update checks. Keeping it out of source avoids committing a key before
    // the matching private signing key has been provisioned.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let builder = if let Some(public_key) = option_env!("TAURI_UPDATER_PUBLIC_KEY")
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        builder.plugin(
            tauri_plugin_updater::Builder::new()
                .pubkey(public_key)
                .build(),
        )
    } else {
        builder
    };

    builder
        .setup(|app| {
            trace_main_window_lifecycle_for_app(app.handle(), "configured", None);
            trace_main_window_lifecycle_for_app(app.handle(), "setup_entered", None);

            // Request the monitor-sized main window before building
            // state. Linux also registers a one-shot correction for
            // the first compositor-resolved size; it is disarmed before
            // applying anything so later manual resizes stay user
            // controlled. A monitor query failure keeps the defaults
            // declared in `tauri.conf.json` and never blocks startup.
            resize_main_window_to_monitor(app);
            trace_main_window_lifecycle_for_app(app.handle(), "layout_completed", None);

            let state = match build_state() {
                Ok(state) => {
                    trace_main_window_lifecycle_for_app(app.handle(), "state_built", None);
                    state
                }
                Err(error) => {
                    trace_main_window_lifecycle_for_app(app.handle(), "state_build_failed", None);
                    error!(error = %error, "ClipVault bootstrap failed");
                    return Err(error);
                }
            };

            #[cfg(all(target_os = "linux", feature = "linux-gnome-shell-integration"))]
            configure_gnome_event_sink(&state, app.handle());

            #[cfg(all(target_os = "linux", feature = "linux-kde-kwin-integration"))]
            configure_kde_capture_toggle_sink(&state, app.handle());

            #[cfg(all(target_os = "linux", feature = "linux-kde-kwin-integration"))]
            if let Some(integration) = state.kde_kwin_integration.as_ref().cloned() {
                let context = state.context.clone();
                let fallback = state.adapters.active_app();
                match crate::kde_kwin_integration::read_bundled_script(app.handle()) {
                    Ok(template) => {
                        let settings = state.context.settings().load(&state.context);
                        let shortcuts = state
                            .context
                            .settings()
                            .load_keyboard_shortcuts(&state.context);
                        let bundled = match crate::kde_kwin_integration::configure_bundled_shortcuts(
                            &template,
                            &shortcuts,
                            &settings.language,
                        ) {
                            Ok(value) => value,
                            Err(error) => {
                                warn!(error = %error, "KWin shortcut settings could not be prepared");
                                template
                            }
                        };
                        tauri::async_runtime::spawn(async move {
                            if let Err(error) = integration
                                .reactivate_if_consented(&context, fallback, &bundled)
                                .await
                            {
                                warn!(
                                    kind = error.stable_kind(),
                                    "KWin integration startup failed"
                                );
                            }
                        });
                    }
                    Err(error) => {
                        warn!(error = %error, "KWin integration script resources unavailable");
                    }
                }
            }

            // Install and retain the Tauri-backed tray. Its managed state owns
            // the native icon and is the sole dispatcher for native menu
            // actions; the core retains no window-specific behavior.
            let menu_handler = move |handle: &AppHandle<tauri::Wry>, event: MenuEvent| {
                on_menu_event(handle.clone(), event);
            };
            let tray_handler = move |tray: &TrayIcon<tauri::Wry>, event: TrayIconEvent| {
                on_tray_event(tray, event);
            };
            let initial_shortcuts = state
                .context
                .settings()
                .load_keyboard_shortcuts(&state.context);
            let capture_shortcut = initial_shortcuts
                .iter()
                .find(|binding| binding.id == "toggle_clipboard_capture")
                .map(|binding| {
                    commands::format_keyboard_shortcut(
                        binding,
                        state.context.platform().os_family == clipvault_core::OsFamily::Macos,
                    )
                })
                .unwrap_or_default();
            match TauriTrayController::install(
                app.handle(),
                state.watcher.is_capture_enabled(),
                &state.context.settings().load(&state.context).language,
                &capture_shortcut,
                menu_handler,
                tray_handler,
            ) {
                Ok(controller) => {
                    app.manage(controller);
                    info!("tray installed");
                }
                Err(error) => {
                    warn!(error = %error, "tray installation failed; running without tray");
                }
            }
            trace_main_window_lifecycle_for_app(app.handle(), "tray_configured", None);

            // Apply the configured retention policy as part of the
            // startup pass. Failures are logged but never block the
            // first paint of the UI.
            run_retention(&state.context);
            trace_main_window_lifecycle_for_app(app.handle(), "retention_completed", None);

            // Warm the active-app cache synchronously on the main
            // thread before the background loop starts so the very
            // first capture tick already sees a non-empty cache.
            // `refresh_active_app_cached` skips the synchronous
            // helper when called from the main thread (the typical
            // Tauri-command path) and goes straight to the inner
            // probe, so this warm-up is a single, fast call.
            let _ = refresh_active_app_cached(&state.context, Some(app.handle()));
            trace_main_window_lifecycle_for_app(app.handle(), "active_app_warmed", None);

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
            trace_main_window_lifecycle_for_app(app.handle(), "state_managed", None);

            // Register the default Quick Search and clipboard-capture
            // shortcuts after managed state is available to their callbacks.
            if let Some(shared) = app.try_state::<SharedState>() {
                let shortcuts = shared
                    .context()
                    .settings()
                    .load_keyboard_shortcuts(shared.context());
                for shortcut in shortcuts {
                    let Some(id) = clipvault_core::keyboard_shortcuts::KeyboardShortcutId::parse(
                        &shortcut.id,
                    ) else {
                        continue;
                    };
                    if !id.is_global() {
                        continue;
                    }
                    #[cfg(target_os = "linux")]
                    if shared.context().platform().display_server
                        == clipvault_core::DisplayServer::Wayland
                    {
                        let mut integration_available = false;
                        #[cfg(feature = "linux-gnome-shell-integration")]
                        if let Some(gnome) = shared.app_state().gnome_integration.as_ref() {
                            let payload = gnome.payload();
                            if payload.applicable && payload.installed && payload.consent == "accepted" {
                                let status = if gnome.update_global_shortcut(&shortcut).is_ok() {
                                    "registered"
                                } else {
                                    "failed"
                                };
                                shared.set_shortcut_status(&shortcut.id, status);
                                integration_available = true;
                            }
                        }
                        #[cfg(feature = "linux-kde-kwin-integration")]
                        if !integration_available {
                            if let Some(kde) = shared.app_state().kde_kwin_integration.as_ref() {
                                let payload = kde.payload();
                                if payload.applicable
                                    && payload.installed
                                    && payload.enabled
                                    && payload.consent == "accepted"
                                {
                                    // KWin reports the effective registration through its
                                    // authenticated shortcut-status method after script reload.
                                    integration_available = true;
                                }
                            }
                        }
                        if !integration_available {
                            shared.set_shortcut_status(&shortcut.id, "unsupported");
                        }
                        continue;
                    }
                    let Some(binding) = platform_hotkey_binding(&shortcut) else {
                        shared.set_shortcut_status(&shortcut.id, "unsupported");
                        continue;
                    };
                    let outcome = register_global_shortcut(
                        shared.app_state(),
                        app.handle(),
                        &binding,
                    );
                    shared.set_shortcut_status(&shortcut.id, outcome.kind());
                    info!(id = shortcut.id, kind = outcome.kind(), "global shortcut registration");
                }
            }
            trace_main_window_lifecycle_for_app(app.handle(), "hotkey_configured", None);

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
            trace_main_window_lifecycle_for_app(app.handle(), "capture_loop_configured", None);

            trace_main_window_lifecycle_for_app(app.handle(), "setup_completed", None);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                let stage = match event {
                    WindowEvent::Focused(_) => Some("focused"),
                    WindowEvent::Resized(_) => Some("resized"),
                    WindowEvent::Moved(_) => Some("moved"),
                    WindowEvent::CloseRequested { .. } => Some("close_requested"),
                    WindowEvent::Destroyed => Some("destroyed"),
                    _ => None,
                };
                if let Some(stage) = stage {
                    trace_main_window_event_lifecycle(window, stage);
                }
            }

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
            commands::clipvault_copy_entry,
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
            commands::clipvault_keyboard_shortcuts_get,
            commands::clipvault_keyboard_shortcut_set,
            commands::clipvault_capture_control_get,
            commands::clipvault_capture_control_set,
            commands::clipvault_ignored_apps_list,
            commands::clipvault_ignored_apps_add,
            commands::clipvault_ignored_apps_remove,
            commands::clipvault_ignored_app_pick_and_add,
            commands::clipvault_ignored_apps_list_with_metadata,
            commands::clipvault_ignored_app_icon,
            #[cfg(target_os = "linux")]
            commands::clipvault_ignored_app_linux_catalog,
            #[cfg(target_os = "linux")]
            commands::clipvault_ignored_app_linux_add,
            commands::clipvault_set_entry_title,
            commands::clipvault_update_text_entry,
            commands::clipvault_create_manual_text,
            commands::clipvault_entry_note,
            commands::clipvault_entry_note_ids,
            commands::clipvault_set_entry_note,
            commands::clipvault_collection_note,
            commands::clipvault_set_collection_note,
            commands::clipvault_source_app_icon,
            commands::clipvault_clipboard_asset,
            commands::clipvault_rich_text_preview,
            commands::clipvault_organization_snapshot,
            commands::clipvault_collections_create,
            commands::clipvault_collections_rename,
            commands::clipvault_collections_delete,
            commands::clipvault_collections_delete_preview,
            commands::clipvault_collections_set_color,
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
            commands::clipvault_code_language_set,
            commands::clipvault_gnome_integration_status,
            commands::clipvault_gnome_integration_set_consent,
            commands::clipvault_gnome_integration_install,
            commands::clipvault_gnome_integration_uninstall,
            commands::clipvault_gnome_integration_retry,
            commands::clipvault_kde_kwin_integration_status,
            commands::clipvault_kde_kwin_integration_activate,
            commands::clipvault_kde_kwin_integration_decline,
            commands::clipvault_kde_kwin_integration_disable,
            commands::clipvault_kde_kwin_integration_uninstall,
            commands::clipvault_kde_kwin_integration_retry,
            commands::clipvault_local_peer_profile_get,
            commands::clipvault_local_peer_profile_update,
            commands::clipvault_peer_sharing_toggle_get,
            commands::clipvault_peer_sharing_toggle_set,
            commands::clipvault_peer_snapshot,
            commands::clipvault_peer_sharing_refresh_identity,
            commands::clipvault_peer_pairing_start,
            commands::clipvault_peer_pairing_approve_local,
            commands::clipvault_peer_pairing_cancel,
            commands::clipvault_peer_pairing_snapshot,
            commands::clipvault_peer_pairing_revoke,
            commands::clipvault_peer_pairing_block,
            commands::clipvault_peer_pairing_unblock,
            commands::clipvault_peer_pairing_health,
            commands::clipvault_peer_history_browse,
            commands::clipvault_peer_history_record_state,
            commands::clipvault_peer_history_forget,
            commands::clipvault_peer_import_fetch,
            commands::clipvault_peer_import_record_state,
            commands::clipvault_peer_import_forget,
            commands::clipvault_peer_image_browse,
            commands::clipvault_peer_image_record_state,
            commands::clipvault_peer_image_forget,
            commands::clipvault_peer_image_fetch,
            commands::clipvault_peer_image_import_record_state,
            commands::clipvault_peer_image_import_forget,
            commands::clipvault_peer_image_thumbnail_fetch,
            commands::clipvault_peer_image_thumbnail_record_state,
            commands::clipvault_peer_image_thumbnail_forget,
            commands::clipvault_peer_source_app_presentation_fetch,
            commands::clipvault_peer_source_app_presentation_record_state,
            commands::clipvault_peer_source_app_presentation_forget,
            commands::clipvault_peer_import_source_app_presentations,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ClipVault")
        .run(handle_run_event);
}

/// Route metadata-free shortcut requests from the consented GNOME bridge.
/// The integration may have started its listener during bootstrap, before
/// Tauri exposed an `AppHandle`, so this attaches the deferred callback once
/// the handle exists.
#[cfg(all(target_os = "linux", feature = "linux-gnome-shell-integration"))]
fn configure_gnome_event_sink(state: &crate::bootstrap::AppState, handle: &AppHandle<tauri::Wry>) {
    let Some(gnome_integration) = state.gnome_integration.as_ref() else {
        return;
    };
    let app_handle = handle.clone();
    gnome_integration.set_event_sink(Arc::new(move |event| match event {
        clipvault_platform::GnomeShellEvent::QuickPasteRequested => {
            if let Err(error) = app_handle.emit(QUICK_SEARCH_EVENT, ()) {
                warn!(error = %error, "failed to emit GNOME quick-search event");
            }
        }
        clipvault_platform::GnomeShellEvent::ClipboardCaptureToggleRequested => {
            crate::commands::toggle_capture_from_hotkey(&app_handle);
        }
    }));
}

/// Route the KWin script's authenticated shortcut event through the same
/// capture-toggle transition used by the native hotkey, tray and settings.
#[cfg(all(target_os = "linux", feature = "linux-kde-kwin-integration"))]
fn configure_kde_capture_toggle_sink(
    state: &crate::bootstrap::AppState,
    handle: &AppHandle<tauri::Wry>,
) {
    let Some(kde_integration) = state.kde_kwin_integration.as_ref() else {
        return;
    };
    let app_handle = handle.clone();
    kde_integration.set_capture_toggle_sink(Arc::new(move || {
        crate::commands::toggle_capture_from_hotkey(&app_handle);
    }));
    let app_handle = handle.clone();
    kde_integration.set_quick_paste_sink(Arc::new(move || {
        if let Err(error) = app_handle.emit(QUICK_SEARCH_EVENT, ()) {
            warn!(error = %error, "failed to emit KWin QuickVault shortcut event");
        }
    }));
}

fn handle_run_event<R: tauri::Runtime>(app: &AppHandle<R>, event: RunEvent) {
    if matches!(event, RunEvent::Ready) {
        trace_main_window_lifecycle_for_app(app, "runtime_ready", None);
    }

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
    // Best-effort shutdown of the local-network subsystems. The
    // helper stops the pairing transport FIRST so the productive
    // mDNS advertisement retracts before the discovery adapter
    // emits the goodbye packet; the desktop then reflects
    // `No disponible` for the remote peer within the bounded
    // window the design pins instead of waiting for the mDNS TTL
    // to expire. See `peer-text-history-browser/design.md`
    // §"Descubrimiento, presencia y compatibilidad" and the
    // regression test in `bootstrap::tests`. Both closures are
    // idempotent and best-effort: a failure logs a metadata-only
    // warning and the helper returns without blocking the
    // process exit.
    stop_network_subsystems(shared.context());
    // Best-effort retention pass on shutdown so a long-running
    // session cleans up expired rows before the next start.
    run_retention(shared.context());
}

fn on_menu_event(handle: AppHandle<tauri::Wry>, event: MenuEvent) {
    let Some(action) = menu_event_to_action(event.id().as_ref()) else {
        warn!("unknown tray menu id: {}", event.id().as_ref());
        return;
    };
    let Some(controller) = handle.try_state::<Arc<TauriTrayController>>() else {
        warn!("tauri tray controller unavailable");
        return;
    };
    if let Err(error) = controller.invoke(action) {
        warn!(error = %error, "tray action failed");
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
        WatchTickOutcome::Paused => "paused",
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

fn window_lifecycle_debug_enabled() -> bool {
    matches!(
        std::env::var(WINDOW_LIFECYCLE_DEBUG_ENV).as_deref(),
        Ok("1")
    )
}

fn normalize_window_visibility(result: Result<bool, ()>) -> &'static str {
    match result {
        Ok(true) => "visible",
        Ok(false) => "hidden",
        Err(()) => "query_failed",
    }
}

fn trace_window_lifecycle(
    stage: &'static str,
    main_present: bool,
    monitor_available: Option<bool>,
    visible: &'static str,
) {
    info!(
        event = "clipvault_window_lifecycle",
        stage,
        main_present,
        ?monitor_available,
        visible,
        "main window lifecycle"
    );
}

fn trace_main_window_lifecycle<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
    stage: &'static str,
    monitor_available: Option<bool>,
) {
    if !window_lifecycle_debug_enabled() {
        return;
    }

    trace_window_lifecycle(
        stage,
        true,
        monitor_available,
        normalize_window_visibility(window.is_visible().map_err(|_| ())),
    );
}

fn trace_main_window_event_lifecycle<R: tauri::Runtime>(
    window: &tauri::Window<R>,
    stage: &'static str,
) {
    if !window_lifecycle_debug_enabled() {
        return;
    }

    trace_window_lifecycle(
        stage,
        true,
        None,
        normalize_window_visibility(window.is_visible().map_err(|_| ())),
    );
}

fn trace_main_window_lifecycle_for_app<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stage: &'static str,
    monitor_available: Option<bool>,
) {
    if !window_lifecycle_debug_enabled() {
        return;
    }

    if let Some(window) = app.get_webview_window("main") {
        trace_main_window_lifecycle(&window, stage, monitor_available);
    } else {
        trace_window_lifecycle(stage, false, monitor_available, "not_applicable");
    }
}

#[allow(dead_code)]
fn _ensure_arc(_: &Arc<()>) {}

/// Apply the main window's startup geometry during Tauri `setup`.
///
/// The window is sized to fill the monitor's available area
/// horizontally and aligned to the top edge. Linux additionally
/// compares the first compositor-resolved width with the target and
/// may correct it once:
///
/// - the correction listener is disarmed before it writes, so later
///   manual user moves and resizes are never clobbered by a loop;
/// - a missing monitor (headless, RDP, X11 without `RANDR`, …) keeps
///   the `tauri.conf.json` defaults, which the conf already picked
///   for that case;
/// - the helper never enters the path that resizes or moves the
///   transient `quick-paste` window: only the `main` label is
///   touched.
///
/// The pure geometry lives in [`crate::main_window_layout`] so its
/// scale, fallback and sizing rules are unit tested without a Tauri
/// runtime.
fn resize_main_window_to_monitor(app: &mut tauri::App) {
    let Some(window) = app.get_webview_window("main") else {
        trace_main_window_lifecycle_for_app(app.handle(), "monitor_available", None);
        warn!("main window not present at setup; skipping initial layout");
        return;
    };

    #[cfg(target_os = "linux")]
    {
        register_linux_initial_size_correction(window.clone());
        if let Some(layout) = linux_main_window_layout(&window) {
            apply_main_window_layout(&window, &layout);
        }
        return;
    }

    #[cfg(not(target_os = "linux"))]
    {
        use crate::main_window_layout::compute_main_window_layout;
        use tauri::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize};

        let monitor = match window.primary_monitor() {
            Ok(Some(monitor)) => {
                trace_main_window_lifecycle(&window, "monitor_available", Some(true));
                monitor
            }
            Ok(None) => {
                trace_main_window_lifecycle(&window, "monitor_available", Some(false));
                warn!("no primary monitor reported; keeping conf defaults");
                return;
            }
            Err(error) => {
                trace_main_window_lifecycle(&window, "monitor_available", None);
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
}

#[cfg(target_os = "linux")]
fn linux_main_window_layout<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
) -> Option<main_window_layout::MainWindowLayout> {
    use crate::main_window_layout::{
        compute_main_window_layout_with_minimum_width, resolve_monitor_area, MAIN_MIN_WIDTH,
    };

    let monitor = match window.current_monitor() {
        Ok(Some(monitor)) => Some(monitor),
        Ok(None) => None,
        Err(error) => {
            warn!(error = %error, "current monitor query failed; trying primary monitor");
            None
        }
    }
    .or_else(|| match window.primary_monitor() {
        Ok(Some(monitor)) => Some(monitor),
        Ok(None) => None,
        Err(error) => {
            warn!(error = %error, "primary monitor query failed");
            None
        }
    });

    let Some(monitor) = monitor else {
        trace_main_window_lifecycle(window, "monitor_available", Some(false));
        warn!("no monitor geometry reported; keeping configured main window size");
        return None;
    };
    trace_main_window_lifecycle(window, "monitor_available", Some(true));

    let work_area = monitor.work_area();
    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let area = resolve_monitor_area(
        (
            work_area.position.x as f64,
            work_area.position.y as f64,
            work_area.size.width as f64,
            work_area.size.height as f64,
        ),
        (
            monitor_position.x as f64,
            monitor_position.y as f64,
            monitor_size.width as f64,
            monitor_size.height as f64,
        ),
    );
    let Some(area) = area else {
        warn!("monitor and work-area geometry are invalid; keeping configured main window size");
        return None;
    };

    let raw_scale = monitor.scale_factor();
    let scale = if raw_scale.is_finite() && raw_scale > 0.0 {
        raw_scale
    } else {
        1.0
    };
    let available_logical_width = area.2 / scale;
    let minimum_width = MAIN_MIN_WIDTH.min(available_logical_width);
    let layout = compute_main_window_layout_with_minimum_width(area, scale, minimum_width);

    info!(
        work_area_x = area.0,
        work_area_y = area.1,
        available_physical_width = area.2,
        available_logical_width,
        scale,
        target_logical_width = layout.logical_size.0,
        target_physical_width = layout.physical_size.0,
        "computed Linux main window width from monitor geometry"
    );
    Some(layout)
}

#[cfg(target_os = "linux")]
fn apply_main_window_layout<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
    layout: &main_window_layout::MainWindowLayout,
) {
    use tauri::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize};

    let min_width = main_window_layout::MAIN_MIN_WIDTH.min(layout.logical_size.0);
    if let Err(error) = window.set_min_size(Some(LogicalSize::new(
        min_width,
        main_window_layout::MAIN_MIN_HEIGHT,
    ))) {
        warn!(error = %error, "could not adapt Linux main window minimum size");
    }

    let logical_size = LogicalSize::new(layout.logical_size.0, layout.logical_size.1);
    let physical_size = PhysicalSize::new(layout.physical_size.0, layout.physical_size.1);
    if let Err(error) = window.set_size(logical_size) {
        warn!(error = %error, "logical resize failed; falling back to physical");
        if let Err(physical_error) = window.set_size(physical_size) {
            warn!(error = %physical_error, "physical resize failed; keeping configured size");
        }
    }

    let logical_position =
        LogicalPosition::new(layout.logical_position.0, layout.logical_position.1);
    let physical_position =
        PhysicalPosition::new(layout.physical_position.0, layout.physical_position.1);
    if let Err(error) = window.set_position(logical_position) {
        warn!(error = %error, "logical position failed; falling back to physical");
        if let Err(physical_error) = window.set_position(physical_position) {
            warn!(error = %physical_error, "physical position failed");
        }
    }
}

#[cfg(target_os = "linux")]
fn initial_width_needs_correction(actual_width: u32, target_width: u32) -> bool {
    actual_width != target_width
}

#[cfg(target_os = "linux")]
fn register_linux_initial_size_correction<R: tauri::Runtime>(window: tauri::WebviewWindow<R>) {
    let correction_pending = Arc::new(AtomicBool::new(true));
    let event_window = window.clone();
    window.on_window_event(move |event| {
        let tauri::WindowEvent::Resized(actual_size) = event else {
            return;
        };
        if !correction_pending.swap(false, Ordering::AcqRel) {
            return;
        }

        let Some(layout) = linux_main_window_layout(&event_window) else {
            // Setup already retained the configured default when no
            // geometry was available. Do not keep observing after the
            // first compositor configure, so a later manual resize is
            // never corrected back to the startup size.
            return;
        };
        let width_mismatch =
            initial_width_needs_correction(actual_size.width, layout.physical_size.0);
        info!(
            effective_physical_width = actual_size.width,
            target_physical_width = layout.physical_size.0,
            effective_physical_height = actual_size.height,
            target_physical_height = layout.physical_size.1,
            width_matches_target = !width_mismatch,
            "measured initial Linux main window geometry"
        );
        if width_mismatch {
            info!("correcting Linux main window after its first compositor resize");
            apply_main_window_layout(&event_window, &layout);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::normalize_window_visibility;

    #[test]
    fn window_lifecycle_visibility_is_normalized_without_error_details() {
        assert_eq!(normalize_window_visibility(Ok(true)), "visible");
        assert_eq!(normalize_window_visibility(Ok(false)), "hidden");
        assert_eq!(normalize_window_visibility(Err(())), "query_failed");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn compositor_reported_width_is_checked_against_requested_width() {
        assert!(!super::initial_width_needs_correction(1366, 1366));
        assert!(super::initial_width_needs_correction(1080, 1366));
    }
}
