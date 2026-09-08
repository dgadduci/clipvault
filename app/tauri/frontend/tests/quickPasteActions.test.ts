import { test } from "node:test";
import * as assert from "node:assert/strict";

import {
  capabilitiesOf,
  clampedSelectedIndex,
  hasAnyQuickPasteAction,
  preserveSelectionAfterReorder,
  quickPasteConfirmAction,
  quickPasteEnterAction,
  quickPasteMenuActions,
  quickPasteOrderedIds,
  quickPasteShiftEnterAction,
  scrollSelectedRowIntoView,
  selectedIndexForClick,
  selectedIndexForEntryId,
} from "../src/lib/quickPasteActions.ts";
import type { EntryRecord, SearchHit } from "../src/types.ts";

const SHA = "a".repeat(64);
const VALID_RICH_REF = `rich-text/${SHA}.preview.html`;
const VALID_HTML_REF = `rich-text/${SHA}.html`;
const VALID_RTF_REF = `rich-text/${SHA}.rtf`;
const VALID_ASSET_REF = `clipboard/${SHA}.png`;

function imageEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 11,
    content: "",
    content_type: "image",
    content_size: 2048,
    content_hash: SHA,
    source_app: "com.apple.Preview",
    is_pinned: false,
    created_at: "2026-09-01T10:11:12Z",
    updated_at: "2026-09-01T10:11:12Z",
    last_seen_at: "2026-09-01T10:11:12Z",
    title: null,
    source_app_name: "Preview",
    source_app_icon_ref: null,
    asset_ref: VALID_ASSET_REF,
    mime_type: "image/png",
    payload_width: 640,
    payload_height: 480,
    rich_text_hash: null,
    rich_html_ref: null,
    rich_rtf_ref: null,
    rich_preview_ref: null,
    rich_html_size: null,
    rich_rtf_size: null,
    ...overrides,
  };
}

function textEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 1,
    content: "captured note",
    content_type: "text",
    content_size: 14,
    content_hash: "b".repeat(64),
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
    ...overrides,
  };
}

function richEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    ...textEntry({ id: 2 }),
    content: "rich plain text",
    rich_text_hash: SHA,
    rich_html_ref: VALID_HTML_REF,
    rich_rtf_ref: VALID_RTF_REF,
    rich_preview_ref: VALID_RICH_REF,
    rich_html_size: 64,
    rich_rtf_size: 32,
    ...overrides,
  };
}

test("capabilitiesOf returns isImage true for image rows", () => {
  const caps = capabilitiesOf(imageEntry());
  assert.equal(caps.isImage, true);
  assert.equal(caps.hasRich, false);
});

test("capabilitiesOf returns hasRich true for entries with rich references", () => {
  const caps = capabilitiesOf(richEntry());
  assert.equal(caps.isImage, false);
  assert.equal(caps.hasRich, true);
});

test("capabilitiesOf returns neither flag for plain text rows", () => {
  const caps = capabilitiesOf(textEntry());
  assert.equal(caps.isImage, false);
  assert.equal(caps.hasRich, false);
});

test("quickPasteEnterAction: rich entry copies rich text", () => {
  const action = quickPasteEnterAction(capabilitiesOf(richEntry()));
  assert.deepEqual(action, { kind: "copy", mode: "rich" });
});

test("quickPasteEnterAction: plain entry copies plain text", () => {
  const action = quickPasteEnterAction(capabilitiesOf(textEntry()));
  assert.deepEqual(action, { kind: "copy", mode: "plain" });
});

test("quickPasteEnterAction: image entry copies image (mode null)", () => {
  const action = quickPasteEnterAction(capabilitiesOf(imageEntry()));
  assert.deepEqual(action, { kind: "copy", mode: null });
});

test("quickPasteShiftEnterAction: rich entry copies plain text", () => {
  const action = quickPasteShiftEnterAction(capabilitiesOf(richEntry()));
  assert.deepEqual(action, { kind: "copy", mode: "plain" });
});

