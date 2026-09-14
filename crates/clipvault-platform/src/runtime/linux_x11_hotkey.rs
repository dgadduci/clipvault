//! Linux X11 global hotkey manager.
//!
//! Passive `XGrabKey` is the normal route: it consumes a registered shortcut
//! before the focused client receives it. X11 suppresses that passive grab
//! while another client owns an active `XGrabKeyboard`. This adapter keeps the
//! passive and XInput2 raw-input routes on one connection, so the raw route
//! covers that narrow case without duplicating a normal passive activation.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tracing::{info, warn};
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xinput::{self, ConnectionExt as XInputConnectionExt};
use x11rb::protocol::xproto::{
    ConnectionExt as XprotoConnectionExt, GrabMode, GrabStatus, Keycode, ModMask, Window,
};
use x11rb::protocol::{ErrorKind, Event};
use x11rb::rust_connection::RustConnection;

use crate::hotkey::{
    HotkeyBinding, HotkeyError, HotkeyKey, HotkeyManager, HotkeyModifiers, HotkeyOutcome,
};

const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(10);
/// A core `KeyPress` from ClipVault's passive grab is normally already queued
/// directly after its raw counterpart. Delaying raw delivery for this small
/// window gives the passive route precedence without making Quick Paste feel
/// delayed when another client owns the keyboard.
const RAW_FALLBACK_WAIT: Duration = Duration::from_millis(25);
const XK_V_LOWER: u32 = 0x0076;
const XK_V_UPPER: u32 = 0x0056;
const XK_RETURN: u32 = 0xff0d;
const XK_ESCAPE: u32 = 0xff1b;
type Callback = Arc<dyn Fn() + Send + Sync + 'static>;

fn ignored_modifier_variants() -> [ModMask; 4] {
    [
        ModMask::default(),
        ModMask::LOCK,
        ModMask::M2,
        ModMask::LOCK | ModMask::M2,
    ]
}

/// X11 implementation of the platform-neutral hotkey trait.
///
/// The handle only transports commands. Its X11 connection is owned by one
/// worker thread, keeping X11 ordering and callback lifetime deterministic
/// without leaking X11 types to the core or Tauri layers.
pub struct LinuxX11HotkeyManagerAdapter {
    commands: Sender<WorkerCommand>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl LinuxX11HotkeyManagerAdapter {
    pub fn new() -> Result<Self, HotkeyError> {
        let (conn, screen_number) =
            x11rb::connect(None).map_err(|_| HotkeyError::backend("x11_connection_unavailable"))?;
        let root = conn.setup().roots[screen_number].root;
        let keymap = Keymap::load(&conn)?;
        let raw_events = match select_raw_key_events(&conn, root) {
            Ok(()) => true,
            Err(reason) => {
                // Passive X11 registration continues to work when XInput2 is
                // unavailable. Keep diagnostics technical-only.
                warn!(reason, "X11 raw hotkey fallback unavailable");
                false
            }
        };
        let (commands, receiver) = mpsc::channel();
        let join = thread::Builder::new()
            .name("clipvault-x11-hotkey".into())
            .spawn(move || run_worker(conn, root, keymap, raw_events, receiver))
            .map_err(|_| HotkeyError::backend("x11_hotkey_worker_unavailable"))?;

        Ok(Self {
            commands,
            join: Mutex::new(Some(join)),
        })
    }

    fn request(&self, command: WorkerCommand) -> Result<HotkeyOutcome, HotkeyError> {
        let (response, receiver) = mpsc::sync_channel(1);
        self.commands
            .send(command.with_response(response))
            .map_err(|_| HotkeyError::backend("x11_hotkey_worker_unavailable"))?;
        receiver
            .recv()
            .map_err(|_| HotkeyError::backend("x11_hotkey_worker_unavailable"))
    }

    fn shutdown(&self) {
        let _ = self.commands.send(WorkerCommand::Shutdown);
        if let Some(join) = self.join.lock().take() {
            let _ = join.join();
        }
    }
}

impl HotkeyManager for LinuxX11HotkeyManagerAdapter {
    fn register(
        &self,
        binding: &HotkeyBinding,
        on_activate: Box<dyn Fn() + Send + Sync + 'static>,
    ) -> Result<HotkeyOutcome, HotkeyError> {
        self.request(WorkerCommand::Register {
            binding: binding.clone(),
            callback: Arc::from(on_activate),
            response: None,
        })
    }

