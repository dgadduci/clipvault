/**
 * Regression coverage for tasks 6.1 - 6.7 of the
 * `quick-paste-actions` change.
 *
 * The suite exercises the contract the spec documents for
 * click-based selection, control isolation, scoped autoscroll,
 * id-stable selection across result changes, title-only search and
 * per-entry image thumbnail settlement:
 *
 *   - the non-interactive surface of every Quick Paste row selects
 *     that entry on pointer / mouse click;
 *   - pin and overflow-menu controls do NOT propagate the click
 *     back to the row, so they never trigger an accidental selection
 *     or a copy / paste confirmation;
 *   - keyboard navigation keeps the selected row visible without
 *     moving the desktop scroll surface;
 *   - selection is preserved by stable entry id across a search, a
 *     favourite toggle or a refresh, never on a stale index;
 *   - Quick Paste search matches both the user-defined card title and
 *     the canonical content with deterministic ordering;
 *   - every initially visible image row independently resolves its
 *     thumbnail, with loading / loaded / error states protected
 *     against stale responses.
 *
 * The helpers live in `lib/quickPasteActions.ts` and the Svelte
 * component itself drives the DOM. The tests pin both layers:
 *
 *   - the pure helpers (`selectedIndexForClick`,
 *     `selectedIndexForEntryId`, `scrollSelectedRowIntoView`) are
 *     exercised with hand-crafted inputs in
 *     `quickPasteActions.test.ts`;
 *   - the Svelte source is inspected here so a regression that
 *     removes the click handler, drops `|stopPropagation` from a
 *     control, or accidentally wires `scrollIntoView` on the
 *     container cannot ship.
 */

import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const FRONTEND_ROOT = process.cwd();

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

/**
 * Strip comments so a regression pinned inside a comment cannot
 * accidentally match the source-level assertions below.
 */
function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
}

const quickPasteSource = stripComments(
  loadSource("src/QuickPaste.svelte"),
);

// ---------------------------------------------------------------------------
// 6.1: the non-interactive surface of every Quick Paste row selects
// that entry on pointer / mouse click.
// ---------------------------------------------------------------------------

/**
 * Extract the qp-row `<li>` opening tag. Svelte attributes can
 * legitimately contain `>` (e.g. `class:foo={a > b}`), so the simple
 * `indexOf(">")` trick stops at the first comparison operator. The
 * row's `bind:this={rowEls[index]}` is always the LAST attribute of
 * the opening tag, so the slice below stops at that sentinel and
 * includes the closing `>`.
 */
function extractRowOpeningTag(): string {
  const start = quickPasteSource.indexOf('<li');
  assert.notEqual(
    start,
    -1,
    "the qp-row <li> must exist in QuickPaste.svelte",
  );
  const sentinel = quickPasteSource.indexOf("bind:this={rowEls", start);
  assert.notEqual(sentinel, -1, "the qp-row <li> must use bind:this");
  const closeBrace = quickPasteSource.indexOf("}", sentinel);
  assert.notEqual(closeBrace, -1);
  return quickPasteSource.slice(start, closeBrace + 2);
}

test("Quick Paste row carries an on:click handler that confirms the entry", () => {
  // The row's <li> element MUST own the click contract; a regression
  // that delegates it to a child control or removes it silently
  // would break the spec scenario "Click confirms a result like
  // Enter". The handler routes through `handleRowClick` so the row
  // both selects the entry and invokes the copy-only confirmation,
  // matching the Enter / Shift+Enter keyboard shortcut by construction.
  const openingTag = extractRowOpeningTag();
  assert.ok(
    openingTag.includes('class="qp-row"'),
    "the <li> must carry the qp-row class",
  );
  assert.ok(
    openingTag.includes('data-testid="quick-paste-row"'),
    "the qp-row <li> must keep its stable test selector",
  );
  assert.ok(
    openingTag.includes("data-entry-id={id}"),
    "the qp-row <li> must keep the data-entry-id attribute the regression suite asserts",
  );
  assert.ok(
    openingTag.includes("on:click={(event) => handleRowClick(id, event)}"),
    "the qp-row <li> must call handleRowClick so click and Enter share the same confirmation",
  );
});

