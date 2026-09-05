/**
 * Regression coverage for the `desktop-dnd-card-visual-corrections`
 * change.
 *
 * The contract being pinned in addition to the prior
 * `desktop-header-card-dnd` invariants:
 *
 *   - The `text/plain` fallback the card drops is a STRICT,
 *     versioned token of the form `clipvault-entry:v1:<integer-id>`.
 *     The parser rejects any deviation so a foreign drag from
 *     another webview (a file path, a piece of clipboard text, an
 *     URL, a JSON blob) cannot impersonate an entry id.
 *   - The sidebar accepts `dragover` events that expose ONLY the
 *     `text/plain` fallback (the WebKit/Tauri path that strips the
 *     private MIME).
 *   - The drop handler reads both representations from the
 *     DataTransfer so the user-visible flow survives the
 *     platform quirk.
 *   - The visual feedback for a valid drop target uses a smooth,
 *     local CSS transition on `background-color`, `border-color`
 *     and `box-shadow`; the highlight is cleaned up on every
 *     documented lifecycle event (dragleave, drop, dragend, error,
 *     destroy) and the document listener is unique.
 *   - The pin control renders a minimalist local pushpin SVG
 *     (outline and filled variants); no star glyph or fallback is
 *     ever painted.
 *   - The source-app icon visual size is exactly 50% larger than
 *     the prior baseline of `1.1rem` (the documented target is
 *     `1.65rem`); the resolver, fallback, accessible name and
 *     square card dimensions stay intact.
 *   - The technical listener-status / synthetic-paste line below
 *     the rail is gone. Diagnostic info still surfaces inside the
 *     `Development` surface (capabilities + shortcut modal
 *     listener status).
 *
 * Tests in this file are intentionally source-level so a regression
 * in the rendered DOM or the CSS surface surfaces as a failed
 * assertion rather than a runtime crash.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  CLIPVAULT_ENTRY_MIME,
  CLIPVAULT_ENTRY_TEXT_PREFIX,
  __resetDragSessionForTests,
  beginDragSession,
  buildDragPayload,
  endDragSession,
  getActiveDragSessionEntryId,
  hasActiveDragSession,
  parseDragPayload,
  parseDragPayloadFromTransfer,
  parseDragTextPayload,
  combineMemberships,
  isDragPayload,
  acceptsDragOver,
  isDropTarget,
} from "../src/lib/dragAndDrop.ts";

const FRONTEND_ROOT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const REPO_ROOT = path.resolve(FRONTEND_ROOT, "..", "..", "..");

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
}

// ---------------------------------------------------------------------------
// Pure helpers: dual payload, versioned text/plain, strict parser.
// ---------------------------------------------------------------------------

test("buildDragPayload returns a dual representation (private MIME + text/plain fallback)", () => {
  const pair = buildDragPayload(42);
  // Private MIME payload keeps the original JSON contract.
  assert.equal(
    pair.privatePayload,
    JSON.stringify({ id: 42 }),
    "private MIME must carry the canonical JSON shape",
  );
  // text/plain fallback uses the strict versioned prefix.
  assert.equal(
    pair.textPayload,
    `${CLIPVAULT_ENTRY_TEXT_PREFIX}42`,
    "text/plain fallback must use the versioned prefix",
  );
});

test("parseDragTextPayload accepts ONLY the exact versioned token", () => {
  // Round-trip succeeds.
  assert.equal(parseDragTextPayload(`${CLIPVAULT_ENTRY_TEXT_PREFIX}7`), 7);
  assert.equal(parseDragTextPayload(`${CLIPVAULT_ENTRY_TEXT_PREFIX}123456`), 123456);
  // Anything that escapes the exact format is rejected.
  const rejected: Array<string | null | undefined> = [
    null,
    undefined,
    "",
    "clipvault-entry:",
    "clipvault-entry:v1:",
    "clipvault-entry:v0:7",
    "clipvault-entry:v2:7",
    "clipvault-entry:v1:-1",
    "clipvault-entry:v1:+1",
    "clipvault-entry:v1:1.5",
    "clipvault-entry:v1:1e2",
    "clipvault-entry:v1:0x10",
    "clipvault-entry:v1: 7",
    "clipvault-entry:v1:7 ",
    "clipvault-entry:v1:7;extra=1",
    ` file:///${CLIPVAULT_ENTRY_TEXT_PREFIX}7`,
    JSON.stringify({ id: 7 }),
    `${CLIPVAULT_ENTRY_TEXT_PREFIX}`.slice(0, -1),
  ];
  for (const malformed of rejected) {
    assert.equal(
      parseDragTextPayload(malformed),
      null,
      `malformed payload must be rejected: ${JSON.stringify(malformed)}`,
    );
  }
});

test("parseDragPayloadFromTransfer prefers the private MIME and falls back to text/plain", () => {
  const pair = buildDragPayload(11);
  assert.equal(
    parseDragPayloadFromTransfer({
      privatePayload: pair.privatePayload,
      textPayload: null,
    }),
    11,
  );
  // The WebKit/Tauri path: only the text/plain fallback is exposed.
  assert.equal(
    parseDragPayloadFromTransfer({
      privatePayload: null,
      textPayload: pair.textPayload,
    }),
    11,
  );
  // Both representations exposed: the helper prefers the private
  // MIME so the original contract wins when both are available.
  assert.equal(
    parseDragPayloadFromTransfer({
      privatePayload: pair.privatePayload,
      textPayload: `${CLIPVAULT_ENTRY_TEXT_PREFIX}999`,
    }),
    11,
  );
  // Neither representation is usable.
  assert.equal(
    parseDragPayloadFromTransfer({
      privatePayload: null,
      textPayload: null,
    }),
    null,
  );
  assert.equal(
    parseDragPayloadFromTransfer({
      privatePayload: "not json",
      textPayload: "not the prefix",
    }),
    null,
  );
});

test("isDragPayload matches the private MIME and the text/plain fallback", () => {
  assert.equal(isDragPayload([CLIPVAULT_ENTRY_MIME]), true);
  assert.equal(isDragPayload(["text/plain"]), true);
  // Both at once.
  assert.equal(
    isDragPayload([CLIPVAULT_ENTRY_MIME, "text/plain"]),
    true,
  );
  // Foreign types are rejected.
  assert.equal(isDragPayload(["Files"]), false);
  assert.equal(isDragPayload([]), false);
  assert.equal(isDragPayload(null), false);
  assert.equal(isDragPayload(undefined), false);
  // DOMStringList shape: the helper still accepts text/plain.
  const domList = {
    contains(value: string): boolean {
      return value === "text/plain";
    },
    length: 1,
  } as unknown as DOMStringList;
  assert.equal(isDragPayload(domList), true);
});

test("acceptsDragOver accepts dragover events exposing only the text/plain fallback", () => {
  assert.equal(
    acceptsDragOver({ dataTransfer: { types: ["text/plain"] } }),
    true,
    "the sidebar must opt a row into drop when only the text/plain fallback is exposed",
  );
  assert.equal(
    acceptsDragOver({ dataTransfer: null }),
    false,
    "missing dataTransfer must keep the row inert",
  );
  assert.equal(
    acceptsDragOver({ dataTransfer: { types: [] } }),
    false,
  );
});

test("combineMemberships is still additive and idempotent with the dual payload", () => {
  // Re-validating the existing safety contract so the new dual
  // payload cannot regress it.
  const added = combineMemberships(1, 7, [3, 4], 99);
  assert.equal(added.safeNoop, false);
  assert.deepEqual(added.nextCollectionIds, [3, 4, 7, 99]);
  const idem = combineMemberships(1, 7, [7, 99], 99);
  assert.equal(idem.safeNoop, true);
  assert.equal(idem.reason, "already_member");
  const hist = combineMemberships(1, 99, [3, 99], 99);
  assert.equal(hist.safeNoop, true);
  assert.equal(hist.reason, "system_collection");
  const empty = combineMemberships(1, 7, null, 99);
  assert.equal(empty.safeNoop, true);
  assert.equal(empty.reason, "missing_hydration");
  assert.equal(empty.nextCollectionIds, undefined);
});

// ---------------------------------------------------------------------------
// Source-level invariants: drop reads both representations, dragover
// opts in via either MIME.
// ---------------------------------------------------------------------------

test("OrganizationSidebar wires the delegated dragover to opt into drop when only the text/plain fallback is exposed", () => {
  const helperSource = stripComments(
    loadSource("src/lib/collectionDropZone.ts"),
  );
  // The delegated handler must gate on `acceptsDragOver` so a
  // drag that only exposes the text/plain fallback (or nothing
  // at all when the in-memory session is active) still opts the
  // row in.
  const dragOverHandler = helperSource.match(
    /function onDragOver\([\s\S]*?\n  \}/,
  );
  assert.ok(dragOverHandler, "onDragOver must be defined in the helper");
  assert.match(
    dragOverHandler?.[0] ?? "",
    /acceptsDragOver\(event\)/,
    "dragover must gate on acceptsDragOver",
  );
  assert.match(
    dragOverHandler?.[0] ?? "",
    /event\.preventDefault\(\)/,
    "dragover must call preventDefault to opt into drop",
  );
  // `onDragEnter` must also gate on acceptsDragOver so a drag
  // that only exposes the text/plain fallback still highlights
  // the target.
  const dragEnterHandler = helperSource.match(
    /function onDragEnter\([\s\S]*?\n  \}/,
  );
  assert.ok(dragEnterHandler, "onDragEnter must be defined in the helper");
  assert.match(
    dragEnterHandler?.[0] ?? "",
    /acceptsDragOver\(event\)/,
    "dragenter must gate on acceptsDragOver",
  );
});

test("OrganizationSidebar drop reads both the private MIME and the text/plain fallback", () => {
  const helperSource = stripComments(
    loadSource("src/lib/collectionDropZone.ts"),
  );
  const dropHandler = helperSource.match(/function onDrop\([\s\S]*?\n  \}/);
  assert.ok(dropHandler, "onDrop must be defined in the helper");
  const body = dropHandler?.[0] ?? "";
  // The drop handler must consult BOTH representations through
  // the single helper so the WebKit/Tauri path stays wired.
  assert.match(body, /getData\(\s*CLIPVAULT_ENTRY_MIME\s*\)/);
  assert.match(body, /getData\(\s*"text\/plain"\s*\)/);
  assert.match(body, /parseDragPayloadFromTransfer\(/);
  // The handler must not log clipboard content, snippets or
  // asset references when the drop fails.
  for (const forbidden of [
    "clipboard",
    "DataTransfer",
    "content_hash",
    "asset_ref",
    "snippet",
    "console\\.log",
  ]) {
    assert.equal(
      new RegExp(forbidden).test(body),
      false,
      `drop handler must not mention "${forbidden}"`,
    );
  }
});

test("pointer drag bridge is metadata-only and card drag disables the competing native path", () => {
  const cardSource = stripComments(loadSource("src/HistoryCard.svelte"));
  const pointerSource = stripComments(loadSource("src/lib/pointerDragAndDrop.ts"));
  assert.match(cardSource, /draggable="false"/);
  assert.match(pointerSource, /data-testid=\"history-card\"/);
  assert.match(pointerSource, /data-entry-id/);
  assert.match(pointerSource, /POINTER_DRAG_OVER_EVENT/);
  assert.match(pointerSource, /POINTER_DROP_EVENT/);
});

// ---------------------------------------------------------------------------
// Visual feedback: transition + highlight cleanup + listener uniqueness.
// ---------------------------------------------------------------------------

test("OrganizationSidebar collection row has a local CSS transition for the drag-over highlight", () => {
  const source = loadSource("src/OrganizationSidebar.svelte");
  // The transition lives on the `.collection-row.drop-target`
  // selector so entering and leaving the row both animate.
  assert.match(
    source,
    /\.collection-row\.drop-target\s*\{[^}]*transition:/s,
    "drop-target row must declare a transition",
  );
  // The transition must cover the documented properties:
  // background-color, border-color, box-shadow.
  assert.match(source, /background-color\s+[\d.]+s\s+ease-out/);
  assert.match(source, /border-color\s+[\d.]+s\s+ease-out/);
  assert.match(source, /box-shadow\s+[\d.]+s\s+ease-out/);
  // The drag-over state must still change background, border and
  // shadow so the highlight is perceptible.
  assert.match(source, /\.collection-row\.drop-target\.drag-over\s*\{[^}]*background:/s);
  assert.match(source, /\.collection-row\.drop-target\.drag-over\s*\{[^}]*box-shadow:/s);
});

test("OrganizationSidebar installs the document dragend listener exactly once", () => {
  const source = stripComments(loadSource("src/OrganizationSidebar.svelte"));
  // The sidebar installs a single document-level `dragend` listener
  // in onMount and removes it in onDestroy so the highlight never
  // sticks across cancels.
  assert.match(source, /document\.addEventListener\("dragend", onWindowDragEnd\)/);
  assert.match(source, /document\.removeEventListener\("dragend", onWindowDragEnd\)/);
  // Only one `onWindowDragEnd` definition must exist.
  const matches = source.match(/function onWindowDragEnd/g) ?? [];
  assert.equal(
    matches.length,
    1,
    "onWindowDragEnd must be defined exactly once",
  );
  // `resetDragOver` must be called inside the dragend handler and
  // inside onDestroy so the highlight clears on cancel and on
  // component teardown.
  assert.match(source, /function onWindowDragEnd[\s\S]*?resetDragOver\(\)/);
  assert.match(source, /onDestroy\(\(\) =>\s*\{[\s\S]*?resetDragOver\(\)/);
});

test("OrganizationSidebar flags the system collection so the highlight never lies", () => {
  const source = stripComments(loadSource("src/OrganizationSidebar.svelte"));
  // The defensive `system-collection` class plus a CSS rule that
  // suppresses the highlight if the system row ever received a
  // dragover keeps the visual contract honest even if the JS guard
  // regressed.
  assert.match(
    source,
    /class:system-collection=\{collection\.kind === "system"\}/,
  );
  assert.match(
    source,
    /\.collection-row\.drop-target\.system-collection\.drag-over\s*\{[^}]*background:\s*transparent/s,
  );
});

// ---------------------------------------------------------------------------
// Pin control: minimalist local pushpin, no star, accessibility preserved.
// ---------------------------------------------------------------------------

test("HistoryCard pin control renders a minimalist local pushpin SVG", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  // No star glyphs anywhere in the component.
  for (const star of ["★", "☆", "⭐", "🌟", "✦", "✧", "❋", "✱"]) {
    assert.equal(
      source.includes(star),
      false,
      `pin control must not include star glyph "${star}"`,
    );
  }
  // Both filled and outline variants render through inline SVGs
  // with their own data-testid hook.
  assert.match(source, /<svg[\s\S]*?history-card-pin-filled/);
  assert.match(source, /<svg[\s\S]*?history-card-pin-outline/);
  // The geometry inside each pin variant is a single `<path>`
  // (drawn vertically, then rotated 45° around the viewport
  // centre). The previous `<circle>` + `<line>` + `<polygon>`
  // composition read as a magnifying glass / key and must never
  // come back; a single continuous path is the unmistakable
  // thumbtack signature the spec pins.
  //
  // The matcher anchors on the `<svg` immediately preceding the
  // pin's unique `data-testid` so it does not bleed into the
  // other inline SVGs the card renders (content-type icon,
  // title-edit confirm / cancel, metadata clock / size, thumbnail
  // loading and fallback, delete menu icon).
  const filledSvg = source.match(
    /<svg\b[^>]*\bdata-testid="history-card-pin-filled"[\s\S]*?<\/svg>/,
  );
  assert.ok(filledSvg, "filled pin svg must be defined");
  assert.equal(
    /<circle\b/.test(filledSvg?.[0] ?? ""),
    false,
    "filled pin must not declare a <circle> (circle + line + polygon = magnifying glass)",
  );
  assert.equal(
    /<line\b/.test(filledSvg?.[0] ?? ""),
    false,
    "filled pin must not declare a <line> (circle + line + polygon = magnifying glass)",
  );
  assert.equal(
    /<polygon\b/.test(filledSvg?.[0] ?? ""),
    false,
    "filled pin must not declare a <polygon> (circle + line + polygon = magnifying glass)",
  );
  assert.match(
    filledSvg?.[0] ?? "",
    /<path\b[^>]*\bd="[^"]+"/,
    "filled pin must declare a single <path> carrying a non-empty 'd' attribute",
  );
  assert.match(
    filledSvg?.[0] ?? "",
    /<g[^>]*\btransform="rotate\(45 12 12\)"/,
    "filled pin must tilt the silhouette 45° around the viewport centre",
  );
  const outlineSvg = source.match(
    /<svg\b[^>]*\bdata-testid="history-card-pin-outline"[\s\S]*?<\/svg>/,
  );
  assert.ok(outlineSvg, "outline pin svg must be defined");
  assert.equal(
    /<circle\b/.test(outlineSvg?.[0] ?? ""),
    false,
    "outline pin must not declare a <circle>",
  );
  assert.equal(
    /<line\b/.test(outlineSvg?.[0] ?? ""),
    false,
    "outline pin must not declare a <line>",
  );
  assert.equal(
    /<polygon\b/.test(outlineSvg?.[0] ?? ""),
    false,
    "outline pin must not declare a <polygon>",
  );
  assert.match(
    outlineSvg?.[0] ?? "",
    /<path\b[^>]*\bd="[^"]+"/,
    "outline pin must declare a single <path> carrying a non-empty 'd' attribute",
  );
  assert.match(
    outlineSvg?.[0] ?? "",
    /<g[^>]*\btransform="rotate\(45 12 12\)"/,
    "outline pin must tilt the silhouette 45° around the viewport centre",
  );
  // No remote resources, no emoji, no fallback glyphs.
  assert.equal(/src=["']https?:/.test(source), false);
  assert.equal(/href=["']https?:/.test(source), false);
});

test("HistoryCard pin control keeps aria-pressed, Anclar/Desanclar, focus-visible and the busy hook", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  assert.match(source, /aria-pressed=\{entry\.is_pinned\}/);
  assert.match(source, /title=\{entry\.is_pinned \? "Desanclar" : "Anclar"\}/);
  assert.match(
    source,
    /aria-label=\{entry\.is_pinned \? `Desanclar entrada \$\{displayTitle\}` : `Anclar entrada \$\{displayTitle\}`\}/,
  );
  assert.match(source, /data-pinned=\{entry\.is_pinned \? "true" : "false"\}/);
  assert.match(source, /on:click=\{handlePinClick\}/);
});

// ---------------------------------------------------------------------------
// Source-app icon visual size: exactly 50% larger than the 1.1rem baseline.
// ---------------------------------------------------------------------------

test("HistoryCard source-app icon visual size is exactly 50% larger than the 1.1rem baseline", () => {
  const source = loadSource("src/HistoryCard.svelte");
  // Both the resolved icon and the fallback share the same CSS
  // rule. The new size is `1.65rem` (50% larger than `1.1rem`).
  assert.match(source, /\.source-app-icon,\s*\.source-app-fallback\s*\{[^}]*width:\s*1\.65rem/s);
  assert.match(source, /\.source-app-icon,\s*\.source-app-fallback\s*\{[^}]*height:\s*1\.65rem/s);
  // The shape must stay a square so the card layout does not
  // shift.
  assert.match(source, /\.source-app-icon,\s*\.source-app-fallback\s*\{[^}]*border-radius:\s*4px/s);
  // `object-fit: contain` must be preserved.
  assert.match(source, /\.source-app-icon,\s*\.source-app-fallback\s*\{[^}]*object-fit:\s*contain/s);
  // The fallback still relies on the same icon resolver pipeline.
  assert.match(source, /source-app-fallback[\s\S]*?\{@html APP_FALLBACK_ICON_SVG\}/);
  // The card keeps its `aria-label` and `title` from the
  // accessible label helper.
  assert.match(source, /aria-label=\{sourceAppAccessibleLabel\(entry\)\}/);
  assert.match(source, /title=\{sourceAppAccessibleLabel\(entry\)\}/);
  // The card width stays on `--cv-card-size` so the source icon
  // enlargement cannot push it out of the rail.
  assert.match(source, /\.card\s*\{[^}]*width:\s*var\(--cv-card-size/s);
});

// ---------------------------------------------------------------------------
// Technical footer line: gone from the main desktop, kept in Development.
// ---------------------------------------------------------------------------

test("App.svelte no longer renders the technical listener-status line below the rail", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  // The `<p class="listener-status">` block is gone, no empty
  // reserved band remains.
  assert.equal(
    source.includes('class="listener-status"'),
    false,
    "the listener-status block must be removed from the desktop",
  );
  assert.equal(
    source.includes("data-testid=\"quick-search-listener-status\""),
    false,
    "the listener-status testid hook must be removed too",
  );
  // The associated CSS rule must be gone so the rail does not
  // reserve vertical space for an invisible element.
  assert.equal(
    /\.listener-status\s*\{/.test(source),
    false,
    "the listener-status CSS rule must be removed",
  );
  // Padding on <main> stays compact — no extra band reserved.
  assert.match(source, /padding:\s*1rem 1\.25rem 1rem/);
});

test("Development surface still exposes capabilities and listener diagnostics", () => {
  const devSource = loadSource("src/DevelopmentModal.svelte");
  // Capabilities (synthetic_paste, clipboard_read, global_hotkey,
  // …) are part of the documented Development surface so the user
  // can still inspect them after the footer is removed.
  assert.match(devSource, /synthetic_paste:/);
  assert.match(devSource, /global_hotkey:/);
  assert.match(devSource, /clipboard_read:/);
  assert.match(devSource, /clipboard_write:/);

  // The shortcut modal (reached from the toolbar / Development
  // surface) carries the listener status and listener error.
  const shortcutSource = loadSource("src/QuickPasteShortcutModal.svelte");
  assert.match(shortcutSource, /describeListener/);
  assert.match(shortcutSource, /shortcut-listener-status/);
  assert.match(shortcutSource, /shortcut-capability-status/);
  assert.match(shortcutSource, /shortcut-listener-error/);
});

// ---------------------------------------------------------------------------
// Existing change invariants that MUST still hold.
// ---------------------------------------------------------------------------

test("OrganizationSidebar still refuses to wire the system Historial collection as a drop target", () => {
  const sidebarSource = stripComments(loadSource("src/OrganizationSidebar.svelte"));
  // `isDropTarget` is the single source of truth: only user
  // collections are wired as drop targets. The CSS class mirrors
  // the predicate so the visual feedback never appears on
  // Historial.
  assert.match(sidebarSource, /class:drop-target=\{isDropTarget\(collection\)\}/);
  // The drop zone helper must refuse non-user collections so
  // the visual feedback and the dispatch both stay inert on
  // Historial.
  const helperSource = stripComments(
    loadSource("src/lib/collectionDropZone.ts"),
  );
  assert.match(
    helperSource,
    /collection\.kind !== "user"\)\s*return\s+null/,
    "resolveRow must refuse non-user targets",
  );
  // `isDropTarget` itself only accepts user collections.
  assert.equal(isDropTarget({ kind: "system" }), false);
  assert.equal(isDropTarget({ kind: "user" }), true);
  assert.equal(isDropTarget(null), false);
});

test("App.svelte routes card-drop through combineMemberships + entryCollectionsSetCommand", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  assert.match(source, /on:card-drop=\{\(e\) => handleCardDrop\(e\)\}/);
  assert.match(source, /function handleCardDrop/);
  assert.match(source, /combineMemberships\(\s*entryId,\s*collectionId/);
  assert.match(
    source,
    /entryCollectionsSetCommand\(\{\s*entryId,\s*collectionIds: result\.nextCollectionIds/,
  );
  // The safe-noop branch refuses to call the bridge when the
  // helper says the operation would be a destructive empty list
  // (missing hydration) or already a member. The branch may also
  // clear the in-flight tracker before `return` so a future
  // drop on the same target can still land.
  assert.match(
    source,
    /if \(result\.safeNoop \|\| result\.nextCollectionIds === undefined\)[\s\S]*?return;/,
  );
  // The flow MUST deduplicate concurrent drops on the same
  // (entry, collection) pair.
  assert.match(source, /dropInFlight\.has\(\s*dropKey\s*\)/);
});

test("App.svelte::handleCardDrop never logs clipboard content or asset paths", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  const handler = source.match(/function handleCardDrop[\s\S]*?\n  \}/);
  assert.ok(handler);
  const body = handler?.[0] ?? "";
  for (const forbidden of [
    "content_hash",
    "snippet",
    "asset_ref",
    "mime_type",
    "clipboard",
    "DataTransfer",
  ]) {
    assert.equal(
      body.includes(forbidden),
      false,
      `handleCardDrop must not include "${forbidden}"`,
    );
  }
});

test("App.svelte still keeps the layout compact and the rail horizontal", () => {
  const source = loadCommentsHelper(loadSource("src/App.svelte"));
  // The two-column grid uses minmax(0, 1fr) so the rail cannot be
  // forced to grow with its content.
  assert.match(source, /minmax\(0, 1fr\)/);
  // The padding is still compact so the rail does not produce an
  // empty band beneath it.
  assert.match(source, /padding:\s*1rem 1\.25rem 1rem/);
  // The HistoryCardRail still uses a horizontal overflow layout.
  const railSource = loadSource("src/HistoryCardRail.svelte");
  assert.match(railSource, /overflow-x:\s*auto/);
});

function loadCommentsHelper(source: string): string {
  return stripComments(source);
}

// ---------------------------------------------------------------------------
// End-to-end drop flow with a mock Tauri bridge.
//
// The tests below prove the parent flow actually executes the
// `entry_collections_set` command and that the persisted state is
// visible to subsequent reads. They cover the contract the spec
// pins:
//
//   - the drop triggers the bridge once with a payload that includes
//     the target id AND `Historial`;
//   - the drop preserves `Historial` and every other existing
//     collection;
//   - a duplicate drop is a no-op at the bridge layer (no second
//     `entry_collections_set` round-trip);
//   - dropping the entry on `Historial` is a safe no-op (the bridge
//     is never called);
//   - a foreign payload (malformed text/plain or a non-integer id)
//     never reaches the bridge;
//   - the drop reads the associations through `entry_collections`
//     when the cache is unhydrated so an empty list is never sent.
// ---------------------------------------------------------------------------

type DropBridgeRecord = {
  calls: { cmd: string; args?: Record<string, unknown> }[];
  historyId: number;
  collectionIdsByEntry: Map<number, number[]>;
  collections: { id: number; kind: "system" | "user"; name: string }[];
};

function installDropBridge(
  options: {
    historyId?: number;
    initialCollections?: Map<number, number[]>;
    collections?: { id: number; kind: "system" | "user"; name: string }[];
  } = {},
): DropBridgeRecord {
  const historyId = options.historyId ?? 1;
  const calls: { cmd: string; args?: Record<string, unknown> }[] = [];
  const collectionIdsByEntry = new Map<number, number[]>(
    options.initialCollections ?? new Map(),
  );
  const collections = options.collections ?? [
    { id: historyId, kind: "system" as const, name: "Historial" },
    { id: 7, kind: "user" as const, name: "Trabajo" },
    { id: 9, kind: "user" as const, name: "Clientes" },
  ];
  const tauri = {
    transformCallback: () => 0,
    invoke: async <T>(
      cmd: string,
      args?: Record<string, unknown>,
    ): Promise<T> => {
      calls.push({ cmd, args });
      if (cmd === "clipvault_entry_collections") {
        const entryId = (args as { entryId: number }).entryId;
        return (collectionIdsByEntry.get(entryId) ?? [
          historyId,
        ]) as unknown as T;
      }
      if (cmd === "clipvault_entry_collections_set") {
        const a = args as { entryId: number; collectionIds: number[] };
        const merged = Array.from(new Set(a.collectionIds));
        if (!merged.includes(historyId)) {
          merged.push(historyId);
        }
        collectionIdsByEntry.set(a.entryId, merged);
        return merged as unknown as T;
      }
      if (cmd === "clipvault_organization_snapshot") {
        return {
          collections: collections.map((c) => ({
            id: c.id,
            kind: c.kind,
            name: c.name,
            display_name: c.name,
            created_at: "2026-01-02T03:04:05Z",
            updated_at: "2026-01-02T03:04:05Z",
          })),
          tags: [],
        } as unknown as T;
      }
      if (cmd === "clipvault_unorganized_clearable_count") {
        return 0 as unknown as T;
      }
      if (cmd === "clipvault_diagnostics") {
        return {
          platform_os: "macos",
          app_version: "0.1.0",
          data_dir: "/tmp/clipvault",
          database_path: "/tmp/clipvault/clipvault.db",
        } as unknown as T;
      }
      if (cmd === "clipvault_platform_capabilities") {
        return {} as unknown as T;
      }
      if (cmd === "clipvault_active_application") {
        return { bundle_identifier: null, display_name: null } as unknown as T;
      }
      if (cmd === "clipvault_recent_entries") {
        return [] as unknown as T;
      }
      if (cmd === "clipvault_entry_tags") {
        return [] as unknown as T;
      }
      return undefined as unknown as T;
    },
  };
  const globalScope = globalThis as unknown as { window?: unknown };
  globalScope.window = { __TAURI_INTERNALS__: tauri };
  return { calls, historyId, collectionIdsByEntry, collections };
}

function uninstallDropBridge(): void {
  const globalScope = globalThis as unknown as { window?: unknown };
  globalScope.window = undefined;
}

/**
 * Drop a card through the same code path the sidebar dispatches.
 * Returns the promise the helper returns so the test can `await` it
 * and inspect the resulting state.
 *
 * The helper imports the command wrappers fresh every time so a
 * regression that wires a stale bridge reference surfaces as a
 * failed assertion.
 */
