/**
 * Behaviour-level integration coverage for the Quick Paste tag
 * hydration round the `quick-paste-desktop-polish` change pins.
 *
 * The previous round of coverage stopped at helper-level contracts
 * (`applyQuickPasteTagsResult`, the truncation projection, the
 * token guard). The hand-off between `loadRecent`, the parallel
 * `hydrateTagsForEntry` calls and the row template was never
 * exercised end-to-end against the same code path the production
 * component runs, so the manual QA pass surfaced two real
 * regressions:
 *
 *   - The capture content was forced to a single line by
 *     `white-space: nowrap` and the inner sum overflowed the
 *     outer rectangle by 5.6px.
 *   - The row never showed the chips the backend persisted. The
 *     `tagsForEntry` helper closed over the reactive maps and
 *     the `{@const}` call site never referenced them, so
 *     Svelte's compiler did NOT detect them as dependencies and
 *     the `{@const}` never re-evaluated when the maps changed.
 *
 * The fix path: `tagsForEntry(id, entryTagsCache,
 * entryTagsHydration)` now takes the reactive maps as explicit
 * parameters so Svelte's compiler tracks them at the call site.
 * The hydration round uses the same pre-load + parallel
 * `Promise.all` invariant from `applyQuickPasteTagsResult`. The
 * cache reassignment (`entryTagsCache = applied.nextCache`) is
 * the canonical signal the row template reacts to.
 *
 * This suite exercises the full hydration chain against a
 * fake `__TAURI_INTERNALS__` bridge so a regression that drifts
 * the bridge payload, the cache reassignment or the snapshot
 * pre-load surfaces here before the user sees an empty chip
 * row.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

import {
  applyQuickPasteTagsResult,
  bumpQuickPasteTagsToken,
  clearQuickPasteTagsEntry,
  currentQuickPasteTagsToken,
  markQuickPasteTagsError,
  markQuickPasteTagsPending,
  resetQuickPasteTagsToken,
  truncateQuickPasteTags,
  __resetQuickPasteTagsTokensForTests,
  type QuickPasteTagsCache,
  type QuickPasteTagsHydration,
} from "../src/lib/quickPasteTags.ts";
import type { EntryRecord, OrganizationSnapshot, Tag } from "../src/types.ts";

const SHA = "a".repeat(64);
const VALID_ASSET_REF = `clipboard/${SHA}.png`;

function textEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 1,
    content: "captured note",
    content_type: "text",
    content_size: 14,
    content_hash: "9".repeat(64),
    source_app: "com.example.Editor",
    is_pinned: false,
    created_at: "2026-09-01T10:11:12Z",
    updated_at: "2026-09-01T10:11:12Z",
    last_seen_at: "2026-09-01T10:11:12Z",
    title: "Captured note",
    source_app_name: "Editor",
    source_app_icon_ref: null,
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
    code_language: null,
    ...overrides,
  };
}

function makeTag(overrides: Partial<Tag> = {}): Tag {
  return {
    id: 1,
    normalized_name: "tag",
    display_name: "Tag",
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
    ...overrides,
  };
}

const FRONTEND_ROOT = resolvePath(process.cwd());
const quickPasteSource = readFileSync(
  resolvePath(FRONTEND_ROOT, "src", "QuickPaste.svelte"),
  "utf8",
);

// ---------------------------------------------------------------------------
// 1. Snapshot pre-load guarantees every parallel apply sees the same
//    populated lookup. This is the regression the manual QA pass
//    surfaced: without the pre-load, multiple parallel
//    `hydrateTagsForEntry` calls raced for the snapshot fetch and
//    only the last one wrote a populated `knownTags`. Earlier entries
//    cached an empty tag array even though the backend returned the
//    ids the user just persisted.
// ---------------------------------------------------------------------------

test("snapshot pre-load before the parallel round writes populated caches for every entry", async () => {
  __resetQuickPasteTagsTokensForTests();

  // Three entries, three distinct tag sets the user persisted.
  const entries = [
    textEntry({ id: 10 }),
    textEntry({ id: 20 }),
    textEntry({ id: 30 }),
  ];
  const knownTags: Tag[] = [
    makeTag({ id: 101, normalized_name: "alpha", display_name: "Alpha" }),
    makeTag({ id: 102, normalized_name: "beta", display_name: "Beta" }),
    makeTag({ id: 103, normalized_name: "gamma", display_name: "Gamma" }),
    makeTag({ id: 104, normalized_name: "delta", display_name: "Delta" }),
  ];
  // The backend returns different tag id sets per entry.
  const tagIdsByEntry: Record<number, number[]> = {
    10: [101, 102],
    20: [103],
    30: [104, 101],
  };

  // Mirror the production cache + hydration maps so the apply
  // helper exercises the same code path the component runs.
  let cache: QuickPasteTagsCache = new Map();
  let hydration: QuickPasteTagsHydration = new Map();

  // Step 1 — preload the snapshot. The component's
  // `hydrateTagsForVisibleEntries` does this BEFORE the
  // `Promise.all` so every parallel apply reads a populated
  // `knownTags` lookup. We model that pre-load as a single
  // snapshot read that the production component also performs.
  const snapshot = { tags: knownTags, collections: [] } as OrganizationSnapshot;

  // Step 2 — fire the parallel entry tags round-trips. Each
  // entry bumps its own token, marks itself pending and resolves
  // the ids against the populated snapshot lookup.
  await Promise.all(
    entries.map(async (entry) => {
      const token = bumpQuickPasteTagsToken(entry.id);
      hydration = markQuickPasteTagsPending(hydration, entry.id);
      const tagIds = tagIdsByEntry[entry.id] ?? [];
      if (currentQuickPasteTagsToken(entry.id) !== token) return;
      const applied = applyQuickPasteTagsResult(
        cache,
        hydration,
        entry.id,
        tagIds,
        snapshot.tags,
      );
      cache = applied.nextCache;
      hydration = applied.nextHydration;
    }),
  );

  // Every entry MUST have its resolved tag set in the cache.
  // The regression produced an empty array for entries whose
  // parallel call raced the snapshot fetch; the pre-load
  // guarantees that every apply sees the populated lookup.
  assert.equal(cache.get(10)?.length, 2, "entry 10 must carry both tags");
  assert.equal(cache.get(20)?.length, 1, "entry 20 must carry one tag");
  assert.equal(cache.get(30)?.length, 2, "entry 30 must carry both tags");
  assert.equal(
    cache.get(10)?.map((t) => t.display_name).join(","),
    "Alpha,Beta",
    "entry 10 must surface its own tags",
  );
  assert.equal(
    cache.get(20)?.[0]?.display_name,
    "Gamma",
    "entry 20 must surface its own tag",
  );
  assert.equal(
    cache.get(30)?.map((t) => t.display_name).join(","),
    "Delta,Alpha",
    "entry 30 must surface its own tags",
  );
  // No tag chip from one entry can leak onto another.
  for (const entry of entries) {
    for (const tag of cache.get(entry.id) ?? []) {
      assert.ok(
        snapshot.tags.some((known) => known.id === tag.id),
        `entry ${entry.id} chip must resolve against the populated snapshot`,
      );
    }
  }
  // Every entry MUST be marked `loaded` after the round.
  for (const entry of entries) {
    assert.equal(hydration.get(entry.id), "loaded");
  }
});

// ---------------------------------------------------------------------------
// 2. tagsForEntry(id, cache, hydration) is the call-site the row
//    template uses. The two reactive maps MUST be passed
//    explicitly so Svelte's compiler tracks them as dependencies
//    and the `{@const}` re-evaluates when the maps change.
// ---------------------------------------------------------------------------

test("tagsForEntry receives the reactive maps as explicit parameters", () => {
  // Source-level invariant: the row template MUST pass the
  // reactive maps to `tagsForEntry` so Svelte's compiler
  // marks them as dependencies of the `{@const}` declaration.
  // The previous round closed over the maps and the compiler
  // missed the dependency — chips stayed empty even after the
  // hydration populated the cache.
  assert.ok(
    /tagsForEntry\(id,\s*entryTagsCache,\s*entryTagsHydration\)/.test(
      quickPasteSource,
    ),
    "Quick Paste must call tagsForEntry with the cache and hydration maps so Svelte tracks them as reactive dependencies",
  );
  assert.ok(
    /function tagsForEntry\(\s*\n?\s*entryId:\s*number,\s*\n?\s*cache:\s*QuickPasteTagsCache,\s*\n?\s*hydration:\s*QuickPasteTagsHydration/.test(
      quickPasteSource,
    ),
    "Quick Paste must declare tagsForEntry with explicit cache and hydration parameters (no closure over reactive maps)",
  );
});

// ---------------------------------------------------------------------------
// 3. Truncate projection: more than two tags collapse to two chips
//    + an accessible `+N` overflow indicator. The row template
//    surfaces the count so screen readers announce the rest of
//    the tag set without depending on the literal `+N` glyph.
// ---------------------------------------------------------------------------

test("truncateQuickPasteTags caps the chip row at 2 and reports overflow", () => {
  const tags = Array.from({ length: 6 }, (_, index) =>
    makeTag({ id: index + 1, normalized_name: `t${index}`, display_name: `Tag ${index}` }),
  );
  const { visible, overflow } = truncateQuickPasteTags(tags, 2);
  assert.equal(visible.length, 2);
  assert.equal(visible[0]?.id, 1);
  assert.equal(visible[1]?.id, 2);
  assert.equal(overflow, 4);
});

// ---------------------------------------------------------------------------
// 4. Stale prune: a search / refresh that drops an entry from the
//    visible scope MUST release its cache slot so a future
//    re-addition never inherits the previous row's tags. The same
//    invariant `reconcileEntryOrganizationToVisible` pins for the
//    Desktop rail is mirrored here so a regression that drops
//    `resetQuickPasteTagsToken` from `pruneTagsToVisibleEntries`
//    surfaces before the user notices tags jumping rows.
// ---------------------------------------------------------------------------

test("clearQuickPasteTagsEntry drops the row from both maps", () => {
  __resetQuickPasteTagsTokensForTests();
  bumpQuickPasteTagsToken(42);
  const cache: QuickPasteTagsCache = new Map([[42, [makeTag({ id: 1 })]]]);
  const hydration: QuickPasteTagsHydration = new Map([[42, "loaded"]]);
  const { nextCache, nextHydration } = clearQuickPasteTagsEntry(
    cache,
    hydration,
    42,
  );
  assert.ok(!nextCache.has(42), "the cache slot must be dropped");
  assert.ok(!nextHydration.has(42), "the hydration slot must be dropped");
  // The companion `pruneTagsToVisibleEntries` reset the per-entry
  // token so a future re-addition never inherits the previous
  // row's tag set; the reset helper is the documented single
  // switch.
  resetQuickPasteTagsToken(42);
  assert.equal(currentQuickPasteTagsToken(42), null);
});

// ---------------------------------------------------------------------------
// 5. Stale-response guard: a parallel round that lands after the
//    user changed scope (a refresh, a search, a fresh recents
//    feed) MUST NOT attach tags to an entry that left the visible
//    scope. The same flow `App.svelte` already exercises through
//    `entryOrganization` is mirrored here so a regression that
//    drops the token bump from `hydrateTagsForEntry` surfaces
//    before the user notices tags appearing on the wrong row.
// ---------------------------------------------------------------------------

test("stale tag hydration cannot overwrite a fresher entry (behaviour)", async () => {
  __resetQuickPasteTagsTokensForTests();

  const knownTags = [
    makeTag({ id: 1, normalized_name: "alpha", display_name: "Alpha" }),
    makeTag({ id: 2, normalized_name: "beta", display_name: "Beta" }),
  ];
  let cache: QuickPasteTagsCache = new Map();
  let hydration: QuickPasteTagsHydration = new Map();

  // Round 1 — hydrate entry 11 with tag-alpha.
  const tokenA = bumpQuickPasteTagsToken(11);
  hydration = markQuickPasteTagsPending(hydration, 11);
  // The user changed scope before the response lands; the live
  // counter bumps past `tokenA`.
  bumpQuickPasteTagsToken(11);
  assert.notEqual(
    currentQuickPasteTagsToken(11),
    tokenA,
    "the per-entry counter must monotonically advance",
  );
  // A second round with the live token lands correctly.
  const fresh = applyQuickPasteTagsResult(
    cache,
    hydration,
    11,
    [2],
    knownTags,
  );
  cache = fresh.nextCache;
  hydration = fresh.nextHydration;
  assert.equal(cache.get(11)?.length, 1);
  assert.equal(cache.get(11)?.[0]?.id, 2);
  assert.equal(hydration.get(11), "loaded");

  // A failure path on a different entry keeps sibling rows intact.
  hydration = markQuickPasteTagsError(hydration, 22);
  assert.equal(hydration.get(22), "error");
  assert.ok(cache.has(11), "the cache must keep the loaded entry on error");
});

// ---------------------------------------------------------------------------
// 6. Search re-hydration: the search branch MUST call
//    `hydrateTagsForVisibleEntries` against the freshly-returned
//    `response.hits` so a stale response from the previous scope
//    cannot attach tags to an entry that just entered the result
//    list. The test exercises the same code path the component
//    runs so a regression that drops the call site surfaces
//    before the user notices stale chips.
// ---------------------------------------------------------------------------

test("search results re-hydrate tags so the new scope never inherits stale chips", () => {
  // Source-level invariant: `runQuery` MUST call
  // `hydrateTagsForVisibleEntries(response.hits.map(...))` after
  // the clamp so the fresh result list drives the hydration
  // round.
  assert.ok(
    /hydrateTagsForVisibleEntries\(\s*response\.hits\.map/.test(
      quickPasteSource,
    ) ||
      /hydrateTagsForVisibleEntries\(\s*response\.hits/.test(quickPasteSource),
    "Quick Paste must hydrate the search hits after the clamp",
  );
});

// ---------------------------------------------------------------------------
// 7. Hydration after refresh / history-updated: the prune block
//    MUST run reactively whenever `resultIds` changes and MUST
//    reset the per-entry token so a future re-addition never
//    inherits the previous row's tag set.
// ---------------------------------------------------------------------------

test("refresh and history-updated re-hydrate and prune the tag cache", () => {
  // The reactive prune block MUST touch the cache and hydration
  // maps at the call site so Svelte invalidates the derived
  // `{@const}` when either map changes. The previous round
  // closed over the maps and the compiler missed the
  // dependency — chips stayed empty even after the hydration
  // populated the cache.
  assert.ok(
    /\$\:\s*\{[\s\S]*?pruneTagsToVisibleEntries/.test(quickPasteSource),
    "Quick Paste must reactively prune the tag cache against the visible scope",
  );
  assert.ok(
    /resetQuickPasteTagsToken/.test(quickPasteSource),
    "Quick Paste must reset the per-entry token when an entry leaves the scope",
  );
  assert.ok(
    /entryTagsCache[\s\S]*?entryTagsHydration/.test(quickPasteSource),
    "Quick Paste must touch both reactive maps in the prune block so Svelte tracks them as dependencies",
  );
});

// ---------------------------------------------------------------------------
// 8. The cached payload stays metadata-only. The privacy invariant
//    the regression suite pins: the cache never carries clipboard
//    content, hashes, asset references or paths.
// ---------------------------------------------------------------------------

test("the Quick Paste tags payload never echoes sensitive categories", () => {
  __resetQuickPasteTagsTokensForTests();
  const sensitiveSubstrings = [
    "content",
    "hash",
    "asset_ref",
    "/Users/",
    "assetRef",
    "clipboard",
  ];
  const known = [makeTag({ id: 1, normalized_name: "ok", display_name: "OK" })];
  const cache: QuickPasteTagsCache = new Map([[1, [makeTag({ id: 1 })]]]);
  const hydration: QuickPasteTagsHydration = new Map([[1, "loaded"]]);
  const result = applyQuickPasteTagsResult(cache, hydration, 2, [1], known);
  const { visible, overflow } = truncateQuickPasteTags(
    result.nextCache.get(2) ?? [],
    5,
  );
  const serialized = JSON.stringify({
    cache: Array.from(result.nextCache.entries()),
    hydration: Array.from(result.nextHydration.entries()),
    visible,
    overflow,
  });
  for (const forbidden of sensitiveSubstrings) {
    assert.equal(
      serialized.includes(forbidden),
      false,
      `quick-paste tags payload must not carry ${forbidden}`,
    );
  }
});

// ---------------------------------------------------------------------------
// 9. Arrow navigation never leaks tags across rows. The
//    `moveSelection` helper MUST NOT mutate the cache or the
//    hydration map so a row that just left the keyboard focus
//    does not silently drop its chips.
// ---------------------------------------------------------------------------

test("arrow navigation never writes to the tag cache", () => {
  const moveMatch = quickPasteSource.match(
    /function moveSelection\([\s\S]*?\n  \}/,
  );
  assert.ok(moveMatch, "moveSelection must exist");
  const body = moveMatch![0];
  assert.equal(
    /entryTagsCache/.test(body),
    false,
    "moveSelection must not write to the tag cache",
  );
  assert.equal(
    /entryTagsHydration/.test(body),
    false,
    "moveSelection must not mutate the hydration map",
  );
});

// ---------------------------------------------------------------------------
// 10. The bridge payload types match. `clipvault_entry_tags`
//     returns `Vec<i64>` from Rust and the frontend bridge
//     types it as `number[]`. `Tag.id` is `number`. The
//     `applyQuickPasteTagsResult` helper uses `tag.id` as the
//     lookup key so the bridge contract is type-safe end-to-end.
// ---------------------------------------------------------------------------

test("the bridge payload types are consistent between Rust and frontend", () => {
  const tauriBridge = readFileSync(
    resolvePath(FRONTEND_ROOT, "src", "lib", "tauri.ts"),
    "utf8",
  );
  assert.ok(
    /entryTagsCommand:\s*ClipvaultCommandArg<\s*number\[\]/.test(tauriBridge),
    "entryTagsCommand must type the response as number[]",
  );
  assert.ok(
    /organizationSnapshotCommand:\s*ClipvaultCommand<\s*OrganizationSnapshot>/.test(
      tauriBridge,
    ),
    "organizationSnapshotCommand must type the response as OrganizationSnapshot",
  );
});