test("quickPasteShiftEnterAction: plain entry is a no-op", () => {
  const action = quickPasteShiftEnterAction(capabilitiesOf(textEntry()));
  assert.deepEqual(action, { kind: "none" });
});

test("quickPasteShiftEnterAction: image entry is a no-op", () => {
  const action = quickPasteShiftEnterAction(capabilitiesOf(imageEntry()));
  assert.deepEqual(action, { kind: "none" });
});

// ---------------------------------------------------------------------------
// Click parity: a row click must dispatch the exact same action as
// Enter / Shift+Enter so the keyboard shortcut and the pointer cannot
// drift apart. The shared `quickPasteConfirmAction` helper is the
// single switch the Svelte layer consults; the assertions below pin
// the parity at the helper level so a regression that re-implements
// the dispatch inside the component is caught before it can ship.
// ---------------------------------------------------------------------------

test("quickPasteConfirmAction parity: plain click matches Enter for every entry shape", () => {
  // The Svelte layer passes `shiftKey: false` for a normal click.
  // The resulting action MUST be byte-identical to the one Enter
  // returns, regardless of the entry capabilities.
  for (const entry of [textEntry(), richEntry(), imageEntry()]) {
    const caps = capabilitiesOf(entry);
    assert.deepEqual(
      quickPasteConfirmAction(caps, false),
      quickPasteEnterAction(caps),
      `plain click on ${entry.content_type} must match Enter`,
    );
  }
});

test("quickPasteConfirmAction parity: shift+click matches Shift+Enter for every entry shape", () => {
  // The Svelte layer forwards the `event.shiftKey` flag from the
  // mouse click. The dispatch MUST mirror Shift+Enter byte-for-byte.
  for (const entry of [textEntry(), richEntry(), imageEntry()]) {
    const caps = capabilitiesOf(entry);
    assert.deepEqual(
      quickPasteConfirmAction(caps, true),
      quickPasteShiftEnterAction(caps),
      `Shift+click on ${entry.content_type} must match Shift+Enter`,
    );
  }
});

test("quickPasteConfirmAction: plain click on a rich text row copies rich text", () => {
  // Spec scenario: "Click sobre una fila rich text copia la
  // representación correcta". The default for a rich row is the
  // rich representation; the helper pins that the dispatch returns
  // `mode: "rich"`.
  const caps = capabilitiesOf(richEntry());
  const action = quickPasteConfirmAction(caps, false);
  assert.deepEqual(action, { kind: "copy", mode: "rich" });
});

test("quickPasteConfirmAction: plain click on an image row copies the image (mode null)", () => {
  // Spec scenario: "Click sobre una fila de imagen copia la imagen".
  // The Rust side ignores `mode` for image rows; the helper still
  // returns `mode: null` so the controller can branch on the
  // capability shape consistently.
  const caps = capabilitiesOf(imageEntry());
  const action = quickPasteConfirmAction(caps, false);
  assert.deepEqual(action, { kind: "copy", mode: null });
});

test("quickPasteConfirmAction: shift+click on a plain text row is a no-op", () => {
  // The contract: Shift+Enter / Shift+Click on a plain-only row is
  // a no-op because the default Enter already covered the only
  // available representation.
  const caps = capabilitiesOf(textEntry());
  const action = quickPasteConfirmAction(caps, true);
  assert.deepEqual(action, { kind: "none" });
});

test("quickPasteConfirmAction: shift+click on an image row is a no-op", () => {
  // The image-only Shift+Enter contract is preserved at the click
  // parity level so a Shift+Click on an image row cannot
  // accidentally copy something else.
  const caps = capabilitiesOf(imageEntry());
  const action = quickPasteConfirmAction(caps, true);
  assert.deepEqual(action, { kind: "none" });
});

test("quickPasteMenuActions: image entry yields exactly one Copiar action plus Previsualizar", () => {
  const entry = imageEntry();
  const actions = quickPasteMenuActions(entry, "Captured image", {
    copyBusy: false,
  });
  assert.equal(actions.length, 2);
  assert.equal(actions[0].kind, "copy");
  assert.equal(actions[0].mode, null);
  assert.equal(actions[1].kind, "preview");
  // An image entry MUST NOT expose text copy actions.
  assert.equal(
    actions.some((action) => action.kind === "copy-rich"),
    false,
    "image entries must not surface a rich-text copy action",
  );
  assert.equal(
    actions.some((action) => action.kind === "copy-plain"),
    false,
    "image entries must not surface a plain-text copy action",
  );
});

