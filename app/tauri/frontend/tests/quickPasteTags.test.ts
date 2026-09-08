/**
 * Unit coverage for `lib/quickPasteTags.ts`, the per-entry tag cache
 * the Quick Paste window consumes.
 *
 * The helpers stay intentionally narrow: the Svelte layer owns the
 * reactive maps and the bridge call, the pure helpers own the
 * stale-response guard, the truncation projection and the cache
 * transitions so the regression suite can exercise them without
 * mounting a Svelte component.
 *
 * Privacy invariant pinned here: every helper returns metadata
 * only. The cache never carries clipboard content, hashes, asset
 * references or paths. The test asserts the invariant through a
 * forbidden-substring scan so a future contributor cannot quietly
 * add a sensitive field.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

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
import type { Tag } from "../src/types.ts";

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

test("bumpQuickPasteTagsToken advances the per-entry token monotonically", () => {
  __resetQuickPasteTagsTokensForTests();
  const first = bumpQuickPasteTagsToken(101);
  const second = bumpQuickPasteTagsToken(101);
  const third = bumpQuickPasteTagsToken(101);
  assert.ok(first > 0);
  assert.ok(second > first);
  assert.ok(third > second);
  assert.equal(currentQuickPasteTagsToken(101), third);
});

test("tokens are isolated per entry id", () => {
  __resetQuickPasteTagsTokensForTests();
  // Bump entry 1 first so its counter starts at 1, then bump
  // entry 2 — both bump from 0 to 1 and we need to assert the
  // helpers keep the counter scoped to each entry id, not a
  // shared global value. After a reset both start from 0.
  bumpQuickPasteTagsToken(1);
  const a = bumpQuickPasteTagsToken(1);
  const b = bumpQuickPasteTagsToken(2);
  assert.notEqual(a, b);
  assert.equal(currentQuickPasteTagsToken(1), a);
  assert.equal(currentQuickPasteTagsToken(2), b);
});

test("resetQuickPasteTagsToken clears the per-entry token", () => {
  __resetQuickPasteTagsTokensForTests();
  bumpQuickPasteTagsToken(1);
  assert.notEqual(currentQuickPasteTagsToken(1), null);
  resetQuickPasteTagsToken(1);
  assert.equal(currentQuickPasteTagsToken(1), null);
});

test("applyQuickPasteTagsResult resolves ids through the known-tag table", () => {
  __resetQuickPasteTagsTokensForTests();
  const known = [
    makeTag({ id: 10, normalized_name: "alpha", display_name: "Alpha" }),
    makeTag({ id: 11, normalized_name: "beta", display_name: "Beta" }),
  ];
  const cache: QuickPasteTagsCache = new Map();
  const hydration: QuickPasteTagsHydration = new Map();
  const { nextCache, nextHydration, tags } = applyQuickPasteTagsResult(
    cache,
    hydration,
    42,
    [10, 11],
    known,
  );
  assert.deepEqual(
    tags.map((t) => t.id),
    [10, 11],
  );
  assert.equal(nextCache.get(42)?.length, 2);
  assert.equal(nextHydration.get(42), "loaded");
});

test("applyQuickPasteTagsResult silently drops unknown ids", () => {
  __resetQuickPasteTagsTokensForTests();
  const known = [makeTag({ id: 10, normalized_name: "alpha", display_name: "Alpha" })];
  const { tags } = applyQuickPasteTagsResult(
    new Map(),
    new Map(),
    42,
    [10, 99, 100],
    known,
  );
  // Only the resolved tag survives; the unknown ids are dropped so a
  // stale snapshot (a tag the user deleted between the request and
  // the response) cannot resurrect on the rail.
  assert.equal(tags.length, 1);
  assert.equal(tags[0]?.id, 10);
});

test("applyQuickPasteTagsResult replaces previous tags for the entry", () => {
  __resetQuickPasteTagsTokensForTests();
  const known = [
    makeTag({ id: 10, normalized_name: "alpha", display_name: "Alpha" }),
    makeTag({ id: 11, normalized_name: "beta", display_name: "Beta" }),
  ];
  const { nextCache } = applyQuickPasteTagsResult(
    new Map([[42, [known[0]!]]]),
    new Map([[42, "loaded" as const]]),
    42,
    [11],
    known,
  );
  assert.equal(nextCache.get(42)?.length, 1);
  assert.equal(nextCache.get(42)?.[0]?.id, 11);
});

test("markQuickPasteTagsPending does not demote a still-pending row", () => {
  __resetQuickPasteTagsTokensForTests();
  const hydration: QuickPasteTagsHydration = new Map([[42, "pending"]]);
  const next = markQuickPasteTagsPending(hydration, 42);
  assert.equal(next, hydration);
});

test("markQuickPasteTagsPending transitions loaded rows to pending", () => {
  __resetQuickPasteTagsTokensForTests();
  const hydration: QuickPasteTagsHydration = new Map([
    [42, "loaded"],
    [99, "loaded"],
  ]);
  const next = markQuickPasteTagsPending(hydration, 42);
  assert.equal(next.get(42), "pending");
  assert.equal(next.get(99), "loaded");
});

test("markQuickPasteTagsError sets only the affected entry", () => {
  __resetQuickPasteTagsTokensForTests();
  const hydration: QuickPasteTagsHydration = new Map([
    [42, "loaded"],
    [99, "loaded"],
  ]);
  const next = markQuickPasteTagsError(hydration, 42);
  assert.equal(next.get(42), "error");
  assert.equal(next.get(99), "loaded");
});

test("clearQuickPasteTagsEntry drops the row from both maps", () => {
  __resetQuickPasteTagsTokensForTests();
  const cache: QuickPasteTagsCache = new Map([
    [42, [makeTag({ id: 1 })]],
  ]);
  const hydration: QuickPasteTagsHydration = new Map([[42, "loaded"]]);
  const { nextCache, nextHydration } = clearQuickPasteTagsEntry(
    cache,
    hydration,
    42,
  );
  assert.equal(nextCache.has(42), false);
  assert.equal(nextHydration.has(42), false);
});

test("clearQuickPasteTagsEntry is a no-op when the row is not cached", () => {
  __resetQuickPasteTagsTokensForTests();
  const cache: QuickPasteTagsCache = new Map();
  const hydration: QuickPasteTagsHydration = new Map();
  const { nextCache, nextHydration } = clearQuickPasteTagsEntry(
    cache,
    hydration,
    42,
  );
  assert.equal(nextCache, cache);
  assert.equal(nextHydration, hydration);
});

test("truncateQuickPasteTags returns the whole list when it fits", () => {
  const tags = [
    makeTag({ id: 1, normalized_name: "a", display_name: "A" }),
    makeTag({ id: 2, normalized_name: "b", display_name: "B" }),
  ];
  const { visible, overflow } = truncateQuickPasteTags(tags, 5);
  assert.equal(visible.length, 2);
  assert.equal(overflow, 0);
});

test("truncateQuickPasteTags caps the visible chips and reports the overflow", () => {
  const tags = [
    makeTag({ id: 1, normalized_name: "a", display_name: "A" }),
    makeTag({ id: 2, normalized_name: "b", display_name: "B" }),
    makeTag({ id: 3, normalized_name: "c", display_name: "C" }),
    makeTag({ id: 4, normalized_name: "d", display_name: "D" }),
    makeTag({ id: 5, normalized_name: "e", display_name: "E" }),
  ];
  const { visible, overflow } = truncateQuickPasteTags(tags, 2);
  assert.equal(visible.length, 2);
  assert.equal(visible[0]?.id, 1);
  assert.equal(visible[1]?.id, 2);
  assert.equal(overflow, 3);
});

test("truncateQuickPasteTags never mutates the input array", () => {
  const tags = [
    makeTag({ id: 1, normalized_name: "a", display_name: "A" }),
    makeTag({ id: 2, normalized_name: "b", display_name: "B" }),
  ];
  const before = [...tags];
  truncateQuickPasteTags(tags, 1);
  assert.deepEqual(tags, before);
});

test("the cache never carries clipboard content, hashes or paths", () => {
  // Privacy invariant: a future contributor cannot accidentally
  // embed sensitive fields inside the cache. The test walks the
  // helpers' return values with a forbidden-substring scan so a
  // regression surfaces here before it reaches the rail.
  __resetQuickPasteTagsTokensForTests();
  const sensitiveSubstrings = [
    "content",
    "hash",
    "asset_ref",
    "/Users/",
    "assetRef",
    "clipboard",
  ];
  const known = [
    makeTag({ id: 1, normalized_name: "ok", display_name: "OK" }),
  ];
  const cache: QuickPasteTagsCache = new Map([
    [1, [makeTag({ id: 1 })]],
  ]);
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
      `quick-paste tags cache must not carry ${forbidden}`,
    );
  }
});