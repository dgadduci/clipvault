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

test("the applicable GNOME diagnostics card offers an explicit configuration entry point", () => {
  const source = loadSource("src/DevelopmentModal.svelte");

  assert.match(
    source,
    /gnomeStatus\?\.kind === "ready" && gnomeStatus\.payload\.applicable/,
  );
  assert.match(source, /data-testid="gnome-configure"/);
  assert.match(source, /Configurar integración GNOME/);
  assert.match(source, /dispatch\("configureGnome", gnomeStatus\)/);
});

test("opening GNOME configuration only forwards the status snapshot to the modal", () => {
  const development = loadSource("src/DevelopmentModal.svelte");
  const app = loadSource("src/App.svelte");

  const entryPoint = development.slice(
    development.indexOf("function configureGnome"),
    development.indexOf("async function refreshCapabilities"),
  );
  assert.equal(entryPoint.includes("gnomeIntegrationSetConsentCommand"), false);
  assert.equal(entryPoint.includes("gnomeIntegrationInstallCommand"), false);

  assert.match(app, /import GnomeIntegrationModal from "\.\/GnomeIntegrationModal\.svelte"/);
  assert.match(app, /\| "gnome_integration"/);
  assert.match(app, /open=\{openModal === "gnome_integration"\}/);
  assert.match(app, /initial=\{gnomeIntegrationStatus\}/);
  assert.match(app, /on:configureGnome=\{onConfigureGnome\}/);
});

test("GNOME command errors are actionable and never stringified as an IPC object", () => {
  assert.match(
    describeGnomeIntegrationError({ kind: "bundled_missing", message: "/private/path" }),
    /no incluye los archivos/i,
  );
  assert.match(
    describeGnomeIntegrationError({ kind: "listener_error", message: "/run/user/1000" }),
    /conexión local/i,
  );
  assert.equal(
    describeGnomeIntegrationError({ kind: "install_error", message: "/home/diego/private" }).includes(
      "/home/diego",
    ),
    false,
  );
  assert.equal(describeGnomeIntegrationError({}), "No se pudo completar la operación de integración GNOME. Reintentá.");
});