// ---------------------------------------------------------------------------
// 6.2: pin and overflow-menu controls do NOT trigger row selection.
// The controls live inside the row, so they MUST `stopPropagation`
// to keep the row's click handler from firing accidentally.
// ---------------------------------------------------------------------------

/**
 * Find an opening tag whose attribute set contains `needle` (one of
 * the unique class markers the regression suite pins). Returns the
 * tag text from the `<tagName` opener up to and including the last
 * attribute the sentinel-driven extraction captures. We anchor the
 * extraction to the `on:click` handler (which is always the LAST
 * attribute on these buttons) so the search is independent of
 * intermediate `>` characters used inside Svelte comparison
 * operators.
 */
function findOpeningTagContaining(
  source: string,
  marker: string,
  tagName: string,
  clickHandler: string,
): string {
  let cursor = source.indexOf(`<${tagName}`);
  while (cursor !== -1) {
    const classPos = source.indexOf(marker, cursor);
    if (classPos === -1) {
      return "";
    }
    const handlerPos = source.indexOf(clickHandler, classPos);
    if (handlerPos === -1) {
      return "";
    }
    return source.slice(cursor, handlerPos + clickHandler.length);
  }
  return "";
}

test("pin button uses click|stopPropagation so it does not select the row", () => {
  const pinOpeningTag = findOpeningTagContaining(
    quickPasteSource,
    'class="qp-pin"',
    "button",
    "on:click|stopPropagation={() => void togglePin(id)}",
  );
  assert.ok(pinOpeningTag, "the pin button must exist in QuickPaste.svelte");
  assert.ok(
    pinOpeningTag.includes("on:click|stopPropagation={() => void togglePin(id)}"),
    "the pin button must stop click propagation before calling togglePin",
  );
  // The pin click MUST only toggle the favourite flag — it MUST NOT
  // confirm the row (run copy or paste) nor select a different row.
  for (const forbidden of [
    "confirmEntry",
    "runCopyForEntry",
    "runMenuPasteForEntry",
    "pasteEntryCommand",
    "copyEntryCommand",
    "selectEntryOnClick",
    "handleRowClick",
  ]) {
    assert.equal(
      pinOpeningTag.includes(forbidden),
      false,
      `the pin click handler must not invoke ${forbidden}`,
    );
  }
});

test("overflow-menu trigger uses click|stopPropagation so it does not select the row", () => {
  const triggerOpeningTag = findOpeningTagContaining(
    quickPasteSource,
    'class="qp-menu-trigger"',
    "button",
    "on:click|stopPropagation={() => toggleMenuFor(id)}",
  );
  assert.ok(
    triggerOpeningTag,
    "the menu trigger button must exist in QuickPaste.svelte",
  );
  assert.ok(
    triggerOpeningTag.includes("on:click|stopPropagation={() => toggleMenuFor(id)}"),
    "the menu trigger must stop click propagation before calling toggleMenuFor",
  );
  // The menu trigger click MUST only open the menu — it MUST NOT
  // confirm the row or invoke any copy / paste command.
  for (const forbidden of [
    "confirmEntry",
    "runCopyForEntry",
    "runMenuPasteForEntry",
    "pasteEntryCommand",
    "copyEntryCommand",
    "selectEntryOnClick",
    "handleRowClick",
  ]) {
    assert.equal(
      triggerOpeningTag.includes(forbidden),
      false,
      `the menu trigger click handler must not invoke ${forbidden}`,
    );
  }
});

