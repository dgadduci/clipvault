import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { createCollectionPreviewLoadBarrier } from "../src/lib/collectionPreviewLoad.ts";

const frontendRoot = process.cwd();

function source(...segments: string[]): string {
  return readFileSync(path.join(frontendRoot, ...segments), "utf8");
}

test("preview barrier settles after every visible image completes", async () => {
  const barrier = createCollectionPreviewLoadBarrier([7, 8, 8, 0, -1]);
  let settled = false;
  void barrier.promise.then(() => {
    settled = true;
  });

  barrier.markReady(7);
  await Promise.resolve();
  assert.equal(settled, false);
  assert.equal(barrier.pendingCount, 1);

  barrier.markReady(8);
  await barrier.promise;
  assert.equal(settled, true);
  assert.equal(barrier.pendingCount, 0);
});

test("preview barrier ignores hidden entries and can be cancelled", async () => {
  const filtered = createCollectionPreviewLoadBarrier([2, 3]);
  filtered.retainVisible([3]);
  assert.equal(filtered.pendingCount, 1);
  filtered.markReady(3);
  await filtered.promise;
  assert.equal(filtered.pendingCount, 0);

  const cancelled = createCollectionPreviewLoadBarrier([4]);
  cancelled.cancel();
  await cancelled.promise;
  assert.equal(cancelled.pendingCount, 0);
});

test("collection refresh reveals new cards before metadata hydration completes", () => {
  const app = source("src", "App.svelte");
  const refresh = app.match(
    /async function refreshEntries\(\): Promise<void> \{[\s\S]*?\n  \}/,
  );
  assert.ok(refresh, "the shared collection refresh must remain present");
  assert.match(refresh[0], /visibleEntries = entries;[\s\S]*?clearingOldCollectionContent = false;[\s\S]*?await waitForPreviewRailPaint\(\);[\s\S]*?hydrateEntryOrganization\(entries,\s*\{\s*progressive: collectionSwitchInteractionLocked/);
  assert.match(app, /PROGRESSIVE_ENTRY_HYDRATION_CONCURRENCY = 8/);
  const progressiveHydration = app.match(
    /if \(options\.progressive\) \{[\s\S]*?return;\n    \}/,
  );
  assert.ok(progressiveHydration, "collection metadata must hydrate progressively");
  assert.match(progressiveHydration[0], /while \(nextEntryIndex < pendingEntries\.length\)/);
  assert.match(progressiveHydration[0], /applyResults\(\[result\]\)/);
  assert.match(app, /function waitForPreviewRailPaint\(\)[\s\S]*?requestAnimationFrame\([\s\S]*?requestAnimationFrame/);
  assert.match(app, /if \(searchQuery\.trim\(\)\.length > 0\)/);
  assert.match(app, /inert=\{collectionSwitchInteractionLocked\}/);
  assert.match(app, /collectionContentLoading \|\| collectionSwitchInteractionLocked/);
  const select = app.match(
    /async function selectCollectionById\([\s\S]*?\n  \}/,
  );
  assert.ok(select, "collection selection must remain centralized");
  assert.match(select[0], /collectionSwitchInteractionLocked = true;[\s\S]*?await refreshEntries\(\)/);
  assert.match(app, /blockCollectionSwitchKeyboard/);
  assert.match(app, /testId="collection-content-loading"/);
  assert.match(app, /data-testid="collection-content-loading-status"/);
  const card = source("src", "HistoryCard.svelte");
  const rail = source("src", "HistoryCardRail.svelte");
  assert.match(card, /onPreviewReady\(entryId, batchId\)/);
  assert.match(card, /on:load=\{onThumbnailLoad\}/);
  assert.match(rail, /\{onPreviewReady\}/);
});
