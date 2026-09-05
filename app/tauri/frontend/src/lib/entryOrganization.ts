// Pure reconciliation helpers for the per-entry tag/collection cache
// rendered by `HistoryCardRail`. The desktop surfaces the same
// `Map<number, { tags, collections }>` and `Map<number,
// "pending" | "loaded" | "error">` shapes to every card; every
// mutation that flows through `App.svelte` (`refreshEntries`,
// `refreshEntryOrganization`, `refreshOrganizationForAllEntries`)
// must go through these helpers so the contract stays testable
// without mounting a Svelte component.
//
// The contract being pinned:
//
//   - `entryOrganization` and `entryOrganizationHydration` are
//     **per-entry caches** keyed by `EntryRecord.id`. They MUST be
//     reassigned (`= new Map(...)`) after every change so Svelte's
//     reactivity picks the change up — mutating them in place is
//     invisible to the compiler and the rail never re-renders.
//   - Pin/unpin and every other action that does NOT modify a
//     card's tags or collections MUST leave both maps untouched.
//     A subset pass (`hydrateEntryOrganization([only one entry])`)
//     would be interpreted by the legacy helper as "the visible
//     set is now only that entry" and would prune every other
//     card out of the cache — that is exactly the regression that
//     turned sibling cards into the "tags could not be loaded"
//     error state.
//   - `lookupHydration` defaults to `"error"` for an id it cannot
//     find in the map. To avoid ever surfacing that error for a
//     card that was simply not part of the latest hydration round,
//     `pruneEntryOrganizationToVisible` MUST keep the entries that
//     were already `"loaded"` even if the caller passes an
//     incomplete visible list. The function therefore returns the
//     next maps without depending on the caller having supplied
//     the full set.

import type { Collection, EntryRecord, Tag } from "../types.ts";

export type EntryOrganization = {
  tags: Tag[];
  collections: Collection[];
};

export type EntryOrganizationHydration = "pending" | "loaded" | "error";

export type EntryOrganizationMap = Map<number, EntryOrganization>;
export type EntryOrganizationHydrationMap = Map<number, EntryOrganizationHydration>;

/**
 * Reconcile the per-entry cache against the visible set passed by the
 * caller. The reconciliation is intentionally tolerant of a subset
 * call: when the caller passes only one entry, the function MUST
 * preserve the already-loaded tags/collections for every other entry
 * the cache knew about. Pin/unpin round-trips rely on this — the
 * affected entry updates `is_pinned` in lockstep with the rest of
 * the rail, and the cache for every other card keeps its `"loaded"`
 * state and its existing `Tag[]` / `Collection[]`.
 *
 * The helper is pure: the caller is expected to reassign the
 * returned maps onto the reactive variables.
 */
export function reconcileEntryOrganizationToVisible(
  currentOrganization: EntryOrganizationMap,
  currentHydration: EntryOrganizationHydrationMap,
  visibleEntries: readonly EntryRecord[],
): {
  nextOrganization: EntryOrganizationMap;
  nextHydration: EntryOrganizationHydrationMap;
} {
  const visibleIds = new Set(visibleEntries.map((entry) => entry.id));
  const nextOrganization: EntryOrganizationMap = new Map();
  const nextHydration: EntryOrganizationHydrationMap = new Map();
  for (const [id, value] of currentOrganization) {
    if (visibleIds.has(id)) {
      nextOrganization.set(id, value);
    }
  }
  for (const [id, state] of currentHydration) {
    if (visibleIds.has(id)) {
      nextHydration.set(id, state);
    }
  }
  return { nextOrganization, nextHydration };
}

/**
 * Pick the entries that still need a backend round-trip. Entries
 * already at `"loaded"` are skipped unless `force` is set, in which
 * case the caller (typically `refreshOrganizationForAllEntries`) is
 * asking for a re-hydration regardless of the cached state.
 */
