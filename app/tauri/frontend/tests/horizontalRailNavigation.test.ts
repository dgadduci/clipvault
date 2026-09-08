/**
 * Behavioural coverage for the horizontal keyboard navigation
 * helper the Desktop rail drives through `ArrowLeft` /
 * `ArrowRight`.
 *
 * The manual QA pass surfaced a regression where pressing the
 * horizontal arrows only scrolled the rail and never updated the
 * rail-owned `selectedEntryId`, so the user observed "the rail
 * scrolls but the card never becomes active". The pure helper
 * `horizontalRailNextSelectionId` (in
 * `../src/lib/horizontalRailNavigation.ts`) is the single switch
 * that resolves the next selected id and the suite below pins its
 * contract byte-for-byte.
 *
 * The tests cover every branch the spec lists:
 *
 *   - `null → first` and `null → last` fallbacks when the rail
 *     has no current selection;
 *   - `+1` / `-1` step inside the visible scope;
 *   - `no wrap-around` at either boundary;
 *   - `stale id` fallback when the current id fell out of the
 *     visible set (delete, refresh, search, collection switch);
 *   - `empty rail` short-circuit so an `ArrowRight` against an
 *     empty state never claims a `selectedEntryId`.
 *
 * The helper is intentionally pure and DOM-free; every test is a
 * plain `node:test` assertion with no fixture or polyfill so a
 * regression that accidentally couples the helper to the DOM or
 * to a global selector surfaces in CI.
 */

import { test } from "node:test";
import * as assert from "node:assert/strict";

import {
  horizontalRailNextSelectionId,
  type HorizontalRailDirection,
} from "../src/lib/horizontalRailNavigation.ts";

const VISIBLE: readonly number[] = [101, 102, 103, 104];

test("horizontalRailNextSelectionId picks the first id when currentId is null and direction is right", () => {
  const result = horizontalRailNextSelectionId(VISIBLE, null, "right");
  assert.equal(result.nextId, 101);
  assert.equal(result.moved, true);
});

test("horizontalRailNextSelectionId picks the last id when currentId is null and direction is left", () => {
  const result = horizontalRailNextSelectionId(VISIBLE, null, "left");
  assert.equal(result.nextId, 104);
  assert.equal(result.moved, true);
});

test("horizontalRailNextSelectionId advances by one to the right inside the visible scope", () => {
  // ArrowRight on the second card must select the third card,
  // confirming the +1 step from a mid-list position.
  const result = horizontalRailNextSelectionId(VISIBLE, 102, "right");
  assert.equal(result.nextId, 103);
  assert.equal(result.moved, true);
});

test("horizontalRailNextSelectionId retreats by one to the left inside the visible scope", () => {
  // ArrowLeft on the third card must select the second card,
  // confirming the -1 step from a mid-list position.
  const result = horizontalRailNextSelectionId(VISIBLE, 103, "left");
  assert.equal(result.nextId, 102);
  assert.equal(result.moved, true);
});

test("horizontalRailNextSelectionId clamps to the right edge without wrapping", () => {
  // ArrowRight on the LAST visible card must STAY on that card
  // (no wrap-around). The previous baseline used `% entries.length`
  // and silently teleported the user back to the first row, which
  // is the regression the manual QA pass surfaced.
  const result = horizontalRailNextSelectionId(VISIBLE, 104, "right");
  assert.equal(result.nextId, 104);
  assert.equal(result.moved, false);
});

test("horizontalRailNextSelectionId clamps to the left edge without wrapping", () => {
  // ArrowLeft on the FIRST visible card must STAY on that card
  // (no wrap-around to the last row).
  const result = horizontalRailNextSelectionId(VISIBLE, 101, "left");
  assert.equal(result.nextId, 101);
  assert.equal(result.moved, false);
});

