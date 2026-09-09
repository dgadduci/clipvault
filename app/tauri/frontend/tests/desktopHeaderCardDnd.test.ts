/**
 * Regression coverage for the drag-and-drop flow the
 * `desktop-header-card-dnd` change introduces.
 *
 * The contract being pinned:
 *
 *   - `buildDragPayload` produces a dual payload: the
 *     ClipVault-private JSON encoding for the private MIME
 *     `application/x.clipvault-entry-id` AND the strict
 *     `clipvault-entry:v1:<integer-id>` `text/plain` fallback. Both
 *     representations carry ONLY the entry id; the helper MUST NOT
 *     accept or emit clipboard content, snippets, hashes, asset
 *     references, file paths or image bytes.
 *   - `parseDragPayload` is the round-trip for the JSON encoding;
 *     `parseDragTextPayload` is the round-trip for the versioned
 *     `text/plain` fallback. Both reject malformed payloads so the
 *     sidebar can treat the drop as a no-op without inspecting
 *     free-form strings.
 *   - `combineMemberships` performs the additive combine the
 *     `entry_collections_set` bridge expects: the target id is
 *     appended to the existing set, the `Historial` membership is
 *     re-added when missing, and the helper refuses to send a
 *     destructive `[]` when hydration is missing.
 *   - The HistoryCard uses the internal pointer drag controller and the
 *     OrganizationSidebar wires the row as a drop target with the
 *     documented CSS hooks.
 *   - The pin control renders a local SVG and never a star glyph.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

import {
  CLIPVAULT_ENTRY_MIME,
  CLIPVAULT_ENTRY_TEXT_PREFIX,
  buildDragPayload,
  parseDragPayload,
  parseDragTextPayload,
  parseDragPayloadFromTransfer,
  combineMemberships,
  isDragPayload,
  acceptsDragOver,
  isDropTarget,
} from "../src/lib/dragAndDrop.ts";

const FRONTEND_ROOT = process.cwd();
const REPO_ROOT = path.resolve(FRONTEND_ROOT, "..", "..", "..");

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

function loadRepoSource(...segments: string[]): string {
  return readFileSync(path.join(REPO_ROOT, ...segments), "utf8");
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

// ---------------------------------------------------------------------------
// Payload privacy: only the entry id travels through the DataTransfer.
// ---------------------------------------------------------------------------

test("buildDragPayload serialises only the entry id on both representations", () => {
  const payloads = buildDragPayload(42);
  // The JSON shape behind the private MIME keeps a single
  // integer `id` key so a regression that wired extra fields
  // would surface here as a failed assertion.
  const privateParsed = JSON.parse(payloads.privatePayload);
  assert.deepEqual(privateParsed, { id: 42 });
  assert.equal(Object.keys(privateParsed).length, 1);
  // The `text/plain` fallback is the strict versioned form
  // documented by the spec: `clipvault-entry:v1:<integer-id>`.
  assert.equal(payloads.textPayload, `${CLIPVAULT_ENTRY_TEXT_PREFIX}42`);
});

test("buildDragPayload refuses non-integer or non-finite ids", () => {
  for (const invalid of [Number.NaN, Number.POSITIVE_INFINITY, 1.5, -0.5]) {
    assert.throws(
      () => buildDragPayload(invalid),
      /integer entry id/,
    );
  }
});

test("buildDragPayload never carries clipboard content, snippets, hashes, paths or bytes", () => {
  const sensitive = "secret-token-pasted-from-keyboard";
  const payloads = buildDragPayload(7);
  for (const forbidden of [
    "content",
    "snippet",
    "hash",
    "asset",
    "path",
    "bytes",
    sensitive,
  ]) {
    assert.equal(
      payloads.privatePayload.includes(forbidden),
      false,
      `private payload must not include "${forbidden}"`,
    );
    assert.equal(
      payloads.textPayload.includes(forbidden),
      false,
      `text payload must not include "${forbidden}"`,
    );
  }
});

test("parseDragPayload accepts the JSON round-trip and rejects foreign shapes", () => {
  assert.equal(parseDragPayload(buildDragPayload(99).privatePayload), 99);
  // Foreign shape: a clip from another app might emit JSON with
  // additional keys (e.g. `{ id, source }`); the helper rejects it
  // so the sidebar never reads a partial entry id.
  assert.equal(parseDragPayload(JSON.stringify({ id: 1, extra: true })), null);
  assert.equal(parseDragPayload(JSON.stringify({})), null);
  assert.equal(parseDragPayload(""), null);
  assert.equal(parseDragPayload(null), null);
  assert.equal(parseDragPayload(undefined), null);
  assert.equal(parseDragPayload("not json"), null);
  // `id` must be an integer; floats and strings are rejected.
  assert.equal(parseDragPayload(JSON.stringify({ id: 1.5 })), null);
  assert.equal(parseDragPayload(JSON.stringify({ id: "1" })), null);
});

test("parseDragTextPayload accepts the versioned text/plain round-trip and rejects everything else", () => {
  // Round-trip: the helper recognises the exact prefix and
  // returns the integer id.
  assert.equal(parseDragTextPayload(buildDragPayload(7).textPayload), 7);
  assert.equal(parseDragTextPayload(`${CLIPVAULT_ENTRY_TEXT_PREFIX}12345`), 12345);
  // Anything that does not match the exact prefix is rejected.
  for (const malformed of [
    "",
    null,
    undefined,
    "clipvault-entry:",
    "clipvault-entry:v2:7",
    "clipvault-entry:v1:",
    `${CLIPVAULT_ENTRY_TEXT_PREFIX}-1`,
    `${CLIPVAULT_ENTRY_TEXT_PREFIX}1.5`,
    `${CLIPVAULT_ENTRY_TEXT_PREFIX}+1`,
    `${CLIPVAULT_ENTRY_TEXT_PREFIX} 1`,
    `${CLIPVAULT_ENTRY_TEXT_PREFIX}01abc`,
    ` ${CLIPVAULT_ENTRY_TEXT_PREFIX}7`,
    `${CLIPVAULT_ENTRY_TEXT_PREFIX}7 `,
    `${CLIPVAULT_ENTRY_TEXT_PREFIX}7;preview=text`,
    `file:///${CLIPVAULT_ENTRY_TEXT_PREFIX}7`,
    JSON.stringify({ id: 7 }),
  ]) {
    assert.equal(
      parseDragTextPayload(malformed as string | null | undefined),
      null,
      `malformed payload must be rejected: ${JSON.stringify(malformed)}`,
    );
  }
});

test("parseDragPayloadFromTransfer accepts whichever representation the DataTransfer exposes", () => {
  const payloads = buildDragPayload(11);
  // Private MIME only.
  assert.equal(
    parseDragPayloadFromTransfer({
      privatePayload: payloads.privatePayload,
      textPayload: null,
    }),
    11,
  );
  // Plain fallback only (the WebKit/Tauri path).
  assert.equal(
    parseDragPayloadFromTransfer({
      privatePayload: null,
      textPayload: payloads.textPayload,
    }),
    11,
  );
  // Both: the helper prefers the private MIME.
  assert.equal(
    parseDragPayloadFromTransfer({
      privatePayload: payloads.privatePayload,
      textPayload: `${CLIPVAULT_ENTRY_TEXT_PREFIX}999`,
    }),
    11,
  );
  // Neither: foreign drop reduces to `null`.
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

// ---------------------------------------------------------------------------
// Membership combination: additive, idempotent, safe no-ops.
// ---------------------------------------------------------------------------

test("combineMemberships adds the target and keeps existing collections", () => {
  const result = combineMemberships(
    /* entryId */ 1,
    /* targetId */ 7,
    /* currentIds */ [3, 4],
    /* systemCollectionId */ 99,
  );
  assert.equal(result.safeNoop, false);
  assert.deepEqual(result.nextCollectionIds, [3, 4, 7, 99]);
  assert.equal(result.reason, "added");
});

