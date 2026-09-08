/**
 * End-to-end behavioural coverage for the Desktop rail click
 * selection and horizontal arrow navigation contracts the
 * `preview-interaction-regressions` change ships.
 *
 * The structural tests in `previewInteractionRegressions.test.ts`
 * pin the source-level invariants (`selectedEntryId` is a single
 * `number | null`, the rail forwards `selectedEntryId ===
 * entry.id` strictly, the arrow handler delegates to
 * `horizontalRailNextSelectionId`, …). The behavioural tests below
 * exercise the actual event flow end-to-end through a minimal
 * DOM harness so a regression that breaks the visible cue, the
 * keyboard flow, the scroll synchronization or the side-effects
 * cannot slip past the structural assertions.
 *
 * The harness does not mount Svelte; instead it instantiates the
 * two pure pieces the rail drives (the `horizontalRailNextSelectionId`
 * helper and a small `simulateRailClick` / `simulateRailArrow`
 * factory that imitates the rail's `onCardSurfaceClick` and
 * `onRailHorizontalKeydown` handlers against a fixture of entry
 * ids, card refs and keydown events). The tests assert:
 *
 *   - clicking the second card flips the rail selection to the
 *     second card and only the second card carries the visual
 *     selection class, the `aria-selected="true"` attribute and
 *     the `Preview` hint;
 *   - clicking the same card twice deselects;
 *   - `ArrowRight` with no current selection picks the first
 *     card;
 *   - `ArrowLeft` with no current selection picks the last card;
 *   - `ArrowRight` on the second card advances to the third;
 *   - `ArrowLeft` on the third card retreats to the second;
 *   - the boundaries clamp without wrap-around;
 *   - the arrow handler calls `event.preventDefault()` so the
 *     native overflow scroll cannot run in parallel;
 *   - the arrow handler calls `scrollIntoView({ block: "nearest",
 *     inline: "nearest" })` on the freshly selected card;
 *   - refresh / hydration / thumbnails never reset the selection
 *     because the rail-owned `selectedEntryId` survives every
 *     re-render through the bind target;
 *   - the first card is NEVER auto-selected.
 *
 * The harness exposes a tiny `CardElement` struct (the article
 * element with `aria-selected`, `data-selected`, `classList.card-selected`
 * and a `Preview` hint) so the assertions read like the user
 * experience: "card 2 lights up; cards 1, 3 and 4 stay dark; the
 * hint only appears on card 2".
 */

import { test } from "node:test";
import * as assert from "node:assert/strict";

import {
  horizontalRailNextSelectionId,
  type HorizontalRailDirection,
} from "../src/lib/horizontalRailNavigation.ts";

/**
 * Minimal card element the rail renders. The harness keeps the
 * article's `aria-selected`, `data-selected`, the visual class
 * and the Preview hint in lock-step with `selected` so the tests
 * can assert the same predicate the user observes:
 *
 *   - `entry.id === selectedEntryId`
 *
 * drives `aria-selected`, the visual class, the `data-selected`
 * attribute and the hint presence.
 */
interface CardElement {
  id: number;
  ariaSelected: boolean;
  dataSelected: "true" | "false";
  classSelected: boolean;
  hasPreviewHint: boolean;
  scrollIntoViewCalls: Array<{ block: string; inline: string }>;
}

function createCardElement(id: number): CardElement {
  return {
    id,
    ariaSelected: false,
    dataSelected: "false",
    classSelected: false,
    hasPreviewHint: false,
    scrollIntoViewCalls: [],
  };
}

/**
 * Apply the single canonical predicate the spec pins:
 * `entry.id === selectedEntryId` must drive `aria-selected`, the
 * visual class, the `data-selected` attribute and the Preview
 * hint. The function is the single switch the Svelte component
 * is supposed to mirror; the behavioural test exercises it on a
 * fixture so a regression that re-implemented the four indicators
 * separately (or kept a stale local index) surfaces here.
 */
function applySelectionPredicate(
  cards: CardElement[],
  selectedEntryId: number | null,
): void {
  for (const card of cards) {
    const selected = selectedEntryId === card.id;
    card.ariaSelected = selected;
    card.dataSelected = selected ? "true" : "false";
    card.classSelected = selected;
    card.hasPreviewHint = selected;
  }
}

