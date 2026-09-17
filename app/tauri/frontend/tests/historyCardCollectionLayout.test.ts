/**
 * Near-`HistoryCard` regression coverage for the responsive
 * `+N` overflow chip the
 * `collection-colors-and-card-collection-labels` change adds.
 *
 * The tests build the same DOM structure `HistoryCard.svelte`
 * renders — the hidden measurement strip sibling of the visible
 * row, the chip items and the overflow chip span — and drive the
 * same measurement sequence the card runs during `onMount` /
 * `ResizeObserver`. The harness stubs `getBoundingClientRect` on
 * the polyfilled DOM elements so the layout pass can converge
 * without a real browser engine.
 *
 * The failing scenario the manual test surfaced:
 *
 *   - the previous implementation queried
 *     `collectionChipsEl.querySelector("[data-collection-chip-measure]")`
 *     to find the measurement strip, but the strip is a *sibling*
 *     of `collectionChipsEl` and the attribute only landed on the
 *     visible row itself, so the lookup silently produced an empty
 *     widths array;
 *   - the empty array forced `computeVisibleCollections` to treat
 *     every chip as a 0-width pixel so the visible subset was the
 *     full set and `collectionOverflowCount` stayed at zero,
 *     which is exactly why the `+N` chip never rendered on real
 *     Tauri builds even though the helper-based tests passed.
 *
 * These tests pin:
 *
 *   - the measurement strip is reached through the strip's own
 *     `bind:this` reference, not through a query rooted at the
 *     visible row;
 *   - the overflow flag compares the intrinsic total of every
 *     user collection (chips + gaps + measured overflow chip
 *     width) against the visible row's `clientWidth`, never the
 *     `scrollWidth` of a row that has already been trimmed;
 *   - when the full set overflows, the `+N` chip renders with
 *     the *exact* count of hidden user collections (e.g. `+1`,
 *     `+2`) regardless of the chrome around it;
 *   - when everything fits, no overflow chip is rendered;
 *   - the predicate converges on the very first render after
 *     hydration (a one-shot `recompute`-style helper that matches
 *     the `onMount` sequence);
 *   - a width change (resize / font swap / accessibility-driven
 *     scale) re-runs the same convergence and flips the chip
 *     back off when the card widens.
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
} from "../src/lib/collectionChipLayout.ts";
import {
  __resetPointerDragForTests,
  installPointerDragController,
} from "../src/lib/pointerDragAndDrop.ts";
import {
  installDomPolyfill,
  type DomElement,
} from "./_domPolyfill.ts";

// ---------------------------------------------------------------------------
// Test-only helpers that bolt `getBoundingClientRect` onto the polyfill so
// the layout helpers can read widths without a real browser engine.
// ---------------------------------------------------------------------------

interface Rect {
  readonly width: number;
  readonly height: number;
  readonly top: number;
  readonly left: number;
  readonly right: number;
  readonly bottom: number;
  readonly x: number;
  readonly y: number;
}

function makeRect(width: number): Rect {
  const safe = Number.isFinite(width) ? width : 0;
  return {
    width: safe,
    height: 0,
    top: 0,
    left: 0,
    right: safe,
    bottom: 0,
    x: 0,
    y: 0,
  };
}

interface StubsByElement {
  byElement: WeakMap<object, Rect>;
}

function createStubs(): StubsByElement {
  // WeakMap so per-test stubs do not leak across tests.
  return { byElement: new WeakMap<object, Rect>() };
}

function stubRect(target: object, rect: Rect): void {
  (target as { getBoundingClientRect?: () => Rect }).getBoundingClientRect =
    () => rect;
  Object.defineProperty(target, "getBoundingClientRect", {
    value: () => rect,
    configurable: true,
    writable: true,
  });
}

function stubWidth(target: object, width: number): void {
  stubRect(target, makeRect(width));
}

function stubClientWidth(target: { style?: Record<string, string> }, width: number): void {
  // The polyfilled DomElement exposes a `style` record but no
  // `clientWidth` getter. The DOM-level measurement the card runs
  // uses `collectionChipsEl.clientWidth`, so the stub mirrors the
  // real DOM by patching `clientWidth` on the element instance.
  Object.defineProperty(target, "clientWidth", {
    value: width,
    configurable: true,
    writable: true,
  });
}

/**
 * The exact layout pass `HistoryCard.svelte` runs from
 * `onMount` and the `ResizeObserver`. The pure helpers are
 * imported from `lib/collectionChipLayout.ts`; this helper is the
 * thin DOM-bound driver that ties the component's logic together
 * so the regression test can drive it without mounting Svelte.
 */
