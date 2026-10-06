import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

import {
  gnomeIntegrationStatusKind,
  hasManageableDesktopIntegration,
  kdeIntegrationStatusKind,
} from "../src/lib/desktopIntegrationSettings.ts";
import type {
  GnomeIntegrationPayload,
  GnomeIntegrationStatusResponse,
  KdeKwinIntegrationPayload,
} from "../src/types.ts";

const frontendRoot = process.cwd();

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(frontendRoot, ...segments), "utf8");
}

function gnomeStatus(
  overrides: Partial<GnomeIntegrationPayload> = {},
): GnomeIntegrationStatusResponse {
  return {
    kind: "ready",
    payload: {
      applicable: true,
      session: "linux_wayland",
      desktop: "gnome",
      consent: "unknown",
      technical_state: "not_installed",
      installed: false,
      identifier: null,
      detail: null,
      uuid: "clipvault@clipvault.app",
      backend: "gnome-shell",
      protocol_version: 1,
      ...overrides,
    },
  };
}

function kdeStatus(
  overrides: Partial<KdeKwinIntegrationPayload> = {},
): KdeKwinIntegrationPayload {
  return {
    applicable: true,
    session: "kde_plasma_wayland",
    consent: "unknown",
    technical_state: "awaiting_consent",
    installed: false,
    enabled: false,
    backend: "kwin-script",
    protocol_version: 1,
    detail: null,
    error: null,
    ...overrides,
  };
}

test("desktop integration settings appear only for an applicable integration", () => {
  assert.equal(hasManageableDesktopIntegration(gnomeStatus(), null), true);
  assert.equal(
    hasManageableDesktopIntegration(
      { kind: "not_applicable", session: "linux_x11", desktop: "gnome" },
      kdeStatus({ applicable: false, session: "not_wayland" }),
    ),
    false,
  );
  assert.equal(
    hasManageableDesktopIntegration(
      { kind: "not_applicable", session: "linux_x11", desktop: "gnome" },
      kdeStatus({ applicable: false, installed: true, session: "not_wayland" }),
    ),
    false,
  );
});

test("integration status is translated to a user-friendly category", () => {
  assert.equal(gnomeIntegrationStatusKind(gnomeStatus({ technical_state: "identified" })), "active");
  assert.equal(
    gnomeIntegrationStatusKind(gnomeStatus({ technical_state: "activation_pending" })),
    "pending",
  );
  assert.equal(
    kdeIntegrationStatusKind(kdeStatus({ technical_state: "communication_error" })),
    "disconnected",
  );
  assert.equal(
    kdeIntegrationStatusKind(kdeStatus({ applicable: false, installed: true, enabled: true, technical_state: "not_applicable" })),
    "unsupported",
  );
});

test("refresh updates both integrations and exposes localized errors", () => {
  const modal = loadSource("src/DesktopIntegrationsModal.svelte");

  assert.match(modal, /const \[gnomeResult, kdeResult\] = await Promise\.allSettled/);
  assert.match(modal, /gnomeStatus = gnomeResult\.value/);
  assert.match(modal, /on:click=\{refreshStatus\}/);
  assert.match(modal, /gnomeError = "settings\.desktop_integrations\.error"/);
  assert.match(modal, /kdeError = "settings\.desktop_integrations\.error"/);
  assert.match(modal, /role="alert"/);
});

test("General settings opens the shared desktop integrations modal", () => {
  const settings = loadSource("src/GeneralSettingsModal.svelte");
  const app = loadSource("src/App.svelte");
  const modal = loadSource("src/DesktopIntegrationsModal.svelte");

  assert.match(settings, /data-testid="desktop-integrations-setting"/);
  assert.match(settings, /desktopIntegrationsRequested/);
  assert.match(app, /on:desktopIntegrationsRequested=\{onOpenDesktopIntegrations\}/);
  assert.match(app, /<DesktopIntegrationsModal\s+open=\{openModal === "desktop_integrations"\}/);
  assert.match(modal, /data-testid="desktop-integrations-kde-activate"/);
  assert.match(modal, /<GnomeIntegrationModal/);
  assert.doesNotMatch(modal, /technical_state\}/);
  assert.match(modal, /settings\.desktop_integrations\.status\.pending\.kde/);
  assert.match(
    loadSource("src/GnomeIntegrationModal.svelte"),
    /settings\.desktop_integrations\.status\.pending\.gnome/,
  );
  assert.match(settings, /Promise\.allSettled\(\[\s*gnomeIntegrationStatusCommand\(\),\s*kdeKwinIntegrationStatusCommand\(\)/);
  assert.doesNotMatch(settings, /platformOs !== "linux"/);
});

test("Development no longer owns the GNOME or KDE integration settings", () => {
  const development = loadSource("src/DevelopmentModal.svelte");
  const app = loadSource("src/App.svelte");

  assert.doesNotMatch(development, /gnomeIntegrationStatusCommand|kdeKwinIntegrationStatusCommand/);
  assert.doesNotMatch(development, /gnome-integration-card|kde-kwin-integration-card/);
  assert.doesNotMatch(development, /configureGnome|openDesktopIntegrations/);
  assert.doesNotMatch(app, /on:configureGnome|on:openDesktopIntegrations/);
  assert.match(app, /on:desktopIntegrationsRequested=\{onOpenDesktopIntegrations\}/);
});
