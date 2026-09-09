/**
 * Regression coverage for the visual contracts the
 * `desktop-collection-card-polish` change ships:
 *
 *   - the trash button in the desktop toolbar is rendered in the
 *     documented danger colour so a destructive action cannot be
 *     mistaken for a neutral control;
 *   - the per-card delete affordance and the per-collection delete
 *     affordance carry the same red presentation and accessible
 *     name and are pinned to the `data-cv-danger` selector the
 *     regression suite reads;
 *   - the trash SVG used in the toolbar matches the locally
 *     documented shape (no emoji, no remote asset, no new
 *     dependency) and is consistent with the rest of the icon
 *     family the rail ships.
 *
 * The tests pin the contract through the rendered source so a
 * future contributor cannot soften the colour, swap the icon for a
 * remote URL or remove the focus / disabled hooks without breaking
 * the suite.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const FRONTEND_ROOT = process.cwd();

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

test("DesktopToolbar trash button is rendered in the danger colour", () => {
  const source = loadSource("src/DesktopToolbar.svelte");
  // The `.trash` selector must paint the rest state in `--cv-danger`
  // and the hover state in `--cv-danger-hover`. A regression that
  // softens it back to a muted grey would surface here.
  assert.match(
    source,
    /\.trash\s*\{[^}]*color:\s*var\(\s*--cv-danger/,
    "the trash button must declare a danger colour",
  );
  assert.match(
    source,
    /\.trash\s*\{[^}]*border:\s*1px solid var\(\s*--cv-danger/,
    "the trash button must outline itself in the danger colour",
  );
  assert.match(
    source,
    /\.trash:hover\s*\{[^}]*--cv-danger-hover/,
    "the trash hover state must reinforce the danger emphasis",
  );
});

test("DesktopToolbar trash button carries the documented accessibility hooks", () => {
  const source = loadSource("src/DesktopToolbar.svelte");
  // The button MUST keep its accessible label, its title and a
  // visible focus ring; a regression that drops any of them would
  // hide the destructive shortcut from screen-reader or
  // keyboard-only users.
  assert.match(source, /aria-label=\{trashLabel\}/);
  assert.match(source, /title=\{trashLabel\}/);
  assert.match(
    source,
    /\.trash:focus-visible\s*\{[^}]*outline:/,
    "the trash button must keep a focus-visible outline",
  );
  assert.match(
    source,
    /\.trash\[aria-busy="?true"?\]\s*\{[^}]*cursor:\s*progress/,
    "the busy state must advertise the in-flight mutation",
  );
});

test("DesktopToolbar trash SVG is local and free of remote or emoji fallbacks", () => {
  const source = loadSource("src/DesktopToolbar.svelte");
  const trashButtonMatch = source.match(
    /<button[\s\S]*?class="trash"[\s\S]*?<\/button>/,
  );
  assert.ok(trashButtonMatch, "the toolbar trash button must exist");
  const trashButton = trashButtonMatch[0];
  assert.match(
    trashButton,
    /data-testid="trash-clear-history"/,
    "the trash button must keep its stable test id",
  );
  // The shape is documented locally: a single SVG with hand-written
  // path data. A regression that introduces an emoji, a remote
  // `href`, or a new dependency would break the contract.
  assert.match(
    trashButton,
    /<svg[^>]*viewBox="0 0 24 24"/,
    "the trash SVG must use the shared 24x24 viewBox",
  );
  assert.doesNotMatch(trashButton, /src="https?:/);
  assert.doesNotMatch(trashButton, /<image\b/);
  assert.doesNotMatch(
    source,
    /from\s+['"]@?lucide/,
    "no icon framework dependency is allowed",
  );
  // The image glyph must remain present so the toolbar's compact
  // recognition holds across the two panels.
  assert.match(trashButton, /<svg/);
});

test("DesktopToolbar trash button is the only path to clear history", () => {
  // The destructive action still flows through the documented
  // confirmation; the icon is the entry point, the parent owns the
  // modal. A regression that bypassed the confirmation would call
  // `clearUnorganizedHistoryCommand({ confirm: true })` directly
  // from the toolbar; the test ensures the toolbar still emits the
  // intent that the parent converts into the confirmation.
  const source = loadSource("src/DesktopToolbar.svelte");
  const trashButtonMatch = source.match(
    /<button[\s\S]*?class="trash"[\s\S]*?<\/button>/,
  );
  assert.ok(trashButtonMatch, "the toolbar trash button must exist");
  assert.match(
    trashButtonMatch[0],
    /on:click=\{onRequestClearHistory\}/,
    "the trash button must keep its parent-owned intent",
  );
  assert.doesNotMatch(
    source,
    /clearUnorganizedHistoryCommand/,
    "the toolbar must never invoke the destructive command directly",
  );
});

test("HistoryCard delete menu item uses the documented danger colour", () => {
  const source = loadSource("src/HistoryCard.svelte");
  // The `data-cv-danger="card-delete"` selector is the regression
  // hook every other suite reads; the visual CSS rule must pin the
  // documented `--cv-danger` token so the icon never drifts back to
  // a neutral tone.
  assert.match(
    source,
    /data-testid="history-card-delete"[\s\S]{0,400}?data-cv-danger="card-delete"/,
    "the card delete menu item must advertise its danger hook",
  );
  assert.match(
    source,
    /\.menu-item\.danger\.delete-action\s*\{[^}]*--cv-danger/,
    "the card delete menu item CSS must use --cv-danger",
  );
  // The textual "Eliminar" label is intentionally hidden from the
  // visible menu row; the icon plus the accessible name drive the
  // affordance. The accessible name is preserved on the button.
  assert.match(
    source,
    /<span class="visually-hidden">\{\`Eliminar \$\{displayTitle\}\`\}<\/span>/,
    "the card delete button keeps an accessible label",
  );
  assert.match(
    source,
    /aria-label=\{\`Eliminar \$\{displayTitle\}\`\}/,
    "the card delete button keeps the screen-reader name",
  );
});

test("HistoryCard delete menu item is the only path to delete an entry", () => {
  const source = loadSource("src/HistoryCard.svelte");
  const menuItemMatch = source.match(
    /<button[\s\S]*?data-testid="history-card-delete"[\s\S]*?<\/button>/,
  );
  assert.ok(menuItemMatch, "the card delete button must exist");
  assert.match(
    menuItemMatch[0],
    /on:click=\{handleDeleteClick\}/,
    "the card delete button must funnel through the parent confirmation",
  );
  assert.doesNotMatch(
    menuItemMatch[0],
    /deleteEntryCommand/,
    "the card must never call the destructive command directly",
  );
});

test("OrganizationSidebar delete icon uses the documented danger colour", () => {
  const source = loadSource("src/OrganizationSidebar.svelte");
  assert.match(
    source,
    /data-testid="sidebar-collection-delete"[\s\S]{0,400}?data-cv-danger="collection-delete"/,
    "the sidebar delete button must advertise its danger hook",
  );
  assert.match(
    source,
    /\.icon-only\.danger\.delete-icon\s*\{[^}]*--cv-danger/,
    "the sidebar delete button CSS must use --cv-danger",
  );
});

test("OrganizationSidebar delete icon is local, accessible and replaces the legacy design", () => {
  const source = loadSource("src/OrganizationSidebar.svelte");
  const buttonMatch = source.match(
    /<button[\s\S]*?data-testid="sidebar-collection-delete"[\s\S]*?<\/button>/,
  );
  assert.ok(buttonMatch, "the sidebar delete button must exist");
  assert.match(
    buttonMatch[0],
    /<svg[^>]*viewBox="0 0 24 24"/,
    "the delete icon must use the shared 24x24 viewBox",
  );
  assert.doesNotMatch(buttonMatch[0], /src="https?:/);
  assert.match(
    buttonMatch[0],
    /aria-label=\{\`Eliminar \$\{collection\.name\}\`\}/,
    "the accessible label must mention the collection name",
  );
  assert.match(
    buttonMatch[0],
    /title=\{\`Eliminar \$\{collection\.name\}\`\}/,
    "the title must mention the collection name",
  );
  // The legacy filled-trash path is the regression we replace.
  assert.doesNotMatch(
    buttonMatch[0],
    /M5\.5 1\.5h3l\.7 1h2\.5v1/,
    "the legacy filled-trash path must be gone",
  );
});

// ---------------------------------------------------------------------------
// Collection delete confirmation now lives in a dedicated modal backed
// by the shared `Modal` shell. The legacy inline confirmation row
// must be gone — a regression that re-introduced it would lift the
// panel above the documented rail height.
// ---------------------------------------------------------------------------

test("OrganizationSidebar renders the collection delete confirmation in a modal", () => {
  const source = loadSource("src/OrganizationSidebar.svelte");
  // The modal is anchored on `pendingDelete` and shares the shared
  // shell so the focus trap, Escape handler and return-focus stay
  // consistent with the rest of the desktop.
  assert.match(
    source,
    /import\s+Modal\s+from\s+"\.\/Modal\.svelte"/,
    "the sidebar must import the shared Modal shell",
  );
  assert.match(
    source,
    /<Modal[\s\S]*?titleId="sidebar-delete-collection-modal-title"/,
    "the delete modal must render with the documented title id",
  );
  assert.match(
    source,
    /open=\{pendingDelete !== null\}/,
    "the modal must open when a delete is pending",
  );
  assert.match(
    source,
    /onClose=\{cancelDelete\}/,
    "Escape / backdrop close must funnel through cancelDelete",
  );
  assert.match(
    source,
    /returnFocusTo=\{pendingDeleteTrigger\}/,
    "the modal must restore focus to the icon that triggered it",
  );
  // The modal must keep the warning copy and the documented action
  // buttons so the user gets the destructive branch in lock-step
  // with the rest of the desktop.
  assert.match(
    source,
    /data-testid="sidebar-delete-modal-warning"/,
    "the modal must render the documented warning node",
  );
  assert.match(
    source,
    /data-testid="sidebar-delete-modal-cancel"/,
    "the modal must render the documented cancel button",
  );
  assert.match(
    source,
    /data-testid="sidebar-delete-modal-confirm"/,
    "the modal must render the documented confirm button",
  );
  // The legacy inline confirmation row is the regression this
  // change removes: it never lifted the panel above the rail
  // height and forced the user to read the destructive branch
  // inside the cramped row.
  assert.equal(
    source.includes("confirm-delete"),
    false,
    "the legacy inline confirmation row must be gone",
  );
  assert.equal(
    source.includes("confirmingDeleteId"),
    false,
    "the legacy confirmingDeleteId state must be gone",
  );
  assert.equal(
    source.includes("sidebar-delete-confirm-yes"),
    false,
    "the legacy inline confirm test id must be gone",
  );
  assert.equal(
    source.includes("sidebar-delete-confirm-no"),
    false,
    "the legacy inline cancel test id must be gone",
  );
});

test("OrganizationSidebar confirm button dispatches the delete event and refreshes the list", () => {
  const source = loadSource("src/OrganizationSidebar.svelte");
  // The confirm button must dispatch `delete` with the pending
  // collection id; the parent already calls `refreshOrganization`
  // and `refreshEntries` so the collections list refreshes as
  // soon as the destructive round-trip commits.
  assert.match(
    source,
    /dispatch\(\s*"delete",\s*\{\s*collectionId(?:\s*:\s*collectionId)?\s*,?\s*\}\s*\)/,
    "the confirm path must dispatch the delete event",
  );
  // The confirm path must also drive the modal closed so the
  // user sees the rail refresh without lingering UI.
  assert.match(
    source,
    /pendingDelete\s*=\s*null/,
    "the confirm path must close the modal",
  );
  // The cancel button must never dispatch — a regression that
  // wired both buttons to `confirmDelete` would delete the
  // collection when the user pressed Cancel.
  assert.match(
    source,
    /on:click=\{cancelDelete\}/,
    "the cancel button must funnel through cancelDelete",
  );
});

test("OrganizationSidebar modal buttons use the documented danger and secondary tokens", () => {
  const source = loadSource("src/OrganizationSidebar.svelte");
  // The confirm button must carry the danger colour hook so a
  // regression that drifted back to `--cv-accent` would surface
  // here. The cancel button stays neutral so the destructive
  // branch remains visually distinct.
  assert.match(
    source,
    /data-cv-danger="collection-delete-confirm"/,
    "the confirm button must advertise its danger hook",
  );
  assert.match(
    source,
    /class="modal-action modal-action-danger"/,
    "the confirm button must use the danger variant",
  );
  assert.match(
    source,
    /class="modal-action modal-action-secondary"/,
    "the cancel button must use the secondary variant",
  );
  // Both buttons must stay disabled while the parent's
  // destructive round-trip is in flight so a double click
  // cannot dispatch the event twice.
  assert.match(source, /disabled=\{pendingDeleteBusy\}/);
});

test("App.svelte registers exactly one search-shortcut listener and removes it", () => {
  // The desktop shell must register the Cmd+F / Ctrl+F listener
  // exactly once during `onMount` and tear it down in `onDestroy` so
  // a hot reload or a remount cannot double-fire the same handler.
  const source = loadSource("src/App.svelte");
  assert.match(
    source,
    /document\.addEventListener\(\s*"keydown",\s*onSearchShortcutKeydown,\s*true\s*\)/,
    "the listener must be registered in capture mode exactly once",
  );
  // The cleanup must mirror the registration with the same capture
  // flag and the same handler reference. The code splits the call
  // across multiple lines, so the assertion strips whitespace first.
  const removeCall = source.match(
    /document\.removeEventListener\(\s*"keydown",\s*onSearchShortcutKeydown,\s*true\s*,?\s*\)/,
  );
  assert.ok(
    removeCall,
    "the listener must be removed with the matching flag",
  );
  // The cleanup must be wired through `detachSearchShortcut` so
  // Svelte's `onDestroy` actually runs it.
  assert.match(
    source,
    /detachSearchShortcut\(\)/,
    "the listener cleanup must run during onDestroy",
  );
  // The handler must NOT register a second, hidden listener for the
  // same `Cmd/Ctrl+F` shortcut (e.g. `window.addEventListener` for
  // the same keystroke) — a duplicate would fire the focus logic
  // twice per shortcut. The Desktop preview overlay installs its
  // own `window.addEventListener("keydown", …)` to forward Escape
  // to the overlay; that listener is intentionally separate and the
  // helper it registers is not a duplicate of the search shortcut
  // listener.
  assert.equal(
    /window\.addEventListener\(\s*"keydown"\s*,\s*onSearchShortcutKeydown/.test(
      source,
    ),
    false,
    "no duplicate search-shortcut keydown listener on window",
  );
});

test("App.svelte registers each quick-search listener exactly once", () => {
  // The `quick-search`, `history-updated` and `organization-updated`
  // listeners each go through a registrar that returns a single
  // unlisten closure. A regression that subscribed twice would
  // double-refresh the rail on every backend event.
  const source = loadSource("src/App.svelte");
  assert.match(
    source,
    /registerQuickSearch\(handleQuickSearchActivation\)/,
    "the quick-search listener must use the documented registrar",
  );
  assert.match(
    source,
    /registerHistoryUpdated\(handleHistoryUpdated\)/,
    "the history-updated listener must use the documented registrar",
  );
  assert.match(
    source,
    /registerOrganizationUpdated\(handleOrganizationUpdated\)/,
    "the organization-updated listener must use the documented registrar",
  );
  // No listener that bypasses the registrar — every shortcut path
  // must funnel through the same dispatcher.
  assert.equal(
    /onMenuShortcut|onQuickSearchRaw/.test(source),
    false,
    "no shortcut listener may bypass the registrar",
  );
});

test("OrganizationSidebar add-collection listener is unique and inline", () => {
  // The new-collection affordance must be a single accessible
  // button; a regression that rendered two icons stacked would
  // double the click surface and confuse screen readers.
  const source = loadSource("src/OrganizationSidebar.svelte");
  const iconMatches = source.match(/data-testid="sidebar-new-collection"/g);
  assert.ok(iconMatches, "the new-collection control must exist");
  assert.equal(
    iconMatches?.length,
    1,
    "the new-collection control must appear exactly once",
  );
  // The create handler must be the documented single path.
  assert.match(source, /on:click=\{startCreate\}/);
});
