/*
    ClipVault - local clipboard manager for KDE Plasma Wayland.
    SPDX-License-Identifier: GPL-3.0-only

    JavaScript entry point for the
    `clipvault-kde-source-app` KWin script.

    The module reads `workspace.activeWindow.desktopFileName`,
    validates the value, and publishes a metadata-only envelope
    over the session D-Bus bus name `org.clipvault.SourceApp` at
    the object path `/org/clipvault/SourceApp`. The interface is
    `org.clipvault.SourceApp` and the method is `Publish(s id,
    s state, s v)`, where:

        id    — the canonical desktop file id (for example
                `org.kate.editor.desktop`) or the empty string
                when no application is focused;
        state — string `"1"` for `identified` (a valid identifier is being
                announced) and `2` for `cleared` (no active
                application or the identifier was invalid; the
                Rust bridge MUST clear its cached snapshot);
        v     — string wire protocol version (`2`).

    Only the identifier, the state code and the protocol version
    travel over D-Bus. The Rust bridge rejects messages with a
    different version, sender or with an identifier that does not
    match the documented `[A-Za-z0-9._+-]+\.desktop` pattern.
*/

const CLIPVAULT_BUS_NAME = "org.clipvault.SourceApp";
const CLIPVAULT_OBJECT_PATH = "/org/clipvault/SourceApp";
const CLIPVAULT_INTERFACE = "org.clipvault.SourceApp";
const CLIPVAULT_METHOD = "Publish";
const CLIPVAULT_CAPTURE_TOGGLE_METHOD = "ToggleCapture";
const CLIPVAULT_OPEN_QUICK_PASTE_METHOD = "OpenQuickPaste";
const CLIPVAULT_SHORTCUT_STATUS_METHOD = "ShortcutStatus";
const CLIPVAULT_PROTOCOL_VERSION = 2;
const CLIPVAULT_QUICK_PASTE_ACCELERATOR = "Ctrl+Shift+V";
const CLIPVAULT_CAPTURE_TOGGLE_ACCELERATOR = "Ctrl+Alt+Shift+B";
const CLIPVAULT_QUICK_PASTE_DESCRIPTION = "Open ClipVault QuickVault";
const CLIPVAULT_CAPTURE_TOGGLE_DESCRIPTION = "Toggle ClipVault local clipboard capture";
function shortcutActionName(action, accelerator) {
    return "clipvault-" + action.replace(/_/g, "-") + "-" + accelerator.replace(/[^A-Za-z0-9]/g, "-");
}
let loggedIdentifierAcknowledgement = false;
let loggedEmptyAcknowledgement = false;

// State codes distinguish an identified app from an explicit clear.
const STATE_IDENTIFIED = 1;
const STATE_CLEARED = 2;

function onWindowActivated() {
    publishActiveWindow();
}

function publishInitial() {
    publishActiveWindow();
}

function publishActiveWindow() {
    let active = null;
    try {
        active = workspace.activeWindow;
    } catch (error) {
        active = null;
    }

    let identifier = "";
    if (active !== null && active !== undefined) {
        try {
            identifier = String(active.desktopFileName || "");
        } catch (error) {
            identifier = "";
        }
    }

    publish(identifier);
}

// Validate the identifier before publishing. The Rust bridge
// performs the canonical check; this helper only avoids emitting
// obvious garbage from the script side so we don't generate noise
// for the bridge to reject.
function normalizeIdentifier(raw) {
    if (raw === null || raw === undefined) {
        return "";
    }
    let value = String(raw).trim();
    if (value.length === 0) {
        return "";
    }
    // KWin normally returns the desktop file basename without an
    // extension (for example `org.kate.editor`). It may instead
    // return an absolute path ending in `.desktop`. In either case,
    // only the canonical desktop file id leaves the script; paths
    // never leave this function.
    let slash = value.lastIndexOf("/");
    if (slash >= 0) {
        value = value.substring(slash + 1);
    }
    if (value.length === 0) {
        return "";
    }
    if (!value.endsWith(".desktop")) {
        // Do not turn a malformed suffix into a superficially valid
        // desktop file id by appending another extension.
        if (value.includes(".desktop")) {
            return "";
        }
        value += ".desktop";
    }
    if (value.length > 512 || !/^[A-Za-z0-9._+-]+\.desktop$/.test(value)) {
        return "";
    }
    return value;
}

