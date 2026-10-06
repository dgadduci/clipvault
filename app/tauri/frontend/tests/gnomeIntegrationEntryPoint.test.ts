/**
 * Regression coverage for the GNOME Wayland integration entry point.
 *
 * Diagnostics deliberately stay read-only. On a GNOME Wayland session
 * the status card is the explicit way into the separate consent / install
 * modal; simply inspecting status must never alter local configuration.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

import { describeGnomeIntegrationError } from "../src/lib/gnomeIntegrationError.ts";

const FRONTEND_ROOT = process.cwd();

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

test("GNOME and KDE configuration are only available from General Settings", () => {
  const development = loadSource("src/DevelopmentModal.svelte");
  const settings = loadSource("src/GeneralSettingsModal.svelte");
  const app = loadSource("src/App.svelte");

  assert.doesNotMatch(development, /GNOME|KDE|gnome|kwin|desktopIntegrations/i);
  assert.match(settings, /data-testid="desktop-integrations-setting"/);
  assert.match(settings, /desktopIntegrationsRequested/);
  assert.match(app, /import DesktopIntegrationsModal from "\.\/DesktopIntegrationsModal\.svelte"/);
  assert.match(app, /\| "desktop_integrations"/);
  assert.match(app, /open=\{openModal === "desktop_integrations"\}/);
  assert.match(app, /on:desktopIntegrationsRequested=\{onOpenDesktopIntegrations\}/);
  assert.doesNotMatch(app, /on:configureGnome|on:openDesktopIntegrations/);
});

test("an installed GNOME integration can explicitly reinstall the bundled resource", () => {
  const modal = loadSource("src/GnomeIntegrationModal.svelte");

  assert.match(modal, /data-testid="gnome-reinstall-action"/);
  assert.match(modal, /on:click=\{activate\}/);
  assert.match(modal, /\$t\("gnome\.install\.reinstall"\)/);
  assert.match(modal, /settings\.desktop_integrations\.status\.\$\{gnomeIntegrationStatusKind\(status\)\}/);
  assert.doesNotMatch(modal, /payload\.(backend|protocol_version|identifier|detail)/);
  assert.doesNotMatch(modal, /deshabilitala\/habilitala/);
});

test("accepting GNOME consent starts installation and activation in the same action", () => {
  const modal = loadSource("src/GnomeIntegrationModal.svelte");

  assert.match(modal, /if \(decision === "accepted"\) \{[\s\S]*?await activate\(\)/);
  assert.match(modal, /await gnomeIntegrationInstallCommand\(\);[\s\S]*?refreshUntilConfirmed\(\)/);
  assert.match(modal, /gnomeIntegrationOpenExtensionsCommand/);
  assert.match(modal, /extensionsOpenFailed/);
});

test("GNOME acceptance is persisted before bundled files are loaded, and errors retain it", () => {
  const integration = loadSource("../src-tauri/src/gnome_integration.rs");
  const modal = loadSource("src/GnomeIntegrationModal.svelte");
  const persistIndex = integration.indexOf("self.record_consent(context, GnomeConsentDecision::Accepted)");
  const loadIndex = integration.indexOf("let bundled = load_bundled()", persistIndex);

  assert.notEqual(persistIndex, -1);
  assert.ok(loadIndex > persistIndex);
  assert.match(modal, /lastError = describeGnomeIntegrationError\(err\);\s*await refresh\(true\)/);
});

test("GNOME command errors are actionable and never stringified as an IPC object", () => {
  assert.match(
    describeGnomeIntegrationError({ kind: "bundled_missing", message: "/private/path" }),
    /gnome\.error\.bundled_missing/,
  );
  assert.match(
    describeGnomeIntegrationError({ kind: "listener_error", message: "/run/user/1000" }),
    /gnome\.error\.listener/,
  );
  assert.match(
    describeGnomeIntegrationError({ kind: "disable_error", message: "private process output" }),
    /gnome\.error\.disable/,
  );
  assert.equal(
    describeGnomeIntegrationError({ kind: "install_error", message: "/home/diego/private" }).includes(
      "/home/diego",
    ),
    false,
  );
  assert.equal(describeGnomeIntegrationError({}), "gnome.error.generic");
});

test("GNOME disable keeps the UI busy only until the command settles", () => {
  const modal = loadSource("src/GnomeIntegrationModal.svelte");

  assert.match(
    modal,
    /async function deactivate\(\): Promise<void> \{[\s\S]*?await gnomeIntegrationUninstallCommand\(\);[\s\S]*?finally \{\s*finishAction\(\);\s*\}/,
  );
});
