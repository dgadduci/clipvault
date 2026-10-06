//! Application layer for the keyboard-shortcut settings flow.
//!
//! Tauri commands in `commands` stay as small adapters; this module coordinates
//! the core settings service with the active desktop shortcut integration.

use std::collections::HashMap;
use std::sync::Arc;

use clipvault_core::HotkeySpec;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tracing::warn;

use crate::commands::CommandError;
use crate::state::SharedState;

#[derive(Debug, Clone, Serialize)]
pub struct KeyboardShortcutsSnapshot {
    pub bindings: Vec<HotkeySpec>,
    pub status: HashMap<String, String>,
}

pub fn snapshot(state: &SharedState) -> KeyboardShortcutsSnapshot {
    let bindings = state
        .context()
        .settings()
        .load_keyboard_shortcuts(state.context());
    let runtime_status = state.shortcut_status();
    let wayland =
        state.context().platform().display_server == clipvault_core::DisplayServer::Wayland;
    let status = bindings
        .iter()
        .map(|binding| {
            let id = clipvault_core::keyboard_shortcuts::KeyboardShortcutId::parse(&binding.id)
                .expect("loaded keyboard shortcut IDs are catalogued");
            let fallback = if id.is_global() {
                "unsupported"
            } else {
                "ready"
            };
            let kde_status = if wayland && id.is_global() {
                #[cfg(all(target_os = "linux", feature = "linux-kde-kwin-integration"))]
                {
                    state
                        .app_state()
                        .kde_kwin_integration
                        .as_ref()
                        .and_then(|integration| integration.shortcut_status(id.as_str()))
                }
                #[cfg(not(all(target_os = "linux", feature = "linux-kde-kwin-integration")))]
                {
                    None
                }
            } else {
                None
            };
            (
                binding.id.clone(),
                runtime_status
                    .get(&binding.id)
                    .cloned()
                    .or(kde_status)
                    .unwrap_or_else(|| fallback.to_string()),
            )
        })
        .collect();
    KeyboardShortcutsSnapshot { bindings, status }
}