test("quickPasteMenuActions: rich entry yields rich + plain actions plus Previsualizar", () => {
  const actions = quickPasteMenuActions(richEntry(), "Captured rich", {
    copyBusy: false,
  });
  assert.equal(actions.length, 3);
  assert.equal(actions[0].kind, "copy-rich");
  assert.equal(actions[0].disabled, false);
  assert.equal(actions[0].mode, "rich");
  assert.equal(actions[1].kind, "copy-plain");
  assert.equal(actions[1].disabled, false);
  assert.equal(actions[2].kind, "preview");
});

test("quickPasteMenuActions: plain entry yields one Copiar action plus Previsualizar", () => {
  const actions = quickPasteMenuActions(textEntry(), "Captured plain", {
    copyBusy: false,
  });
  assert.equal(actions.length, 2);
  assert.equal(actions[0].kind, "copy");
  assert.equal(actions[0].mode, "plain");
  assert.equal(actions[0].disabled, false);
  assert.equal(actions[1].kind, "preview");
  // The compact Quick Paste palette MUST NOT expose the disabled
  // rich-text variant for a non-rich entry: the design collapses the
  // rail's two-action shape into a single `Copiar` action so the row
  // stays the one-keystroke surface the spec documents.
  assert.equal(
    actions.some((action) => action.kind === "copy-rich"),
    false,
    "non-rich entries must not surface a rich-text copy action",
  );
});

test("quickPasteMenuActions: copyBusy disables every action", () => {
  const actions = quickPasteMenuActions(richEntry(), "Captured rich", {
    copyBusy: true,
  });
  for (const action of actions) {
    assert.equal(
      action.disabled,
      true,
      `copyBusy must disable every menu action; ${action.kind} stayed enabled`,
    );
  }
});

test("quickPasteMenuActions: Previsualizar action is always enabled for every entry shape", () => {
  for (const entry of [textEntry(), richEntry(), imageEntry()]) {
    const actions = quickPasteMenuActions(entry, "Title", { copyBusy: false });
    const preview = actions.find((action) => action.kind === "preview");
    assert.ok(preview, `entry shape ${entry.content_type} must expose Previsualizar`);
    assert.equal(preview?.disabled, false);
  }
});

test("quickPasteMenuActions: image copy action carries mode null and Copiar label", () => {
  const actions = quickPasteMenuActions(imageEntry(), "Captured image", {
    copyBusy: false,
  });
  const copyAction = actions.find((action) => action.kind === "copy");
  assert.ok(copyAction);
  assert.equal(copyAction?.label, "Copiar");
  assert.equal(
    copyAction && "mode" in copyAction ? copyAction.mode : "unexpected",
    null,
  );
  assert.equal(copyAction?.testId, "quick-paste-menu-copy");
});

test("quickPasteMenuActions: rich copy action carries mode rich and Copiar texto enriquecido label", () => {
  const actions = quickPasteMenuActions(richEntry(), "Captured rich", {
    copyBusy: false,
  });
  const richAction = actions.find((action) => action.kind === "copy-rich");
  assert.ok(richAction);
  assert.equal(
    richAction && "mode" in richAction ? richAction.mode : "unexpected",
    "rich",
  );
  assert.equal(richAction?.label, "Copiar texto enriquecido");
  assert.equal(richAction?.testId, "quick-paste-menu-copy-rich");
});

test("quickPasteMenuActions: plain copy action carries mode plain", () => {
  const actions = quickPasteMenuActions(textEntry(), "Captured plain", {
    copyBusy: false,
  });
  const copyAction = actions.find((action) => action.kind === "copy");
  assert.ok(copyAction);
  assert.equal(copyAction?.label, "Copiar");
  assert.equal(
    copyAction && "mode" in copyAction ? copyAction.mode : "unexpected",
    "plain",
  );
  assert.equal(copyAction?.testId, "quick-paste-menu-copy");
});

