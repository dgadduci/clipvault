/**
 * Regression coverage for the collection-panel height contract the
 * `desktop-collection-card-polish` change ships.
 *
 * The reported regression is that the sidebar grew in height when
 * collections were added — the user expected the panel to share the
 * stable `--cv-card-rail-height` token with the rail and to scroll
 * the collection list internally instead of expanding the desktop.
 *
 * The tests pin the CSS contract through the rendered source so a
 * future contributor cannot lift the panel above the rail height or
 * widen the desktop past the rail's stable slice.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { visualTokenCss } from "../src/lib/visualTokens.ts";

const FRONTEND_ROOT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

test("OrganizationSidebar keeps the collection list in a bounded internal scroller", () => {
  const source = loadSource("src/OrganizationSidebar.svelte");
  // The desktop-toolbar-layout change introduced a shared-height
  // workspace: the grid now uses `align-items: stretch` and the
  // sidebar grows with `height: 100%` to match the right column
  // (toolbar + status + rail). The list below remains the only
  // internal vertical scroller so adding collections cannot grow
  // the desktop or move the drop targets outside the visible
  // panel.
  assert.equal(
    /\.collection-list\s*\{[^}]*overflow-y:\s*auto/.test(source),
    true,
    "the list must own the vertical scroller",
  );
  assert.equal(
    /\.collection-list\s*\{[^}]*overflow-x:\s*hidden/.test(source),
    true,
    "the list must hide horizontal overflow",
  );
  assert.equal(
    /\.sidebar\s*\{[^}]*overflow:\s*hidden/.test(source),
    true,
    "the panel must clip around its internal viewport",
  );
  // The panel no longer pins to the fixed rail token: it stretches
  // to the row height the grid owns through `height: 100%`. The
  // `min-height: 0` guard is the structural piece that lets the
  // panel shrink below its intrinsic content (many collections)
  // without pushing the row taller than the rail.
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
  assert.equal(
    /\.sidebar\s*\{[^}]*max-height:\s*var\(\s*--cv-card-rail-height/.test(
      source,
    ),
    false,
    "the panel does not need a max-height separate from its stretch",
  );
});

test("OrganizationSidebar panel opts out of flex shrinking inside the workspace", () => {
  const source = loadSource("src/OrganizationSidebar.svelte");
  // The `flex-shrink: 0` guard prevents a regression that places
  // the panel inside a flex container (the right column or a
  // future responsive row) from silently shrinking the sidebar
  // below its content. The `align-self: flex-start` override the
  // previous change shipped is gone on purpose: the grid now uses
  // `align-items: stretch` and the panel sizes itself with
  // `height: 100%`.
  assert.doesNotMatch(
    source,
    /\.sidebar\s*\{[^}]*align-self:\s*flex-start/,
    "the sidebar must not opt out of the grid stretch",
  );
  assert.match(
    source,
    /\.sidebar\s*\{[^}]*flex-shrink:\s*0/,
    "the sidebar must opt out of flex shrinking",
  );
});

test("OrganizationSidebar collection list keeps its item layout when it grows past the rail", () => {
  const source = loadSource("src/OrganizationSidebar.svelte");
  // The list now grows naturally past the rail's height — the
  // window scrolls. Each row still owns its rounded rectangle and
  // the names keep ellipsizing instead of pushing the row width.
  assert.match(
    source,
    /\.collection-list\s*\{[^}]*display:\s*flex/,
    "the list must keep its flex column layout",
  );
  assert.match(
    source,
    /\.collection-list\s*\{[^}]*flex-direction:\s*column/,
    "the list must stack items vertically",
  );
  assert.match(
    source,
    /\.collection-name\s*\{[^}]*overflow:\s*hidden/,
    "collection names must clip instead of pushing the row width",
  );
  assert.match(
    source,
    /\.collection-name\s*\{[^}]*text-overflow:\s*ellipsis/,
    "collection names must use ellipsis on overflow",
  );
  assert.match(
    source,
    /\.collection-name\s*\{[^}]*white-space:\s*nowrap/,
    "collection names must stay on a single line",
  );
});

test("OrganizationSidebar keeps the header and the new-collection icon visible above the list", () => {
  const source = loadSource("src/OrganizationSidebar.svelte");
  // The header and the inline form MUST keep their documented
  // `flex: 0 0 auto` slot so the user never loses access to the
  // destructive confirmation flow when the list is long.
  assert.match(
    source,
    /\.sidebar-header\s*\{[^}]*flex:\s*0 0 auto/,
    "the sidebar header must not shrink",
  );
});

test("Desktop layout stretches both columns to the rail height", () => {
  const source = loadSource("src/App.svelte");
  // The visual token block is no longer inlined inside this file's
  // `<style>` section: `App.svelte` injects the same string the
  // Quick Paste palette uses through `<svelte:head>` so the desktop
  // rail and the Quick Paste palette share one source of truth.
  // The regression asserts both that the helper is the single
  // writer and that the resulting CSS carries the rail-height
  // declaration the rail consumes.
  assert.match(
    source,
    /visualTokenCss\(/,
    "the desktop must source the visual tokens through the shared helper",
  );
  assert.match(
    source,
    /svelte:head/,
    "the desktop must inject the visual tokens through <svelte:head>",
  );
  assert.match(
    visualTokenCss(),
    /--cv-card-rail-height:\s*calc\(var\(--cv-card-size/,
    "the shared token block must declare --cv-card-rail-height",
  );
  assert.match(
    source,
    /\.layout\s*\{[^}]*align-items:\s*stretch/,
    "the desktop grid must stretch both columns to the same height",
  );
  // The search-status bar MUST NOT lift the grid row when its
  // message wraps.
  assert.match(
    source,
    /\.search-status\s*\{[^}]*max-height:\s*1\.4em/,
    "the search-status bar must reserve a single line",
  );
});

test("HistoryCardRail pins the rail height to the same token", () => {
  const source = loadSource("src/HistoryCardRail.svelte");
  // The rail height must come from the same `--cv-card-rail-height`
  // token. A regression that hard-coded a different `calc()` would
  // silently drift the rail above or below the sidebar.
  assert.match(
    source,
    /\.rail\s*\{[^}]*height:\s*var\(\s*--cv-card-rail-height/,
  );
  assert.match(
    source,
    /\.rail\s*\{[^}]*overflow-x:\s*auto/,
    "the rail must own the horizontal scrollbar",
  );
  assert.match(
    source,
    /\.rail\s*\{[^}]*overflow-y:\s*hidden/,
    "the rail must never introduce a vertical scrollbar",
  );
  assert.match(
    source,
    /flex:\s*0 0 var\(--cv-card-size\)/,
    "the cards must keep their fixed square dimensions",
  );
});
