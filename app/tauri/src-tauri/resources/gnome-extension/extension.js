// ClipVault - GNOME Shell Extension
//
// This extension communicates ONLY the focused application's desktop
// identifier to a local ClipVault process through a metadata-only
// channel. It does not read window content, does not enumerate
// /proc, does not scrape titles and does not pipe clipboard data.
//
// Channel: a per-session socket file inside
// `$XDG_RUNTIME_DIR/clipvault/`. The probe on the Rust side acts as
// the listener; this extension only owns the writer side. Both
// peers negotiate a 16-bit protocol version on connect and refuse
// messages from incompatible peers, so a stray reader cannot cause
// the extension to leak state.

const Me = imports.misc.extensionUtils.getCurrentExtension();
const Main = imports.ui.main;
const GLib = imports.gi.GLib;
const Gio = imports.gi.Gio;
const Meta = imports.gi.Meta;
const Shell = imports.gi.Shell;

const PROTOCOL_VERSION = 1;
const SOCKET_BASENAME = 'clipvault-focus.sock';
const CONNECT_BACKOFF_MS = 250;
const MAX_BACKOFF_MS = 4000;
const FOCUS_HYSTERESIS_MS = 80;
const QUICK_PASTE_ACCELERATOR = '<Control><Shift>v';
const CAPTURE_TOGGLE_ACCELERATOR = '<Control><Alt><Shift>b';
const COMMAND_SOCKET_BASENAME = 'clipvault-shortcuts.sock';
const QUICK_PASTE_ACTION_MODES = Shell.ActionMode.NORMAL | Shell.ActionMode.OVERVIEW;

const AppState = {
  socket: null,
  socket_path: null,
  output: null,
  command_service: null,
  command_socket_path: null,
  reconnect_source: null,
  focus_source: null,
  accelerator_source: null,
  accelerator_action: 0,
  capture_toggle_source: null,
  capture_toggle_action: 0,
  reconnect_attempts: 0,
  busy: false,
  // `sending` tracks every async write on the output stream so a new
  // write cannot be queued while the previous one is still in flight.
  // The flag covers both the handshake and the app_id frames; the
  // queue relies on this so a focus change observed during the
  // handshake window never races the handshake write on the wire.
  sending: false,
  // `handshake_complete` flips to `true` once the hello write
  // resolves successfully. Until then the focus handler refuses to
  // publish an `app_id`: the Rust listener drops any app_id that
  // arrives before the hello envelope, so any attempt to send one
  // would close the connection.
  handshake_complete: false,
  pending_app_id: null,
  pending_quick_paste: false,
  pending_capture_toggle: false,
  quick_paste_accelerator: QUICK_PASTE_ACCELERATOR,
  capture_toggle_accelerator: CAPTURE_TOGGLE_ACCELERATOR,
  last_app_id: null,
  destroyed: false,
};

// GNOME Shell 42 loads legacy extensions through this lifecycle entry point.
// There is deliberately no work here: opening the local socket or publishing
// focus before the user-enabled lifecycle reaches `enable()` would violate the
// consent boundary and could leave a stale connection across reloads.
function init() {}

function _log(...args) {
    return;
}

function _buildSocketPath() {
    const runtimeDir = GLib.getenv('XDG_RUNTIME_DIR');
    if (!runtimeDir) return null;
    const dir = runtimeDir + '/clipvault';
    try {
        if (GLib.file_test(dir, GLib.FileTest.IS_DIR) === false) {
            GLib.mkdir_with_parents(dir, 0o700);
        }
    } catch (e) {
        return null;
    }
    return dir + '/' + SOCKET_BASENAME;
}

function _resetSocket() {
    AppState.socket = null;
    AppState.output = null;
    AppState.sending = false;
    AppState.handshake_complete = false;
    AppState.pending_quick_paste = false;
    AppState.pending_capture_toggle = false;
}