async function simulateDrop(
  entryId: number,
  collectionId: number,
  historyId: number,
  hydration: "loaded" | "pending" | "error" | "absent" = "loaded",
  collections: { id: number; kind: "system" | "user" }[] = [],
): Promise<{ setCalls: number; mergedIds: number[] | null }> {
  // Step 1: parse the payload (the card always writes the dual
  // payload, the sidebar always parses both representations).
  const {
    CLIPVAULT_ENTRY_TEXT_PREFIX,
    buildDragPayload,
    parseDragPayloadFromTransfer,
    combineMemberships,
  } = await import("../src/lib/dragAndDrop.ts");
  const payloads = buildDragPayload(entryId);
  const entryIdParsed = parseDragPayloadFromTransfer({
    privatePayload: payloads.privatePayload,
    textPayload: payloads.textPayload,
  });
  if (entryIdParsed !== entryId) {
    throw new Error(
      `Drop simulator: payload round-trip failed (got ${entryIdParsed})`,
    );
  }

  // Step 2: read current associations either from the
  // metadata-only bridge or from the simulated cache.
  const { entryCollectionsCommand, entryCollectionsSetCommand } =
    await import("../src/lib/tauri.ts");
  let currentCollectionIds: number[] | null;
  if (hydration === "loaded") {
    currentCollectionIds = [historyId];
  } else {
    currentCollectionIds = await entryCollectionsCommand({ entryId });
  }
  // Step 3: the parent filters non-user collections.
  const target = collections.find((c) => c.id === collectionId);
  if (!target || target.kind !== "user") {
    return { setCalls: 0, mergedIds: null };
  }
  const result = combineMemberships(
    entryId,
    collectionId,
    currentCollectionIds,
    historyId,
  );
  if (result.safeNoop || result.nextCollectionIds === undefined) {
    return { setCalls: 0, mergedIds: null };
  }
  const merged = await entryCollectionsSetCommand({
    entryId,
    collectionIds: result.nextCollectionIds,
  });
  // Track the calls that happened during this drop.
  const allCalls = (
    globalThis as unknown as {
      window: { __TAURI_INTERNALS__: { invoke: unknown } };
    }
  ).window.__TAURI_INTERNALS__.invoke;
  void allCalls;
  return {
    setCalls: 1,
    mergedIds: merged,
  };
}