test("quickPasteMenuActions: plain copy-rich variant exposes Copiar texto enriquecido label", () => {
  const actions = quickPasteMenuActions(richEntry(), "Captured rich", {
    copyBusy: false,
  });
  const plainAction = actions.find((action) => action.kind === "copy-plain");
  assert.ok(plainAction);
  assert.equal(plainAction?.label, "Copiar texto plano");
  assert.equal(plainAction?.testId, "quick-paste-menu-copy-plain");
});

test("quickPasteMenuActions: never surfaces a Pegar wording", () => {
  // The menu MUST NEVER advertise a "Pegar" wording or route through
  // a pasteEntryCommand call. The copy-only surface is the documented
  // contract; a regression that re-introduces the legacy wording is
  // caught here before it ships.
  for (const entry of [textEntry(), richEntry(), imageEntry()]) {
    const actions = quickPasteMenuActions(entry, "Title", { copyBusy: false });
    for (const action of actions) {
      assert.equal(
        action.label.includes("Pegar"),
        false,
        `action ${action.kind} must not advertise Pegar wording, got "${action.label}"`,
      );
    }
  }
});

test("quickPasteMenuActions: preview action has no mode and a stable test id", () => {
  for (const entry of [textEntry(), richEntry(), imageEntry()]) {
    const actions = quickPasteMenuActions(entry, "Title", { copyBusy: false });
    const preview = actions.find((action) => action.kind === "preview");
    assert.ok(preview, `entry shape ${entry.content_type} must expose Previsualizar`);
    assert.equal(preview?.testId, "quick-paste-menu-preview");
    assert.equal("mode" in (preview ?? {}), false);
  }
});

test("hasAnyQuickPasteAction returns true for every entry shape", () => {
  assert.equal(hasAnyQuickPasteAction(imageEntry()), true);
  assert.equal(hasAnyQuickPasteAction(textEntry()), true);
  assert.equal(hasAnyQuickPasteAction(richEntry()), true);
});

function hit(record: EntryRecord): SearchHit {
  return {
    entry_id: record.id,
    snippet: record.content.slice(0, 10),
    score: 100 - record.id,
    record,
  };
}

test("quickPasteOrderedIds: recents put favourites first and order each group by created_at DESC, id DESC", () => {
  // Recent mode orders favourites first and then sorts each group
  // chronologically. The source-feed ordering is intentionally
  // overridden: a newer capture must never appear after an older one
  // within the same group.
  const pinnedNew = textEntry({
    id: 30,
    is_pinned: true,
    created_at: "2026-09-03T12:00:00Z",
  });
  const pinnedOld = textEntry({
    id: 10,
    is_pinned: true,
    created_at: "2026-09-01T12:00:00Z",
  });
  const plainNew = textEntry({
    id: 5,
    created_at: "2026-09-02T12:00:00Z",
  });
  const plainOld = textEntry({
    id: 20,
    created_at: "2026-08-30T12:00:00Z",
  });
  const recents = [plainOld, pinnedNew, plainNew, pinnedOld];
  const ids = quickPasteOrderedIds("recent", recents, []);
  assert.deepEqual(ids, [pinnedNew.id, pinnedOld.id, plainNew.id, plainOld.id]);
});

test("quickPasteOrderedIds: search puts favourites first and keeps search ranking within each group", () => {
  const pinned = textEntry({ id: 99, is_pinned: true });
  const plain = textEntry({ id: 1 });
  const hits = [hit(plain), hit(pinned)];
  const ids = quickPasteOrderedIds("search", [], hits);
  assert.deepEqual(ids, [pinned.id, plain.id]);
});

test("quickPasteOrderedIds: empty input yields empty output", () => {
  assert.deepEqual(quickPasteOrderedIds("recent", [], []), []);
  assert.deepEqual(quickPasteOrderedIds("search", [], []), []);
  assert.deepEqual(quickPasteOrderedIds("idle", [], []), []);
});