function _scheduleReconnect() {
    if (AppState.reconnect_source) return;
    if (AppState.destroyed) return;
    const delayMs = Math.min(
        MAX_BACKOFF_MS,
        CONNECT_BACKOFF_MS * Math.pow(2, AppState.reconnect_attempts),
    );
    AppState.reconnect_attempts += 1;
    AppState.reconnect_source = GLib.timeout_add(
        GLib.PRIORITY_DEFAULT,
        delayMs,
        function () {
            AppState.reconnect_source = null;
            _connect();
            return GLib.SOURCE_REMOVE;
        },
    );
}

function _onSocketLost() {
    _resetSocket();
    AppState.last_app_id = null;
    AppState.pending_app_id = null;
    _scheduleReconnect();
}

function _nextQueuedMessage() {
    if (AppState.pending_app_id !== null) {
        const message = JSON.stringify({
            v: PROTOCOL_VERSION,
            kind: 'app_id',
            app_id: AppState.pending_app_id,
        }) + '\n';
        AppState.pending_app_id = null;
        return message;
    }
    if (AppState.pending_quick_paste) {
        AppState.pending_quick_paste = false;
        return JSON.stringify({
            v: PROTOCOL_VERSION,
            kind: 'quick_paste',
        }) + '\n';
    }
    if (AppState.pending_capture_toggle) {
        AppState.pending_capture_toggle = false;
        return JSON.stringify({
            v: PROTOCOL_VERSION,
            kind: 'toggle_capture',
        }) + '\n';
    }
    return null;
}

function _sendQueued() {
    if (!AppState.output || AppState.sending) return;
    if (!AppState.handshake_complete) {
        // The handshake must finish before any app_id is published.
        // Until it does, the queued value stays in `pending_app_id`
        // and is replayed the moment the handshake callback runs.
        return;
    }
    const message = _nextQueuedMessage();
    if (message === null) return;
    AppState.sending = true;
    AppState.output.write_async(
        message,
        GLib.PRIORITY_DEFAULT,
        null,
        function (source, result) {
            try {
                const written = source.write_finish(result);
                AppState.sending = false;
                if (written < message.length) {
                    _onSocketLost();
                    return;
                }
            } catch (e) {
                _onSocketLost();
                return;
            }
            if (AppState.pending_app_id !== null || AppState.pending_quick_paste
                || AppState.pending_capture_toggle) {
                _sendQueued();
            }
        },
    );
}

function _publish(app_id) {
    if (AppState.destroyed) return;
    if (!app_id || typeof app_id !== 'string') {
        app_id = '';
    }
    if (app_id === AppState.last_app_id && AppState.output === null) {
        return;
    }
    AppState.last_app_id = app_id;
    AppState.pending_app_id = app_id;
    if (AppState.output === null) {
        _scheduleReconnect();
        return;
    }
    _sendQueued();
}

function _publishQuickPaste() {
    if (AppState.destroyed || AppState.output === null) return;
    // A request belongs to the current local connection. Replaying it after
    // a reconnect could open the modal long after the user pressed the key,
    // so `_resetSocket()` deliberately drops it on any socket failure.
    AppState.pending_quick_paste = true;
    _sendQueued();
}

function _publishCaptureToggle() {
    if (AppState.destroyed || AppState.output === null) return;
    // Do not replay a toggle after reconnect; the user may have since
    // changed the state through Settings or the tray.
    AppState.pending_capture_toggle = true;
    _sendQueued();
}