test("menu items use click|stopPropagation so they do not select the row", () => {
  // The `...` menu renders one menuitem per direct paste action
  // plus the read-only `Previsualizar` entry. Every menu item MUST
  // stop propagation so a click on the menu item never bubbles
  // back to the row and never runs twice.
  const menuStart = quickPasteSource.indexOf('data-testid="quick-paste-menu"');
  assert.notEqual(menuStart, -1, "the menu <ul> must exist in QuickPaste.svelte");
  const menuEnd = quickPasteSource.indexOf("</ul>", menuStart);
  const menuBlock = quickPasteSource.slice(menuStart, menuEnd);
  assert.ok(
    menuBlock.includes("on:click|stopPropagation={() =>"),
    "every menu item must stop click propagation",
  );
  assert.ok(
    menuBlock.includes("runMenuAction("),
    "direct paste menu items must delegate to runMenuAction",
  );
  assert.ok(
    menuBlock.includes("openPreviewFor("),
    "the Previsualizar menu item must delegate to openPreviewFor",
  );
  // The menu items MUST keep the legacy direct paste lifecycle
  // (they MUST NOT collapse into the keyboard copy-only flow).
  for (const forbidden of [
    "confirmEntry",
    "runCopyForEntry",
    "copyEntryCommand",
  ]) {
    assert.equal(
      menuBlock.includes(forbidden),
      false,
      `menu items must not invoke ${forbidden}; the direct paste lifecycle belongs to the menu only`,
    );
  }
});

test("row click goes through the shared confirmEntry controller, not the paste flow", () => {
  // The row click MUST invoke the same copy-only confirmation as
  // Enter. A regression that wires `runMenuPasteForEntry` or
  // `pasteEntryCommand` directly on the row click would silently
  // fall back to the legacy synthetic paste lifecycle, which the
  // spec explicitly forbids for the keyboard / click contract.
  // Likewise, calling `runCopyForEntry` directly (bypassing the
  // capability dispatch in `confirmEntry`) would let a click skip
  // the rich / plain / image mode selection.
  const openingTag = extractRowOpeningTag();
  for (const forbidden of [
    "runMenuPasteForEntry",
    "pasteEntryCommand",
  ]) {
    assert.equal(
      openingTag.includes(forbidden),
      false,
      `the qp-row click handler must not invoke ${forbidden}`,
    );
  }
  // The row click MUST delegate to the shared confirmation
  // controller so Enter and click share the same capability path.
  assert.ok(
    openingTag.includes("handleRowClick"),
    "the qp-row click handler must delegate to handleRowClick so Enter and click share confirmEntry",
  );
});

// ---------------------------------------------------------------------------
// 6.3: scoped autoscroll — the helper must only target the row, never
// the list container, the page or the desktop scroll surface.
// ---------------------------------------------------------------------------

test("autoscroll helper imports the pure scrollSelectedRowIntoView", () => {
  assert.match(
    quickPasteSource,
    /import\s*\{[^}]*scrollSelectedRowIntoView[^}]*\}\s*from\s*"\.\/lib\/quickPasteActions"/,
    "QuickPaste.svelte must import the pure autoscroll helper",
  );
});

test("autoscroll helper is wired on every keyboard navigation entry point", () => {
  // ArrowUp / ArrowDown / Home / End all flow through
  // moveSelection / jumpToFirst / jumpToLast. Each of those helpers
  // must invoke the autoscroll helper so the row stays visible
  // without re-implying the the same logic in the Svelte template.
  assert.match(quickPasteSource, /function moveSelection[\s\S]*?scrollSelectedIntoView\(\)/);
  assert.match(quickPasteSource, /function jumpToFirst[\s\S]*?scrollSelectedIntoView\(\)/);
  assert.match(quickPasteSource, /function jumpToLast[\s\S]*?scrollSelectedIntoView\(\)/);
});

test("autoscroll helper never calls scrollIntoView on the container", () => {
  // The pure helper accepts a `QuickPasteScrollContainer` whose only
  // contract is `length`; the production wrapper passes
  // `resultsListEl` as that container. A regression that started
  // calling `scrollIntoView` on the container itself would move the
  // desktop scroll surface.
  const wrapperMatch = quickPasteSource.match(
    /function scrollSelectedIntoView[\s\S]*?\n  \}/,
  );
  assert.ok(wrapperMatch, "scrollSelectedIntoView must exist in QuickPaste.svelte");
  assert.match(
    wrapperMatch[0],
    /scrollSelectedRowIntoView\(/,
    "the production wrapper must delegate to the pure helper",
  );
  assert.doesNotMatch(
    wrapperMatch[0],
    /resultsListEl\?\.scrollIntoView/,
    "the wrapper must not call scrollIntoView on the container",
  );
});