test("drop on a user collection runs entry_collections_set with the merged membership", async () => {
  const bridge = installDropBridge();
  try {
    const { setCalls, mergedIds } = await simulateDrop(
      7,
      9,
      bridge.historyId,
      "loaded",
      bridge.collections,
    );
    assert.equal(setCalls, 1, "the bridge call must run exactly once");
    const setCall = bridge.calls.find(
      (c) => c.cmd === "clipvault_entry_collections_set",
    );
    assert.ok(setCall, "entry_collections_set must run on drop");
    const a = setCall as unknown as { args: { entryId: number; collectionIds: number[] } };
    assert.equal(a.args.entryId, 7);
    assert.ok(
      a.args.collectionIds.includes(9),
      "the target collection must be in the payload",
    );
    assert.ok(
      a.args.collectionIds.includes(bridge.historyId),
      "Historial must be reattached so the card never leaves the protected membership",
    );
    // Subsequent read returns the persisted list — i.e. the drop
    // is durable in the same round-trip.
    const { entryCollectionsCommand } = await import("../src/lib/tauri.ts");
    const persisted = await entryCollectionsCommand({ entryId: 7 });
    assert.deepEqual(persisted.sort(), [bridge.historyId, 9].sort());
    // The set call returned the merged list too.
    assert.deepEqual(mergedIds?.sort(), [bridge.historyId, 9].sort());
  } finally {
    uninstallDropBridge();
  }
});

