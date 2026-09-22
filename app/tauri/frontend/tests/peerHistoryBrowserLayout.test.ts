/**
 * Regression coverage for the visual layout fix in
 * `peer-text-history-browser` (Bloque 1).
 *
 * The previous workspace hosted three direct children inside
 * `.layout` (OrganizationSidebar, LinkedPeers and `.layout-main`),
 * even though the grid only defined two columns. The implicit third
 * column displaced the main panel and broke the toolbar, the rail
 * and the visibility of the collection list. The fix re-hosts
 * `Equipos vinculados` INSIDE the sidebar as a sibling scroller and
 * keeps the grid at exactly two columns.
 *
 * The contract being pinned:
 *
 *   - `App.svelte` hosts EXACTLY two direct children inside
 *     `.layout`: `OrganizationSidebar` and `.layout-main`. The
 *     previous standalone `<LinkedPeers />` block is gone.
 *   - `OrganizationSidebar` renders the `LinkedPeers` block as a
 *     descendant (sibling of the `collection-list`). Both blocks
 *     share the panel's vertical space as independent scroll
 *     containers (`overflow-y: auto` + `min-height: 0`).
 *   - `.layout-main` keeps the previous order: the
 *     `DesktopToolbar` first, then the search-status bar, then
 *     the `HistoryCardRail` (or `RemoteHistoryRail`).
 *   - Selecting a local collection (or `Historial`) clears
 *     `activePeerId` so the rail local vuelve a renderizar sin
 *     esperar otra interacción.
 *   - `peerSnapshotCommand` se ejecuta explícitamente al cerrar
 *     `PeerPairingModal`, sin depender de la siguiente selección
 *     del peer.
 *   - `LinkedPeers` colorea el punto verde SÓLO cuando el peer es
 *     `trusted && is_present`. La pill "No disponible" aparece
 *     sólo para peers trusted pero sin presencia; el resto de
 *     estados (incluido `discovery_only`) muestra `—`.
 *   - `RemoteHistoryRail` NO tiene botón `Volver` como
 *     navegación principal. Volver ocurre al seleccionar
 *     `Historial` o una colección local en el sidebar.
 *   - `RemoteHistoryRail` NO abre una llamada de red si el peer
 *     no es `trusted && is_present`: usa el mismo predicado que
 *     `LinkedPeers` para decidir si muestra el placeholder de
 *     "No disponible" o dispara `requestFirstPage`.
 *
 * The tests read the production sources through `loadSource` so
 * they stay in lock-step with the markup and CSS contracts.
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
    .replace(/^\s*\/\/\/.*$/gm, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
}

const appSource = stripComments(loadSource("src/App.svelte"));
const sidebarSource = stripComments(loadSource("src/OrganizationSidebar.svelte"));
const linkedPeersSource = stripComments(loadSource("src/LinkedPeers.svelte"));
const remoteRailSource = stripComments(loadSource("src/RemoteHistoryRail.svelte"));

// ---------------------------------------------------------------------------
// Two-column workspace: no third direct child inside `.layout`.
// ---------------------------------------------------------------------------

test("App.svelte keeps exactly two columns: OrganizationSidebar + .layout-main", () => {
  // The previous regression surfaced here: a third direct child
  // (LinkedPeers) silently expanded the grid into three implicit
  // columns and displaced the main panel. The fix pins the
  // structural shape of the workspace grid.
  const layoutMatch = appSource.match(
    /<div class="layout" data-testid="desktop-workspace">[\s\S]*?<\/div>\s*{\/if}\s*<\/main>/,
  );
  assert.ok(layoutMatch, "the .layout wrapper must exist in App.svelte");
  const layoutBlock = layoutMatch[0];

  // The block must NOT contain a standalone `<LinkedPeers />` at
  // the same indentation as the other two children.
  const linkedPeersStandalone = /^\s*<LinkedPeers\s*\/>/m;
  assert.equal(
    linkedPeersStandalone.test(layoutBlock),
    false,
    ".layout must not host a standalone <LinkedPeers /> child",
  );

  // The block must host exactly two direct grid children: the
  // sidebar and the .layout-main wrapper. The assertion below is
  // deliberately structural: it counts the components the grid
  // can lay out.
  assert.match(
    layoutBlock,
    /<OrganizationSidebar\b[\s\S]*?\/>/,
    ".layout must render <OrganizationSidebar /> as a direct child",
  );
  assert.match(
    layoutBlock,
    /<div class="layout-main"[\s\S]*?<\/div>\s*<\/div>/,
    ".layout must render <div class=\"layout-main\"> as a direct child",
  );
});

test("App.svelte forwards linkedPeers + activePeerId to OrganizationSidebar", () => {
  // The sidebar now owns the linked-peers surface; the parent must
  // forward the snapshot and the currently-selected peer id, and
  // re-emit the `select-peer` event so the main panel flips back
  // and forth between the local rail and the remote rail.
  const sidebarOpen = /<OrganizationSidebar[\s\S]*?\/>/m;
  const sidebarTag = appSource.match(sidebarOpen);
  assert.ok(sidebarTag, "OrganizationSidebar must be rendered");
  assert.match(
    sidebarTag[0],
    /linkedPeers=\{peerSnapshot\}/,
    "OrganizationSidebar must receive linkedPeers={peerSnapshot}",
  );
  assert.match(
    sidebarTag[0],
    /activePeerId=\{activePeerId\}/,
    "OrganizationSidebar must receive activePeerId={activePeerId}",
  );
  assert.match(
    sidebarTag[0],
    /on:select-peer=/,
    "OrganizationSidebar must re-emit the select-peer event",
  );
});

test("App.svelte grid still pins two columns through grid-template-columns", () => {
  // The CSS contract lives separately from the markup. A
  // regression that widens the grid to host three columns would
  // surface here as a failed pattern match.
  assert.match(
    appSource,
    /\.layout\s*\{[^}]*grid-template-columns:\s*minmax\(180px,\s*220px\)\s*minmax\(0,\s*1fr\)/,
    ".layout must pin the grid to exactly two columns",
  );
});

// ---------------------------------------------------------------------------
// Equipos vinculados lives INSIDE the sidebar with its own scroller.
// ---------------------------------------------------------------------------

test("OrganizationSidebar renders LinkedPeers as a descendant of the sidebar", () => {
  // The fix re-hosts the linked peers inside the sidebar so the
  // desktop grid keeps two columns. The wrapper must receive the
  // snapshot, the activePeerId and re-emit the selection event.
  assert.match(
    sidebarSource,
    /<LinkedPeers\s+snapshot=\{linkedPeers\}/,
    "the sidebar must mount <LinkedPeers /> with snapshot={linkedPeers}",
  );
  assert.match(
    sidebarSource,
    /<LinkedPeers[\s\S]*?\{activePeerId\}/,
    "the sidebar must forward the activePeerId",
  );
  assert.match(
    sidebarSource,
    /on:select-peer=\{forwardPeerSelection\}/,
    "the sidebar must forward the select-peer event to the parent",
  );
});

test("OrganizationSidebar keeps two independent vertical scrollers (collections + equipos)", () => {
  // The collections list already owns a bounded scroller (the
  // regression suite from desktop-toolbar-layout pins it). The
  // linked-peers section adds a SECOND bounded scroller so a long
  // list of trusted peers cannot steal height from the collection
  // list (and vice versa). The CSS contract is pinned below.
  assert.match(
    sidebarSource,
    /\.collection-list\s*\{[^}]*overflow-y:\s*auto/,
    ".collection-list must own a vertical scroller",
  );
  assert.match(
    sidebarSource,
    /\.collection-list\s*\{[^}]*min-height:\s*0/,
    ".collection-list must allow shrinking below its intrinsic content",
  );
  // The linked-peers section (inside LinkedPeers.svelte) must
  // also own a vertical scroller so the two surfaces share the
  // panel height fairly.
  assert.match(
    linkedPeersSource,
    /\.linked-peers-list\s*\{[^}]*overflow-y:\s*auto/,
    ".linked-peers-list must own an independent vertical scroller",
  );
  assert.match(
    linkedPeersSource,
    /\.linked-peers-list\s*\{[^}]*min-height:\s*0/,
    ".linked-peers-list must allow shrinking below its intrinsic content",
  );
  // The section wrapper must request `flex: 1 1 0` (or similar)
  // so it shares the panel height with the collection list.
  assert.match(
    linkedPeersSource,
    /\.linked-peers\s*\{[^}]*flex:\s*1 1 0/,
    ".linked-peers section must share the sidebar vertical space with .collection-list",
  );
});

// ---------------------------------------------------------------------------
// .layout-main keeps the previous order.
// ---------------------------------------------------------------------------

test("App.svelte keeps toolbar → search-status → HistoryCardRail in .layout-main", () => {
  // The fix pins that the right column restores its previous order
  // so the toolbar is the FIRST descendant of `.layout-main` and
  // the rail is the LAST one. A regression that swaps the order
  // (e.g. putting the rail first) surfaces here.
  const columnMatch = appSource.match(
    /<div class="layout-main"[\s\S]*?<\/div>\s*<\/div>/,
  );
  assert.ok(columnMatch, ".layout-main must exist");
  const column = columnMatch[0];

  const toolbarIdx = column.indexOf("<DesktopToolbar");
  const statusIdx = column.indexOf('data-testid="search-status"');
  const remoteRailIdx = column.indexOf("<RemoteHistoryRail");
  const historyRailIdx = column.indexOf("<HistoryCardRail");

  assert.notEqual(toolbarIdx, -1, "DesktopToolbar must live inside .layout-main");
  assert.notEqual(statusIdx, -1, "search-status must live inside .layout-main");
  // Exactly one rail block must live inside the column (the
  // either-or branch Svelte emits at runtime, the source still
  // hosts both components inside the conditional).
  assert.ok(
    remoteRailIdx !== -1 || historyRailIdx !== -1,
    ".layout-main must host at least one rail block",
  );

  assert.ok(
    toolbarIdx < statusIdx,
    "DesktopToolbar must precede the search-status bar",
  );
  if (remoteRailIdx !== -1 && historyRailIdx !== -1) {
    // The either-or branch keeps the rail AFTER the toolbar +
    // status regardless of the active peer id.
    const minRailIdx = Math.min(remoteRailIdx, historyRailIdx);
    assert.ok(
      minRailIdx > statusIdx,
      "the rail (remote or local) must follow the toolbar + status",
    );
  }
});

// ---------------------------------------------------------------------------
// Selecting a local collection clears activePeerId.
// ---------------------------------------------------------------------------

test("App.svelte clears activePeerId when a local collection (or Historial) is selected", () => {
  // The previous implementation never cleared `activePeerId` when
  // the user picked a collection while a peer was open, so the
  // rail local never came back. The fix MUST call `closePeerRail`
  // (or equivalent) inside `selectCollectionFromSidebar`.
  const handlerMatch = appSource.match(
    /async function selectCollectionFromSidebar[\s\S]*?\n\s*await refreshEntries\(\);/,
  );
  assert.ok(
    handlerMatch,
    "selectCollectionFromSidebar must exist in App.svelte",
  );
  const handler = handlerMatch[0];
  assert.match(
    handler,
    /activePeerId\s*!==\s*null/,
    "selectCollectionFromSidebar must guard on activePeerId being non-null",
  );
  assert.match(
    handler,
    /closePeerRail\(\)/,
    "selectCollectionFromSidebar must call closePeerRail to restore the local rail",
  );
});

// ---------------------------------------------------------------------------
// peerSnapshot refreshes when the pairing modal closes.
// ---------------------------------------------------------------------------

test("App.svelte refreshes peerSnapshot when the pairing modal closes", () => {
  // Closing the pairing modal MUST trigger an explicit
  // `peerSnapshotCommand` round-trip so a freshly trusted peer
  // appears in `Equipos vinculados` without forcing the user to
  // select it. The handler is `onPairingClosed`.
  const handlerMatch = appSource.match(
    /function onPairingClosed[\s\S]*?\n {0,2}\}/,
  );
  assert.ok(handlerMatch, "onPairingClosed must exist in App.svelte");
  const handler = handlerMatch[0];
  assert.match(
    handler,
    /refreshPeerSnapshot\(\)/,
    "onPairingClosed must refresh peerSnapshot on close",
  );

  const refresherMatch = appSource.match(
    /async function refreshPeerSnapshot\(\)[\s\S]*?\n {2}\}/,
  );
  assert.ok(
    refresherMatch,
    "refreshPeerSnapshot helper must exist in App.svelte",
  );
  assert.match(
    refresherMatch[0],
    /peerSnapshotCommand\(\)/,
    "refreshPeerSnapshot must invoke peerSnapshotCommand",
  );
});

// ---------------------------------------------------------------------------
// LinkedPeers dot is green ONLY for trusted && is_present.
// ---------------------------------------------------------------------------

test("LinkedPeers paints the dot green ONLY for trusted && is_present", () => {
  // The previous helper treated any peer as Active whenever
  // `is_present` was true, even if trust was `unverified`. The
  // fix pins the predicate to `trust_state === "trusted" &&
  // is_present`, mirroring the gate `RemoteHistoryRail` uses to
  // decide whether to open the network call.
  const helperMatch = linkedPeersSource.match(
    /function isActive\([\s\S]*?\n\s*}/,
  );
  assert.ok(helperMatch, "isActive helper must exist in LinkedPeers.svelte");
  assert.match(
    helperMatch[0],
    /trust_state\s*===\s*"trusted"\s*&&\s*entry\.is_present/,
    "isActive must require trust_state === \"trusted\" AND is_present",
  );
});

test("LinkedPeers surfaces a gray pill for trusted but unavailable peers", () => {
  // The No disponible pill must show when the peer is trusted but
  // currently absent so the dot and the copy cannot disagree.
  const helperMatch = linkedPeersSource.match(
    /function isTrustedUnavailable\([\s\S]*?\n\s*}/,
  );
  assert.ok(
    helperMatch,
    "isTrustedUnavailable helper must exist in LinkedPeers.svelte",
  );
  assert.match(
    helperMatch[0],
    /trust_state\s*===\s*"trusted"\s*&&\s*!entry\.is_present/,
    "isTrustedUnavailable must require trust_state === \"trusted\" AND !is_present",
  );
  // The template must use the helper to surface the pill copy.
  assert.match(
    linkedPeersSource,
    /\{\s*#if isActive\(entry\)\s*\}\s*Activo\s*\{\s*:else if isTrustedUnavailable\(entry\)\s*\}\s*No disponible/,
    "the template must show Activo / No disponible based on the helpers",
  );
});

// ---------------------------------------------------------------------------
// RemoteHistoryRail removes the Volver button and respects the gate.
// ---------------------------------------------------------------------------

test("RemoteHistoryRail does NOT render a Volver back button", () => {
  // The previous implementation exposed a primary `Volver` button
  // that duplicated the local-collection selection. The fix
  // removes it: return happens by selecting `Historial` or a local
  // collection in the sidebar. A regression that re-introduces a
  // `Volver` button surfaces here.
  assert.equal(
    /data-testid="remote-history-rail-back"|remote-history-rail-back/.test(
      remoteRailSource,
    ),
    false,
    "RemoteHistoryRail must not render a Volver back button",
  );
  assert.equal(
    />\s*Volver\s*</.test(remoteRailSource),
    false,
    "RemoteHistoryRail must not contain the literal Volver label",
  );
});

test("RemoteHistoryRail skips the network call when the peer is not trusted && is_present", () => {
  // The rail MUST NOT call `requestFirstPage` when the active peer
  // fails the trust + presence gate; it should surface the
  // unavailable placeholder instead. The reactive block that
  // triggers the first-page request must guard on the same
  // predicate the LinkedPeers dot uses.
  const reactiveMatch = remoteRailSource.match(
    /\$: if \(peerId !== activePeerId\)[\s\S]*?\n {2}\}/,
  );
  assert.ok(
    reactiveMatch,
    "the peerId switch reactive block must exist in RemoteHistoryRail.svelte",
  );
  assert.match(
    reactiveMatch[0],
    /railShouldShowUnavailable/,
    "the reactive block must guard on railShouldShowUnavailable",
  );

  const predicateMatch = remoteRailSource.match(
    /\$: railShouldShowUnavailable\s*=[\s\S]*?;/,
  );
  assert.ok(
    predicateMatch,
    "railShouldShowUnavailable predicate must exist",
  );
  assert.match(
    predicateMatch[0],
    /activeIsTrusted\s*&&\s*activeIsPresent/,
    "railShouldShowUnavailable must use activeIsTrusted && activeIsPresent",
  );
});