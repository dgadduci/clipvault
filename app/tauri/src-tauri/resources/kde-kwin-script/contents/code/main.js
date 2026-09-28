/*
    ClipVault - local clipboard manager for KDE Plasma Wayland.
    SPDX-License-Identifier: MIT

    Companion JavaScript module for the
    `clipvault-kde-source-app` KWin script.

    The module reads `workspace.activeWindow.desktopFileName`,
    validates the value, and publishes a metadata-only envelope
    over the session D-Bus bus name `org.clipvault.SourceApp` at
    the object path `/org/clipvault/SourceApp`. The interface is
    `org.clipvault.SourceApp` and the method is `Publish(s id,
    u state, u v)`, where:

        id    — the trimmed desktop file name (for example
                `org.kate.editor.desktop`) or the empty string
                when no application is focused;
        state — `1` for `identified` (a valid identifier is being
                announced) and `2` for `cleared` (no active
                application or the identifier was invalid; the
                Rust bridge MUST clear its cached snapshot);
        v     — the wire protocol version (`1`).

    Only the identifier, the state code and the protocol version
    travel over D-Bus. The Rust bridge rejects messages with a
    different version, sender or with an identifier that does not
    match the documented `[A-Za-z0-9._+-]+\.desktop` pattern.
*/

.pragma library

const CLIPVAULT_BUS_NAME = "org.clipvault.SourceApp";
const CLIPVAULT_OBJECT_PATH = "/org/clipvault/SourceApp";
const CLIPVAULT_INTERFACE = "org.clipvault.SourceApp";
const CLIPVAULT_METHOD = "Publish";
const CLIPVAULT_PROTOCOL_VERSION = 1;

// `state` carries an unsigned integer so a peer can tell apart
// "valid identifier published" from "explicit snapshot clear".
const STATE_IDENTIFIED = 1;
const STATE_CLEARED = 2;

// KWin's API is asynchronous; defer the very first publish until
// the workspace has populated `activeWindow`. Otherwise the very
// first activation can be missed when the script loads before the
// window tree is ready.
let attached = false;

function attach() {
    if (attached) {
        return;
    }
    attached = true;
}

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
    // Strip a leading absolute path KWin may report when the
    // application is launched from a non-standard location. Only
    // the basename ever leaves the script — the Rust bridge is
    // responsible for refusing any leftover path-like shape.
    let slash = value.lastIndexOf("/");
    if (slash >= 0) {
        value = value.substring(slash + 1);
    }
    if (value.length === 0) {
        return "";
    }
    return value;
}

function publish(rawIdentifier) {
    let identifier = normalizeIdentifier(rawIdentifier);
    let state = identifier.length === 0 ? STATE_CLEARED : STATE_IDENTIFIED;

    // KWin exposes `callDBus` synchronously; the Rust bridge
    // answers the `Publish` method without a reply payload so the
    // script thread is not blocked on the bridge.
    try {
        callDBus(
            CLIPVAULT_BUS_NAME,
            CLIPVAULT_OBJECT_PATH,
            CLIPVAULT_INTERFACE,
            CLIPVAULT_METHOD,
            "susu",
            identifier,
            state,
            CLIPVAULT_PROTOCOL_VERSION
        );
    } catch (error) {
        // The bridge may not be running yet. The contract is that
        // the bridge publishes its initial snapshot on reconnect;
        // we never propagate the failure further.
    }
}