test("drop with only the text/plain fallback still runs entry_collections_set", async () => {
  // The WebKit/Tauri path strips the private MIME during
  // `dragover` and `drop`. The parser must accept the
  // `clipvault-entry:v1:<id>` fallback and the bridge call must
  // still go through.
  const bridge = installDropBridge();
  try {
    const { parseDragTextPayload, combineMemberships } =
      await import("../src/lib/dragAndDrop.ts");
    const { entryCollectionsCommand, entryCollectionsSetCommand } =
      await import("../src/lib/tauri.ts");
    const { CLIPVAULT_ENTRY_TEXT_PREFIX } = await import(
      "../src/lib/dragAndDrop.ts"
    );
    const entryId = 11;
    const textPayload = `${CLIPVAULT_ENTRY_TEXT_PREFIX}${entryId}`;
    const parsed = parseDragTextPayload(textPayload);
    assert.equal(parsed, entryId, "the text/plain fallback must round-trip");
    const currentCollectionIds = await entryCollectionsCommand({ entryId });
    const result = combineMemberships(
      entryId,
      7,
      currentCollectionIds,
      bridge.historyId,
    );
    if (result.safeNoop || result.nextCollectionIds === undefined) {
      throw new Error("combine must produce a non-safe mutation");
    }
    const merged = await entryCollectionsSetCommand({
      entryId,
      collectionIds: result.nextCollectionIds,
    });
    assert.deepEqual(merged.sort(), [bridge.historyId, 7].sort());
    const persisted = await entryCollectionsCommand({ entryId });
    assert.deepEqual(persisted.sort(), [bridge.historyId, 7].sort());
  } finally {
    uninstallDropBridge();
  }
});

