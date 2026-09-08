/**
 * Behaviour-level coverage for the tag-filter one-event delay the
 * manual QA pass detected on Desktop.
 *
 * The bug surfaced because `App.svelte` exposed the tag id list as a
 * reactive `$:` declaration, which Svelte flushes on the next
 * microtask after the synchronous portion of an event handler. The
 * `loadEntries` / `performSearch` calls therefore captured the
 * *previous* `tagFilter` value, so the visible combobox selection and
 * the rail/search response drifted by exactly one event. Selecting
 * `tag-a` filtered by no tag; the next selection `tag-b` was the first
 * one that actually filtered — by `tag-a`.
 *
 * The fix replaces the reactive declaration with a `currentTagFilterIds()`
 * function call. The tests in this file pin the new contract through
 * pure-helper calls and source-level greps so a regression that
 * re-introduces a stale `tagFilterIds` declaration (or any other
 * indirection between the toolbar selection and the backend call)
 * surfaces here before it reaches the user.
 *
 * Privacy invariants are also pinned: the helper never carries
 * clipboard content, hashes, asset references, source-app
 * identifiers, paths or byte counts.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

const FRONTEND_ROOT = resolvePath(process.cwd());

function readSource(relative: string): string {
  return readFileSync(resolvePath(FRONTEND_ROOT, relative), "utf8");
}

const appSource = readSource("src/App.svelte");
const tagFilterSource = readSource("src/TagFilter.svelte");
const typesSource = readSource("src/types.ts");

function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
}

const cleanAppSource = stripComments(appSource);

// ---------------------------------------------------------------------------
// 10.4 Tag-filter contract: the new selection is the source of truth.
// ---------------------------------------------------------------------------

test("App exposes a synchronous tag-id helper, not a reactive declaration", () => {
  // The fix MUST surface a `currentTagFilterIds()` function so the
  // backend call captures the live `tagFilter` value at call time.
  // A regression that re-introduces a `$:` declaration re-creates
  // the one-event delay.
  assert.ok(
    /function\s+currentTagFilterIds\s*\(/.test(cleanAppSource),
    "App.svelte must declare currentTagFilterIds() so the live tag ids are computed at call time",
  );
  assert.ok(
    /currentTagFilterIds\(\)/.test(cleanAppSource),
    "App.svelte must call currentTagFilterIds() at least once",
  );
  // The reactive `$:` declaration MUST NOT exist anymore; otherwise
  // a future contributor could re-introduce the lag by reading
  // `tagFilterIds` instead of calling `currentTagFilterIds()`.
  assert.equal(
    /\$\:\s*tagFilterIds\s*=/.test(cleanAppSource),
    false,
    "App.svelte must not declare tagFilterIds as a reactive declaration",
  );
  // `tagFilterIds` (without parentheses) MUST NOT appear at a call
  // site: every backend call MUST route through the helper.
  const callSite = cleanAppSource.match(/tagIds:\s*[A-Za-z_]+/g) ?? [];
  for (const site of callSite) {
    assert.equal(
      /tagFilterIds\b(?!\s*\()/.test(site),
      false,
      `App.svelte must not read tagFilterIds directly at "${site}"`,
    );
  }
});

test("loadEntries forwards the live tag ids, not a stale reactive value", () => {
  // The recents request MUST call `currentTagFilterIds()` so the
  // backend receives the tag the user just selected in the same
  // event. Reading a stale reactive declaration would silently
  // filter by the previous selection.
  const loadEntriesMatch = cleanAppSource.match(
    /async function loadEntries\(\)[\s\S]*?\n  \}/,
  );
  assert.ok(loadEntriesMatch, "loadEntries must exist");
  assert.ok(
    /tagIds:\s*currentTagFilterIds\(\)/.test(loadEntriesMatch![0]),
    "loadEntries must forward tagIds: currentTagFilterIds()",
  );
  // Sanity check: the source-app filter is still passed alongside
  // the tag filter so the AND combination stays intact.
  assert.ok(
    /sourceApp:\s*sourceAppFilter/.test(loadEntriesMatch![0]),
    "loadEntries must keep forwarding sourceApp: sourceAppFilter",
  );
});

test("performSearch forwards the live tag ids, not a stale reactive value", () => {
  // The search request MUST also call `currentTagFilterIds()` so a
  // tag selection while a search query is active updates the
  // results immediately. The previous reactive declaration lagged
  // on this path too.
  const performSearchMatch = cleanAppSource.match(
    /async function performSearch\([\s\S]*?return response;\n      \},/,
  );
  assert.ok(performSearchMatch, "performSearch must exist");
  assert.ok(
    /tagIds:\s*currentTagFilterIds\(\)/.test(performSearchMatch![0]),
    "performSearch must forward tagIds: currentTagFilterIds()",
  );
});

test("handleTagFilterChange does not compute the new filter from the previous state", () => {
  // The handler MUST assign `tagFilter = next` synchronously and
  // forward to `refreshEntries()`. A regression that computed the
  // new ids from the OLD `tagFilter` (e.g. inside a closure that
  // captured the previous event) would re-introduce the lag.
  const handleMatch = cleanAppSource.match(
    /async function handleTagFilterChange[\s\S]*?\n  \}/,
  );
  assert.ok(handleMatch, "handleTagFilterChange must exist");
  const body = handleMatch![0];
  // The new selection is the source of truth: `tagFilter = next`
  // must precede `refreshEntries()`.
  const assignIndex = body.indexOf("tagFilter = next");
  const refreshIndex = body.indexOf("refreshEntries");
  assert.ok(
    assignIndex > -1 && refreshIndex > -1,
    "handleTagFilterChange must assign tagFilter and call refreshEntries",
  );
  assert.ok(
    assignIndex < refreshIndex,
    "tagFilter must be assigned BEFORE refreshEntries so the new value reaches the backend in the same event",
  );
  // The short-circuit branch (no-op when the new selection equals
  // the current one) MUST NOT trigger when the selection actually
  // changed. A regression that always returned early would silently
  // disable the filter.
  const guardIndex = body.indexOf("if (sameKind");
  assert.ok(guardIndex > -1, "handleTagFilterChange must keep the no-op short-circuit");
  assert.ok(
    guardIndex < assignIndex,
    "the no-op short-circuit must run BEFORE the assignment so the new tag reaches the backend in the same event",
  );
});

test("the toolbar forwards the new selection through the documented onChange contract", () => {
  // The combobox MUST emit `onChange(next)` synchronously so the
  // parent receives the new value inside the same event. A
  // regression that wraps the call in `setTimeout`, `queueMicrotask`
  // or a debounced promise would re-introduce the lag.
  const selectKeyMatch = tagFilterSource.match(
    /function\s+selectKey\s*\(key:\s*string\):\s*void\s*\{[\s\S]*?\n  \}/,
  );
  assert.ok(selectKeyMatch, "TagFilter.selectKey must exist");
  assert.ok(
    /onChange\(next\)/.test(selectKeyMatch![0]),
    "TagFilter.selectKey must call onChange(next) synchronously",
  );
  // The combobox MUST close only AFTER the parent has been
  // notified — otherwise the parent's `tagFilter = next` runs in a
  // microtask and the backend call could capture the stale value.
  const onChangeIndex = selectKeyMatch![0].indexOf("onChange(next)");
  const closeIndex = selectKeyMatch![0].indexOf("closeCombobox");
  assert.ok(
    onChangeIndex > -1 && closeIndex > -1,
    "the selectKey helper must call onChange and closeCombobox",
  );
  assert.ok(
    onChangeIndex < closeIndex,
    "onChange(next) must run BEFORE closeCombobox so the new tag reaches the parent in the same event",
  );
});

test("App wires the tag filter through the synchronous onChange callback", () => {
  // The toolbar MUST receive `onTagFilterChange` directly (no
  // `await` wrapping, no debounce) so the parent can synchronously
  // forward the new selection to the backend.
  assert.ok(
    /onTagFilterChange=\{\(next\) => handleTagFilterChange\(next\)\}/.test(
      appSource,
    ),
    "App.svelte must wire onTagFilterChange to handleTagFilterChange directly",
  );
});

test("the AND combination with sourceAppFilter stays intact on the same event", () => {
  // Selecting a tag while a source-app filter is active MUST
  // forward both facets to the backend inside the same call. The
  // fix MUST NOT regress the AND combination.
  const loadEntriesMatch = cleanAppSource.match(
    /async function loadEntries\(\)[\s\S]*?\n  \}/,
  );
  assert.ok(loadEntriesMatch, "loadEntries must exist");
  assert.ok(
    /tagIds:\s*currentTagFilterIds\(\)/.test(loadEntriesMatch![0]) &&
      /sourceApp:\s*sourceAppFilter/.test(loadEntriesMatch![0]),
    "loadEntries must forward both the live tag ids and the source-app filter",
  );
});

test("the collection switch resets the tag filter without re-using the previous selection", () => {
  // Selecting a new collection while a tag filter is active MUST
  // collapse back to `Todas` so the rail does not silently hide
  // every entry. The handler MUST read the LIVE `tagFilter` so the
  // reset decision uses the current selection.
  const selectMatch = cleanAppSource.match(
    /async function selectCollectionFromSidebar[\s\S]*?await refreshEntries\(\);/,
  );
  assert.ok(selectMatch, "selectCollectionFromSidebar must exist");
  // The contract: if the active tag is "tag", reset to "all" so the
  // new collection does not silently filter to an empty set.
  assert.ok(
    /if\s*\(tagFilter\.kind\s*===\s*"tag"\)/.test(selectMatch![0]),
    "selectCollectionFromSidebar must check tagFilter.kind === 'tag'",
  );
  assert.ok(
    /tagFilter\s*=\s*\{\s*kind:\s*"all"\s*\}/.test(selectMatch![0]),
    "selectCollectionFromSidebar must reset tagFilter to { kind: 'all' } when a tag is active",
  );
});

test("refresh entries path keeps a single tag filter write without stale listeners", () => {
  // The `refresh()` / `handleHistoryUpdated` flow MUST NOT install
  // duplicate event listeners. A regression that re-registered
  // the toolbar callback would compound the lag: each emit would
  // re-issue a backend call with the previously flushed
  // `tagFilterIds`.
  const handleHistoryUpdatedMatch = cleanAppSource.match(
    /async function handleHistoryUpdated[\s\S]*?\n  \}/,
  );
  assert.ok(handleHistoryUpdatedMatch, "handleHistoryUpdated must exist");
  // The handler MUST only call helpers that re-run the request
  // through the same `refreshEntries()` path; it MUST NOT install a
  // second `onTagFilterChange` listener.
  assert.equal(
    /addEventListener/.test(handleHistoryUpdatedMatch![0]),
    false,
    "handleHistoryUpdated must not register a second event listener",
  );
  assert.ok(
    /refreshEntries\(\)/.test(handleHistoryUpdatedMatch![0]),
    "handleHistoryUpdated must call refreshEntries() to re-issue the request",
  );
});

test("the TagFilter combobox exposes its selection through the stable testid contract", () => {
  // The combobox's `data-tag-filter-selected` attribute MUST keep
  // its documented selector so the regression suite can assert the
  // visible selection. A regression that drops the selector would
  // silently disable the combobox's accessibility surface.
  assert.ok(
    /data-tag-filter-selected=\{selected\.kind\}/.test(tagFilterSource),
    "TagFilter must expose data-tag-filter-selected on the trigger",
  );
  // The combobox MUST keep the `Todas` sentinel so the default
  // view reproduces the pre-tag-filter rail byte-for-byte.
  assert.ok(
    /display_name:\s*"Todas"/.test(tagFilterSource),
    "TagFilter must synthesize the Todas sentinel",
  );
});

test("the TagFilter union keeps the All + tag discriminator", () => {
  // The discriminated union MUST keep the `all` and `tag` branches
  // so the parent's equality check (`sameKind && sameTag`) stays
  // exhaustive. A regression that collapses the union would
  // silently disable the no-op short-circuit.
  const tagFilterMatch = typesSource.match(/export type TagFilter[\s\S]*?;/);
  assert.ok(tagFilterMatch, "the TagFilter type must be defined");
  assert.ok(
    /kind:\s*"all"/.test(tagFilterMatch![0]),
    "the union must keep the All sentinel",
  );
  assert.ok(
    /kind:\s*"tag"/.test(tagFilterMatch![0]),
    "the union must keep the tag branch",
  );
});

test("the tag-filter helper never echoes clipboard content, hashes, asset refs or paths", () => {
  // Privacy invariant: the synchronous tag-id helper MUST NOT
  // surface clipboard content, hashes, asset references, source-app
  // identifiers, paths or byte counts. A regression that forwards
  // an entry payload through `currentTagFilterIds()` would break
  // the spec's privacy contract.
  const sensitive = [
    "entry.content",
    "asset_ref",
    "content_hash",
    "rich_html_ref",
    "rich_rtf_ref",
    "rich_preview_ref",
    "/Users/",
    "entry.source_app",
    "assetRef",
  ];
  const helperMatch = cleanAppSource.match(
    /function\s+currentTagFilterIds\s*\([\s\S]*?\n  \}/,
  );
  assert.ok(helperMatch, "currentTagFilterIds must exist");
  for (const token of sensitive) {
    assert.equal(
      helperMatch![0].includes(token),
      false,
      `currentTagFilterIds must not reference ${token}`,
    );
  }
});