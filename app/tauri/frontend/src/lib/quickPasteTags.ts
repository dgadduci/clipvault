// Per-entry tag cache the Quick Paste window consumes.
//
// Quick Paste is its own webview so it cannot reuse the
// `entryOrganization` map `App.svelte` already hydrates for the
// desktop rail. The module mirrors the same contracts:
//
//   - per-entry cache keyed by `EntryRecord.id`;
//   - hydration state machine `"pending" | "loaded" | "error"`;
//   - monotonic per-entry token guards every bridge round so a
//     stale response cannot overwrite a fresher commit when the
//     user navigates between entries while a request is in flight;
//   - the cache only ever carries metadata: the helpers read tag
//     ids through `entryTagsCommand` and resolve them against the
//     `OrganizationSnapshot.tags` array the same shell command
//     returns. The bridge payloads never carry clipboard content,
//     hashes, asset references or paths.
//
// The module is intentionally narrow. It does NOT duplicate the
// source-app or collection machinery: Quick Paste only surfaces
// tags on each row, and the rail keeps owning the rich card-side
// organisation data.

import type { Tag } from "../types.ts";

export type QuickPasteTagsCache = Map<number, Tag[]>;
export type QuickPasteTagsHydration = Map<number, QuickPasteTagState>;
export type QuickPasteTagState = "pending" | "loaded" | "error";

/**
 * Reset the module-level token table. Tests use it to guarantee
 * isolation between scenarios; production code never calls it.
 */
export function __resetQuickPasteTagsTokensForTests(): void {
  pendingByEntry.clear();
}

/**
 * Monotonic token per entry. The `bump` helper returns the next
 * value so a stale response (one whose `token` no longer matches
 * the table) can be discarded silently without polluting the
 * cache. The map is process-local and survives across hydrations
 * because the Quick Paste window keeps the same set of entries in
 * memory between successive scope refreshes.
 */
const pendingByEntry = new Map<number, number>();

/**
 * Bump and return the next token for the supplied entry id. The
 * helper is intentionally small and exported so the Svelte layer
 * can use it as the canonical stale-response guard without
 * re-implementing the contract.
 */
export function bumpQuickPasteTagsToken(entryId: number): number {
  const next = (pendingByEntry.get(entryId) ?? 0) + 1;
  pendingByEntry.set(entryId, next);
  return next;
}

/**
 * Read the active token for the supplied entry id. Returns `null`
 * when no round-trip has been scheduled for the entry; the Svelte
 * layer uses the sentinel to skip the stale-response branch.
 */
export function currentQuickPasteTagsToken(entryId: number): number | null {
  const value = pendingByEntry.get(entryId);
  return value === undefined ? null : value;
}

/**
 * Reset the per-entry token table. The Svelte layer calls this
 * helper when the visible scope changes (a scope switch, an
 * explicit refresh) so a stale response from the previous scope
 * never satisfies a future equality check.
 */
export function resetQuickPasteTagsToken(entryId: number): void {
  pendingByEntry.delete(entryId);
}

/**
 * Apply a single fetched tag set to the cache. The helper is pure:
 * the caller (the Svelte layer) decides whether the result is
 * still fresh by comparing the captured token against the live
 * `pendingByEntry` map; this helper writes whatever it is given.
 *
 * `tagIds` is the metadata-only payload the bridge returns; the
 * function resolves them against `knownTags` so the cache only
 * carries the typed `Tag[]` the rest of the desktop consumes. An
 * unknown id is silently dropped so a stale snapshot (a tag the
 * user deleted between the request and the response) cannot
 * resurrect it on the rail.
 */
export function applyQuickPasteTagsResult(
  cache: QuickPasteTagsCache,
  hydration: QuickPasteTagsHydration,
  entryId: number,
  tagIds: readonly number[],
  knownTags: readonly Tag[],
): {
  nextCache: QuickPasteTagsCache;
  nextHydration: QuickPasteTagsHydration;
  tags: Tag[];
} {
  const lookup = new Map(knownTags.map((tag) => [tag.id, tag] as const));
  const tags: Tag[] = [];
  for (const id of tagIds) {
    const tag = lookup.get(id);
    if (tag) tags.push(tag);
  }
  const nextCache = new Map(cache);
  nextCache.set(entryId, tags);
  const nextHydration = new Map(hydration);
  nextHydration.set(entryId, "loaded");
  return { nextCache, nextHydration, tags };
}

/**
 * Mark a single entry as `pending` without disturbing any other
 * row's hydration state. The helper is the only writer the Svelte
 * layer should use before issuing a bridge call; using it keeps
 * the row's prior state ("loaded" or "error") recoverable until
 * the round-trip resolves so a single failure cannot visibly
 * regress a previously-loaded entry.
 */
export function markQuickPasteTagsPending(
  hydration: QuickPasteTagsHydration,
  entryId: number,
): QuickPasteTagsHydration {
  if (hydration.get(entryId) === "pending") {
    return hydration;
  }
  const next = new Map(hydration);
  next.set(entryId, "pending");
  return next;
}

/**
 * Mark a single entry as `error` without disturbing any other row.
 * The helper is the single switch the Svelte layer uses on a
 * rejected bridge call.
 */
export function markQuickPasteTagsError(
  hydration: QuickPasteTagsHydration,
  entryId: number,
): QuickPasteTagsHydration {
  const next = new Map(hydration);
  next.set(entryId, "error");
  return next;
}

/**
 * Reset the per-entry cache slot. The Svelte layer calls this
 * helper when the visible scope shrinks (a stale entry left the
 * result list) so the cached tag set never leaks onto another row.
 */
export function clearQuickPasteTagsEntry(
  cache: QuickPasteTagsCache,
  hydration: QuickPasteTagsHydration,
  entryId: number,
): {
  nextCache: QuickPasteTagsCache;
  nextHydration: QuickPasteTagsHydration;
} {
  if (!cache.has(entryId) && !hydration.has(entryId)) {
    return { nextCache: cache, nextHydration: hydration };
  }
  const nextCache = new Map(cache);
  nextCache.delete(entryId);
  const nextHydration = new Map(hydration);
  nextHydration.delete(entryId);
  return { nextCache, nextHydration };
}

/**
 * Truncate the supplied tag list to the requested maximum number
 * of chips. The helper is the stable projection the row template
 * uses so a long tag set never expands the title line beyond the
 * documented `+N` indicator. The function is total and never
 * mutates the input array.
 *
 * The returned `overflow` count is the number of tags the row
 * dropped; the renderer surfaces it as a chip with the
 * accessible name the spec pins (`{N} tags adicionales`).
 */
export function truncateQuickPasteTags(
  tags: readonly Tag[],
  maxChips: number,
): { visible: Tag[]; overflow: number } {
  if (tags.length <= maxChips) {
    return { visible: [...tags], overflow: 0 };
  }
  return { visible: tags.slice(0, maxChips), overflow: tags.length - maxChips };
}