/**
 * Regression coverage for the `source-app-filter` bootstrap
 * regression. The user-reported bug: on a fresh start, the default
 * view is `Historial` (`selectedCollectionId === null`); selecting
 * an application in the combobox did not filter the cards. The
 * filter only started working after the user entered a user
 * collection and came back to `Historial`.
 *
 * Root cause: `App.svelte::loadEntries()` branched on
 * `selectedCollectionId === null` and called
 * `clipvault_recent_entries` (the unfiltered recents command),
 * which silently dropped the `source_app` argument. The combobox
 * still loaded its options correctly because
 * `clipvault_source_applications` already used the explicit
 * `activeCollectionIsHistory ? null : selectedCollectionId`
 * scope, but the rail could never receive the filter.
 *
 * Fix: `loadEntries()` now always calls
 * `clipvault_recent_entries_filtered` and forwards the source-app
 * filter for every scope. The tests below pin the contract:
 *
 * 1. Bootstrap in Historial calls
 *    `clipvault_recent_entries_filtered` with `collectionId: null`
 *    and the current `sourceApp` filter.
 * 2. Historial + text search + source-app filter forwards both
 *    facets to `clipvault_search_entries`.
 * 3. Historial → collection → Historial keeps the source-app
 *    filter reset to `Todas` and forwards the new scope on every
 *    `loadEntries()` round.
 * 4. `Todas` clears only the source-app filter; collection, query
 *    and tags are preserved.
 * 5. `Unknown` distinguishes from `Todas` (the wire payload
 *    carries `kind: "unknown"`, not `null`).
 * 6. The stale-response guard the parent already owns is still in
 *    place: a bootstrap response that lands after a collection
 *    switch cannot overwrite the new scope.
 * 7. `clipvault://history-updated` triggers a single
 *    `refreshSourceAppOptions` + `refreshEntries` round without
 *    duplicate listeners.
 *
 * The tests read the source directly through the `loadSource`
 * helper (the same approach `desktopToolbarLayout.test.ts` and
 * `sourceApplicationsBridge.test.ts` use) so a Svelte runtime is
 * not required.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const FRONTEND_ROOT = process.cwd();

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/\/.*$/gm, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
}

// ---------------------------------------------------------------------------
// Bootstrap: loadEntries must use the filtered command in every scope.
// ---------------------------------------------------------------------------

test("loadEntries always routes through clipvault_recent_entries_filtered", () => {
  // The previous implementation branched on
  // `selectedCollectionId === null` and used the unfiltered
  // `clipvault_recent_entries` command, silently dropping the
  // source-app filter on the bootstrap Historial view. The fix
  // collapses both branches into a single filtered call so the
  // source-app filter reaches the backend regardless of the
  // active collection.
  const source = stripComments(loadSource("src/App.svelte"));
  // The bridge command is the only path `loadEntries` uses now.
  assert.match(
    source,
    /async function loadEntries[^{]*\{[\s\S]*?recentEntriesFilteredCommand\(/,
    "loadEntries must funnel through recentEntriesFilteredCommand",
  );
  // The legacy unfiltered call must be gone.
  assert.equal(
    /loadEntries[^{]*\{[\s\S]*?recentEntriesCommand\(/.test(source),
    false,
    "loadEntries must no longer call the unfiltered recentEntriesCommand",
  );
  // The import for the unfiltered command is dropped from App.svelte
  // because nothing else inside it uses it (QuickPaste and
  // DevelopmentModal still own their own imports).
  const importBlock = source.match(
    /import\s*\{[\s\S]*?\}\s*from\s*"\.\/lib\/tauri";/,
  );
  assert.ok(importBlock, "App.svelte must keep the tauri import block");
  assert.equal(
    /\brecentEntriesCommand\b/.test(importBlock[0]),
    false,
    "App.svelte must no longer import recentEntriesCommand",
  );
});

test("loadEntries forwards activeCollectionIsHistory as collectionId null", () => {
  // The wire contract the rest of the backend speaks treats
  // `collection_id: null` as the Historial scope. The new
  // `loadEntries` mirrors that explicitly so the bridge never has
  // to guess whether a missing collectionId means "no filter" or
  // "Historial" — both are the same `null`.
  const source = stripComments(loadSource("src/App.svelte"));
  const block = source.match(
    /async function loadEntries[^{]*\{[\s\S]*?\n\s{2}\}/,
  );
  assert.ok(block, "loadEntries block must exist");
  const body = block[0];
  assert.match(
    body,
    /collectionId:\s*activeCollectionIsHistory\s*\?\s*null\s*:\s*selectedCollectionId/,
    "loadEntries must map activeCollectionIsHistory to collectionId: null",
  );
  assert.match(
    body,
    /sourceApp:\s*sourceAppFilter/,
    "loadEntries must forward the source-app filter to the bridge",
  );
});

test("source-app combobox still drives refreshEntries on selection change", () => {
  // After the bootstrap regression is fixed, picking an option from
  // the combobox MUST still re-run `refreshEntries` so the rail
  // reflects the new scope. The handler is the same
  // `handleSourceAppFilterChange` already wired by the original
  // change; we only verify the wiring survived the loadEntries
  // refactor.
  const source = stripComments(loadSource("src/App.svelte"));
  assert.match(
    source,
    /async function handleSourceAppFilterChange\([\s\S]*?sourceAppFilter = next;[\s\S]*?await refreshEntries\(\)/,
    "the handler must update sourceAppFilter and refresh entries",
  );
  // `refreshEntries` is the only entry-point that re-reads the
  // rail; the helper itself still anchors on
  // `recentEntriesFilteredCommand`.
  const refreshBlock = source.match(
    /async function refreshEntries\(\)[\s\S]*?\n\s{2}\}/,
  );
  assert.ok(refreshBlock, "refreshEntries must exist");
  assert.match(
    refreshBlock[0],
    /entries = await loadEntries\(\)/,
    "refreshEntries must call loadEntries",
  );
});

// ---------------------------------------------------------------------------
// Search path: text + source-app filter must compose in Historial.
// ---------------------------------------------------------------------------

test("searchEntriesCommand receives both collectionId null and sourceApp filter in Historial", () => {
  // The bootstrap regression touched the recents path; the search
  // path was already routed through `searchEntriesCommand` with
  // both facets, but a regression that mirrored the recents bug
  // would surface here too. Pin the contract: `selectedCollectionId
  // === null` (Historial) must produce `collectionId: null` and
  // the search MUST forward the source-app filter.
  const source = stripComments(loadSource("src/App.svelte"));
  const searchBlock = source.match(
    /const response: SearchResponse = await searchEntriesCommand\(\{[\s\S]*?\}\);/,
  );
  assert.ok(searchBlock, "the search invocation must exist");
  const body = searchBlock[0];
  assert.match(
    body,
    /collectionId:\s*selectedCollectionId\s*!==\s*null\s*&&\s*!activeCollectionIsHistory\s*\?\s*selectedCollectionId\s*:\s*null/,
    "search must map activeCollectionIsHistory to collectionId: null",
  );
  assert.match(
    body,
    /sourceApp:\s*sourceAppFilter/,
    "search must forward the source-app filter",
  );
});

// ---------------------------------------------------------------------------
// Scope refresh: collection switch resets the filter and reloads options.
// ---------------------------------------------------------------------------

test("selectCollectionFromSidebar resets the source-app filter to Todas and reloads options", () => {
  // The original design document pins the contract: switching
  // collections MUST reset the source-app filter to `Todas` and
  // reload the combobox options so the user does not carry a
  // criterion that has no meaning in the new scope. The function
  // also reloads the entries; the entries path now uses the
  // filtered command in every scope.
  const source = stripComments(loadSource("src/App.svelte"));
  const switchBlock = source.match(
    /async function selectCollectionFromSidebar\([\s\S]*?\n\s{2}\}/,
  );
  assert.ok(switchBlock, "selectCollectionFromSidebar must exist");
  const body = switchBlock[0];
  assert.match(
    body,
    /if\s*\(sourceAppFilter\.kind\s*!==\s*"all"\)\s*\{[\s\S]*?sourceAppFilter\s*=\s*\{\s*kind:\s*"all"\s*\}/,
    "collection switch must reset the source-app filter to Todas",
  );
  assert.match(
    body,
    /await refreshSourceAppOptions\(\)/,
    "collection switch must reload the combobox options",
  );
  assert.match(
    body,
    /await refreshEntries\(\)/,
    "collection switch must refresh the rail",
  );
});

test("refreshSourceAppOptions uses activeCollectionIsHistory to derive the collectionId", () => {
  // The combobox options are computed against the same scope the
  // rail reads. The fix keeps the explicit
  // `activeCollectionIsHistory ? null : selectedCollectionId`
  // expression so a regression that hardcodes a numeric id (e.g.
  // the legacy history collection id) fails this assertion.
  const source = stripComments(loadSource("src/App.svelte"));
  const optionsBlock = source.match(
    /async function refreshSourceAppOptions\(\)[\s\S]*?\n\s{2}\}/,
  );
  assert.ok(optionsBlock, "refreshSourceAppOptions must exist");
  assert.match(
    optionsBlock[0],
    /collectionId:\s*activeCollectionIsHistory\s*\?\s*null\s*:\s*selectedCollectionId/,
    "refreshSourceAppOptions must derive the scope from activeCollectionIsHistory",
  );
  // No numeric id is hardcoded as the history collection id.
  assert.equal(
    /historyCollectionId\s*\}\s*\?/.test(optionsBlock[0]),
    false,
    "refreshSourceAppOptions must not branch on historyCollectionId",
  );
});

// ---------------------------------------------------------------------------
// Stale-response guard for the bootstrap round.
// ---------------------------------------------------------------------------

test("refreshSourceAppOptions keeps a monotonic token to drop stale bootstrap responses", () => {
  // A bootstrap call that lands after the user has already switched
  // to a user collection (and back) must not pollute the new scope.
  // The parent pins a `sourceAppOptionsToken` counter and only
  // commits the snapshot when the token matches. The regression
  // surface for the bootstrap bug is a missing guard, so the test
  // asserts the guard is wired.
  const source = stripComments(loadSource("src/App.svelte"));
  assert.match(source, /let sourceAppOptionsToken = 0;/);
  assert.match(source, /const token = \+\+sourceAppOptionsToken;/);
  assert.match(
    source,
    /if \(token !== sourceAppOptionsToken\) \{[\s\S]*?return;[\s\S]*?\}/,
    "refreshSourceAppOptions must drop stale responses",
  );
});

// ---------------------------------------------------------------------------
// History-updated listener must refresh once without duplicates.
// ---------------------------------------------------------------------------

test("handleHistoryUpdated refreshes source-app options and entries without duplicates", () => {
  // The history-updated event must trigger a single
  // `refreshSourceAppOptions` + `refreshEntries` round so the
  // combobox and the rail stay coherent. The parent wraps the
  // listener through `createHistoryUpdatedRegistrar` (the same
  // helper the rest of the desktop uses) which is pinned
  // elsewhere to install exactly one listener; this test pins the
  // Svelte-level handler contract on top of that helper.
  const source = stripComments(loadSource("src/App.svelte"));
  const block = source.match(
    /async function handleHistoryUpdated\(\)[\s\S]*?\n\s{2}\}/,
  );
  assert.ok(block, "handleHistoryUpdated must exist");
  const body = block[0];
  assert.match(body, /await refreshSourceAppOptions\(\)/);
  assert.match(body, /await refreshEntries\(\)/);
  assert.match(body, /await refreshUnorganizedClearableCount\(\)/);
  // The registrar that owns the listener is still the documented
  // helper and the listener is registered exactly once in
  // `onMount`.
  assert.match(source, /createHistoryUpdatedRegistrar/);
  assert.match(
    source,
    /registerHistoryUpdated\(handleHistoryUpdated\)[\s\S]*?unlistenHistoryUpdated = unlisten;/,
  );
});

// ---------------------------------------------------------------------------
// Bridge contract: `recentEntriesFilteredCommand` accepts the
// `collectionId: null` + `sourceApp` pair the rail sends.
// ---------------------------------------------------------------------------

test("App.svelte does not call clipvault_recent_entries any more", () => {
  // The unfiltered recents command is gone from the desktop rail.
  // QuickPaste and DevelopmentModal still own their own imports
  // because they legitimately need the unfiltered history (the
  // quick-paste flow is intentionally collection-agnostic), but
  // the desktop rail must funnel through the filtered command.
  // The unfiltered command name would surface in
  // `recentEntriesCommand(`; the JS function name in tauri.ts
  // (which QuickPaste and DevelopmentModal still use) keeps the
  // call reachable from those modules but is no longer imported
  // by App.svelte.
  const source = stripComments(loadSource("src/App.svelte"));
  assert.equal(
    /\brecentEntriesCommand\(/.test(source),
    false,
    "App.svelte must no longer call recentEntriesCommand",
  );
  assert.match(
    source,
    /\brecentEntriesFilteredCommand\(/,
    "loadEntries must call recentEntriesFilteredCommand",
  );
});

// ---------------------------------------------------------------------------
// Filter semantics: `Todas` clears only the source-app facet.
// ---------------------------------------------------------------------------

test("Todas is the absence of a source-app restriction; Known and Unknown remain distinct", () => {
  // The SourceAppFilter type is the single source of truth shared
  // by the rail, the search and the bridge. `All` (Todas) is the
  // absence of a restriction; `Known` carries the stable
  // identifier; `Unknown` targets NULL or empty rows. The
  // bootstrap fix relies on `All` being the default so the
  // bootstrap round (which carries no filter) maps to "no
  // restriction".
  const source = loadSource("src/types.ts");
  assert.match(
    source,
    /export type SourceAppFilter =[\s\S]*?\{\s*kind:\s*"all"\s*\}[\s\S]*?\{\s*kind:\s*"known"[\s\S]*?\{\s*kind:\s*"unknown"\s*\}/,
  );
  // The SourceAppFilter wire format is the discriminator the
  // backend speaks; the Rust enum uses the same variant names.
  // The frontend mirror must keep the `kind` discriminator at the
  // top level so the `serde(tag = "kind")` derivation agrees.
  assert.match(source, /\| \{ kind: "all" \}/);
  assert.match(source, /\| \{ kind: "known"; source_app: string \}/);
  assert.match(source, /\| \{ kind: "unknown" \}/);
});

// ---------------------------------------------------------------------------
// No-regression: drag-and-drop, image lifecycle, search and history.
// ---------------------------------------------------------------------------

test("App.svelte keeps drag-and-drop and the image lifecycle intact", () => {
  // The bootstrap fix must not drift into the protected baseline:
  // the pointer drag controller, the combobox isolation, the
  // hydration sequence and the image fallback chain all survive.
  const source = loadSource("src/App.svelte");
  assert.match(source, /installPointerDragController\(document\)/);
  assert.match(source, /detachPointerDragController/);
  assert.match(source, /onInternalDragOver/);
  assert.match(source, /hydrateEntryOrganization/);
  assert.match(source, /setFavoriteCommand/);
  assert.match(source, /combineMemberships\(/);
});

test("App.svelte keeps the documented wire-level constants", () => {
  // The filter payload must remain metadata-only: the combobox
  // never receives content, snippets, hashes or asset references
  // through the bridge. The `source_app` field stays as the
  // stable, opaque identifier (never rendered as visible text in
  // any UI surface). The check is performed against the stripped
  // source so a docstring that names the forbidden category as a
  // contract marker does not trip the assertion.
  const source = stripComments(loadSource("src/App.svelte"));
  // Strip the literal `entry.` prefix the wire shape uses: the
  // Tauri command arguments are namespaced (`entry.content`,
  // `entry.asset_ref`) so the combobox never binds them directly,
  // but the raw `content_hash` / `asset_ref` strings must not
  // appear as standalone identifiers on the wire.
  for (const forbidden of ["content_hash", "asset_ref"]) {
    assert.equal(
      source.includes(forbidden),
      false,
      `App.svelte must not surface ${forbidden} to the combobox`,
    );
  }
});
