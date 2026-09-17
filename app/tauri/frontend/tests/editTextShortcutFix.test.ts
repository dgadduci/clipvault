/**
 * Source-level regression coverage for the
 * `editable-text-shortcut-fix` capability.
 *
 * The change fixes the bug the previous
 * `editable-text-captures` capability left on disk: the per-card
 * matcher only accepted `ctrlKey` so the shortcut was a no-op on
 * macOS and on Linux cards the user had selected without focusing
 * the `<article>`. The follow-up manual bug report added the
 * target-identity contract: after editing capture A and selecting
 * capture B the shortcut must edit B, never A.
 *
 * The tests below pin both layers of the new architecture:
 *
 *   - the platform-aware matcher / labels live in
 *     `lib/editTextShortcut.ts` so the listener, the visible hint
 *     and the `aria-keyshortcuts` value cannot drift apart;
 *   - App.svelte installs ONE document-level capture-phase listener
 *     that owns the keyboard matcher, resolves the entry from the
 *     focused card or the rail's `selectedEntryId`, validates the
 *     eligibility predicate, gates interactive targets and dispatches
 *     a `clipvault:edit-text-shortcut` custom event with an explicit
 *     `{ entryId }` payload;
 *   - HistoryCardRail forwards the custom event to the matching
 *     card's `<article>` so the card receives exactly one
 *     `card-edit-text-shortcut` event per shortcut press, and
 *     broadcasts a no-payload `card-edit-text-shortcut-close` on
 *     every other mounted card so the previous capture's modal
 *     cannot survive a target change;
 *   - HistoryCard.svelte opens the editor through the same
 *     `openTextEditor` entry point the menu item uses, validates
 *     the dispatched `entryId` against `entry.id`, and honours the
 *     close broadcast by flipping `textEditorOpen` back to `false`
 *     only when the modal was actually open;
 *   - EntryTextEditorModal.svelte re-seeds the draft and the
 *     baseline when `entry.id` changes so a target change never
 *     inherits the previous capture's draft, baseline, accessible
 *     ids or focus;
 *   - the menu hint is platform-aware (`⌘E` on macOS,
 *     `Ctrl+E` everywhere else) and the `aria-keyshortcuts`
 *     attribute mirrors the matcher (`Meta+E` / `Control+E`);
 *   - drag-and-drop baselines (`data-testid`, `data-entry-id`,
 *     `draggable`, pointer capture, opaque ID payload) and the
 *     `pointerDragAndDrop.ts` singleton stay intact.
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

const helperSource = stripComments(
  loadSource("src/lib/editTextShortcut.ts"),
);
const cardSource = stripComments(loadSource("src/HistoryCard.svelte"));
const railSource = stripComments(loadSource("src/HistoryCardRail.svelte"));
const appSource = stripComments(loadSource("src/App.svelte"));
const pointerDragSource = stripComments(
  loadSource("src/lib/pointerDragAndDrop.ts"),
);
const modalSource = stripComments(
  loadSource("src/EntryTextEditorModal.svelte"),
);

// ---------------------------------------------------------------------------
// 1. Pure matcher / label helper is the single source of truth.
// ---------------------------------------------------------------------------

test("editTextShortcut helper exposes the platform-aware label pair", () => {
  assert.match(
    helperSource,
    /export\s+function\s+editTextShortcutPlatform\b/,
    "the helper must export the platform resolver",
  );
  assert.match(
    helperSource,
    /export\s+function\s+matchesEditTextShortcut\b/,
    "the helper must export the matcher",
  );
  assert.match(
    helperSource,
    /export\s+function\s+editTextShortcutLabel\b/,
    "the helper must export the visible label",
  );
  assert.match(
    helperSource,
    /export\s+function\s+editTextShortcutKeyAttribute\b/,
    "the helper must export the aria-keyshortcuts value",
  );
  // The helper must consult the search-shortcut platform resolver
  // so a tweak to the diagnostics string only has to land in
  // `platformFromDiagnostics`.
  assert.match(
    helperSource,
    /searchShortcutPlatform/,
    "the helper must delegate to searchShortcutPlatform",
  );
});

test("editTextShortcut helper rejects Alt / Shift / wrong modifier / case variants", () => {
  // The matcher body pins the documented modifier table so a
  // future tweak cannot accidentally accept `Shift+E` or a
  // mixed `Ctrl+Cmd+E` combo.
  const matcher = helperSource.match(
    /export\s+function\s+matchesEditTextShortcut[\s\S]*?\n\}/,
  );
  assert.notEqual(matcher, null, "the matcher must exist");
  const body = matcher?.[0] ?? "";
  assert.match(body, /event\.altKey\s*\|\|\s*event\.shiftKey/);
  assert.match(body, /event\.key\s*\?\?\s*""\)\.toLowerCase/);
  assert.match(
    body,
    /Boolean\(event\.metaKey\)\s*&&\s*!event\.ctrlKey/,
    "macOS branch must require metaKey and forbid ctrlKey",
  );
  assert.match(
    body,
    /Boolean\(event\.ctrlKey\)\s*&&\s*!event\.metaKey/,
    "Linux branch must require ctrlKey and forbid metaKey",
  );
});

// ---------------------------------------------------------------------------
// 2. App-level document listener owns the keyboard matcher.
// ---------------------------------------------------------------------------

test("App installs one document-level capture-phase keydown listener", () => {
  // The shell must keep the existing search / preview listeners
  // intact and add the edit-text shortcut branch to the same
  // listener; a second keydown subscription on `document` would
  // double the matcher work and contradict the "single shared
  // listener" requirement of the spec.
  const searchSubscribe = appSource.match(
    /document\.addEventListener\(\s*["']keydown["']\s*,\s*onSearchShortcutKeydown\s*,\s*true\s*\)/,
  );
  assert.notEqual(
    searchSubscribe,
    null,
    "App.svelte must keep the existing keydown listener registration",
  );
  // The shell must NOT install a second keydown subscription: the
  // spec's "only one shared listener handles the shortcut"
  // scenario requires the matcher to run inside the existing
  // capture-phase listener, not in a new document / window handler.
  const extraListeners = appSource.match(
    /document\.addEventListener\(\s*["']keydown["']/g,
  );
  assert.equal(
    extraListeners?.length ?? 0,
    1,
    "App.svelte must install exactly one document keydown subscription",
  );
});

test("App-level keydown listener delegates to the edit-text matcher", () => {
  // The matcher must run inside the same `onSearchShortcutKeydown`
  // function the search-shortcut branch already installs so the
  // capture-phase listener keeps a single subscription.
  const handler = appSource.match(
    /function\s+onSearchShortcutKeydown[\s\S]*?\n  \}\n/,
  );
  assert.notEqual(
    handler,
    null,
    "App.svelte must declare onSearchShortcutKeydown as the single shell handler",
  );
  const body = handler?.[0] ?? "";
  assert.match(
    body,
    /matchesEditTextShortcut\s*\(\s*event\s*,\s*editTextShortcut\s*\)/,
    "the shell handler must consult the helper matcher with the resolved platform",
  );
});

test("App-level matcher resolves the selected entry before stale card focus", () => {
  // A pointer selection is authoritative: after editing A, browser
  // focus may remain on A while the user selects B. The resolver
  // must not reopen the stale capture from focus. A focused card is
  // still the fallback when the rail has no selection.
  const resolve = appSource.match(
    /function\s+resolveEditTextShortcutEntryId[\s\S]*?\n  \}/,
  );
  assert.notEqual(
    resolve,
    null,
    "App.svelte must declare the entry-id resolver helper",
  );
  const body = resolve?.[0] ?? "";
  assert.match(
    body,
    /closest\(\s*['"]\[data-testid=["']history-card["']\]['"]\s*\)/,
    "the resolver must walk the focus path through the data-testid selector",
  );
  assert.match(
    body,
    /data-entry-id/,
    "the resolver must read the card's data-entry-id attribute",
  );
  assert.match(
    body,
    /resolveEditTextShortcutTargetId\(\s*\n?\s*railSelectedEntryId\s*,/,
    "the resolver must pass the rail selection as the authoritative id",
  );
  assert.match(
    body,
    /focusedCardEntryId/,
    "the resolver must keep the focused card as a fallback",
  );
});

test("App-level matcher gates interactive targets and dialogs", () => {
  // The shell listener must short-circuit when the target is an
  // input / textarea / contenteditable / button / menu / menuitem /
  // title-editor / collection chip / dialog / confirm-dialog so the
  // shortcut never steals focus from a typing surface and never
  // re-opens a modal the user is already editing.
  const handler = appSource.match(
    /function\s+onSearchShortcutKeydown[\s\S]*?\n  \}\n/,
  );
  const body = handler?.[0] ?? "";
  const branch = body.match(
    /if\s*\(\s*matchesEditTextShortcut[\s\S]*?\}\s*\n  \}/,
  );
  assert.notEqual(branch, null, "the edit-text branch must exist");
  const branchBody = branch?.[0] ?? "";
  for (const selector of [
    "HTMLInputElement",
    "HTMLTextAreaElement",
    "isContentEditable",
    "role='menu'",
    "role='menuitem'",
    ".menu",
    ".title-input",
    ".collection-chips",
    "role='dialog'",
    "confirm-dialog",
  ]) {
    assert.ok(
      branchBody.includes(selector),
      `the edit-text branch must gate against ${selector}`,
    );
  }
});

test("App-level matcher validates the eligibility predicate before opening the editor", () => {
  // An image or rich-text row must never reach the modal even when
  // the user happens to have it focused. The shell listener routes
  // the eligibility check through the existing
  // `isEditableTextEntry` predicate the menu / modal already
  // consult so the front of house stays the single source of
  // truth.
  const handler = appSource.match(
    /function\s+onSearchShortcutKeydown[\s\S]*?\n  \}\n/,
  );
  const body = handler?.[0] ?? "";
  const branch = body.match(
    /if\s*\(\s*matchesEditTextShortcut[\s\S]*?\}\s*\n  \}/,
  );
  const branchBody = branch?.[0] ?? "";
  assert.match(
    branchBody,
    /isEditableTextEntry\s*\(/,
    "the edit-text branch must consult the eligibility predicate",
  );
  assert.match(
    branchBody,
    /event\.preventDefault\(\)/,
    "the edit-text branch must call preventDefault only when accepted",
  );
  assert.match(
    branchBody,
    /stopImmediatePropagation/,
    "the edit-text branch must stop propagation so the per-card fallback never fires",
  );
  assert.match(
    branchBody,
    /clipvault:edit-text-shortcut/,
    "the edit-text branch must dispatch the custom DOM event on document",
  );
});

test("ineligible shortcut targets are consumed and show a notice instead of reopening the last editor", () => {
  // Returning from the document listener is not enough: the keydown
  // would continue to the card that still owns DOM focus and its
  // local fallback could reopen the last text editor. The ineligible
  // branch must consume the event before showing a notice.
  const guard = appSource.match(
    /event\.preventDefault\(\);\s*event\.stopImmediatePropagation\(\);\s*if\s*\(\s*!isEditableTextEntry\(targetEntry\)\s*\)\s*\{[\s\S]*?\n      \}/,
  );
  assert.notEqual(
    guard,
    null,
    "the listener must consume the shortcut before handling an ineligible entry",
  );
  const body = guard?.[0] ?? "";
  assert.match(
    body,
    /nonEditableTextNotice\s*=\s*\{/,
    "the ineligible branch must open the informational notice",
  );
  assert.match(
    body,
    /contentType:\s*targetEntry\.content_type/,
    "the notice may carry only the entry type, never capture content",
  );
  assert.doesNotMatch(
    body,
    /clipvault:edit-text-shortcut/,
    "an ineligible entry must not dispatch the editor-opening event",
  );
  assert.match(
    appSource,
    /data-testid="non-editable-text-notice-message"/,
    "the user-facing notice must have a stable regression anchor",
  );
  assert.match(
    appSource,
    /data-testid="non-editable-text-notice-close"/,
    "the notice must expose an explicit close action",
  );
  assert.match(
    appSource,
    /title="Captura no editable"/,
    "the notice must identify the capture as non-editable",
  );
  assert.match(
    appSource,
    /function\s+closeNonEditableTextNotice[\s\S]*?nonEditableTextNotice\s*=\s*null/,
    "closing the notice must clear only notice state",
  );
});

// ---------------------------------------------------------------------------
// 3. The rail forwards the request to the matching card's article.
// ---------------------------------------------------------------------------

test("HistoryCardRail listens for the custom event and dispatches on the matching card", () => {
  // The rail must own the only document-level listener for the
  // custom event the shell emits so each card receives exactly one
  // forwarded event per shortcut press.
  assert.match(
    railSource,
    /document\.addEventListener\(\s*["']clipvault:edit-text-shortcut["']/,
    "the rail must subscribe to the shell's custom event",
  );
  const handler = railSource.match(
    /function\s+handleEditTextShortcutRequest[\s\S]*?\n  \}/,
  );
  assert.notEqual(
    handler,
    null,
    "the rail must declare the forwarding handler",
  );
  const body = handler?.[0] ?? "";
  assert.match(
    body,
    /cardEls\.get\(/,
    "the handler must consult the rail's card element registry",
  );
  // The per-card event MUST carry an explicit `{ entryId }` payload
  // so the receiving card can validate the id against its own
  // `entry.id` and discard stale, duplicate or mismatched requests
  // without ever opening a wrong modal.
  assert.match(
    body,
    /detail:\s*\{\s*entryId:\s*detail\.entryId\s*\}/,
    "the per-card event must carry the { entryId } payload the shell forwarded",
  );
  assert.match(
    body,
    /dispatchEvent\(\s*new\s+CustomEvent\(\s*["']card-edit-text-shortcut["']/,
    "the handler must dispatch the per-card event on the article element",
  );
  // The custom DOM event the rail emits must travel exactly once:
  // `bubbles: false` keeps it from re-entering the rail's document
  // listener and prevents a stray rail from catching the per-card
  // hop.
  assert.match(
    body,
    /bubbles:\s*false/,
    "the per-card event must not bubble so the rail cannot recurse",
  );
  // Before opening a new modal the rail MUST broadcast a close
  // event on every OTHER mounted card so the contract "at most one
  // active edit modal" is enforced without the rail having to
  // track per-card modal state.
  assert.match(
    body,
    /card-edit-text-shortcut-close/,
    "the rail must dispatch a close event so a stale modal cannot survive a target change",
  );
});

test("HistoryCardRail removes the custom-event listener on unmount", () => {
  // The teardown path the rail owns must remove the listener so a
  // remount never leaks a handler and a destroyed rail never
  // re-opens the editor for a stale entry id.
  const onDestroy = railSource.match(
    /onDestroy\(\(\)\s*=>\s*\{[\s\S]*?\}\)/,
  );
  assert.notEqual(onDestroy, null, "the rail must declare an onDestroy block");
  const body = onDestroy?.[0] ?? "";
  assert.match(
    body,
    /detachWindow\?\.\(\)/,
    "the teardown must call the existing detach helper",
  );
  assert.match(
    railSource,
    /document\.removeEventListener\(\s*["']clipvault:edit-text-shortcut["'][\s\S]*?handleEditTextShortcutRequest/,
    "the detach helper must remove the custom-event listener",
  );
});

// ---------------------------------------------------------------------------
// 4. The card opens the modal through the same single entry point.
// ---------------------------------------------------------------------------

test("HistoryCard listens for the per-card event and reuses openTextEditor", () => {
  // The card must listen through `addEventListener` (the Svelte
  // type checker does not know the custom event name) and the
  // listener MUST reuse `openTextEditor` so the modal lifecycle
  // stays single-sourced.
  assert.match(
    cardSource,
    /addEventListener\(\s*["']card-edit-text-shortcut["']\s*,\s*handleCardEditTextShortcut/,
    "the card must register a card-edit-text-shortcut listener",
  );
  assert.match(
    cardSource,
    /function\s+handleCardEditTextShortcut[\s\S]*?openTextEditor\(\)/,
    "the card listener must delegate to openTextEditor",
  );
  // The card MUST validate the dispatched `entryId` against its own
  // `entry.id` so a stale, duplicate or mismatched request cannot
  // leak into a wrong modal.
  const handler = cardSource.match(
    /function\s+handleCardEditTextShortcut[\s\S]*?\n  \}/,
  );
  assert.notEqual(handler, null, "the card must declare a handler for the per-card event");
  const body = handler?.[0] ?? "";
  assert.match(
    body,
    /entryId\s*!==\s*entry\.id/,
    "the handler must reject an entryId that does not match the card's entry.id",
  );
  // The teardown must remove the listener so a remount cannot
  // keep firing against a detached DOM node.
  assert.match(
    cardSource,
    /removeEventListener\(\s*["']card-edit-text-shortcut["']\s*,\s*handleCardEditTextShortcut/,
    "the card must remove the listener on destroy",
  );
  // The card MUST listen to the per-card close event so the rail
  // can flip `textEditorOpen` back to false when a different entry
  // becomes the new target.
  assert.match(
    cardSource,
    /addEventListener\(\s*["']card-edit-text-shortcut-close["']\s*,\s*handleCardEditTextShortcutClose/,
    "the card must register a card-edit-text-shortcut-close listener",
  );
  assert.match(
    cardSource,
    /function\s+handleCardEditTextShortcutClose[\s\S]*?textEditorOpen\s*=\s*false/,
    "the close handler must reset textEditorOpen so the previous capture's modal cannot survive the target change",
  );
  assert.match(
    cardSource,
    /removeEventListener\(\s*["']card-edit-text-shortcut-close["']\s*,\s*handleCardEditTextShortcutClose/,
    "the card must remove the close listener on destroy",
  );
  // The card MUST NOT install its own document or window keydown
  // listener for the shortcut: that would reintroduce the
  // per-card matcher the new design removed.
  assert.equal(
    cardSource.match(/document\.addEventListener\(\s*["']keydown/),
    null,
    "HistoryCard must not install document-level keydown listeners",
  );
  assert.equal(
    cardSource.match(/window\.addEventListener\(\s*["']keydown/),
    null,
    "HistoryCard must not install window-level keydown listeners",
  );
});

test("EDIT_TEXT_SHORTCUT_TESTID constant stays stable so the menu testid cannot drift", () => {
  // The testid is the regression anchor the rest of the suite
  // pins; the platform-aware label can change freely but the
  // testid the rail / external tests look up must not.
  assert.match(
    cardSource,
    /EDIT_TEXT_SHORTCUT_TESTID\s*=\s*"history-card-edit-text-shortcut"/,
    "the shortcut testid constant must remain stable",
  );
});

// ---------------------------------------------------------------------------
// 5. Drag-and-drop and pointer baselines stay intact.
// ---------------------------------------------------------------------------

test("drag-and-drop baselines stay intact across the change", () => {
  // The change MUST NOT touch the pointer drag controller, the
  // protected card attributes or the singleton drag state.
  assert.match(cardSource, /data-testid="history-card"/);
  assert.match(cardSource, /data-entry-id/);
  assert.match(cardSource, /draggable="false"/);
  assert.match(cardSource, /data-testid="history-card-menu-trigger"/);
  assert.match(cardSource, /data-testid="history-card-pin"/);
  assert.match(cardSource, /data-testid="history-card-title"/);
  assert.match(
    pointerDragSource,
    /let pendingDrag:\s*PendingPointerDrag\s*\|\s*null\s*=\s*null/,
    "the pointer drag singleton must keep its state shape",
  );
});

// ---------------------------------------------------------------------------
// 6. Target identity contract: each request transports only `{ entryId }`,
//    the rail forwards only to the matching card and broadcasts a close
//    before opening, the card validates the id and discards mismatches,
//    and the modal never reuses the previous capture's draft / baseline.
// ---------------------------------------------------------------------------

test("rail forwards the per-card event with an explicit entryId payload", () => {
  // The dispatch MUST carry `{ entryId }` so the receiving card can
  // compare the value against its own `entry.id`; a payload-less
  // event would let the rail bypass the card-side validation and
  // reopen a stale modal on the first card the dispatch reaches.
  const handler = railSource.match(
    /function\s+handleEditTextShortcutRequest[\s\S]*?\n  \}/,
  );
  assert.notEqual(handler, null, "the rail must declare the forwarding handler");
  const body = handler?.[0] ?? "";
  assert.match(
    body,
    /detail:\s*\{\s*entryId:\s*detail\.entryId\s*\}/,
    "the rail must forward the { entryId } payload to the per-card event",
  );
  // The rail MUST never forward capture content, snippets, hashes
  // or asset references — only the opaque id the shell resolved.
  assert.doesNotMatch(
    body,
    /detail:\s*\{[^}]*content[^}]*\}/,
    "the rail must never carry capture content in the per-card event payload",
  );
});

test("rail broadcasts a close event before opening a new modal", () => {
  // The "at most one active modal" contract requires the rail to
  // close every OTHER mounted card's edit modal before the new
  // target's modal opens. The rail iterates the cardEls registry
  // and dispatches a no-payload close event on each non-target
  // article; the receiving card flips `textEditorOpen` back to
  // false only if it was actually open.
  const handler = railSource.match(
    /function\s+handleEditTextShortcutRequest[\s\S]*?\n  \}/,
  );
  const body = handler?.[0] ?? "";
  assert.match(
    body,
    /for\s*\(\s*const\s*\[\s*mountedId[\s\S]*?cardEls\s*\)/,
    "the rail must iterate the cardEls registry to broadcast the close event",
  );
  assert.match(
    body,
    /card-edit-text-shortcut-close/,
    "the rail must emit the documented close event name so cards can listen for it",
  );
  // The close event must skip the target card — closing the card
  // we are about to open would race the open dispatch and leave the
  // modal in an inconsistent state.
  assert.match(
    body,
    /if\s*\(\s*mountedId\s*===\s*detail\.entryId\s*\)\s*continue/,
    "the rail must skip the target card when broadcasting the close event",
  );
  // The close event must travel without a payload so a wrong
  // target cannot leak any capture content into a non-target card.
  const closeDispatch = body.match(
    /dispatchEvent\(\s*new\s+CustomEvent\(\s*["']card-edit-text-shortcut-close["'][\s\S]*?\)/,
  );
  assert.notEqual(
    closeDispatch,
    null,
    "the rail must dispatch the close event on every other card",
  );
  assert.doesNotMatch(
    closeDispatch?.[0] ?? "",
    /detail:\s*\{/,
    "the close event must not carry any payload — the rail never forwards capture content",
  );
});

test("rail drops requests that miss a mounted card without leaving a stale modal open", () => {
  // If the entry id does not map to a mounted article, the rail
  // silently drops the request. The rail MUST NOT keep a stale
  // modal open by mistake — a future caller can re-issue the
  // request once the entry returns to the visible scope.
  const handler = railSource.match(
    /function\s+handleEditTextShortcutRequest[\s\S]*?\n  \}/,
  );
  const body = handler?.[0] ?? "";
  assert.match(
    body,
    /if\s*\(\s*!card\s*\)\s*return/,
    "the rail must drop requests whose entryId does not map to a mounted card",
  );
});

test("card validates the entryId before opening its modal", () => {
  // The card listener MUST read `event.detail.entryId` and reject
  // any value that does not match its own `entry.id`. A stale
  // dispatch (the rail was rebuilt with a different cardEls
  // registry, a future caller reused the helper, a remount kept a
  // detached listener) cannot leak into a wrong modal.
  const handler = cardSource.match(
    /function\s+handleCardEditTextShortcut[\s\S]*?\n  \}/,
  );
  const body = handler?.[0] ?? "";
  assert.match(
    body,
    /\(event\s+as\s+CustomEvent[\s\S]*?\)\.detail/,
    "the handler must read the dispatched entryId from event.detail",
  );
  assert.match(
    body,
    /typeof\s+detail\.entryId\s*!==\s*["']number["']/,
    "the handler must reject non-numeric entryIds",
  );
  assert.match(
    body,
    /detail\.entryId\s*!==\s*entry\.id/,
    "the handler must reject entryIds that do not match this card's entry.id",
  );
  // The handler MUST also drop a request whose detail is missing
  // or malformed; the rail already validates the payload before
  // dispatching, but the card-side guard is the last line of
  // defence.
  assert.match(
    body,
    /if\s*\(\s*!detail\s*\|\|\s*typeof\s+detail\.entryId\s*!==\s*["']number["']\s*\)\s*return/,
    "the handler must early-return when detail.entryId is malformed",
  );
  // Only the validation-pass branch reaches `openTextEditor()`.
  const tail = body.split("detail.entryId !== entry.id").pop() ?? "";
  assert.match(
    tail,
    /openTextEditor\(\)/,
    "the only path to openTextEditor must run after the id validation",
  );
});

test("card resets textEditorOpen when the rail broadcasts a close for a different target", () => {
  // The rail broadcasts `card-edit-text-shortcut-close` on every
  // non-target article so the previous capture's modal cannot
  // survive a target change. The handler is a no-op when the
  // card never opened its modal so the close event is a safe
  // broadcast against every mounted card.
  const handler = cardSource.match(
    /function\s+handleCardEditTextShortcutClose[\s\S]*?\n  \}/,
  );
  assert.notEqual(handler, null, "the card must declare a close handler");
  const body = handler?.[0] ?? "";
  assert.match(
    body,
    /if\s*\(\s*!textEditorOpen\s*\)\s*return/,
    "the close handler must early-return when the card never opened its modal",
  );
  assert.match(
    body,
    /textEditorOpen\s*=\s*false/,
    "the close handler must reset textEditorOpen so the previous capture's modal cannot survive a target change",
  );
  // The card MUST also remove the close listener on destroy so a
  // remount cannot leak handlers and a destroyed card cannot
  // silently reopen its modal against a future dispatch.
  assert.match(
    cardSource,
    /removeEventListener\(\s*["']card-edit-text-shortcut-close["']\s*,\s*handleCardEditTextShortcutClose/,
    "the card must remove the close listener on destroy",
  );
});

test("modal re-seeds draft and baseline when the target entry changes", () => {
  // When the user moves from capture A to capture B and the
  // shortcut reopens the editor, the modal must show B's content
  // and the persisted baseline must come from B. The reactive
  // block uses `lastOpenedEntryId !== entry.id` to detect the
  // transition and re-seeds the draft / baseline / ids.
  const reactive = modalSource.match(
    /\$:[\s\S]*?lastOpenedEntryId\s*!==\s*entry\.id[\s\S]*?baselineEntry\s*=\s*null/,
  );
  assert.notEqual(
    reactive,
    null,
    "the modal must keep the open/close reactive block that detects target changes",
  );
  const body = reactive?.[0] ?? "";
  assert.match(body, /draft\s*=\s*entry\.content/);
  assert.match(body, /baselineEntry\s*=\s*entry/);
  assert.match(body, /lastOpenedEntryId\s*=\s*entry\.id/);
  // The close branch MUST reset the stamp so the next open cycle
  // for any entry seeds a fresh draft.
  assert.match(body, /lastOpenedEntryId\s*=\s*null/);
  assert.match(body, /baselineEntry\s*=\s*null/);
  // The accessible ids (`titleId` / `editorId`) are derived from
  // `entry.id` so a target change automatically refreshes them.
  assert.match(modalSource, /titleId\s*=\s*[`]entry-text-editor-title-\$\{entry\.id\}[`]/);
  assert.match(modalSource, /editorId\s*=\s*[`]entry-text-editor-\$\{entry\.id\}[`]/);
  assert.match(modalSource, /data-entry-id=\{entry\.id\}/);
});

test("modal never carries capture content in events, attributes or logs", () => {
  // Privacy: the modal must never emit or echo the draft / entry
  // content through events, attributes or logs. The bridge is the
  // single channel that ever carries the draft, and the request
  // payload is the typed `{ id, content }` shape the bridge
  // already accepts.
  assert.doesNotMatch(
    modalSource,
    /dispatch\(\s*["']close["']\s*,\s*\{/,
    "the close dispatcher must not carry a payload",
  );
  assert.doesNotMatch(
    modalSource,
    /console\.(log|info|warn|error)\([^)]*(entry\.content|draft)/,
    "the modal must never log capture content",
  );
  // The data-entry-id attribute on the modal container is the
  // only capture-related attribute the markup exposes, and it
  // only carries the opaque id — never the content.
  const dataEntryId = modalSource.match(/data-entry-id=\{entry\.id\}/);
  assert.notEqual(
    dataEntryId,
    null,
    "the modal must surface the entry id for test inspection",
  );
});

test("card menu payload does not leak capture content into the shortcut request", () => {
  // The card-edit-text-shortcut request must carry only the entry
  // id the shell resolved. A future change that accidentally
  // appends `entry.content` or any other sensitive payload to the
  // dispatch would break the privacy contract and the regression
  // suite would catch it.
  assert.doesNotMatch(
    cardSource,
    /card-edit-text-shortcut["'][^}]*detail[^}]*entry\.content/,
    "the card must never embed entry content in the shortcut dispatch",
  );
  assert.doesNotMatch(
    railSource,
    /card-edit-text-shortcut-close["'][^}]*detail\s*:/,
    "the close dispatch must never carry a payload",
  );
});

test("drag-and-drop / card baselines survive the identity contract change", () => {
  // The pointer drag controller and the protected card attributes
  // MUST stay intact across the A→B change. The card surface
  // keeps `data-testid`, `data-entry-id`, `draggable="false"`,
  // the pin / menu / title affordances and the singleton drag
  // state the spec protects.
  assert.match(cardSource, /data-testid="history-card"/);
  assert.match(cardSource, /data-entry-id/);
  assert.match(cardSource, /draggable="false"/);
  assert.match(cardSource, /data-testid="history-card-menu-trigger"/);
  assert.match(cardSource, /data-testid="history-card-pin"/);
  assert.match(cardSource, /data-testid="history-card-title"/);
  assert.match(
    pointerDragSource,
    /let pendingDrag:\s*PendingPointerDrag\s*\|\s*null\s*=\s*null/,
  );
  assert.match(pointerDragSource, /setPointerCapture/);
  assert.match(pointerDragSource, /mousedown/);
  assert.match(pointerDragSource, /mousemove/);
  assert.match(pointerDragSource, /mouseup/);
});