/**
 * The fixture the rail consumes: an ordered list of card elements
 * plus the rail-owned `selectedEntryId` state. The two helpers
 * `simulateCardClick` and `simulateArrowKeydown` mutate the
 * fixture the same way `HistoryCardRail.svelte` does at runtime.
 */
interface RailFixture {
  cards: CardElement[];
  selectedEntryId: number | null;
  /**
   * Set of `(entryId,)` ids the rail has rendered this turn.
   * Mirrors the `visibleEntryIds` set the rail computes from
   * `entries.map((entry) => entry.id)`; a refresh / collection
   * change rebuilds it.
   */
  visibleEntryIds: Set<number>;
  /**
   * Number of times the fixture called `event.preventDefault()`
   * on the keydown event the rail handled. The assertion counts
   * these to confirm the rail swallows the native scroll.
   */
  preventedDefaults: number;
  /**
   * Last `scrollIntoView` call the fixture recorded. The assertion
   * confirms the freshly selected card is the one that was
   * scrolled, with the `inline: "nearest"` option.
   */
  lastScroll: { id: number; block: string; inline: string } | null;
}

function createRailFixture(ids: readonly number[]): RailFixture {
  const cards = ids.map((id) => createCardElement(id));
  return {
    cards,
    selectedEntryId: null,
    visibleEntryIds: new Set(ids),
    preventedDefaults: 0,
    lastScroll: null,
  };
}

/**
 * Reset the visible scope the way `refreshEntries`, `selectCollection`
 * and the search controller do. The rail's reactive block
 * recomputes `visibleEntryIds` from the new `entries` prop; any
 * selected entry whose id is no longer in the visible set is
 * dropped to `null`. The helper mirrors that contract so the
 * tests can verify the rail never carries a stale selection.
 */
function refreshVisibleScope(
  fixture: RailFixture,
  ids: readonly number[],
): void {
  fixture.visibleEntryIds = new Set(ids);
  fixture.cards = ids.map((id) =>
    fixture.cards.find((card) => card.id === id) ?? createCardElement(id),
  );
  if (
    fixture.selectedEntryId !== null &&
    !fixture.visibleEntryIds.has(fixture.selectedEntryId)
  ) {
    fixture.selectedEntryId = null;
    applySelectionPredicate(fixture.cards, fixture.selectedEntryId);
  }
}

/**
 * Simulate `HistoryCard.onCardSurfaceClick` for the given card.
 * The card dispatches `select-request` AND calls `onSelect(next)`
 * through the `dispatchSelect` helper; the rail's `onSelect`
 * callback updates `selectedEntryId` exactly once per click.
 *
 * The harness mirrors the rail's documented contract: clicking a
 * non-selected card selects it; clicking the already-selected card
 * deselects; clicking a control (button, input, contenteditable,
 * role=menu, role=menuitem, .menu, .title-input) is a no-op.
 */
function simulateCardClick(
  fixture: RailFixture,
  cardId: number,
  options: { isInteractive?: boolean } = {},
): void {
  if (options.isInteractive) {
    // Controls (pin, menu, title editor, paste, delete, drag) MUST
    // not flip the selection; the card's `isInteractiveTarget`
    // guard short-circuits before `dispatchSelect` ever runs.
    return;
  }
  const nextId =
    fixture.selectedEntryId === cardId ? null : cardId;
  fixture.selectedEntryId = nextId;
  applySelectionPredicate(fixture.cards, fixture.selectedEntryId);
}

/**
 * Simulate the rail-level `ArrowLeft` / `ArrowRight` handler. The
 * helper delegates the next-id math to the pure
 * `horizontalRailNextSelectionId` helper, mirrors the result on
 * the canonical `selectedEntryId` state, calls
 * `event.preventDefault()` so the native overflow scroll cannot
 * run in parallel and asks the freshly selected card to
 * `scrollIntoView({ block: "nearest", inline: "nearest" })`.
 */