function _resolveFocusedAppId() {
    try {
        const tracker = Shell.WindowTracker.get_default();
        if (!tracker) return '';
        const focusApp = tracker.focus_app;
        if (!focusApp) return '';
        if (typeof focusApp.get_id !== 'function') return '';
        // A `Shell.App` may have no associated `.desktop` file when it
        // is window-backed. `is_window_backed()` reports that case
        // explicitly so we MUST publish absence instead of an
        // ephemeral identifier the resolver cannot honour. Guarding
        // here keeps the IPC contract metadata-only: the extension
        // never sends the window index, the title, the PID or any
        // other identifying artefact as a fallback.
        if (typeof focusApp.is_window_backed === 'function'
            && focusApp.is_window_backed()) {
            return '';
        }
        const appId = focusApp.get_id();
        if (typeof appId !== 'string') return '';
        const trimmed = appId.trim();
        // Defensive guard for older GNOME Shell runtimes where
        // `is_window_backed()` may be absent or throw. A leading
        // `window:` prefix is the documented shape GNOME uses for an
        // app that is not associated with a `.desktop` file, so
        // treating it as absence keeps the channel metadata-only.
        if (trimmed.indexOf('window:') === 0) {
            return '';
        }
        return trimmed;
    } catch (e) {
        return '';
    }
}

function _checkFocus() {
    const next = _resolveFocusedAppId();
    if (next === AppState.last_app_id) return;
    _publish(next);
}

function _installQuickPasteBinding() {
    return _replaceQuickPasteBinding(AppState.quick_paste_accelerator);
}

function _replaceQuickPasteBinding(accelerator) {
    if (AppState.accelerator_action && AppState.quick_paste_accelerator === accelerator) {
        return 'registered';
    }
    let action = 0;
    let source = null;
    try {
        action = global.display.grab_accelerator(
            accelerator,
            Meta.KeyBindingFlags.IGNORE_AUTOREPEAT,
        );
        if (!action || action === Meta.KeyBindingAction.NONE) return 'conflict';
        const bindingName = Meta.external_binding_name_for_action(action);
        Main.wm.allowKeybinding(bindingName, QUICK_PASTE_ACTION_MODES);
        source = global.display.connect(
            'accelerator-activated',
            function (_display, activatedAction) {
                if (activatedAction !== action) return;
                // Queue any new focus id first so the listener observes the
                // same metadata ordering as the existing focus bridge.
                _checkFocus();
                _publishQuickPaste();
            },
        );
        const oldAction = AppState.accelerator_action;
        const oldSource = AppState.accelerator_source;
        AppState.accelerator_action = action;
        AppState.accelerator_source = source;
        AppState.quick_paste_accelerator = accelerator;
        _releaseBinding(oldAction, oldSource);
        return 'registered';
    } catch (e) {
        if (source !== null) {
            try { global.display.disconnect(source); } catch (ignored) {}
        }
        _releaseBinding(action, null);
        return 'failed';
    }
}

function _installCaptureToggleBinding() {
    return _replaceCaptureToggleBinding(AppState.capture_toggle_accelerator);
}

function _replaceCaptureToggleBinding(accelerator) {
    if (AppState.capture_toggle_action && AppState.capture_toggle_accelerator === accelerator) {
        return 'registered';
    }
    let action = 0;
    let source = null;
    try {
        action = global.display.grab_accelerator(
            accelerator,
            Meta.KeyBindingFlags.IGNORE_AUTOREPEAT,
        );
        if (!action || action === Meta.KeyBindingAction.NONE) {
            return 'conflict';
        }
        const bindingName = Meta.external_binding_name_for_action(action);
        Main.wm.allowKeybinding(bindingName, QUICK_PASTE_ACTION_MODES);
        source = global.display.connect(
            'accelerator-activated',
            function (_display, activatedAction) {
                if (activatedAction !== action) return;
                _publishCaptureToggle();
            },
        );
        const oldAction = AppState.capture_toggle_action;
        const oldSource = AppState.capture_toggle_source;
        AppState.capture_toggle_action = action;
        AppState.capture_toggle_source = source;
        AppState.capture_toggle_accelerator = accelerator;
        _releaseBinding(oldAction, oldSource);
        return 'registered';
    } catch (e) {
        if (source !== null) {
            try { global.display.disconnect(source); } catch (ignored) {}
        }
        _releaseBinding(action, null);
        return 'failed';
    }
}

