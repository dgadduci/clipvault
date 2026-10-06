import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import path from "node:path";
import {
  contextualDesktopSetupCandidates,
} from "../src/lib/contextualDesktopSetupGuidance.js";
import type {
  ContextualDesktopSetupDismissals,
  Diagnostics,
  GnomeIntegrationStatusResponse,
  KdeKwinIntegrationPayload,
} from "../src/types.js";

const linuxWayland: Pick<Diagnostics, "platform_os" | "display_server"> = {
  platform_os: "linux",
  display_server: "wayland",
};
const defaultDismissals: ContextualDesktopSetupDismissals = {
  gnome_dismissed: false,
  kde_dismissed: false,
};
const frontendRoot = process.cwd();

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(frontendRoot, ...segments), "utf8");
}
const gnomeSetup: GnomeIntegrationStatusResponse = {
  kind: "ready",
  payload: {
    applicable: true,
    session: "wayland",
    desktop: "GNOME",
    consent: "unknown",
    technical_state: "not_installed",
    installed: false,
    extensions_manager_available: false,
    identifier: null,
    detail: null,
    uuid: "",
    backend: "",
    protocol_version: 0,
  },
};
const kdeSetup: KdeKwinIntegrationPayload = {
  applicable: true,
  session: "wayland",
  consent: "unknown",
  technical_state: "awaiting_consent",
  installed: false,
  enabled: false,
  backend: "",
  protocol_version: 0,
  detail: null,
  error: null,
};

test("shows the matching initial setup guidance for typed Wayland status", () => {
  assert.deepEqual(
    contextualDesktopSetupCandidates(
      linuxWayland,
      gnomeSetup,
      { ...kdeSetup, applicable: false },
      defaultDismissals,
    ),
    ["gnome"],
  );
  assert.deepEqual(
    contextualDesktopSetupCandidates(
      linuxWayland,
      { kind: "not_applicable", session: "wayland", desktop: "KDE" },
      kdeSetup,
      defaultDismissals,
    ),
    ["kde"],
  );
});

test("keeps dismissal independent for each integration", () => {
  assert.deepEqual(
    contextualDesktopSetupCandidates(
      linuxWayland,
      gnomeSetup,
      kdeSetup,
      { gnome_dismissed: true, kde_dismissed: false },
    ),
    ["kde"],
  );
});

test("suppresses integration states that are decided, active, or disabled", () => {
  assert.deepEqual(
    contextualDesktopSetupCandidates(
      linuxWayland,
      {
        ...gnomeSetup,
        payload: { ...gnomeSetup.payload, consent: "declined" },
      },
      { ...kdeSetup, technical_state: "identified", enabled: true },
      defaultDismissals,
    ),
    [],
  );
  assert.deepEqual(
    contextualDesktopSetupCandidates(
      linuxWayland,
      {
        ...gnomeSetup,
        payload: { ...gnomeSetup.payload, consent: "disabled" },
      },
      { ...kdeSetup, consent: "disabled", technical_state: "disabled" },
      defaultDismissals,
    ),
    [],
  );
});

test("does not offer Wayland setup on macOS, X11, or unknown desktops", () => {
  for (const diagnostics of [
    { platform_os: "macos", display_server: "unknown" },
    { platform_os: "linux", display_server: "x11" },
    { platform_os: "linux", display_server: "unknown" },
  ] as Pick<Diagnostics, "platform_os" | "display_server">[]) {
    assert.deepEqual(
      contextualDesktopSetupCandidates(
        diagnostics,
        gnomeSetup,
        kdeSetup,
        defaultDismissals,
      ),
      [],
    );
  }
});

test("configure opens and focuses integration settings without granting consent", () => {
  const app = loadSource("src/App.svelte");
  const card = loadSource("src/ContextualDesktopSetupCard.svelte");
  const modal = loadSource("src/DesktopIntegrationsModal.svelte");

  assert.match(app, /onConfigure=\{onConfigureDesktopSetup\}/);
  assert.match(app, /focusIntegration=\{desktopIntegrationFocus\}/);
  assert.match(app, /closedModal === "desktop_integrations"[\s\S]*?loadContextualSetupGuidance\(\)/);
  assert.match(app, /contextualSetupFirstFrame = requestAnimationFrame/);
  assert.match(app, /gnomeIntegrationStatusCommand\(\),\s*kdeKwinIntegrationStatusCommand\(\)/);
  assert.match(modal, /focusIntegration === "gnome"\) gnomeFocusTarget\?\.focus\(\)/);
  assert.match(modal, /focusIntegration === "kde"\) kdeFocusTarget\?\.focus\(\)/);
  assert.match(card, /onConfigure\(\s*integration/);
  assert.match(card, /onDismiss\(integration\)/);
  assert.doesNotMatch(card, /gnomeIntegrationSetConsentCommand|kdeKwinIntegrationActivateCommand/);
});