test("horizontalRailNextSelectionId returns null for an empty visible set", () => {
  // An empty rail must short-circuit without claiming a selection;
  // a `null → null` answer lets the caller bail out before touching
  // the `selectedEntryId` state.
  const right = horizontalRailNextSelectionId([], null, "right");
  assert.equal(right.nextId, null);
  assert.equal(right.moved, false);
  const left = horizontalRailNextSelectionId([], null, "left");
  assert.equal(left.nextId, null);
  assert.equal(left.moved, false);
  // Even with a stale currentId, an empty rail must return
  // `{ nextId: null, moved: false }` so the rail can drop a stale
  // selection without re-anchoring on a non-existent row.
  const stale = horizontalRailNextSelectionId([], 999, "right");
  assert.equal(stale.nextId, null);
  assert.equal(stale.moved, false);
});

test("horizontalRailNextSelectionId falls back to the closest neighbour when the currentId fell out of scope", () => {
  // Delete, refresh, search and collection switch can drop the
  // currently selected id from the visible set. The helper mirrors
  // the rail's reactive scope-reset by returning the first id for
  // ArrowRight and the last id for ArrowLeft.
  const staleRight = horizontalRailNextSelectionId(VISIBLE, 999, "right");
  assert.equal(staleRight.nextId, 101);
  assert.equal(staleRight.moved, true);
  const staleLeft = horizontalRailNextSelectionId(VISIBLE, 999, "left");
  assert.equal(staleLeft.nextId, 104);
  assert.equal(staleLeft.moved, true);
});

test("horizontalRailNextSelectionId never mutates the entries array", () => {
  // The helper is pure. A regression that mutated the visible set
  // (e.g. through `entries.reverse()` or `entries.splice(...)`)
  // would silently break the rail's `{#each entries as entry}`
  // iteration and surface as flickering rows. The test asserts the
  // helper never touches its inputs.
  const snapshot = [...VISIBLE];
  horizontalRailNextSelectionId(VISIBLE, 102, "right");
  assert.deepEqual(VISIBLE, snapshot);
});

test("horizontalRailNextSelectionId handles a single-entry rail", () => {
  // Boundary case the Quick Paste-style `clampedSelectedIndex`
  // helper also pins: a rail with exactly one entry must keep the
  // selection on that single entry regardless of the arrow
  // direction; the helper must not wrap or fall through to `null`.
  const only = horizontalRailNextSelectionId([42], null, "right");
  assert.equal(only.nextId, 42);
  assert.equal(only.moved, true);
  const right = horizontalRailNextSelectionId([42], 42, "right");
  assert.equal(right.nextId, 42);
  assert.equal(right.moved, false);
  const left = horizontalRailNextSelectionId([42], 42, "left");
  assert.equal(left.nextId, 42);
  assert.equal(left.moved, false);
});

test("horizontalRailNextSelectionId respects the visible order for both directions", () => {
  // The helper must honour the order the rail passed in. A regression
  // that re-ordered the ids (e.g. by `id ASC` / `id DESC`) would
  // silently select a different card than the one the user saw.
  // The parametrised sweep confirms both directions advance through
  // the array in order without skipping or duplicating rows.
  for (const direction of ["right", "left"] as HorizontalRailDirection[]) {
    for (let i = 0; i < VISIBLE.length; i++) {
      const currentId = VISIBLE[i] ?? null;
      const expectedIndex = direction === "right" ? i + 1 : i - 1;
      const expectedId =
        expectedIndex < 0 || expectedIndex >= VISIBLE.length
          ? currentId
          : VISIBLE[expectedIndex] ?? currentId;
      const expectedMoved =
        expectedIndex >= 0 && expectedIndex < VISIBLE.length;
      const result = horizontalRailNextSelectionId(
        VISIBLE,
        currentId,
        direction,
      );
      assert.equal(
        result.nextId,
        expectedId,
        `direction=${direction} currentId=${currentId} expected nextId=${expectedId}, got ${result.nextId}`,
      );
      assert.equal(
        result.moved,
        expectedMoved,
        `direction=${direction} currentId=${currentId} expected moved=${expectedMoved}, got ${result.moved}`,
      );
    }
  }
});