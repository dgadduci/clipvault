/**
 * Regression coverage for the Linux blacklist picker UI wiring.
 *
 * App.svelte mounts PrivacyModal.svelte. Keeping this invariant at source
 * level prevents the catalog flow from silently moving to a different
 * surface, which is not part of the active Privacy modal.
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
});

test("the Linux picker resolves catalog icons via the application-icons bridge", () => {
  // The picker candidates live in the `application-icons/` namespace
  // (the metadata provider writes the rasterised PNG there). The
  // picker MUST route icon lookups through `sourceAppIconCommand`,
  // not `ignoredAppIconCommand`, because the latter only accepts the
  // `ignored-apps/` namespace and would silently fall back to the
  // initial-letter render.
  const privacy = loadSource("PrivacyModal.svelte");
  assert.match(
    privacy,
    /sourceAppIconCommand/,
    "PrivacyModal must import sourceAppIconCommand for the picker icons",
  );
  assert.match(
    privacy,
    /linuxPickerIconLoader/,
    "PrivacyModal must build a dedicated loader for the picker icons",
  );
  assert.match(
    privacy,
    /createIconResolver\(linuxPickerIconLoader\)/,
    "the picker icon resolver must consume the application-icons loader",
  );
  // The blacklist panel keeps the existing `ignoredAppIconCommand`
  // loader so persisted rows still resolve under the `ignored-apps/`
  // namespace. The two resolvers MUST stay distinct.
  assert.match(
    privacy,
    /createIconResolver\(tauriIconLoader\)/,
    "the blacklist resolver must continue to use the ignored-apps loader",
  );
  assert.doesNotMatch(
    privacy,
    /createIconResolver\(linuxPickerIconLoader\)[\s\S]*?ignoredAppIconCommand/,
    "the picker resolver MUST NOT route through ignoredAppIconCommand",
  );
});

test("the Linux picker add command only forwards the opaque identifier", () => {
  // The picker payload MUST contain only the opaque identifier; the
  // backend re-resolves the catalog and reuses its own `display_name`
  // and `icon_ref` instead of trusting the frontend. Sending the
  // metadata would leak the application name and icon reference
  // through the IPC bridge for no behavioural benefit.
  const privacy = loadSource("PrivacyModal.svelte");
  const confirm = privacy.match(
    /async function confirmLinuxPick\([\s\S]*?\n  \}/,
  )?.[0];
  assert.ok(confirm, "PrivacyModal must define confirmLinuxPick");
  assert.match(
    confirm,
    /ignoredAppLinuxAddCommand\(\{\s*identifier:\s*candidate\.identifier\s*,?\s*\}\)/,
    "confirmLinuxPick MUST only forward the opaque identifier",
  );
  assert.doesNotMatch(
    confirm,
    /displayName:\s*candidate\.display_name/,
    "the picker MUST NOT echo display_name into the add payload",
  );
  assert.doesNotMatch(
    confirm,
    /iconRef:\s*candidate\.icon_ref/,
    "the picker MUST NOT echo icon_ref into the add payload",
  );
});