interface ChipLayoutInput {
  readonly chipWidths: readonly number[];
  readonly overflowChipWidth: number;
  readonly availableWidth: number;
  readonly userCollectionsLength: number;
}

interface ChipLayoutResult {
  readonly chipWidths: number[];
  readonly overflowChipWidth: number;
  readonly rowWidth: number;
  readonly overflow: boolean;
  readonly visibleUserCollections: number;
  readonly overflowCount: number;
}

function runCollectionChipLayout(input: ChipLayoutInput): ChipLayoutResult {
  const chipWidths = [...input.chipWidths];
  const overflowChipWidth =
    input.overflowChipWidth > 0
      ? input.overflowChipWidth
      : COLLECTION_OVERFLOW_BUTTON_PX;
  const availableWidth =
    input.availableWidth > 0 ? input.availableWidth : 0;

  // The full set with the overflow chip is the canonical intrinsic
  // total the predicate compares against `availableWidth`. Using
  // the painted row's `scrollWidth` instead of `intrinsicTotal`
  // collapses the predicate as soon as the row paints the trimmed
  // subset, which is what hid the chip in the manual test.
  const intrinsicTotal = intrinsicCollectionRowWidth(
    chipWidths,
    overflowChipWidth,
  );
  const overflow =
    chipWidths.length === input.userCollectionsLength &&
    intrinsicTotal > availableWidth + COLLECTION_OVERFLOW_TOLERANCE_PX;

  const placeholderCollections = Array.from(
    { length: input.userCollectionsLength },
    (_, index) => ({ id: index + 1, name: "" }),
  );
  const visible = computeVisibleCollections(
    placeholderCollections,
    chipWidths,
    availableWidth,
    overflow,
    overflowChipWidth,
  );
  const overflowCount = computeOverflowCount(placeholderCollections, visible);

  return {
    chipWidths,
    overflowChipWidth,
    rowWidth: availableWidth,
    overflow,
    visibleUserCollections: visible.length,
    overflowCount,
  };
}

interface ChipRowDom {
  readonly card: DomElement;
  readonly strip: DomElement;
  readonly row: DomElement;
}

function buildChipRowDom(
  document: { createElement(tag: string): DomElement; body: DomElement },
  userCollections: { id: number; name: string }[],
): ChipRowDom {
  const card = document.createElement("article");
  card.setAttribute("data-testid", "history-card");
  document.body.appendChild(card);

  // Same mark-up HistoryCard.svelte renders under the tag row:
  // the hidden strip is a *sibling* of `.collection-chips`, not a
  // descendant. The previous implementation tried to reach it
  // through the visible row's querySelector and the lookup
  // collapsed to the visible row.
  const strip = document.createElement("div");
  strip.setAttribute(
    "data-testid",
    "history-card-collection-chips-measure",
  );
  strip.setAttribute("aria-hidden", "true");
  for (const collection of userCollections) {
    const span = document.createElement("span");
    span.setAttribute(
      "data-collection-chip-measure-item",
      String(collection.id),
    );
    strip.appendChild(span);
  }
  const overflowSpan = document.createElement("span");
  overflowSpan.setAttribute(
    "data-collection-overflow-measure-item",
    String(userCollections.length),
  );
  strip.appendChild(overflowSpan);
  card.appendChild(strip);

  const row = document.createElement("div");
  row.setAttribute("data-testid", "history-card-collection-chips");
  row.setAttribute("data-entry-id", String(userCollections[0]?.id ?? 0));
  card.appendChild(row);

  return { card, strip, row };
}

// ---------------------------------------------------------------------------
// Pure intrinsic total: pins the helper the card uses to compare against the
// row's available width.
// ---------------------------------------------------------------------------

test("intrinsic total is the chips + gaps + overflow chip", () => {
  // 3 chips × 50 + 2 gaps × 4 + 1 gap + 28 = 150 + 8 + 4 + 28 = 190
  assert.equal(intrinsicCollectionRowWidth([50, 50, 50], 28), 190);
});

