/**
 * Regression coverage for the desktop-owned peer-snapshot
 * refresh the `peer-text-history-browser` change pins (task 3.2
 * reapertura).
 *
 * The desktop (`App.svelte`) is the single owner of the
 * `peer_snapshot` the `Equipos vinculados` list and the
 * `RemoteHistoryRail` consume. The previous implementation only
 * refreshed the snapshot once (bootstrap) and once more when the
 * pairing modal closed; a fresh `Observed` / `Removed` event the
 * discovery worker drained in between was invisible to the
 * linked list and the remote rail. The fix:
 *
 *   - centralises the snapshot round trip behind a single-flight
 *     `refreshPeerSnapshot()` helper so the bootstrap refresh,
 *     the post-pairing refresh and the polling tick coalesce
 *     into one in-flight bridge call when they overlap;
 *   - keeps `refreshPeerSnapshot()` as the only call site of
 *     `peerSnapshotCommand()` so no path can open a duplicate
 *     request;
 *   - runs a bounded, locally-defined 2 s cadence the desktop
 *     owns (consistent with `PeerSharingModal`); the interval is
 *     started on `onMount` and cleared on `onDestroy` so a hot
 *     reload or remount never leaks a timer;
 *   - keeps `LinkedPeers` and `RemoteHistoryRail` purely
 *     presentational — they render the snapshot the parent
 *     supplies and never start their own timers or open
 *     `peerSnapshotCommand()` themselves.
 *
 * The suite reads the production source so it stays in lock-step
 * with the implementation without standing up a Svelte runtime.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const FRONTEND_ROOT = process.cwd();

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

/**
 * Strip comments so a regression pinned inside a comment cannot
 * accidentally match the source-level assertions below. The
 * contract the suite pins is structural: every assertion reads
 * the runtime behaviour that ships in the bundle, not the prose
 * that surrounds it.
 */
function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
}

const appSource = stripComments(loadSource("src/App.svelte"));
const linkedPeersSource = stripComments(loadSource("src/LinkedPeers.svelte"));
const remoteRailSource = stripComments(loadSource("src/RemoteHistoryRail.svelte"));

/**
 * Extract the body of a top-level `function name(` / `async function
 * name(` declaration. The walk balances braces so an inner IIFE or
 * a nested `setInterval` callback does not truncate the slice.
 * The previous tests in `peerSharingModal.test.ts` rely on the
 * same helper shape so the assertions below stay consistent with
 * the rest of the regression suite.
 *
 * The walk skips nested braces that appear inside the function's
 * type signature (e.g. `CustomEvent<{ sessionId: number | null }>`)
 * so a Svelte 5 typed event handler does not confuse the
 * brace counter.
 */
function extractFunctionBody(source: string, signature: string): string {
  const start = source.indexOf(signature);
  assert.ok(
    start >= 0,
    `expected to find declaration matching ${signature}`,
  );
  // Find the parameter list's closing `)` so any `{` inside a
  // generic type annotation cannot skew the brace walk below.
  const parenStart = source.indexOf("(", start);
  assert.ok(parenStart >= 0, `expected an opening paren after ${signature}`);
  let parenDepth = 1;
  let cursor = parenStart + 1;
  while (parenDepth > 0 && cursor < source.length) {
    const ch = source[cursor];
    if (ch === "(") parenDepth += 1;
    else if (ch === ")") parenDepth -= 1;
    cursor += 1;
  }
  // Skip the return type and `async` decorations until we hit
  // the body's opening `{`.
  const openBrace = source.indexOf("{", cursor);
  assert.ok(openBrace >= 0, `expected an opening brace after ${signature}`);
  let depth = 1;
  cursor = openBrace + 1;
  while (depth > 0 && cursor < source.length) {
    const ch = source[cursor];
    if (ch === "{") depth += 1;
    else if (ch === "}") depth -= 1;
    cursor += 1;
  }
  return source.slice(openBrace, cursor);
}

// ---------------------------------------------------------------------------
// Desktop owns the snapshot: a single helper, single bridge call site.
// ---------------------------------------------------------------------------

