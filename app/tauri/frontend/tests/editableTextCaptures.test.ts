/**
 * Source-level regression coverage for the
 * `editable-text-captures` capability.
 *
 * The tests pin the contract the rail relies on:
 *
 *   - `HistoryCard.svelte` exposes a stable `Editar captura`
 *     menu item gated on `isEditableTextEntry(entry)`;
 *   - the menu item is the only place the editor opens from;
 *     drag/mousedown, click, F2 / Enter / dblclick, the title
 *     editor and the preview shortcut MUST NOT open it;
 *   - the modal is rendered through `EntryTextEditorModal.svelte`
 *     reusing `Modal.svelte`; Escape, backdrop and the close
 *     button dismiss it without persisting;
 *   - the textarea is a native `<textarea>` (no CodeMirror,
 *     Monaco, Tiptap or contenteditable wrappers);
 *   - the modal is guarded by a single `saving` flag that
 *     disables Guardar / Cancelar / Restablecer / Escape /
 *     backdrop / close while the IPC round-trip is in flight;
 *   - the bridge is `updateTextEntryCommand` →
 *     `clipvault_update_text_entry` with `entryId` / `content`;
 *   - the modal never forwards the draft to logs, errors or
 *     attribute values;
 *   - the response and the metadata-only
 *     `clipvault://history-updated` event stay payload-free;
 *   - the card surface keeps `data-testid="history-card"`,
 *     `data-entry-id`, `draggable="false"` and its pin / menu /
 *     title actions intact;
 *   - the `pointerDragAndDrop` singleton stays a singleton with
 *     pointer capture / fallback mouse listeners;
 *   - the close lifecycle routes through a `close` dispatcher so
 *     `HistoryCard.textEditorOpen` is the single source of truth
 *     and the same entry can be opened, closed and reopened in
 *     the same session without re-mounting the card.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const FRONTEND_ROOT = process.cwd();

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
}

const cardSource = stripComments(loadSource("src/HistoryCard.svelte"));
const modalSource = stripComments(
  loadSource("src/EntryTextEditorModal.svelte"),
);
const typesSource = stripComments(loadSource("src/types.ts"));
const tauriBridgeSource = stripComments(loadSource("src/lib/tauri.ts"));

// ---------------------------------------------------------------------------
// Menu surface: the action is exposed, gated, and reachable only from the
// ellipsis menu of an eligible card.
// ---------------------------------------------------------------------------

test("card menu exposes the Editar captura entry point", () => {
  const menuOpen = cardSource.indexOf('data-testid="history-card-menu"');
  const editIndex = cardSource.indexOf('data-testid="history-card-edit-text"');
  assert.notEqual(menuOpen, -1, "the card menu must render in HistoryCard.svelte");
  assert.notEqual(editIndex, -1, "Editar captura menu item must exist");
  assert.ok(
    editIndex > menuOpen,
    "Editar captura must live inside the menu block",
  );
});

test("Editar captura is gated on the eligibility predicate", () => {
  // The menu item must be wrapped in a `{#if canEditText}` block so
  // image / rich-text rows never see the action, and so a stale
  // foreground cannot open the editor for ineligible rows.
  const editIndex = cardSource.indexOf('data-testid="history-card-edit-text"');
  const predicateIndex = cardSource.indexOf("canEditText");
  assert.notEqual(editIndex, -1);
  assert.notEqual(predicateIndex, -1);
  // Find the `{#if canEditText}` guard preceding the menu item.
  const guardOpen = cardSource.lastIndexOf("{#if canEditText}", editIndex);
  const guardClose = cardSource.indexOf("{/if}", guardOpen);
  assert.notEqual(guardOpen, -1, "the menu item must be gated on canEditText");
  assert.ok(
    guardOpen < editIndex && editIndex < guardClose,
    "Editar captura must sit inside the {#if canEditText} guard",
  );
});

test("Edit text editor is the only path that opens the modal", () => {
  // The modal opens from the menu click; no drag, click, keydown or
  // dblclick handler on the card surface should be wired to open
  // it.
  const dragHandler = cardSource.match(
    /on:(?:pointer|mouse)down[^{]*=[\s\S]{0,200}openTextEditor/g,
  );
  // The menu item is the only legitimate place `openTextEditor` is
  // invoked from the user-visible UI. Other references must be the
  // function declaration or internal wiring.
  const onClickReferences = cardSource.match(
    /on:click=\{openTextEditor\}/g,
  );
  assert.equal(
    dragHandler,
    null,
    "no pointer / mouse handler may open the text editor",
  );
  assert.ok(
    onClickReferences !== null && onClickReferences.length === 1,
    "the menu item must be the single on:click={openTextEditor} wire",
  );
  // No card surface onClick handler is allowed to dispatch the
  // editor. Walk every other on:click handler in the source and
  // ensure none of them routes to openTextEditor.
  const allOnClicks = [
    ...cardSource.matchAll(/on:click=\{([^{}()]+)\}/g),
  ].map((match) => match[1].trim());
  const offending = allOnClicks.filter(
    (handler) => handler.includes("openTextEditor") && handler !== "openTextEditor",
  );
  assert.equal(
    offending.length,
    0,
    `no compound on:click handler may route to openTextEditor: ${offending.join(", ")}`,
  );
});

// ---------------------------------------------------------------------------
// Modal contract: native textarea, shared shell, focus, double submit lock,
// privacy.
// ---------------------------------------------------------------------------

test("modal renders a native textarea and reuses Modal.svelte", () => {
  assert.ok(
    /<textarea[\s\S]*?data-testid="entry-text-editor-textarea"/.test(modalSource),
    "the editor must render a native <textarea> with the documented testid",
  );
  assert.ok(
    /import Modal from "\.\/Modal\.svelte"/.test(modalSource),
    "the editor must reuse the shared Modal shell",
  );
  assert.ok(
    /<Modal/.test(modalSource),
    "the editor must mount the shared Modal component",
  );
});

test("modal mounts no alternative rich-text editor dependency", () => {
  // The spec mandates plain text via a native <textarea>. The
  // regression suite pins the absence of CodeMirror / Monaco /
  // Tiptap / ProseMirror / contenteditable wrappers.
  assert.ok(
    !/codemirror|tiptap|monaco|prosemirror/i.test(modalSource),
    "the editor must not depend on any third-party rich-text framework",
  );
  assert.ok(
    !/contenteditable\s*=\s*"true"/i.test(modalSource),
    "the editor must not wrap the draft in a contenteditable element",
  );
});

test("modal disables Guardar while the IPC round-trip is in flight", () => {
  // The single `saving` flag must guard the save button so a double
  // click cannot issue two `clipvault_update_text_entry` calls.
  const savingDecl = modalSource.match(/let saving\s*=\s*false/);
  assert.notEqual(savingDecl, null, "the modal must declare a `saving` flag");
  const disabled = modalSource.match(
    /disabled=\{!canSave\}[\s\S]{0,100}Guardar/,
  );
  assert.notEqual(
    disabled,
    null,
    "Guardar must stay disabled while saving=false and the draft is invalid",
  );
  // Confirm the disabled attr is bound to !canSave (which combines
  // saving + isEligible + draft length + draft equality).
  assert.ok(
    /disabled=\{!canSave\}/.test(modalSource),
    "Guardar must be bound to the `canSave` reactive flag",
  );
});

test("modal forwards the draft only through updateTextEntryCommand", () => {
  // The IPC contract: only the bridge carries the draft. The modal
  // never logs, attributes or events the text.
  const tauriImport = modalSource.match(
    /import\s*\{[^}]*updateTextEntryCommand[^}]*\}\s*from\s*"\.\/lib\/tauri"/,
  );
  assert.notEqual(
    tauriImport,
    null,
    "the modal must consume updateTextEntryCommand from ./lib/tauri",
  );
  const invokeMatch = modalSource.match(
    /await\s+updateTextEntryCommand\(\s*\{[\s\S]{0,200}?id:\s*entry\.id[\s\S]{0,200}?content:\s*draft[\s\S]{0,200}?\}\s*\)/,
  );
  assert.notEqual(
    invokeMatch,
    null,
    "the modal must call updateTextEntryCommand with id + content",
  );
  // No log of the draft.
  assert.ok(
    !/console\.(log|info|warn|error)\([^)]*draft/.test(modalSource),
    "the modal must never log the draft",
  );
});

// ---------------------------------------------------------------------------
// Card surface: protected attributes and the drag-and-drop singleton stay
// intact.
// ---------------------------------------------------------------------------

test("card keeps the protected drag/drop, pin, menu and title attributes", () => {
  assert.ok(
    /data-testid="history-card"/.test(cardSource),
    "the card must keep its `data-testid=\"history-card\"`",
  );
  assert.ok(
    /data-entry-id/.test(cardSource),
    "the card must keep `data-entry-id`",
  );
  assert.ok(
    /draggable="false"/.test(cardSource),
    "the card must stay `draggable=\"false\"`",
  );
  assert.ok(
    /data-testid="history-card-pin"/.test(cardSource),
    "the pin button must stay on the card",
  );
  assert.ok(
    /data-testid="history-card-menu-trigger"/.test(cardSource),
    "the ellipsis menu trigger must stay on the card",
  );
  assert.ok(
    /data-testid="history-card-title"/.test(cardSource),
    "the title element must stay on the card",
  );
});

test("pointer drag controller stays a singleton with pointer + mouse fallback", () => {
  const dragSource = stripComments(
    loadSource("src/lib/pointerDragAndDrop.ts"),
  );
  assert.ok(
    /let pendingDrag: PendingPointerDrag \| null = null/.test(dragSource),
    "the pointer drag controller must keep its singleton state",
  );
  assert.ok(
    /setPointerCapture/.test(dragSource) || /pointercapture/i.test(dragSource),
    "the controller must own pointer capture",
  );
  assert.ok(
    /mousedown/.test(dragSource) && /mousemove/.test(dragSource) && /mouseup/.test(dragSource),
    "the controller must keep the mousedown / mousemove / mouseup fallback",
  );
});

// ---------------------------------------------------------------------------
// Bridge contract: the typed updateTextEntryCommand covers every outcome the
// modal can reach.
// ---------------------------------------------------------------------------

test("bridge exports updateTextEntryCommand with the documented signature", () => {
  assert.ok(
    /export const updateTextEntryCommand:[\s\S]*?ClipvaultCommandArg<[\s\S]*?UpdateTextEntryResponse[\s\S]*?\{[\s\S]*?id:\s*number[\s\S]*?content:\s*string[\s\S]*?\}[\s\S]*?>/.test(
      tauriBridgeSource,
    ),
    "the bridge must export updateTextEntryCommand with id + content",
  );
  assert.ok(
    /invoke<UpdateTextEntryResponse>\("clipvault_update_text_entry"/.test(
      tauriBridgeSource,
    ),
    "the bridge must target `clipvault_update_text_entry`",
  );
});

test("types.ts exposes the discriminated UpdateTextEntryResponse", () => {
  const expected = [
    '"updated"',
    '"noop"',
    '"not_found"',
    '"not_editable"',
    '"empty_content"',
    '"duplicate_content"',
  ];
  for (const kind of expected) {
    assert.ok(
      new RegExp(`kind:\\s*${kind}`).test(typesSource),
      `UpdateTextEntryResponse must include kind: ${kind}`,
    );
  }
});

test("isEditableTextEntry gates images and rich-text rows", () => {
  // The predicate is the documented frontend gate. The modal and the
  // menu both branch on it so the affordance never surfaces for
  // ineligible entries.
  const predicate = typesSource.match(
    /export function isEditableTextEntry[\s\S]+?\n\}/,
  );
  assert.notEqual(predicate, null, "types.ts must export isEditableTextEntry");
  const body = predicate?.[0] ?? "";
  assert.ok(
    /content_type === "image"/.test(body) &&
      /asset_ref/.test(body) &&
      /rich_text_hash/.test(body) &&
      /rich_html_ref/.test(body) &&
      /rich_rtf_ref/.test(body) &&
      /rich_preview_ref/.test(body),
    "the predicate must reject image + rich-text rows",
  );
});

// ---------------------------------------------------------------------------
// Refresh contract: the shell emits the empty metadata-only history-updated
// event after a successful commit.
// ---------------------------------------------------------------------------

test("history-updated event name stays stable", () => {
  const history = stripComments(loadSource("src/lib/historyUpdates.ts"));
  assert.ok(
    /HISTORY_UPDATED_EVENT\s*=\s*"clipvault:\/\/history-updated"/.test(history),
    "the metadata-only event identifier must remain stable",
  );
  // The payload must be metadata-only (`null` or `Record<string, never>`),
  // never the edited text or any other sensitive payload.
  assert.ok(
    /HistoryUpdatedPayload\s*=\s*null\s*\|\s*Record<string,\s*never>/.test(history),
    "the event payload must stay metadata-only",
  );
});

// ---------------------------------------------------------------------------
// Reopen lifecycle: the `true -> false -> true` cycle must work for the same
// entry. The modal must dispatch a `close` event instead of mutating the
// `open` prop locally; `HistoryCard` must own the canonical `textEditorOpen`
// flag and reset it from the dispatcher handler. A regression that flips
// `open = false` inside the modal leaves the parent in `true`, which makes
// the second "Editar captura" click a no-op for the same entry.
// ---------------------------------------------------------------------------

function extractScriptBody(source: string): string {
  const match = source.match(/<script[^>]*>([\s\S]*?)<\/script>/);
  assert.notEqual(match, null, "the source must contain a <script> block");
  return match?.[1] ?? "";
}

test("modal declares a close dispatcher and never mutates the open prop locally", () => {
  // The dispatcher is the contract that lets `HistoryCard` own the
  // canonical `textEditorOpen` flag. Without it the parent never
  // hears about a close request and the modal cannot reopen on the
  // same entry within the same session.
  assert.ok(
    /createEventDispatcher<\s*\{\s*close:\s*void\s*\}\s*>/.test(modalSource),
    "the modal must declare a close event dispatcher",
  );
  const dispatchUses = [
    ...modalSource.matchAll(/dispatch\(\s*"close"\s*\)/g),
  ];
  // Two dispatch points are sufficient and intentional:
  //   1. `cancel()` covers Cancelar, Escape, backdrop and the
  //      close button (the latter through `Modal.onClose={cancel}`)
  //   2. The `save()` success branch covers the Guardar path.
  // Any additional paths that need to close should route through
  // `cancel()` to keep the contract single-sourced.
  assert.ok(
    dispatchUses.length >= 2,
    `the modal must dispatch close from cancel and save (got ${dispatchUses.length})`,
  );

  // The script must not flip the local `open` prop: doing so would
  // hide the close from the parent and silently break the reopen
  // path. We assert against the script body so the markup can keep
  // its readonly `bind:value={draft}` references untouched.
  const scriptBody = extractScriptBody(modalSource);
  assert.equal(
    /\bopen\s*=\s*false\b/.test(scriptBody),
    false,
    "the modal must never assign `open = false` in its script block",
  );
  assert.equal(
    /\bopen\s*=\s*!open\b/.test(scriptBody),
    false,
    "the modal must never toggle the `open` prop in its script block",
  );
});

test("cancel dispatches close and is wired to the shared Modal onClose path", () => {
  // Escape, backdrop and the close button MUST funnel through the
  // same single dispatcher route so the four close paths produce
  // the same observable state. Otherwise the parent would diverge
  // depending on how the user dismissed the dialog.
  const cancelBlock = modalSource.match(
    /function cancel\([^)]*\):\s*void\s*\{[\s\S]*?\n\s*\}/,
  );
  assert.notEqual(cancelBlock, null, "cancel must exist as a local function");
  assert.match(
    cancelBlock?.[0] ?? "",
    /dispatch\(\s*"close"\s*\)/,
    "cancel must dispatch close",
  );

  const modalMount = modalSource.match(/<Modal[\s\S]*?<\/Modal>/);
  assert.notEqual(modalMount, null, "the Modal mount must exist");
  assert.match(
    modalMount?.[0] ?? "",
    /onClose=\{cancel\}/,
    "the Modal must receive `onClose={cancel}` so Escape / backdrop / close button funnel through the dispatcher",
  );

  // The Cancel button inside the body must call cancel so a click
  // on the explicit button also routes through the dispatcher.
  const cancelButton = modalSource.match(
    /<button[\s\S]*?data-testid="entry-text-editor-cancel"[\s\S]*?<\/button>/,
  );
  assert.notEqual(
    cancelButton,
    null,
    "the explicit Cancel button must exist in the body",
  );
  assert.match(
    cancelButton?.[0] ?? "",
    /on:click=\{cancel\}/,
    "the Cancel button must call cancel so the dispatcher fires",
  );
});

test("save success branch dispatches close instead of mutating the open prop", () => {
  // The save success branch is the path that runs after
  // `clipvault_update_text_entry` returns `updated` or `noop`. A
  // regression that writes `open = false` here would re-introduce
  // the original reopen bug for the same entry.
  const saveBlock = modalSource.match(
    /async function save\(\)[\s\S]*?\n  \}/,
  );
  assert.notEqual(saveBlock, null, "save must exist as a local async function");
  const body = saveBlock?.[0] ?? "";
  assert.match(
    body,
    /case\s+"updated"[\s\S]*?case\s+"noop"[\s\S]*?dispatch\(\s*"close"\s*\)/,
    "the updated / noop branch must dispatch close",
  );
  assert.doesNotMatch(
    body,
    /\bopen\s*=\s*false\b/,
    "the save branch must never assign `open = false`",
  );
  assert.doesNotMatch(
    body,
    /\bopen\s*=\s*!open\b/,
    "the save branch must never toggle `open`",
  );

  // The saving guard must remain so Guardar cannot race itself and
  // so the modal never dispatches close mid-flight.
  assert.match(body, /if\s*\(\s*saving\s*\|\|\s*!canSave\s*\)\s*return/);
  assert.match(body, /saving\s*=\s*true/);
  assert.match(body, /saving\s*=\s*false/);
});

test("card listens to the close dispatcher and owns the textEditorOpen flag", () => {
  // HistoryCard must wire `on:close` to the dispatcher that
  // flips `textEditorOpen` back to `false`. The handler must run
  // even if the user has not yet selected the card and must not
  // call into the modal itself.
  const modalMount = cardSource.match(
    /<EntryTextEditorModal[\s\S]*?\/>/,
  );
  assert.notEqual(
    modalMount,
    null,
    "the EntryTextEditorModal mount must remain in HistoryCard",
  );
  const body = modalMount?.[0] ?? "";
  assert.match(
    body,
    /on:close=\{[^}]*textEditorOpen\s*=\s*false[^}]*\}/,
    "the card must wire on:close to textEditorOpen = false",
  );
  assert.doesNotMatch(
    body,
    /bind:open/,
    "the card must not bind:open into the editor; the dispatcher route is the explicit close contract",
  );

  // The card must keep the openTextEditor entry point so the
  // second click on "Editar captura" reaches the modal again. The
  // guard must still gate on `canEditText` so an ineligible entry
  // cannot reach the editor through any path.
  const openFn = cardSource.match(
    /function openTextEditor\(\):\s*void\s*\{[\s\S]*?\n  \}/,
  );
  assert.notEqual(openFn, null, "openTextEditor must exist in HistoryCard");
  const openBody = openFn?.[0] ?? "";
  assert.match(openBody, /if\s*\(\s*!canEditText\s*\)\s*return/);
  assert.match(openBody, /textEditorOpen\s*=\s*true/);
  assert.match(openBody, /closeMenuAfterAction\(\)/);

  // The menu item must still be the only wire to openTextEditor.
  const menuWires = [
    ...cardSource.matchAll(/on:click=\{openTextEditor\}/g),
  ];
  assert.equal(
    menuWires.length,
    1,
    "only the Editar captura menu item may invoke openTextEditor",
  );
});

test("modal re-seeds the draft from the persisted entry when reopened", () => {
  // The reopen branch relies on the `lastOpenedEntryId !== entry.id`
  // predicate: when the modal closes, `lastOpenedEntryId` is
  // cleared, so the next open cycle (same entry, same `id`) seeds
  // a fresh draft from `entry.content`. Without this reseed the
  // second cycle would either show a stale draft from the first
  // session or refuse to write because `canSave` short-circuits
  // when the draft equals the persisted content.
  const reactiveBlock = modalSource.match(
    /\$: if\s*\(\s*open\s*&&\s*entry\s*&&\s*lastOpenedEntryId\s*!==\s*entry\.id\s*\)\s*\{[\s\S]*?\}\s*else if\s*\(!open\s*\)\s*\{[\s\S]*?\}/,
  );
  assert.notEqual(
    reactiveBlock,
    null,
    "the modal must keep the open/close reactive block that seeds the draft",
  );
  const body = reactiveBlock?.[0] ?? "";
  assert.match(body, /draft\s*=\s*entry\.content/, "draft must reseed from entry.content on open");
  assert.match(body, /baselineEntry\s*=\s*entry/, "baselineEntry must be reset on open");
  assert.match(body, /lastOpenedEntryId\s*=\s*entry\.id/, "lastOpenedEntryId must be stamped on open");
  assert.match(body, /void focusEditor\(\)/, "focus must move into the textarea on open");
  // The close branch must clear the stamp so the next open cycle
  // re-seeds the draft for the same entry.
  assert.match(body, /lastOpenedEntryId\s*=\s*null/, "lastOpenedEntryId must reset on close");
  assert.match(body, /baselineEntry\s*=\s*null/, "baselineEntry must reset on close");
  // The draft equality check must keep the second cycle honest:
  // if `canSave` only relied on `draft.length > 0`, a stale draft
  // would still be savable as a noop. The comparison must
  // reference the persisted content so the reopen cycle starts
  // from the post-save payload.
  assert.match(
    modalSource,
    /draft\s*!==\s*\(baselineEntry\.content\s*\?\?/,
    "canSave must compare draft against the persisted content",
  );
});

test("modal preserves accessibility, focus return and the busy lock across cycles", () => {
  // The reopen regression must not regress the focus, accessibility
  // or busy-lock invariants. The assertions pin the surface so
  // future changes cannot drop one of them while shipping the
  // dispatcher fix.
  assert.match(
    modalSource,
    /aria-labelledby=\{editorId\}/,
    "the textarea must keep its aria-labelledby label",
  );
  // The explanatory summary was removed in the incremental UI
  // pass: the textarea MUST NOT carry an `aria-describedby`
  // pointing at the dropped paragraph, and the modal source MUST
  // NOT keep the `${titleId}-summary` id only for that paragraph.
  assert.doesNotMatch(
    modalSource,
    /aria-describedby=/,
    "the textarea must no longer expose aria-describedby for the dropped summary",
  );
  assert.doesNotMatch(
    modalSource,
    /\$\{titleId\}-summary/,
    "the modal must no longer mint the `${titleId}-summary` id",
  );
  assert.match(
    modalSource,
    /returnFocusTo/,
    "the modal must keep `returnFocusTo` so focus returns to the menu trigger",
  );
  assert.match(
    cardSource,
    /returnFocusTo=\{menuTriggerEl\}/,
    "HistoryCard must forward the menu trigger as returnFocusTo",
  );
  assert.match(
    modalSource,
    /busy=\{saving\}/,
    "the shared Modal must remain bound to the saving flag",
  );
  assert.match(
    modalSource,
    /disabled=\{!canSave\}/,
    "Guardar must remain bound to canSave",
  );
  assert.match(
    modalSource,
    /disabled=\{saving \|\| !isEligible\}/,
    "the textarea must remain disabled while saving or ineligible",
  );
  // The double-submit lock must check `saving || !canSave` at the
  // top of `save` so the second cycle cannot race the first.
  const saveBlock = modalSource.match(
    /async function save\(\)[\s\S]*?finally\s*\{[\s\S]*?\}/,
  );
  assert.match(
    saveBlock?.[0] ?? "",
    /if\s*\(\s*saving\s*\|\|\s*!canSave\s*\)\s*return/,
    "save must early-return when a save is already in flight",
  );
});

test("pointerDragAndDrop singleton stays intact across reopen cycles", () => {
  // The card surface that owns the editor stays mounted across the
  // reopen cycle; only the `Modal` element inside the editor
  // mounts/unmounts. The drag-and-drop singleton must remain a
  // singleton with the protected pointer / mouse fallback.
  const dragSource = stripComments(
    loadSource("src/lib/pointerDragAndDrop.ts"),
  );
  assert.match(
    dragSource,
    /let pendingDrag:\s*PendingPointerDrag\s*\|\s*null\s*=\s*null/,
  );
  assert.match(dragSource, /setPointerCapture/);
  assert.match(dragSource, /mousedown/);
  assert.match(dragSource, /mousemove/);
  assert.match(dragSource, /mouseup/);
  // The HistoryCard must keep its protected surface attributes.
  assert.match(cardSource, /data-testid="history-card"/);
  assert.match(cardSource, /data-entry-id/);
  assert.match(cardSource, /draggable="false"/);
  assert.match(cardSource, /data-testid="history-card-menu-trigger"/);
});

// ---------------------------------------------------------------------------
// UI incremental pass: dynamic title, removed summary, Ctrl+E shortcut.
//
// The follow-up incremental adjustments are exclusively presentational
// and a keyboard shortcut. The contract being pinned here:
//
//   - the modal heading is the card's resolved `displayTitle` and
//     the generic `Editar captura` literal is gone;
//   - the explanatory summary below the editor was removed along
//     with its `aria-describedby` reference, id and CSS;
//   - the textarea keeps its `aria-labelledby`, the visible label
//     and the `role="alert"` error surface;
//   - the card mounts a `Ctrl+E` shortcut for eligible entries,
//     routed through the same `openTextEditor` entry point used by
//     the menu, with `aria-keyshortcuts="Control+E"` and a stable
//     testid on the shortcut hint;
//   - the matcher refuses images, rich-text, the editor modal and
//     every interactive target without installing a global
//     `document`/`window` listener and without firing `preventDefault`
//     outside the accepted branch.
// ---------------------------------------------------------------------------

test("modal exposes a displayTitle prop and never uses a hardcoded generic title", () => {
  // The dialog heading MUST come from the parent through the
  // `displayTitle` prop and not from a hardcoded literal.
  const propMatch = modalSource.match(
    /export let displayTitle:\s*string/,
  );
  assert.notEqual(
    propMatch,
    null,
    "the modal must accept displayTitle as a string prop",
  );
  // The Modal mount must bind the `title` attribute to a
  // `displayTitle`-derived value, not a literal string. The
  // historical bug had `title=\"Editar captura\"` baked into the
  // markup; the regression pin forbids the literal.
  const modalMount = modalSource.match(/<Modal[\s\S]*?<\/Modal>/);
  assert.notEqual(modalMount, null, "the Modal mount must exist");
  const mountBody = modalMount?.[0] ?? "";
  assert.doesNotMatch(
    mountBody,
    /title=\s*["']Editar captura["']/,
    "the Modal must not hardcode the title as 'Editar captura'",
  );
  assert.match(
    mountBody,
    /title=\{modalTitle\}/,
    "the Modal title attribute must bind to the modalTitle derived from displayTitle",
  );
  // The reactive derivation must read from the displayTitle prop
  // so the heading tracks the card's resolved title.
  assert.match(
    modalSource,
    /modalTitle\s*=\s*displayTitle/,
    "modalTitle must be derived from displayTitle",
  );
  // The card must forward the resolved title to the modal so the
  // dialog and the rail agree on the heading without any extra
  // prop plumbing.
  const editorMount = cardSource.match(
    /<EntryTextEditorModal[\s\S]*?\/>/,
  );
  assert.notEqual(editorMount, null, "the editor mount must exist on the card");
  assert.match(
    editorMount?.[0] ?? "",
    /\{displayTitle\}/,
    "HistoryCard must forward displayTitle to the editor",
  );
});

test("modal drops the explanatory summary and its aria-describedby hook", () => {
  // The summary paragraph was removed along with the id it
  // generated (`${titleId}-summary`) and the `aria-describedby`
  // reference the textarea used to point at it. The pin keeps all
  // three out of the modal source so a future regression cannot
  // reintroduce the helper copy.
  assert.doesNotMatch(
    modalSource,
    /entry-text-editor-summary/,
    "the modal must no longer render the summary testid / class",
  );
  assert.doesNotMatch(
    modalSource,
    /Guardar actualiza el contenido/,
    "the modal must not render the explanatory summary copy",
  );
  assert.doesNotMatch(
    modalSource,
    /\$\{titleId\}-summary/,
    "the modal must not mint the `${titleId}-summary` id",
  );
  assert.doesNotMatch(
    modalSource,
    /aria-describedby/,
    "the textarea must not expose aria-describedby anymore",
  );
  // The summary CSS rule must also be gone so a stray style block
  // cannot reanimate the paragraph.
  assert.doesNotMatch(
    modalSource,
    /entry-text-editor-summary\s*\{/,
    "the modal stylesheet must no longer declare the summary class",
  );
});

test("modal keeps aria-labelledby, textarea label and accessible error surface", () => {
  // Removing the summary MUST NOT regress the editor's accessible
  // name, its visible label or the error announcement channel.
  assert.match(
    modalSource,
    /aria-labelledby=\{editorId\}/,
    "the textarea must keep its aria-labelledby pointing at the editor id",
  );
  const labelBlock = modalSource.match(
    /<label[\s\S]*?<\/label>/,
  );
  assert.notEqual(labelBlock, null, "the textarea must keep a visible <label>");
  assert.match(
    labelBlock?.[0] ?? "",
    /Contenido editable de la captura/,
    "the visible label must keep the documented copy",
  );
  const errorBlock = modalSource.match(
    /<p[\s\S]*?data-testid="entry-text-editor-error"[\s\S]*?<\/p>/,
  );
  assert.notEqual(errorBlock, null, "the error paragraph must remain");
  assert.match(
    errorBlock?.[0] ?? "",
    /role="alert"/,
    "the error paragraph must keep role=\"alert\" so it is announced",
  );
  // Guardar / Cancelar / Restablecer stay reachable.
  assert.match(
    modalSource,
    /data-testid="entry-text-editor-save"/,
  );
  assert.match(
    modalSource,
    /data-testid="entry-text-editor-cancel"/,
  );
  assert.match(
    modalSource,
    /data-testid="entry-text-editor-reset"/,
  );
});

test("card menu item exposes Ctrl+E alongside Editar captura with aria-keyshortcuts", () => {
  // The shortcut hint must live inside the menu item block (so it
  // is part of the same click target as the action) and must carry
  // the documented testid / aria attribute pair so screen readers
  // announce the keyboard shortcut.
  const itemMatch = cardSource.match(
    /<button\s[\s\S]*?data-testid="history-card-edit-text"[\s\S]*?<\/button>/,
  );
  assert.notEqual(itemMatch, null, "the Editar captura menu item must exist");
  const itemBody = itemMatch?.[0] ?? "";
  assert.match(
    itemBody,
    /aria-keyshortcuts=\{EDIT_TEXT_SHORTCUT_KEY_ATTR\}/,
    "the menu item must expose aria-keyshortcuts bound to the Ctrl+E constant",
  );
  // The shortcut testid is bound to a constant so the visible
  // affordance and the aria attribute cannot drift.
  assert.match(
    itemBody,
    /data-testid=\{EDIT_TEXT_SHORTCUT_TESTID\}/,
    "the menu item must bind data-testid to the shortcut testid constant",
  );
  // The hint copy must read the `EDIT_TEXT_SHORTCUT_LABEL` constant
  // so the visual affordance and the aria attribute agree.
  const shortcutSpan = cardSource.match(
    /EDIT_TEXT_SHORTCUT_TESTID[\s\S]*?\{EDIT_TEXT_SHORTCUT_LABEL\}/,
  );
  assert.notEqual(
    shortcutSpan,
    null,
    "the shortcut hint must render the EDIT_TEXT_SHORTCUT_LABEL constant",
  );
  // The `Ctrl+E` literal must be defined as a constant so the
  // visible copy and the aria attribute cannot drift.
  assert.match(
    cardSource,
    /EDIT_TEXT_SHORTCUT_LABEL\s*=\s*"Ctrl\+E"/,
    "the card must declare the Ctrl+E label constant",
  );
  assert.match(
    cardSource,
    /EDIT_TEXT_SHORTCUT_KEY_ATTR\s*=\s*"Control\+E"/,
    "the card must declare the aria-keyshortcuts value constant",
  );
  assert.match(
    cardSource,
    /EDIT_TEXT_SHORTCUT_TESTID\s*=\s*"history-card-edit-text-shortcut"/,
    "the card must declare the shortcut testid constant",
  );
  // The action label must still read literally `Editar captura`.
  assert.match(
    itemBody,
    />\s*Editar captura\s*</,
    "the menu item must keep the visible 'Editar captura' label",
  );
});

test("Ctrl+E shortcut lives on the card keyboard handler and delegates to openTextEditor", () => {
  // The matcher must run inside `onCardKeydown` (the card's own
  // surface keyboard handler) and must call the same `openTextEditor`
  // entry point the menu uses. The shortcut MUST NOT install a
  // global listener on `document` or `window` from the card.
  const handler = cardSource.match(
    /function onCardKeydown\([^)]*\):\s*void\s*\{[\s\S]*?\n  \}/,
  );
  assert.notEqual(
    handler,
    null,
    "onCardKeydown must exist as a local function on the card",
  );
  const body = handler?.[0] ?? "";
  assert.match(
    body,
    /matchesEditTextShortcut\(event\)/,
    "onCardKeydown must run the Ctrl+E matcher",
  );
  assert.match(
    body,
    /openTextEditor\(\)/,
    "the Ctrl+E branch must delegate to openTextEditor",
  );
  // preventDefault must run inside the accepted branch only.
  const branchMatch = body.match(
    /if\s*\(\s*matchesEditTextShortcut\(event\)\s*\)\s*\{[\s\S]*?\n {4}\}/,
  );
  assert.notEqual(branchMatch, null, "the Ctrl+E branch must exist");
  assert.match(
    branchMatch?.[0] ?? "",
    /event\.preventDefault\(\)/,
    "preventDefault must run inside the accepted branch",
  );
  // The card MUST NOT install document or window keydown
  // listeners from this module. Existing document-level listeners
  // (the Modal shell, the pointer drag controller) are allowed to
  // stay but the card itself cannot add a new global handler.
  const documentAdd = cardSource.match(/document\.addEventListener\(\s*["']keydown/);
  const windowAdd = cardSource.match(/window\.addEventListener\(\s*["']keydown/);
  assert.equal(
    documentAdd,
    null,
    "HistoryCard must not install document-level keydown listeners for the shortcut",
  );
  assert.equal(
    windowAdd,
    null,
    "HistoryCard must not install window-level keydown listeners for the shortcut",
  );
});

test("Ctrl+E matcher rejects ineligible entries and modal targets, calls preventDefault only when accepted", () => {
  // The matcher must refuse ineligible entries (`canEditText`) and
  // any target that lives inside a `[role="dialog"]`. Both checks
  // must appear inside the same branch so `preventDefault` only
  // runs when the shortcut is accepted.
  const handler = cardSource.match(
    /function onCardKeydown\([^)]*\):\s*void\s*\{[\s\S]*?\n  \}/,
  );
  const body = handler?.[0] ?? "";
  const branch = body.match(
    /if\s*\(\s*matchesEditTextShortcut\(event\)\s*\)\s*\{[\s\S]*?\n {4}\}/,
  );
  assert.notEqual(branch, null, "the Ctrl+E branch must exist");
  const branchBody = branch?.[0] ?? "";
  assert.match(
    branchBody,
    /if\s*\(\s*!canEditText\s*\)\s*return/,
    "the Ctrl+E branch must early-return when canEditText is false",
  );
  assert.match(
    branchBody,
    /if\s*\(\s*isInsideModalTarget\(event\.target\)\s*\)\s*return/,
    "the Ctrl+E branch must early-return when the target lives inside a modal",
  );
  assert.match(
    branchBody,
    /event\.preventDefault\(\)/,
    "preventDefault must run inside the accepted branch",
  );
  // The modal-target guard must be implemented as a closest walk
  // over the target, not as a global flag, so it works for every
  // dialog future modals might mount.
  const helperMatch = cardSource.match(
    /function isInsideModalTarget[\s\S]*?\n  \}/,
  );
  assert.notEqual(
    helperMatch,
    null,
    "the card must declare an isInsideModalTarget helper",
  );
  assert.match(
    helperMatch?.[0] ?? "",
    /closest\(\s*['"]\[role=["']dialog["']\]['"]\s*\)/,
    "isInsideModalTarget must walk to the closest role=dialog ancestor",
  );
  // The existing isInteractiveTarget guard must run before the
  // shortcut matcher so input / textarea / select / button /
  // contenteditable / menu / overflow chip are excluded first.
  const orderedMatch = body.match(
    /if\s*\(\s*isInteractiveTarget\(event\.target\)\s*\)\s*return[\s\S]*?matchesEditTextShortcut\(event\)/,
  );
  assert.notEqual(
    orderedMatch,
    null,
    "isInteractiveTarget must run before the Ctrl+E matcher",
  );
});

test("Ctrl+E shortcut does not select the card or interact with the drag controller", () => {
  // The shortcut MUST NOT mutate the selection (the rail owns
  // `selectedEntryId`) and MUST NOT touch the singleton pointer
  // drag controller. The acceptance branch only calls
  // `openTextEditor`, which already closes the menu and never
  // dispatches `select-request`.
  const handler = cardSource.match(
    /function onCardKeydown\([^)]*\):\s*void\s*\{[\s\S]*?\n  \}/,
  );
  const branch = (handler?.[0] ?? "").match(
    /if\s*\(\s*matchesEditTextShortcut\(event\)\s*\)\s*\{[\s\S]*?\n\s*\}/,
  );
  const branchBody = branch?.[0] ?? "";
  assert.doesNotMatch(
    branchBody,
    /dispatchSelect/,
    "the Ctrl+E branch must not flip the selection",
  );
  assert.doesNotMatch(
    branchBody,
    /requestPreview/,
    "the Ctrl+E branch must not open the preview",
  );
  assert.doesNotMatch(
    branchBody,
    /pendingDrag|setPointerCapture|beginDragSession/,
    "the Ctrl+E branch must not start a pointer drag",
  );
  // The card must keep the drag-and-drop singleton source intact
  // and the protected surface attributes.
  const dragSource = stripComments(
    loadSource("src/lib/pointerDragAndDrop.ts"),
  );
  assert.match(
    dragSource,
    /let pendingDrag:\s*PendingPointerDrag\s*\|\s*null\s*=\s*null/,
  );
  assert.match(cardSource, /data-testid="history-card"/);
  assert.match(cardSource, /data-entry-id/);
  assert.match(cardSource, /draggable="false"/);
  // The menu trigger and the pin / title controls must stay in
  // place so the open-from-menu path is unchanged.
  assert.match(
    cardSource,
    /data-testid="history-card-menu-trigger"/,
  );
  assert.match(cardSource, /data-testid="history-card-pin"/);
  assert.match(cardSource, /data-testid="history-card-title"/);
});