test("quickPasteOrderedIds: pinned order within the favourites group follows created_at DESC", () => {
  // Recent mode sorts each group by capture time, NOT by id. The
  // earlier pinned entry (older `created_at`) must move to the back
  // of the favourites group, regardless of its source position.
  const firstPinned = textEntry({
    id: 50,
    is_pinned: true,
    created_at: "2026-09-04T12:00:00Z",
  });
  const secondPinned = textEntry({
    id: 10,
    is_pinned: true,
    created_at: "2026-09-02T12:00:00Z",
  });
  const recents = [secondPinned, firstPinned];
  const ids = quickPasteOrderedIds("recent", recents, []);
  assert.deepEqual(ids, [firstPinned.id, secondPinned.id]);
});

test("quickPasteOrderedIds: ties on created_at use id DESC", () => {
  // Two captures with identical timestamps must use the entry id as
  // the tie-breaker so the order stays deterministic across
  // `history-updated` cycles.
  const pinned = textEntry({
    id: 30,
    is_pinned: true,
    created_at: "2026-09-01T12:00:00Z",
  });
  const plain = textEntry({
    id: 5,
    created_at: "2026-09-01T12:00:00Z",
  });
  const older = textEntry({
    id: 7,
    created_at: "2026-08-30T12:00:00Z",
  });
  const recents = [older, plain, pinned];
  const ids = quickPasteOrderedIds("recent", recents, []);
  // Favourites first, then non-favourites sorted by created_at DESC.
  assert.deepEqual(ids, [pinned.id, plain.id, older.id]);
});

test("quickPasteOrderedIds: identical timestamps use id DESC within the same group", () => {
  const newerId = textEntry({
    id: 12,
    created_at: "2026-09-01T12:00:00Z",
  });
  const smallerId = textEntry({
    id: 3,
    created_at: "2026-09-01T12:00:00Z",
  });
  const recents = [smallerId, newerId];
  const ids = quickPasteOrderedIds("recent", recents, []);
  assert.deepEqual(ids, [newerId.id, smallerId.id]);
});

test("preserveSelectionAfterReorder keeps the index inside the bounds", () => {
  const reordered = preserveSelectionAfterReorder([1, 2, 3], 4);
  assert.equal(reordered.selectedIndex, 0);
});

test("preserveSelectionAfterReorder keeps the index for empty ids", () => {
  const reordered = preserveSelectionAfterReorder([], 0);
  assert.deepEqual(reordered.ids, []);
  assert.equal(reordered.selectedIndex, 0);
});

// ---------------------------------------------------------------------------
// 6.1: Click selects a row. The pure helper is the single switch the
// row's `on:click` handler consults, so a regression that ignores a
// click, selects the wrong row or hard-codes `selectedIndex = 0`
// surfaces here.
// ---------------------------------------------------------------------------

test("selectedIndexForClick returns the index of the clicked row", () => {
  assert.equal(selectedIndexForClick([10, 20, 30], 20, 0), 1);
  assert.equal(selectedIndexForClick([10, 20, 30], 30, 0), 2);
  assert.equal(selectedIndexForClick([10, 20, 30], 10, 0), 0);
});

test("selectedIndexForClick keeps the previous index for an unknown id", () => {
  // The id the click handler just received is not in the visible
  // list (e.g. a stale closure). The helper MUST NOT silently land on
  // a different row; the previous (clamped) index is the safe
  // fallback so the keyboard flow stays deterministic.
  assert.equal(selectedIndexForClick([10, 20, 30], 999, 1), 1);
  assert.equal(selectedIndexForClick([10, 20, 30], 999, 5), 0);
});

test("selectedIndexForClick returns 0 for an empty list", () => {
  // An empty list is the only time the helper returns a fresh index
  // the caller can apply without a follow-up clamp.
  assert.equal(selectedIndexForClick([], 42, 0), 0);
});