test("combineMemberships never sends a destructive empty list", () => {
  // The unhydrated branch is the critical safety guard: a missing
  // current list MUST never produce an empty `[]` that would clobber
  // the backend associations.
  const result = combineMemberships(1, 7, null, 99);
  assert.equal(result.safeNoop, true);
  assert.equal(result.nextCollectionIds, undefined);
  assert.equal(result.reason, "missing_hydration");
});

test("combineMemberships treats duplicate drops as idempotent no-ops", () => {
  const result = combineMemberships(1, 7, [7, 99], 99);
  assert.equal(result.safeNoop, true);
  assert.equal(result.nextCollectionIds, undefined);
  assert.equal(result.reason, "already_member");
});

test("combineMemberships refuses to drop on the system Historial collection", () => {
  const result = combineMemberships(1, 99, [3, 99], 99);
  assert.equal(result.safeNoop, true);
  assert.equal(result.nextCollectionIds, undefined);
  assert.equal(result.reason, "system_collection");
});

test("combineMemberships rejects invalid entry or target ids without mutating", () => {
  for (const invalidEntry of [Number.NaN, Number.POSITIVE_INFINITY, 1.5]) {
    const result = combineMemberships(invalidEntry, 7, [1, 2], 99);
    assert.equal(result.safeNoop, true);
    assert.equal(result.nextCollectionIds, undefined);
  }
  for (const invalidTarget of [null, undefined, Number.NaN, 1.5]) {
    const result = combineMemberships(1, invalidTarget as number, [1, 2], 99);
    assert.equal(result.safeNoop, true);
    assert.equal(result.nextCollectionIds, undefined);
  }
});