/// Replace one keyboard binding transactionally. Global actions register the
/// candidate before it is persisted; a failed write unregisters the candidate
/// while the previous binding remains in place.
pub async fn set(
    app: AppHandle,
    state: SharedState,
    mut binding: HotkeySpec,
) -> Result<KeyboardShortcutsSnapshot, CommandError> {
    use clipvault_core::keyboard_shortcuts::{
        is_reserved_development_shortcut, same_shortcut, shortcut_conflict, validate_shortcut,
        KeyboardShortcutId,
    };

    let id = validate_shortcut(&binding)
        .map_err(|_| CommandError::new("keyboard_shortcut_invalid", "invalid shortcut"))?;
    binding.key = binding.key.trim().to_ascii_lowercase();
    if is_reserved_development_shortcut(&binding) {
        return Err(CommandError::new(
            "keyboard_shortcut_conflict",
            "shortcut conflicts with reserved Development shortcut",
        ));
    }
    let current = state
        .context()
        .settings()
        .load_keyboard_shortcuts(state.context());
    let interface_language = state.context().settings().load(state.context()).language;
    if let Some(conflict) = shortcut_conflict(&binding, &current) {
        return Err(CommandError::new(
            "keyboard_shortcut_conflict",
            format!("shortcut conflicts with {}", conflict.as_str()),
        ));
    }
    let previous = current
        .iter()
        .find(|existing| existing.id == id.as_str())
        .cloned()
        .ok_or_else(|| CommandError::new("keyboard_shortcut_invalid", "unknown shortcut"))?;
    binding.id = id.as_str().to_string();

    let mut candidate_platform_binding = None;
    let mut update_gnome = false;
    #[cfg(all(target_os = "linux", feature = "linux-kde-kwin-integration"))]
    let mut kde_shortcut_bundles = None;
    let mut kde_available = false;
    if id.is_global() && !same_shortcut(&binding, &previous) {
        let platform = state.context().platform();
        if platform.display_server == clipvault_core::DisplayServer::Wayland {
            #[cfg(all(target_os = "linux", feature = "linux-gnome-shell-integration"))]
            if let Some(gnome) = state.app_state().gnome_integration.as_ref() {
                let payload = gnome.payload();
                if payload.applicable && payload.installed && payload.consent == "accepted" {
                    gnome.update_global_shortcut(&binding).map_err(|kind| {
                        let error_kind = match kind.as_str() {
                            "shortcut_conflict" => "keyboard_shortcut_conflict",
                            "shortcut_unsupported" => "keyboard_shortcut_unsupported",
                            _ => "keyboard_shortcut_registration",
                        };
                        CommandError::new(
                            error_kind,
                            "the GNOME Shell integration could not activate the shortcut",
                        )
                    })?;
                    update_gnome = true;
                }
            }

            #[cfg(all(target_os = "linux", feature = "linux-kde-kwin-integration"))]
            if !update_gnome {
                if let Some(kde) = state.app_state().kde_kwin_integration.as_ref() {
                    let payload = kde.payload();
                    if payload.applicable
                        && payload.installed
                        && payload.enabled
                        && payload.consent == "accepted"
                    {
                        let template = crate::kde_kwin_integration::read_bundled_script(&app)
                            .map_err(|_| {
                                CommandError::new(
                                    "keyboard_shortcut_registration",
                                    "the KWin script is unavailable",
                                )
                            })?;
                        let previous_script =
                            crate::kde_kwin_integration::configure_bundled_shortcuts(
                                &template,
                                &current,
                                &interface_language,
                            )
                            .map_err(|_| {
                                CommandError::new(
                                    "keyboard_shortcut_registration",
                                    "the KWin shortcut settings are invalid",
                                )
                            })?;
                        let mut candidate_shortcuts = current.clone();
                        if let Some(target) = candidate_shortcuts
                            .iter_mut()
                            .find(|item| item.id == id.as_str())
                        {
                            *target = binding.clone();
                        }
                        let candidate_script =
                            crate::kde_kwin_integration::configure_bundled_shortcuts(
                                &template,
                                &candidate_shortcuts,
                                &interface_language,
                            )
                            .map_err(|_| {
                                CommandError::new(
                                    "keyboard_shortcut_registration",
                                    "the KWin shortcut settings are invalid",
                                )
                            })?;
                        kde.replace_shortcuts(&previous_script, &candidate_script)
                            .await
                            .map_err(|error| {
                                let kind = error.stable_kind();
                                CommandError::new("keyboard_shortcut_registration", kind)
                            })?;
                        kde_shortcut_bundles = Some((previous_script, candidate_script));
                        kde_available = true;
                    }
                }
            }

            if !update_gnome && !kde_available {
                return Err(CommandError::new(
                    "keyboard_shortcut_unsupported",
                    "the active Wayland shortcut integration is unavailable",
                ));
            }
        } else {
            let native_binding = crate::bootstrap::platform_hotkey_binding(&binding)
                .ok_or_else(|| CommandError::new("keyboard_shortcut_invalid", "unsupported key"))?;
            let outcome = crate::bootstrap::register_global_shortcut(
                state.app_state(),
                &app,
                &native_binding,
            );
            if !outcome.is_registered() {
                return Err(CommandError::new(
                    "keyboard_shortcut_registration",
                    outcome.kind(),
                ));
            }
            candidate_platform_binding = Some(native_binding);
        }
    }

    if let Err(error) = state
        .context()
        .settings()
        .persist_keyboard_shortcut(state.context(), &binding)
    {
        if let Some(candidate) = candidate_platform_binding.as_ref() {
            if let Err(rollback_error) = state.adapters().hotkey().unregister(candidate) {
                warn!(error = %rollback_error, "failed to unregister shortcut after persistence error");
            }
        }
        #[cfg(all(target_os = "linux", feature = "linux-gnome-shell-integration"))]
        if update_gnome {
            if let Some(gnome) = state.app_state().gnome_integration.as_ref() {
                if let Err(error) = gnome.update_global_shortcut(&previous) {
                    warn!(error = %error, "failed to restore GNOME global shortcut after save error");
                }
            }
        }
        #[cfg(all(target_os = "linux", feature = "linux-kde-kwin-integration"))]
        if let Some((previous_script, candidate_script)) = kde_shortcut_bundles.as_ref() {
            if let Some(kde) = state.app_state().kde_kwin_integration.as_ref() {
                if let Err(error) = kde
                    .replace_shortcuts(candidate_script, previous_script)
                    .await
                {
                    warn!(
                        kind = error.stable_kind(),
                        "failed to restore KWin shortcuts after save error"
                    );
                }
            }
        }
        return Err(error.into());
    }

    if let Some(candidate) = candidate_platform_binding.as_ref() {
        if !same_shortcut(&binding, &previous) {
            if let Some(previous_binding) = crate::bootstrap::platform_hotkey_binding(&previous) {
                if let Err(error) = state.adapters().hotkey().unregister(&previous_binding) {
                    let _ = state
                        .context()
                        .settings()
                        .persist_keyboard_shortcut(state.context(), &previous);
                    let _ = state.adapters().hotkey().unregister(candidate);
                    return Err(CommandError::new(
                        "keyboard_shortcut_registration",
                        error.to_string(),
                    ));
                }
            }
        }
        state.set_shortcut_status(id.as_str(), "registered");
    } else if id.is_global() {
        state.set_shortcut_status(id.as_str(), "registered");
    } else {
        state.set_shortcut_status(id.as_str(), "ready");
    }

    if id == KeyboardShortcutId::ToggleClipboardCapture {
        let label = crate::commands::format_keyboard_shortcut(
            &binding,
            state.context().platform().os_family == clipvault_core::OsFamily::Macos,
        );
        if let Some(controller) = app.try_state::<Arc<crate::tray::TauriTrayController>>() {
            if let Err(error) = controller.set_capture_shortcut(&label) {
                warn!(error = %error, "failed to refresh capture shortcut in tray");
            }
        }
    }

    let snapshot = snapshot(&state);
    if let Err(error) = app.emit(
        clipvault_core::keyboard_shortcuts::KEYBOARD_SHORTCUTS_CHANGED_EVENT,
        &snapshot,
    ) {
        warn!(error = %error, "failed to emit keyboard-shortcuts-changed event");
    }
    Ok(snapshot)
}