function _releaseBinding(action, source) {
    if (source !== null) {
        try { global.display.disconnect(source); } catch (e) {}
    }
    if (!action) return;
    try {
        const bindingName = Meta.external_binding_name_for_action(action);
        Main.wm.allowKeybinding(bindingName, Shell.ActionMode.NONE);
        global.display.ungrab_accelerator(action);
    } catch (e) {}
}

function _shortcutCommandPath() {
    const focusPath = _buildSocketPath();
    if (!focusPath) return null;
    return focusPath.replace(SOCKET_BASENAME, COMMAND_SOCKET_BASENAME);
}

function _replyShortcutCommand(connection, response) {
    const output = connection.get_output_stream();
    output.write_async(
        JSON.stringify(response) + '\n',
        GLib.PRIORITY_DEFAULT,
        null,
        function (source, result) {
            try { source.write_finish(result); } catch (e) {}
            try { connection.close(null); } catch (e) {}
        },
    );
}

function _handleShortcutCommand(connection) {
    const input = Gio.DataInputStream.new(connection.get_input_stream());
    input.read_line_async(GLib.PRIORITY_DEFAULT, null, function (source, result) {
        let line = null;
        try {
            const [value] = source.read_line_finish_utf8(result);
            line = value;
        } catch (e) {}
        let request = null;
        try { request = line ? JSON.parse(line) : null; } catch (e) {}
        let status = 'failed';
        const validAccelerator = request && typeof request.accelerator === 'string'
            && /^((<(Control|Alt|Shift|Super)>){1,4})([A-Za-z0-9]|Return|Escape|space)$/.test(request.accelerator);
        if (request && request.v === PROTOCOL_VERSION && request.kind === 'set_shortcut'
            && validAccelerator) {
            if (request.action === 'open_quick_paste') {
                status = _replaceQuickPasteBinding(request.accelerator);
            } else if (request.action === 'toggle_clipboard_capture') {
                status = _replaceCaptureToggleBinding(request.accelerator);
            } else {
                status = 'unsupported';
            }
        }
        _replyShortcutCommand(connection, {
            v: PROTOCOL_VERSION,
            kind: 'shortcut_result',
            action: request && typeof request.action === 'string' ? request.action : '',
            status,
        });
    });
}

function _startShortcutCommandServer() {
    const path = _shortcutCommandPath();
    if (!path) return;
    try {
        if (GLib.file_test(path, GLib.FileTest.EXISTS)) GLib.unlink(path);
        const service = new Gio.SocketService();
        service.add_address(
            new Gio.UnixSocketAddress({ path }),
            Gio.SocketType.STREAM,
            Gio.SocketProtocol.DEFAULT,
            null,
        );
        service.connect('incoming', function (_service, connection) {
            _handleShortcutCommand(connection);
            return true;
        });
        service.start();
        AppState.command_service = service;
        AppState.command_socket_path = path;
    } catch (e) {
        AppState.command_service = null;
        AppState.command_socket_path = null;
    }
}

function _stopShortcutCommandServer() {
    if (AppState.command_service) {
        try { AppState.command_service.stop(); } catch (e) {}
        AppState.command_service = null;
    }
    if (AppState.command_socket_path) {
        try { GLib.unlink(AppState.command_socket_path); } catch (e) {}
        AppState.command_socket_path = null;
    }
}

function _uninstallCaptureToggleBinding() {
    if (AppState.capture_toggle_source !== null) {
        try {
            global.display.disconnect(AppState.capture_toggle_source);
        } catch (e) {}
        AppState.capture_toggle_source = null;
    }
    const action = AppState.capture_toggle_action;
    AppState.capture_toggle_action = 0;
    if (!action) return;
    try {
        const bindingName = Meta.external_binding_name_for_action(action);
        Main.wm.allowKeybinding(bindingName, Shell.ActionMode.NONE);
        global.display.ungrab_accelerator(action);
    } catch (e) {}
}

