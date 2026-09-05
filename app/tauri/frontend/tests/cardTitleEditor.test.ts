/**
 * Regression coverage for the editable card title flow
 * (`card-title-editing-regression`).
 *
 * The contract being pinned:
 *
 *   - `HistoryCard` exposes `Editar título` as a stable menu item
 *     that opens the same inline editor the dblclick and Enter/F2
 *     paths open. The menu closes exactly once when the entry point
 *     is fired.
 *   - The dblclick handler is wired on the title element and calls
 *     the shared `startEditTitle` function. The same function is
 *     reachable from the title's keyboard activation (Enter / F2)
 *     and from the menu item.
 *   - The inline editor exposes an `<input>`, a confirm icon and a
 *     cancel icon with stable `data-testid` hooks; `Escape` cancels
 *     the editor without persisting; `Enter` confirms.
 *   - The title editor only forwards through `setEntryTitleCommand`
 *     (`clipvault_set_entry_title`). No alternative command or
 *     SQLite write path exists in the source.
 *   - The title editor must NEVER leak content, content_hash,
 *     asset_ref, mime_type, source_app or the asset path through
 *     logs, errors or attributes.
 *   - `Restaurar título` continues to forward `title: null` through
 *     the same command so the backend's restore-default path
 *     remains the single authority.
 *
 * The source-level checks read `HistoryCard.svelte` directly because
 * the test harness does not mount Svelte components; the
 * drag-and-drop companion tests live in
 * `tests/pointerDragAndDrop.test.ts` and exercise the singleton
 * pointer controller with the polyfill DOM.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const FRONTEND_ROOT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);

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

const cardSource = stripComments(loadSource("src/HistoryCard.svelte"));

// ---------------------------------------------------------------------------
// Menu: every entry point routes through startEditTitle.
// ---------------------------------------------------------------------------

test("card menu exposes the documented Editar título entry as the first title action", () => {
  // The menu item must precede the legacy Restaurar título entry so
  // the regression that hid Editar título from the menu stays pinned
  // here. Both actions share the same data-testid prefix.
  const menuOpen = cardSource.indexOf(
    "data-testid=\"history-card-menu\"",
  );
  const editIndex = cardSource.indexOf('data-testid="history-card-edit-title"');
  const restoreIndex = cardSource.indexOf(
    'data-testid="history-card-restore-title"',
  );
  assert.notEqual(menuOpen, -1, "the menu must render in HistoryCard.svelte");
  assert.notEqual(editIndex, -1, "Editar título menu item must exist");
  assert.notEqual(restoreIndex, -1, "Restaurar título menu item must exist");
  assert.ok(
    editIndex > menuOpen,
    "Editar título must live inside the menu block",
  );
  assert.ok(
    editIndex < restoreIndex,
    "Editar título must be listed before Restaurar título",
  );
  // The label and the role must follow the menu contract.
  assert.match(cardSource, /Editar título/);
  assert.match(cardSource, /role="menuitem"/);
  assert.match(cardSource, /on:click=\{\(\) => startEditTitle\(\)\}/);
});

test("card menu wires Editar título through the shared startEditTitle entry point", () => {
  // The single source-level function the dblclick, keyboard and
  // menu entry points call. A regression that wires the menu
  // through a parallel state machine would silently fork the
  // editor and the persistence path.
  assert.match(
    cardSource,
    /function startEditTitle\([\s\S]*?menuOpen = false;[\s\S]*?dispatch\("menu-toggle"/,
    "startEditTitle must close the menu exactly once",
  );
  // The dblclick handler on the title element must call
  // startEditTitle without any intermediate state.
  assert.match(
    cardSource,
    /on:dblclick=\{\(\) => startEditTitle\(\)\}/,
    "the title element must call startEditTitle on dblclick",
  );
  // The keyboard activation path on the title container must call
  // startEditTitle for Enter and F2.
  const keydownStart = cardSource.indexOf(
    "function onTitleContainerKeydown",
  );
  assert.notEqual(
    keydownStart,
    -1,
    "the keyboard activation handler must remain in HistoryCard.svelte",
  );
  // The body is small (a single if + return); a fixed slice is
  // enough and avoids the brittle non-greedy closing brace.
  const keydownSlice = cardSource.slice(keydownStart, keydownStart + 400);
  assert.match(
    keydownSlice,
    /F2[\s\S]*?Enter[\s\S]*?startEditTitle/,
    "the keyboard activation must accept both F2 and Enter and call startEditTitle",
  );
});

test("card menu wires Restaurar título through setEntryTitleCommand with title null", () => {
  // The restore-default action continues to send `title: null`
  // through the documented bridge. The card must not write
  // directly to SQLite, nor call any parallel command.
  const restoreStart = cardSource.indexOf(
    "async function restoreDefaultTitle",
  );
  assert.notEqual(
    restoreStart,
    -1,
    "restoreDefaultTitle must remain in HistoryCard.svelte",
  );
  // The body is small enough to inspect via a fixed slice without
  // tripping on inner braces.
  const restoreSlice = cardSource.slice(restoreStart, restoreStart + 600);
  assert.match(
    restoreSlice,
    /setEntryTitleCommand\(\{[\s\S]*?id: entry\.id,[\s\S]*?title: null[\s\S]*?\}\)/,
    "restoreDefaultTitle must forward title: null through setEntryTitleCommand",
  );
});

test("card editor saves through the documented setEntryTitleCommand and never writes directly to SQLite", () => {
  // The single save path the card uses. The test pins the command
  // name and the response discriminator so a regression that
  // wired the editor to a parallel SQLite call or to a different
  // command surfaces here as a failed assertion.
  const confirmStart = cardSource.indexOf("async function confirmEditTitle");
  assert.notEqual(
    confirmStart,
    -1,
    "confirmEditTitle must remain in HistoryCard.svelte",
  );
  const confirmSlice = cardSource.slice(confirmStart, confirmStart + 700);
  assert.match(
    confirmSlice,
    /setEntryTitleCommand\(\{[\s\S]*?id: entry\.id,[\s\S]*?title: result\.title[\s\S]*?\}\)/,
    "confirmEditTitle must forward the validated title through setEntryTitleCommand",
  );
  assert.match(
    confirmSlice,
    /response\.kind === "updated"/,
    "the editor must branch on the discriminated response kind",
  );
  assert.equal(
    confirmSlice.includes("invoke("),
    false,
    "the editor must never invoke a raw Tauri command",
  );
  assert.equal(
    confirmSlice.includes("sqlite"),
    false,
    "the editor must never reference SQLite directly",
  );
});

test("card editor keyboard handlers route Enter to confirm and Escape to cancel", () => {
  const handlerStart = cardSource.indexOf("function onTitleKeydown");
  assert.notEqual(
    handlerStart,
    -1,
    "onTitleKeydown must remain in HistoryCard.svelte",
  );
  const handlerSlice = cardSource.slice(handlerStart, handlerStart + 400);
  assert.match(
    handlerSlice,
    /Enter[\s\S]*?confirmEditTitle/,
    "Enter must trigger confirmEditTitle",
  );
  assert.match(
    handlerSlice,
    /Escape[\s\S]*?cancelEditTitle/,
    "Escape must trigger cancelEditTitle",
  );
});

test("card editor exposes a stable data-testid hook for confirm and cancel", () => {
  // The integration tests and the manual QA flow rely on these
  // hooks. A regression that drops them would break the test
  // pipeline and remove the only way to drive the confirm /
  // cancel buttons from automated checks.
  assert.match(
    cardSource,
    /data-testid="history-card-title-confirm"/,
  );
  assert.match(
    cardSource,
    /data-testid="history-card-title-cancel"/,
  );
  assert.match(
    cardSource,
    /maxlength="120"/,
    "the editor input must keep the documented maxlength cap",
  );
});

test("card editor never leaks clipboard content, hashes, asset references or paths", () => {
  // The editor only mutates `title`. No clipboard payload, hash,
  // asset reference, mime type, source-app id or filesystem path
  // is forwarded to the backend, logged or stored on the editor
  // itself. The source-level check is intentionally strict: any
  // future addition that wires one of those fields through the
  // editor must be justified through a new OpenSpec change.
  const editorBlocks = [
    /function startEditTitle\([\s\S]*?\n  \}/,
    /async function confirmEditTitle\([\s\S]*?\n  \}/,
    /async function restoreDefaultTitle\([\s\S]*?\n  \}/,
    /function cancelEditTitle\([\s\S]*?\n  \}/,
  ];
  for (const block of editorBlocks) {
    const match = cardSource.match(block);
    assert.ok(match, `editor block must remain: ${String(block)}`);
    const body = match?.[0] ?? "";
    for (const forbidden of [
      "content",
      "snippet",
      "content_hash",
      "hash",
      "asset_ref",
      "asset",
      "mime_type",
      "path",
      "bytes",
      "source_app",
    ]) {
      assert.equal(
        body.includes(forbidden),
        false,
        `editor block must not reference "${forbidden}"`,
      );
    }
  }
});

test("card editor disables the menu actions while saving so a save fires once per interaction", () => {
  // The `titleBusy` guard prevents a second save from racing the
  // first one when the user presses Enter twice or clicks the
  // confirm icon while the bridge round-trip is in flight.
  const editButton = cardSource.match(
    /data-testid="history-card-edit-title"[\s\S]*?<\/button>/,
  );
  assert.ok(editButton, "Editar título menu item must exist");
  assert.match(
    editButton?.[0] ?? "",
    /disabled=\{titleBusy\}/,
    "Editar título menu item must respect titleBusy",
  );
  const confirmButton = cardSource.match(
    /data-testid="history-card-title-confirm"[\s\S]*?<\/button>/,
  );
  assert.ok(confirmButton, "history-card-title-confirm button must exist");
  assert.match(
    confirmButton?.[0] ?? "",
    /disabled=\{titleBusy\}/,
    "the confirm icon must respect titleBusy",
  );
});

test("card editor keeps the legacy Restaurar título label and behaviour intact", () => {
  // The legacy "Restaurar título" action must keep its Spanish
  // label so a regression that rewrites it to English surfaces
  // here as a failed assertion.
  assert.match(cardSource, /Restaurar título/);
  const restoreBlock = cardSource.match(
    /data-testid="history-card-restore-title"[\s\S]*?<\/button>/,
  );
  assert.ok(restoreBlock, "Restaurar título menu item must exist");
  assert.match(
    restoreBlock?.[0] ?? "",
    /on:click=\{\(\) => void restoreDefaultTitle\(\)\}/,
  );
});

// ---------------------------------------------------------------------------
// Title surface: the dblclick / single-click / drag arbitration.
// ---------------------------------------------------------------------------

test("title surface accepts dblclick and Enter / F2 keyboard activation", () => {
  // The title element must remain keyboard-focusable and accept
  // double click. A regression that drops the dblclick handler or
  // removes the role="button" affordance would break the primary
  // entry point the spec mandates.
  assert.match(
    cardSource,
    /class="title"[\s\S]*?role="button"[\s\S]*?on:dblclick=\{\(\) => startEditTitle\(\)\}/,
    "the title must keep role=button, keyboard focus and the dblclick handler",
  );
  assert.match(
    cardSource,
    /tabindex=\{editingTitle \? -1 : 0\}/,
    "the title must be keyboard-focusable while not editing",
  );
});

test("title surface does not register a single-click handler that opens the editor", () => {
  // A single click on the title MUST NOT open the editor; the
  // entry points are dblclick, Enter / F2 and the menu item only.
  // The check guards against a regression that wires
  // `on:click={() => startEditTitle()}` on the title itself.
  const titleBlock = cardSource.match(
    /class="title"[\s\S]*?<\/div>/,
  );
  assert.ok(titleBlock, "title block must remain in HistoryCard.svelte");
  assert.doesNotMatch(
    titleBlock?.[0] ?? "",
    /on:click=\{\(\) => startEditTitle\(\)\}/,
    "the title must not call startEditTitle on a single click",
  );
});

test("title editor controls are not drag sources", () => {
  // The editor input, confirm icon and cancel icon MUST NOT be
  // drag sources. The polyfill's `INTERACTIVE_SELECTORS` list pins
  // the rule at the controller level; this assertion keeps the
  // card in sync by guaranteeing the editor controls live under
  // an `<input>` or `<button>` element (the documented
  // interactive selectors).
  const editorBlock = cardSource.match(
    /\{#if editingTitle\}[\s\S]*?\{\:else\}/,
  );
  assert.ok(editorBlock, "editor block must remain in HistoryCard.svelte");
  const body = editorBlock?.[0] ?? "";
  assert.match(body, /<input[\s\S]*?class="title-input"/);
  assert.match(body, /<button[\s\S]*?class="icon-inline title-confirm"/);
  assert.match(body, /<button[\s\S]*?class="icon-inline title-cancel"/);
});