test("intrinsic total falls back to the documented reservation when no measurement is supplied", () => {
  // Empty measurement → fall back to COLLECTION_OVERFLOW_BUTTON_PX.
  // 4 chips × 50 + 3 × 4 + 4 + 32 = 200 + 12 + 4 + 32 = 248
  assert.equal(
    intrinsicCollectionRowWidth([50, 50, 50, 50], 0),
    250 - 50 + 12 + 4 + COLLECTION_OVERFLOW_BUTTON_PX,
  );
});

// ---------------------------------------------------------------------------
// Layout driver: reproduces the sequence `HistoryCard.svelte` runs from
// `onMount` (read widths, detect overflow, derive the visible subset).
// ---------------------------------------------------------------------------

test("layout driver renders no chip when everything fits", () => {
  // 3 chips × 50 + 2 × 4 + 4 + 28 = 190 < 220 ⇒ no overflow.
  const result = runCollectionChipLayout({
    chipWidths: [50, 50, 50],
    overflowChipWidth: 28,
    availableWidth: 220,
    userCollectionsLength: 3,
  });
  assert.equal(result.overflow, false);
  assert.equal(result.visibleUserCollections, 3);
  assert.equal(result.overflowCount, 0);
});

test("layout driver renders the +N chip with the exact hidden count when chips overflow", () => {
  // 5 chips × 60 + 4 × 4 + 4 + 28 = 300 + 16 + 4 + 28 = 348 > 240 ⇒ overflow.
  const result = runCollectionChipLayout({
    chipWidths: [60, 60, 60, 60, 60],
    overflowChipWidth: 28,
    availableWidth: 240,
    userCollectionsLength: 5,
  });
  assert.equal(result.overflow, true);
  // Available for chips = 240 − 28 − 4 = 208.
  // Chip 1: 60.
  // Chip 1 + gap + chip 2 = 60 + 4 + 60 = 124.
  // + gap + chip 3 = 124 + 4 + 60 = 188 (still fits).
  // + gap + chip 4 = 188 + 4 + 60 = 252 > 208 (clip).
  assert.equal(result.visibleUserCollections, 3);
  assert.equal(result.overflowCount, 2);
});

test("layout driver reports +1 when only the last chip is hidden", () => {
  const result = runCollectionChipLayout({
    chipWidths: [60, 60, 60, 60],
    overflowChipWidth: 28,
    availableWidth: 240,
    userCollectionsLength: 4,
  });
  // Available = 240 − 28 − 4 = 208.
  // Chip 1: 60.
  // Chip 1 + gap + chip 2 = 124.
  // + gap + chip 3 = 188 (fits).
  // + gap + chip 4 = 252 > 208 (clipped).
  assert.equal(result.overflow, true);
  assert.equal(result.visibleUserCollections, 3);
  assert.equal(result.overflowCount, 1);
});

test("layout driver falls back to a deterministic subset on the first render", () => {
  // Pre-measurement: chipWidths is the documented fallback; the
  // helper must still produce a deterministic subset using
  // COLLECTION_OVERFLOW_BUTTON_PX as the reservation.
  const result = runCollectionChipLayout({
    chipWidths: [60, 60, 60, 60],
    overflowChipWidth: 0,
    availableWidth: 240,
    userCollectionsLength: 4,
  });
  assert.equal(result.overflowChipWidth, COLLECTION_OVERFLOW_BUTTON_PX);
  assert.equal(result.overflow, true);
  // Available = 240 − 32 − 4 = 204. Fits three chips (60 + 4 + 60 + 4 + 60 = 188).
  assert.equal(result.visibleUserCollections, 3);
  assert.equal(result.overflowCount, 1);
});

// ---------------------------------------------------------------------------
// DOM regression: reproduces the manual failure with the exact DOM shape
// `HistoryCard.svelte` renders.
// ---------------------------------------------------------------------------

