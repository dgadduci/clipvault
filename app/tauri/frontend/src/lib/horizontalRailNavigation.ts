/**
 * Pure helpers for the horizontal keyboard navigation the rail
 * exposes through `ArrowLeft` / `ArrowRight`.
 *
 * The Desktop rail scrolls horizontally and renders a row of
 * square cards; the spec demands that arrow keys both scroll the
 * rail AND change the rail-owned `selectedEntryId`. The previous
 * baseline only intercepted the keydown event for `Escape` so
 * pressing `ArrowLeft` / `ArrowRight` merely scrolled the native
 * overflow surface without ever flipping the canonical selection
 * id; the user observed "the rail scrolls but the card never
 * becomes active".
 *
 * The helpers in this module:
 *
 *   - resolve the next selected id for a horizontal arrow press
 *     against the visible entry ids the rail already received
 *     (the rail is the single source of truth — `entries` /
 *     `visibleEntries` are not duplicated here);
 *   - never wrap around: `ArrowRight` on the last visible card
 *     stays on that card, `ArrowLeft` on the first stays on the
 *     first. The user can always reach the same row again with a
 *     second press instead of being silently teleported to the
 *     other end of the list;
 *   - fall back to the first visible id when the selection is
 *     `null` and the user presses `ArrowRight`, and to the last
 *     visible id when the user presses `ArrowLeft`;
 *   - return `null` when the visible set is empty so the caller
 *     can short-circuit without a second guard;
 *   - expose a small `HorizontalRailDirection` enum the DOM-bound
 *     Svelte layer maps `ArrowLeft` / `ArrowRight` to without
 *     hard-coding the key strings in the helper.
 *
 * The module is intentionally DOM-free: no `getBoundingClientRect`,
 * no `scrollIntoView`, no `document`. The Svelte layer applies the
 * selection through the existing `selectedEntryId` binding and asks
 * the active card to `scrollIntoView({ inline: "nearest" })`.
 *
 * Tests in `tests/horizontalRailNavigation.test.ts` pin the math
 * byte-for-byte so a regression that re-introduces a wrap-around,
 * a stale index or a non-strict equality surfaces in CI.
 */

export type HorizontalRailEntryId = number;

export type HorizontalRailDirection = "left" | "right";

/**
 * Result of the navigation helper. The helper returns the next id
 * the rail should write into `selectedEntryId`, alongside a flag
 * signalling whether the selection actually moved — the Svelte
 * layer uses the flag to skip the `scrollIntoView` round-trip when
 * the user pressed the arrow against an already-selected row at
 * the boundary, and to skip the assignment when the rail is
 * empty.
 */
export interface HorizontalRailNavigation {
  nextId: HorizontalRailEntryId | null;
  /**
   * Whether the navigation produced a different id than the
   * input. The flag is `false` for boundary presses (the rail is
   * already on the last / first card) and for an empty visible
   * set. A `null → first` press and a `null → last` press both
   * yield `moved = true`.
   */
  moved: boolean;
}

/**
 * Resolve the next selected id for an arrow press on the rail.
 *
 * Contract:
 *
 *   - `entries` is the visible id list the rail owns. The order is
 *     authoritative: the helper never reorders, never filters, and
 *     never compares against a stale closure.
 *   - When `currentId` is `null` and the visible set is non-empty,
 *     `direction = "right"` selects the first id and `direction =
 *     "left"` selects the last id.
 *   - When `currentId` is found in `entries`, `direction = "right"`
 *     selects the next id and `direction = "left"` selects the
 *     previous id.
 *   - When `currentId` is the last visible id, `direction = "right"`
 *     stays on the same id (no wrap-around, `moved = false`).
 *   - When `currentId` is the first visible id, `direction = "left"`
 *     stays on the same id (no wrap-around, `moved = false`).
 *   - When `currentId` is `null` and `entries` is empty, the helper
 *     returns `{ nextId: null, moved: false }` so the rail can
 *     short-circuit without a second guard.
 *   - When `currentId` is non-null but is no longer in `entries`
 *     (the entry was filtered out, deleted, or moved to another
 *     collection), the helper falls back to the empty-selection
 *     branch so the user lands on the closest neighbour instead
 *     of an orphan row.
 *
 * The function is total and pure: any combination of arguments
 * produces a deterministic answer and never mutates its inputs.
 */
export function horizontalRailNextSelectionId(
  entries: readonly HorizontalRailEntryId[],
  currentId: HorizontalRailEntryId | null,
  direction: HorizontalRailDirection,
): HorizontalRailNavigation {
  if (entries.length === 0) {
    return { nextId: null, moved: false };
  }
  if (currentId === null) {
    const fallback =
      direction === "right" ? entries[0] : entries[entries.length - 1];
    return { nextId: fallback ?? null, moved: fallback !== null };
  }
  const currentIndex = entries.indexOf(currentId);
  if (currentIndex === -1) {
    // The current id fell out of the visible scope (delete, refresh,
    // search, collection switch). The rail's reactive block already
    // drops the value; the navigation helper mirrors that fallback so
    // a delayed ArrowRight after a scope change still lands on a
    // visible neighbour.
    const fallback =
      direction === "right" ? entries[0] : entries[entries.length - 1];
    return { nextId: fallback ?? null, moved: fallback !== null };
  }
  const nextIndex =
    direction === "right" ? currentIndex + 1 : currentIndex - 1;
  if (nextIndex < 0 || nextIndex >= entries.length) {
    // Boundary: do not wrap. The id stays on the current row and
    // the caller skips the `scrollIntoView` round-trip.
    return { nextId: currentId, moved: false };
  }
  const nextId = entries[nextIndex] ?? null;
  if (nextId === null) {
    return { nextId: currentId, moved: false };
  }
  return { nextId, moved: true };
}