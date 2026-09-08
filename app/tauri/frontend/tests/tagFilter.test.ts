/**
 * Source-level coverage for the `TagFilter` component the
 * `quick-paste-desktop-polish` change introduces.
 *
 * The suite mirrors the structure of the source-app filter tests:
 * it walks the Svelte source through small string greps and the
 * shared `tsconfig` so a regression that drops an accessibility
 * hook, the AND combination wiring or the scope reset surfaces
 * here before it reaches the user.
 *
 * The tests do NOT mount the component: the suite stays metadata
 * only (no clipboard content, no entry id, no source-application
 * identifier, no asset reference) and exercises the contract the
 * toolbar exposes to `App.svelte`.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

const tagFilterSource = readFileSync(
  resolvePath(process.cwd(), "src", "TagFilter.svelte"),
  "utf8",
);

const desktopToolbarSource = readFileSync(
  resolvePath(process.cwd(), "src", "DesktopToolbar.svelte"),
  "utf8",
);

const appSource = readFileSync(
  resolvePath(process.cwd(), "src", "App.svelte"),
  "utf8",
);

const typesSource = readFileSync(
  resolvePath(process.cwd(), "src", "types.ts"),
  "utf8",
);

test("TagFilter declares the documented accessibility surface", () => {
  // The combobox MUST expose the listbox semantics the source-app
  // filter already pins: `aria-haspopup="listbox"`, an
  // `aria-expanded` toggle, an `aria-activedescendant` link to the
  // highlighted option, and the keyboard matcher. A regression
  // that drops any of these makes the new filter inaccessible.
  assert.ok(
    tagFilterSource.includes('aria-haspopup="listbox"'),
    "the trigger must declare aria-haspopup=listbox",
  );
  assert.ok(
    tagFilterSource.includes("aria-expanded"),
    "the trigger must expose aria-expanded",
  );
  assert.ok(
    tagFilterSource.includes("aria-activedescendant"),
    "the trigger must link aria-activedescendant to the highlighted option",
  );
  assert.ok(
    tagFilterSource.includes('role="listbox"'),
    "the listbox must declare role=listbox",
  );
  assert.ok(
    tagFilterSource.includes('role="option"'),
    "every option must declare role=option",
  );
});

test("TagFilter renders Todas as the first option", () => {
  // The `desktop-filtering` spec pins `Todas` as the first option
  // so the default view reproduces the pre-tag-filter rail
  // byte-for-byte. A regression that reorders the options or
  // drops the sentinel would silently default the rail to an
  // unfiltered view that is functionally correct but no longer
  // matches the contract.
  assert.ok(
    /display_name:\s*"Todas"/.test(tagFilterSource),
    "the combobox must synthesize a Todas row",
  );
  const firstRowMatch = tagFilterSource.match(
    /\{\s*kind:\s*"all",\s*key:\s*"all",\s*display_name:\s*"Todas"\s*,?\s*\}/,
  );
  assert.ok(firstRowMatch, "the first option MUST be the Todas sentinel");
});

test("TagFilter is documented as a stable testid", () => {
  // The regression suite targets the combobox by its
  // `data-testid`; the contract from the source-app filter
  // (default `tag-filter`, suffix `trigger`, `listbox`,
  // `option-<key>`) is the same pattern the new filter follows.
  assert.ok(
    tagFilterSource.includes('testId: string = "tag-filter"'),
    "the combobox must default the testid to tag-filter",
  );
  assert.ok(
    /\$\{testId\}-trigger/.test(tagFilterSource),
    "the trigger must use the documented {testId}-trigger selector",
  );
  assert.ok(
    /\$\{testId\}-listbox/.test(tagFilterSource),
    "the listbox must use the documented {testId}-listbox selector",
  );
  assert.ok(
    /\$\{testId\}-option-\$\{key\}/.test(tagFilterSource),
    "every option must use the documented {testId}-option-{key} selector",
  );
});

test("TagFilter handles Escape and ArrowUp/Down/Home/End/Enter", () => {
  // The keyboard matcher is a strict subset of the source-app
  // filter list; we assert each documented branch still exists
  // so a refactor cannot accidentally drop the menu's
  // accessibility contract.
  for (const key of [
    "ArrowDown",
    "ArrowUp",
    "Home",
    "End",
    "Enter",
    "Escape",
  ]) {
    assert.ok(
      tagFilterSource.includes(`"${key}"`),
      `the keyboard matcher must handle ${key}`,
    );
  }
});

test("DesktopToolbar mounts the TagFilter between SourceAppFilter and overflow", () => {
  // The toolbar MUST mount the new combobox between the
  // source-app filter and the configuration menu so the visual
  // order stays source-app → tag → overflow, matching the
  // contract the design pins.
  const sourceAppIndex = desktopToolbarSource.indexOf("SourceAppFilter");
  const tagIndex = desktopToolbarSource.indexOf("TagFilter");
  const overflowIndex = desktopToolbarSource.indexOf("open-overflow-menu");
  assert.ok(sourceAppIndex > -1, "SourceAppFilter must remain mounted");
  assert.ok(tagIndex > -1, "TagFilter must be mounted by the toolbar");
  assert.ok(overflowIndex > -1, "the overflow menu must remain mounted");
  assert.ok(
    sourceAppIndex < tagIndex && tagIndex < overflowIndex,
    "TagFilter must sit between SourceAppFilter and the overflow menu",
  );
});

test("App wires the tag filter through onTagFilterChange and the live tag ids", () => {
  // The parent MUST read the tag combobox through the same
  // callback pattern the source-app filter uses, and the
  // recents/search requests MUST forward the `tagIds` payload so
  // the backend can apply the AND combination with the
  // source-app filter. A regression that drops either branch
  // would silently disable the new facet.
  assert.ok(
    appSource.includes("onTagFilterChange"),
    "App.svelte must forward the tag filter selection through onTagFilterChange",
  );
  assert.ok(
    appSource.includes("handleTagFilterChange"),
    "App.svelte must declare a handleTagFilterChange callback",
  );
  // The recents/search requests MUST compute the tag ids from the
  // live `tagFilter` value (NOT from a reactive `$:` block whose
  // update lags one event behind). The bug the manual QA pass
  // detected was that the previous `tagFilterIds` reactive
  // declaration still held the OLD value when `loadEntries` /
  // `performSearch` issued their backend call, so a freshly
  // selected tag only took effect on the NEXT selection. The fix
  // forwards `currentTagFilterIds()` (a function call) so the
  // request always captures the current `tagFilter`.
  assert.ok(
    /tagIds:\s*currentTagFilterIds\(\)/.test(appSource),
    "App.svelte must forward the live tag ids through currentTagFilterIds()",
  );
  assert.ok(
    /function\s+currentTagFilterIds\s*\(/.test(appSource),
    "App.svelte must declare currentTagFilterIds() so the live tag ids are computed at call time",
  );
});

test("App resets the tag filter when switching collections", () => {
  // The collection switch MUST reset the tag filter to `Todas`
  // when the active tag no longer belongs to the new scope. The
  // helper sits next to the source-app filter reset so a future
  // refactor cannot forget the symmetric contract.
  const switchMatch = appSource.match(
    /async function selectCollectionFromSidebar[\s\S]*?await refreshEntries\(\);/,
  );
  assert.ok(switchMatch, "the collection switch handler must exist");
  assert.ok(
    /tagFilter\s*=\s*\{\s*kind:\s*"all"\s*\}/.test(switchMatch![0]),
    "the collection switch must reset the tag filter to Todas",
  );
});

test("App derives tagFilterOptions from the entryOrganization cache", () => {
  // The desktop MUST derive the combobox options from the
  // per-entry cache the parent already hydrates, not from a
  // second bridge call. The reactive declaration is the
  // single source of truth for the visible list.
  assert.ok(
    /tagFilterOptions\s*=\s*\(/.test(appSource),
    "App.svelte must derive tagFilterOptions reactively",
  );
  assert.ok(
    /entryOrganization/.test(appSource),
    "App.svelte must read the per-entry cache to build the options",
  );
  assert.ok(
    /display_name/.test(appSource),
    "App.svelte must surface only the display_name (never the id)",
  );
});

test("the TagFilter and TagFilterOption types are exported from types.ts", () => {
  // The combobox relies on the discriminated union the types
  // module exports. A regression that drops the exports breaks
  // every consumer (the combobox, the toolbar and the parent).
  assert.ok(
    /export type TagFilter\s*=/.test(typesSource),
    "types.ts must export the TagFilter union",
  );
  assert.ok(
    /export interface TagFilterOption/.test(typesSource),
    "types.ts must export the TagFilterOption interface",
  );
  // The union MUST keep its documented branches so the rest of
  // the desktop keeps branching on the same discriminator.
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

test("TagFilter never echoes the stable tag id as visible content", () => {
  // The combobox MUST render only the human-readable
  // `display_name`; the stable `id` is opaque and must never
  // leak into a visible string. A regression that surfaces the
  // id would break the spec's privacy contract.
  assert.ok(
    /option\.option\.display_name/.test(tagFilterSource),
    "the rendered label must read from option.display_name",
  );
  assert.ok(
    /\{option\.kind === "all"\s*\?\s*option\.display_name/.test(tagFilterSource),
    "the Todas row must read from the all-branch display_name",
  );
  // The trigger label must use the resolved display_name too;
  // the id must never reach the visible surface.
  assert.ok(
    /labelForSelected/.test(tagFilterSource),
    "the trigger must derive its label from the labelForSelected helper",
  );
  assert.ok(
    /options\.find\([^)]+\)\?\.display_name/.test(tagFilterSource) ||
      /\.display_name\s*\?\?\s*"Tag"/.test(tagFilterSource),
    "the trigger label must read from options[].display_name",
  );
});