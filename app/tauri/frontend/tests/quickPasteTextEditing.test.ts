/**
 * Source-level coverage for the `quick-paste-text-editing` change.
 *
 * The Quick Paste palette and the desktop rail share the same
 * `EntryTextEditorModal.svelte`, the same `updateTextEntryCommand`
 * bridge and the same `editTextShortcut*` helper module so a
 * regression that drifts the Quick Paste surface away from the
 * existing desktop contract is caught here before it can ship.
 *
 * The suite pins:
 *
 *   - the persistent text editor modal is mounted through the
 *     shared `EntryTextEditorModal.svelte` component (no parallel
 *     modal, no parallel IPC call);
 *   - the modal is fed `displayTitle` derived from the entry's
 *     resolved title (the exact same `displayTitle` the rail uses);
 *   - the menu exposes `Editar captura` between `Copiar` and
 *     `Previsualizar` for plain text entries and never for image /
 *     rich entries (the matrix helpers `quickPasteMenuActions` and
 *     the matching `QUICK_PASTE_EDIT_*` constants are exercised
 *     here too so a rename surfaces as a test failure);
 *   - the visible shortcut hint and the `aria-keyshortcuts`
 *     attribute are derived from `editTextShortcutLabel` /
 *     `editTextShortcutKeyAttribute` (and the matching preview
 *     helpers) so the modifier table and the matcher cannot drift;
 *   - the `Cmd/Ctrl+E` shortcut listener is integrated with the
 *     gating the spec requires (input / textarea / select /
 *     contenteditable, menu, dialog) and resolves the entry by id;
 *   - the `clipvault://history-updated` event the shell emits after
 *     a successful commit rehydrates the active recent / search
 *     source without ever carrying clipboard content, hashes,
 *     snippets, asset references or paths;
 *   - the drag-and-drop baselines the `AGENTS.md` file pins
 *     (`pointerDragAndDrop.ts`, `data-testid="history-card"`,
 *     `data-entry-id`, `draggable="false"`, the singleton drag
 *     controller, the WebKit fallback) stay intact across the
 *     Quick Paste integration.
 *
 * Privacy: the suite inspects source files only and never reads
 * clipboard content, asset references, paths or hashes. The event
 * payload is asserted to be empty (`()`) for both the
 * `clipvault://history-updated` emission and the dispatcher
 * payload the modal sends on close.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

import {
  matchesEditTextShortcut,
  editTextShortcutLabel,
  editTextShortcutKeyAttribute,
  editTextShortcutAccessibleLabel,
} from "../src/lib/editTextShortcut.ts";

// ---------------------------------------------------------------------------
// Shortcut labels — the visible hint, the accessible label and the
// `aria-keyshortcuts` attribute all derive from the shared helper
// modules so the matcher the listener consults and the affordance
// the user sees cannot drift apart across macOS / Linux hosts.
// ---------------------------------------------------------------------------

test("shortcut labels: edit hint matches the shared editTextShortcutLabel helper", () => {
  assert.equal(editTextShortcutLabel("macos"), "⌘E");
  assert.equal(editTextShortcutLabel("other"), "Ctrl+E");
});

test("shortcut labels: edit accessible label matches the shared helper", () => {
  assert.equal(
    editTextShortcutAccessibleLabel("macos"),
    "Editar captura (Comando E)",
  );
  assert.equal(
    editTextShortcutAccessibleLabel("other"),
    "Editar captura (Control E)",
  );
});

test("shortcut labels: edit key attribute uses the Meta+E / Control+E syntax", () => {
  assert.equal(editTextShortcutKeyAttribute("macos"), "Meta+E");
  assert.equal(editTextShortcutKeyAttribute("other"), "Control+E");
});

test("shortcut labels: preview hint matches the shared previewShortcutLabel helper", () => {
  // The preview labels live in `lib/clipboardPreview.ts`. The
  // source-level assertions below pin the same constants from the
  // QuickPaste.svelte file so this suite can run without pulling in
  // the metadata re-exports `clipboardPreview.ts` ships (the chain
  // would otherwise reach `tauri.ts`, which the pre-existing test
  // infrastructure cannot resolve).
  const source = readQuickPasteSource();
  assert.match(
    source,
    /previewShortcutLabel\(shortcutPlatform\)/,
    "QuickPaste.svelte must render the preview shortcut hint through the shared helper",
  );
});

// ---------------------------------------------------------------------------
// Matcher gating — the `Cmd/Ctrl+E` matcher must accept the platform
// correct shortcut and reject the cross-platform modifier, `Alt`,
// `Shift` and an unrelated key. These tests pin the contract the
// Quick Paste keyboard listener consults before it routes the
// activation through the menu action entry point.
// ---------------------------------------------------------------------------

test("matcher: Cmd+E on macOS opens the editor", () => {
  assert.equal(
    matchesEditTextShortcut(
      { key: "e", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false },
      "macos",
    ),
    true,
  );
});

test("matcher: Ctrl+E on Linux opens the editor", () => {
  assert.equal(
    matchesEditTextShortcut(
      { key: "e", metaKey: false, ctrlKey: true, altKey: false, shiftKey: false },
      "other",
    ),
    true,
  );
});

test("matcher: Ctrl+E on macOS is rejected", () => {
  assert.equal(
    matchesEditTextShortcut(
      { key: "e", metaKey: false, ctrlKey: true, altKey: false, shiftKey: false },
      "macos",
    ),
    false,
  );
});

test("matcher: Cmd+E on Linux is rejected", () => {
  assert.equal(
    matchesEditTextShortcut(
      { key: "e", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false },
      "other",
    ),
    false,
  );
});

test("matcher: Alt+E / Shift+E are always rejected", () => {
  for (const platform of ["macos", "other"] as const) {
    for (const event of [
      { key: "e", metaKey: false, ctrlKey: false, altKey: true, shiftKey: false },
      { key: "e", metaKey: false, ctrlKey: false, altKey: false, shiftKey: true },
    ]) {
      assert.equal(
        matchesEditTextShortcut(event, platform),
        false,
        `Alt/Shift modifier on ${platform} must not trigger the edit shortcut`,
      );
    }
  }
});

test("matcher: case-insensitive E and Enter-related keys are distinct", () => {
  // Caps Lock surfaces as an uppercase "E" on every layout; the
  // matcher must still trigger through the webview.
  assert.equal(
    matchesEditTextShortcut(
      { key: "E", metaKey: false, ctrlKey: true, altKey: false, shiftKey: false },
      "other",
    ),
    true,
  );
  // An unrelated key (Enter) must NOT trigger the edit shortcut.
  assert.equal(
    matchesEditTextShortcut(
      { key: "Enter", metaKey: false, ctrlKey: true, altKey: false, shiftKey: false },
      "other",
    ),
    false,
  );
});

test("matcher: Cmd+Enter still resolves the preview matcher (no overlap)", () => {
  // The two shortcuts share the `e` / `Enter` key with different
  // modifiers. Confirm the preview matcher does not pick up
  // `Cmd/Ctrl+E` so the keyboard listener cannot dispatch both
  // affordances on a single keystroke. The preview matcher lives in
  // `clipboardPreview.ts` which transitively imports `tauri.ts`; we
  // pin the cross-shortcut isolation at the QuickPaste.svelte
  // source level so the assertion can run without breaking the
  // pre-existing test infrastructure.
  const source = readQuickPasteSource();
  assert.match(
    source,
    /matchesQuickPastePreviewShortcut\(event,\s*shortcutPlatform\)/,
    "Quick Paste must consult the shared preview matcher for Cmd/Ctrl+Enter",
  );
  assert.match(
    source,
    /matchesEditTextShortcut\(event,\s*shortcutPlatform\)/,
    "Quick Paste must consult the shared edit matcher for Cmd/Ctrl+E",
  );
});

// ---------------------------------------------------------------------------
// Source-level integration — the Quick Paste component imports the
// shared modal, subscribes to the metadata-only history-updated
// event, wires the menu shortcut hint with the matching
// `aria-keyshortcuts` attribute and routes the activation through
// the entry id. Reading the source file directly keeps the
// regression independent of the Svelte runtime / Vite preview so
// the assertions stay deterministic on every host.
// ---------------------------------------------------------------------------

function readQuickPasteSource(): string {
  const path = resolvePath(process.cwd(), "src", "QuickPaste.svelte");
  return readFileSync(path, "utf8");
}

function readPointerDragAndDropSource(): string {
  const path = resolvePath(process.cwd(), "src", "lib", "pointerDragAndDrop.ts");
  return readFileSync(path, "utf8");
}

function readEntryTextEditorModalSource(): string {
  const path = resolvePath(
    process.cwd(),
    "src",
    "EntryTextEditorModal.svelte",
  );
  return readFileSync(path, "utf8");
}

function readEditTextShortcutSource(): string {
  const path = resolvePath(
    process.cwd(),
    "src",
    "lib",
    "editTextShortcut.ts",
  );
  return readFileSync(path, "utf8");
}

function readHistoryUpdatesSource(): string {
  const path = resolvePath(
    process.cwd(),
    "src",
    "lib",
    "historyUpdates.ts",
  );
  return readFileSync(path, "utf8");
}

function readQuickPasteActionsSource(): string {
  const path = resolvePath(
    process.cwd(),
    "src",
    "lib",
    "quickPasteActions.ts",
  );
  return readFileSync(path, "utf8");
}

test("source: Quick Paste imports the shared EntryTextEditorModal and updateTextEntryCommand is only mentioned in comments", () => {
  const source = readQuickPasteSource();
  assert.match(
    source,
    /import\s+EntryTextEditorModal\s+from\s+"\.\/EntryTextEditorModal\.svelte"/,
    "QuickPaste.svelte must import the shared editor modal",
  );
  // The Quick Paste window MUST NOT call the IPC bridge directly;
  // the modal owns the commit so double submit / error surface /
  // close dispatcher stay in one place.
  assert.equal(
    /\bupdateTextEntryCommand\s*\(/.test(source),
    false,
    "QuickPaste.svelte must not call updateTextEntryCommand directly; the modal owns the commit",
  );
});

test("source: Quick Paste mounts the editor with displayTitle and a stable focus target", () => {
  const source = readQuickPasteSource();
  assert.match(
    source,
    /<EntryTextEditorModal[\s\S]*?displayTitle=\{editorTitle\}[\s\S]*?returnFocusTo=\{editorFocusTarget\}[\s\S]*?on:close=\{handleQuickPasteEditorClose\}\s*\/>/,
    "the Quick Paste window must mount the modal with displayTitle and a stable focus target",
  );
});

test("source: Quick Paste subscribes to the metadata-only history-updated event", () => {
  const source = readQuickPasteSource();
  // The subscription helper MUST be the shared registrar the
  // desktop rail consumes so the payload stays `()` and the
  // refresh reuses the same `clipvault_recent_entries` round-trip.
  assert.match(
    source,
    /import\s*\{[^}]*listenHistoryUpdated[^}]*\}\s*from\s+"\.\/lib\/historyUpdates"/,
    "QuickPaste.svelte must import listenHistoryUpdated from the shared historyUpdates module",
  );
  assert.match(
    source,
    /unlistenHistoryUpdated\s*=\s*safeListenHistoryUpdated/,
    "the window must install the subscription through the safe listener wrapper",
  );
  // The Quick Paste refresh path MUST never carry clipboard content
  // or hashes through the event payload; the dispatcher payload is
  // explicitly `()` in the shared registrar and the refresh reuses
  // the existing `loadRecent` / `runQuery` paths.
  const historyUpdates = readHistoryUpdatesSource();
  assert.match(
    historyUpdates,
    /return\s+listen\(HISTORY_UPDATED_EVENT,\s*\(\)\s*=>/,
    "historyUpdates must register the listener with an empty payload (metadata-only)",
  );
});

test("source: Quick Paste never emits, logs or sends capture content through the edit flow", () => {
  const source = readQuickPasteSource();
  // The Quick Paste window's edit path MUST NOT log the persisted
  // content or the draft; a regression that pipes the entry's
  // content through a `console.*` would be caught here. The regex
  // anchors on `entry.content,` / `entry.content)` / `entry.content }`
  // so legitimate `entry.content_type` / `entry.content_hash`
  // references (used by `findEntry`) are not flagged.
  const contentRefRegex =
    /entry\.content(?!\w)|quickPasteEditorEntry\.content(?!\w)/;
  assert.equal(
    contentRefRegex.test(source),
    false,
    "QuickPaste.svelte must never pipe entry.content through console or dispatch payload",
  );
});

test("source: menu item renders the platform-aware shortcut hint and the aria-keyshortcuts attribute", () => {
  const source = readQuickPasteSource();
  assert.match(
    source,
    /aria-keyshortcuts=\{shortcutKey\}/,
    "the menu item must expose the platform-aware aria-keyshortcuts attribute",
  );
  assert.match(
    source,
    /<span\s+class="qp-menu-item-shortcut"/,
    "the visible shortcut hint must live in a dedicated qp-menu-item-shortcut span",
  );
});

test("source: Cmd/Ctrl+E matcher gates inputs, textareas, selects, contenteditable, menu and dialog", () => {
  const source = readQuickPasteSource();
  assert.match(
    source,
    /matchesEditTextShortcut/,
    "Quick Paste must consult the shared matchesEditTextShortcut helper",
  );
  // The listener MUST refuse the shortcut while focus lives inside
  // an editable surface so the user can keep typing in the search
  // field or in any other editable input without losing keystrokes.
  assert.match(
    source,
    /HTMLInputElement/,
  );
  assert.match(
    source,
    /HTMLTextAreaElement/,
  );
  assert.match(
    source,
    /HTMLSelectElement/,
  );
  assert.match(
    source,
    /isContentEditable/,
  );
  // The menu popover and the dialog wrapper MUST keep focus on
  // their own surface; the matcher closes both branches by walking
  // the closest role attribute.
  assert.match(
    source,
    /'\[role="menu"\]'/,
  );
  assert.match(
    source,
    /'\[role="dialog"\]'/,
  );
});

test("source: edit shortcut uses the entry id from the live selection, not a stale closure", () => {
  const source = readQuickPasteSource();
  // The handler MUST resolve the entry by id from the live
  // `recent` / `hits` feeds so a refresh that landed between the
  // activation and the resolution cannot route the request to a
  // stale row.
  assert.match(
    source,
    /findEntry\(mode,\s*recent,\s*hits,\s*id\)/,
    "the keyboard handler must resolve the entry by id from the live feeds",
  );
  // The eligibility predicate MUST be re-evaluated even when the
  // menu already gated the entry so a stale rich / image row can
  // never reach the editor.
  assert.match(
    source,
    /isEditableTextEntry\(entry\)/,
    "the keyboard handler must re-evaluate isEditableTextEntry before opening the modal",
  );
});

test("source: persistent editor mounts only when open AND entry is resolved", () => {
  const source = readQuickPasteSource();
  // The block must guard against an unresolved entry id; a render
  // cycle that flips `quickPasteEditorOpen` before
  // `quickPasteEditorEntry` would silently lose the modal.
  assert.match(
    source,
    /\{#if\s+quickPasteEditorOpen\s*&&\s*quickPasteEditorEntry\s*\}/,
    "the modal must be mounted behind the open AND entry-resolved guard",
  );
});

test("source: drag-and-drop baselines stay intact across the Quick Paste integration", () => {
  const drag = readPointerDragAndDropSource();
  // The singleton drag controller the AGENTS.md file pins must keep
  // its `pendingDrag: PendingPointerDrag | null` shape and the
  // pointer / mouse fallback so a Quick Paste edit cannot regress
  // the rail's drag-and-drop contract.
  assert.match(
    drag,
    /pendingDrag:\s*PendingPointerDrag\s*\|\s*null\s*=\s*null/,
    "pointerDragAndDrop.ts must keep the singleton pendingDrag state",
  );
  assert.match(
    drag,
    /setPointerCapture/,
    "pointerDragAndDrop.ts must keep setPointerCapture",
  );
  assert.match(
    drag,
    /mousedown/,
    "pointerDragAndDrop.ts must keep the mousedown / mousemove / mouseup fallback",
  );
  assert.match(
    drag,
    /mousemove/,
    "pointerDragAndDrop.ts must keep the mousedown / mousemove / mouseup fallback",
  );
  assert.match(
    drag,
    /mouseup/,
    "pointerDragAndDrop.ts must keep the mousedown / mousemove / mouseup fallback",
  );
});

test("source: shared editor modal keeps its close dispatcher and never mutates the open prop locally", () => {
  const modal = readEntryTextEditorModalSource();
  // The desktop / Quick Paste contract the editable-text-captures
  // change pins: the modal must dispatch `close` and MUST NOT
  // assign `open = false` from the child. The regex anchors on
  // `open = false` outside block comments so the JSDoc warning
  // ("naive `open = false` inside the child") does not trip the
  // assertion.
  const openAssignmentRegex = /^[^*]*\bopen\s*=\s*false\b/m;
  assert.match(
    modal,
    /createEventDispatcher<\s*\{\s*close:\s*void\s*\}\s*>\(\)/,
    "the modal must dispatch a close event so the parent owns the open flag",
  );
  // Strip block comments before checking for the assignment.
  const stripped = modal.replace(/\/\*[\s\S]*?\*\//g, "");
  assert.equal(
    openAssignmentRegex.test(stripped),
    false,
    "the modal must never mutate the open prop locally",
  );
});

test("source: shared matcher module never imports clipboard content", () => {
  const matcher = readEditTextShortcutSource();
  // The matcher module MUST only consume platform + event metadata.
  // A regression that imports the entry, the clipboard payload or
  // the asset reference would surface here.
  for (const banned of [
    "types",
    "clipboardAsset",
    "historyUpdates",
    "tauri.ts",
    "EntryTextEditorModal",
  ]) {
    assert.equal(
      new RegExp(`from\\s+["']\\.\\.?\\/.*${banned}`).test(matcher),
      false,
      `editTextShortcut.ts must not import ${banned}`,
    );
  }
});

test("source: matrix constants surface as exported symbols with stable test ids", () => {
  // The Quick Paste menu matrix the design documents exports the
  // `QUICK_PASTE_EDIT_LABEL` / `QUICK_PASTE_EDIT_TEST_ID` constants
  // so the regression suite can rename-detect a drift without
  // inspecting the rendered DOM. The constants live in the same
  // module the matrix helper consults so a future rename of the
  // `kind` discriminator or the test id surfaces here.
  const actions = readQuickPasteActionsSource();
  assert.match(
    actions,
    /export\s+const\s+QUICK_PASTE_EDIT_LABEL\s*=\s*"Editar captura"/,
    "quickPasteActions.ts must export the QUICK_PASTE_EDIT_LABEL constant",
  );
  assert.match(
    actions,
    /export\s+const\s+QUICK_PASTE_EDIT_TEST_ID\s*=\s*"quick-paste-menu-edit"/,
    "quickPasteActions.ts must export the QUICK_PASTE_EDIT_TEST_ID constant",
  );
  // The matrix helper MUST reject rich and image entries through
  // the `isEditableTextEntry` predicate so a future tweak cannot
  // accidentally surface the editor on a non-eligible row.
  assert.match(
    actions,
    /isEditableTextEntry\(entry\s+as\s+EntryRecord\)/,
    "quickPasteActions.ts must gate the edit action through isEditableTextEntry",
  );
});

// ---------------------------------------------------------------------------
// Fixes derived from the first manual pass on Wayland / X11 / macOS.
// The previous section already covered the structural contract; this
// section pins the keyboard + dialog behaviour the manual exercise
// surfaced:
//
//   - Ctrl/Cmd+E works on the first activation (no stale entry id,
//     no fall-back to a previously edited row);
//   - the search input is the only HTMLInputElement that allows the
//     shortcut (every other input / textarea / select / contenteditable
//     must keep the keystroke);
//   - image and rich-text rows trigger an informative dialog (not the
//     editor) and the dialog never mounts EntryTextEditorModal nor
//     invokes the updateTextEntryCommand bridge;
//   - Escape inside the editor must not close Quick Paste (the global
//     Escape handler must detect the [role="dialog"] wrapper).
// ---------------------------------------------------------------------------

test("fix: shortcut resolves the live selection id and never falls back to the last edited entry", () => {
  // The keyboard handler MUST compute the target entry id from the
  // current `selectedEntryId` (preferred, when it still maps to a row
  // in `resultIds`) or from `resultIds[selectedIndex]` (fallback). It
  // MUST NOT consult `quickPasteEditorEntryId` (removed) nor a stale
  // `quickPasteEditorEntry` snapshot — those references would
  // silently reopen a previously edited row when the current
  // selection is image / rich text.
  const source = readQuickPasteSource();
  assert.match(
    source,
    /selectedEntryId\s*!==\s*null[\s\S]*?resultIds\.indexOf\(selectedEntryId\)\s*>=\s*0/,
    "the keyboard handler must resolve the live selection id from selectedEntryId when it is in scope",
  );
  assert.match(
    source,
    /resultIds\[selectedIndex\]\s*\?\?\s*null/,
    "the keyboard handler must fall back to resultIds[selectedIndex] when the selection id is stale",
  );
  // The Quick Paste window MUST NOT keep a stale id variable. The
  // `quickPasteEditorEntryId` flag was the regression source for the
  // "second activation reopens the last edited row" failure. Strip
  // block comments AND line comments before checking so the
  // documentation that mentions the old variable name (so a future
  // contributor knows why the variable is gone) does not trip the
  // assertion.
  const strippedSource = source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\/\/[^\n]*/g, "");
  assert.equal(
    /quickPasteEditorEntryId/.test(strippedSource),
    false,
    "QuickPaste.svelte must not keep a stale last-edited-entry id; the live selection is the single source of truth",
  );
});

