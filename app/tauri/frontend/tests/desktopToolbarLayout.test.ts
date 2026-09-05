/**
 * Regression coverage for the `desktop-toolbar-layout` change.
 *
 * The change reorganises the desktop so the search surface and the
 * four secondary actions live next to the rail of cards instead of
 * on a global bar above the workspace. The contract being pinned
 * here:
 *
 *   - The desktop renders EXACTLY one `DesktopToolbar` and that
 *     toolbar lives inside the right column (the layout-main flex
 *     column) — never above the workspace as a global bar.
 *   - The search input, its shortcut hint and the trash button
 *     survive; the four standalone modal buttons are gone and are
 *     replaced by a single accessible menu with four menuitems.
 *   - The trash button is a sibling of the ellipsis trigger, NOT
 *     a menu item.
 *   - The menu handles Escape, click outside, selection and
 *     destroy idempotently so a remount or hot reload cannot leak
 *     listeners; focus returns to the trigger on close.
 *   - The right column (toolbar + status + rail) and the sidebar
 *     share height through `align-items: stretch` +
 *     `height: 100%`; the sidebar list owns the vertical scroller
 *     so adding collections never grows the desktop.
 *   - The search keeps filtering the active collection, the
 *     `Cmd-F` / `Ctrl-F` shortcut still focuses the relocated
 *     input and the rest of the desktop contracts (image
 *     lifecycle, drag-and-drop, organization operations) are
 *     preserved.
 *
 * The tests below read the production source through the
 * `loadSource` helper so they stay in lock-step with the markup
 * and CSS contracts.
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

function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/\/.*$/gm, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
}

// ---------------------------------------------------------------------------
// Toolbar position: single instance, inside the right column.
// ---------------------------------------------------------------------------

test("App.svelte renders exactly one DesktopToolbar inside the right column", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  // The toolbar must be a descendant of `.layout-main` (the right
  // column flex container) and appear BEFORE both the search-status
  // bar and the HistoryCardRail. A regression that re-introduces a
  // global toolbar above the workspace, or that places the toolbar
  // after the rail, surfaces here as a failed assertion.
  const layoutMatch = source.match(
    /<div class="layout-main"[\s\S]*?<\/div>\s*<\/div>\s*{\/if}/,
  );
  assert.ok(
    layoutMatch,
    "the right column wrapper .layout-main must wrap the toolbar + status + rail",
  );
  const column = layoutMatch[0];
  const toolbarIdx = column.indexOf("<DesktopToolbar");
  const statusIdx = column.indexOf('data-testid="search-status"');
  const railIdx = column.indexOf("<HistoryCardRail");
  assert.notEqual(toolbarIdx, -1, "the toolbar must live inside .layout-main");
  assert.notEqual(statusIdx, -1, "the search-status bar must still live inside .layout-main");
  assert.notEqual(railIdx, -1, "the HistoryCardRail must still live inside .layout-main");
  assert.ok(
    toolbarIdx < statusIdx && statusIdx < railIdx,
    "the toolbar must appear before the search-status bar and the rail",
  );
  // The toolbar must NOT be a sibling of `.layout`: the previous
  // contract rendered it as a global bar above the workspace.
  const beforeLayout = source.split('<div class="layout"')[0];
  assert.equal(
    beforeLayout.includes("<DesktopToolbar"),
    false,
    "the toolbar must not be rendered above the workspace",
  );
  // No second toolbar must exist anywhere in the file.
  const toolbarOccurrences = source.split("<DesktopToolbar").length - 1;
  assert.equal(
    toolbarOccurrences,
    1,
    "the desktop must render exactly one DesktopToolbar instance",
  );
});

test("App.svelte drops the standalone secondary action buttons beside the search", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  // The old contract rendered four buttons in the toolbar
  // (Development / Privacidad / Retención / Atajo). The new contract
  // moves them into a menu owned by the toolbar itself. The parent
  // must NOT keep forwarding them as inline buttons.
  for (const testId of [
    "open-development",
    "open-privacy",
    "open-retention",
    "open-shortcut",
  ]) {
    assert.equal(
      source.includes(`data-testid="${testId}"`),
      false,
      `App.svelte must not render a standalone ${testId} button`,
    );
  }
});

// ---------------------------------------------------------------------------
// Menu: role, menuitems, callbacks, trigger semantics.
// ---------------------------------------------------------------------------

test("DesktopToolbar renders a single ellipsis trigger with accessible semantics", () => {
  const source = stripComments(loadSource("src/DesktopToolbar.svelte"));
  // Exactly one trigger that controls the menu.
  assert.match(
    source,
    /data-testid="open-overflow-menu"/,
    "the toolbar must expose a single overflow trigger",
  );
  // The trigger must declare `aria-haspopup="menu"` and
  // `aria-expanded` so AT users can discover the popup relationship
  // and the current open state.
  assert.match(source, /aria-haspopup="menu"/);
  assert.match(source, /aria-expanded=\{menuOpen\}/);
  // `aria-controls` must point at the menu id so the relationship
  // is explicit for AT users navigating the DOM.
  assert.match(source, /aria-controls="desktop-overflow-menu"/);
  // No second ellipsis trigger must exist (the trash is a sibling,
  // not a trigger).
  const triggerCount = source.split("open-overflow-menu").length - 1;
  assert.equal(
    triggerCount,
    1,
    "the overflow trigger must be unique per toolbar instance",
  );
});

test("DesktopToolbar menu exposes exactly four menuitems that forward the existing callbacks", () => {
  const source = stripComments(loadSource("src/DesktopToolbar.svelte"));
  // The menu container must use `role="menu"` so screen readers
  // announce it as a popup menu.
  assert.match(source, /role="menu"/);
  // The four documented actions must each render as a menuitem
  // with the data-testid the parent still dispatches through. We
  // pin the count through `data-testid` (a real attribute) instead
  // of `role="menuitem"` because the source also uses the role
  // inside a string literal that drives the focus lookup.
  const menuItemTestIds = new Set([
    "open-development",
    "open-privacy",
    "open-retention",
    "open-shortcut",
  ]);
  for (const testId of menuItemTestIds) {
    const matches = source.match(
      new RegExp(`data-testid="${testId}"`, "g"),
    ) ?? [];
    assert.equal(
      matches.length,
      1,
      `the menu must surface ${testId} exactly once`,
    );
  }
  // Every menu item must carry `role="menuitem"` together with its
  // documented `data-testid` so AT users hear the menu semantics.
  for (const testId of menuItemTestIds) {
    assert.match(
      source,
      new RegExp(
        `<button[\\s\\S]*?role="menuitem"[\\s\\S]*?data-testid="${testId}"`,
      ),
      `${testId} must live on a menuitem`,
    );
  }
  // Each menuitem forwards to the same callback the parent already
  // owned — the `on:click` handler calls `selectItem(onOpenX, event)`
  // which in turn invokes the documented callback.
  for (const callback of [
    "onOpenDevelopment",
    "onOpenPrivacy",
    "onOpenRetention",
    "onOpenShortcut",
  ]) {
    assert.match(
      source,
      new RegExp(`selectItem\\(${callback},`),
      `${callback} must still flow through the menuitem click`,
    );
  }
  // Each label must surface exactly once as the visible content of
  // a `<button>` element — i.e. the four secondary desktop actions
  // are accessible only through the menu, never as standalone
  // text buttons beside the search input. The regex matches a
  // `<button` opener, any markup, then the label as the text
  // content, then the closing `</button>`.
  for (const literal of ["Development", "Privacidad", "Retención", "Atajo de pegado rápido"]) {
    const labelMatches = source.match(
      new RegExp(`<button[\\s\\S]*?>\\s*${literal}\\s*</button>`, "g"),
    ) ?? [];
    assert.equal(
      labelMatches.length,
      1,
      `${literal} must appear exactly once as the text content of a <button>`,
    );
  }
});

test("DesktopToolbar menu hides when closed and removes its listeners on destroy", () => {
  const source = stripComments(loadSource("src/DesktopToolbar.svelte"));
  // The menu container is only rendered when `menuOpen` is true.
  assert.match(source, /\{#if menuOpen\}/);
  // The `role="menu"` block must live inside the `{#if menuOpen}`
  // guard so the menu DOM does not exist when the menu is closed.
  const ifOpenIdx = source.indexOf("{#if menuOpen}");
  const nextIfCloseIdx = source.indexOf("{/if}", ifOpenIdx);
  assert.notEqual(ifOpenIdx, -1);
  assert.notEqual(nextIfCloseIdx, -1);
  const menuBlock = source.slice(ifOpenIdx, nextIfCloseIdx);
  assert.match(
    menuBlock,
    /role="menu"/,
    "the menu DOM must live inside the {#if menuOpen} guard",
  );
  // The component installs the outside-click and Escape handlers
  // exactly once (idempotent flag) and removes them on destroy.
  assert.match(source, /function ensureWindowListeners/);
  assert.match(source, /function detachWindowListeners/);
  assert.match(source, /onDestroy\(\(\) => \{[\s\S]*?detachWindowListeners\(\)/);
  // The reactive guard must add/remove listeners when the menu
  // toggles open/closed so they cannot leak across remounts.
  assert.match(source, /\$: if \(menuOpen\) \{[\s\S]*?ensureWindowListeners\(\)/);
});

// ---------------------------------------------------------------------------
// Trash button: sibling of the ellipsis trigger, NOT inside the menu.
// ---------------------------------------------------------------------------

test("DesktopToolbar renders the trash button outside the menu as a sibling of the ellipsis trigger", () => {
  const source = stripComments(loadSource("src/DesktopToolbar.svelte"));
  // The trash button keeps its data-testid, accessible label,
  // danger treatment and aria-busy hook so the existing
  // confirmation flow remains reachable.
  assert.match(source, /data-testid="trash-clear-history"/);
  assert.match(source, /data-cv-danger="clear-history"/);
  assert.match(source, /aria-busy=\{trashConfirming\}/);
  // The trash button must NOT be inside the menu: the menu markup
  // is the block that uses `role="menu"` and `{#if menuOpen}`. A
  // regression that moves the trash into the menu would surface
  // here because the `trash-clear-history` testid would appear
  // after the `{#if menuOpen}` opener inside the same script path.
  const menuOpenIdx = source.indexOf("{#if menuOpen}");
  const menuCloseIdx = source.indexOf("{/if}", menuOpenIdx);
  assert.notEqual(menuOpenIdx, -1, "menu block must be present");
  assert.notEqual(menuCloseIdx, -1, "menu block must close");
  const menuBody = source.slice(menuOpenIdx, menuCloseIdx);
  assert.equal(
    menuBody.includes("trash-clear-history"),
    false,
    "the trash button must live outside the menu",
  );
  // The menu body must contain ONLY the four documented items.
  for (const itemTestId of [
    "open-development",
    "open-privacy",
    "open-retention",
    "open-shortcut",
  ]) {
    assert.ok(
      menuBody.includes(itemTestId),
      `menu body must expose ${itemTestId}`,
    );
  }
});

// ---------------------------------------------------------------------------
// Toolbar callbacks: same signatures, same forwarding contract.
// ---------------------------------------------------------------------------

test("DesktopToolbar exports the four modal callbacks plus the trash and search callbacks", () => {
  const source = stripComments(loadSource("src/DesktopToolbar.svelte"));
  // The component must keep exporting every callback the parent
  // already wired through `App.svelte` so the parent never needs
  // to invent a new event flow.
  for (const callback of [
    "onSearchInput",
    "onOpenDevelopment",
    "onOpenPrivacy",
    "onOpenRetention",
    "onOpenShortcut",
    "onRequestClearHistory",
  ]) {
    assert.match(
      source,
      new RegExp(`export let ${callback}:`),
      `${callback} must remain a public prop`,
    );
  }
});

// ---------------------------------------------------------------------------
// Workspace layout: shared height, min-width: 0, no horizontal overflow.
// ---------------------------------------------------------------------------

test("App.svelte keeps the two-column grid with stretch alignment and min-width: 0", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  // The grid still uses the documented template so the sidebar
  // stays bounded while the right column owns the rest of the row.
  assert.match(
    source,
    /grid-template-columns:\s*minmax\(180px,\s*220px\)\s*minmax\(0,\s*1fr\)/,
  );
  // `align-items: stretch` is the new contract that lets the
  // sidebar grow with the right column instead of pinning to a
  // fixed token.
  assert.match(
    source,
    /\.layout\s*\{[^}]*align-items:\s*stretch/,
    "the workspace grid must stretch both columns",
  );
  assert.match(
    source,
    /\.layout\s*\{[^}]*min-width:\s*0/,
    "the workspace grid must allow shrinking below its content",
  );
  assert.match(
    source,
    /\.layout-main\s*\{[^}]*min-width:\s*0/,
    "the right column must allow shrinking below its content",
  );
  // The body of `<main>` must not reintroduce the legacy `max-width`
  // cap that defeated the horizontal scroll.
  assert.equal(/max-width:\s*880px/.test(source), false);
  assert.match(source, /max-width:\s*none/);
});

test("OrganizationSidebar stretches to the workspace height and keeps its scroller", () => {
  const source = stripComments(loadSource("src/OrganizationSidebar.svelte"));
  // The desktop-toolbar-layout change replaces the fixed
  // `--cv-card-rail-height` pin with a stretch-based contract so
  // the sidebar matches the right column (toolbar + status + rail)
  // without a second fixed-height token.
  assert.match(
    source,
    /\.sidebar\s*\{[^}]*height:\s*100%/,
    "the sidebar must stretch to the workspace height",
  );
  assert.match(
    source,
    /\.sidebar\s*\{[^}]*min-height:\s*0/,
    "the sidebar must allow shrinking below its intrinsic content",
  );
  assert.match(
    source,
    /\.sidebar\s*\{[^}]*flex-shrink:\s*0/,
    "the sidebar must opt out of flex shrinking",
  );
  // The previous `align-self: flex-start` override is gone on
  // purpose: the grid now uses `align-items: stretch` and the
  // panel sizes itself through `height: 100%`.
  assert.equal(
    /\.sidebar\s*\{[^}]*align-self:\s*flex-start/.test(source),
    false,
  );
  // The collection list still owns the vertical scroller so adding
  // collections cannot grow the desktop.
  assert.match(source, /\.collection-list\s*\{[^}]*overflow-y:\s*auto/);
  assert.match(source, /\.collection-list\s*\{[^}]*overflow-x:\s*hidden/);
  // The panel still clips around its internal viewport.
  assert.match(source, /\.sidebar\s*\{[^}]*overflow:\s*hidden/);
});

// ---------------------------------------------------------------------------
// Responsive: the overflow menu stays usable in narrow windows.
// ---------------------------------------------------------------------------

test("DesktopToolbar keeps the search, menu and trash usable in narrow windows", () => {
  const source = stripComments(loadSource("src/DesktopToolbar.svelte"));
  // The toolbar still wraps onto multiple rows below the
  // documented breakpoint so the search, menu and trash remain
  // reachable on narrow windows.
  assert.match(source, /@media \(max-width: 640px\)/);
  // The menu must position itself relative to a stable wrapper
  // so it cannot be clipped by the toolbar's own overflow. The
  // wrapper is `position: relative` and the menu is
  // `position: absolute` anchored to it.
  assert.match(source, /\.menu-wrapper\s*\{[^}]*position:\s*relative/);
  assert.match(source, /\.menu\s*\{[^}]*position:\s*absolute/);
  // The menu must carry a z-index above the toolbar surface so it
  // does not get hidden behind the rail / cards.
  assert.match(source, /\.menu\s*\{[^}]*z-index:\s*30/);
});

// ---------------------------------------------------------------------------
// Search: filtering over the active collection, Cmd-F / Ctrl-F shortcut.
// ---------------------------------------------------------------------------

test("App.svelte keeps filtering the active collection through the relocated search", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  // The search query MUST still scope its results to the active
  // collection: the rail filter flows through
  // `recentEntriesFilteredCommand` when `selectedCollectionId`
  // is set, and the global listener cannot have been removed by
  // the layout change.
  assert.match(source, /recentEntriesFilteredCommand\(/);
  assert.match(source, /selectedCollectionId/);
  // The global shortcut listener remains anchored on
  // `Cmd+F` / `Ctrl+F` so the relocated input still receives
  // focus when the user presses the platform-specific binding.
  assert.match(source, /onSearchShortcutKeydown/);
  assert.match(source, /document\.addEventListener\("keydown", onSearchShortcutKeydown/);
  // The shortcut label still surfaces inside the toolbar.
  assert.match(source, /searchShortcut=\{searchShortcutLabelText\}/);
});

// ---------------------------------------------------------------------------
// Image lifecycle, favorites, organization and drag-and-drop survive.
// ---------------------------------------------------------------------------

test("App.svelte keeps the image lifecycle, pin, organization and drag-and-drop wiring", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  // Pin / unpin flow.
  assert.match(source, /setFavoriteCommand\(/);
  // Drop handler still wires through `combineMemberships` so the
  // additive combine contract is preserved.
  assert.match(source, /combineMemberships\(/);
  // Trash confirmation still routes through the documented
  // callback the parent owns.
  assert.match(source, /onRequestClearHistory/);
  // Image entry hydration is still mounted on the parent.
  assert.match(source, /hydrateEntryOrganization/);
  // The pointer drag controller is still installed exactly once.
  assert.match(source, /installPointerDragController\(document\)/);
  assert.match(source, /detachPointerDragController/);
});

test("OrganizationSidebar keeps the data-collections-drop-viewport contract", () => {
  const source = stripComments(loadSource("src/OrganizationSidebar.svelte"));
  // The drag-and-drop row layout, the drop zone factory and the
  // global `dragend` listener all survive the layout change.
  assert.match(source, /data-collections-drop-viewport=/);
  assert.match(source, /data-drop-target=/);
  assert.match(source, /document\.addEventListener\("dragend", onWindowDragEnd\)/);
  assert.match(source, /document\.removeEventListener\("dragend", onWindowDragEnd\)/);
  assert.match(source, /createCollectionDropZoneHandlers/);
});

test("DesktopToolbar never logs clipboard content, snippets or asset references", () => {
  const source = stripComments(loadSource("src/DesktopToolbar.svelte"));
  // The toolbar is presentational: it must never inspect or
  // persist clipboard content, snippets, asset paths or hashes.
  for (const forbidden of [
    "content_hash",
    "snippet",
    "asset_ref",
    "mime_type",
    "clipboard",
    "DataTransfer",
    "base64",
  ]) {
    assert.equal(
      source.includes(forbidden),
      false,
      `DesktopToolbar must not reference "${forbidden}"`,
    );
  }
});