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

const FRONTEND_ROOT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

test("OrganizationSidebar keeps the collection list in a fixed internal scroller", () => {
  const source = loadSource("src/OrganizationSidebar.svelte");
  // The panel has the same fixed height as the card rail and the
  // list owns the vertical scroll. This prevents collections from
  // growing the desktop while keeping each visible row a DOM drop
  // target.
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
    "the fixed panel must clip around its internal viewport",
  );
  assert.equal(
    /\.sidebar\s*\{[^}]*height:\s*var\(\s*--cv-card-rail-height/.test(
      source,
    ),
    true,
    "the panel must pin its height to the rail token",
  );
  assert.equal(
    /\.sidebar\s*\{[^}]*max-height:\s*var\(\s*--cv-card-rail-height/.test(
      source,
    ),
    false,
    "the panel does not need a second max-height constraint",
  );
});

test("OrganizationSidebar panel pins its grid alignment through align-self and flex-shrink", () => {
  const source = loadSource("src/OrganizationSidebar.svelte");
  // The defensive `align-self: flex-start` and `flex-shrink: 0`
  // guards prevent a regression that switched `.layout` to
  // `align-items: stretch` (or moved the panel inside a flex
  // container that allowed shrinking) from silently growing the
  // panel to match the search-status bar.
  assert.match(
    source,
    /\.sidebar\s*\{[^}]*align-self:\s*flex-start/,
    "the sidebar must opt out of the grid track stretch",
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

test("Desktop layout pins the sidebar to the rail token", () => {
  const source = loadSource("src/App.svelte");
  // The grid token is declared on `:root` so the rail and the
  // sidebar share it. The layout grid MUST use `align-items:
  // flex-start` so a future contributor cannot accidentally lift
  // the sidebar to match a tall right column.
  assert.match(
    source,
    /--cv-card-rail-height:\s*calc\(var\(--cv-card-size/,
    "the desktop declares the shared card-rail-height token",
  );
  assert.match(
    source,
    /\.layout\s*\{[^}]*align-items:\s*flex-start/,
    "the desktop grid must align items to the start",
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