test("combineMemberships appends Historial only when missing from current", () => {
  // The helper appends the system collection when it is missing so
  // an upstream mutation that lost Historial cannot slip through;
  // it never duplicates the existing Historial membership.
  const withoutHistorial = combineMemberships(1, 7, [3, 4], 99);
  assert.deepEqual(withoutHistorial.nextCollectionIds, [3, 4, 7, 99]);
  const withHistorial = combineMemberships(1, 7, [3, 99, 4], 99);
  assert.deepEqual(withHistorial.nextCollectionIds, [3, 99, 4, 7]);
});

// ---------------------------------------------------------------------------
// MIME-type guard: only the private token opts the sidebar in.
// ---------------------------------------------------------------------------

test("isDragPayload matches the ClipVault private MIME and the text/plain fallback", () => {
  assert.equal(isDragPayload([CLIPVAULT_ENTRY_MIME]), true);
  // WebKit/Tauri sometimes drops the private MIME and only exposes
  // the generic `text/plain` representation. The sidebar MUST keep
  // accepting the drag in that case.
  assert.equal(isDragPayload(["text/plain"]), true);
  assert.equal(isDragPayload(["Files"]), false);
  assert.equal(isDragPayload([]), false);
  assert.equal(isDragPayload(null), false);
  // `DOMStringList` compatibility: when the type is the live list
  // shape browsers expose, the helper still answers correctly for
  // both the private MIME and the `text/plain` fallback.
  const privateDomList = {
    contains(value: string): boolean {
      return value === CLIPVAULT_ENTRY_MIME;
    },
    length: 1,
  } as unknown as DOMStringList;
  assert.equal(isDragPayload(privateDomList), true);
  const textDomList = {
    contains(value: string): boolean {
      return value === "text/plain";
    },
    length: 1,
  } as unknown as DOMStringList;
  assert.equal(isDragPayload(textDomList), true);
});

test("acceptsDragOver returns true for the private MIME or the text/plain fallback", () => {
  assert.equal(
    acceptsDragOver({ dataTransfer: { types: [CLIPVAULT_ENTRY_MIME] } }),
    true,
  );
  // The critical WebKit/Tauri path: only the fallback is exposed.
  assert.equal(
    acceptsDragOver({ dataTransfer: { types: ["text/plain"] } }),
    true,
  );
  // Missing dataTransfer (the type was never written) must opt
  // out so the row never receives a phantom drop.
  assert.equal(
    acceptsDragOver({ dataTransfer: null }),
    false,
  );
  // A foreign drag with no recognised MIME is still rejected.
  assert.equal(
    acceptsDragOver({ dataTransfer: { types: ["Files"] } }),
    false,
  );
});

test("isDropTarget only accepts user collections", () => {
  assert.equal(isDropTarget({ kind: "user" }), true);
  assert.equal(isDropTarget({ kind: "system" }), false);
  assert.equal(isDropTarget(null), false);
  assert.equal(isDropTarget(undefined), false);
});