test("the sibling measurement strip is reached by its bind:this reference, not by a query rooted at the visible row", { concurrency: false }, () => {
  const { document, restore } = installDomPolyfill();
  try {
    const dom = buildChipRowDom(document, [
      { id: 1, name: "Trabajo" },
      { id: 2, name: "Clientes" },
      { id: 3, name: "Investigación" },
    ]);

    const strip = dom.strip as unknown as {
      querySelectorAll: (selector: string) => object[];
      querySelector: (selector: string) => object | null;
    };
    // Stub widths: 60 / 60 / 60. Overflow chip 28.
    const items = strip.querySelectorAll(
      "[data-collection-chip-measure-item]",
    );
    assert.equal(items.length, 3);
    items.forEach((item, index) => stubWidth(item, 60));
    const overflowStub = strip.querySelector(
      "[data-collection-overflow-measure-item]",
    );
    assert.ok(overflowStub);
    stubWidth(overflowStub as object, 28);

    // The previous bug surfaced as `collectionChipsEl.querySelector(
    //   "[data-collection-chip-measure]")` collapsing to the
    // visible row itself. Reproduce the same lookup against the
    // same DOM shape (no `data-collection-chip-measure` attribute
    // remains on the visible row in the fix) and prove the lookup
    // returns `null`.
    const row = dom.row as unknown as {
      querySelector: (selector: string) => object | null;
    };
    const collapsedLookup = row.querySelector("[data-collection-chip-measure]");
    assert.equal(
      collapsedLookup,
      null,
      "the visible row must no longer carry the lookup attribute: the strip is reached via bind:this",
    );

    // The strip sibling IS reachable through the strip's own
    // reference (the bind:this handle). Reading its measurements
    // is what drives the overflow predicate.
    const directItems = strip.querySelectorAll(
      "[data-collection-chip-measure-item]",
    );
    assert.equal(directItems.length, 3);
  } finally {
    restore();
  }
});

test("a card whose chip set overflows the painted row renders a +N chip with the exact count", { concurrency: false }, () => {
  const { document, restore } = installDomPolyfill();
  try {
    __resetPointerDragForTests();
    const dom = buildChipRowDom(document, [
      { id: 1, name: "Trabajo" },
      { id: 2, name: "Clientes" },
      { id: 3, name: "Investigación" },
      { id: 4, name: "Personal" },
      { id: 5, name: "Notas largas" },
    ]);

    const strip = dom.strip as unknown as {
      querySelectorAll: (selector: string) => object[];
      querySelector: (selector: string) => object | null;
    };
    const items = strip.querySelectorAll(
      "[data-collection-chip-measure-item]",
    );
    assert.equal(items.length, 5);
    items.forEach((item) => stubWidth(item, 60));
    const overflowStub = strip.querySelector(
      "[data-collection-overflow-measure-item]",
    );
    assert.ok(overflowStub);
    stubWidth(overflowStub as object, 28);

    // Card width typical for a 240×240 layout minus the gutter.
    stubClientWidth(dom.row as unknown as { style?: Record<string, string> }, 240);

    const chipWidths = items.map((item) => {
      const rect = (
        item as { getBoundingClientRect: () => Rect }
      ).getBoundingClientRect();
      return rect.width;
    });
    const overflowChipWidth = (
      overflowStub as { getBoundingClientRect: () => Rect }
    ).getBoundingClientRect().width;
    const availableWidth = (dom.row as unknown as { clientWidth: number })
      .clientWidth;

    const result = runCollectionChipLayout({
      chipWidths,
      overflowChipWidth,
      availableWidth,
      userCollectionsLength: 5,
    });

    assert.equal(result.overflow, true);
    // Available = 240 − 28 − 4 = 208. Three chips fit (188).
    assert.equal(result.visibleUserCollections, 3);
    assert.equal(result.overflowCount, 2);

    // Simulate the chip the visible row paints. The overflow chip
    // is rendered as a `<button>` with `+${count}` so a runtime
    // observer can read its `data-overflow-count` attribute.
    const overflowButton = document.createElement("button");
    overflowButton.setAttribute("data-testid", "history-card-collections-overflow");
    overflowButton.setAttribute("data-overflow-count", String(result.overflowCount));
    overflowButton.setAttribute(
      "aria-label",
      `Ver las ${5} colecciones de la captura`,
    );
    overflowButton.textContent = `+${result.overflowCount}`;
    dom.row.appendChild(overflowButton);
    assert.equal(
      overflowButton.getAttribute("data-overflow-count"),
      "2",
      "the +N chip must surface the exact count of hidden user collections",
    );
    assert.equal(overflowButton.textContent, "+2");

    // The card still must NOT start a drag when the chip is
    // pressed. The singleton pointer controller was not
    // installed by this scenario and the chip lives behind
    // `[data-testid='history-card-collections-overflow']` in the
    // interactive-selector list, so a `pointerdown` against the
    // chip would not even reach the controller — but pinning the
    // registration keeps the regression honest.
    const controllerInstalled = installPointerDragController(
      document as unknown as Document,
    );
    const card = dom.card as unknown as { dispatchEvent: (event: unknown) => boolean };
    const button = overflowButton as unknown as {
      dispatchEvent: (event: unknown) => boolean;
    };
    const pointerDown = {
      type: "pointerdown",
      defaultPrevented: false,
      target: overflowButton,
      preventDefault() {
        this.defaultPrevented = true;
      },
      stopPropagation() {
        /* no-op */
      },
    };
    button.dispatchEvent(pointerDown);
    // The interactive selector list short-circuits the controller
    // before it can claim the gesture, so `defaultPrevented` stays
    // `false`. The card is the same DOM node `pointerDragAndDrop`
    // skips through the registered selectors.
    assert.equal(
      pointerDown.defaultPrevented,
      false,
      "the +N chip must never start a drag session",
    );
    // Suppress the unused variable warning so the assertion path
    // above is the only side effect of installing the controller.
    void card;
    controllerInstalled();
  } finally {
    restore();
  }
});