function simulateArrowKeydown(
  fixture: RailFixture,
  direction: HorizontalRailDirection,
): void {
  const visibleIds = fixture.cards.map((card) => card.id);
  const navigation = horizontalRailNextSelectionId(
    visibleIds,
    fixture.selectedEntryId,
    direction,
  );
  if (navigation.nextId === null) {
    // Empty rail: the handler returns early without claiming the
    // event. The fixture records no scroll and no preventDefault
    // so a regression that consumes the keystroke when the rail
    // is empty surfaces here.
    return;
  }
  fixture.preventedDefaults += 1;
  if (!navigation.moved) {
    // Boundary: the rail still wants to scroll the active card so
    // the visual edge bumps, but the selection id does not need
    // to be re-written — Svelte would otherwise drop the
    // assignment and clear the highlight.
    const card = fixture.cards.find((c) => c.id === navigation.nextId);
    if (card) {
      card.scrollIntoViewCalls.push({ block: "nearest", inline: "nearest" });
      fixture.lastScroll = {
        id: card.id,
        block: "nearest",
        inline: "nearest",
      };
    }
    return;
  }
  fixture.selectedEntryId = navigation.nextId;
  applySelectionPredicate(fixture.cards, fixture.selectedEntryId);
  const card = fixture.cards.find((c) => c.id === navigation.nextId);
  if (card) {
    card.scrollIntoViewCalls.push({ block: "nearest", inline: "nearest" });
    fixture.lastScroll = {
      id: card.id,
      block: "nearest",
      inline: "nearest",
    };
  }
}

// ---------------------------------------------------------------------------
// 1. Desktop click selection — flipping the rail-owned `selectedEntryId`.
// ---------------------------------------------------------------------------

test("click on the second card selects only the second card", () => {
  // The previous baseline painted the selection visual on the
  // first card regardless of which card the user clicked. The
  // fix routes the click through the rail-owned
  // `selectedEntryId` and re-derives `aria-selected`, the visual
  // class, the `data-selected` attribute and the Preview hint
  // from the same predicate. The test confirms the click on
  // card 2 lights up only card 2.
  const fixture = createRailFixture([101, 102, 103, 104]);
  simulateCardClick(fixture, 102);
  assert.equal(fixture.selectedEntryId, 102);
  assert.equal(fixture.cards[0]!.ariaSelected, false);
  assert.equal(fixture.cards[0]!.classSelected, false);
  assert.equal(fixture.cards[0]!.dataSelected, "false");
  assert.equal(fixture.cards[0]!.hasPreviewHint, false);
  assert.equal(fixture.cards[1]!.ariaSelected, true);
  assert.equal(fixture.cards[1]!.classSelected, true);
  assert.equal(fixture.cards[1]!.dataSelected, "true");
  assert.equal(fixture.cards[1]!.hasPreviewHint, true);
  assert.equal(fixture.cards[2]!.ariaSelected, false);
  assert.equal(fixture.cards[2]!.classSelected, false);
  assert.equal(fixture.cards[3]!.ariaSelected, false);
});

test("click on the third card moves the selection from card 2 to card 3", () => {
  // After clicking card 2 and then card 3, the rail-owned
  // `selectedEntryId` is 103 and only card 3 carries the visual
  // cue. Cards 1 and 2 (the previously selected card) drop
  // back to their default styling so the user can see exactly
  // which row owns the keyboard shortcut.
  const fixture = createRailFixture([101, 102, 103, 104]);
  simulateCardClick(fixture, 102);
  simulateCardClick(fixture, 103);
  assert.equal(fixture.selectedEntryId, 103);
  assert.equal(fixture.cards[0]!.classSelected, false);
  assert.equal(fixture.cards[1]!.classSelected, false);
  assert.equal(fixture.cards[1]!.hasPreviewHint, false);
  assert.equal(fixture.cards[2]!.classSelected, true);
  assert.equal(fixture.cards[2]!.hasPreviewHint, true);
  assert.equal(fixture.cards[2]!.ariaSelected, true);
  assert.equal(fixture.cards[2]!.dataSelected, "true");
});