test("App.svelte routes every snapshot refresh through a single shared helper", () => {
  // The previous implementation called `peerSnapshotCommand()`
  // inline from `refresh()` (bootstrap) and again from
  // `onPairingClosed` (post-pairing) with no shared guard. The
  // fix collapses both into `refreshPeerSnapshot()` so the
  // single-flight latch can coalesce them. A regression that
  // reintroduced a direct `peerSnapshotCommand()` call from any
  // other site breaks here.
  const directCalls = appSource.match(/peerSnapshotCommand\s*\(\s*\)/g) ?? [];
  assert.equal(
    directCalls.length,
    1,
    `App.svelte must contain exactly one direct peerSnapshotCommand() call (found ${directCalls.length})`,
  );
});

test("App.svelte centralises the snapshot call inside refreshPeerSnapshot", () => {
  // The shared helper is the only place that may invoke
  // `peerSnapshotCommand()`. Locating the helper body by brace
  // balance keeps the assertion robust against the inner IIFE
  // the single-flight guard uses.
  const helperStart = appSource.indexOf("function refreshPeerSnapshot");
  assert.ok(
    helperStart >= 0,
    "App.svelte must declare the shared refreshPeerSnapshot helper",
  );
  const helperBody = extractFunctionBody(
    appSource,
    "function refreshPeerSnapshot",
  );
  assert.match(
    helperBody,
    /peerSnapshotCommand\(\)/,
    "refreshPeerSnapshot must own the only peerSnapshotCommand() call",
  );
});

test("App.svelte refreshPeerSnapshot implements a single-flight guard", () => {
  // The shared helper MUST coalesce overlapping calls. The
  // simplest correct shape is a module-level promise that the
  // helper short-circuits on and clears in a `finally` block so
  // a rejected refresh cannot lock the guard forever and starve
  // the next caller.
  const helperBody = extractFunctionBody(
    appSource,
    "function refreshPeerSnapshot",
  );
  assert.match(
    appSource,
    /let\s+peerSnapshotRefresh\s*:\s*Promise<void>\s*\|\s*null\s*=\s*null/,
    "the single-flight guard must be a module-level promise",
  );
  assert.match(
    helperBody,
    /if\s*\(\s*peerSnapshotRefresh\s*!==\s*null\s*\)/,
    "the shared helper must guard on the in-flight promise",
  );
  assert.match(
    helperBody,
    /return\s+peerSnapshotRefresh\s*;/,
    "the shared helper must reuse the in-flight promise",
  );
  assert.match(
    helperBody,
    /finally\s*\{[^}]*peerSnapshotRefresh\s*=\s*null/,
    "the in-flight promise must clear in a finally block so errors do not lock the guard",
  );
});

test("App.svelte routes bootstrap + post-pairing refreshes through the shared helper", () => {
  // Both `refresh()` and `onPairingClosed()` MUST funnel through
  // the shared `refreshPeerSnapshot()` helper so they cannot
  // open a duplicate bridge call when they overlap.
  const refreshBody = extractFunctionBody(appSource, "async function refresh");
  assert.match(
    refreshBody,
    /refreshPeerSnapshot\(\)/,
    "bootstrap refresh must route through refreshPeerSnapshot",
  );
  assert.doesNotMatch(
    refreshBody,
    /peerSnapshotCommand\s*\(\s*\)/,
    "bootstrap refresh must not call peerSnapshotCommand directly",
  );

  const pairingBody = extractFunctionBody(
    appSource,
    "function onPairingClosed",
  );
  assert.match(
    pairingBody,
    /refreshPeerSnapshot\(\)/,
    "post-pairing refresh must route through refreshPeerSnapshot",
  );
  assert.doesNotMatch(
    pairingBody,
    /peerSnapshotCommand\s*\(\s*\)/,
    "post-pairing refresh must not call peerSnapshotCommand directly",
  );
});

// ---------------------------------------------------------------------------
// Desktop owns the polling cadence: a single interval, mounted only.
// ---------------------------------------------------------------------------

