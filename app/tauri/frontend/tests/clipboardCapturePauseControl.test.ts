import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { captureToggleShortcutLabel } from "../src/lib/clipboardCaptureControl.js";

const FRONTEND_ROOT = process.cwd();

function source(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

test("capture shortcut labels match the native platform defaults", () => {
  assert.equal(captureToggleShortcutLabel("macos"), "⌘⌥⇧B");
  assert.equal(captureToggleShortcutLabel("linux"), "Ctrl+Alt+Shift+B");
  assert.equal(captureToggleShortcutLabel(null), "Ctrl+Alt+Shift+B");
});

test("General Settings uses persisted commands and reflects external toggles", () => {
  const modal = source("src/GeneralSettingsModal.svelte");
  assert.match(modal, /captureControlGetCommand\(\)/);
  assert.match(modal, /captureControlSetCommand\(/);
  assert.match(modal, /listenCaptureControlChanged\(/);
  assert.match(modal, /Captura del portapapeles/);
  assert.match(modal, /Estado: \{captureEnabled \? "Activa" : "Pausada"\}/);
  assert.match(modal, /Atajo global: \{shortcut\}/);
  assert.match(modal, /capture-control-wayland-note/);
  assert.match(modal, /catch \(saveError\)/);
});

test("General Settings loads and persists the capture-note export preference", () => {
  const modal = source("src/GeneralSettingsModal.svelte");
  assert.match(modal, /settingsGetCommand\(\)/);
  assert.match(modal, /capture_notes_sharing_enabled/);
  assert.match(modal, /settingsSetCommand\(\{ capture_notes_sharing_enabled: next \}\)/);
  assert.match(modal, /capture-note-sharing-toggle/);
  assert.match(modal, /shareNotesEnabled = previous/);
});

test("General Settings is reachable from the toolbar overflow menu", () => {
  const toolbar = source("src/DesktopToolbar.svelte");
  const app = source("src/App.svelte");
  assert.match(toolbar, /data-testid="open-general-settings"/);
  assert.match(toolbar, /selectItem\(onOpenGeneralSettings,/);
  assert.match(app, /onOpenGeneralSettings=\{onOpenGeneralSettings\}/);
  assert.match(app, /<GeneralSettingsModal[\s\S]*?platformOs=\{diagnostics\?\.platform_os/);
});

test("GNOME and KDE Wayland bridges register the Ctrl+Alt+Shift+B shortcut", () => {
  const gnomeExtension = readFileSync(
    path.resolve(
      FRONTEND_ROOT,
      "..",
      "src-tauri",
      "resources",
      "gnome-extension",
      "extension.js",
    ),
    "utf8",
  );
  const kdeScript = readFileSync(
    path.resolve(
      FRONTEND_ROOT,
      "..",
      "src-tauri",
      "resources",
      "kde-kwin-script",
      "contents",
      "code",
      "main.js",
    ),
    "utf8",
  );
  assert.match(gnomeExtension, /CAPTURE_TOGGLE_ACCELERATOR = '<Control><Alt><Shift>b'/);
  assert.match(gnomeExtension, /kind:\s*'toggle_capture'/);
  assert.match(kdeScript, /registerShortcut\([\s\S]*?"Ctrl\+Alt\+Shift\+B"/);
  assert.match(kdeScript, /CLIPVAULT_CAPTURE_TOGGLE_METHOD = "ToggleCapture"/);
});
