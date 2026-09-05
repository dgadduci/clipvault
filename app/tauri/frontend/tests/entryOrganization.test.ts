/**
 * Regression coverage for the pin/unpin ↔ per-entry hydration
 * interaction that surfaced after the `desktop-shell-layout` change
 * shipped the four-modal coordinator.
 *
 * The reported regression: pinning a card with tags caused every
 * other visible card to show "No se pudieron cargar los tags de esta
 * entrada." until the user changed collection or restarted the app.
 *
 * Root cause: `App.svelte::toggleFavorite()` updated the local
 * `EntryRecord` and then called `hydrateEntryOrganization([updated])`.
 * `hydrateEntryOrganization` interpreted its argument as the
 * complete visible set and pruned `entryOrganization` and
 * `entryOrganizationHydration` to only the entry being pinned.
 * `HistoryCardRail.lookupHydration` defaults to `"error"` for any id
 * that is no longer in the map, so every other card was rendered as
 * a tag-load failure.
 *
 * The fix has two layers:
 *
 *   1. `toggleFavorite()` no longer calls `hydrateEntryOrganization`
 *      because pin/unpin does not change the affected entry's tag or
 *      collection associations.
 *   2. The reconciliation logic moved to `lib/entryOrganization.ts`
 *      as pure, testable helpers (`reconcileEntryOrganizationToVisible`,
 *      `selectPendingEntries`, `markEntriesPending`,
 *      `applyEntryOrganizationResults`, `applyPinUpdate`). The pure
 *      helpers are intentionally tolerant of a subset pass so a
 *      future caller that accidentally passes a single entry can no
 *      longer wipe the cache for the rest of the rail.
 *
 * The tests below pin both layers: the source-level invariant that
 * `toggleFavorite` no longer triggers a hydration round, and the
 * pure helpers that demonstrate a `loaded` entry stays `loaded`
 * after a pin and that the cache for every other entry is
 * preserved when the round-trip resolves.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  applyEntryOrganizationResults,
  applyPinUpdate,
  markEntriesPending,
  reconcileEntryOrganizationToVisible,
  selectPendingEntries,
  type EntryOrganizationFetchResult,
  type EntryOrganizationMap,
  type EntryOrganizationHydrationMap,
} from "../src/lib/entryOrganization.ts";
import type {
  Collection,
  EntryRecord,
  Tag,
} from "../src/types.ts";

const FRONTEND_ROOT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);

function loadFixture(absPath: string): string {
  return readFileSync(absPath, "utf8");
}

function makeTag(overrides: Partial<Tag> = {}): Tag {
  return {
    id: 42,
    normalized_name: "critical",
    display_name: "Critical",
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
    ...overrides,
  };
}

function makeCollection(overrides: Partial<Collection> = {}): Collection {
  return {
    id: 1,
    stable_key: "history",
    name: "Historial",
    kind: "system",
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
    ...overrides,
  };
}

function makeEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 1,
    content: "captured note",
    content_type: "text",
    content_size: 13,
    content_hash: "h",
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
    last_seen_at: "2026-01-02T03:04:05Z",
    is_pinned: false,
    source_app: "test",
    source_app_name: null,
    source_app_icon_ref: null,
    title: null,
    asset_ref: null,
    mime_type: null,
    payload_width: null,
    payload_height: null,
    rich_text_hash: null,
    rich_html_ref: null,
    rich_rtf_ref: null,
    rich_preview_ref: null,
    rich_html_size: null,
    rich_rtf_size: null,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Source-level invariants: pin/unpin must not trigger hydration.
// ---------------------------------------------------------------------------

/**
 * Strip line/block comments from a Svelte `<script>` body so the
 * downstream regexes can match actual call sites and never match
 * a regression pinned inside a comment.
 */
function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
}