function _uninstallQuickPasteBinding() {
    if (AppState.accelerator_source !== null) {
        try {
            global.display.disconnect(AppState.accelerator_source);
        } catch (e) {}
        AppState.accelerator_source = null;
    }
    const action = AppState.accelerator_action;
    AppState.accelerator_action = 0;
    if (!action) return;
    try {
        const bindingName = Meta.external_binding_name_for_action(action);
        Main.wm.allowKeybinding(bindingName, Shell.ActionMode.NONE);
        global.display.ungrab_accelerator(action);
    } catch (e) {}
}

function _connect() {
    if (AppState.destroyed) return;
    if (AppState.socket) return;
    if (!AppState.socket_path) {
        AppState.socket_path = _buildSocketPath();
    }
    if (!AppState.socket_path) {
        _scheduleReconnect();
        return;
    }
    let socket;
    try {
        socket = new Gio.SocketClient({
            type: Gio.SocketType.STREAM,
        });
        const addr = new Gio.UnixSocketAddress({ path: AppState.socket_path });
        socket.connect_async(
            addr,
            null,
            function (client, result) {
                let conn = null;
                try {
                    conn = client.connect_finish(result);
                } catch (e) {
                    _scheduleReconnect();
                    return;
                }
                if (AppState.destroyed) {
                    try {
                        conn.close(null);
                    } catch (e2) {}
                    return;
                }
                AppState.socket = conn;
                AppState.output = conn.get_output_stream();
                AppState.reconnect_attempts = 0;
                AppState.handshake_complete = false;
                AppState.sending = true;
                const handshake =
                    JSON.stringify({ v: PROTOCOL_VERSION, kind: 'hello' }) + '\n';
                AppState.output.write_async(
                    handshake,
                    GLib.PRIORITY_DEFAULT,
                    null,
                    function (source, result) {
                        try {
                            const written = source.write_finish(result);
                            AppState.sending = false;
                            if (written < handshake.length) {
                                _onSocketLost();
                                return;
                            }
                            // Mark the handshake as complete only
                            // after the hello bytes reach the
                            // listener. Any focus change queued
                            // while the handshake was in flight is
                            // flushed here, in protocol order.
                            AppState.handshake_complete = true;
                            if (AppState.pending_app_id !== null || AppState.pending_quick_paste
                                || AppState.pending_capture_toggle) {
                                _sendQueued();
                            }
                        } catch (e) {
                            _onSocketLost();
                            return;
                        }
                    },
                );
                _checkFocus();
            },
        );
    } catch (e) {
        _scheduleReconnect();
    }
}

function enable() {
    AppState.destroyed = false;
    _installQuickPasteBinding();
    _installCaptureToggleBinding();
    _startShortcutCommandServer();
    AppState.socket_path = _buildSocketPath();
    _connect();
    if (AppState.focus_source === null) {
        AppState.focus_source = GLib.timeout_add(
            GLib.PRIORITY_DEFAULT,
            FOCUS_HYSTERESIS_MS,
            function () {
                _checkFocus();
                return GLib.SOURCE_CONTINUE;
            },
        );
    }
    _checkFocus();
}

function disable() {
    AppState.destroyed = true;
    _stopShortcutCommandServer();
    _uninstallQuickPasteBinding();
    _uninstallCaptureToggleBinding();
    if (AppState.focus_source !== null) {
        GLib.source_remove(AppState.focus_source);
        AppState.focus_source = null;
    }
    if (AppState.reconnect_source !== null) {
        GLib.source_remove(AppState.reconnect_source);
        AppState.reconnect_source = null;
    }
    if (AppState.socket) {
        try {
            AppState.socket.close(null);
        } catch (e) {
            _log('close failed', e);
        }
    }
    _resetSocket();
    AppState.last_app_id = null;
    AppState.pending_app_id = null;
    AppState.pending_quick_paste = false;
    AppState.pending_capture_toggle = false;
    AppState.reconnect_attempts = 0;
    AppState.handshake_complete = false;
}
