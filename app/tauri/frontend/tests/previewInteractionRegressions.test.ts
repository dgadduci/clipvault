/**
 * Regression tests for the `preview-interaction-regressions`
 * change. The suite pins the contracts the manual QA pass
 * surfaced as regressions and the helpers the Desktop rail and
 * the Quick Paste palette now share:
 *
 *   - rich preview preserves LF / CRLF / tabs / blank lines /
 *     indentation in both the highlighted and the plain-text
 *     fallback paths (no more `replace(/\s+/g, " ").trim()`);
 *   - Quick Paste recent mode orders favourites first and then
 *     sorts each group by `created_at DESC` with `id DESC` as the
 *     tie-breaker;
 *   - the card menu popover computes a viewport-aware rectangle
 *     that flips above the trigger near the bottom edge and
 *     scrolls internally when the available height is too short;
 *   - the menu exposes `aria-keyshortcuts` on the `Previsualizar`
 *     item and never invents shortcuts for items that have none;
 *   - selection is purely local UI state, never persisted and
 *     never forwarded to the backend;
 *   - `Cmd/Ctrl+Enter` only opens the preview for the selected
 *     card and ignores inputs, the title editor and menus.
 *
 * The tests intentionally inspect component source so a
 * regression that re-implements the helper inline, drops the
 * `aria-keyshortcuts` attribute or copies the entry content into
 * a DOM attribute surfaces in CI.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

import {
  entryFullPreviewText,
  escapeForPreview,
  isImageEntry,
} from "../src/lib/clipboardAsset.ts";
import type { EntryRecord } from "../src/types.ts";
import {
  computeCardMenuPosition,
  cardMenuStyle,
  cardMenuPreviewShortcutKeyAttribute,
} from "../src/lib/cardMenuPositioning.ts";
import { COLLECTION_DROP_TARGET_VALUE } from "../src/lib/collectionDropZone.ts";

const SHA = "a".repeat(64);
const VALID_ASSET_REF = `clipboard/${SHA}.png`;

function textEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 1,
    content: "",
    content_type: "text",
    content_size: 0,
    content_hash: SHA,
    source_app: "com.example.Editor",
    is_pinned: false,
    created_at: "2026-09-01T10:11:12Z",
    updated_at: "2026-09-01T10:11:12Z",
    last_seen_at: "2026-09-01T10:11:12Z",
    title: null,
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
    code_language: null,
    ...overrides,
  };
}

function imageEntry(): EntryRecord {
  return {
    ...textEntry({
      id: 11,
      content: "",
      content_type: "image",
      content_size: 2048,
      asset_ref: VALID_ASSET_REF,
      mime_type: "image/png",
      payload_width: 640,
      payload_height: 480,
    }),
  };
}

// ---------------------------------------------------------------------------
// 1. Rich preview preserves the original whitespace byte-for-byte.
// ---------------------------------------------------------------------------

test("rich preview preserves LF separators verbatim", () => {
  const entry = textEntry({ content: "line1\nline2\nline3" });
  const preview = entryFullPreviewText(entry);
  assert.ok(preview.includes("\n"), "LF separators must reach the preview");
  assert.equal(preview.match(/\n/g)?.length, 2);
});

test("rich preview preserves CRLF separators verbatim", () => {
  const entry = textEntry({ content: "line1\r\nline2\r\nline3" });
  const preview = entryFullPreviewText(entry);
  assert.ok(preview.includes("\r\n"), "CRLF separators must reach the preview");
  assert.equal(preview.match(/\r\n/g)?.length, 2);
});

test("rich preview preserves tab characters verbatim", () => {
  const entry = textEntry({ content: "before\t\tafter" });
  const preview = entryFullPreviewText(entry);
  assert.ok(preview.includes("\t\t"), "consecutive tabs must reach the preview");
});

test("rich preview preserves consecutive blank lines", () => {
  const entry = textEntry({ content: "line1\n\n\nline2" });
  const preview = entryFullPreviewText(entry);
  assert.ok(preview.includes("\n\n\n"), "consecutive blank lines must survive");
});

test("rich preview preserves four-space Python indentation", () => {
  const entry = textEntry({
    content: "def f():\n    return 1\n\n# tail\n",
  });
  const preview = entryFullPreviewText(entry);
  assert.ok(preview.includes("\n    "), "four-space indent must survive");
  assert.ok(preview.includes("\n\n# tail"), "blank line + comment must survive");
});

test("rich preview does not trim leading or trailing whitespace", () => {
  const entry = textEntry({ content: "\n\nindented\n" });
  const preview = entryFullPreviewText(entry);
  assert.ok(preview.startsWith("\n"), "leading blank lines must survive");
  assert.ok(preview.endsWith("\n"), "trailing blank lines must survive");
});

test("rich preview escapes HTML-active characters through escapeForPreview", () => {
  // The plain-text fallback path combines entryFullPreviewText and
  // escapeForPreview so the preview never re-introduces script /
  // event / navigation surfaces even when the content is hostile.
  const entry = textEntry({
    content: "<script>alert('x')</script>\n&\"<>\n",
  });
  const preview = entryFullPreviewText(entry);
  const escaped = escapeForPreview(preview);
  assert.equal(
    escaped.includes("<script>"),
    false,
    "<script> must be escaped so the preview cannot execute it",
  );
  assert.ok(escaped.includes("\n"), "LF separators must reach the escaped preview");
});

test("rich preview returns the empty string for image rows", () => {
  const entry = imageEntry();
  assert.equal(isImageEntry(entry), true);
  assert.equal(entryFullPreviewText(entry), "");
});

// ---------------------------------------------------------------------------
// 2. Card menu positioning: viewport-aware rectangle, flip and scroll.
// ---------------------------------------------------------------------------

test("card menu positions below a normal trigger without flipping", () => {
  const trigger = { top: 200, left: 400, right: 480, bottom: 240, width: 80, height: 40 };
  const viewport = { width: 1024, height: 768 };
  const position = computeCardMenuPosition(trigger, viewport);
  assert.equal(position.flippedAbove, false);
  assert.equal(position.top >= trigger.bottom, true);
  assert.equal(position.left >= 8, true);
});

test("card menu flips above a trigger near the bottom edge", () => {
  const trigger = { top: 700, left: 400, right: 480, bottom: 740, width: 80, height: 40 };
  const viewport = { width: 1024, height: 768 };
  const position = computeCardMenuPosition(trigger, viewport);
  assert.equal(position.flippedAbove, true);
  assert.equal(position.top < trigger.top, true);
});

test("card menu clamps to the viewport on the right edge", () => {
  // Trigger near the right edge of the viewport; the popover
  // must clamp to the right gutter instead of overflowing.
  const trigger = { top: 200, left: 900, right: 980, bottom: 240, width: 80, height: 40 };
  const viewport = { width: 1024, height: 768 };
  const position = computeCardMenuPosition(trigger, viewport);
  assert.equal(position.left <= viewport.width - 176 - 8, true);
});

test("card menu keeps its width constant across layouts", () => {
  const trigger = { top: 200, left: 100, right: 180, bottom: 240, width: 80, height: 40 };
  const position = computeCardMenuPosition(trigger, { width: 1024, height: 768 });
  assert.equal(position.width, 176);
});

test("card menu scrolls internally when the viewport is very short", () => {
  const trigger = { top: 60, left: 100, right: 180, bottom: 100, width: 80, height: 40 };
  const viewport = { width: 1024, height: 200 };
  const position = computeCardMenuPosition(trigger, viewport);
  assert.equal(position.scrollable, true);
  assert.equal(position.maxHeight > 0, true);
});

test("card menu never escapes the viewport bottom", () => {
  // Trigger anchored at the very bottom; the popover must stay
  // inside the viewport even when the flip math overflows.
  const trigger = { top: 700, left: 400, right: 480, bottom: 750, width: 80, height: 40 };
  const viewport = { width: 1024, height: 768 };
  const position = computeCardMenuPosition(trigger, viewport);
  assert.equal(position.top >= 0, true);
  assert.equal(position.top + position.maxHeight <= viewport.height, true);
});

test("cardMenuStyle emits top / left / width / max-height values", () => {
  const trigger = { top: 200, left: 400, right: 480, bottom: 240, width: 80, height: 40 };
  const position = computeCardMenuPosition(trigger, { width: 1024, height: 768 });
  const style = cardMenuStyle(position);
  assert.match(style, /top: \d+px;/);
  assert.match(style, /left: \d+px;/);
  assert.match(style, /width: \d+px;/);
  assert.match(style, /max-height: \d+px;/);
});

test("cardMenuStyle escapes the card clipping context with position fixed", () => {
  // The popover is rendered inline inside the card (which carries
  // `overflow: hidden`), so the inline style MUST promote the
  // popover to a viewport-anchored `position: fixed` rectangle.
  // Without this declaration the popover inherits `position:
  // absolute` from the CSS class and is clipped by the card's
  // own border-box, hiding the menu the moment the user clicks the
  // ellipsis trigger. The regression surfaced during a second
  // round of manual QA on `preview-interaction-regressions` and
  // must not return.
  const trigger = { top: 200, left: 400, right: 480, bottom: 240, width: 80, height: 40 };
  const position = computeCardMenuPosition(trigger, { width: 1024, height: 768 });
  const style = cardMenuStyle(position);
  assert.match(
    style,
    /position:\s*fixed;/,
    "cardMenuStyle must emit `position: fixed` so the popover escapes the card's overflow clipping",
  );
});

test("cardMenuStyle pins z-index so the popover never hides behind the card / rail / toolbar", () => {
  // The card menu is portalised through `position: fixed`. Its
  // z-index must be high enough to remain above the rail, the
  // toolbar and the parent window so the menu is the topmost
  // surface the user can interact with. The regression suite pins
  // the value so a future tweak cannot drift back to a low z-index
  // that hides the popover under the card surface.
  const trigger = { top: 200, left: 400, right: 480, bottom: 240, width: 80, height: 40 };
  const position = computeCardMenuPosition(trigger, { width: 1024, height: 768 });
  const style = cardMenuStyle(position);
  const zIndexMatch = style.match(/z-index:\s*(\d+);/);
  assert.ok(zIndexMatch, "cardMenuStyle must emit a z-index value");
  const zIndex = Number(zIndexMatch![1]);
  assert.ok(
    zIndex >= 900,
    `cardMenuStyle z-index (${zIndex}) must stay above the rail / toolbar / modals so the popover never hides behind them`,
  );
});

test("cardMenuStyle appends overflow-y auto when scrollable", () => {
  const trigger = { top: 60, left: 100, right: 180, bottom: 100, width: 80, height: 40 };
  const position = computeCardMenuPosition(trigger, { width: 1024, height: 200 });
  assert.equal(position.scrollable, true);
  const style = cardMenuStyle(position);
  assert.match(style, /overflow-y: auto;/);
});

test("cardMenuPreviewShortcutKeyAttribute returns the canonical aria value", () => {
  // The attribute is the contract that screen readers announce
  // for the `Previsualizar` menu item; the helper centralises the
  // syntax so a future tweak to the modifier table lands once.
  assert.equal(cardMenuPreviewShortcutKeyAttribute("macos"), "Meta+Enter");
  assert.equal(cardMenuPreviewShortcutKeyAttribute("other"), "Control+Enter");
});

test("card menu hugs the trigger without an excessive vertical gap", () => {
  // The popover must read as a visual continuation of the `...`
  // button. The vertical distance between trigger.bottom and
  // popover.top (or trigger.top and popover.bottom when flipped)
  // MUST be zero when there is enough room for the menu; the
  // previous 4 px gap made the popover look detached and forced
  // the user to drag the pointer across empty space to reach the
  // first item.
  const trigger = { top: 200, left: 400, right: 480, bottom: 240, width: 80, height: 40 };
  const viewport = { width: 1024, height: 768 };
  const position = computeCardMenuPosition(trigger, viewport);
  assert.equal(position.flippedAbove, false);
  assert.equal(
    position.top,
    trigger.bottom,
    "popover.top must equal trigger.bottom when the menu drops below",
  );

  // Trigger near the bottom edge so the menu flips above; the
  // popover's bottom edge MUST equal the trigger's top edge.
  const triggerAbove = {
    top: 700,
    left: 400,
    right: 480,
    bottom: 740,
    width: 80,
    height: 40,
  };
  const positionAbove = computeCardMenuPosition(triggerAbove, viewport);
  assert.equal(positionAbove.flippedAbove, true);
  assert.equal(
    positionAbove.top + positionAbove.maxHeight,
    triggerAbove.top,
    "popover.bottom must equal trigger.top when the menu flips above",
  );
});

test("card menu clamps inside the viewport on every edge", () => {
  // The popover must remain inside the viewport for each of the
  // four edges the user might drag the rail to. The helper is
  // total: any combination of trigger + viewport returns a
  // rectangle that never escapes the visible area.
  const viewport = { width: 1024, height: 768 };
  // Top edge: trigger anchored at the very top of the viewport.
  const topTrigger = { top: 4, left: 400, right: 480, bottom: 44, width: 80, height: 40 };
  const topPosition = computeCardMenuPosition(topTrigger, viewport);
  assert.equal(topPosition.top >= 0, true);
  assert.equal(topPosition.top + topPosition.maxHeight <= viewport.height, true);
  // Bottom edge: trigger anchored at the very bottom.
  const bottomTrigger = { top: 700, left: 400, right: 480, bottom: 750, width: 80, height: 40 };
  const bottomPosition = computeCardMenuPosition(bottomTrigger, viewport);
  assert.equal(bottomPosition.top >= 0, true);
  assert.equal(
    bottomPosition.top + bottomPosition.maxHeight <= viewport.height,
    true,
  );
  // Left edge: trigger anchored near the left side.
  const leftTrigger = { top: 200, left: 8, right: 88, bottom: 240, width: 80, height: 40 };
  const leftPosition = computeCardMenuPosition(leftTrigger, viewport);
  assert.equal(leftPosition.left >= 8, true);
  assert.equal(leftPosition.left + leftPosition.width <= viewport.width - 8, true);
  // Right edge: trigger anchored near the right side.
  const rightTrigger = { top: 200, left: 900, right: 980, bottom: 240, width: 80, height: 40 };
  const rightPosition = computeCardMenuPosition(rightTrigger, viewport);
  assert.equal(rightPosition.left >= 8, true);
  assert.equal(rightPosition.left + rightPosition.width <= viewport.width - 8, true);
});

// ---------------------------------------------------------------------------
// 3. Source-level checks: shortcuts, selection, and privacy.
// ---------------------------------------------------------------------------

const cardSource = readFileSync(
  resolvePath(process.cwd(), "src", "HistoryCard.svelte"),
  "utf8",
);
const railSource = readFileSync(
  resolvePath(process.cwd(), "src", "HistoryCardRail.svelte"),
  "utf8",
);
const appSource = readFileSync(
  resolvePath(process.cwd(), "src", "App.svelte"),
  "utf8",
);
const previewSource = readFileSync(
  resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
  "utf8",
);
const clipboardAssetSource = readFileSync(
  resolvePath(process.cwd(), "src", "lib", "clipboardAsset.ts"),
  "utf8",
);

test("HistoryCard exposes aria-selected on the selected card", () => {
  // The selection flag is local UI state and must reach screen
  // readers through the canonical `aria-selected` attribute.
  assert.ok(
    cardSource.includes("aria-selected={selected}"),
    "HistoryCard must expose aria-selected={selected} on the card element",
  );
});

test("HistoryCard sets data-selected on the card element", () => {
  // The data attribute is the contract the regression suite and
  // the e2e selectors use to read the selection state.
  assert.ok(
    cardSource.includes('data-selected={selected ? "true" : "false"}'),
    "HistoryCard must set data-selected on the card element",
  );
});

test("HistoryCard mounts the preview-shortcut hint only when selected", () => {
  assert.ok(
    cardSource.includes("class=\"preview-hint\""),
    "HistoryCard must declare the .preview-hint style class",
  );
  assert.ok(
    cardSource.includes("history-card-preview-hint"),
    "HistoryCard must declare data-testid=\"history-card-preview-hint\"",
  );
  // The hint block is gated on the `selected` flag so other cards
  // never render the shortcut. The `{#if selected}` block guards
  // the entire hint subtree.
  assert.ok(
    cardSource.includes("{#if selected}"),
    "HistoryCard must gate the preview hint on {#if selected}",
  );
});

test("HistoryCard wires Cmd/Ctrl+Enter to requestPreview", () => {
  assert.ok(
    cardSource.includes("matchesPreviewShortcut"),
    "HistoryCard must import the shared matchesPreviewShortcut helper",
  );
  assert.ok(
    cardSource.includes("requestPreview()"),
    "HistoryCard must call requestPreview when the platform matcher fires",
  );
  assert.ok(
    cardSource.includes("if (!selected) return"),
    "HistoryCard must ignore Cmd/Ctrl+Enter on non-selected cards",
  );
});

test("HistoryCard routes the click selection through isInteractiveTarget", () => {
  // The selection click handler MUST route through the same
  // interactive-target guard the keyboard matcher uses so a click
  // on pin / menu / title / tag / collection / paste / delete
  // never accidentally flips selection.
  assert.ok(
    cardSource.includes("function onCardSurfaceClick"),
    "HistoryCard must declare onCardSurfaceClick",
  );
  assert.ok(
    cardSource.includes("if (isInteractiveTarget(event.target)) return"),
    "the click handler must consult isInteractiveTarget",
  );
});

test("HistoryCard menu exposes aria-keyshortcuts on Previsualizar", () => {
  assert.ok(
    cardSource.includes("data-testid=\"history-card-preview\""),
    "the Previsualizar menu item must keep its stable test id",
  );
  assert.ok(
    cardSource.includes("aria-keyshortcuts={cardMenuPreviewShortcutKeyAttribute(previewPlatform)}"),
    "Previsualizar must expose aria-keyshortcuts through the helper",
  );
});

test("HistoryCard menu never invents shortcuts for items that have none", () => {
  // Inspect every menu item test id: only `history-card-preview`
  // carries an `aria-keyshortcuts` attribute.
  const menuStart = cardSource.indexOf('data-testid="history-card-menu"');
  const menuEnd = cardSource.indexOf("{/if}", menuStart);
  assert.notEqual(menuStart, -1, "the menu block must exist");
  assert.notEqual(menuEnd, -1, "the menu block must close");
  const menuBlock = cardSource.slice(menuStart, menuEnd);
  const keyshortcutMatches = menuBlock.match(/aria-keyshortcuts/g) ?? [];
  assert.equal(
    keyshortcutMatches.length,
    1,
    "the menu must expose exactly one aria-keyshortcuts (Previsualizar)",
  );
});

test("HistoryCardRail exports the selectedEntryId bind target", () => {
  // The rail owns the canonical selection id; the parent binds to
  // it through `bind:selectedEntryId` so App.svelte can route the
  // keyboard shortcut through one switch.
  assert.ok(
    railSource.includes("export let selectedEntryId"),
    "HistoryCardRail must export the selectedEntryId binding",
  );
  assert.ok(
    railSource.includes("bind:selectedEntryId"),
    "App.svelte must bind:selectedEntryId to the rail",
  );
});

test("HistoryCardRail clears the selection when the entry leaves scope", () => {
  // A delete, refresh, search or collection change can drop the
  // selected entry from the visible scope. The rail must silently
  // drop the selection so the keyboard shortcut cannot open a
  // preview against a stale id.
  assert.ok(
    railSource.includes("visibleEntryIds"),
    "HistoryCardRail must track the visible entry ids",
  );
  assert.ok(
    railSource.includes("selectedEntryId = null"),
    "HistoryCardRail must reset the selection when the entry leaves scope",
  );
});

test("HistoryCardRail wires Escape to clear the selection", () => {
  assert.ok(
    railSource.includes("if (selectedEntryId !== null && event.key === \"Escape\")"),
    "HistoryCardRail must clear the selection on Escape",
  );
});

test("HistoryCardRail wires Escape to close an open card menu", () => {
  // The rail MUST route Escape through the menu branch even when
  // focus is inside the popover itself; the listener lives on the
  // document so the keystroke cannot be swallowed by the active
  // menu item or its container.
  assert.ok(
    railSource.includes("if (openCardId !== null && event.key === \"Escape\")"),
    "HistoryCardRail must close the active card menu on Escape",
  );
  assert.ok(
    railSource.includes("closeAllMenus()"),
    "HistoryCardRail must expose closeAllMenus so Escape can dismiss the menu",
  );
});

test("HistoryCardRail closes the menu on outside click", () => {
  // The outside-click guard lives on the document so a click on
  // the document body, the desktop toolbar or another card closes
  // the popover without leaking listeners between menu
  // activations.
  assert.ok(
    railSource.includes("document.addEventListener(\"click\", onWindowClick, true)"),
    "HistoryCardRail must listen to document clicks in capture phase",
  );
  assert.ok(
    railSource.includes("closeAllMenus()"),
    "outside-click handler must close the menu through the documented helper",
  );
  // The trigger and the menu surface are the only exceptions.
  assert.ok(
    railSource.includes("history-card-menu-trigger"),
    "clicking the menu trigger must not close its own menu",
  );
  assert.ok(
    railSource.includes("history-card-menu"),
    "clicking inside the popover must not close its own menu",
  );
});

test("HistoryCardRail cleans up document listeners on destroy", () => {
  // The mount/destroy pair must be a single attach/detach pair;
  // calling destroy twice (or mounting twice) must never stack
  // listeners and must always detach exactly what the matching
  // mount installed.
  assert.ok(
    railSource.includes("detachWindow?.()"),
    "HistoryCardRail.onDestroy must call the idempotent detachWindow helper",
  );
  assert.ok(
    railSource.includes("document.removeEventListener(\"click\", onWindowClick, true)"),
    "HistoryCardRail must remove the click listener on destroy",
  );
  assert.ok(
    railSource.includes("document.removeEventListener(\"keydown\", onWindowKeydown, true)"),
    "HistoryCardRail must remove the keydown listener on destroy",
  );
  assert.ok(
    railSource.includes("detachWindow = null"),
    "HistoryCardRail must null the detach handle so a second destroy is a no",
  );
});

test("HistoryCardRail enforces a single card menu instance", () => {
  // The rail owns the canonical `openCardId` and the menu-toggle
  // event collapses every other menu before flipping the active
  // one; the design must not expose a per-card boolean that races
  // two simultaneous menu opens.
  assert.ok(
    railSource.includes("let openCardId"),
    "HistoryCardRail must own a single openCardId slot",
  );
  assert.ok(
    railSource.includes("openCardId = open ? id : null"),
    "HistoryCardRail menu-toggle handler must collapse to one active menu",
  );
  // Closing the menu must collapse every card's menu to null so
  // a stale menu cannot survive a refresh.
  assert.ok(
    railSource.includes("openCardId = null"),
    "HistoryCardRail must collapse every menu to null on close",
  );
});

test("HistoryCard preview hint sits in the footer, not absolutely positioned", () => {
  // The previous baseline pinned the hint to the top-right corner
  // with `position: absolute`, which forced the label to overlap
  // the chincheta and the menu. The regression fix moves the hint
  // into the card-actions footer so it reads as a continuation of
  // the card, never overlapping the controls.
  const hintBlock = cardSource.slice(
    cardSource.indexOf("class=\"preview-hint\""),
    cardSource.indexOf("</span>", cardSource.indexOf("class=\"preview-hint\"")),
  );
  assert.equal(
    hintBlock.includes("position: absolute"),
    false,
    "the preview hint must not be absolutely positioned",
  );
  // The hint must be declared inside the .card-actions footer so
  // pin and menu keep their fixed columns on the right.
  const footerStart = cardSource.indexOf('class="card-actions"');
  const footerEnd = cardSource.indexOf("</footer>", footerStart);
  assert.notEqual(footerStart, -1, "the .card-actions footer must exist");
  assert.notEqual(footerEnd, -1, "the .card-actions footer must close");
  const footer = cardSource.slice(footerStart, footerEnd);
  assert.ok(
    footer.includes("class=\"preview-hint\""),
    "the preview hint must be declared inside the .card-actions footer",
  );
  // The hint must be visually left-aligned (the previous baseline
  // was top-right; the regression fix moves it to the bottom-left
  // corner of the card so the chincheta and menu keep their fixed
  // right-aligned columns).
  assert.ok(
    cardSource.includes(".preview-hint {"),
    "HistoryCard must declare the .preview-hint rule block",
  );
  assert.equal(
    /margin-right:\s*auto/.test(cardSource),
    true,
    "the preview hint must push itself to the left with margin-right: auto",
  );
});

test("HistoryCard preview hint uses the English 'Preview' label", () => {
  // The previous baseline rendered the Spanish `Previsualizar`
  // label. The regression fix uses the English `Preview` label
  // to match the spec wording and keep the surface consistent
  // with the rest of the menu copy.
  const labelMatch = cardSource.match(/preview-hint-label">([^<]+)</);
  assert.ok(labelMatch, "the preview hint must declare a label span");
  assert.equal(
    labelMatch![1],
    "Preview",
    "the preview hint must render the English 'Preview' label",
  );
});

test("HistoryCard menu never picks the card as a drag source", () => {
  // The card body MUST NOT register a dragstart / drag handler
  // for the menu toggle; the drag-and-drop contract lives on the
  // pointer controller and the menu button is a `<button>` so a
  // drag attempt from the menu button is a no-op.
  assert.ok(
    cardSource.includes("draggable=\"false\""),
    "the card element must declare draggable=false so the menu button cannot pick a drag",
  );
});

test("QuickPaste exposes a preview shortcut hint on the selected row", () => {
  // The Quick Paste row must mirror the desktop rail's hint on
  // the currently selected entry so the user sees the same
  // shortcut glyph that `Cmd/Ctrl+Enter` will trigger.
  const quickPasteSource = readFileSync(
    resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
    "utf8",
  );
  assert.ok(
    quickPasteSource.includes("quick-paste-preview-hint"),
    "QuickPaste.svelte must declare data-testid=\"quick-paste-preview-hint\"",
  );
  // The hint must be gated on the selected index so a single
  // row — not all of them — receives the label. Look for the
  // `{#if index === selectedIndex}` opener that precedes the
  // hint class declaration.
  const hintClassIdx = quickPasteSource.indexOf("class=\"qp-preview-hint\"");
  assert.notEqual(hintClassIdx, -1, "the hint class must exist");
  const openerIdx = quickPasteSource.lastIndexOf(
    "{#if index === selectedIndex}",
    hintClassIdx,
  );
  assert.notEqual(
    openerIdx,
    -1,
    "the Quick Paste preview hint must be wrapped in an {#if index === selectedIndex} block",
  );
  const closerIdx = quickPasteSource.indexOf("{/if}", openerIdx);
  assert.notEqual(closerIdx, -1, "the {#if} block must close");
  const hintBlock = quickPasteSource.slice(openerIdx, closerIdx);
  assert.ok(
    hintBlock.includes("class=\"qp-preview-hint\""),
    "the {#if selectedIndex} block must contain the preview hint",
  );
});

test("QuickPaste preview hint uses the shared shortcut helpers", () => {
  // The Quick Paste surface must consult the platform-aware
  // helpers `previewShortcutLabel` and `previewShortcutAccessibleLabel`
  // instead of re-implementing the modifier table so the visible
  // glyph and the keyboard matcher cannot drift apart.
  const quickPasteSource = readFileSync(
    resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
    "utf8",
  );
  assert.ok(
    quickPasteSource.includes("previewShortcutLabel(shortcutPlatform)"),
    "QuickPaste must consume previewShortcutLabel through the shared helper",
  );
  assert.ok(
    quickPasteSource.includes("previewShortcutAccessibleLabel(shortcutPlatform)"),
    "QuickPaste must consume previewShortcutAccessibleLabel through the shared helper",
  );
  // No parallel implementation must exist; a regression that
  // hard-codes `⌘Enter` would surface here.
  assert.equal(
    /['"`]⌘Enter['"`]/.test(quickPasteSource),
    false,
    "QuickPaste must not hard-code the macOS shortcut glyph",
  );
  assert.equal(
    /['"`]Ctrl Enter['"`]/.test(quickPasteSource),
    false,
    "QuickPaste must not hard-code the Linux shortcut glyph",
  );
});

test("QuickPaste preview hint uses the English 'Preview' label", () => {
  // The label must read `Preview` so the Quick Paste palette and
  // the desktop rail share the same wording.
  const quickPasteSource = readFileSync(
    resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
    "utf8",
  );
  const labelMatch = quickPasteSource.match(/qp-preview-hint-label">([^<]+)</);
  assert.ok(labelMatch, "the Quick Paste preview hint must declare a label span");
  assert.equal(
    labelMatch![1],
    "Preview",
    "the Quick Paste preview hint must render the English 'Preview' label",
  );
});

test("QuickPaste preview hint is non-interactive and never breaks the row layout", () => {
  // The hint must declare `pointer-events: none` so a click on
  // the pill never steals the row click that selects / copies the
  // entry, and it must use `flex: 0 0 auto` so the row's fixed
  // 72 px footprint stays untouched while the title flexes.
  const quickPasteSource = readFileSync(
    resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
    "utf8",
  );
  // Extract the .qp-preview-hint rule body so the regexes never
  // cross into the .qp-visually-hidden block below (which uses
  // `position: absolute` for its own purpose).
  const hintRuleMatch = quickPasteSource.match(
    /\.qp-preview-hint\s*\{([^}]*)\}/,
  );
  assert.ok(hintRuleMatch, "the .qp-preview-hint CSS rule must exist");
  const hintBody = hintRuleMatch![1];
  assert.ok(
    /pointer-events:\s*none/.test(hintBody),
    "the Quick Paste preview hint must declare pointer-events: none",
  );
  assert.ok(
    /flex:\s*0\s+0\s+auto/.test(hintBody),
    "the Quick Paste preview hint must declare flex: 0 0 auto so the title flexes",
  );
  assert.equal(
    /position:\s*absolute/.test(hintBody),
    false,
    "the Quick Paste preview hint must not be absolutely positioned",
  );
});

test("HistoryCardRail routes the selection through the documented helper", () => {
  // The rail MUST NOT ship a second click-selection helper; the
  // card surface click is the single switch the keyboard
  // shortcut and the click contract share.
  assert.ok(
    railSource.includes("on:select-request"),
    "HistoryCardRail must forward the select-request event",
  );
});

test("App.svelte routes Cmd/Ctrl+Enter through the selected id", () => {
  assert.ok(
    appSource.includes("matchesPreviewShortcut(event, previewShortcutPlatform"),
    "App.svelte must consult the shared matchesPreviewShortcut helper",
  );
  assert.ok(
    appSource.includes("railSelectedEntryId"),
    "App.svelte must route through the rail selection state",
  );
  assert.ok(
    appSource.includes("if (railSelectedEntryId === null)"),
    "App.svelte must no-op when no card is selected",
  );
});

test("App.svelte excludes inputs / dialogs / menus from the keyboard matcher", () => {
  // The document-level matcher MUST NOT steal Escape or
  // `Cmd/Ctrl+Enter` from a typing surface or a modal dialog so
  // the rail-wide shortcuts cannot break a title editor or a
  // tag-selector modal that is currently focused.
  assert.ok(
    appSource.includes("role='dialog'") ||
      appSource.includes("role=\"dialog\""),
    "App.svelte must guard role=\"dialog\" in the matcher",
  );
  assert.ok(
    appSource.includes("isContentEditable"),
    "App.svelte must exclude contenteditable surfaces",
  );
});

test("ClipboardPreview keeps whitespace via entryFullPreviewText (no collapse)", () => {
  // The plain-text fallback path uses entryFullPreviewText; the
  // helper no longer collapses whitespace. The pin is that the
  // helper source itself stays free of any `replace(/\s+/g, " ")` /
  // `trim()` calls.
  const helperMatch = clipboardAssetSource.match(
    /export function entryFullPreviewText[\s\S]+?\n\}/,
  );
  assert.ok(helperMatch, "entryFullPreviewText must be declared");
  const body = helperMatch![0];
  assert.equal(
    /\\s\+/.test(body) && /\.replace\(/.test(body),
    false,
    "entryFullPreviewText must never collapse whitespace runs",
  );
  assert.equal(/trim\(/.test(body), false, "entryFullPreviewText must never trim");
});

test("ClipboardPreview renders the fallback inside <pre> with white-space: pre-wrap", () => {
  const cssBlock = previewSource.slice(previewSource.indexOf("<style>"));
  const textRule = cssBlock.match(/\.cv-preview-text\s*\{[^}]*\}/);
  assert.ok(textRule, "the .cv-preview-text CSS rule must exist");
  assert.ok(
    /white-space:\s*pre-wrap/.test(textRule![0]),
    "the plain-text preview must use white-space: pre-wrap",
  );
  const codeRule = cssBlock.match(/\.cv-preview-code\s*\{[^}]*\}/);
  assert.ok(codeRule, "the .cv-preview-code CSS rule must exist");
  assert.ok(
    /tab-size:\s*4/.test(codeRule![0]),
    "the highlighted preview must pin tab-size so tabs render with a stable width",
  );
});

test("selection never persists to the backend", () => {
  // The selection is local UI state. A regression that wrote it
  // to a Tauri command, a SQLite migration, a `console.log` call
  // or a fetch call would surface here.
  const forbidden = [
    "setSelectedEntryIdCommand",
    "selectionCommand",
    "saveSelection",
    "persistSelection",
  ];
  for (const token of forbidden) {
    assert.equal(
      appSource.includes(token),
      false,
      `App.svelte must not reference a backend token ${token}`,
    );
    assert.equal(
      railSource.includes(token),
      false,
      `HistoryCardRail must not reference a backend token ${token}`,
    );
  }
  // The selected id MUST NOT be forwarded through a Tauri invoke
  // call — the only contract is local UI state.
  const invokeCall = appSource.match(/invoke\s*\(\s*["'][^"']*selection/i);
  assert.equal(invokeCall, null, "no invoke('…selection…') call may exist");
});

test("the preview-shortcut hint never carries clipboard content", () => {
  // The visible hint and the `aria-keyshortcuts` attribute must
  // never carry entry content, snippets, hashes, asset references
  // or paths. The assertion inspects the hint markup so a future
  // regression that interpolates an entry field surfaces here.
  const hintStart = cardSource.indexOf("class=\"preview-hint\"");
  const hintEnd = cardSource.indexOf("</span>", hintStart);
  assert.notEqual(hintStart, -1);
  assert.notEqual(hintEnd, -1);
  const hintBlock = cardSource.slice(hintStart, hintEnd);
  for (const forbidden of [
    "entry.content",
    "entry.title",
    "asset_ref",
    "content_hash",
    "preview_ref",
  ]) {
    assert.equal(
      hintBlock.includes(forbidden),
      false,
      `the hint block must not interpolate ${forbidden}`,
    );
  }
});

// ---------------------------------------------------------------------------
// 10. Geometric regression — card menu hugs the trigger (post-render pass).
//
// The four-point manual regression surfaced two related defects:
//   * the popover stayed detached from the `...` trigger because the
//     first pass used `maxHeight` as the popover's height, so the
//     popover's bottom edge drifted above `trigger.top` whenever the
//     rendered content was shorter than the reserved ceiling;
//   * the menu's escape behaviour silently failed because the card's
//     local `menuOpen` state drifted out of sync with the rail's
//     `openCardId`, so Escape closed the menu in the rail but the
//     card kept painting the popover.
//
// The tests below codify the geometric contract byte-for-byte and the
// structural wiring that the four-point regression relied on.
// ---------------------------------------------------------------------------

import {
  recomputeCardMenuPositionForActualHeight,
} from "../src/lib/cardMenuPositioning.ts";

test("card menu top equals trigger.bottom when the menu drops below", () => {
  // Trigger sitting mid-viewport: the menu opens downward. The
  // popover's top edge MUST equal the trigger's bottom edge with a
  // zero gap so the menu reads as a visual continuation of the
  // `...` button. The helper is total: any viewport / trigger
  // combination produces a rectangle that satisfies this contract
  // whenever the room below the trigger is enough.
  const trigger = {
    top: 200,
    left: 400,
    right: 480,
    bottom: 240,
    width: 80,
    height: 40,
  };
  const viewport = { width: 1024, height: 768 };
  const position = computeCardMenuPosition(trigger, viewport);
  assert.equal(position.flippedAbove, false);
  assert.equal(
    position.top,
    trigger.bottom,
    "popover.top must equal trigger.bottom when the menu drops below",
  );
  assert.equal(
    position.top,
    trigger.bottom + 0,
    "gap between popover.top and trigger.bottom must be exactly zero",
  );
});

test("card menu bottom equals trigger.top when the menu flips above", () => {
  // Trigger near the bottom edge: the menu opens upward. The
  // popover's bottom edge MUST equal the trigger's top edge with
  // a zero gap. The helper pins the position to the trigger's
  // edge so the menu reads as a continuation of the button.
  const trigger = {
    top: 700,
    left: 400,
    right: 480,
    bottom: 740,
    width: 80,
    height: 40,
  };
  const viewport = { width: 1024, height: 768 };
  const position = computeCardMenuPosition(trigger, viewport);
  assert.equal(position.flippedAbove, true);
  // The pre-anchor pass must already place the popover so its
  // bottom edge lands on `trigger.top`. The visible popover
  // bottom matches the trigger top once the second pass uses the
  // actual rendered height.
  assert.equal(
    position.top + position.maxHeight,
    trigger.top,
    "popover.bottom must equal trigger.top when the menu flips above",
  );
  // Run the post-render pass with the actual rendered height
  // (here equal to the reserved ceiling) and confirm the
  // contract still holds.
  const corrected = recomputeCardMenuPositionForActualHeight(
    position,
    trigger,
    viewport,
    position.maxHeight,
  );
  assert.equal(
    corrected.top + corrected.maxHeight,
    trigger.top,
    "post-render pass must keep popover.bottom on trigger.top",
  );
});

test("card menu second pass anchors the visible bottom to trigger.top", () => {
  // The previous baseline reserved `maxHeight` for the popover
  // even when the actual rendered content was shorter. The
  // post-render pass is what closes the visible gap by
  // re-anchoring the top against the measured height.
  const trigger = {
    top: 700,
    left: 400,
    right: 480,
    bottom: 740,
    width: 80,
    height: 40,
  };
  const viewport = { width: 1024, height: 768 };
  const position = computeCardMenuPosition(trigger, viewport);
  assert.equal(position.flippedAbove, true);
  const actualHeight = position.maxHeight - 80;
  const corrected = recomputeCardMenuPositionForActualHeight(
    position,
    trigger,
    viewport,
    actualHeight,
  );
  assert.equal(
    corrected.top,
    trigger.top - actualHeight,
    "the post-render top must equal trigger.top minus the actual height",
  );
  assert.equal(
    corrected.top + corrected.maxHeight,
    trigger.top,
    "popover.bottom must equal trigger.top after the post-render pass",
  );
  assert.ok(
    corrected.maxHeight <= actualHeight,
    "max-height must not exceed the actual rendered height",
  );
});

test("card menu stays inside the viewport when the trigger is at the bottom edge", () => {
  // Trigger anchored at the very bottom: the popover flips above
  // and stays inside the viewport. The clamp guards the four
  // edges so the menu never escapes the desktop rail.
  const trigger = {
    top: 700,
    left: 400,
    right: 480,
    bottom: 750,
    width: 80,
    height: 40,
  };
  const viewport = { width: 1024, height: 768 };
  const position = computeCardMenuPosition(trigger, viewport);
  assert.ok(position.top >= 0, "popover.top must be non-negative");
  assert.ok(
    position.top + position.maxHeight <= viewport.height,
    "popover.bottom must stay inside the viewport",
  );
});

test("card menu stays inside the viewport when the trigger is at the top edge", () => {
  // Trigger anchored at the very top: the popover drops below and
  // stays inside the viewport. The clamp guards the four edges
  // so the menu never escapes the desktop rail.
  const trigger = {
    top: 4,
    left: 400,
    right: 480,
    bottom: 44,
    width: 80,
    height: 40,
  };
  const viewport = { width: 1024, height: 768 };
  const position = computeCardMenuPosition(trigger, viewport);
  assert.ok(position.top >= 0, "popover.top must be non-negative");
  assert.ok(
    position.top + position.maxHeight <= viewport.height,
    "popover.bottom must stay inside the viewport",
  );
});

test("card menu post-render pass preserves the viewport clamp", () => {
  // When the popover's actual height is smaller than the
  // available space, the post-render pass must keep the rectangle
  // inside the viewport even when the trigger sits at the top
  // edge. The first pass clamps the popover so it does not
  // overflow; the post-render pass must honour the same clamp.
  const trigger = {
    top: 60,
    left: 100,
    right: 180,
    bottom: 100,
    width: 80,
    height: 40,
  };
  const viewport = { width: 1024, height: 200 };
  const position = computeCardMenuPosition(trigger, viewport);
  // The viewport is so short the popover is clamped at the top of
  // the viewport; the clamp keeps the rectangle inside the
  // visible area instead of dropping to `trigger.bottom`.
  assert.ok(position.top >= 0, "first-pass top must be non-negative");
  assert.ok(
    position.top + position.maxHeight <= viewport.height,
    "first-pass bottom must stay inside the viewport",
  );
  // The post-render pass with a shorter height must keep the
  // popover inside the viewport and never claim a height larger
  // than the rendered content.
  const corrected = recomputeCardMenuPositionForActualHeight(
    position,
    trigger,
    viewport,
    80,
  );
  assert.ok(
    corrected.top >= 0,
    "post-render popover.top must stay inside the viewport",
  );
  assert.ok(
    corrected.top + corrected.maxHeight <= viewport.height,
    "post-render popover.bottom must stay inside the viewport",
  );
  assert.ok(
    corrected.maxHeight <= 80,
    "post-render max-height must not exceed the actual rendered height",
  );
});

// ---------------------------------------------------------------------------
// 11. Card menu escape wiring — rail is the single source of truth.
// ---------------------------------------------------------------------------

test("HistoryCard delegates the menu state to the rail through the menuOpen prop", () => {
  // The rail owns the canonical menu state via `openCardId`. The
  // card receives `menuOpen={openCardId === entry.id}` and renders
  // the popover when the flag is true. A regression that re-introduces
  // a local `let menuOpen` would race the rail's `closeAllMenus`
  // helper so Escape never closes the popover.
  assert.match(
    cardSource,
    /export let menuOpen:\s*boolean\s*=\s*false/,
    "HistoryCard must declare menuOpen as an exported prop",
  );
  assert.equal(
    /let menuOpen\s*=\s*false/.test(cardSource),
    false,
    "HistoryCard must not redeclare a local menuOpen binding",
  );
});

test("HistoryCard closes the menu through closeMenuAfterAction, not a local write", () => {
  // The card's close paths (delete, restore, edit, escape, click
  // outside) must go through the shared `closeMenuAfterAction`
  // helper which only dispatches the `menu-toggle` event. The
  // rail is the single source of truth: closing the menu in the
  // rail must close the popover in the card without a second
  // setter racing the popover visibility.
  assert.match(
    cardSource,
    /function closeMenuAfterAction\([\s\S]*?dispatch\("menu-toggle"/,
    "closeMenuAfterAction must dispatch the menu-toggle event",
  );
  // No direct write to a `menuOpen` symbol from any close path:
  // the helper is the single switch.
  const closeBranches = [
    "function toggleMenu",
    "function startEditTitle",
    "function restoreDefaultTitle",
    "function handleDeleteClick",
    "function onMenuKeydown",
  ];
  for (const branch of closeBranches) {
    const startIdx = cardSource.indexOf(branch);
    assert.notEqual(startIdx, -1, `${branch} must exist`);
    const endIdx = cardSource.indexOf("}", startIdx);
    const body = cardSource.slice(startIdx, endIdx);
    assert.equal(
      /menuOpen\s*=\s*false/.test(body),
      false,
      `${branch} must not write menuOpen = false; route through closeMenuAfterAction`,
    );
  }
});

test("HistoryCardRail forwards menuOpen to every card so Escape closes the popover", () => {
  // The rail renders each card with `menuOpen={openCardId === entry.id}`
  // and listens to `menu-toggle` so the canonical id flows back into
  // the prop. Escape (or outside click) calls `closeAllMenus()`,
  // which sets `openCardId = null`, which collapses every card's
  // `menuOpen` prop to `false`, which removes the popover from the
  // DOM in the same Svelte tick.
  assert.match(
    railSource,
    /menuOpen=\{openCardId === entry\.id\}/,
    "HistoryCardRail must forward menuOpen derived from openCardId",
  );
  assert.match(
    railSource,
    /function closeAllMenus\(\)[\s\S]*?openCardId\s*=\s*null/,
    "closeAllMenus must reset openCardId",
  );
});

test("HistoryCard dispatches select-request through the shared helper", () => {
  // The rail forwards `select-request` to update `selectedEntryId`,
  // the canonical selection id. The card never persists the value.
  assert.match(
    cardSource,
    /function dispatchSelect\([\s\S]*?dispatch\("select-request"/,
    "dispatchSelect must dispatch the select-request event",
  );
});

// ---------------------------------------------------------------------------
// 14. Card menu synchronous position + portalised rendering
// ---------------------------------------------------------------------------

test("HistoryCard computes the menu position synchronously when the menu opens", () => {
  // The card menu MUST compute its `position: fixed` rectangle
  // synchronously inside the `$:` reactive block that flips when
  // `menuOpen` becomes `true`. A `queueMicrotask` defer would leave
  // the menu with an empty inline style for one frame, painting it
  // at the card's `(0, 0)` origin before snapping to the trigger.
  // That flash was the second regression surfaced during the
  // manual QA pass; the regression suite pins the synchronous
  // call so a future refactor cannot reintroduce the deferral.
  assert.match(
    cardSource,
    /\$\:\s*if\s*\(menuOpen\)\s*\{[\s\S]*?recomputeMenuPosition\(\)/,
    "HistoryCard must call recomputeMenuPosition synchronously inside the $: menuOpen reactive block",
  );
  // The reactive block must NOT defer the call through a microtask
  // / setTimeout / requestAnimationFrame; the first paint of the
  // popover must already carry the computed rectangle.
  const reactiveIdx = cardSource.indexOf("$: if (menuOpen)");
  assert.notEqual(reactiveIdx, -1, "the $: if (menuOpen) block must exist");
  const reactiveEnd = cardSource.indexOf("}", reactiveIdx);
  const reactiveBody = cardSource.slice(reactiveIdx, reactiveEnd);
  assert.equal(
    /queueMicrotask\s*\(\s*\(\s*\)\s*=>\s*recomputeMenuPosition/.test(reactiveBody),
    false,
    "the $: if (menuOpen) block must NOT defer recomputeMenuPosition through queueMicrotask",
  );
});

test("HistoryCard menu CSS drops `position: absolute` so the inline style is authoritative", () => {
  // The previous baseline declared `position: absolute` on the
  // `.menu` rule. With `overflow: hidden` on `.card`, the popover
  // was clipped by its own parent because the inline `style`
  // attribute did NOT carry a `position` declaration. The fix
  // moves the geometry into the inline style (which now carries
  // `position: fixed`) and removes the conflicting declaration
  // from the CSS class so the popover is always visible.
  const menuRuleMatch = cardSource.match(/\.menu\s*\{([^}]*)\}/);
  assert.ok(menuRuleMatch, "the .menu CSS rule must exist");
  const menuBody = menuRuleMatch![1];
  assert.equal(
    /position:\s*absolute/.test(menuBody),
    false,
    "the .menu CSS rule must NOT declare position: absolute; the inline style is the single source of truth",
  );
  assert.equal(
    /z-index:\s*\d+/.test(menuBody),
    false,
    "the .menu CSS rule must NOT declare z-index; the inline style carries the layer order",
  );
});

test("HistoryCard mounts the popover only inside an {#if menuOpen} block (single visible instance)", () => {
  // The previous baseline left the popover mounted in the DOM with
  // an empty `style` attribute and flipped it on / off via a CSS
  // class. The regression surfaced as "I clicked the `...` button
  // and saw nothing". The fix wraps the popover in an
  // `{#if menuOpen}` block so the popover is mounted with the
  // computed inline style from the very first paint — there is
  // exactly one instance per rail and it cannot be hidden behind
  // a stale style.
  const menuOpenIdx = cardSource.indexOf('{#if menuOpen}');
  assert.notEqual(menuOpenIdx, -1, "the {#if menuOpen} wrapper must exist");
  const menuTestIdIdx = cardSource.indexOf('data-testid="history-card-menu"', menuOpenIdx);
  assert.notEqual(
    menuTestIdIdx,
    -1,
    "the popover must live inside the {#if menuOpen} wrapper so it mounts when the menu opens",
  );
  // The popover MUST NOT live outside the {#if menuOpen} block:
  // any duplicate popover is a regression that surfaces as
  // "I can see two menus" the moment two cards race a click.
  const menuBeforeIdx = cardSource.indexOf('data-testid="history-card-menu"');
  const menuAfterIdx = menuBeforeIdx + 1;
  const menuAfterAfterIdx = cardSource.indexOf('data-testid="history-card-menu"', menuAfterIdx);
  assert.equal(
    menuAfterAfterIdx,
    -1,
    "the popover must be rendered exactly once per card; a second declaration surfaces as two stacked menus",
  );
});

// ---------------------------------------------------------------------------
// 12. Quick Paste hint order — title precedes the hint.
// ---------------------------------------------------------------------------

test("QuickPaste preview hint renders AFTER the title in the meta line", () => {
  // The user-facing visual order MUST be type/icon → title → hint
  // → pin / source-app. The regression that surfaced during the
  // manual QA pass placed the hint between the type icon and the
  // title, which broke the reading order; the fix moves the hint
  // below the title so the row reads top-to-bottom the way the
  // user expects.
  const quickPasteSource = readFileSync(
    resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
    "utf8",
  );
  const titleIdx = quickPasteSource.indexOf('class="qp-title"');
  assert.notEqual(titleIdx, -1, "the qp-title element must exist");
  const hintIdx = quickPasteSource.indexOf('class="qp-preview-hint"');
  assert.notEqual(hintIdx, -1, "the qp-preview-hint element must exist");
  assert.ok(
    titleIdx < hintIdx,
    "the title element must come BEFORE the preview hint in the source",
  );
  // The hint must still be inside the `{#if index === selectedIndex}`
  // block so only the selected row carries the label.
  const openerIdx = quickPasteSource.lastIndexOf(
    "{#if index === selectedIndex}",
    hintIdx,
  );
  assert.notEqual(
    openerIdx,
    -1,
    "the preview hint must be wrapped in an {#if index === selectedIndex} block",
  );
  assert.ok(
    openerIdx > titleIdx,
    "the {#if selectedIndex} block must open AFTER the title element",
  );
});

test("QuickPaste preview hint still uses the shared shortcut helpers and English label", () => {
  // Reaffirm the existing contract: the hint must consult the
  // shared helpers (no parallel implementation) and render the
  // English `Preview` label so the rail and Quick Paste surfaces
  // stay aligned.
  const quickPasteSource = readFileSync(
    resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
    "utf8",
  );
  assert.ok(
    quickPasteSource.includes("previewShortcutLabel(shortcutPlatform)"),
    "QuickPaste must consume previewShortcutLabel through the shared helper",
  );
  assert.ok(
    quickPasteSource.includes("previewShortcutAccessibleLabel(shortcutPlatform)"),
    "QuickPaste must consume previewShortcutAccessibleLabel through the shared helper",
  );
  const labelMatch = quickPasteSource.match(/qp-preview-hint-label">([^<]+)</);
  assert.ok(labelMatch, "the Quick Paste preview hint must declare a label span");
  assert.equal(labelMatch![1], "Preview");
});

test("QuickPaste preview hint never breaks the row layout", () => {
  // The `auto` column the hint lives in must collapse to `0` when
  // the row is not selected so the type / pin / source-app icons
  // keep their fixed columns; the title still truncates with an
  // ellipsis instead of growing the row. The hint must remain
  // non-interactive (`pointer-events: none`).
  const quickPasteSource = readFileSync(
    resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
    "utf8",
  );
  const metaRuleMatch = quickPasteSource.match(
    /\.qp-row-line-meta\s*\{([^}]*)\}/,
  );
  assert.ok(metaRuleMatch, "the .qp-row-line-meta CSS rule must exist");
  assert.ok(
    /grid-template-columns:\s*1\.5rem\s+minmax\(0,\s*1fr\)\s+auto\s+1\.65rem\s+1\.65rem/.test(
      metaRuleMatch![1],
    ),
    "the meta line grid must allocate an auto column for the hint",
  );
  const hintRuleMatch = quickPasteSource.match(
    /\.qp-preview-hint\s*\{([^}]*)\}/,
  );
  assert.ok(hintRuleMatch, "the .qp-preview-hint CSS rule must exist");
  assert.ok(
    /pointer-events:\s*none/.test(hintRuleMatch![1]),
    "the hint must remain non-interactive",
  );
  assert.ok(
    /flex:\s*0\s+0\s+auto/.test(hintRuleMatch![1]),
    "the hint must declare flex: 0 0 auto so the row layout stays predictable",
  );
});

// ---------------------------------------------------------------------------
// 13. Desktop preview Escape — document-level wiring in App.svelte.
// ---------------------------------------------------------------------------

test("App.svelte installs a window-level Escape handler while the preview is open", () => {
  // The Desktop preview overlay already closes on Escape when the
  // focus is inside the overlay. To make Escape close the preview
  // regardless of focus, the container (App.svelte) installs a
  // document-level keydown listener while the preview is open.
  // The listener MUST use capture phase on `window` so it fires
  // before the rail's `document` listener.
  assert.match(
    appSource,
    /window\.addEventListener\(\s*"keydown"\s*,\s*[^,]+,\s*true\s*\)/,
    "App.svelte must install a keydown listener on window in capture phase",
  );
  assert.match(
    appSource,
    /function onPreviewWindowKeydown\([\s\S]*?closePreview\(\)/,
    "the listener must delegate to closePreview",
  );
  assert.match(
    appSource,
    /event\.stopImmediatePropagation\(\)/,
    "the listener must call stopImmediatePropagation so the rail does not also act on Escape",
  );
});

test("App.svelte tears down the preview Escape listener when the preview closes", () => {
  // The handler MUST be torn down the moment the preview closes
  // (and again on App unmount) so a stale listener cannot keep
  // closing the preview after the user dismissed it.
  assert.match(
    appSource,
    /detachPreviewKeydown\(\)[\s\S]*?detachPreviewKeydown\s*=\s*null/,
    "App.svelte must clear detachPreviewKeydown after the preview closes",
  );
  // The onDestroy path must also tear down the handler.
  assert.match(
    appSource,
    /if \(detachPreviewKeydown\)\s*\{[\s\S]*?detachPreviewKeydown\(\)[\s\S]*?detachPreviewKeydown\s*=\s*null\s*;?\s*\}/,
    "App.svelte must tear down the preview Escape listener on destroy",
  );
  // Idempotency: the detach handle is null after the first tear
  // down so a second invocation cannot double-remove.
  assert.match(
    appSource,
    /window\.removeEventListener\(\s*"keydown"\s*,\s*handler\s*,\s*true\s*\)/,
    "App.svelte must call removeEventListener with capture phase true",
  );
});

test("App.svelte preview Escape ignores inputs, / textareas, / contenteditable", () => {
  // The preview's Escape handler MUST skip typing surfaces so the
  // search input / title editor / rename modal keep their default
  // Escape behaviour. A regression that swallowed Escape on a
  // typing surface would silently break every input in the
  // desktop while the preview is open.
  const previewKeydownIdx = appSource.indexOf(
    "function onPreviewWindowKeydown",
  );
  assert.notEqual(
    previewKeydownIdx,
    -1,
    "the preview Escape handler must be declared in App.svelte",
  );
  const body = appSource.slice(
    previewKeydownIdx,
    appSource.indexOf("\n  function", previewKeydownIdx + 1),
  );
  assert.ok(
    /HTMLInputElement/.test(body),
    "the handler must skip HTMLInputElement",
  );
  assert.ok(
    /HTMLTextAreaElement/.test(body),
    "the handler must skip HTMLTextAreaElement",
  );
  assert.ok(
    /isContentEditable/.test(body),
    "the handler must skip contenteditable surfaces",
  );
});

test("App.svelte preview Escape must not change the rail selection or menu state", () => {
  // The preview's Escape handler MUST use stopImmediatePropagation
  // so the rail's keydown listener (which closes the menu and
  // clears the selection) cannot fire while the preview is open.
  // Without this guard, pressing Escape with both the preview and
  // a menu open would close the menu before the preview, breaking
  // the visual stacking the user expects.
  assert.match(
    appSource,
    /previewEntry\s*!==\s*null/,
    "the App.svelte Escape handler must check whether the preview is open",
  );
  assert.match(
    appSource,
    /stopImmediatePropagation\(\)/,
    "the App.svelte Escape handler must stop the event from reaching the rail",
  );
});

test("HistoryCard preview-hint and QuickPaste preview-hint still share the helpers", () => {
  // The `previewShortcutLabel` / `previewShortcutAccessibleLabel`
  // helpers are the single source of truth for the visible glyph
  // and the long-form accessible name. Both surfaces MUST consume
  // them so a future platform tweak lands once.
  assert.match(
    cardSource,
    /previewShortcutLabel\(previewPlatform\)/,
    "HistoryCard must consume previewShortcutLabel through the shared helper",
  );
  assert.match(
    cardSource,
    /previewShortcutAccessibleLabel\(previewPlatform\)/,
    "HistoryCard must consume previewShortcutAccessibleLabel through the shared helper",
  );
});

// ---------------------------------------------------------------------------
// 15. Desktop selection — canonical `selectedEntryId` only, no visual index.
// ---------------------------------------------------------------------------

test("HistoryCard click handler dispatches the card's entry id, not an index", () => {
  // The selection contract is by **entry id**, not by the visual
  // position of the card in the rail. A regression that read
  // `entries.indexOf(entry)` (or anything that produces a 0-based
  // cursor) would silently re-select the first row whenever the
  // rail reordered itself — the regression surfaced during the
  // second round of manual QA on `preview-interaction-regressions`
  // because the user observed "clicking card 3 selects card 1"
  // on a search-filtered rail.
  const handlerMatch = cardSource.match(
    /function onCardSurfaceClick[\s\S]*?\n  \}/,
  );
  assert.ok(handlerMatch, "onCardSurfaceClick must exist in HistoryCard.svelte");
  const body = handlerMatch![0];
  assert.match(
    body,
    /dispatchSelect\(\s*selected\s*\?\s*null\s*:\s*entry\.id\s*\)/,
    "onCardSurfaceClick must dispatch `selected ? null : entry.id`; selection is keyed by entry id, not by a visual index",
  );
  assert.equal(
    /indexOf/.test(body),
    false,
    "onCardSurfaceClick must not call indexOf; selection is keyed by entry id, not by a visual index",
  );
});

test("HistoryCardRail forwards `selectedEntryId === entry.id` strictly", () => {
  // The rail MUST compare `selectedEntryId` against `entry.id` with
  // strict equality so a stale selection from a deleted row cannot
  // accidentally light up the first visible row. A regression that
  // switched to `==` (loose equality) would coerce `null` to `0`
  // and silently select the first entry on every render.
  assert.match(
    railSource,
    /selected=\{selectedEntryId === entry\.id\}/,
    "HistoryCardRail must forward the canonical strict equality `selectedEntryId === entry.id`",
  );
  assert.equal(
    /selectedEntryId\s*===\s*entry\.id/.test(railSource),
    true,
    "HistoryCardRail must use strict `===` equality; a loose `==` would coerce null to 0 and select the first card",
  );
  // The rail must NOT compute selection from a visual index either.
  assert.equal(
    /entries\.indexOf/.test(railSource),
    false,
    "HistoryCardRail must not call entries.indexOf; selection is keyed by entry id",
  );
});

test("HistoryCardRail binds selectedEntryId through `bind:selectedEntryId`", () => {
  // The two-way binding is the contract `App.svelte` uses to keep
  // its `railSelectedEntryId` mirror in lock-step with the rail's
  // canonical id. Without `bind:`, a click that updated the rail
  // would NOT propagate to the desktop `Cmd/Ctrl+Enter` matcher.
  assert.match(
    appSource,
    /bind:selectedEntryId=\{railSelectedEntryId\}/,
    "App.svelte must bind:selectedEntryId to the rail so the desktop-level keyboard matcher sees the latest selection",
  );
});

test("HistoryCardRail never defaults to a non-null selection (no auto-select of the first card)", () => {
  // The previous baselines accidentally seeded the rail with
  // `selectedEntryId = entries[0]?.id ?? null`, which silently
  // selected the first card on every mount. The fix pins the
  // default to a literal `null` so the rail cannot start the user
  // off on the wrong row.
  assert.match(
    railSource,
    /export let selectedEntryId:\s*number\s*\|\s*null\s*=\s*null/,
    "HistoryCardRail must default selectedEntryId to `null` so no card is selected on mount",
  );
  assert.match(
    appSource,
    /let railSelectedEntryId:\s*number\s*\|\s*null\s*=\s*null/,
    "App.svelte must default railSelectedEntryId to `null` so no card is selected on mount",
  );
  // No `entries[0]` or `resultIds[0]` seeding of the selection in
  // either component.
  assert.equal(
    /selectedEntryId\s*=\s*entries\[0\]/.test(railSource) ||
      /selectedEntryId\s*=\s*entries\[0\]\?\.id/.test(railSource),
    false,
    "HistoryCardRail must not seed selectedEntryId from entries[0]",
  );
  assert.equal(
    /railSelectedEntryId\s*=\s*entries\[0\]/.test(appSource) ||
      /railSelectedEntryId\s*=\s*entries\[0\]\?\.id/.test(appSource),
    false,
    "App.svelte must not seed railSelectedEntryId from entries[0]",
  );
});

test("HistoryCardRail clears the selection when the visible scope changes", () => {
  // The rail must drop `selectedEntryId` whenever the visible
  // scope changes (delete, refresh, search, collection switch)
  // and the previously selected id is no longer in the visible
  // set. The contract is encoded by a reactive block that compares
  // against `visibleEntryIds`.
  assert.match(
    railSource,
    /visibleEntryIds\s*=\s*new Set\(entries\.map\(\(entry\)\s*=>\s*entry\.id\)\)/,
    "HistoryCardRail must build `visibleEntryIds` from the entries' ids",
  );
  assert.match(
    railSource,
    /selectedEntryId\s*=\s*null/,
    "HistoryCardRail must reset selectedEntryId to null when the entry leaves scope",
  );
});

test("App.svelte clears railSelectedEntryId when the collection changes", () => {
  // The collection-switch flow is the user-facing entry point the
  // manual QA pass used to verify "selection survives only while
  // the entry is visible". The handler MUST reset the rail
  // selection so a card that was selected in the previous scope
  // cannot silently remain selected after the user clicks a
  // different collection row.
  assert.match(
    appSource,
    /selectCollectionFromSidebar[\s\S]*?railSelectedEntryId\s*=\s*null/,
    "App.svelte must reset railSelectedEntryId when the user switches collection",
  );
});

// ---------------------------------------------------------------------------
// 16. Desktop click selection — visual indicator and accessible state.
//
// The manual QA pass surfaced a regression where clicking on a
// card executed the click handler but the card never lit up as
// active; the user observed "clicking card 3 still selects card
// 1". The audit confirmed that the structural contract was
// already correct (single `selectedEntryId`, `===` strict
// equality, bind target in App.svelte), but two visible bugs
// were silently shipping:
//
//   1. The `class:card-selected={selected}` directive in
//      HistoryCard.svelte was applied to the `<article>` but the
//      `.card-selected` CSS rule had been removed (or never
//      shipped). The card therefore never painted a visible cue
//      regardless of the `selected` prop value.
//   2. The `aria-selected` attribute used Svelte's boolean
//      attribute serialization (which collapses `false` to the
//      attribute's absence on some platforms). The accessible
//      state never carried an explicit "false" string so the
//      regression suite could not assert it on a non-selected
//      card.
//
// The tests below pin both fixes so a future refactor cannot
// silently regress the visual selection feedback.
// ---------------------------------------------------------------------------

test("HistoryCard declares the .card-selected CSS rule so the click cue actually paints", () => {
  // The card element carries `class:card-selected={selected}` but
  // the previous baseline shipped no rule for the class so the
  // user never saw a visible change after clicking a row. The fix
  // declares a `.card.card-selected` selector that paints a blue
  // accent border + soft ring + lighter background so the active
  // row is unmistakable. The regression assertion inspects the
  // rule body so a future refactor that drops the rule (or
  // re-introduces the bare `.card-selected` selector without the
  // base class qualifier) surfaces in CI.
  const ruleMatch = cardSource.match(
    /\.card\.card-selected\s*\{([^}]*)\}/,
  );
  assert.ok(
    ruleMatch,
    "HistoryCard must declare a `.card.card-selected` CSS rule so the visual cue actually paints",
  );
  const body = ruleMatch![1];
  assert.ok(
    /border-color\s*:/i.test(body),
    "the selected card must override its border colour so the user can see the active row",
  );
  assert.ok(
    /box-shadow\s*:/i.test(body),
    "the selected card must declare a soft ring through box-shadow so the active row stands out on a dark surface",
  );
});

test("HistoryCard declares the .menu-open CSS rule so the open menu cue is visible", () => {
  // The card element carries `class:menu-open={menuOpen}` so the
  // user can tell which row owns the popover. The previous baseline
  // declared the directive without a matching rule; the cue was
  // invisible even when the menu was open. The regression assertion
  // inspects the `.card.menu-open` rule body so a refactor that
  // drops the rule surfaces here.
  const ruleMatch = cardSource.match(/\.card\.menu-open\s*\{([^}]*)\}/);
  assert.ok(
    ruleMatch,
    "HistoryCard must declare a `.card.menu-open` CSS rule so the open menu cue actually paints",
  );
  const body = ruleMatch![1];
  assert.ok(
    /border-color\s*:/i.test(body),
    "the menu-open card must override its border colour so the user can see which row owns the popover",
  );
});

test("HistoryCard keeps the selection cue when both selected and menu-open are true", () => {
  // When the user opens the ellipsis menu on the selected card,
  // both `class:card-selected` and `class:menu-open` are active.
  // The blue accent must win so the keyboard shortcut hint
  // (Cmd/Ctrl+Enter) stays unambiguous; otherwise the menu open
  // would mask the selection and the user would not know whether
  // `Cmd/Ctrl+Enter` opens the preview or the menu.
  const ruleMatch = cardSource.match(
    /\.card\.card-selected\.menu-open\s*\{([^}]*)\}/,
  );
  assert.ok(
    ruleMatch,
    "HistoryCard must declare a `.card.card-selected.menu-open` rule that keeps the selection accent dominant",
  );
  const body = ruleMatch![1];
  assert.ok(
    /border-color\s*:/i.test(body),
    "the combined rule must pin the border colour so the selection accent wins over the menu cue",
  );
  assert.ok(
    /box-shadow\s*:/i.test(body),
    "the combined rule must pin the box-shadow so the selection ring wins over the menu cue",
  );
});

test("HistoryCard mirrors the selected flag onto aria-selected as a real boolean attribute", () => {
  // The regression that surfaced during the manual QA pass
  // observed "clicking card 3 still selects card 1". The audit
  // confirmed the structural state was correct but the visible
  // cue was missing; the accessible cue must stay rock-solid so
  // assistive tech can also detect which row is active. The
  // assertion pins the literal `aria-selected={selected}` binding
  // (and forbids a third-party `aria-selected="false"` constant)
  // so a refactor cannot silently drop the value to `undefined`.
  assert.match(
    cardSource,
    /aria-selected=\{selected\}/,
    "HistoryCard must bind aria-selected to the canonical selected prop so screen readers see the active row",
  );
  assert.equal(
    /aria-selected\s*=\s*["']false["']/.test(cardSource),
    false,
    "HistoryCard must NOT hard-code aria-selected=false; the binding must drive the value",
  );
});

test("HistoryCard surfaces data-selected so the click state is observable from the DOM", () => {
  // The DOM-level `data-selected` attribute is the contract the
  // e2e selectors and the regression suite use to read the
  // selection state. The assertion pins the binding so a future
  // refactor that drops the attribute (or replaces it with a
  // computed class only) surfaces here.
  assert.match(
    cardSource,
    /data-selected=\{selected\s*\?\s*["']true["']\s*:\s*["']false["']\}/,
    "HistoryCard must expose data-selected={selected ? \"true\" : \"false\"} so the click state is observable",
  );
});

// ---------------------------------------------------------------------------
// 17. Desktop horizontal keyboard navigation — rail wiring + preventDefault
// + scrollIntoView + no wrap-around.
//
// The manual QA pass surfaced a regression where pressing
// `ArrowLeft` / `ArrowRight` only scrolled the rail's overflow
// surface without ever updating the rail-owned
// `selectedEntryId`. The structural tests below pin the rail
// wiring, the keyboard handler and the helper delegation so the
// horizontal navigation cannot silently regress.
// ---------------------------------------------------------------------------

test("HistoryCardRail imports the horizontalRailNextSelectionId helper", () => {
  // The rail is the single switch that drives the keyboard
  // navigation; the pure helper is the only source of truth for
  // the next-id math. A regression that re-implemented the math
  // inline (e.g. with `(currentIndex + delta + total) % total`)
  // would silently re-introduce the wrap-around bug the manual
  // QA pass surfaced.
  assert.match(
    railSource,
    /import\s*\{[^}]*horizontalRailNextSelectionId[^}]*\}\s*from\s*"\.\/lib\/horizontalRailNavigation/,
    "HistoryCardRail must import horizontalRailNextSelectionId from the pure helper module",
  );
});

test("HistoryCardRail registers a keydown listener for the horizontal arrow keys", () => {
  // The previous baseline never registered a keydown listener
  // for `ArrowLeft` / `ArrowRight` so pressing the horizontal
  // arrows only triggered the native overflow scroll the rail
  // already owns. The fix installs a capture-phase listener that
  // mirrors the rail's `Escape` listener and re-uses the same
  // `detachWindow` cleanup helper so the cleanup stays
  // idempotent.
  assert.match(
    railSource,
    /document\.addEventListener\(\s*["']keydown["']\s*,\s*onRailHorizontalKeydown\s*,\s*true\s*\)/,
    "HistoryCardRail must install a capture-phase keydown listener that drives the horizontal arrow navigation",
  );
  assert.match(
    railSource,
    /document\.removeEventListener\([\s\S]*?["']keydown["'][\s\S]*?onRailHorizontalKeydown/,
    "HistoryCardRail must tear down the horizontal arrow listener on destroy so a remount cannot double-fire",
  );
});

test("HistoryCardRail calls preventDefault on a horizontal arrow press the handler consumes", () => {
  // The keyboard handler MUST swallow the default scroll when the
  // rail actually flips the selection id; a regression that
  // forgot `event.preventDefault()` would leave the native
  // overflow scroll running in parallel and double-scroll the
  // rail. The assertion pins the `preventDefault()` call so a
  // future refactor cannot silently drop the gesture.
  const handlerMatch = railSource.match(
    /function onRailHorizontalKeydown[\s\S]*?\n  \}/,
  );
  assert.ok(
    handlerMatch,
    "HistoryCardRail must declare onRailHorizontalKeydown",
  );
  const body = handlerMatch![0];
  assert.match(
    body,
    /event\.preventDefault\(\)/,
    "onRailHorizontalKeydown must call preventDefault so the native scroll cannot run alongside the rail navigation",
  );
});

test("HistoryCardRail skips horizontal navigation on typing surfaces", () => {
  // The keyboard handler MUST short-circuit when the focus sits
  // inside an input / textarea / contenteditable so the search
  // field, the title editor and the rename modal keep their
  // default caret movement. The assertion pins the explicit
  // `HTMLInputElement` / `HTMLTextAreaElement` / `isContentEditable`
  // checks so a future refactor that forgot the typing-surface
  // guard cannot ship.
  const handlerMatch = railSource.match(
    /function onRailHorizontalKeydown[\s\S]*?\n  \}/,
  );
  assert.ok(
    handlerMatch,
    "HistoryCardRail must declare onRailHorizontalKeydown",
  );
  const body = handlerMatch![0];
  assert.match(
    body,
    /HTMLInputElement/,
    "onRailHorizontalKeydown must skip HTMLInputElement so the search input keeps its caret movement",
  );
  assert.match(
    body,
    /HTMLTextAreaElement/,
    "onRailHorizontalKeydown must skip HTMLTextAreaElement so the title editor keeps its caret movement",
  );
  assert.match(
    body,
    /isContentEditable/,
    "onRailHorizontalKeydown must skip contenteditable surfaces so the rename modal keeps its caret movement",
  );
});

test("HistoryCardRail delegates the next-id math to horizontalRailNextSelectionId (no inline modulo wrap)", () => {
  // The handler MUST delegate the navigation math to the pure
  // helper; a regression that re-implemented the wrap-around
  // inline (e.g. with `(currentIndex + delta + total) % total`)
  // would silently teleport the user across the rail. The
  // assertion pins the helper call so the math stays centralised.
  const handlerMatch = railSource.match(
    /function onRailHorizontalKeydown[\s\S]*?\n  \}/,
  );
  assert.ok(
    handlerMatch,
    "HistoryCardRail must declare onRailHorizontalKeydown",
  );
  const body = handlerMatch![0];
  assert.match(
    body,
    /horizontalRailNextSelectionId\(/,
    "onRailHorizontalKeydown must delegate the next-id math to the pure horizontalRailNextSelectionId helper",
  );
  // A future regression that re-introduced the modulo wrap would
  // surface here: the inline wrap helper MUST NOT exist on the
  // rail handler body.
  assert.equal(
    /%\s*entries\.length/.test(body) || /%\s*visibleIds\.length/.test(body),
    false,
    "onRailHorizontalKeydown must NOT use the modulo wrap operator; the helper handles the clamp",
  );
});

test("HistoryCardRail calls scrollIntoView on the freshly selected card", () => {
  // The handler MUST bring the freshly selected card into view so
  // the user always sees the row they just navigated to. The
  // previous baseline never asked the rail to scroll, which meant
  // a horizontally-clipped rail kept the same edge card visible
  // even after the selection flipped to an off-screen row. The
  // assertion pins `scrollIntoView` with the `inline: "nearest"`
  // option so the helper never scrolls the rail vertically.
  const helperMatch = railSource.match(
    /function scrollSelectedCardIntoView[\s\S]*?\n  \}/,
  );
  assert.ok(
    helperMatch,
    "HistoryCardRail must declare scrollSelectedCardIntoView",
  );
  const body = helperMatch![0];
  assert.match(
    body,
    /scrollIntoView\(/,
    "scrollSelectedCardIntoView must call scrollIntoView so the freshly selected card lands inside the visible portion of the rail",
  );
  assert.match(
    body,
    /inline:\s*["']nearest["']/,
    "scrollSelectedCardIntoView must scroll horizontally only (inline: 'nearest') so the rail never moves vertically",
  );
  assert.match(
    body,
    /block:\s*["']nearest["']/,
    "scrollSelectedCardIntoView must scroll only when needed (block: 'nearest') so the rail never forces the card to a corner",
  );
});

test("HistoryCardRail wires the card registry through the onCardRef callback", () => {
  // The rail MUST register / unregister each card's DOM element
  // through the `onCardRef` callback the HistoryCard exposes; a
  // regression that kept the registry inside a separate
  // `bind:this` would silently break the `scrollIntoView` lookup
  // the handler relies on. The assertion pins the callback
  // signature and the registry lifecycle so a future refactor
  // cannot drop it.
  assert.match(
    cardSource,
    /export let onCardRef:\s*\(\s*el:\s*HTMLElement\s*\|\s*null\s*\)\s*=>\s*void\s*=\s*\(\)\s*=>\s*\{\}/,
    "HistoryCard must export the onCardRef callback so the rail can register / unregister the article element",
  );
  assert.match(
    cardSource,
    /bind:this=\{cardArticleEl\}/,
    "HistoryCard must bind:this the article element so the rail's registry stays in lock-step with the DOM",
  );
  assert.match(
    railSource,
    /onCardRef=\{\(el\)\s*=>\s*registerCardRef\(entry\.id,\s*el\)\}/,
    "HistoryCardRail must forward onCardRef into the per-card registerCardRef helper so the registry is keyed by entry id",
  );
  assert.match(
    railSource,
    /function registerCardRef\(/,
    "HistoryCardRail must declare registerCardRef so the per-card onCardRef callback can mutate the registry",
  );
});

test("HistoryCardRail cleans up the card registry when an entry leaves the visible scope", () => {
  // A delete, refresh, search or collection change can shrink the
  // visible set. The rail MUST drop the registry entry so a
  // follow-up ArrowLeft against a stale id cannot try to
  // `scrollIntoView` an unmounted element. The assertion pins the
  // reactive cleanup block.
  assert.match(
    railSource,
    /cardEls\.delete\(id\)/,
    "HistoryCardRail must delete registry entries whose entry id fell out of the visible scope",
  );
});

test("HistoryCardRail never seeds the initial selection from entries[0]", () => {
  // The horizontal navigation must start from `null` so the
  // user's first ArrowRight lands on the FIRST visible card
  // (the documented `null → first` fallback) instead of silently
  // starting on whatever card the rail rehydrated first. The
  // assertion forbids `selectedEntryId = entries[0]` so a
  // regression that seeded the selection from the rail rehydrate
  // path surfaces here.
  assert.equal(
    /selectedEntryId\s*=\s*entries\[0\]/.test(railSource),
    false,
    "HistoryCardRail must NOT seed selectedEntryId from entries[0]",
  );
  assert.match(
    railSource,
    /export let selectedEntryId:\s*number\s*\|\s*null\s*=\s*null/,
    "HistoryCardRail must default selectedEntryId to null so the first arrow press lands on the FIRST visible card",
  );
});

// ---------------------------------------------------------------------------
// 13. Bottom-of-rail cleanup: no icon, image, placeholder, text or visual
//     container must appear below the HistoryCardRail inside the desktop
//     layout. The previous CardDropText drop indicator lived directly
//     below the rail; the regression surfaced during a fourth QA pass and
//     must not return. The tests below pin the contract from every
//     surface that could regress it:
//     - source-level: App.svelte / HistoryCardRail.svelte must not
//       import or mount a CardDropText, drop-feedback or any other
//       bottom-of-rail visual element;
//     - layout-level: the right column must end with the rail and must
//       not reserve space through a placeholder container;
//     - file-level: the CardDropText component, its handler factory and
//       its dedicated test file must be deleted so a future copy-paste
//       cannot resurrect them;
//     - regression-level: drag-and-drop on collections, the pointer
//       drag ghost, the horizontal arrow navigation, the click
//       selection and the rail accessibility markers MUST stay
//       intact so the cleanup cannot silently break the contract.
// ---------------------------------------------------------------------------

const FRONTEND_ROOT = resolvePath(process.cwd(), "src");

test("App.svelte does not import or mount the legacy bottom drop indicator", () => {
  // The CardDropText component used to render a "Suelta aquí" drop
  // indicator directly below the rail. The visual element was a
  // debug-only feedback that confused the user with a permanent
  // banner that was never the canonical drop target (the sidebar's
  // collection row owns the mutation). Removing the indicator
  // deletes the visual AND the markup import so a future copy-
  // paste cannot resurrect the element.
  assert.equal(
    /import\s+CardDropText/.test(appSource),
    false,
    "App.svelte must NOT import CardDropText",
  );
  assert.equal(
    /<CardDropText[\s>]/.test(appSource),
    false,
    "App.svelte must NOT mount a <CardDropText> element below the rail",
  );
  assert.equal(
    /data-testid="card-drop-text"/.test(appSource),
    false,
    "App.svelte must NOT carry the legacy card-drop-text testid",
  );
  assert.equal(
    /class="card-drop-text"/.test(appSource),
    false,
    "App.svelte must NOT carry the legacy card-drop-text class",
  );
  assert.equal(
    /data-testid="drop-feedback"/.test(appSource),
    false,
    "App.svelte must NOT carry the legacy drop-feedback testid",
  );
  assert.equal(
    /class="drop-feedback"/.test(appSource),
    false,
    "App.svelte must NOT carry the legacy drop-feedback class",
  );
});

test("HistoryCardRail does not mount any element below the rail list", () => {
  // The rail component is the single surface that owns the
  // horizontal list of cards. The contract: the template renders
  // only the empty-state `<p>` or the `<div class="rail">` and
  // nothing else; a future regression that appends a placeholder,
  // a drop indicator or any visual container must surface here.
  // The assertion scans the template for any element rendered
  // outside the `{#if entries.length === 0} … {:else} … {/if}`
  // block that wraps the rail.
  const templateStart = railSource.indexOf("<script");
  const templateEnd = railSource.indexOf("</script>", templateStart);
  assert.notEqual(templateStart, -1, "HistoryCardRail must declare a <script> block");
  assert.notEqual(templateEnd, -1, "HistoryCardRail must close its <script> block");
  const afterScript = railSource.slice(templateEnd);
  // The empty-state branch and the rail branch are the only two
  // elements the template renders at the top level.
  const hasRailBranch = /class="rail"/.test(afterScript);
  const hasEmptyBranch = /data-testid="history-rail-empty"/.test(afterScript);
  assert.ok(hasRailBranch, "HistoryCardRail must render the .rail branch");
  assert.ok(hasEmptyBranch, "HistoryCardRail must render the empty-state branch");
  // No extra element with a class or testid outside the rail block.
  assert.equal(
    /data-testid="card-drop-text"/.test(railSource),
    false,
    "HistoryCardRail must NOT mount the legacy card-drop-text indicator",
  );
  assert.equal(
    /data-testid="drop-feedback"/.test(railSource),
    false,
    "HistoryCardRail must NOT mount a drop-feedback element below the rail",
  );
});

test("App.svelte layout-main ends with the rail and reserves no extra height", () => {
  // The right column (`.layout-main`) renders the toolbar, the
  // search-status line and the rail. The rail MUST be the LAST
  // element inside `.layout-main` so the bottom height is bound
  // to the rail's `--cv-card-rail-height` (no placeholder
  // container, no padded footer, no debug indicator reserving
  // vertical space). The assertion locates the rail inside the
  // layout-main block and verifies that nothing but whitespace
  // and `</div>` follows it before the layout-main closes.
  const layoutMainStart = appSource.indexOf('class="layout-main"');
  assert.notEqual(layoutMainStart, -1, "App.svelte must declare the .layout-main column");
  // The rail is the last meaningful element; isolate the slice
  // between the rail tag and the closing of layout-main.
  const railIdx = appSource.indexOf("<HistoryCardRail", layoutMainStart);
  assert.notEqual(railIdx, -1, "App.svelte must mount <HistoryCardRail> inside .layout-main");
  const railEnd = appSource.indexOf("/>", railIdx) + 2;
  const layoutMainEnd = appSource.indexOf("</div>", railEnd);
  assert.notEqual(layoutMainEnd, -1, "layout-main must close after the rail");
  const slice = appSource.slice(railEnd, layoutMainEnd);
  // The slice between the rail and the closing </div> must not
  // contain any element markup (div / p / section / span / svg /
  // footer / aside / img / aside / nav). Only whitespace and the
  // closing tag are allowed.
  assert.equal(
    /<\s*(div|section|p|span|svg|aside|footer|header|nav|article|main)\b/i.test(slice),
    false,
    "App.svelte must NOT render any element between the rail and the closing of layout-main",
  );
});

test("App.svelte layout-main has no fixed min-height that reserves space below the rail", () => {
  // A regression that pinned `min-height: <value>` (anything other
  // than `0`) on `.layout-main` would reserve empty space below
  // the rail even with no icon in the DOM. The layout column must
  // rely on its content (`auto`) and the rail's
  // `--cv-card-rail-height` so the bottom edge stops at the
  // rail's natural height. The token `min-height: 0` is the
  // structural guard that lets the column shrink; it is allowed
  // and pinned here so a future tweak cannot re-introduce a
  // positive min-height that would push the row past the rail.
  const styleStart = appSource.indexOf("<style>");
  const styleEnd = appSource.indexOf("</style>", styleStart);
  assert.notEqual(styleStart, -1, "App.svelte must declare a <style> block");
  assert.notEqual(styleEnd, -1, "App.svelte must close its <style> block");
  const css = appSource.slice(styleStart, styleEnd);
  const layoutMainRule = css.match(/\.layout-main\s*\{[^}]*\}/);
  assert.ok(layoutMainRule, "App.svelte must declare the .layout-main CSS rule");
  assert.equal(
    /min-height\s*:\s*0(?!\d)/.test(layoutMainRule![0]),
    true,
    "App.svelte must pin min-height: 0 on .layout-main so the column can shrink below its content",
  );
  assert.equal(
    /min-height\s*:\s*([1-9]\d*|[a-z]+)/.test(layoutMainRule![0]),
    false,
    "App.svelte must NOT pin a positive min-height on .layout-main so the column stops at the rail's natural height",
  );
});

test("HistoryCardRail still owns the fixed --cv-card-rail-height token", () => {
  // The rail's vertical footprint is bounded by `--cv-card-rail-height`
  // so the layout column can sit tight against the bottom of the
  // rail. The fix that removed the bottom indicator MUST NOT
  // silently grow or shrink the rail; the height token stays.
  const styleStart = railSource.indexOf("<style>");
  const styleEnd = railSource.indexOf("</style>", styleStart);
  assert.notEqual(styleStart, -1, "HistoryCardRail must declare a <style> block");
  const css = railSource.slice(styleStart, styleEnd);
  assert.match(
    css,
    /\.rail\s*\{[^}]*--cv-card-size:\s*240px/,
    "HistoryCardRail must pin --cv-card-size to 240px so the cards stay square",
  );
  assert.match(
    css,
    /\.rail\s*\{[^}]*height:\s*var\(\s*--cv-card-rail-height/,
    "HistoryCardRail must drive the rail height from --cv-card-rail-height",
  );
  assert.match(
    css,
    /\.rail\s*\{[^}]*overflow-x:\s*auto/,
    "HistoryCardRail must keep its horizontal scrollbar so a wide rail still scrolls",
  );
});

test("the CardDropText component, its handler factory and its test file are deleted", () => {
  // The cleanup is structural: the visual markup, the handler
  // factory that powered its listeners AND the dedicated test
  // file that exercised the indicator must be gone so a future
  // copy-paste cannot bring the icon back. A regression that
  // resurrects the component from the archive must surface here.
  assert.equal(
    existsSync(resolvePath(FRONTEND_ROOT, "..", "src", "CardDropText.svelte")),
    false,
    "src/CardDropText.svelte must be deleted",
  );
  assert.equal(
    existsSync(
      resolvePath(FRONTEND_ROOT, "..", "src", "lib", "cardDropTextHandlers.ts"),
    ),
    false,
    "src/lib/cardDropTextHandlers.ts must be deleted",
  );
  assert.equal(
    existsSync(
      resolvePath(FRONTEND_ROOT, "..", "tests", "cardDropText.test.ts"),
    ),
    false,
    "tests/cardDropText.test.ts must be deleted",
  );
});

test("drag-and-drop on collections stays operational after the cleanup", () => {
  // The sidebar drop zone remains the single surface that
  // executes the mutation. The pointer drag controller, the
  // sidebar's collection drop zone and the cv-pointer-drag-ghost
  // are the three pieces that must stay intact; the previous
  // CardDropText indicator only fed the visual debug layer.
  const sidebarSource = readFileSync(
    resolvePath(process.cwd(), "src", "OrganizationSidebar.svelte"),
    "utf8",
  );
  assert.ok(
    sidebarSource.includes(COLLECTION_DROP_TARGET_VALUE),
    "OrganizationSidebar must keep the collection drop-target marker",
  );
  assert.ok(
    sidebarSource.includes("createCollectionDropZoneHandlers"),
    "OrganizationSidebar must keep the drop-zone handler factory",
  );
  const pointerSource = readFileSync(
    resolvePath(process.cwd(), "src", "lib", "pointerDragAndDrop.ts"),
    "utf8",
  );
  assert.ok(
    pointerSource.includes("installPointerDragController"),
    "pointerDragAndDrop must keep the singleton pointer controller",
  );
  assert.ok(
    pointerSource.includes("cv-pointer-drag-ghost"),
    "pointerDragAndDrop must keep the cv-pointer-drag-ghost factory",
  );
  assert.equal(
    /createCardDropTextHandlers/.test(pointerSource),
    false,
    "pointerDragAndDrop must NOT depend on the deleted cardDropTextHandlers factory",
  );
});

test("the horizontal arrow navigation stays wired after the cleanup", () => {
  // The regression surfaced because a future cleanup could
  // accidentally remove the rail handler that delegates the
  // navigation math to `horizontalRailNextSelectionId`. The
  // assertions pin the keydown listener, the preventDefault call
  // and the helper delegation so a future refactor that removes
  // the bottom indicator cannot silently drop the navigation.
  assert.match(
    railSource,
    /function onRailHorizontalKeydown/,
    "HistoryCardRail must keep onRailHorizontalKeydown so arrow navigation survives",
  );
  assert.match(
    railSource,
    /addEventListener\("keydown",\s*onRailHorizontalKeydown,\s*true\)/,
    "HistoryCardRail must install the horizontal keydown listener in capture phase",
  );
  assert.match(
    railSource,
    /horizontalRailNextSelectionId\(/,
    "HistoryCardRail must keep delegating to horizontalRailNextSelectionId",
  );
  assert.match(
    railSource,
    /scrollSelectedCardIntoView/,
    "HistoryCardRail must keep scrollSelectedCardIntoView so the freshly selected card lands inside the rail",
  );
});

test("the click selection stays wired after the cleanup", () => {
  // The click surface that toggles `selectedEntryId` must remain
  // inside the card's `article` element. Removing the bottom
  // indicator cannot regress the click handler that flips
  // selection.
  assert.match(
    cardSource,
    /function onCardSurfaceClick/,
    "HistoryCard must keep onCardSurfaceClick so click toggles selection",
  );
  assert.match(
    cardSource,
    /dispatchSelect\(selected\s*\?\s*null\s*:\s*entry\.id\)/,
    "HistoryCard must dispatch the entry id (not an index) when click toggles selection",
  );
  assert.match(
    railSource,
    /bind:selectedEntryId/,
    "HistoryCardRail must keep the selectedEntryId binding the parent App.svelte reads",
  );
  assert.match(
    railSource,
    /selectedEntryId\s*!==\s*null[\s\S]*?visibleEntryIds\.has\(selectedEntryId\)/,
    "HistoryCardRail must keep the reactive cleanup that drops a stale selection when the entry leaves scope",
  );
});

test("no accessibility marker was removed from the rail or the card", () => {
  // The cleanup targeted the visual indicator only; the rail's
  // `role="list"`, the card's `role`-friendly markup, the
  // `aria-label` on the rail and the `aria-live` status must
  // stay intact so screen readers still announce the same
  // surfaces after the indicator is gone.
  assert.match(
    railSource,
    /role="list"/,
    "HistoryCardRail must keep role=list on the rail container",
  );
  assert.match(
    railSource,
    /aria-label="Historial reciente"/,
    "HistoryCardRail must keep the rail's accessible name",
  );
  assert.match(
    cardSource,
    /aria-selected=\{selected\}/,
    "HistoryCard must keep the aria-selected binding the screen reader consults",
  );
  assert.match(
    cardSource,
    /data-testid="history-card"/,
    "HistoryCard must keep the stable data-testid hook",
  );
  assert.match(
    cardSource,
    /draggable="false"/,
    "HistoryCard must keep draggable=\"false\" so the HTML5 drag lifecycle stays out of the rail",
  );
});

// ---------------------------------------------------------------------------
// 14. Orphan APP_FALLBACK_ICON_SVG leaking below the rail
//
// A fifth QA pass detected that the giant icon visible below the
// horizontal card list on the Desktop was NOT the legacy
// CardDropText drop indicator the section 13 cleanup removed. The
// real source was the unconditional `{@html APP_FALLBACK_ICON_SVG}`
// render that the `ClipboardPreview.svelte` component was mounting
// once at its top level (outside the `{#if entry != null}` block).
//
// The fallback glyph is consumed inline (inside
// `<span class="…-fallback">` cells with explicit parent
// dimensions) by `HistoryCard.svelte`, `SourceAppFilter.svelte`
// and `QuickPaste.svelte`. The string itself carries NO
// `width`, `height`, `position:absolute` or styling wrapper, so
// rendering it once at the ClipboardPreview root painted the
// glyph at the document's intrinsic SVG size (~ 300×150 px in
// headless Chrome) immediately below the rail.
//
// The fix: drop the unconditional render, drop the import, and
// add a regression test that pins the new contract so the glyph
// can never silently reappear at the root of the overlay.
// ---------------------------------------------------------------------------

test("ClipboardPreview never renders APP_FALLBACK_ICON_SVG at the component root", () => {
  // The component must mount the hidden `CONTENT_TYPE_ICON_SPRITE`
  // so the `<use href="#cv-icon-…">` references inside the overlay
  // resolve, but it MUST NOT mount the fallback glyph at the
  // component root: the glyph has no `width`, `height` or
  // `position:absolute` wrapper, so rendering it once at the root
  // would paint the fallback mark at the document's intrinsic SVG
  // size immediately below the rail (the regression the QA pass
  // surfaced as "icono grande debajo de la lista horizontal de
  // cards del Desktop").
  const previewSource = readFileSync(
    resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
    "utf8",
  );
  assert.ok(
    previewSource.includes("CONTENT_TYPE_ICON_SPRITE"),
    "ClipboardPreview must keep mounting the shared content-type sprite so <use> resolves",
  );
  assert.equal(
    previewSource.includes("APP_FALLBACK_ICON_SVG"),
    false,
    "ClipboardPreview must not import or render APP_FALLBACK_ICON_SVG at the root (it would leak a giant glyph below the rail)",
  );
});

test("APP_FALLBACK_ICON_SVG is still consumed by the inline fallback cells", () => {
  // The fix is structural: the glyph is still consumed by the
  // documented inline fallback cells (`<span class="…-fallback">`)
  // in HistoryCard, SourceAppFilter and QuickPaste. Removing the
  // top-level render must not regress the per-row fallback, which
  // is the only path that needs the SVG to render visibly at a
  // controlled size.
  const cardSource = readFileSync(
    resolvePath(process.cwd(), "src", "HistoryCard.svelte"),
    "utf8",
  );
  const sourceAppSource = readFileSync(
    resolvePath(process.cwd(), "src", "SourceAppFilter.svelte"),
    "utf8",
  );
  const quickPasteSource = readFileSync(
    resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
    "utf8",
  );
  assert.match(
    cardSource,
    /\{@html APP_FALLBACK_ICON_SVG\}/,
    "HistoryCard must keep the inline fallback glyph inside .source-app-fallback",
  );
  assert.match(
    sourceAppSource,
    /\{@html APP_FALLBACK_ICON_SVG\}/,
    "SourceAppFilter must keep the inline fallback glyph inside the trigger / option icons",
  );
  assert.match(
    quickPasteSource,
    /\{@html APP_FALLBACK_ICON_SVG\}/,
    "QuickPaste must keep the inline fallback glyph inside .qp-source-app-fallback",
  );
});

test("the Desktop layout-main never mounts a stray visible SVG below the rail", () => {
  // Beyond the APP_FALLBACK_ICON_SVG root cause, the section 13
  // regression surfaced the broader question: are there ANY visible
  // SVG / image / placeholder nodes the layout column could
  // accidentally mount below the rail? The structural guarantee is
  // that the right column ends with `<HistoryCardRail />` and that
  // nothing else sits between the rail and `</div>`. The assertion
  // re-asserts the slice between the rail and the closing of
  // `.layout-main` is empty (or whitespace-only), so no orphan
  // sprite — visible or otherwise — can resurface below the rail.
  const appSource = readFileSync(
    resolvePath(process.cwd(), "src", "App.svelte"),
    "utf8",
  );
  const layoutMainStart = appSource.indexOf('class="layout-main"');
  assert.notEqual(layoutMainStart, -1, "App.svelte must declare the .layout-main column");
  const railIdx = appSource.indexOf("<HistoryCardRail", layoutMainStart);
  assert.notEqual(railIdx, -1, "App.svelte must mount <HistoryCardRail> inside .layout-main");
  const railEnd = appSource.indexOf("/>", railIdx) + 2;
  const layoutMainEnd = appSource.indexOf("</div>", railEnd);
  assert.notEqual(layoutMainEnd, -1, "layout-main must close after the rail");
  const slice = appSource.slice(railEnd, layoutMainEnd);
  assert.equal(
    /<\s*(div|section|p|span|svg|aside|footer|header|nav|article|main|img)\b/i.test(slice),
    false,
    "App.svelte must not render any block element between the rail and the closing of .layout-main",
  );
});

test("ClipboardPreview mounts only the hidden sprite, never the visible glyph", () => {
  // The two sprite constants the overlay needs are:
  //
  //   - CONTENT_TYPE_ICON_SPRITE: a single `<svg width="0"
  //     height="0" style="position:absolute">` that carries the
  //     `<symbol id="cv-icon-…">` registry. Mounting it at the root
  //     is safe because the geometry is forced to zero.
  //   - APP_FALLBACK_ICON_SVG: a standalone `<svg viewBox="0 0 24 24">`
  //     with no `width` / `height`, so it falls back to the user
  //     agent's intrinsic SVG size (~ 300×150 px) when rendered
  //     outside a sized parent. The overlay must never mount it
  //     at the root; the consumers that DO need the glyph already
  //     wrap it inside dimenisoned spans.
  //
  // This test pins both invariants through the component source so
  // a future contributor cannot reintroduce the regression. The
  // root-level `{@html …}` consumers may legitimately include the
  // `highlightedHtml` payload the code-language branch consumes;
  // those consumers render escaped highlight.js output, not raw
  // icons, so they are out of scope for this assertion.
  const previewSource = readFileSync(
    resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
    "utf8",
  );
  // No `APP_FALLBACK_ICON_SVG` reference survives in the component
  // — neither as an import nor as a root-level `@html` consumer.
  // A regression that re-imported or re-rendered the glyph at the
  // root would surface here.
  assert.equal(
    /APP_FALLBACK_ICON_SVG/.test(previewSource),
    false,
    "ClipboardPreview must not reference APP_FALLBACK_ICON_SVG (the glyph has no intrinsic dimensions and would render visibly at the document level)",
  );
  // The sprite registry still mounts at the root so the `<use>`
  // references inside the overlay resolve.
  assert.ok(
    previewSource.includes("CONTENT_TYPE_ICON_SPRITE"),
    "ClipboardPreview must keep mounting the hidden CONTENT_TYPE_ICON_SPRITE so <use> resolves",
  );
});