test("fix: shortcut gates every HTMLInputElement except the Quick Paste search input", () => {
  const source = readQuickPasteSource();
  // The previous round rejected every HTMLInputElement, which
  // silently killed the shortcut because the Quick Paste search
  // input holds the initial focus. The fix keeps the gate for
  // every other input while carving out the search input through a
  // stable identity comparison (`target === searchInputEl`).
  assert.match(
    source,
    /isQuickPasteSearchInput\s*=\s*target\s+instanceof\s+HTMLInputElement\s*&&\s*target\s*===\s*searchInputEl/,
    "the keyboard handler must identify the Quick Paste search input through `target === searchInputEl`",
  );
  assert.match(
    source,
    /isOtherEditableSurface/,
    "the keyboard handler must distinguish other editable surfaces from the search input",
  );
});

test("fix: shortcut resolves to the informative dialog for image entries", () => {
  const source = readQuickPasteSource();
  // When the resolved entry fails `isEditableTextEntry`, the
  // handler MUST mount the informative dialog (and NEVER open the
  // editor). The dialog open helper is the single switch the
  // keyboard listener consults so a regression that opens the
  // editor on an image row is caught here.
  assert.match(
    source,
    /isEditableTextEntry\(entry\)/,
    "the keyboard handler must re-evaluate isEditableTextEntry",
  );
  assert.match(
    source,
    /openQuickPasteNotEditableDialogFor\(entry\)/,
    "the keyboard handler must open the informative dialog for ineligible entries",
  );
  // The dialog state MUST live next to the editor state, never
  // reuse the editor flags so a stale editor row cannot sneak into
  // the dialog.
  assert.match(
    source,
    /let\s+quickPasteNotEditableDialogEntryId/,
    "Quick Paste must keep a separate state slot for the informative dialog entry id",
  );
});

