/*
    ClipVault - local clipboard manager for KDE Plasma Wayland.
    SPDX-License-Identifier: MIT

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
const CLIPVAULT_PROTOCOL_VERSION = 2;
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
print("ClipVault KWin source-app script started");
publishInitial();