    fn unregister_all(&self) -> Result<(), HotkeyError> {
        match self.request(WorkerCommand::Clear { response: None })? {
            HotkeyOutcome::Registered => Ok(()),
            HotkeyOutcome::Conflict { reason }
            | HotkeyOutcome::Unsupported { reason }
            | HotkeyOutcome::Failed { reason } => Err(HotkeyError::backend(reason)),
        }
    }

    fn name(&self) -> &'static str {
        "global_hotkey"
    }
}

impl Drop for LinuxX11HotkeyManagerAdapter {
    fn drop(&mut self) {
        let _ = self.unregister_all();
        self.shutdown();
    }
}

enum WorkerCommand {
    Register {
        binding: HotkeyBinding,
        callback: Callback,
        response: Option<SyncSender<HotkeyOutcome>>,
    },
    Clear {
        response: Option<SyncSender<HotkeyOutcome>>,
    },
    Shutdown,
}

impl WorkerCommand {
    fn with_response(self, response: SyncSender<HotkeyOutcome>) -> Self {
        match self {
            Self::Register {
                binding, callback, ..
            } => Self::Register {
                binding,
                callback,
                response: Some(response),
            },
            Self::Clear { .. } => Self::Clear {
                response: Some(response),
            },
            Self::Shutdown => Self::Shutdown,
        }
    }
}

struct RegisteredBinding {
    modifiers: ModMask,
    callback: Callback,
    pressed: bool,
}

struct PendingRawActivation {
    keycode: Keycode,
    modifiers: ModMask,
    observed_at: Instant,
}

#[derive(Default)]
struct RawModifierState {
    active: ModMask,
}

impl RawModifierState {
    fn update(
        &mut self,
        modifier_keys: &HashMap<Keycode, ModMask>,
        keycode: Keycode,
        pressed: bool,
    ) {
        let Some(mask) = modifier_keys.get(&keycode).copied() else {
            return;
        };
        if pressed {
            self.active |= mask;
        } else {
            self.active = ModMask::from(u16::from(self.active) & !u16::from(mask));
        }
    }

    fn active(&self) -> ModMask {
        self.active
    }
}

struct Keymap {
    keysyms: HashMap<Keycode, Vec<u32>>,
    modifier_keys: HashMap<Keycode, ModMask>,
}

impl Keymap {
    fn load(conn: &RustConnection) -> Result<Self, HotkeyError> {
        let setup = conn.setup();
        let first = setup.min_keycode;
        let count = setup.max_keycode - first + 1;
        let reply = conn
            .get_keyboard_mapping(first, count)
            .map_err(|_| HotkeyError::backend("x11_keymap_unavailable"))?
            .reply()
            .map_err(|_| HotkeyError::backend("x11_keymap_unavailable"))?;
        let per_keycode = usize::from(reply.keysyms_per_keycode);
        if per_keycode == 0 {
            return Err(HotkeyError::backend("x11_keymap_unavailable"));
        }
        let keysyms = reply
            .keysyms
            .chunks(per_keycode)
            .enumerate()
            .map(|(index, symbols)| (first + index as Keycode, symbols.to_vec()))
            .collect();

        let reply = conn
            .get_modifier_mapping()
            .map_err(|_| HotkeyError::backend("x11_keymap_unavailable"))?
            .reply()
            .map_err(|_| HotkeyError::backend("x11_keymap_unavailable"))?;
        let per_modifier = reply.keycodes.len() / 8;
        if per_modifier == 0 {
            return Err(HotkeyError::backend("x11_keymap_unavailable"));
        }
        let masks = [
            ModMask::SHIFT,
            ModMask::LOCK,
            ModMask::CONTROL,
            ModMask::M1,
            ModMask::M2,
            ModMask::M3,
            ModMask::M4,
            ModMask::M5,
        ];
        let mut modifier_keys = HashMap::new();
        for (index, keys) in reply.keycodes.chunks(per_modifier).enumerate() {
            for keycode in keys.iter().copied().filter(|keycode| *keycode != 0) {
                modifier_keys.insert(keycode, masks[index]);
            }
        }

        Ok(Self {
            keysyms,
            modifier_keys,
        })
    }