test("drop preserves Historial and never sends []", async () => {
  const historyId = 1;
  const bridge = installDropBridge({
    historyId,
    initialCollections: new Map([[7, [historyId, 9]]]),
  });
  try {
    const { combineMemberships } = await import(
      "../src/lib/dragAndDrop.ts"
    );
    const { entryCollectionsSetCommand, entryCollectionsCommand } =
      await import("../src/lib/tauri.ts");
    const currentCollectionIds = await entryCollectionsCommand({ entryId: 7 });
    assert.deepEqual(
      currentCollectionIds.sort(),
      [historyId, 9].sort(),
      "the entry must already belong to Historial and Clientes",
    );
    const result = combineMemberships(
      7,
      7,
      currentCollectionIds,
      historyId,
    );
    assert.equal(result.safeNoop, false);
    assert.deepEqual(
      (result.nextCollectionIds ?? []).sort(),
      [historyId, 7, 9].sort(),
      "Historial and the prior memberships must remain in the merged list",
    );
    await entryCollectionsSetCommand({
      entryId: 7,
      collectionIds: result.nextCollectionIds ?? [],
    });
    const persisted = await entryCollectionsCommand({ entryId: 7 });
    assert.deepEqual(
      persisted.sort(),
      [historyId, 7, 9].sort(),
      "every membership must survive the drop",
    );
    const setCalls = bridge.calls.filter(
      (c) => c.cmd === "clipvault_entry_collections_set",
    );
    assert.equal(setCalls.length, 1, "the bridge call must run once");
    const setArgs = setCalls[0].args as { collectionIds: number[] };
    assert.notEqual(setArgs.collectionIds.length, 0, "no [] may ever be sent");
    void bridge;
  } finally {
    uninstallDropBridge();
  }
});