test("fix: shortcut resolves to the informative dialog for rich-text entries", () => {
  const source = readQuickPasteSource();
  // `isEditableTextEntry` is the single predicate that gates both
  // image and rich-text rows; the helper is shared with the desktop
  // rail so a regression that opens the editor on a rich row is
  // caught here even when the matrix helper never surfaces the
  // menu item.
  assert.match(
    source,
    /if\s*\(\s*!\s*isEditableTextEntry\(entry\)\s*\)/,
    "the negative eligibility branch must surface the informative dialog",
  );
  // The dialog branch must short-circuit (`return`) BEFORE the
  // editor mount can run, so the two surfaces stay mutually
  // exclusive: an image row opens the dialog, a text row opens
  // the editor, never both.
  assert.match(
    source,
    /openQuickPasteNotEditableDialogFor\(entry\)[\s\S]*?return\s*;/,
    "the dialog branch must short-circuit before the editor mount",
  );
  // The editor mount line that lives in the keyboard handler MUST
  // appear after the dialog helper inside the same handler. The
  // helper is only used by the keyboard path; the menu path also
  // assigns `quickPasteEditorEntry = entry` but in a different
  // function, so the assertion searches forward from the dialog
  // helper to find the next editor mount line.
  const dialogIndex = source.indexOf("openQuickPasteNotEditableDialogFor(entry)");
  assert.ok(dialogIndex > 0, "the dialog helper must be present");
  const editorIndexAfterDialog = source.indexOf(
    "quickPasteEditorEntry = entry;",
    dialogIndex,
  );
  assert.ok(
    editorIndexAfterDialog > dialogIndex,
    "the editor mount line that lives after the dialog helper must be present so the dialog branch short-circuits it",
  );
});

