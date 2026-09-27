import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const QUICK_PASTE = readFileSync(
  resolve(process.cwd(), "src/QuickPaste.svelte"),
  "utf8",
);
const FALLBACK = readFileSync(
  resolve(process.cwd(), "src/lib/sourceAppFallback.ts"),
  "utf8",
);

test("QuickVault batches local imported-attribution lookup without delaying rows", () => {
  const projection = QUICK_PASTE.match(
    /async function refreshPeerImportedSourceApps\([\s\S]*?\n  \}/,
  )?.[0] ?? "";
  assert.match(projection, /peerImportSourceAppPresentationsCommand/);
  assert.match(projection, /collection_id: null/);
  assert.match(projection, /\.slice\(0, 100\)/);
  assert.match(projection, /token !== peerImportedSourceAppsToken/);
  assert.doesNotMatch(projection, /await .*loadRecent|await .*runQuery/);

  assert.match(QUICK_PASTE, /void refreshPeerImportedSourceApps\(recent\)/);
  assert.match(
    QUICK_PASTE,
    /void refreshPeerImportedSourceApps\([\s\S]*?response\.hits\.map\(\(hit\) => hit\.record\)/,
  );
});

test("QuickVault resolves imported icons and exposes the name as a tag only", () => {
  assert.match(
    QUICK_PASTE,
    /sourceAppPresentationIconRef\(entry, imported\)/,
  );
  assert.match(
    QUICK_PASTE,
    /sourceAppPresentationAccessibleLabel\(entry, importedSourceApp\)/,
  );
  assert.match(QUICK_PASTE, /title=\{sourceAppLabel\}/);
  assert.match(QUICK_PASTE, /aria-label=\{sourceAppLabel\}/);
  assert.match(QUICK_PASTE, /IMPORTED_SOURCE_APP_FALLBACK_ICON_SVG/);
  assert.doesNotMatch(QUICK_PASTE, /quick-paste-imported-source-app-name/);
  assert.match(FALLBACK, /export function sourceAppPresentationIconRef/);
  assert.match(FALLBACK, /export function sourceAppPresentationAccessibleLabel/);
});

test("QuickVault discards stale icon work and releases cached Object URLs on teardown", () => {
  assert.match(QUICK_PASTE, /appIconTokens\.get\(entry\.id\) !== token/);
  assert.match(QUICK_PASTE, /appIconRefs\.get\(entry\.id\) !== ref/);
  assert.match(QUICK_PASTE, /appIconResolver\.release\(\)/);
  assert.match(QUICK_PASTE, /quickPasteDestroyed = true/);
  assert.match(QUICK_PASTE, /if \(quickPasteDestroyed\) appIconResolver\.releaseFor\(ref\)/);
});