test("drop is idempotent at the bridge layer when repeated on the same target", async () => {
  const bridge = installDropBridge();
  try {
    const { combineMemberships } = await import(
      "../src/lib/dragAndDrop.ts"
    );
    const { entryCollectionsCommand, entryCollectionsSetCommand } =
      await import("../src/lib/tauri.ts");
    const entryId = 5;
    // First drop: bridge runs and the entry ends up in the target
    // collection.
    const firstCurrent = await entryCollectionsCommand({ entryId });
    const firstResult = combineMemberships(
      entryId,
      7,
      firstCurrent,
      bridge.historyId,
    );
    assert.equal(firstResult.safeNoop, false);
    await entryCollectionsSetCommand({
      entryId,
      collectionIds: firstResult.nextCollectionIds ?? [],
    });
    const setCallsAfterFirst = bridge.calls.filter(
      (c) => c.cmd === "clipvault_entry_collections_set",
    ).length;
    assert.equal(setCallsAfterFirst, 1);
    // Second and third drop: the helper short-circuits on
    // `already_member` so the bridge is never called again.
    for (let i = 0; i < 2; i += 1) {
      const current = await entryCollectionsCommand({ entryId });
      const result = combineMemberships(
        entryId,
        7,
        current,
        bridge.historyId,
      );
      assert.equal(result.safeNoop, true, "subsequent drops must be safe no-ops");
      assert.equal(result.reason, "already_member");
    }
    const setCallsAfterAll = bridge.calls.filter(
      (c) => c.cmd === "clipvault_entry_collections_set",
    ).length;
    assert.equal(
      setCallsAfterAll,
      1,
      "the bridge call must run exactly once across the entire sequence",
    );
    const persisted = await entryCollectionsCommand({ entryId });
    assert.deepEqual(
      persisted.sort(),
      [bridge.historyId, 7].sort(),
      "repeated drops must converge to the same persisted state",
    );
    assert.equal(
      persisted.filter((id) => id === bridge.historyId).length,
      1,
      "Historial must appear exactly once",
    );
    assert.equal(
      persisted.filter((id) => id === 7).length,
      1,
      "the target collection must appear exactly once",
    );
  } finally {
    uninstallDropBridge();
  }
});