    fn resolve_keycode(&self, key: HotkeyKey) -> Option<Keycode> {
        self.keysyms.iter().find_map(|(keycode, symbols)| {
            keysyms_for(key)
                .iter()
                .any(|wanted| symbols.contains(wanted))
                .then_some(*keycode)
        })
    }
}

fn keysyms_for(key: HotkeyKey) -> &'static [u32] {
    match key {
        HotkeyKey::V => &[XK_V_LOWER, XK_V_UPPER],
        HotkeyKey::Enter => &[XK_RETURN],
        HotkeyKey::Escape => &[XK_ESCAPE],
    }
}

fn x11_modifiers(modifiers: HotkeyModifiers) -> ModMask {
    let mut out = ModMask::default();
    if modifiers.cmd_or_ctrl {
        out |= ModMask::CONTROL;
    }
    if modifiers.shift {
        out |= ModMask::SHIFT;
    }
    if modifiers.alt {
        out |= ModMask::M1;
    }
    if modifiers.meta {
        out |= ModMask::M4;
    }
    out
}

fn normalized_modifiers(modifiers: ModMask) -> ModMask {
    modifiers & (ModMask::CONTROL | ModMask::SHIFT | ModMask::M1 | ModMask::M4)
}

fn select_raw_key_events(conn: &RustConnection, root: Window) -> Result<(), &'static str> {
    conn.xinput_xi_query_version(2, 2)
        .map_err(|_| "xinput_unavailable")?
        .reply()
        .map_err(|_| "xinput_unavailable")?;
    let mask = xinput::XIEventMask::RAW_KEY_PRESS | xinput::XIEventMask::RAW_KEY_RELEASE;
    conn.xinput_xi_select_events(
        root,
        &[xinput::EventMask {
            deviceid: xinput::Device::ALL_MASTER.into(),
            mask: vec![mask],
        }],
    )
    .map_err(|_| "xinput_subscription_failed")?
    .check()
    .map_err(|_| "xinput_subscription_failed")?;
    conn.flush().map_err(|_| "xinput_subscription_failed")
}

/// Check whether the server reports any active keyboard grab. A passive grab
/// may have just activated for this same client, so this result alone cannot
/// identify its owner. The worker therefore gives the queued core `KeyPress`
/// precedence before delivering the raw fallback.
fn keyboard_grab_is_active(conn: &RustConnection, root: Window) -> bool {
    let Ok(cookie) = conn.grab_keyboard(
        false,
        root,
        x11rb::CURRENT_TIME,
        GrabMode::ASYNC,
        GrabMode::ASYNC,
    ) else {
        return false;
    };
    let Ok(reply) = cookie.reply() else {
        return false;
    };
    if reply.status == GrabStatus::ALREADY_GRABBED {
        return true;
    }
    if reply.status == GrabStatus::SUCCESS {
        if let Ok(cookie) = conn.ungrab_keyboard(x11rb::CURRENT_TIME) {
            let _ = cookie.check();
        }
        let _ = conn.flush();
    }
    false
}