test("App.svelte declares a bounded polling cadence for the snapshot refresh", () => {
  // The cadence MUST be a number constant bounded so a
  // regression that swapped it for a hard-coded string, a
  // requestAnimationFrame loop or a runaway value breaks here.
  // 2 s matches the cadence `PeerSharingModal` already uses
  // internally, so the user experience across both surfaces
  // stays consistent.
  const declaration = appSource.match(
    /const\s+PEER_SNAPSHOT_REFRESH_MS\s*=\s*([0-9][0-9_]*)\s*;/,
  );
  assert.ok(
    declaration,
    "App.svelte must declare a PEER_SNAPSHOT_REFRESH_MS constant",
  );
  const ms = Number(declaration![1].replace(/_/g, ""));
  assert.ok(
    Number.isFinite(ms) && ms > 0 && ms <= 5_000,
    `snapshot refresh cadence must stay bounded (got ${ms} ms)`,
  );
  assert.equal(
    ms,
    2_000,
    "the desktop polling cadence must stay at 2 s to match PeerSharingModal",
  );
  assert.match(
    appSource,
    /setInterval\([\s\S]*?PEER_SNAPSHOT_REFRESH_MS[\s\S]*?\)/,
    "the polling timer must consume PEER_SNAPSHOT_REFRESH_MS",
  );
});

test("App.svelte polling timer routes through the single-flight helper", () => {
  // The interval callback MUST call `refreshPeerSnapshot()` so
  // every tick coalesces with the bootstrap / post-pairing
  // refreshes. A regression that bypassed the shared helper
  // (e.g. a direct `peerSnapshotCommand()` inside the callback)
  // would surface here.
  const intervalMatch = appSource.match(
    /setInterval\([\s\S]*?PEER_SNAPSHOT_REFRESH_MS[\s\S]*?\)/,
  );
  assert.ok(
    intervalMatch,
    "App.svelte must consume PEER_SNAPSHOT_REFRESH_MS inside setInterval",
  );
  assert.match(
    intervalMatch![0],
    /refreshPeerSnapshot\(\)/,
    "the polling tick must call refreshPeerSnapshot",
  );
  assert.doesNotMatch(
    intervalMatch![0],
    /peerSnapshotCommand\s*\(\s*\)/,
    "the polling tick must not call peerSnapshotCommand directly",
  );
});