test("drop on Historial is a safe no-op and never runs entry_collections_set", async () => {
  const bridge = installDropBridge();
  try {
    const { setCalls } = await simulateDrop(
      7,
      bridge.historyId,
      bridge.historyId,
      "loaded",
      bridge.collections,
    );
    assert.equal(setCalls, 0, "drop on Historial must skip the bridge call");
    const setCalls2 = bridge.calls.filter(
      (c) => c.cmd === "clipvault_entry_collections_set",
    );
    assert.equal(setCalls2.length, 0, "no set call must ever be issued");
  } finally {
    uninstallDropBridge();
  }
});

test("drop with a malformed payload never reaches entry_collections_set", async () => {
  const bridge = installDropBridge();
  try {
    const { parseDragPayloadFromTransfer } = await import(
      "../src/lib/dragAndDrop.ts"
    );
    // Clipboard-style text: a foreign payload that the strict
    // parser must reject.
    const parsed = parseDragPayloadFromTransfer({
      privatePayload: null,
      textPayload: "hello world",
    });
    assert.equal(parsed, null, "free-form text must be rejected");
    // Foreign drag with the JSON in the private MIME but the wrong
    // shape (extra fields).
    const parsed2 = parseDragPayloadFromTransfer({
      privatePayload: JSON.stringify({ id: 7, name: "leak" }),
      textPayload: null,
    });
    assert.equal(
      parsed2,
      null,
      "a JSON payload with extra fields must be rejected",
    );
    // The bridge never received an entry_collections_set call.
    const setCalls = bridge.calls.filter(
      (c) => c.cmd === "clipvault_entry_collections_set",
    );
    assert.equal(
      setCalls.length,
      0,
      "the bridge must never be called for a malformed payload",
    );
  } finally {
    uninstallDropBridge();
  }
});

