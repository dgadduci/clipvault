//! `global-hotkey`-backed manager.
//!
//! `global-hotkey` keeps a single process-wide event handler; we install
//! a static dispatcher the first time a [`GlobalHotkeyManagerAdapter`]
//! is created and rely on it to find the right per-binding callback.

use std::collections::HashMap;
use std::sync::Arc;

use global_hotkey::hotkey::{Code, HotKey as GhHotKey, Modifiers as GhModifiers};
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyEvent as Event, GlobalHotKeyManager, HotKeyState,
};
use parking_lot::Mutex;
use tracing::warn;

use crate::hotkey::{
    HotkeyBinding, HotkeyError, HotkeyKey, HotkeyManager, HotkeyModifiers, HotkeyOutcome,
};

type Callback = Arc<dyn Fn() + Send + Sync + 'static>;

struct CallbackEntry {
    binding_id: String,
    callback: Callback,
}

struct Dispatcher {
    map: Mutex<HashMap<u32, CallbackEntry>>,
}

impl Dispatcher {
    fn install() -> Arc<Self> {
        // `global-hotkey` allows one global handler per process; we
        // install it on first use and never replace it. The handler
        // captures the dispatcher by `Arc` so a fresh manager updates
        // its map.
        static INIT: std::sync::OnceLock<Arc<Dispatcher>> = std::sync::OnceLock::new();
        INIT.get_or_init(|| {
            let arc = Arc::new(Dispatcher {
                map: Mutex::new(HashMap::new()),
            });
            let weak = Arc::downgrade(&arc);
            Event::set_event_handler(Some(move |event| {
                if let Some(dispatcher) = weak.upgrade() {
                    dispatcher.dispatch(event);
                }
            }));
            arc
        })
        .clone()
    }

    fn dispatch(&self, event: GlobalHotKeyEvent) {
        if !matches!(event.state(), HotKeyState::Pressed) {
            return;
        }
        let entry = {
            let map = self.map.lock();
            map.get(&event.id()).map(|entry| CallbackEntry {
                binding_id: entry.binding_id.clone(),
                callback: entry.callback.clone(),
            })
        };
        if let Some(entry) = entry {
            (entry.callback)();
        }
    }

    fn insert(&self, id: u32, entry: CallbackEntry) {
        self.map.lock().insert(id, entry);
    }

    fn clear(&self) {
        self.map.lock().clear();
    }
}

/// Manager wrapper that exposes the platform-agnostic [`HotkeyManager`]
/// trait.
pub struct GlobalHotkeyManagerAdapter {
    manager: GlobalHotKeyManager,
    dispatcher: Arc<Dispatcher>,
    registered: Mutex<Vec<GhHotKey>>,
}

impl GlobalHotkeyManagerAdapter {
    pub fn new() -> Result<Self, HotkeyError> {
        let manager = GlobalHotKeyManager::new().map_err(HotkeyError::backend)?;
        let dispatcher = Dispatcher::install();
        Ok(Self {
            manager,
            dispatcher,
            registered: Mutex::new(Vec::new()),
        })
    }

    fn map_key(key: HotkeyKey) -> Code {
        match key {
            HotkeyKey::V => Code::KeyV,
            HotkeyKey::Enter => Code::Enter,
            HotkeyKey::Escape => Code::Escape,
        }
    }

    fn map_modifiers(modifiers: HotkeyModifiers) -> GhModifiers {
        let mut out = GhModifiers::empty();
        if modifiers.cmd_or_ctrl {
            // `cmd_or_ctrl` is the cross-platform alias for the platform's
            // primary modifier: `SUPER` (Command) on macOS and `CONTROL`
            // everywhere else. Mapping it to `CONTROL` unconditionally
            // would make the macOS default (`Cmd+Shift+V`) unusable.
            #[cfg(target_os = "macos")]
            {
                out |= GhModifiers::SUPER;
            }
            #[cfg(not(target_os = "macos"))]
            {
                out |= GhModifiers::CONTROL;
            }
        }
        if modifiers.shift {
            out |= GhModifiers::SHIFT;
        }
        if modifiers.alt {
            out |= GhModifiers::ALT;
        }
        if modifiers.meta {
            out |= GhModifiers::SUPER;
        }
        out
    }