test("App.svelte installs and tears down exactly one polling interval", () => {
  // `onMount` MUST call `startPeerSnapshotRefresh()` so the
  // cadence begins as soon as the desktop mounts. `onDestroy`
  // MUST call the matching stop helper so a hot reload or a
  // remount never leaks a timer. A regression that paired
  // `setInterval` with `clearInterval` outside `onDestroy`,
  // or that started a second cadence, breaks here.
  const mountMatch = appSource.match(/onMount\s*\(\s*\(\s*\)\s*=>\s*\{([\s\S]*?)\}\s*\)/);
  assert.ok(mountMatch, "App.svelte must declare an onMount hook");
  assert.match(
    mountMatch![1],
    /startPeerSnapshotRefresh\(\)/,
    "onMount must start the polling cadence",
  );
  // The cadence MUST start AFTER the bootstrap refresh so the
  // initial round trip and the first tick coalesce through the
  // single-flight helper.
  const startIdx = mountMatch![1].indexOf("startPeerSnapshotRefresh");
  const refreshIdx = mountMatch![1].indexOf("void refresh()");
  assert.ok(
    refreshIdx >= 0 && startIdx > refreshIdx,
    "the polling cadence must start after the bootstrap refresh",
  );

  const destroyMatch = appSource.match(
    /onDestroy\s*\(\s*\(\s*\)\s*=>\s*\{([\s\S]*?)\}\s*\)/,
  );
  assert.ok(destroyMatch, "App.svelte must declare an onDestroy hook");
  assert.match(
    destroyMatch![1],
    /stopPeerSnapshotRefresh\(\)/,
    "onDestroy must halt the polling cadence",
  );

  // Exactly one start helper and exactly one stop helper must
  // exist. A regression that introduced a second cadence (e.g.
  // a parallel interval to backfill missing presence) would
  // surface here. The regex matches the helper name plus an
  // opening paren; the function declaration (`function
  // startPeerSnapshotRefresh(`) is excluded because the
  // leading `function` keyword is preceded by a space, which the
  // `\n\s*` anchor below rejects.
  const startCallCount = (
    appSource.match(/(^|\n)\s*startPeerSnapshotRefresh\s*\(/g) ?? []
  ).length;
  const stopCallCount = (
    appSource.match(/(^|\n)\s*stopPeerSnapshotRefresh\s*\(/g) ?? []
  ).length;
  assert.equal(
    startCallCount,
    1,
    "App.svelte must own exactly one startPeerSnapshotRefresh call site",
  );
  assert.equal(
    stopCallCount,
    1,
    "App.svelte must own exactly one stopPeerSnapshotRefresh call site",
  );
});

test("App.svelte stopPeerSnapshotRefresh clears the stored interval handle", () => {
  // The stop helper MUST pair `setInterval` with the matching
  // `clearInterval` so a regression that wrote a no-op stop
  // surfaces here.
  assert.match(
    appSource,
    /stopPeerSnapshotRefresh[\s\S]*?clearInterval\(peerSnapshotTimer\)/,
    "stopPeerSnapshotRefresh must clear the interval handle",
  );
  assert.match(
    appSource,
    /peerSnapshotTimer\s*=\s*null/,
    "the interval handle must reset to null after clearInterval",
  );
  // The start helper MUST be idempotent so `onMount` can call
  // it unconditionally without risking a duplicate interval.
  assert.match(
    appSource,
    /function startPeerSnapshotRefresh\(\)[\s\S]*?peerSnapshotTimer\s*!==\s*null\s*\) return/,
    "startPeerSnapshotRefresh must short-circuit when the timer is already set",
  );
});

// ---------------------------------------------------------------------------
// LinkedPeers and RemoteHistoryRail render only the parent snapshot.
// ---------------------------------------------------------------------------

test("LinkedPeers does not start its own timers or open peerSnapshotCommand", () => {
  // The component is presentational: it renders the snapshot the
  // parent supplies and never opens a peer-snapshot bridge call
  // of its own. A regression that started an interval inside
  // `LinkedPeers.svelte` (or imported the bridge helper)
  // surfaces here.
  assert.equal(
    /setInterval\s*\(/.test(linkedPeersSource),
    false,
    "LinkedPeers must not start a setInterval cadence",
  );
  assert.equal(
    /setTimeout\s*\(/.test(linkedPeersSource),
    false,
    "LinkedPeers must not start a setTimeout cadence",
  );
  assert.equal(
    /peerSnapshotCommand/.test(linkedPeersSource),
    false,
    "LinkedPeers must not import or call peerSnapshotCommand",
  );
});

test("RemoteHistoryRail does not start its own timers or open peerSnapshotCommand", () => {
  // The remote rail is the other consumer of the parent
  // snapshot. It must NOT open a peer-snapshot bridge call
  // either — the desktop shell owns the snapshot exclusively
  // and refreshes it on its own cadence.
  assert.equal(
    /setInterval\s*\(/.test(remoteRailSource),
    false,
    "RemoteHistoryRail must not start a setInterval cadence",
  );
  assert.equal(
    /setTimeout\s*\(/.test(remoteRailSource),
    false,
    "RemoteHistoryRail must not start a setTimeout cadence",
  );
  assert.equal(
    /peerSnapshotCommand/.test(remoteRailSource),
    false,
    "RemoteHistoryRail must not import or call peerSnapshotCommand",
  );
});

test("App.svelte wires the reactive snapshot through LinkedPeers and RemoteHistoryRail", () => {
  // The snapshot the shell owns MUST reach both consumers
  // through the documented bindings. A regression that dropped
  // `linkedPeers={peerSnapshot}` from `OrganizationSidebar` or
  // `snapshot={peerSnapshot}` from `RemoteHistoryRail` would
  // silently stop the rail from updating.
  const sidebarTag = appSource.match(/<OrganizationSidebar\b[\s\S]*?\/>/m);
  assert.ok(sidebarTag, "OrganizationSidebar must be rendered");
  assert.match(
    sidebarTag![0],
    /linkedPeers=\{peerSnapshot\}/,
    "OrganizationSidebar must receive the reactive snapshot",
  );
  const railMatch = appSource.match(/<RemoteHistoryRail\b[\s\S]*?\/>/m);
  assert.ok(railMatch, "RemoteHistoryRail must be rendered");
  assert.match(
    railMatch![0],
    /snapshot=\{peerSnapshot\}/,
    "RemoteHistoryRail must receive the reactive snapshot",
  );
});