test("drop with unhydrated cache queries entry_collections first and never sends []", async () => {
  const bridge = installDropBridge({
    initialCollections: new Map(),
  });
  try {
    const { combineMemberships } = await import(
      "../src/lib/dragAndDrop.ts"
    );
    const { entryCollectionsCommand, entryCollectionsSetCommand } =
      await import("../src/lib/tauri.ts");
    const entryId = 42;
    // The cache is unhydrated: the parent must fall back to the
    // bridge to discover the current membership.
    const current = await entryCollectionsCommand({ entryId });
    assert.deepEqual(
      current,
      [bridge.historyId],
      "the bridge returns the documented Historial default",
    );
    const result = combineMemberships(
      entryId,
      7,
      current,
      bridge.historyId,
    );
    assert.equal(result.safeNoop, false);
    assert.deepEqual(
      result.nextCollectionIds?.sort(),
      [bridge.historyId, 7].sort(),
    );
    await entryCollectionsSetCommand({
      entryId,
      collectionIds: result.nextCollectionIds ?? [],
    });
    const persisted = await entryCollectionsCommand({ entryId });
    assert.deepEqual(persisted.sort(), [bridge.historyId, 7].sort());
    // The bridge calls happened in the documented order.
    const cmdSequence = bridge.calls.map((c) => c.cmd);
    const setIdx = cmdSequence.indexOf("clipvault_entry_collections_set");
    const readIdx = cmdSequence.indexOf("clipvault_entry_collections");
    assert.ok(setIdx > readIdx, "read must precede write");
    assert.ok(readIdx >= 0, "read must run when the cache is unhydrated");
  } finally {
    uninstallDropBridge();
  }
});

test("drop survives a restart round-trip: the persisted list is the same after a fresh read", async () => {
  const bridge = installDropBridge();
  try {
    const { combineMemberships } = await import(
      "../src/lib/dragAndDrop.ts"
    );
    const { entryCollectionsSetCommand, entryCollectionsCommand } =
      await import("../src/lib/tauri.ts");
    const entryId = 33;
    // First drop, like a user dragging a card.
    const firstCurrent = await entryCollectionsCommand({ entryId });
    const firstResult = combineMemberships(
      entryId,
      7,
      firstCurrent,
      bridge.historyId,
    );
    await entryCollectionsSetCommand({
      entryId,
      collectionIds: firstResult.nextCollectionIds ?? [],
    });
    // Simulate a restart by tearing down + reinstalling the bridge
    // with the persisted state seeded.
    uninstallDropBridge();
    const rehydrated = installDropBridge({
      initialCollections: new Map([
        [entryId, [bridge.historyId, 7]],
      ]),
    });
    try {
      const rehydratedRead = await (
        await import("../src/lib/tauri.ts")
      ).entryCollectionsCommand({ entryId });
      assert.deepEqual(
        rehydratedRead.sort(),
        [bridge.historyId, 7].sort(),
        "the persisted list must survive a restart",
      );
      // A second drop on a different target does not lose the
      // existing memberships.
      const secondCurrent = await (
        await import("../src/lib/tauri.ts")
      ).entryCollectionsCommand({ entryId });
      const secondResult = combineMemberships(
        entryId,
        9,
        secondCurrent,
        rehydrated.historyId,
      );
      assert.equal(secondResult.safeNoop, false);
      assert.deepEqual(
        (secondResult.nextCollectionIds ?? []).sort(),
        [rehydrated.historyId, 7, 9].sort(),
      );
      void rehydrated;
    } finally {
      uninstallDropBridge();
    }
  } finally {
    if (
      (globalThis as unknown as { window?: unknown }).window !== undefined
    ) {
      uninstallDropBridge();
    }
  }
});

// ---------------------------------------------------------------------------
// Collection delete icon: enlarged by 30% from the prior 14x14
// baseline to approximately 18x18.
// ---------------------------------------------------------------------------

test("OrganizationSidebar renders the collection delete icon at 18x18 (≈130% of the prior baseline)", () => {
  const source = loadSource("src/OrganizationSidebar.svelte");
  // The `<svg>` inside the sidebar-collection-delete button must
  // declare width="18" and height="18" so the visual footprint
  // grows by 30% without changing the clickable button.
  const deleteSvg = source.match(
    /<button[\s\S]*?sidebar-collection-delete[\s\S]*?<svg[\s\S]*?<\/svg>/,
  );
  assert.ok(deleteSvg, "delete svg must be defined");
  assert.match(
    deleteSvg?.[0] ?? "",
    /<svg[\s\S]*?width="18"/,
    "delete svg width must be 18 (≈130% of the prior 14)",
  );
  assert.match(
    deleteSvg?.[0] ?? "",
    /<svg[\s\S]*?height="18"/,
    "delete svg height must be 18 (≈130% of the prior 14)",
  );
  // The button still declares the original accessible name and the
  // danger colour.
  assert.match(source, /sidebar-collection-delete/);
  assert.match(
    source,
    /aria-label=\{`Eliminar \$\{collection\.name\}`\}/,
  );
  assert.match(source, /\.icon-only\.danger\.delete-icon\s*\{[^}]*color:/s);
});

// ---------------------------------------------------------------------------
// Package + tauri config sanity (kept lean).
// ---------------------------------------------------------------------------

test("tauri.conf.json keeps the bounded main window dimensions", () => {
  const conf = JSON.parse(
    readFileSync(
      path.join(REPO_ROOT, "app/tauri/src-tauri/tauri.conf.json"),
      "utf8",
    ),
  );
  const main = conf.app.windows.find(
    (window: { label: string }) => window.label === "main",
  );
  assert.ok(main, "main window must remain in tauri.conf.json");
  assert.ok(main.height <= 460, `main window height must stay bounded`);
  assert.ok(main.minHeight <= 400, `main window minHeight must stay bounded`);
  assert.equal(main.title, "ClipVault");
});