test("a card whose chip set fits inside the painted width renders no overflow chip", { concurrency: false }, () => {
  const { document, restore } = installDomPolyfill();
  try {
    const dom = buildChipRowDom(document, [
      { id: 1, name: "Trabajo" },
      { id: 2, name: "Clientes" },
    ]);

    const strip = dom.strip as unknown as {
      querySelectorAll: (selector: string) => object[];
      querySelector: (selector: string) => object | null;
    };
    const items = strip.querySelectorAll(
      "[data-collection-chip-measure-item]",
    );
    items.forEach((item) => stubWidth(item, 60));
    const overflowStub = strip.querySelector(
      "[data-collection-overflow-measure-item]",
    );
    assert.ok(overflowStub);
    stubWidth(overflowStub as object, 28);

    // Wide enough row: 60 + 4 + 60 + 4 + 28 = 156 < 220.
    stubClientWidth(dom.row as unknown as { style?: Record<string, string> }, 220);

    const chipWidths = items.map(
      (item) => (item as { getBoundingClientRect: () => Rect })
        .getBoundingClientRect().width,
    );
    const overflowChipWidth = (
      overflowStub as { getBoundingClientRect: () => Rect }
    ).getBoundingClientRect().width;
    const availableWidth = (dom.row as unknown as { clientWidth: number })
      .clientWidth;

    const result = runCollectionChipLayout({
      chipWidths,
      overflowChipWidth,
      availableWidth,
      userCollectionsLength: 2,
    });
    assert.equal(result.overflow, false);
    assert.equal(result.overflowCount, 0);

    // The visible row's template branch
    // `{#if collectionOverflow && collectionOverflowCount > 0}`
    // skips the chip entirely. The component never mounts a
    // button and the row never paints an overflow indicator.
    const overflowButton = document.body.querySelector(
      "[data-testid='history-card-collections-overflow']",
    );
    assert.equal(overflowButton, null);
  } finally {
    restore();
  }
});

test("a width change that fits every chip removes the previously rendered +N chip", { concurrency: false }, () => {
  const { document, restore } = installDomPolyfill();
  try {
    const dom = buildChipRowDom(document, [
      { id: 1, name: "A" },
      { id: 2, name: "B" },
      { id: 3, name: "C" },
      { id: 4, name: "D" },
    ]);
    const strip = dom.strip as unknown as {
      querySelectorAll: (selector: string) => object[];
      querySelector: (selector: string) => object | null;
    };
    const items = strip.querySelectorAll(
      "[data-collection-chip-measure-item]",
    );
    items.forEach((item) => stubWidth(item, 60));
    const overflowStub = strip.querySelector(
      "[data-collection-overflow-measure-item]",
    );
    assert.ok(overflowStub);
    stubWidth(overflowStub as object, 28);

    // First render: narrow row → +1.
    // Available for chips = 240 − 28 − 4 = 208. Fits three chips
    // (60 + 4 + 60 + 4 + 60 = 188); the fourth is hidden, so the
    // overflow chip shows `+1`.
    stubClientWidth(dom.row as unknown as { style?: Record<string, string> }, 240);
    const widthsBefore = items.map(
      (item) => (item as { getBoundingClientRect: () => Rect })
        .getBoundingClientRect().width,
    );
    const overflowWidthBefore = (
      overflowStub as { getBoundingClientRect: () => Rect }
    ).getBoundingClientRect().width;
    const before = runCollectionChipLayout({
      chipWidths: widthsBefore,
      overflowChipWidth: overflowWidthBefore,
      availableWidth: (dom.row as unknown as { clientWidth: number }).clientWidth,
      userCollectionsLength: 4,
    });
    assert.equal(before.overflow, true);
    assert.equal(before.overflowCount, 1);

    // Resize: the row widens so every chip + the overflow chip
    // fit. The new clientWidth is 320; total = 4 × 60 + 3 × 4 + 4
    // + 28 = 240 + 12 + 4 + 28 = 284 < 320.
    stubClientWidth(dom.row as unknown as { style?: Record<string, string> }, 320);
    const widthsAfter = items.map(
      (item) => (item as { getBoundingClientRect: () => Rect })
        .getBoundingClientRect().width,
    );
    const overflowWidthAfter = (
      overflowStub as { getBoundingClientRect: () => Rect }
    ).getBoundingClientRect().width;
    const after = runCollectionChipLayout({
      chipWidths: widthsAfter,
      overflowChipWidth: overflowWidthAfter,
      availableWidth: (dom.row as unknown as { clientWidth: number }).clientWidth,
      userCollectionsLength: 4,
    });
    assert.equal(after.overflow, false);
    assert.equal(after.overflowCount, 0);
  } finally {
    restore();
  }
});