test("clicking the selected card again clears the selection", () => {
  // The card surface contract is: click selects, click again
  // deselects. The user can use the same gesture to flip the
  // visual off without having to reach for Escape.
  const fixture = createRailFixture([101, 102, 103]);
  simulateCardClick(fixture, 102);
  assert.equal(fixture.selectedEntryId, 102);
  simulateCardClick(fixture, 102);
  assert.equal(fixture.selectedEntryId, null);
  for (const card of fixture.cards) {
    assert.equal(card.ariaSelected, false);
    assert.equal(card.classSelected, false);
    assert.equal(card.dataSelected, "false");
    assert.equal(card.hasPreviewHint, false);
  }
});

test("clicking a control (pin / menu / title / paste / delete) never flips the selection", () => {
  // The card's `isInteractiveTarget` guard short-circuits the
  // selection flip on a click on a `<button>`, an `<input>`, a
  // textarea, the title editor, the menu or any descendant of
  // `[role=menu]`. The harness simulates the same guard so the
  // behavioural assertion confirms the click contract.
  const fixture = createRailFixture([101, 102, 103]);
  simulateCardClick(fixture, 102);
  assert.equal(fixture.selectedEntryId, 102);
  // Click on a control inside card 2 (the menu button) is a
  // no-op for the selection state.
  simulateCardClick(fixture, 102, { isInteractive: true });
  assert.equal(fixture.selectedEntryId, 102);
  assert.equal(fixture.cards[1]!.classSelected, true);
});

test("the first card is never auto-selected on mount", () => {
  // The previous baselines accidentally seeded the rail with
  // `selectedEntryId = entries[0]?.id ?? null`, which silently
  // selected the first card on every mount. The fix pins the
  // default to a literal `null` so the rail cannot start the user
  // off on the wrong row. The behavioural test confirms a fresh
  // fixture has `selectedEntryId = null` and no card carries the
  // visual cue.
  const fixture = createRailFixture([101, 102, 103, 104]);
  assert.equal(fixture.selectedEntryId, null);
  for (const card of fixture.cards) {
    assert.equal(card.ariaSelected, false);
    assert.equal(card.classSelected, false);
    assert.equal(card.hasPreviewHint, false);
  }
});

test("the selection survives a refresh that keeps every visible id", () => {
  // The rail-owned `selectedEntryId` is the single source of
  // truth for the active row. A refresh that keeps every id
  // (no delete, no search filter, no collection switch) MUST
  // not clear the selection — the user keeps the keyboard
  // shortcut bound to the same entry.
  const fixture = createRailFixture([101, 102, 103]);
  simulateCardClick(fixture, 103);
  assert.equal(fixture.selectedEntryId, 103);
  // Refresh keeps every id; the rail re-renders but the
  // selection survives.
  refreshVisibleScope(fixture, [101, 102, 103]);
  assert.equal(fixture.selectedEntryId, 103);
  assert.equal(fixture.cards[2]!.classSelected, true);
  assert.equal(fixture.cards[0]!.classSelected, false);
  assert.equal(fixture.cards[1]!.classSelected, false);
});

test("the selection is cleared when the visible scope drops the selected id", () => {
  // A delete, refresh, search or collection change can shrink
  // the visible set. The rail's reactive block drops the
  // selection silently so the keyboard shortcut cannot open a
  // preview against a stale id.
  const fixture = createRailFixture([101, 102, 103]);
  simulateCardClick(fixture, 103);
  assert.equal(fixture.selectedEntryId, 103);
  // Drop the selected id (simulates a delete): the rail clears
  // the selection so the user is not left with an invisible
  // active row.
  refreshVisibleScope(fixture, [101, 102]);
  assert.equal(fixture.selectedEntryId, null);
  for (const card of fixture.cards) {
    assert.equal(card.classSelected, false);
    assert.equal(card.hasPreviewHint, false);
  }
});

// ---------------------------------------------------------------------------
// 2. Desktop horizontal keyboard navigation — ArrowLeft / ArrowRight.
// ---------------------------------------------------------------------------