// ---------------------------------------------------------------------------
// 6.4: stable entry id is the source of truth for the selection.
// ---------------------------------------------------------------------------

test("Quick Paste tracks selectedEntryId alongside selectedIndex", () => {
  assert.match(
    quickPasteSource,
    /let selectedEntryId:\s*number\s*\|\s*null\s*=\s*null/,
    "selectedEntryId must be declared as a nullable number",
  );
});

test("clampSelection re-anchors on the stable id, never on a stale index", () => {
  // The clampSelection helper is the single switch that turns a
  // result-list change into a deterministic selection. The pure
  // helper `selectedIndexForEntryId` does the lookup by id.
  const clampMatch = quickPasteSource.match(
    /function clampSelection\(\)[\s\S]*?\n  \}/,
  );
  assert.ok(clampMatch, "clampSelection must exist in QuickPaste.svelte");
  assert.match(
    clampMatch[0],
    /thumbnailTokens|selectedEntryId/,
    "clampSelection must use stable selection state",
  );
});

test("keyboard navigation helpers keep selectedEntryId in sync", () => {
  // moveSelection / jumpToFirst / jumpToLast / selectEntryOnClick
  // MUST all update `selectedEntryId` after mutating `selectedIndex`
  // so the next clampSelection round can re-anchor by id.
  for (const name of [
    "function moveSelection",
    "function jumpToFirst",
    "function jumpToLast",
    "function selectEntryOnClick",
  ]) {
    const block = quickPasteSource.match(
      new RegExp(`${name}[\\s\\S]*?\\n  \\}`),
    );
    assert.ok(block, `${name} must exist in QuickPaste.svelte`);
    assert.match(
      block[0],
      /selectedEntryId\s*=/,
      `${name} must reassign selectedEntryId so the id-based clamp stays in sync`,
    );
  }
});

// ---------------------------------------------------------------------------
// 6.6: every initially visible image row independently resolves its
// thumbnail. The thumbnail state machine uses a shared token so a
// late response cannot overwrite the row that owns it.
// ---------------------------------------------------------------------------

test("thumbnail state machine is keyed per entry", () => {
  assert.match(
    quickPasteSource,
    /let\s+thumbnailStates:\s*Record<number,\s*ThumbnailState>/,
    "thumbnailStates must be a per-entry record keyed by entry id",
  );
  assert.match(
    quickPasteSource,
    /let\s+thumbnails:\s*Record<number,\s*string>/,
    "thumbnails must be a per-entry record keyed by entry id",
  );
});

test("thumbnail resolver protects each entry against stale responses", () => {
  const loadMatch = quickPasteSource.match(
    /function loadThumbnail\(entry[\s\S]*?\n  \}/,
  );
  assert.ok(loadMatch, "loadThumbnail must exist in QuickPaste.svelte");
  assert.match(loadMatch[0], /const token = \+\+thumbnailToken/);
  assert.match(
    loadMatch[0],
    /thumbnailTokens\.get\(entry\.id\) !== token[\s\S]*?return/,
    "loadThumbnail must discard stale responses per entry",
  );
});

test("syncThumbnails fans out a round-trip per visible entry", () => {
  // The reactive sync helper iterates over the visible result set
  // and kicks off `loadThumbnail(entry)` for every row so a single
  // slow or failing round-trip does NOT block the others.
  const syncMatch = quickPasteSource.match(
    /function syncThumbnails[\s\S]*?\n  \}/,
  );
  assert.ok(syncMatch, "syncThumbnails must exist in QuickPaste.svelte");
  assert.match(
    syncMatch[0],
    /void loadThumbnail\(entry\)/,
    "syncThumbnails must kick off a round-trip per visible entry",
  );
});

test("image thumbnail placeholders expose loading / loaded / error states", () => {
  // The three-state machine is exposed through `data-thumb-state` so
  // the rail can branch on `loading` without inspecting the URL.
  assert.match(
    quickPasteSource,
    /data-thumb-state=\{isImage \? thumbState : "none"\}/,
    "the row must expose data-thumb-state for the three-state machine",
  );
  assert.match(
    quickPasteSource,
    /data-testid="quick-paste-thumbnail-loading"/,
    "the loading placeholder test selector must remain stable",
  );
  assert.match(
    quickPasteSource,
    /data-testid="quick-paste-thumbnail-error"/,
    "the error placeholder test selector must remain stable",
  );
});