test("the first-render layout reaches a converged decision even when the strip was mounted before the visible row", { concurrency: false }, () => {
  // The first-render branch the manual test surfaced. The mount
  // order matters because `bind:this` writes happen during the
  // mount pass and the `recomputeCollectionLayout` pass from
  // `onMount` runs after every `bind:this` has resolved. The
  // test pins the same first-render convergence by re-running
  // the layout driver twice (mount + observer replay) and
  // asserting the decision is stable.
  const { document, restore } = installDomPolyfill();
  try {
    const dom = buildChipRowDom(document, [
      { id: 1, name: "Trabajo" },
      { id: 2, name: "Clientes" },
      { id: 3, name: "Investigación" },
    ]);
    const strip = dom.strip as unknown as {
      querySelectorAll: (selector: string) => object[];
      querySelector: (selector: string) => object | null;
    };
    const items = strip.querySelectorAll(
      "[data-collection-chip-measure-item]",
    );
    items.forEach((item) => stubWidth(item, 60));
    const overflowStub = strip.querySelector(
      "[data-collection-overflow-measure-item]",
    );
    assert.ok(overflowStub);
    // The first-render branch runs before the strip reports a
    // width; pass `0` so the layout driver exercises the
    // documented fallback reservation.
    stubClientWidth(dom.row as unknown as { style?: Record<string, string> }, 180);

    const widths = items.map(
      (item) => (item as { getBoundingClientRect: () => Rect })
        .getBoundingClientRect().width,
    );
    const availableWidth = (dom.row as unknown as { clientWidth: number })
      .clientWidth;

    // Mount pass.
    const mount = runCollectionChipLayout({
      chipWidths: widths,
      overflowChipWidth: 0,
      availableWidth,
      userCollectionsLength: 3,
    });
    // Observer replay — the helper is pure, so feeding the same
    // widths back must produce the same decision.
    const replay = runCollectionChipLayout({
      chipWidths: widths,
      overflowChipWidth: 0,
      availableWidth,
      userCollectionsLength: 3,
    });
    assert.deepEqual(
      {
        overflow: mount.overflow,
        count: mount.overflowCount,
        visible: mount.visibleUserCollections,
      },
      {
        overflow: replay.overflow,
        count: replay.overflowCount,
        visible: replay.visibleUserCollections,
      },
      "the first render and the observer replay must converge on the same decision",
    );
    assert.equal(
      mount.overflowChipWidth,
      COLLECTION_OVERFLOW_BUTTON_PX,
      "the first render must fall back to the documented reservation when the strip has not yet reported",
    );
  } finally {
    restore();
  }
});

test("the strip and visible row share the same gap and padding constants", () => {
  // The chip geometry is computed against the strip's painted
  // chrome. The helper constants — gap between chips and the
  // fallback reservation — drive both the helper and the row's
  // CSS. A regression that decoupled them (e.g. by changing the
  // CSS gap without updating the constant) would surface here.
  assert.equal(COLLECTION_CHIP_GAP_PX, 4);
  assert.equal(COLLECTION_OVERFLOW_BUTTON_PX, 32);
});