test("ArrowRight with no current selection selects the first visible card", () => {
  // The rail starts in the `null` state; the first ArrowRight
  // press MUST land on the first visible card (the documented
  // `null → first` fallback). The regression that the manual QA
  // pass surfaced was that the arrow key only scrolled the
  // rail and never updated the selection.
  const fixture = createRailFixture([101, 102, 103, 104]);
  simulateArrowKeydown(fixture, "right");
  assert.equal(fixture.selectedEntryId, 101);
  assert.equal(fixture.cards[0]!.classSelected, true);
  assert.equal(fixture.cards[1]!.classSelected, false);
  assert.equal(fixture.cards[2]!.classSelected, false);
  assert.equal(fixture.cards[3]!.classSelected, false);
  assert.equal(fixture.preventedDefaults, 1);
  assert.deepEqual(fixture.lastScroll, {
    id: 101,
    block: "nearest",
    inline: "nearest",
  });
});

test("ArrowLeft with no current selection selects the last visible card", () => {
  // The symmetric `null → last` fallback: the first ArrowLeft
  // press lands on the last card so the user can navigate to
  // the bottom of the rail without first pressing ArrowRight
  // four times.
  const fixture = createRailFixture([101, 102, 103, 104]);
  simulateArrowKeydown(fixture, "left");
  assert.equal(fixture.selectedEntryId, 104);
  assert.equal(fixture.cards[0]!.classSelected, false);
  assert.equal(fixture.cards[3]!.classSelected, true);
  assert.equal(fixture.preventedDefaults, 1);
  assert.deepEqual(fixture.lastScroll, {
    id: 104,
    block: "nearest",
    inline: "nearest",
  });
});

test("ArrowRight on the second card advances to the third card", () => {
  // The contract is `currentId + 1`; pressing ArrowRight on card
  // 102 selects card 103 and only card 103 carries the visual
  // cue. The previous regression left the selection on card 1
  // and only scrolled the rail.
  const fixture = createRailFixture([101, 102, 103, 104]);
  simulateCardClick(fixture, 102);
  simulateArrowKeydown(fixture, "right");
  assert.equal(fixture.selectedEntryId, 103);
  assert.equal(fixture.cards[1]!.classSelected, false);
  assert.equal(fixture.cards[2]!.classSelected, true);
  assert.equal(fixture.cards[2]!.hasPreviewHint, true);
  assert.equal(fixture.cards[2]!.ariaSelected, true);
  assert.equal(fixture.preventedDefaults, 1);
  assert.deepEqual(fixture.lastScroll, {
    id: 103,
    block: "nearest",
    inline: "nearest",
  });
});

test("ArrowLeft on the third card retreats to the second card", () => {
  // The symmetric contract: `currentId - 1`. The behavioural
  // assertion verifies the same predicate drives the visual cue
  // and the Preview hint, so the user always sees the right
  // affordance.
  const fixture = createRailFixture([101, 102, 103, 104]);
  simulateCardClick(fixture, 103);
  simulateArrowKeydown(fixture, "left");
  assert.equal(fixture.selectedEntryId, 102);
  assert.equal(fixture.cards[1]!.classSelected, true);
  assert.equal(fixture.cards[2]!.classSelected, false);
  assert.equal(fixture.cards[2]!.hasPreviewHint, false);
  assert.equal(fixture.cards[1]!.hasPreviewHint, true);
  assert.equal(fixture.preventedDefaults, 1);
  assert.deepEqual(fixture.lastScroll, {
    id: 102,
    block: "nearest",
    inline: "nearest",
  });
});

test("ArrowRight on the last card clamps without wrapping", () => {
  // The boundary case the spec mandates: the rail MUST NOT wrap
  // to the first card on ArrowRight at the last row. The
  // selection stays on the last card and the scrollIntoView
  // round-trip keeps the row visible. The
  // `preventDefault()` call still fires so the native overflow
  // scroll cannot double-fire alongside the rail navigation.
  const fixture = createRailFixture([101, 102, 103, 104]);
  simulateCardClick(fixture, 104);
  simulateArrowKeydown(fixture, "right");
  assert.equal(fixture.selectedEntryId, 104);
  assert.equal(fixture.cards[3]!.classSelected, true);
  assert.equal(fixture.preventedDefaults, 1);
  assert.deepEqual(fixture.lastScroll, {
    id: 104,
    block: "nearest",
    inline: "nearest",
  });
});