fn run_worker(
    conn: RustConnection,
    root: Window,
    keymap: Keymap,
    raw_events: bool,
    commands: Receiver<WorkerCommand>,
) {
    let mut raw_modifiers = RawModifierState::default();
    let mut bindings: HashMap<Keycode, Vec<RegisteredBinding>> = HashMap::new();
    let mut pending_raw = Vec::new();

    loop {
        activate_expired_raw(&mut bindings, &mut pending_raw);
        match drain_commands(
            &conn,
            root,
            &keymap,
            &commands,
            &mut bindings,
            &mut pending_raw,
        ) {
            WorkerControl::Continue => {}
            WorkerControl::Shutdown => return,
        }

        match conn.poll_for_event() {
            Ok(Some(Event::KeyPress(event))) => {
                discard_pending_raw(&mut pending_raw, event.detail);
                let callbacks = activate_matching(
                    &mut bindings,
                    event.detail,
                    normalized_modifiers(ModMask::from(u16::from(event.state))),
                );
                invoke_callbacks(callbacks);
            }
            Ok(Some(Event::KeyRelease(event))) => {
                discard_pending_raw(&mut pending_raw, event.detail);
                release_bindings(&mut bindings, event.detail);
            }
            Ok(Some(Event::XinputRawKeyPress(event))) if raw_events => {
                let keycode = event.detail as Keycode;
                raw_modifiers.update(&keymap.modifier_keys, keycode, true);
                let modifiers = normalized_modifiers(raw_modifiers.active());
                if binding_matches(&bindings, keycode, modifiers)
                    && keyboard_grab_is_active(&conn, root)
                {
                    queue_pending_raw(&mut pending_raw, keycode, modifiers);
                }
            }
            Ok(Some(Event::XinputRawKeyRelease(event))) if raw_events => {
                let keycode = event.detail as Keycode;
                if let Some(pending) = take_pending_raw(&mut pending_raw, keycode) {
                    let callbacks =
                        activate_matching(&mut bindings, pending.keycode, pending.modifiers);
                    if !callbacks.is_empty() {
                        info!(backend = "xinput2_raw", "X11 raw hotkey fallback activated");
                        invoke_callbacks(callbacks);
                    }
                }
                raw_modifiers.update(&keymap.modifier_keys, keycode, false);
                release_bindings(&mut bindings, keycode);
            }
            Ok(Some(_)) | Ok(None) => thread::sleep(WORKER_POLL_INTERVAL),
            Err(_) => {
                warn!("X11 hotkey worker stopped after a connection error");
                return;
            }
        }
    }
}

enum WorkerControl {
    Continue,
    Shutdown,
}

fn drain_commands(
    conn: &RustConnection,
    root: Window,
    keymap: &Keymap,
    commands: &Receiver<WorkerCommand>,
    bindings: &mut HashMap<Keycode, Vec<RegisteredBinding>>,
    pending_raw: &mut Vec<PendingRawActivation>,
) -> WorkerControl {
    loop {
        match commands.try_recv() {
            Ok(WorkerCommand::Register {
                binding,
                callback,
                response,
            }) => {
                let outcome = register_binding(conn, root, keymap, bindings, binding, callback);
                if let Some(response) = response {
                    let _ = response.send(outcome);
                }
            }
            Ok(WorkerCommand::Clear { response }) => {
                clear_bindings(conn, root, bindings);
                pending_raw.clear();
                if let Some(response) = response {
                    let _ = response.send(HotkeyOutcome::Registered);
                }
            }
            Ok(WorkerCommand::Shutdown) | Err(TryRecvError::Disconnected) => {
                clear_bindings(conn, root, bindings);
                pending_raw.clear();
                return WorkerControl::Shutdown;
            }
            Err(TryRecvError::Empty) => return WorkerControl::Continue,
        }
    }
}

fn register_binding(
    conn: &RustConnection,
    root: Window,
    keymap: &Keymap,
    bindings: &mut HashMap<Keycode, Vec<RegisteredBinding>>,
    binding: HotkeyBinding,
    callback: Callback,
) -> HotkeyOutcome {
    let Some(keycode) = keymap.resolve_keycode(binding.key) else {
        return HotkeyOutcome::Failed {
            reason: "x11_keycode_unavailable".into(),
        };
    };
    let modifiers = x11_modifiers(binding.modifiers);
    if let Some(existing) = bindings.get_mut(&keycode).and_then(|entries| {
        entries
            .iter_mut()
            .find(|entry| entry.modifiers == modifiers)
    }) {
        existing.callback = callback;
        existing.pressed = false;
        return HotkeyOutcome::Registered;
    }

    if let Err(outcome) = grab_passive_variants(conn, root, keycode, modifiers) {
        return outcome;
    }
    bindings
        .entry(keycode)
        .or_default()
        .push(RegisteredBinding {
            modifiers,
            callback,
            pressed: false,
        });
    HotkeyOutcome::Registered
}