// ---------------------------------------------------------------------------
// Source-level invariants: HistoryCard participates in the internal pointer
// drag flow and keeps the legacy private-MIME helpers isolated as fallback
// compatibility code.
// ---------------------------------------------------------------------------

test("HistoryCard disables native HTML5 dragging in favor of the pointer bridge", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  // WebKit/Tauri can swallow the native drag lifecycle. The article must
  // not start a competing native drag while App.svelte owns the singleton
  // pointer bridge.
  assert.match(source, /<article[\s\S]*?draggable="false"/);
  assert.doesNotMatch(source, /on:dragstart=/);
  assert.doesNotMatch(source, /on:dragend=/);
});

test("App installs one pointer drag controller and cleans it on destroy", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  assert.match(source, /installPointerDragController\(document\)/);
  assert.match(source, /detachPointerDragController/);
});

test("pointer drag source ignores interactive card controls", () => {
  const source = stripComments(loadSource("src/lib/pointerDragAndDrop.ts"));
  assert.match(
    source,
    /INTERACTIVE_SELECTORS/,
    "the pointer bridge must inspect the original event target",
  );
  assert.match(
    source,
    /closestElement\(target, selector\)/,
    "interactive controls must not become drag sources",
  );
});

// ---------------------------------------------------------------------------
// Source-level invariants: OrganizationSidebar wires drop targets.
// ---------------------------------------------------------------------------