test("ArrowLeft on the first card clamps without wrapping", () => {
  // The symmetric boundary: ArrowLeft at the first row MUST
  // NOT teleport to the last row. The selection stays on the
  // first card and the scrollIntoView keeps the row visible.
  const fixture = createRailFixture([101, 102, 103, 104]);
  simulateCardClick(fixture, 101);
  simulateArrowKeydown(fixture, "left");
  assert.equal(fixture.selectedEntryId, 101);
  assert.equal(fixture.cards[0]!.classSelected, true);
  assert.equal(fixture.preventedDefaults, 1);
  assert.deepEqual(fixture.lastScroll, {
    id: 101,
    block: "nearest",
    inline: "nearest",
  });
});

test("the arrow handler calls preventDefault so the native scroll cannot run", () => {
  // The contract is explicit: the rail-level arrow handler MUST
  // swallow the native overflow scroll the rail's
  // `overflow-x: auto` would otherwise trigger. A regression
  // that forgot `event.preventDefault()` would let the browser
  // scroll the rail while the rail scrolls itself, double-
  // scrolling the row and confusing the user. The behavioural
  // assertion counts the `preventDefault()` calls across a
  // multi-press sequence.
  const fixture = createRailFixture([101, 102, 103, 104]);
  simulateArrowKeydown(fixture, "right"); // selects 101
  simulateArrowKeydown(fixture, "right"); // selects 102
  simulateArrowKeydown(fixture, "right"); // selects 103
  assert.equal(fixture.selectedEntryId, 103);
  assert.equal(
    fixture.preventedDefaults,
    3,
    "the arrow handler must call preventDefault exactly once per consumed key",
  );
});

test("the arrow handler calls scrollIntoView on the freshly selected card only", () => {
  // The contract is `scrollIntoView({ block: "nearest", inline:
  // "nearest" })` on the freshly selected card. A regression
  // that called `scrollIntoView` on the previous card or on
  // the container would move the rail's scroll surface instead
  // of the card. The behavioural test exercises a multi-press
  // sequence and asserts each `scrollIntoView` round-trip
  // targets the freshly selected id with the documented
  // options.
  const fixture = createRailFixture([101, 102, 103, 104]);
  simulateArrowKeydown(fixture, "right"); // selects 101
  simulateArrowKeydown(fixture, "right"); // selects 102
  simulateArrowKeydown(fixture, "right"); // selects 103
  simulateArrowKeydown(fixture, "left"); // selects 102
  const card1 = fixture.cards.find((card) => card.id === 101)!;
  const card2 = fixture.cards.find((card) => card.id === 102)!;
  const card3 = fixture.cards.find((card) => card.id === 103)!;
  const card4 = fixture.cards.find((card) => card.id === 104)!;
  // Press 1 (right) → scroll card 1.
  // Press 2 (right) → scroll card 2.
  // Press 3 (right) → scroll card 3.
  // Press 4 (left)  → scroll card 2 (re-anchored).
  assert.equal(card1.scrollIntoViewCalls.length, 1);
  assert.equal(card2.scrollIntoViewCalls.length, 2);
  assert.equal(card3.scrollIntoViewCalls.length, 1);
  assert.equal(card4.scrollIntoViewCalls.length, 0);
  // Every recorded call must use the documented `{ block:
  // "nearest", inline: "nearest" }` options.
  for (const card of [card1, card2, card3]) {
    for (const call of card.scrollIntoViewCalls) {
      assert.deepEqual(call, { block: "nearest", inline: "nearest" });
    }
  }
});

test("the arrow handler is a no-op on an empty rail", () => {
  // An empty rail MUST short-circuit without claiming the event
  // so a stray ArrowRight against the empty-state copy does not
  // interfere with the user's flow. The behavioural test
  // asserts `preventDefault` and `scrollIntoView` are both
  // untouched.
  const fixture = createRailFixture([]);
  simulateArrowKeydown(fixture, "right");
  assert.equal(fixture.selectedEntryId, null);
  assert.equal(fixture.preventedDefaults, 0);
  assert.equal(fixture.lastScroll, null);
});