function publish(rawIdentifier) {
    let identifier = normalizeIdentifier(rawIdentifier);
    let state = identifier.length === 0 ? STATE_CLEARED : STATE_IDENTIFIED;

    // KWin infers D-Bus argument types from the JavaScript values;
    // it does not take a separate signature string. Sending all
    // fields as strings keeps the bridge signature stable.
    try {
        callDBus(
            CLIPVAULT_BUS_NAME,
            CLIPVAULT_OBJECT_PATH,
            CLIPVAULT_INTERFACE,
            CLIPVAULT_METHOD,
            identifier,
            String(state),
            String(CLIPVAULT_PROTOCOL_VERSION),
            function () {
                reportPublishAcknowledgement(identifier.length > 0);
            }
        );
    } catch (error) {
        // The bridge may not be running yet. The contract is that
        // the bridge publishes its initial snapshot on reconnect;
        // we never propagate the failure further.
    }
}

function toggleLocalCapture() {
    // This call carries no clipboard, window or application metadata. The
    // Rust bridge accepts it only from KWin's authenticated D-Bus name.
    try {
        callDBus(
            CLIPVAULT_BUS_NAME,
            CLIPVAULT_OBJECT_PATH,
            CLIPVAULT_INTERFACE,
            CLIPVAULT_CAPTURE_TOGGLE_METHOD,
            function () {}
        );
    } catch (error) {
        // ClipVault may be restarting; no state change is replayed later.
    }
}

function openQuickPaste() {
    try {
        callDBus(
            CLIPVAULT_BUS_NAME,
            CLIPVAULT_OBJECT_PATH,
            CLIPVAULT_INTERFACE,
            CLIPVAULT_OPEN_QUICK_PASTE_METHOD,
            function () {}
        );
    } catch (error) {
        // ClipVault may be restarting; do not replay this activation.
    }
}

function reportShortcutStatus(action, registered) {
    try {
        callDBus(
            CLIPVAULT_BUS_NAME,
            CLIPVAULT_OBJECT_PATH,
            CLIPVAULT_INTERFACE,
            CLIPVAULT_SHORTCUT_STATUS_METHOD,
            action,
            registered ? "registered" : "conflict",
            String(CLIPVAULT_PROTOCOL_VERSION),
            function () {}
        );
    } catch (error) {
        // The bridge may still be starting. Its next reload reports status.
    }
}

function reportPublishAcknowledgement(hasIdentifier) {
    if (hasIdentifier) {
        if (loggedIdentifierAcknowledgement) {
            return;
        }
        loggedIdentifierAcknowledgement = true;
        print("ClipVault KWin bridge acknowledged a snapshot with an identifier");
        return;
    }

    if (loggedEmptyAcknowledgement) {
        return;
    }
    loggedEmptyAcknowledgement = true;
    print("ClipVault KWin bridge acknowledged an empty snapshot");
}

// KWin executes this file as the package's JavaScript entry point.
// Publish immediately for the current focus, then follow future changes.
workspace.windowActivated.connect(onWindowActivated);
const captureShortcutRegistered = registerShortcut(
    shortcutActionName("toggle-clipboard-capture", CLIPVAULT_CAPTURE_TOGGLE_ACCELERATOR),
    CLIPVAULT_CAPTURE_TOGGLE_DESCRIPTION,
    CLIPVAULT_CAPTURE_TOGGLE_ACCELERATOR,
    toggleLocalCapture
);
reportShortcutStatus("toggle_clipboard_capture", captureShortcutRegistered);
if (!captureShortcutRegistered) {
    print("ClipVault Ctrl+Alt+Shift+B shortcut unavailable (conflict)");
}
const quickPasteShortcutRegistered = registerShortcut(
    shortcutActionName("open-quick-paste", CLIPVAULT_QUICK_PASTE_ACCELERATOR),
    CLIPVAULT_QUICK_PASTE_DESCRIPTION,
    CLIPVAULT_QUICK_PASTE_ACCELERATOR,
    openQuickPaste
);
reportShortcutStatus("open_quick_paste", quickPasteShortcutRegistered);
if (!quickPasteShortcutRegistered) {
    print("ClipVault QuickVault shortcut unavailable (conflict)");
}
print("ClipVault KWin source-app script started");
publishInitial();