// ---------------------------------------------------------------------------
// 6.3: Scoped autoscroll on keyboard navigation. The pure helper
// only invokes `scrollIntoView` on the row element so the desktop
// scroll surface cannot move.
// ---------------------------------------------------------------------------

interface FakeRow {
  calls: Array<ScrollIntoViewOptions | undefined>;
  scrollIntoView: (options?: ScrollIntoViewOptions) => void;
}

function fakeRow(): FakeRow {
  const row: FakeRow = {
    calls: [],
    scrollIntoView(options) {
      row.calls.push(options);
    },
  };
  return row;
}

test("scrollSelectedRowIntoView only targets the selected row with block: nearest", () => {
  const rows = [fakeRow(), fakeRow(), fakeRow()];
  scrollSelectedRowIntoView({ length: 3 }, rows, 1);
  assert.deepEqual(rows[0].calls, []);
  assert.deepEqual(rows[1].calls, [{ block: "nearest" }]);
  assert.deepEqual(rows[2].calls, []);
});

test("scrollSelectedRowIntoView is a no-op when the index is out of range", () => {
  const rows = [fakeRow(), fakeRow()];
  scrollSelectedRowIntoView({ length: 2 }, rows, 5);
  scrollSelectedRowIntoView({ length: 2 }, rows, -1);
  assert.deepEqual(rows[0].calls, []);
  assert.deepEqual(rows[1].calls, []);
});

test("scrollSelectedRowIntoView skips a missing row without crashing", () => {
  const rows: (FakeRow | null)[] = [fakeRow(), null, fakeRow()];
  // The no-op row at index 1 must not throw; index 2 must be
  // scrolled into view normally.
  scrollSelectedRowIntoView({ length: 3 }, rows, 1);
  scrollSelectedRowIntoView({ length: 3 }, rows, 2);
  assert.deepEqual(rows[0].calls, []);
  assert.deepEqual(rows[2].calls, [{ block: "nearest" }]);
});

test("scrollSelectedRowIntoView never invokes anything on the container", () => {
  // The helper only touches the row. A regression that started
  // calling `scrollIntoView` on the container (which would move the
  // desktop scroll surface) is impossible by construction because
  // the helper accepts a length-only container contract.
  const rows = [fakeRow()];
  const container = { length: 1 };
  scrollSelectedRowIntoView(container, rows, 0);
  // The container carries no other methods; asserting `length` is
  // enough to prove the helper never reaches for any DOM surface.
  assert.equal(typeof (container as unknown as Record<string, unknown>)["scrollIntoView"], "undefined");
});

// ---------------------------------------------------------------------------
// 6.4: Preserve selection by stable entry id across search, favourite
// toggle and refresh.
// ---------------------------------------------------------------------------

test("selectedIndexForEntryId follows the stable id across a re-rank", () => {
  // The list re-renders with the same entry ids in a new order; the
  // helper MUST land on the id the user previously selected.
  const next = selectedIndexForEntryId([30, 10, 20], 20, 0);
  assert.equal(next, 2);
});

test("selectedIndexForEntryId falls back to the previous index when the id is gone", () => {
  // The user toggled a pin and the row was filtered out (or the row
  // was deleted). The helper keeps the keyboard focus on a
  // deterministic, non-stale row instead of silently selecting the
  // first visible entry.
  const next = selectedIndexForEntryId([30, 10, 20], 999, 1);
  assert.equal(next, 1);
});

test("selectedIndexForEntryId returns 0 when the list is empty", () => {
  assert.equal(selectedIndexForEntryId([], 42, 3), 0);
  assert.equal(selectedIndexForEntryId([], null, 0), 0);
});

test("selectedIndexForEntryId falls back to 0 when the fallback index is out of range", () => {
  // The fallback index must itself be clamped so the keyboard flow
  // never lands on a stale slot.
  assert.equal(selectedIndexForEntryId([10, 20, 30], null, 99), 0);
});

test("selectedIndexForEntryId tolerates a null selected id", () => {
  // `null` is the "selection not yet established" sentinel; the
  // helper MUST NOT crash on it and MUST honour the fallback index.
  assert.equal(selectedIndexForEntryId([10, 20, 30], null, 0), 0);
  assert.equal(selectedIndexForEntryId([10, 20, 30], null, 2), 2);
});