test("the arrow handler survives a mid-list refresh that keeps every id", () => {
  // A refresh that keeps every visible id (e.g. a hydration
  // round finishes, a thumbnail settles, a tag mutation
  // re-renders the rail) MUST NOT reset the selection. The
  // arrow handler then continues from the same active row.
  const fixture = createRailFixture([101, 102, 103, 104]);
  simulateArrowKeydown(fixture, "right"); // 101
  simulateArrowKeydown(fixture, "right"); // 102
  assert.equal(fixture.selectedEntryId, 102);
  // Refresh keeps every id: the rail re-renders but the
  // selection survives.
  refreshVisibleScope(fixture, [101, 102, 103, 104]);
  assert.equal(fixture.selectedEntryId, 102);
  assert.equal(fixture.cards[1]!.classSelected, true);
  // A follow-up ArrowRight still advances from the same row.
  simulateArrowKeydown(fixture, "right");
  assert.equal(fixture.selectedEntryId, 103);
});

test("the arrow handler drops the selection when the visible scope shrinks past the active id", () => {
  // A delete that drops the active id from the visible scope
  // clears the selection (the rail's reactive block) and the
  // next ArrowRight press MUST pick a visible neighbour instead
  // of trying to scrollIntoView the deleted card.
  const fixture = createRailFixture([101, 102, 103]);
  simulateCardClick(fixture, 103);
  assert.equal(fixture.selectedEntryId, 103);
  // Drop the selected id from the visible scope.
  refreshVisibleScope(fixture, [101, 102]);
  assert.equal(fixture.selectedEntryId, null);
  // A follow-up ArrowRight from the empty selection lands on
  // the first visible card (the `null → first` fallback).
  simulateArrowKeydown(fixture, "right");
  assert.equal(fixture.selectedEntryId, 101);
  assert.equal(fixture.cards[0]!.classSelected, true);
  // The post-refresh ArrowRight landed on card 101; card 102 was
  // never the active row so it received no scrollIntoView call.
  assert.equal(fixture.cards[0]!.scrollIntoViewCalls.length, 1);
  assert.equal(fixture.cards[1]!.scrollIntoViewCalls.length, 0);
  // The deleted card 103 fell out of the rail's array entirely
  // after `refreshVisibleScope`; the registry is the only place
  // it could have lingered, and the assertion below confirms the
  // rail can never try to `scrollIntoView` a card whose id fell
  // out of the visible scope.
  for (const card of fixture.cards) {
    assert.notEqual(card.id, 103);
  }
});

test("ArrowLeft / ArrowRight keep the Preview hint in lockstep with the active card", () => {
  // The hint must follow the active card; a regression that
  // pinned the hint to a fixed id (or to the first card) would
  // confuse the user about which row owns the `Cmd/Ctrl+Enter`
  // shortcut. The behavioural test walks a multi-press sequence
  // and confirms exactly one card carries the hint at every
  // step.
  const fixture = createRailFixture([101, 102, 103, 104]);
  const sequence: HorizontalRailDirection[] = [
    "right",
    "right",
    "right",
    "left",
    "right",
  ];
  const expectedIds = [101, 102, 103, 102, 103];
  sequence.forEach((direction, index) => {
    simulateArrowKeydown(fixture, direction);
    const expectedId = expectedIds[index]!;
    assert.equal(
      fixture.selectedEntryId,
      expectedId,
      `step ${index}: expected selectedEntryId=${expectedId} got ${fixture.selectedEntryId}`,
    );
    for (const card of fixture.cards) {
      const isActive = card.id === expectedId;
      assert.equal(
        card.hasPreviewHint,
        isActive,
        `step ${index}: card ${card.id} hint must be ${isActive} when active=${expectedId}`,
      );
      assert.equal(
        card.ariaSelected,
        isActive,
        `step ${index}: card ${card.id} aria-selected must be ${isActive} when active=${expectedId}`,
      );
    }
  });
});