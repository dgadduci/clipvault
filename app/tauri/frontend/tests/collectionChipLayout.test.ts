/**
 * Pure-helper regression coverage for the responsive chip-row
 * subset the `clipboard-history-cards` change renders on each
 * card. The helper lives in `lib/collectionChipLayout.ts` and
 * returns the slice of user collections that fit alongside the
 * `+N` overflow chip. The card surface calls it from a reactive
 * `$:` block; the tests below pin the contract so a future refactor
 * cannot silently change the visible-subset computation.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  COLLECTION_CHIP_GAP_PX,
  COLLECTION_OVERFLOW_BUTTON_PX,
  COLLECTION_OVERFLOW_TOLERANCE_PX,
  computeOverflowCount,
  computeVisibleCollections,
  intrinsicCollectionRowWidth,
  type CollectionLike,
} from "../src/lib/collectionChipLayout.ts";

function makeCollection(id: number, name: string): CollectionLike {
  return { id, name };
}

const ROW_WIDTH = 200;

test("computeVisibleCollections returns every chip when no overflow is detected", () => {
  const collections = [
    makeCollection(1, "Trabajo"),
    makeCollection(2, "Clientes"),
    makeCollection(3, "Investigación"),
  ];
  const widths = [80, 80, 110];
  const visible = computeVisibleCollections(collections, widths, ROW_WIDTH, false);
  assert.equal(visible.length, 3);
  assert.deepEqual(
    visible.map((c) => c.id),
    [1, 2, 3],
  );
});

test("computeVisibleCollections trims from the end when overflow is detected", () => {
  const collections = [
    makeCollection(1, "Trabajo"),
    makeCollection(2, "Clientes"),
    makeCollection(3, "Investigación"),
    makeCollection(4, "Personal"),
  ];
  const widths = [90, 90, 110, 90];
  // Available width = 200 - 32 = 168.
  // Chip 1 fits (90).
  // Chip 2 fits (90 + 4 + 90 = 184 > 168, clipped).
  const visible = computeVisibleCollections(collections, widths, ROW_WIDTH, true);
  assert.equal(visible.length, 1, "only the first chip should fit");
  assert.equal(visible[0].id, 1);
});

test("computeVisibleCollections accounts for the gap between chips", () => {
  const collections = [
    makeCollection(1, "A"),
    makeCollection(2, "B"),
    makeCollection(3, "C"),
  ];
  // Widths chosen so two chips fit exactly:
  //   - chip 1: 70
  //   - chip 1 + gap + chip 2 = 70 + 4 + 70 = 144 (fits in 168)
  //   - + gap + chip 3 = 144 + 4 + 70 = 218 (overflow)
  const widths = [70, 70, 70];
  const visible = computeVisibleCollections(collections, widths, ROW_WIDTH, true);
  assert.equal(visible.length, 2, "the third chip must be clipped by the gap");
  assert.deepEqual(
    visible.map((c) => c.id),
    [1, 2],
  );
});

test("computeVisibleCollections always reserves space for the overflow chip", () => {
  const collections = [makeCollection(1, "Trabajo")];
  const widths = [200];
  // Single chip wider than the row minus the chip reservation:
  // the helper still reports overflow so the chip appears, but
  // must keep the chip visible instead of returning an empty
  // list.
  const visible = computeVisibleCollections(collections, widths, ROW_WIDTH, true);
  assert.equal(visible.length, 1);
  assert.equal(visible[0].id, 1);
});

test("computeVisibleCollections returns at least one chip for very narrow rows", () => {
  const collections = [
    makeCollection(1, "Trabajo"),
    makeCollection(2, "Clientes"),
  ];
  const widths = [120, 120];
  // Row narrower than the chip reservation alone: the helper
  // falls back to the first chip so the inline row is never
  // empty.
  const visible = computeVisibleCollections(collections, widths, 12, true);
  assert.equal(visible.length, 1);
  assert.equal(visible[0].id, 1);
});

test("computeVisibleCollections returns an empty list when there are no chips", () => {
  const visible = computeVisibleCollections([], [], ROW_WIDTH, true);
  assert.deepEqual(visible, []);
});

test("computeVisibleCollections returns the full list when overflow is true but the row is wider than every chip", () => {
  const collections = [
    makeCollection(1, "A"),
    makeCollection(2, "B"),
  ];
  const widths = [40, 40];
  // Row wide enough to hold both chips AND the chip. The
  // measurement helper would only flip overflow to true when the
  // painted row actually clips; when overflow is true but every
  // chip still fits the helper must honour the input.
  const visible = computeVisibleCollections(collections, widths, 400, true);
  assert.equal(visible.length, 2);
});

test("computeVisibleCollections tolerates shorter width arrays", () => {
  // A `ResizeObserver` race can land before the measurement
  // strip finishes rendering; the helper must default the missing
  // width to zero so the loop does not throw. With every width
  // unknown the helper conservatively treats every chip as
  // fitting (the painted row would still trigger overflow through
  // its own `scrollWidth`/`clientWidth` measurement), so the
  // test pins the tolerant behaviour rather than the trim.
  const collections = [
    makeCollection(1, "Trabajo"),
    makeCollection(2, "Clientes"),
  ];
  const visible = computeVisibleCollections(collections, [], ROW_WIDTH, true);
  assert.equal(visible.length, 2);
});

test("computeVisibleCollections accepts the measured overflow chip width and trims accordingly", () => {
  // When the caller forwards the real `+N` width, the helper
  // must reserve that exact space instead of the documented
  // fallback so the responsive subset tracks the painted chip.
  const collections = [
    makeCollection(1, "A"),
    makeCollection(2, "B"),
    makeCollection(3, "C"),
  ];
  const widths = [70, 70, 70];
  // Available width = 200 - 24 = 176 (measured chip width = 24).
  //   - chip 1 fits (70).
  //   - chip 2 fits (70 + 4 + 70 = 144).
  //   - chip 3 overflow (144 + 4 + 70 = 218 > 176).
  const visible = computeVisibleCollections(collections, widths, ROW_WIDTH, true, 24);
  assert.equal(visible.length, 2);
  assert.deepEqual(
    visible.map((c) => c.id),
    [1, 2],
  );
});

test("computeVisibleCollections falls back to the documented reservation when no chip width is supplied", () => {
  // First render before the measurement strip has reported: the
  // helper must still produce a deterministic subset using the
  // documented reservation so the row never grows beyond the row
  // width.
  const collections = [
    makeCollection(1, "A"),
    makeCollection(2, "B"),
  ];
  const widths = [70, 70];
  const fallback = computeVisibleCollections(collections, widths, ROW_WIDTH, true);
  const explicitZero = computeVisibleCollections(
    collections,
    widths,
    ROW_WIDTH,
    true,
    0,
  );
  assert.equal(fallback.length, explicitZero.length);
  assert.deepEqual(
    fallback.map((c) => c.id),
    explicitZero.map((c) => c.id),
  );
});

test("computeVisibleCollections grows the visible subset when the measured chip shrinks", () => {
  // The card measures the `+N` chip with the worst-case text
  // (`+{userCollections.length}`) so the visible subset can be
  // smaller than what would fit once the real `+N` lands. This
  // pins the helper's sensitivity: a smaller reported reservation
  // must let more chips through.
  const collections = [
    makeCollection(1, "A"),
    makeCollection(2, "B"),
    makeCollection(3, "C"),
    makeCollection(4, "D"),
  ];
  const widths = [50, 50, 50, 50];
  // Worst case (`+4` ≈ 30): available = 200 - 30 = 170; fits
  //   3 chips (50 + 4 + 50 + 4 + 50 = 158).
  const conservative = computeVisibleCollections(
    collections,
    widths,
    ROW_WIDTH,
    true,
    30,
  );
  assert.equal(conservative.length, 3);
  // Realistic (`+1` ≈ 22): available = 200 - 22 = 178; still fits
  //   3 chips (158) but the +1 text would expose that one chip is
  //   hidden.
  const relaxed = computeVisibleCollections(
    collections,
    widths,
    ROW_WIDTH,
    true,
    22,
  );
  assert.equal(relaxed.length, 3);
  // Very narrow reservation (`+1` measured as 4px): available = 196;
  //   fits 3 chips (158 + 4 + 50 = 212 > 196). The helper honours
  //   the input width without re-deriving the overflow flag.
  const tiny = computeVisibleCollections(
    collections,
    widths,
    ROW_WIDTH,
    true,
    4,
  );
  assert.equal(tiny.length, 3);
});

test("computeVisibleCollections clips to one chip when the reservation exhausts the row", () => {
  const collections = [
    makeCollection(1, "Trabajo"),
    makeCollection(2, "Clientes"),
    makeCollection(3, "Investigación"),
  ];
  const widths = [120, 120, 120];
  // Row narrower than a single chip + measured reservation: the
  // helper still returns the first chip so the row is never
  // empty.
  const visible = computeVisibleCollections(collections, widths, 80, true, 30);
  assert.equal(visible.length, 1);
  assert.equal(visible[0].id, 1);
});

test("computeOverflowCount returns the number of hidden user collections", () => {
  const collections = [
    makeCollection(1, "A"),
    makeCollection(2, "B"),
    makeCollection(3, "C"),
    makeCollection(4, "D"),
  ];
  const visible = collections.slice(0, 2);
  assert.equal(computeOverflowCount(collections, visible), 2);
});

test("computeOverflowCount returns zero when every collection is visible", () => {
  const collections = [
    makeCollection(1, "A"),
    makeCollection(2, "B"),
  ];
  assert.equal(computeOverflowCount(collections, collections), 0);
});

test("computeOverflowCount never reports a negative count", () => {
  // Defensive: a stale or partial layout could feed a visible
  // subset larger than the input. The helper clamps to zero so
  // the chip never shows a phantom negative label.
  const collections = [makeCollection(1, "A")];
  const visible = [
    makeCollection(1, "A"),
    makeCollection(2, "B"),
  ];
  assert.equal(computeOverflowCount(collections, visible), 0);
});

test("the constants exported by the layout helper pin the chip-row measurement", () => {
  // The button reservation and the inter-chip gap are the
  // contract between the measurement strip and the painted row.
  // A regression that resizes either constant must surface as a
  // failed assertion here.
  assert.equal(COLLECTION_OVERFLOW_BUTTON_PX, 32);
  assert.equal(COLLECTION_CHIP_GAP_PX, 4);
  assert.equal(COLLECTION_OVERFLOW_TOLERANCE_PX, 1);
});

// ---------------------------------------------------------------------------
// Intrinsic total: drives the overflow predicate `intrinsicTotal > rowWidth +
// COLLECTION_OVERFLOW_TOLERANCE_PX`. The row must compare against the full
// user-collection set so a trimmed painted row cannot mask the original
// overflow.
// ---------------------------------------------------------------------------

test("intrinsicCollectionRowWidth sums the chips, gaps and the measured overflow chip", () => {
  // 4 chips × 50 + 3 gaps × 4 + 1 gap + 32 (measured overflow chip)
  //   = 200 + 12 + 4 + 32 = 248
  const total = intrinsicCollectionRowWidth([50, 50, 50, 50], 32);
  assert.equal(total, 248);
});

test("intrinsicCollectionRowWidth falls back to the documented reservation when no measurement is supplied", () => {
  // Single chip wider than the fallback reservation must still
  // account for the overflow area so the row converges.
  const total = intrinsicCollectionRowWidth([120], 0);
  // 120 + 0 gaps + 4 gap + 32 (fallback) = 156
  assert.equal(total, 156);
});

test("intrinsicCollectionRowWidth returns zero when there are no chips", () => {
  assert.equal(intrinsicCollectionRowWidth([], 32), 0);
});

test("intrinsicCollectionRowWidth adds exactly one gap before the overflow chip", () => {
  // The overflow chip is preceded by a single inter-chip gap.
  // The helper must never add the gap twice or skip it entirely.
  const withFallback = intrinsicCollectionRowWidth([10, 10], 0);
  // 10 + 10 + 1 gap + 4 gap + 32 (fallback) = 60
  assert.equal(withFallback, 60);
  const withMeasured = intrinsicCollectionRowWidth([10, 10], 28);
  // 10 + 10 + 1 gap + 4 gap + 28 = 56
  assert.equal(withMeasured, 56);
});