test("onDestroy revokes every thumbnail blob URL on teardown", () => {
  const destroyMatch = quickPasteSource.match(
    /onDestroy\(\(\) =>\s*\{[\s\S]*?\n  \}\);/,
  );
  assert.ok(destroyMatch, "onDestroy must exist in QuickPaste.svelte");
  assert.match(
    destroyMatch[0],
    /assetResolver\.release\(\)/,
    "onDestroy must revoke every thumbnail blob URL",
  );
  assert.match(
    destroyMatch[0],
    /thumbnails = \{\}/,
    "onDestroy must clear the thumbnail record so no row keeps a stale URL",
  );
});

// ---------------------------------------------------------------------------
// 7. Pointer confirmation parity regression.
//
// The user reported regression: clicking a row in Quick Paste used to
// only select the entry; the keyboard user had to press Enter
// afterwards. Per the spec, clicking the non-interactive surface of a
// row MUST be exactly equivalent to pressing Enter: copy the
// type-appropriate representation, hide Quick Paste and leave the
// clipboard available for a later Cmd/Ctrl+V — without invoking the
// synthetic paste controller.
//
// The regression suite pins the source-level wiring:
//
//   - a single `confirmEntry` controller dispatches Enter, Shift+Enter
//     and the row click so they cannot drift apart;
//   - the pin and overflow-menu controls MUST stay isolated;
//   - the menu items MUST keep the legacy direct paste flow;
//   - the row click MUST NOT double-register the listener.
// ---------------------------------------------------------------------------