test("OrganizationSidebar installs a single delegated drop zone on the scrollable viewport", () => {
  const source = stripComments(loadSource("src/OrganizationSidebar.svelte"));
  // The handlers live on the scrollable <ul> (the viewport),
  // not on each row, so a regression that wires per-row
  // listeners surfaces here as a failed assertion. The viewport
  // must carry the documented identifier so the helper module
  // can resolve it without depending on the DOM position.
  assert.match(
    source,
    /on:dragenter=\{\(event\) => dropZoneHandlers\.onDragEnter/,
    "dragenter must delegate to the shared drop zone",
  );
  assert.match(
    source,
    /on:dragover=\{\(event\) => dropZoneHandlers\.onDragOver/,
    "dragover must delegate to the shared drop zone",
  );
  assert.match(
    source,
    /on:dragleave=\{\(event\) => dropZoneHandlers\.onDragLeave/,
    "dragleave must delegate to the shared drop zone",
  );
  assert.match(
    source,
    /on:drop=\{\(event\) => dropZoneHandlers\.onDrop/,
    "drop must delegate to the shared drop zone",
  );
  assert.match(
    source,
    /data-collections-drop-viewport/,
    "viewport must carry the data-collections-drop-viewport attribute",
  );
  // The rows themselves must carry data-drop-target and a
  // numeric data-collection-id so the delegated handler can
  // resolve the target without depending on the Svelte render
  // output.
  assert.match(source, /data-drop-target=/);
  // The highlight hook still lives on a CSS class toggled per
  // row so the visual feedback continues to surface even though
  // the events are delegated.
  assert.match(source, /class:drag-over=\{dragOverCollectionId === collection\.id\}/);
  // The factory that builds the handlers must live in the
  // dedicated helper module; the sidebar MUST NOT duplicate the
  // drop zone logic inline.
  const helperSource = stripComments(
    loadSource("src/lib/collectionDropZone.ts"),
  );
  assert.match(
    helperSource,
    /export function createCollectionDropZoneHandlers/,
    "the helper module must export the drop zone factory",
  );
});

test("OrganizationSidebar installs a global dragend listener", () => {
  const source = stripComments(loadSource("src/OrganizationSidebar.svelte"));
  assert.match(source, /document\.addEventListener\("dragend", onWindowDragEnd\)/);
  assert.match(source, /document\.removeEventListener\("dragend", onWindowDragEnd\)/);
});

test("OrganizationSidebar rejects Historial as a drop target", () => {
  const sidebarSource = stripComments(loadSource("src/OrganizationSidebar.svelte"));
  // `isDropTarget` is the single source of truth: only user
  // collections are wired as drop targets. The CSS class
  // `drop-target` must mirror the same predicate so the visual
  // feedback never appears on Historial.
  assert.match(
    sidebarSource,
    /class:drop-target=\{isDropTarget\(collection\)\}/,
  );
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

test("OrganizationSidebar dispatches card-drop with the entry id and the collection id", () => {
  // The sidebar forwards the drop to the parent through
  // `dispatch("card-drop", …)`. The exact shape is owned by the
  // sidebar so the source-level check stays in the sidebar
  // itself; the helper module only forwards the resolved ids.
  const source = stripComments(loadSource("src/OrganizationSidebar.svelte"));
  assert.match(
    source,
    /dispatch\("card-drop",\s*\{\s*entryId,\s*collectionId\s*\}\)/,
  );
  // The helper module must forward the resolved entry id and
  // collection id through `onCardDrop(entryId, hit.collection.id)`.
  const helperSource = stripComments(
    loadSource("src/lib/collectionDropZone.ts"),
  );
  assert.match(
    helperSource,
    /options\.onCardDrop\(\s*entryId,\s*hit\.collection\.id\s*\)/,
  );
});

// ---------------------------------------------------------------------------
// App.svelte wires the drop into the additive combine helper.
// ---------------------------------------------------------------------------

test("App.svelte routes card-drop through combineMemberships + entryCollectionsSetCommand", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  // The drop handler must dispatch the event and use the combine
  // helper so the operation stays additive and idempotent.
  assert.match(source, /on:card-drop=\{\(e\) => handleCardDrop\(e\)\}/);
  assert.match(source, /function handleCardDrop/);
  assert.match(source, /combineMemberships\(\s*entryId,\s*collectionId/);
  assert.match(
    source,
    /entryCollectionsSetCommand\(\{\s*entryId,\s*collectionIds: result\.nextCollectionIds/,
  );
  // The handler must refuse to send a destructive empty list: the
  // safe-noop branch is the only path that skips the bridge call.
  // The branch may also clear the in-flight tracker before
  // `return` so a future drop on the same target can still land.
  assert.match(
    source,
    /if \(result\.safeNoop \|\| result\.nextCollectionIds === undefined\)[\s\S]*?return;/,
  );
  // The flow must deduplicate concurrent drops on the same
  // (entry, collection) pair so the bridge call never fires twice
  // for the same drag.
  assert.match(source, /dropInFlight\.has\(\s*dropKey\s*\)/);
});

test("App.svelte::handleCardDrop never logs clipboard content or asset paths", () => {
  // Privacy regression: an error path inside the drop handler MUST
  // not echo the payload, the content hash, the source-app id or
  // any other clipboard-derived value. The test reads the source
  // and asserts none of those substrings appear in the handler
  // body, alongside the existing `organizationError` surface.
  const source = stripComments(loadSource("src/App.svelte"));
  const handlerMatch = source.match(/function handleCardDrop[\s\S]*?\n  \}/);
  assert.ok(handlerMatch);
  const body = handlerMatch?.[0] ?? "";
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

// ---------------------------------------------------------------------------
// Pin control: local SVG, no star glyph, accessible labels preserved.
// ---------------------------------------------------------------------------

test("HistoryCard pin control never renders a star glyph or emoji", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  // The card must not use ★ / ☆ / any star character.
  assert.equal(source.includes("★"), false);
  assert.equal(source.includes("☆"), false);
  assert.equal(source.includes("⭐"), false);
  // The pin SVG lives in a local SVG block, not in a string.
  assert.match(source, /<svg[\s\S]*?history-card-pin-filled/);
  assert.match(source, /<svg[\s\S]*?history-card-pin-outline/);
});

test("HistoryCard pin control keeps aria-pressed, the Anclar/Desanclar labels and the busy hook", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  // `aria-pressed` and the Spanish labels are the persistent
  // contract the rest of the spec relies on. A regression that
  // drops them would break every keyboard / screen reader user.
  assert.match(source, /aria-pressed=\{entry\.is_pinned\}/);
  assert.match(source, /title=\{entry\.is_pinned \? "Desanclar" : "Anclar"\}/);
  assert.match(
    source,
    /aria-label=\{entry\.is_pinned \? `Desanclar entrada \$\{displayTitle\}` : `Anclar entrada \$\{displayTitle\}`\}/,
  );
  // The data-testid hook survives for the regression tests.
  assert.match(source, /data-testid="history-card-pin"/);
  // The pin/unpin dispatch still flows through the parent handler.
  assert.match(source, /on:click=\{handlePinClick\}/);
});

test("HistoryCard pin control never paints an emoji or external icon resource", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  // Banned shapes: a remote `http(s)://` URL, a `url(...)` CSS
  // function pointing at a remote, or a `data:` image with content
  // the user copied to the clipboard.
  assert.equal(/src=["']https?:/.test(source), false);
  assert.equal(/href=["']https?:/.test(source), false);
  assert.equal(/background-image: url\(["']?http/.test(source), false);
});

// ---------------------------------------------------------------------------
// Layout: the redundant application-title line is gone, the CSS keeps
// the desktop compact, and the Tauri config reflects the documented
// minimum heights.
// ---------------------------------------------------------------------------

test("App.svelte no longer renders the redundant title/subtitle header", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  // The `<h1>ClipVault</h1>` plus the localised subtitle lived in a
  // `<header class="desktop-header">` block; every variant must be
  // gone from the production source. The native window title is
  // still bound to "ClipVault" via `tauri.conf.json` so the OS task
  // switcher / menu bar keep the brand.
  assert.equal(source.includes('<header class="desktop-header">'), false);
  assert.equal(source.includes("Historial local · búsqueda · pegado rápido"), false);
  assert.equal(source.includes("<h1>ClipVault</h1>"), false);
  assert.equal(/class="subtitle"/.test(source), false);
});

test("App.svelte keeps the error, loading, search and accessibility states", () => {
  // The header removal must not have swept the rest of the document.
  // Every state the original `<main>` exposed before the change
  // remains mounted and accessible.
  const source = stripComments(loadSource("src/App.svelte"));
  assert.match(source, /role="alert"/);
  assert.match(source, /data-testid="organization-error"/);
  assert.match(source, /data-testid="search-status"/);
  assert.match(source, /aria-live="polite"/);
  // The accessibility helpers the spec mandates (e.g. `visually-hidden`)
  // survive.
  assert.match(source, /\.visually-hidden\s*\{/);
});

test("tauri.conf.json reflects the bounded desktop height", () => {
  // Pin the conf defaults so a future contributor that resizes the
  // window without updating the conf breaks the test. The
  // `desktop-shell-layout` companion change uses the same pattern.
  const source = loadRepoSource("app/tauri/src-tauri/tauri.conf.json");
  const parsed = JSON.parse(source);
  const main = parsed.app.windows.find(
    (window: { label: string }) => window.label === "main",
  );
  assert.ok(main, "main window must remain in tauri.conf.json");
  assert.ok(
    main.height <= 460,
    `main window height must stay bounded (got ${main.height})`,
  );
  assert.ok(
    main.minHeight <= 400,
    `main window minHeight must stay bounded (got ${main.minHeight})`,
  );
  assert.equal(main.title, "ClipVault", "the OS taskbar / Dock must still see the title");
});

test("App.svelte keeps the layout compact: no large empty band beneath the rail", () => {
  // The padding the body of `<main>` carries must not reintroduce a
  // large bottom band beneath the rail.
  const source = stripComments(loadSource("src/App.svelte"));
  assert.match(source, /padding:\s*1rem 1\.25rem 1rem/);
  // The two-column layout grid still uses the `minmax(0, 1fr)` so
  // a tall right column does not push the window.
  assert.match(source, /minmax\(0, 1fr\)/);
});

test("OrganizationSidebar stretches to the workspace height and keeps its scroller", () => {
  // The desktop-toolbar-layout change replaces the previous
  // fixed-height contract with a stretch-based one: the desktop
  // grid now uses `align-items: stretch` and the sidebar grows
  // through `height: 100%` so it matches the right column
  // (toolbar + status + rail) without a second fixed-height
  // token. The collection list keeps its own vertical scroller
  // so adding collections cannot grow the desktop.
  const source = loadSource("src/OrganizationSidebar.svelte");
  assert.equal(
    /\.collection-list\s*\{[^}]*overflow-y:\s*auto/.test(source),
    true,
    "the collection list must own a vertical scroller",
  );
  assert.equal(
    /\.sidebar\s*\{[^}]*max-height:\s*var\(\s*--cv-card-rail-height/.test(
      source,
    ),
    false,
    "the panel does not need a max-height separate from its stretch",
  );
  assert.match(
    source,
    /\.sidebar\s*\{[^}]*height:\s*100%/,
    "the panel must stretch to the workspace height",
  );
  assert.match(
    source,
    /\.sidebar\s*\{[^}]*min-height:\s*0/,
    "the panel must allow shrinking below its intrinsic content",
  );
});

test("App.svelte does not render the legacy bottom drop indicator", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  assert.doesNotMatch(source, /CardDropText/);
  assert.doesNotMatch(source, /card-drop-text/);
  assert.doesNotMatch(source, /drop-feedback/);
});

// ---------------------------------------------------------------------------
// Tauri / setup: the main window startup layout is one-shot, centred
// horizontally and pinned to the top of the work area.
// ---------------------------------------------------------------------------

test("main.rs::resize_main_window_to_monitor runs once during setup", () => {
  // The helper must NOT be wired to a `ResizeObserver` or any
  // event listener: it runs inside the `tauri::Builder::setup`
  // callback exactly once. A regression that subscribed to the
  // window's resize event would clobber a user move/resize, which
  // is what the spec forbids.
  const source = loadRepoSource("app/tauri/src-tauri/src/main.rs");
  // The setup callback is the only call site.
  const setupMatch = source.match(/tauri::Builder::default\(\)\s*\.setup\(\|app\| \{([\s\S]*?)\}\)/);
  assert.ok(setupMatch, "setup callback must remain");
  const setupBody = setupMatch?.[1] ?? "";
  assert.match(
    setupBody,
    /resize_main_window_to_monitor\(app\)/,
    "setup must invoke the helper exactly once",
  );
  // The helper itself must not subscribe to resize/move events.
  const helperMatch = source.match(/fn resize_main_window_to_monitor[\s\S]*?\n\}/);
  assert.ok(helperMatch);
  const helperBody = helperMatch?.[0] ?? "";
  for (const forbidden of [
    "ResizeObserver",
    "on_window_event",
    "add_event_listener",
    "Resized",
    "Moved",
  ]) {
    assert.equal(
      helperBody.includes(forbidden),
      false,
      `the startup helper must not subscribe to "${forbidden}"`,
    );
  }
});

test("main.rs::resize_main_window_to_monitor never touches the quick-paste window", () => {
  // The helper only manipulates the `main` window label. Touching
  // `quick-paste` would clobber the transient always-on-top
  // surface the spec demands stays independent.
  const source = loadRepoSource("app/tauri/src-tauri/src/main.rs");
  const helperMatch = source.match(/fn resize_main_window_to_monitor[\s\S]*?\n\}/);
  assert.ok(helperMatch);
  const helperBody = helperMatch?.[0] ?? "";
  assert.match(helperBody, /get_webview_window\("main"\)/);
  assert.equal(
    /get_webview_window\("quick-paste"\)/.test(helperBody),
    false,
    "the helper must never touch the quick-paste window",
  );
});

test("main_window_layout pure helper centres horizontally and pins to the top", () => {
  // Source-level contract: the documented centring math lives in
  // `compute_main_window_layout`. The setup callback consumes the
  // helper's output, never duplicates the math.
  const layout = loadRepoSource(
    "app/tauri/src-tauri/src/main_window_layout.rs",
  );
  assert.match(
    layout,
    /pub fn compute_main_window_layout\(\s*work_area: \(f64, f64, f64, f64\),\s*scale_factor: f64/,
  );
  // The centring formula reads `(work_logical_width - logical_width) / 2.0`,
  // which is the documented horizontal centre.
  assert.match(
    layout,
    /\(work_logical_width - logical_width\) \/ 2\.0/,
  );
  // The top edge equals the work-area origin (no vertical offset).
  assert.match(layout, /let logical_y = work_y \/ scale;/);
});