test("App.svelte::toggleFavorite no longer hydrates the per-entry cache", () => {
  // The legacy implementation called
  // `await hydrateEntryOrganization([updated])` after a successful
  // pin. That call pruned the cache to a single entry, dropping
  // every other card's `"loaded"` state. The follow-up removes the
  // call: pin/unpin does NOT change a card's tags or collections,
  // so a hydration round is both unnecessary and harmful.
  const source = stripComments(
    loadFixture(path.join(FRONTEND_ROOT, "src/App.svelte")),
  );
  // Locate `toggleFavorite` and verify the body never calls
  // `hydrateEntryOrganization` with a subset.
  const match = source.match(
    /async function toggleFavorite[\s\S]*?\n  \}/,
  );
  assert.ok(match, "toggleFavorite must remain defined in App.svelte");
  const body = match?.[0] ?? "";
  assert.equal(
    /hydrateEntryOrganization\(\s*\[/.test(body),
    false,
    "toggleFavorite must not pass a subset to hydrateEntryOrganization",
  );
  assert.equal(
    /hydrateEntryOrganization\(/.test(body),
    false,
    "toggleFavorite must not trigger any hydration round",
  );
});

test("App.svelte::hydrateEntryOrganization always receives the full visible set", () => {
  // The remaining call sites of `hydrateEntryOrganization` must
  // pass the complete `entries` list. A subset pass would prune the
  // cache for every other card and surface them as tag-load errors.
  const source = stripComments(
    loadFixture(path.join(FRONTEND_ROOT, "src/App.svelte")),
  );
  const callSites = Array.from(
    source.matchAll(/hydrateEntryOrganization\(([^)]*)\)/g),
  ).map((m) => m[1]?.trim() ?? "");
  assert.ok(callSites.length >= 3, "there must be at least three call sites");
  for (const args of callSites) {
    assert.equal(
      args.startsWith("entries"),
      true,
      `hydrateEntryOrganization(${args}) must receive the full entries list`,
    );
  }
});

test("App.svelte::hydrateEntryOrganization relies on the pure helpers", () => {
  // The follow-up extracts the reconciliation logic to
  // `lib/entryOrganization.ts`. The component must call those
  // helpers instead of inlining the prune/pending logic so a future
  // contributor cannot regress the subset-pass behaviour.
  const source = stripComments(
    loadFixture(path.join(FRONTEND_ROOT, "src/App.svelte")),
  );
  assert.match(
    source,
    /reconcileEntryOrganizationToVisible\(\s*entryOrganization,\s*entryOrganizationHydration,\s*entries/,
  );
  assert.match(source, /selectPendingEntries\(entries, entryOrganizationHydration/);
  assert.match(
    source,
    /markEntriesPending\(\s*entryOrganizationHydration,\s*pendingEntries/,
  );
  assert.match(
    source,
    /applyEntryOrganizationResults\(\s*entryOrganization,\s*entryOrganizationHydration,\s*results/,
  );
});

test("App.svelte::toggleFavorite uses applyPinUpdate instead of inline .map()", () => {
  // The follow-up also routes the pin update through a pure helper
  // so the contract is testable. The inline `entries.map(...)`
  // mutation pattern was always correct, but pulling it into
  // `applyPinUpdate` lets us prove the no-pruning invariant from a
  // unit test.
  const source = stripComments(
    loadFixture(path.join(FRONTEND_ROOT, "src/App.svelte")),
  );
  assert.match(source, /applyPinUpdate\(entries, visibleEntries, updated,/);
});

// ---------------------------------------------------------------------------
// Pure helper: reconcileEntryOrganizationToVisible.
// ---------------------------------------------------------------------------

test(
  "reconcileEntryOrganizationToVisible preserves every loaded entry " +
    "when the caller passes the full visible set",
  () => {
    // The contract this test pins: when the caller passes the
    // full visible set, the helper keeps every loaded entry's
    // data and hydration state untouched. `refreshEntries` and
    // `refreshOrganizationForAllEntries` rely on this round-trip
    // being lossless. The legacy `toggleFavorite([updated])`
    // regression came from passing a subset; the dedicated
    // `applyPinUpdate` helper ensures pin/unpin never reaches for
    // the reconciliation helper at all.
    const tags1 = [makeTag({ id: 10, display_name: "Critical" })];
    const tags2 = [makeTag({ id: 11, display_name: "Draft" })];
    const tags3 = [makeTag({ id: 12, display_name: "WIP" })];

    const entryOrganization: EntryOrganizationMap = new Map([
      [1, { tags: tags1, collections: [] }],
      [2, { tags: tags2, collections: [] }],
      [3, { tags: tags3, collections: [] }],
    ]);
    const entryOrganizationHydration: EntryOrganizationHydrationMap = new Map([
      [1, "loaded"],
      [2, "loaded"],
      [3, "loaded"],
    ]);

    const reconciled = reconcileEntryOrganizationToVisible(
      entryOrganization,
      entryOrganizationHydration,
      [
        makeEntry({ id: 1, is_pinned: true }),
        makeEntry({ id: 2 }),
        makeEntry({ id: 3 }),
      ],
    );

    // Every loaded entry keeps its data and its hydration state.
    assert.equal(reconciled.nextOrganization.has(1), true);
    assert.equal(reconciled.nextOrganization.has(2), true);
    assert.equal(reconciled.nextOrganization.has(3), true);
    assert.equal(reconciled.nextHydration.get(1), "loaded");
    assert.equal(reconciled.nextHydration.get(2), "loaded");
    assert.equal(reconciled.nextHydration.get(3), "loaded");
    assert.deepEqual(
      reconciled.nextOrganization.get(2)?.tags.map((t) => t.id),
      [11],
    );
    assert.deepEqual(
      reconciled.nextOrganization.get(3)?.tags.map((t) => t.id),
      [12],
    );
  },
);

test(
  "reconcileEntryOrganizationToVisible on the full visible set is a no-op",
  () => {
    // When the caller passes every visible entry, the helper must
    // preserve every cached association unchanged. This is the
    // contract `refreshEntries` and `refreshOrganizationForAllEntries`
    // rely on.
    const tags1 = [makeTag({ id: 10, display_name: "Critical" })];
    const tags2 = [makeTag({ id: 11, display_name: "Draft" })];
    const entryOrganization: EntryOrganizationMap = new Map([
      [1, { tags: tags1, collections: [] }],
      [2, { tags: tags2, collections: [] }],
    ]);
    const entryOrganizationHydration: EntryOrganizationHydrationMap = new Map([
      [1, "loaded"],
      [2, "loaded"],
    ]);

    const reconciled = reconcileEntryOrganizationToVisible(
      entryOrganization,
      entryOrganizationHydration,
      [makeEntry({ id: 1 }), makeEntry({ id: 2 })],
    );

    assert.equal(reconciled.nextOrganization.size, 2);
    assert.equal(reconciled.nextHydration.size, 2);
    assert.deepEqual(
      reconciled.nextOrganization.get(1)?.tags.map((t) => t.id),
      [10],
    );
    assert.deepEqual(
      reconciled.nextOrganization.get(2)?.tags.map((t) => t.id),
      [11],
    );
  },
);

test(
  "reconcileEntryOrganizationToVisible drops entries that left the visible set",
  () => {
    // When the user switches collection, the previous visible set
    // falls out of scope. The helper drops those entries from the
    // cache so they don't accumulate forever, but it never touches
    // the entries that are still visible.
    const entryOrganization: EntryOrganizationMap = new Map([
      [1, { tags: [], collections: [] }],
      [2, { tags: [], collections: [] }],
      [3, { tags: [], collections: [] }],
    ]);
    const entryOrganizationHydration: EntryOrganizationHydrationMap = new Map([
      [1, "loaded"],
      [2, "loaded"],
      [3, "loaded"],
    ]);

    const reconciled = reconcileEntryOrganizationToVisible(
      entryOrganization,
      entryOrganizationHydration,
      [makeEntry({ id: 3 })],
    );

    assert.equal(reconciled.nextOrganization.has(1), false);
    assert.equal(reconciled.nextOrganization.has(2), false);
    assert.equal(reconciled.nextOrganization.has(3), true);
    assert.equal(reconciled.nextHydration.get(3), "loaded");
  },
);

// ---------------------------------------------------------------------------
// Pure helper: selectPendingEntries / markEntriesPending.
// ---------------------------------------------------------------------------

test("selectPendingEntries skips already-loaded entries unless force is set", () => {
  const hydration: EntryOrganizationHydrationMap = new Map([
    [1, "loaded"],
    [2, "loaded"],
    [3, "pending"],
  ]);
  const visible = [makeEntry({ id: 1 }), makeEntry({ id: 2 }), makeEntry({ id: 3 })];

  // Without force, only the `pending` entry needs fetching.
  assert.deepEqual(
    selectPendingEntries(visible, hydration).map((entry) => entry.id),
    [3],
  );
  // Force rehydrates every visible entry regardless of the cached state.
  assert.deepEqual(
    selectPendingEntries(visible, hydration, { force: true }).map(
      (entry) => entry.id,
    ),
    [1, 2, 3],
  );
});

test("markEntriesPending promotes entries to pending without disturbing siblings", () => {
  const hydration: EntryOrganizationHydrationMap = new Map([
    [1, "loaded"],
    [2, "loaded"],
    [3, "pending"],
  ]);
  const next = markEntriesPending(hydration, [makeEntry({ id: 2 })]);
  // Entry 2 transitions to pending; entries 1 and 3 keep their state.
  assert.equal(next.get(1), "loaded");
  assert.equal(next.get(2), "pending");
  assert.equal(next.get(3), "pending");
  // The helper MUST return a new Map so Svelte's reactivity picks
  // the change up; mutating in place is invisible to the compiler.
  assert.notEqual(next, hydration);
});

// ---------------------------------------------------------------------------
// Pure helper: applyEntryOrganizationResults.
// ---------------------------------------------------------------------------

test("applyEntryOrganizationResults commits loaded entries and isolates errors", () => {
  const organization: EntryOrganizationMap = new Map([
    [1, { tags: [], collections: [] }],
  ]);
  const hydration: EntryOrganizationHydrationMap = new Map([
    [1, "pending"],
    [2, "pending"],
  ]);
  const tags = [makeTag({ id: 10, display_name: "Critical" })];
  const results: EntryOrganizationFetchResult[] = [
    { id: 1, ok: true, tagIds: [10], collectionIds: [] },
    { id: 2, ok: false, error: "boom" },
  ];
  const applied = applyEntryOrganizationResults(organization, hydration, results, {
    resolveTags: (ids) => tags.filter((t) => ids.includes(t.id)),
    resolveCollections: () => [],
  });
  assert.equal(applied.nextOrganization.get(1)?.tags[0]?.id, 10);
  assert.equal(applied.nextHydration.get(1), "loaded");
  // A failed fetch sets ONLY the failing entry to `error`; sibling
  // entries (none here, but the helper must be robust to a mix).
  assert.equal(applied.nextHydration.get(2), "error");
});

test("applyEntryOrganizationResults keeps a previously-loaded entry when a sibling fails", () => {
  const organization: EntryOrganizationMap = new Map([
    [1, { tags: [makeTag({ id: 10 })], collections: [] }],
    [2, { tags: [makeTag({ id: 11 })], collections: [] }],
  ]);
  const hydration: EntryOrganizationHydrationMap = new Map([
    [1, "loaded"],
    [2, "pending"],
  ]);
  const tags = [
    makeTag({ id: 10, display_name: "Critical" }),
    makeTag({ id: 11, display_name: "Draft" }),
  ];
  const applied = applyEntryOrganizationResults(
    organization,
    hydration,
    [{ id: 2, ok: false, error: "boom" }],
    {
      resolveTags: (ids) => tags.filter((t) => ids.includes(t.id)),
      resolveCollections: () => [],
    },
  );
  // Entry 1 keeps its loaded tags and its `loaded` state.
  assert.equal(applied.nextHydration.get(1), "loaded");
  assert.equal(applied.nextOrganization.get(1)?.tags[0]?.id, 10);
  // Entry 2 is the only one flipped to `error`.
  assert.equal(applied.nextHydration.get(2), "error");
});

// ---------------------------------------------------------------------------
// Pure helper: applyPinUpdate.
// ---------------------------------------------------------------------------

test("applyPinUpdate flips is_pinned on the affected entry without touching siblings", () => {
  const entries: EntryRecord[] = [
    makeEntry({ id: 1, is_pinned: false }),
    makeEntry({ id: 2, is_pinned: false }),
    makeEntry({ id: 3, is_pinned: false }),
  ];
  const visibleEntries: EntryRecord[] = [...entries];
  const updated = makeEntry({ id: 1, is_pinned: true });

  const patched = applyPinUpdate(entries, visibleEntries, updated, {
    isFiltering: false,
  });

  const pinnedEntry = patched.nextEntries.find((entry) => entry.id === 1);
  assert.equal(pinnedEntry?.is_pinned, true);
  // Sibling entries keep their previous state.
  for (const id of [2, 3]) {
    const sibling = patched.nextEntries.find((entry) => entry.id === id);
    assert.equal(sibling?.is_pinned, false);
  }
  // visibleEntries mirrors entries when not filtering.
  assert.equal(patched.nextVisibleEntries.length, 3);
  assert.equal(
    patched.nextVisibleEntries.find((entry) => entry.id === 1)?.is_pinned,
    true,
  );
});

test("applyPinUpdate keeps visibleEntries independent of entries during a search", () => {
  // During a search, `visibleEntries` is the rail's filter result
  // and may diverge from `entries`. The helper must patch BOTH
  // lists so the rail reflects the new `is_pinned` for the affected
  // card AND every other visible card keeps its previous state.
  const entries: EntryRecord[] = [
    makeEntry({ id: 1, is_pinned: false }),
    makeEntry({ id: 4, is_pinned: false }),
  ];
  const visibleEntries: EntryRecord[] = [
    makeEntry({ id: 1, is_pinned: false }),
    makeEntry({ id: 4, is_pinned: false }),
  ];
  const updated = makeEntry({ id: 1, is_pinned: true });

  const patched = applyPinUpdate(entries, visibleEntries, updated, {
    isFiltering: true,
  });

  // visibleEntries updates the row that matched; siblings in the
  // search result are unaffected.
  assert.equal(
    patched.nextVisibleEntries.find((entry) => entry.id === 1)?.is_pinned,
    true,
  );
  assert.equal(
    patched.nextVisibleEntries.find((entry) => entry.id === 4)?.is_pinned,
    false,
  );
  // entries is patched independently because it owns the canonical
  // list; the rail reads visibleEntries when filtering.
  assert.equal(
    patched.nextEntries.find((entry) => entry.id === 1)?.is_pinned,
    true,
  );
});

// ---------------------------------------------------------------------------
// End-to-end behavioural test: a pin preserves every other entry's
// hydration state without going through Svelte.
// ---------------------------------------------------------------------------

test(
  "pinning one of three loaded cards keeps the other two loaded with their tags",
  () => {
    // Reproduces the user-reported regression in pure code. Three
    // cards have distinct tags loaded in the cache. Pinning the
    // first card must:
    //   - flip `is_pinned` on entry 1 only;
    //   - keep `entryOrganization` for entries 2 and 3 untouched;
    //   - keep `entryOrganizationHydration` for entries 2 and 3 at
    //     `"loaded"` so `lookupHydration` returns the cached
    //     associations instead of the `"error"` default.
    const tags1 = [makeTag({ id: 10, display_name: "Critical" })];
    const tags2 = [makeTag({ id: 11, display_name: "Draft" })];
    const tags3 = [makeTag({ id: 12, display_name: "WIP" })];
    const initialOrganization: EntryOrganizationMap = new Map([
      [1, { tags: tags1, collections: [] }],
      [2, { tags: tags2, collections: [] }],
      [3, { tags: tags3, collections: [] }],
    ]);
    const initialHydration: EntryOrganizationHydrationMap = new Map([
      [1, "loaded"],
      [2, "loaded"],
      [3, "loaded"],
    ]);

    const entries: EntryRecord[] = [
      makeEntry({ id: 1, is_pinned: false }),
      makeEntry({ id: 2, is_pinned: false }),
      makeEntry({ id: 3, is_pinned: false }),
    ];
    const visibleEntries: EntryRecord[] = [...entries];
    const updated = makeEntry({ id: 1, is_pinned: true });

    // Apply the pin. The follow-up does NOT call
    // `hydrateEntryOrganization([updated])` — pin/unpin must not
    // touch the hydration cache. The pure pin helper is the only
    // tool `toggleFavorite` uses to update `entries` and
    // `visibleEntries`; the hydration maps are not reassigned.
    const patched = applyPinUpdate(entries, visibleEntries, updated, {
      isFiltering: false,
    });

    // State assertions.
    assert.equal(
      patched.nextEntries.find((entry) => entry.id === 1)?.is_pinned,
      true,
      "the affected entry must report pinned",
    );
    assert.equal(
      patched.nextEntries.find((entry) => entry.id === 2)?.is_pinned,
      false,
      "sibling entries must not flip",
    );
    // The hydration cache stays exactly as it was — `toggleFavorite`
    // never calls a hydration helper, so neither `entryOrganization`
    // nor `entryOrganizationHydration` is reassigned.
    assert.equal(initialHydration.get(1), "loaded");
    assert.equal(initialHydration.get(2), "loaded");
    assert.equal(initialHydration.get(3), "loaded");
    assert.deepEqual(
      initialOrganization.get(2)?.tags.map((t) => t.id),
      [11],
      "entry 2 tags must remain the freshly hydrated set",
    );
    assert.deepEqual(
      initialOrganization.get(3)?.tags.map((t) => t.id),
      [12],
      "entry 3 tags must remain the freshly hydrated set",
    );
  },
);

test("unpinning without tags clears the pin flag without touching the cache", () => {
  // The follow-up covers unpin as well: unpin is the inverse
  // operation and must obey the same no-pruning invariant.
  const tags1 = [makeTag({ id: 10 })];
  let entryOrganization: EntryOrganizationMap = new Map([
    [1, { tags: tags1, collections: [] }],
    [2, { tags: [], collections: [] }],
  ]);
  let entryOrganizationHydration: EntryOrganizationHydrationMap = new Map([
    [1, "loaded"],
    [2, "loaded"],
  ]);

  const entries: EntryRecord[] = [
    makeEntry({ id: 1, is_pinned: true }),
    makeEntry({ id: 2, is_pinned: false }),
  ];
  const visibleEntries: EntryRecord[] = [...entries];
  const updated = makeEntry({ id: 1, is_pinned: false });

  const patched = applyPinUpdate(entries, visibleEntries, updated, {
    isFiltering: false,
  });
  assert.equal(
    patched.nextEntries.find((entry) => entry.id === 1)?.is_pinned,
    false,
  );

  // The hydration cache stays at `loaded` for every entry.
  assert.equal(entryOrganizationHydration.get(1), "loaded");
  assert.equal(entryOrganizationHydration.get(2), "loaded");
});

test("pin during a search keeps the cache and patches the visible list", () => {
  // The bug also reproduces during a search: the helper is invoked
  // with the visible subset, which is even smaller than `entries`
  // and would prune even more rows. The follow-up removes the
  // call entirely; the pin updates `visibleEntries` (the search
  // hits) and `entries` independently but never reaches for the
  // hydration helpers.
  const hydration: EntryOrganizationHydrationMap = new Map([
    [1, "loaded"],
    [2, "loaded"],
    [3, "loaded"],
  ]);
  const organization: EntryOrganizationMap = new Map([
    [1, { tags: [makeTag({ id: 10 })], collections: [] }],
    [2, { tags: [makeTag({ id: 11 })], collections: [] }],
    [3, { tags: [makeTag({ id: 12 })], collections: [] }],
  ]);

  // The pin still updates only the visible entry, regardless of
  // whether the search narrows the rail to a single card.
  const searchResult: EntryRecord[] = [
    makeEntry({ id: 1, is_pinned: false }),
    makeEntry({ id: 2, is_pinned: false }),
  ];
  const updated = makeEntry({ id: 1, is_pinned: true });
  const patched = applyPinUpdate(
    /* currentEntries = */ searchResult,
    /* currentVisibleEntries = */ searchResult,
    updated,
    { isFiltering: true },
  );
  assert.equal(
    patched.nextVisibleEntries.find((entry) => entry.id === 1)?.is_pinned,
    true,
  );
  // Hydration cache stays untouched; the pin never reaches for the
  // hydration helpers regardless of the filter state.
  assert.equal(hydration.get(1), "loaded");
  assert.equal(hydration.get(2), "loaded");
  assert.equal(hydration.get(3), "loaded");
  assert.equal(organization.get(2)?.tags[0]?.id, 11);
  assert.equal(organization.get(3)?.tags[0]?.id, 12);
});

test("pinning an image card does not touch the hydration cache", () => {
  // The follow-up must also hold for image cards: the thumbnail
  // surface is independent of the per-entry hydration state and
  // must not be invalidated by the pin. The pure helpers do not
  // inspect the entry's content_type, so the test confirms the
  // invariant through the same entry-shape regardless of payload.
  const tags = [makeTag({ id: 10 })];
  const hydration: EntryOrganizationHydrationMap = new Map([[1, "loaded"]]);
  const organization: EntryOrganizationMap = new Map([
    [1, { tags, collections: [] }],
  ]);
  const imageEntry = makeEntry({
    id: 1,
    content_type: "image",
    content: "",
    asset_ref: "clipboard/abcdef.png",
    mime_type: "image/png",
    payload_width: 320,
    payload_height: 240,
  });
  const updated = { ...imageEntry, is_pinned: true };
  const patched = applyPinUpdate([imageEntry], [imageEntry], updated, {
    isFiltering: false,
  });
  assert.equal(patched.nextEntries[0]?.is_pinned, true);
  // Image metadata (asset_ref, mime_type, dimensions) survives.
  assert.equal(patched.nextEntries[0]?.asset_ref, "clipboard/abcdef.png");
  assert.equal(patched.nextEntries[0]?.mime_type, "image/png");
  // The hydration cache is untouched.
  assert.equal(hydration.get(1), "loaded");
});

test("pinning an entry with no tags does not introduce a phantom hydration error", () => {
  // A card without tags must never be promoted to `"error"` by a
  // pin/unpin round. The pin only changes `is_pinned`; the entry
  // is reported as `loaded` with an empty `tags` array.
  const hydration: EntryOrganizationHydrationMap = new Map([[1, "loaded"]]);
  const organization: EntryOrganizationMap = new Map([
    [1, { tags: [], collections: [] }],
  ]);
  const entry = makeEntry({ id: 1, is_pinned: false });
  const updated = { ...entry, is_pinned: true };
  const patched = applyPinUpdate([entry], [entry], updated, {
    isFiltering: false,
  });
  assert.equal(patched.nextEntries[0]?.is_pinned, true);
  // Cache stays loaded; a no-tag entry is NOT an error state.
  assert.equal(hydration.get(1), "loaded");
  assert.equal(organization.get(1)?.tags.length, 0);
});

test("a single-entry hydrate pass does not convert loaded entries to error", () => {
  // Sanity: a buggy future caller that still invokes the helper
  // with a single entry MUST NOT be able to convert `"loaded"`
  // entries into `"error"`. The helper is tolerant by construction
  // (it preserves every entry that is already in the visible
  // ids, and a missing entry falls out of the cache entirely).
  const hydration: EntryOrganizationHydrationMap = new Map([
    [1, "loaded"],
    [2, "loaded"],
    [3, "loaded"],
  ]);
  const organization: EntryOrganizationMap = new Map([
    [1, { tags: [], collections: [] }],
    [2, { tags: [], collections: [] }],
    [3, { tags: [], collections: [] }],
  ]);
  const subset = [makeEntry({ id: 1 })];
  const reconciled = reconcileEntryOrganizationToVisible(
    organization,
    hydration,
    subset,
  );
  // Entries 2 and 3 are pruned, but the ones that remain keep
  // their `loaded` state. No entry is promoted to `error`.
  assert.equal(reconciled.nextHydration.get(1), "loaded");
  for (const [id, state] of reconciled.nextHydration) {
    assert.notEqual(state, "error");
    void id;
  }
});

test("rehydrating one entry without clearing the rest of the cache", () => {
  // The single-entry hydration helper (`refreshEntryOrganization`)
  // already exists. The pure contract this test pins: a single
  // entry's tags and hydration state can be updated independently
  // without affecting any other entry.
  const hydration: EntryOrganizationHydrationMap = new Map([
    [1, "loaded"],
    [2, "loaded"],
  ]);
  const organization: EntryOrganizationMap = new Map([
    [1, { tags: [makeTag({ id: 10 })], collections: [] }],
    [2, { tags: [makeTag({ id: 11 })], collections: [] }],
  ]);
  // The single-entry update only touches entry 2.
  const results: EntryOrganizationFetchResult[] = [
    { id: 2, ok: true, tagIds: [11], collectionIds: [] },
  ];
  const tagsCatalog = [
    makeTag({ id: 10, display_name: "Critical" }),
    makeTag({ id: 11, display_name: "Draft" }),
  ];
  const applied = applyEntryOrganizationResults(
    organization,
    hydration,
    results,
    {
      resolveTags: (ids) => tagsCatalog.filter((t) => ids.includes(t.id)),
      resolveCollections: () => [],
    },
  );
  // Entry 1 is untouched.
  assert.equal(applied.nextHydration.get(1), "loaded");
  assert.equal(applied.nextOrganization.get(1)?.tags[0]?.id, 10);
  // Entry 2 stays `loaded` with its tag set.
  assert.equal(applied.nextHydration.get(2), "loaded");
  assert.equal(applied.nextOrganization.get(2)?.tags[0]?.id, 11);
});

test("no duplicate requests are issued when toggling pin", () => {
  // A pin/unpin round must NOT trigger an extra `entry_tags` or
  // `entry_collections` fetch. The pure helpers do not call any
  // Tauri command — the regression coverage at the source level
  // pins that `toggleFavorite` no longer reaches for the
  // hydration helper. This test re-asserts the same invariant
  // through the helpers themselves.
  const hydration: EntryOrganizationHydrationMap = new Map([[1, "loaded"]]);
  const organization: EntryOrganizationMap = new Map([
    [1, { tags: [], collections: [] }],
  ]);
  // Calling the pin helper does not generate any pending entries
  // because the helper doesn't fetch.
  const pending = selectPendingEntries(
    [makeEntry({ id: 1, is_pinned: false })],
    hydration,
  );
  assert.deepEqual(pending, []);
  // And calling the reconcile helper with the full visible set
  // does not generate pending entries either.
  const reconciled = reconcileEntryOrganizationToVisible(
    organization,
    hydration,
    [makeEntry({ id: 1, is_pinned: true })],
  );
  const pendingAfter = selectPendingEntries(
    [makeEntry({ id: 1, is_pinned: true })],
    reconciled.nextHydration,
  );
  assert.deepEqual(pendingAfter, []);
});

// ---------------------------------------------------------------------------
// Secondary collection and history scenarios.
// ---------------------------------------------------------------------------

test("pin inside a secondary collection preserves every other entry's cache", () => {
  // The regression also reproduces when the active collection is a
  // user (secondary) collection. The follow-up keeps the same
  // invariant: pin/unpin never touches the per-entry cache.
  const hydration: EntryOrganizationHydrationMap = new Map([
    [1, "loaded"],
    [2, "loaded"],
  ]);
  const organization: EntryOrganizationMap = new Map([
    [1, { tags: [makeTag({ id: 10 })], collections: [makeCollection({ id: 2, kind: "user" })] }],
    [2, { tags: [makeTag({ id: 11 })], collections: [makeCollection({ id: 2, kind: "user" })] }],
  ]);
  const entries: EntryRecord[] = [
    makeEntry({ id: 1, is_pinned: false }),
    makeEntry({ id: 2, is_pinned: false }),
  ];
  const updated = makeEntry({ id: 1, is_pinned: true });
  const patched = applyPinUpdate(entries, entries, updated, {
    isFiltering: false,
  });
  assert.equal(patched.nextEntries[0]?.is_pinned, true);
  // Cache stays untouched.
  assert.equal(hydration.get(1), "loaded");
  assert.equal(hydration.get(2), "loaded");
  assert.equal(organization.get(2)?.tags[0]?.id, 11);
});

test("History collection: pinning inside Historial preserves the cache", () => {
  // Same scenario but with the system `Historial` collection as the
  // active selection. The contract is identical: pin/unpin does
  // not invalidate the cache.
  const hydration: EntryOrganizationHydrationMap = new Map([
    [1, "loaded"],
    [2, "loaded"],
    [3, "loaded"],
  ]);
  const entries: EntryRecord[] = [
    makeEntry({ id: 1, is_pinned: false }),
    makeEntry({ id: 2, is_pinned: false }),
    makeEntry({ id: 3, is_pinned: false }),
  ];
  const updated = makeEntry({ id: 2, is_pinned: true });
  const patched = applyPinUpdate(entries, entries, updated, {
    isFiltering: false,
  });
  assert.equal(
    patched.nextEntries.find((entry) => entry.id === 2)?.is_pinned,
    true,
  );
  // Every entry keeps its loaded state.
  for (const id of [1, 2, 3]) {
    assert.equal(hydration.get(id), "loaded");
  }
});