fn grab_passive_variants(
    conn: &RustConnection,
    root: Window,
    keycode: Keycode,
    modifiers: ModMask,
) -> Result<(), HotkeyOutcome> {
    let mut grabbed = Vec::new();
    for ignored in ignored_modifier_variants() {
        let variant = modifiers | ignored;
        let result = conn
            .grab_key(
                false,
                root,
                variant,
                keycode,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )
            .map_err(|_| HotkeyOutcome::Failed {
                reason: "x11_grab_request_failed".into(),
            })
            .and_then(|cookie| cookie.check().map_err(grab_error_outcome));
        match result {
            Ok(()) => grabbed.push(variant),
            Err(outcome) => {
                for registered in grabbed {
                    if let Ok(cookie) = conn.ungrab_key(keycode, root, registered) {
                        let _ = cookie.check();
                    }
                }
                let _ = conn.flush();
                return Err(outcome);
            }
        }
    }
    let _ = conn.flush();
    Ok(())
}

fn grab_error_outcome(error: ReplyError) -> HotkeyOutcome {
    if matches!(error, ReplyError::X11Error(ref x11_error) if x11_error.error_kind == ErrorKind::Access)
    {
        HotkeyOutcome::Conflict {
            reason: "x11_binding_already_grabbed".into(),
        }
    } else {
        HotkeyOutcome::Failed {
            reason: "x11_grab_failed".into(),
        }
    }
}

fn clear_bindings(
    conn: &RustConnection,
    root: Window,
    bindings: &mut HashMap<Keycode, Vec<RegisteredBinding>>,
) {
    for (keycode, entries) in bindings.iter() {
        for entry in entries {
            for ignored in ignored_modifier_variants() {
                if let Ok(cookie) = conn.ungrab_key(*keycode, root, entry.modifiers | ignored) {
                    let _ = cookie.check();
                }
            }
        }
    }
    bindings.clear();
    let _ = conn.flush();
}

fn binding_matches(
    bindings: &HashMap<Keycode, Vec<RegisteredBinding>>,
    keycode: Keycode,
    modifiers: ModMask,
) -> bool {
    bindings
        .get(&keycode)
        .is_some_and(|entries| entries.iter().any(|entry| entry.modifiers == modifiers))
}

fn activate_matching(
    bindings: &mut HashMap<Keycode, Vec<RegisteredBinding>>,
    keycode: Keycode,
    modifiers: ModMask,
) -> Vec<Callback> {
    let Some(entries) = bindings.get_mut(&keycode) else {
        return Vec::new();
    };
    entries
        .iter_mut()
        .filter_map(|entry| {
            if entry.modifiers == modifiers && !entry.pressed {
                entry.pressed = true;
                Some(Arc::clone(&entry.callback))
            } else {
                None
            }
        })
        .collect()
}

fn release_bindings(bindings: &mut HashMap<Keycode, Vec<RegisteredBinding>>, keycode: Keycode) {
    if let Some(entries) = bindings.get_mut(&keycode) {
        for entry in entries {
            entry.pressed = false;
        }
    }
}

fn invoke_callbacks(callbacks: Vec<Callback>) {
    for callback in callbacks {
        callback();
    }
}

fn queue_pending_raw(
    pending_raw: &mut Vec<PendingRawActivation>,
    keycode: Keycode,
    modifiers: ModMask,
) {
    if pending_raw
        .iter()
        .any(|pending| pending.keycode == keycode && pending.modifiers == modifiers)
    {
        return;
    }
    pending_raw.push(PendingRawActivation {
        keycode,
        modifiers,
        observed_at: Instant::now(),
    });
}

fn take_pending_raw(
    pending_raw: &mut Vec<PendingRawActivation>,
    keycode: Keycode,
) -> Option<PendingRawActivation> {
    pending_raw
        .iter()
        .position(|pending| pending.keycode == keycode)
        .map(|index| pending_raw.remove(index))
}

fn discard_pending_raw(pending_raw: &mut Vec<PendingRawActivation>, keycode: Keycode) {
    pending_raw.retain(|pending| pending.keycode != keycode);
}