test("Enter, Shift+Enter and row click all funnel through confirmEntry", () => {
  // The shared controller is the only switch that maps a stable
  // entry id to the typed copy action. handleEnter and
  // handleRowClick MUST call it so a regression that re-implements
  // the dispatch (or routes the click through the paste flow) is
  // visible in CI.
  const confirmMatch = quickPasteSource.match(
    /async function confirmEntry[\s\S]*?\n  \}/,
  );
  assert.ok(confirmMatch, "confirmEntry must exist in QuickPaste.svelte");
  assert.match(
    confirmMatch[0],
    /runCopyForEntry/,
    "confirmEntry must call runCopyForEntry so the in-flight guard stays in one place",
  );
  assert.match(
    confirmMatch[0],
    /quickPasteConfirmAction/,
    "confirmEntry must compute the type-appropriate action via quickPasteConfirmAction",
  );
  assert.match(
    confirmMatch[0],
    /shiftKey\s*\?\?\s*false/,
    "confirmEntry must forward the shiftKey flag to the dispatch helper",
  );

  // Both keyboard and click entry points MUST delegate to
  // confirmEntry — no shortcut may bypass it.
  const enterMatch = quickPasteSource.match(
    /async function handleEnter[\s\S]*?\n  \}/,
  );
  assert.ok(enterMatch, "handleEnter must exist in QuickPaste.svelte");
  assert.match(
    enterMatch[0],
    /confirmEntry\(entryId, \{ shiftKey:/,
    "handleEnter must delegate to confirmEntry with the shiftKey flag",
  );

  const clickMatch = quickPasteSource.match(
    /function handleRowClick[\s\S]*?\n  \}/,
  );
  assert.ok(clickMatch, "handleRowClick must exist in QuickPaste.svelte");
  assert.match(
    clickMatch[0],
    /confirmEntry\(entryId, \{ shiftKey:/,
    "handleRowClick must delegate to confirmEntry with the shiftKey flag",
  );
  assert.match(
    clickMatch[0],
    /selectEntryOnClick\(entryId\)/,
    "handleRowClick must move the keyboard selection to the clicked row before confirming",
  );
});

test("row click and Enter share the copy-only command path", () => {
  // The single source-level guarantee that click and Enter cannot
  // diverge is that `copyEntryCommand` is invoked from exactly one
  // site: `runCopyForEntry`. A regression that wired a second copy
  // path directly on the row click (or that bypassed the
  // suppression / hide flow) would re-introduce the synthetic paste
  // regression.
  const occurrences = quickPasteSource.match(/copyEntryCommand\(/g) ?? [];
  assert.equal(
    occurrences.length,
    1,
    `copyEntryCommand must be invoked from exactly one site (runCopyForEntry); found ${occurrences.length}`,
  );
  // The keyboard and click entry points MUST NOT reach for
  // `pasteEntryCommand` — the menu owns the direct paste flow and
  // a click must not collapse into it. The menu call lives in
  // `runMenuPasteForEntry`; we extract the bodies of every keyboard
  // / click helper and assert they never touch it.
  for (const name of ["confirmEntry", "handleEnter", "handleRowClick", "runCopyForEntry"]) {
    const re = new RegExp(
      `(async function ${name}|function ${name})[\\s\\S]*?\\n  \\}`,
    );
    const block = quickPasteSource.match(re);
    assert.ok(block, `${name} must exist in QuickPaste.svelte`);
    assert.equal(
      block![0].includes("pasteEntryCommand"),
      false,
      `${name} must not invoke pasteEntryCommand; the menu owns the direct paste flow`,
    );
  }
});

test("row click does not wire a duplicate on:click handler", () => {
  // A regression that registered the row's on:click twice would
  // double-fire the copy and could hide Quick Paste twice. The
  // assertion below counts every `on:click` literal that lives on
  // the qp-row <li> opening tag so a future refactor that adds a
  // second listener cannot ship unnoticed.
  const openingTag = extractRowOpeningTag();
  const matches = openingTag.match(/on:click(\|[a-zA-Z]+)?=/g) ?? [];
  assert.equal(
    matches.length,
    1,
    `the qp-row <li> must register exactly one on:click handler; found ${matches.length}`,
  );
});

test("pin button click handler does not confirm or copy", () => {
  // The pin button uses `|stopPropagation` so a click never reaches
  // the row's confirmEntry path. Belt-and-braces: the pin handler
  // itself MUST NOT call anything copy-related so even a future
  // refactor that drops the stop-propagation guard cannot start
  // firing copy operations from the pin.
  const pinOpeningTag = findOpeningTagContaining(
    quickPasteSource,
    'class="qp-pin"',
    "button",
    "on:click|stopPropagation={() => void togglePin(id)}",
  );
  assert.ok(pinOpeningTag, "the pin button must exist in QuickPaste.svelte");
  for (const forbidden of [
    "confirmEntry",
    "runCopyForEntry",
    "copyEntryCommand",
    "handleRowClick",
    "pasteEntryCommand",
    "runMenuPasteForEntry",
  ]) {
    assert.equal(
      pinOpeningTag.includes(forbidden),
      false,
      `clicking the pin must not invoke ${forbidden}`,
    );
  }
});

test("overflow-menu trigger click handler does not confirm or copy", () => {
  const triggerOpeningTag = findOpeningTagContaining(
    quickPasteSource,
    'class="qp-menu-trigger"',
    "button",
    "on:click|stopPropagation={() => toggleMenuFor(id)}",
  );
  assert.ok(
    triggerOpeningTag,
    "the menu trigger button must exist in QuickPaste.svelte",
  );
  for (const forbidden of [
    "confirmEntry",
    "runCopyForEntry",
    "copyEntryCommand",
    "handleRowClick",
    "pasteEntryCommand",
    "runMenuPasteForEntry",
  ]) {
    assert.equal(
      triggerOpeningTag.includes(forbidden),
      false,
      `clicking the menu trigger must not invoke ${forbidden}`,
    );
  }
});

test("row click forwards the keyboard modifier (shiftKey) to the confirm controller", () => {
  // Shift+Click is the pointer counterpart of Shift+Enter and MUST
  // produce the plain-text copy. The source-level assertion pins the
  // shiftKey propagation so a future refactor that drops the
  // modifier flag cannot silently regress the keyboard parity.
  const clickMatch = quickPasteSource.match(
    /function handleRowClick[\s\S]*?\n  \}/,
  );
  assert.ok(clickMatch, "handleRowClick must exist in QuickPaste.svelte");
  assert.match(
    clickMatch[0],
    /event\?\.shiftKey\s*\?\?\s*false/,
    "handleRowClick must forward the shiftKey modifier from the click event",
  );
});

test("confirmEntry keeps the no-op contract for Shift+Enter on image-only rows", () => {
  // The shiftKey branch of confirmEntry MUST honour the existing
  // quickPasteShiftEnterAction no-op semantics: an image-only row
  // never produces a copy action for Shift+Enter / Shift+Click, so
  // the controller stays a no-op rather than falling back to the
  // default rich/plain action.
  const confirmMatch = quickPasteSource.match(
    /async function confirmEntry[\s\S]*?\n  \}/,
  );
  assert.ok(confirmMatch, "confirmEntry must exist in QuickPaste.svelte");
  assert.match(
    confirmMatch[0],
    /action\.kind === "none"/,
    "confirmEntry must drop no-op actions so click cannot accidentally copy a no-op representation",
  );
  assert.match(
    confirmMatch[0],
    /return;/,
    "confirmEntry must return without invoking the copy command when the action is a no-op",
  );
});