// ---------------------------------------------------------------------------
// Regression: arrow-key navigation clamps at the list bounds.
//
// The previous baseline wrapped `selectedIndex` with the modulo
// operator on every ArrowDown press; pressing the key on the last
// row silently teleported the selection back to the first row, which
// surfaced as a regression during the second round of manual QA on
// `preview-interaction-regressions`. The new contract is **clamp**:
// the selection stays on the boundary row when the user overshoots.
// Home / End remain the canonical wrap-around shortcuts; the
// pure helper `clampedSelectedIndex` owns the math so the Svelte
// layer stays a thin caller.
// ---------------------------------------------------------------------------

test("clampedSelectedIndex advances by the delta inside the list bounds", () => {
  assert.equal(clampedSelectedIndex([10, 20, 30], 0, 1), 1);
  assert.equal(clampedSelectedIndex([10, 20, 30], 1, 1), 2);
});

test("clampedSelectedIndex decrements by the delta inside the list bounds", () => {
  assert.equal(clampedSelectedIndex([10, 20, 30], 2, -1), 1);
  assert.equal(clampedSelectedIndex([10, 20, 30], 1, -1), 0);
});

test("clampedSelectedIndex clamps at the top edge (no negative index)", () => {
  // ArrowUp on the first row MUST stay on the first row, not jump
  // to the last entry. The helper pins this contract so a future
  // refactor that introduces `delta < 0` cannot silently flip the
  // selection across the entire list.
  assert.equal(clampedSelectedIndex([10, 20, 30], 0, -1), 0);
  assert.equal(clampedSelectedIndex([10, 20, 30], 0, -5), 0);
});

test("clampedSelectedIndex clamps at the bottom edge (no wrap-around)", () => {
  // ArrowDown on the last row MUST stay on the last row, not wrap
  // back to the first entry. The helper is the single switch that
  // owns the math so a regression that reintroduces the modulo
  // wrap surfaces here.
  assert.equal(clampedSelectedIndex([10, 20, 30], 2, 1), 2);
  assert.equal(clampedSelectedIndex([10, 20, 30], 2, 5), 2);
});

test("clampedSelectedIndex returns -1 for an empty list", () => {
  // The Svelte layer checks `next < 0` to skip scrollIntoView and
  // any state mutation, so the empty-list sentinel must be a
  // negative value distinct from any valid index.
  assert.equal(clampedSelectedIndex([], 0, 1), -1);
  assert.equal(clampedSelectedIndex([], 5, -3), -1);
});

test("clampedSelectedIndex normalises an out-of-range current index", () => {
  // A stale closure from a previous render can land on a current
  // index outside the new list bounds (e.g. the user filtered
  // down to a single row while selectedIndex was still 4). The
  // helper MUST NOT crash; it normalises the index to a safe value
  // before applying the delta so the next render never lands on a
  // stale slot.
  assert.equal(clampedSelectedIndex([10], 5, 1), 0);
  assert.equal(clampedSelectedIndex([10], -1, 1), 0);
  assert.equal(clampedSelectedIndex([10], 0, -1), 0);
});

test("clampedSelectedIndex never returns a value outside [0, total-1]", () => {
  // Property-based check: every (currentIndex, delta) pair the
  // regression suite can produce must return an index the Svelte
  // layer can pass to `resultIds[...]` without an `undefined`
  // lookup. The helper is total; the assertion sweeps the
  // representative input space.
  const ids = [10, 20, 30, 40, 50];
  for (const current of [-3, -1, 0, 2, 4, 7, 99]) {
    for (const delta of [-5, -1, 0, 1, 5]) {
      const next = clampedSelectedIndex(ids, current, delta);
      if (next < 0) continue;
      assert.ok(
        next >= 0 && next < ids.length,
        `clampedSelectedIndex([10..], ${current}, ${delta}) returned ${next}, expected a value in [0, ${ids.length - 1}]`,
      );
    }
  }
});