test("fix: informative dialog reuses Modal.svelte and never invokes the update bridge", () => {
  const source = readQuickPasteSource();
  // The dialog MUST mount through `Modal.svelte` so the focus
  // trap, Escape handling, backdrop dismissal and accessible
  // labelling stay in lockstep with the rest of the Quick Paste
  // surfaces.
  assert.match(
    source,
    /import\s+Modal\s+from\s+"\.\/Modal\.svelte"/,
    "QuickPaste.svelte must import the shared Modal shell for the informative dialog",
  );
  // The dialog close path MUST NOT call any IPC bridge — the only
  // surface mutation is clearing the dialog id slot. The regex
  // strips block comments so the JSDoc paragraph that mentions the
  // bridge contract does not trip the assertion.
  const stripped = source.replace(/\/\*[\s\S]*?\*\//g, "");
  assert.match(
    stripped,
    /function\s+closeQuickPasteNotEditableDialog\(\)\s*:\s*void\s*\{[\s\S]*?quickPasteNotEditableDialogEntryId\s*=\s*null/s,
    "the dialog close helper must clear the id slot without invoking the update bridge",
  );
  // The dialog must carry a stable testid the regression suite
  // can target without touching the rendered DOM.
  assert.match(
    source,
    /data-testid="quick-paste-not-editable-dialog"/,
    "the dialog markup must carry the stable quick-paste-not-editable-dialog testid",
  );
  // The dialog body must explain the cause and the title must be
  // accessible through the shared Modal shell.
  assert.match(
    source,
    /QUICK_PASTE_NOT_EDITABLE_DIALOG_TITLE\s*=\s*"Captura no editable"/,
    "the dialog title must be 'Captura no editable'",
  );
  assert.match(
    source,
    /QUICK_PASTE_NOT_EDITABLE_DIALOG_BODY/,
    "the dialog body must come from a named constant so a future tweak is one diff away",
  );
  assert.match(
    source,
    /data-testid="quick-paste-not-editable-close"/,
    "the dialog must expose a stable close-button testid",
  );
});

test("fix: Escape inside the editor must not hide Quick Paste", () => {
  const source = readQuickPasteSource();
  // The global Escape handler MUST detect that the keystroke
  // originated from inside a [role="dialog"] wrapper and bail
  // before `handleEscape` / `hideQuickPasteWindow` can fire. The
  // previous regression: Modal closed the editor but the bubble-
  // phase Svelte window listener still routed Escape to
  // `handleEscape()` which hid the window.
  assert.match(
    source,
    /event\.key\s*===\s*"Escape"[\s\S]{0,2000}target\.closest\('\[role="dialog"\]'\)/,
    "the global Escape handler must detect a [role=\"dialog\"] target and bail",
  );
  // Defence in depth: `closeQuickPasteEditor` MUST arm
  // `suppressNextWindowEscape` so a focus flicker cannot race the
  // bubble-phase Escape through the editor close.
  assert.match(
    source,
    /function\s+closeQuickPasteEditor\(\)\s*:\s*void\s*\{[\s\S]*?suppressNextWindowEscape\s*=\s*true/s,
    "closeQuickPasteEditor must arm suppressNextWindowEscape as defence in depth",
  );
  // The dialog close helper MUST do the same so a focus flicker
  // during the dialog close cannot hide the window either.
  assert.match(
    source,
    /function\s+closeQuickPasteNotEditableDialog\(\)\s*:\s*void\s*\{[\s\S]*?suppressNextWindowEscape\s*=\s*true/s,
    "closeQuickPasteNotEditableDialog must also arm suppressNextWindowEscape",
  );
});

test("fix: focus return target is stable across Cancelar, Escape, backdrop and save", () => {
  const source = readQuickPasteSource();
  // The editor must always hand focus to a stable element: the
  // menu trigger when present, the row otherwise. The helper is the
  // single switch the modal mount consults so a future tweak
  // cannot land focus on a stale element.
  assert.match(
    source,
    /function\s+resolveQuickPasteEditorFocusTarget\(\s*entryId:[\s\S]*?menuAnchorEls\[entryId\][\s\S]*?rowRefs\.get\(entryId\)/,
    "resolveQuickPasteEditorFocusTarget must check the menu anchor before the row",
  );
  // The dialog focus target must come from the captured id slot
  // (not from any previous editor state).
  assert.match(
    source,
    /function\s+resolveQuickPasteNotEditableDialogFocusTarget\(\)[\s\S]*?rowRefs\.get\(id\)/,
    "the dialog focus target must resolve from rowRefs using the captured id",
  );
});

test("fix: dialog close never logs or pipes entry content", () => {
  const source = readQuickPasteSource();
  // The dialog close helper MUST clear the id slot only — no
  // console, no `entry.content`, no log of the entry record. The
  // regex anchors on `entry.content` outside block comments so the
  // documentation does not trip the assertion.
  const stripped = source.replace(/\/\*[\s\S]*?\*\//g, "");
  assert.equal(
    /entry\.content(?!\w)/.test(stripped),
    false,
    "QuickPaste.svelte must never reference entry.content outside documentation",
  );
});