    /// Resolve the `cmd_or_ctrl` modifier for the current target OS. The
    /// resolution is identical to [`Self::map_modifiers`] but it returns
    /// just the relevant bit so unit tests can assert the platform
    /// behaviour without touching the rest of the modifier mapping.
    pub fn cmd_or_ctrl_for_host() -> GhModifiers {
        #[cfg(target_os = "macos")]
        {
            GhModifiers::SUPER
        }
        #[cfg(not(target_os = "macos"))]
        {
            GhModifiers::CONTROL
        }
    }
}

impl HotkeyManager for GlobalHotkeyManagerAdapter {
    fn register(
        &self,
        binding: &HotkeyBinding,
        on_activate: Box<dyn Fn() + Send + Sync + 'static>,
    ) -> Result<HotkeyOutcome, HotkeyError> {
        let gh_hotkey = GhHotKey::new(
            Some(Self::map_modifiers(binding.modifiers)),
            Self::map_key(binding.key),
        );
        let id = gh_hotkey.id();

        match self.manager.register(gh_hotkey) {
            Ok(()) => {
                self.dispatcher.insert(
                    id,
                    CallbackEntry {
                        binding_id: binding.id.clone(),
                        callback: Arc::from(on_activate),
                    },
                );
                self.registered.lock().push(gh_hotkey);
                Ok(HotkeyOutcome::Registered)
            }
            Err(error) => {
                let reason = error.to_string();
                if reason.to_lowercase().contains("already") {
                    Ok(HotkeyOutcome::Conflict { reason })
                } else {
                    warn!(reason = %reason, "hotkey registration failed");
                    Ok(HotkeyOutcome::Failed { reason })
                }
            }
        }
    }

    fn unregister_all(&self) -> Result<(), HotkeyError> {
        // Drop callbacks first so any concurrent handler invocation
        // becomes a no-op.
        self.dispatcher.clear();
        let mut registered = self.registered.lock();
        let snapshot = std::mem::take(&mut *registered);
        self.manager
            .unregister_all(&snapshot)
            .map_err(HotkeyError::backend)?;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "global_hotkey"
    }
}

impl Drop for GlobalHotkeyManagerAdapter {
    fn drop(&mut self) {
        let _ = self.unregister_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{default_linux_binding, default_macos_binding};

    #[test]
    fn cmd_or_ctrl_for_host_matches_compile_target() {
        // The helper exposes the platform-specific bit so we can pin
        // the mapping against the build target without instantiating
        // the manager (which would need a desktop session).
        let bit = GlobalHotkeyManagerAdapter::cmd_or_ctrl_for_host();
        #[cfg(target_os = "macos")]
        assert!(
            bit.contains(GhModifiers::SUPER),
            "macOS must map cmd_or_ctrl to SUPER"
        );
        #[cfg(not(target_os = "macos"))]
        assert!(
            bit.contains(GhModifiers::CONTROL),
            "non-macOS must map cmd_or_ctrl to CONTROL"
        );
    }

    #[test]
    fn default_bindings_keep_their_expected_modifiers() {
        // Pin the contract that the default bindings stay
        // `Cmd + Shift + V` on macOS and `Ctrl + Shift + V` on Linux
        // even after the runtime mapping fix.
        let mac = default_macos_binding("quick_search");
        let linux = default_linux_binding("quick_search");
        assert!(mac.modifiers.cmd_or_ctrl);
        assert!(mac.modifiers.shift);
        assert!(linux.modifiers.cmd_or_ctrl);
        assert!(linux.modifiers.shift);
        assert_eq!(mac.key, HotkeyKey::V);
        assert_eq!(linux.key, HotkeyKey::V);
    }

    #[test]
    fn map_modifiers_includes_only_expected_bits() {
        // The combination `CMD_SHIFT + ALT + META` should map to the
        // platform-specific primary modifier plus SHIFT, ALT and SUPER.
        let mapped = GlobalHotkeyManagerAdapter::map_modifiers(HotkeyModifiers {
            cmd_or_ctrl: true,
            shift: true,
            alt: true,
            meta: true,
        });
        assert!(mapped.contains(GhModifiers::SHIFT));
        assert!(mapped.contains(GhModifiers::ALT));
        assert!(mapped.contains(GhModifiers::SUPER));
        let primary = GlobalHotkeyManagerAdapter::cmd_or_ctrl_for_host();
        assert!(
            mapped.contains(primary),
            "primary modifier must be present after mapping"
        );
    }
}
