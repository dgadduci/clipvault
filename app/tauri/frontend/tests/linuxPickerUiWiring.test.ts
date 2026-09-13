/**
 * Regression coverage for the Linux blacklist picker UI wiring.
 *
 * App.svelte mounts PrivacyModal.svelte. Keeping this invariant at source
 * level prevents the catalog flow from silently returning to SettingsPanel,
 * which is not part of the active Privacy surface.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(process.cwd(), "src", ...segments), "utf8");
}

test("the active Privacy surface owns the Linux picker flow", () => {
  const app = loadSource("App.svelte");
  const privacy = loadSource("PrivacyModal.svelte");
  const settingsPanel = loadSource("SettingsPanel.svelte");

  assert.match(app, /import\s+PrivacyModal\s+from\s+"\.\/PrivacyModal\.svelte"/);
  assert.match(app, /<PrivacyModal\s*\/?>(?:<\/PrivacyModal>)?/);
  assert.match(privacy, /ignoredAppLinuxCatalogCommand/);
  assert.match(privacy, /ignoredAppLinuxAddCommand/);
  assert.match(privacy, /data-testid="linux-picker-modal"/);
  assert.match(privacy, /data-testid="linux-picker-cancel"/);

  const pickAndAdd = privacy.match(
    /async function pickAndAddIgnored\(\)[\s\S]*?\n  \}/,
  )?.[0];
  assert.ok(pickAndAdd, "PrivacyModal must define pickAndAddIgnored");
  const catalogCall = pickAndAdd.indexOf("loadLinuxCatalog()");
  const legacyCall = pickAndAdd.indexOf("ignoredAppPickAndAddCommand()");
  assert.ok(catalogCall >= 0, "PrivacyModal must call the Linux catalog");
  assert.ok(legacyCall >= 0, "PrivacyModal must retain the legacy fallback");
  assert.ok(
    catalogCall < legacyCall,
    "the Linux catalog must be attempted before the legacy picker",
  );
  assert.match(
    pickAndAdd,
    /if \(linuxCatalog\)[\s\S]*?describeLinuxCatalogUnavailable\(linuxCatalog\.reason\)/,
    "an explicit Linux catalog rejection must not be masked by the legacy picker",
  );
  assert.doesNotMatch(settingsPanel, /ignoredAppLinux(?:Add|Catalog)Command/);
  assert.doesNotMatch(settingsPanel, /linux-picker-modal/);
});