fn activate_expired_raw(
    bindings: &mut HashMap<Keycode, Vec<RegisteredBinding>>,
    pending_raw: &mut Vec<PendingRawActivation>,
) {
    let now = Instant::now();
    let mut ready = Vec::new();
    pending_raw.retain(|pending| {
        if now.duration_since(pending.observed_at) >= RAW_FALLBACK_WAIT {
            ready.push((pending.keycode, pending.modifiers));
            false
        } else {
            true
        }
    });
    for (keycode, modifiers) in ready {
        let callbacks = activate_matching(bindings, keycode, modifiers);
        if !callbacks.is_empty() {
            info!(backend = "xinput2_raw", "X11 raw hotkey fallback activated");
            invoke_callbacks(callbacks);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl_shift() -> ModMask {
        ModMask::CONTROL | ModMask::SHIFT
    }

    #[test]
    fn activation_is_once_per_press_and_resets_on_release() {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let callback_calls = Arc::clone(&calls);
        let callback: Callback = Arc::new(move || {
            callback_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        let mut bindings = HashMap::from([(
            55,
            vec![RegisteredBinding {
                modifiers: ctrl_shift(),
                callback,
                pressed: false,
            }],
        )]);

        invoke_callbacks(activate_matching(&mut bindings, 55, ctrl_shift()));
        invoke_callbacks(activate_matching(&mut bindings, 55, ctrl_shift()));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);

        release_bindings(&mut bindings, 55);
        invoke_callbacks(activate_matching(&mut bindings, 55, ctrl_shift()));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn raw_modifiers_track_controls_and_ignore_lock_slots() {
        let mut state = RawModifierState::default();
        let modifier_keys = HashMap::from([
            (37, ModMask::CONTROL),
            (50, ModMask::SHIFT),
            (66, ModMask::LOCK),
            (77, ModMask::M2),
        ]);

        state.update(&modifier_keys, 37, true);
        state.update(&modifier_keys, 50, true);
        state.update(&modifier_keys, 66, true);
        state.update(&modifier_keys, 77, true);
        assert_eq!(normalized_modifiers(state.active()), ctrl_shift());

        state.update(&modifier_keys, 50, false);
        assert_eq!(normalized_modifiers(state.active()), ModMask::CONTROL);
    }

    #[test]
    fn raw_route_checks_only_matching_bindings() {
        let bindings = HashMap::from([(
            55,
            vec![RegisteredBinding {
                modifiers: ctrl_shift(),
                callback: Arc::new(|| {}),
                pressed: false,
            }],
        )]);
        assert!(binding_matches(&bindings, 55, ctrl_shift()));
        assert!(!binding_matches(&bindings, 55, ModMask::CONTROL));
        assert!(!binding_matches(&bindings, 54, ctrl_shift()));
    }

    #[test]
    fn passive_event_cancels_its_pending_raw_fallback() {
        let mut pending = Vec::new();
        queue_pending_raw(&mut pending, 55, ctrl_shift());

        discard_pending_raw(&mut pending, 55);

        assert!(pending.is_empty());
    }

    #[test]
    fn expired_raw_fallback_activates_once_when_passive_event_never_arrives() {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let callback_calls = Arc::clone(&calls);
        let mut bindings = HashMap::from([(
            55,
            vec![RegisteredBinding {
                modifiers: ctrl_shift(),
                callback: Arc::new(move || {
                    callback_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }),
                pressed: false,
            }],
        )]);
        let mut pending = vec![PendingRawActivation {
            keycode: 55,
            modifiers: ctrl_shift(),
            observed_at: Instant::now() - RAW_FALLBACK_WAIT,
        }];

        activate_expired_raw(&mut bindings, &mut pending);
        activate_expired_raw(&mut bindings, &mut pending);

        assert!(pending.is_empty());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn x11_mapping_matches_the_cross_platform_binding_contract() {
        let modifiers = x11_modifiers(HotkeyModifiers {
            cmd_or_ctrl: true,
            shift: true,
            alt: true,
            meta: true,
        });
        assert_eq!(
            normalized_modifiers(modifiers),
            ModMask::CONTROL | ModMask::SHIFT | ModMask::M1 | ModMask::M4
        );
    }

    #[test]
    fn keysyms_cover_the_only_supported_hotkey_keys() {
        assert!(keysyms_for(HotkeyKey::V).contains(&XK_V_LOWER));
        assert!(keysyms_for(HotkeyKey::Enter).contains(&XK_RETURN));
        assert!(keysyms_for(HotkeyKey::Escape).contains(&XK_ESCAPE));
    }
}