export function selectPendingEntries(
  visibleEntries: readonly EntryRecord[],
  hydration: EntryOrganizationHydrationMap,
  options: { force?: boolean } = {},
): EntryRecord[] {
  if (options.force) {
    return [...visibleEntries];
  }
  return visibleEntries.filter((entry) => hydration.get(entry.id) !== "loaded");
}

/**
 * Mark the supplied entries as `"pending"` without disturbing any
 * other entry in the hydration map. The helper is the only place
 * the parent should mark entries as in-flight; reusing the same
 * call site guarantees that a `"loaded"` entry never gets demoted
 * to `"pending"` while a fetch is in flight.
 */
export function markEntriesPending(
  hydration: EntryOrganizationHydrationMap,
  entries: readonly EntryRecord[],
): EntryOrganizationHydrationMap {
  if (entries.length === 0) return hydration;
  const next = new Map(hydration);
  for (const entry of entries) {
    next.set(entry.id, "pending");
  }
  return next;
}

export type EntryOrganizationFetchResult =
  | { id: number; ok: true; tagIds: number[]; collectionIds: number[] }
  | { id: number; ok: false; error: string };

/**
 * Apply a batch of fetch results to the per-entry cache. The helper
 * is pure so the parent can compose the stale-response guard (a
 * monotonic sequence token) around it. The contract being pinned:
 *
 *   - The caller decides whether a result is stale; this helper
 *     applies whatever it is given.
 *   - An `ok: false` result for an entry sets only that entry to
 *     `"error"`; every other entry keeps its current state. There
 *     is no global "everything failed" branch — a single failure
 *     must never tip sibling cards into `"error"`.
 *   - An `ok: true` result replaces the entry's tags/collections
 *     with the freshly resolved ones and sets its state to
 *     `"loaded"`. The previous value, if any, is dropped without
 *     any de-duplication work — the backend is the source of
 *     truth.
 */
export function applyEntryOrganizationResults(
  organization: EntryOrganizationMap,
  hydration: EntryOrganizationHydrationMap,
  results: readonly EntryOrganizationFetchResult[],
  options: {
    resolveTags: (tagIds: number[]) => Tag[];
    resolveCollections: (collectionIds: number[]) => Collection[];
  },
): {
  nextOrganization: EntryOrganizationMap;
  nextHydration: EntryOrganizationHydrationMap;
} {
  const nextOrganization = new Map(organization);
  const nextHydration = new Map(hydration);
  for (const result of results) {
    if (result.ok) {
      const tags = options.resolveTags(result.tagIds);
      const collections = options.resolveCollections(result.collectionIds);
      nextOrganization.set(result.id, { tags, collections });
      nextHydration.set(result.id, "loaded");
    } else {
      nextHydration.set(result.id, "error");
    }
  }
  return { nextOrganization, nextHydration };
}

/**
 * Update a single entry's `is_pinned` flag in the canonical lists
 * the rail renders. The helper is intentionally pure and free of
 * any tag/collection side effect: a pin/unpin round-trip never
 * changes `entryOrganization` or `entryOrganizationHydration`, so a
 * sibling card never loses its `"loaded"` state during the pin.
 *
 * The function returns the new `entries` and `visibleEntries` lists
 * so the parent can reassign them in lockstep. When `isFiltering` is
 * `true` the visible list is patched independently because the
 * search hits keep their own snapshot of the entry — patching
 * `entries` alone would leave the rail showing a stale `is_pinned`.
 */
export function applyPinUpdate(
  currentEntries: readonly EntryRecord[],
  currentVisibleEntries: readonly EntryRecord[],
  updated: EntryRecord,
  options: { isFiltering: boolean },
): {
  nextEntries: EntryRecord[];
  nextVisibleEntries: EntryRecord[];
} {
  const patchEntry = (existing: EntryRecord): EntryRecord =>
    existing.id === updated.id ? { ...existing, is_pinned: updated.is_pinned } : existing;
  const nextEntries = currentEntries.map(patchEntry);
  const nextVisibleEntries = options.isFiltering
    ? currentVisibleEntries.map(patchEntry)
    : nextEntries;
  return { nextEntries, nextVisibleEntries };
}