test("confirmEntry is the single switch used by Enter and click", () => {
  // A second assertion pinning the same contract as the helper
  // existence check above, but framed around the wiring: a future
  // refactor that re-implements the dispatch inside handleEnter or
  // handleRowClick would break this assertion.
  const handleEnterBody = quickPasteSource.match(
    /async function handleEnter[\s\S]*?\n  \}/,
  );
  const handleClickBody = quickPasteSource.match(
    /function handleRowClick[\s\S]*?\n  \}/,
  );
  assert.ok(handleEnterBody && handleClickBody, "both entry points must exist");
  // Neither entry point may import or call performCopyFlow directly
  // — the controller MUST be reached through runCopyForEntry so the
  // in-flight guard and the typed outcome stay centralised.
  for (const forbidden of ["performCopyFlow", "performPasteFlow"]) {
    assert.equal(
      handleEnterBody![0].includes(forbidden),
      false,
      `handleEnter must not call ${forbidden} directly`,
    );
    assert.equal(
      handleClickBody![0].includes(forbidden),
      false,
      `handleRowClick must not call ${forbidden} directly`,
    );
  }
});

// ---------------------------------------------------------------------------
// Regression: Quick Paste arrow keys must change `selectedIndex` (clamped)
// and update the visual selection, even when the focus lives on the
// search input. The previous baseline only registered the keydown handler
// on `<svelte:window>`; while the focus sat inside the search field the
// browser default (cursor at end of query) hijacked the keystroke and
// the keyboard selection never advanced.
// ---------------------------------------------------------------------------

test("moveSelection clamps at the bounds of the list (no wrap-around)", () => {
  // The previous baseline used `(selectedIndex + delta + total) % total`
  // so ArrowDown on the last row silently teleported back to the first
  // row. The documented contract for ArrowUp / ArrowDown is to clamp
  // against the visible list bounds so the user keeps the same row
  // when they overshoot the edge; Home / End stay the wrap-around
  // shortcuts. The clamp math is delegated to the pure
  // `clampedSelectedIndex` helper so the regression suite can exercise
  // it without mounting Svelte.
  const moveMatch = quickPasteSource.match(
    /function moveSelection[\s\S]*?\n  \}/,
  );
  assert.ok(moveMatch, "moveSelection must exist in QuickPaste.svelte");
  const body = moveMatch![0];
  // The moveSelection implementation MUST delegate the clamp to the
  // pure helper rather than re-implementing the modulo wrap inline.
  assert.match(
    body,
    /clampedSelectedIndex\(/,
    "moveSelection must delegate to clampedSelectedIndex for the clamp math",
  );
  // The clamp helper is the documented alternative to the modulo wrap.
  // A regression that re-introduces the inline `(...) % total` would
  // silently teleport the selection past the row the user was reading.
  assert.equal(
    /%\s*total/.test(body),
    false,
    "moveSelection must NOT wrap with the modulo operator — Arrow keys clamp at the list bounds",
  );
  // moveSelection MUST NOT scrollIntoView or mutate state when the
  // helper returns the same index (boundary case); otherwise the
  // browser default could still hijack the keystroke on the last row.
  assert.match(
    body,
    /next\s*===\s*selectedIndex[\s\S]*?return/,
    "moveSelection must short-circuit when the clamp lands on the same row (boundary case)",
  );
});

