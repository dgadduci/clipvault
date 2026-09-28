/*
    ClipVault - local clipboard manager for KDE Plasma Wayland.
    SPDX-License-Identifier: MIT

    This declarative script loads the companion JavaScript module that
    publishes the focused application's desktop file name over the
    session D-Bus. The script is intentionally narrow:

      * the snapshot is only populated from
        `workspace.activeWindow.desktopFileName`
        (metadata the application itself published, the same trust
        model as `WM_CLASS` and Wayland `app_id`);
      * it never reads window titles, process IDs, or clipboard
        contents;
      * it publishes either a validated identifier or an explicit
        empty state so a previous identity never lingers when the
        focus drops.

    The Rust bridge at `org.clipvault.SourceApp` validates the wire
    envelope, refuses unknown protocol versions, and clears the
    snapshot when the session drops.
*/
import QtQuick

import org.kde.kwin

Loader {
    id: clipvaultSourceAppRoot

    source: Qt.resolvedUrl("../code/main.js")

    onStatusChanged: {
        if (status == Loader.Ready) {
            // Push the initial snapshot once the script has been
            // evaluated so a ClipVault instance that connects after
            // KWin has loaded the script still observes the current
            // active application.
            if (item && typeof item.publishInitial === "function") {
                item.publishInitial();
            }
        }
    }

    Component.onCompleted: {
        if (item && typeof item.attach === "function") {
            item.attach();
        }
    }

    Connections {
        target: Workspace
        // `windowActivated` is the signal KWin fires whenever the
        // active window changes. We forward it to the JS module so
        // the publish pipeline stays in one place.
        function onWindowActivated() {
            if (clipvaultSourceAppRoot.item
                && typeof clipvaultSourceAppRoot.item.onWindowActivated === "function") {
                clipvaultSourceAppRoot.item.onWindowActivated();
            }
        }
    }
}