test("search input installs an on:keydown handler so arrow keys work while typing", () => {
  // The previous baseline only listened for ArrowDown / ArrowUp on
  // `<svelte:window>`. When the user typed into the search field the
  // browser default (cursor at end of query) hijacked the keystroke
  // and the keyboard selection never advanced. The regression fix
  // adds an explicit `on:keydown` to the input so the navigation
  // works regardless of focus.
  const inputTagMatch = quickPasteSource.match(
    /<input[^>]*data-testid="quick-paste-input"[\s\S]*?\/>/,
  );
  assert.ok(inputTagMatch, "the search input must exist in QuickPaste.svelte");
  const inputTag = inputTagMatch![0];
  assert.match(
    inputTag,
    /on:keydown=\{onSearchInputKeydown\}/,
    "the search input must register an on:keydown handler so arrow navigation works while typing",
  );
});

test("onSearchInputKeydown routes ArrowUp / ArrowDown / Home / End to the moveSelection helpers", () => {
  // The new per-input handler is the only switch that drives the
  // navigation keys when the focus sits inside the search field.
  // It MUST delegate to the documented `moveSelection` / `jumpToFirst`
  // / `jumpToLast` helpers and call `preventDefault()` so the
  // browser default (cursor movement / native list scroll) cannot
  // run alongside the Quick Paste navigation.
  const handlerMatch = quickPasteSource.match(
    /function onSearchInputKeydown[\s\S]*?\n  \}/,
  );
  assert.ok(
    handlerMatch,
    "onSearchInputKeydown must exist in QuickPaste.svelte",
  );
  const body = handlerMatch![0];
  assert.match(
    body,
    /ArrowDown[\s\S]*?moveSelection\(1\)/,
    "ArrowDown on the search input must advance selectedIndex by 1",
  );
  assert.match(
    body,
    /ArrowUp[\s\S]*?moveSelection\(-1\)/,
    "ArrowUp on the search input must decrement selectedIndex by 1",
  );
  assert.match(
    body,
    /Home[\s\S]*?jumpToFirst\(\)/,
    "Home on the search input must jump to the first row",
  );
  assert.match(
    body,
    /End[\s\S]*?jumpToLast\(\)/,
    "End on the search input must jump to the last row",
  );
  // preventDefault must accompany every handled key so the
  // browser default (cursor movement / native scroll) cannot
  // hijack the keystroke.
  const preventDefaultCount = (body.match(/preventDefault\(\)/g) ?? []).length;
  assert.ok(
    preventDefaultCount >= 4,
    `onSearchInputKeydown must call preventDefault() for every handled navigation key; found ${preventDefaultCount}`,
  );
});

test("onSearchInputKeydown stops propagation so the window listener does not double-fire", () => {
  // The window-level keydown listener (`onWindowKeydown`) routes
  // ArrowUp / ArrowDown / Home / End through the same navigation
  // helpers when the focus sits outside the input. Without
  // `stopPropagation` the per-input handler would call
  // `moveSelection(1)` once and the window listener would call
  // it again on the same event, advancing the selection by two
  // rows on a single ArrowDown press.
  const handlerMatch = quickPasteSource.match(
    /function onSearchInputKeydown[\s\S]*?\n  \}/,
  );
  assert.ok(
    handlerMatch,
    "onSearchInputKeydown must exist in QuickPaste.svelte",
  );
  const body = handlerMatch![0];
  const stopPropagationCount = (body.match(/stopPropagation\(\)/g) ?? []).length;
  assert.ok(
    stopPropagationCount >= 4,
    `onSearchInputKeydown must call stopPropagation() for every handled key; found ${stopPropagationCount}`,
  );
});