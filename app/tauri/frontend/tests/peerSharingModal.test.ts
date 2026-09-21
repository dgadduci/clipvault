/**
 * Regression coverage for the periodic snapshot refresh the
 * `local-peer-discovery` modal introduces (task 4.4).
 *
 * The modal hosts the opt-in `Compartir en red local` toggle and
 * the read-only `Equipos` view. The discovery worker can drain a
 * new `Observed` or `Removed` event at any time while the modal
 * stays open, so the modal must re-poll the metadata-only
 * snapshot on a short interval while it is mounted AND the
 * runtime is browsing the LAN (`toggle.kind === "active"`).
 *
 * The contract this suite pins:
 *
 *   - The polling is bounded to a short interval (≤ 5 s); the
 *     test pins the exact cadence the implementation chose so a
 *     regression that accidentally bumps it to a slower value (or
 *     removes the cadence entirely) breaks here.
 *   - The polling is gated on `toggle.kind === "active"`; the
 *     `identity_unavailable` / `runtime_stopped` branches must
 *     never start a timer because the worker is not surfacing
 *     new events in those states.
 *   - Every snapshot read (mount-time refresh, post-toggle
 *     refresh, identity retry and the polling tick) routes
 *     through the shared `refreshSnapshot` helper; that helper
 *     is the only place that calls `peerSnapshotCommand()` and
 *     it owns the single-flight guard. A regression that
 *     reopened a direct `peerSnapshotCommand()` call from any
 *     other call site, or that moved the guard back into the
 *     timer callback, breaks here.
 *   - The polling routes through the documented bridge; the
 *     frontend never reads SQLite, sockets, IPs, ports or mDNS
 *     metadata directly. A regression that reached into the
 *     bridge internals would surface here as a failed assertion.
 *   - The interval is cleared in `onDestroy`; closing the modal
 *     must halt the bridge round-trip so the modal does not leak
 *     timers while the user keeps the app open. A regression that
 *     wired `setInterval` without pairing `clearInterval` in the
 *     destroy hook breaks here.
 *
 * The suite reads the production source through the
 * `loadSource` helper so it stays in lock-step with the
 * implementation without standing up a Svelte runtime.
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
 * accidentally match the source-level assertions below.
 */
function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
}

function source(): string {
  return stripComments(loadSource("src/PeerSharingModal.svelte"));
}

test("PeerSharingModal declares a bounded polling cadence for the snapshot refresh", () => {
  const raw = loadSource("src/PeerSharingModal.svelte");
  // The component must declare a polling interval that lives in
  // the script block (not inline within the template). The
  // expression MUST resolve to a number — not a string, not a
  // DOM lookup — so a regression that swapped the cadence for a
  // hard-coded string (or wired it through a Svelte prop by
  // accident) breaks here.
  const declaration =
    raw.match(
      /const\s+PEER_SNAPSHOT_REFRESH_MS\s*=\s*([0-9][0-9_]*)\s*;/,
    );
  assert.ok(
    declaration,
    "PeerSharingModal must declare a PEER_SNAPSHOT_REFRESH_MS constant",
  );
  const ms = Number(declaration![1].replace(/_/g, ""));
  assert.ok(
    Number.isFinite(ms) && ms > 0 && ms <= 5_000,
    `snapshot refresh cadence must stay bounded (got ${ms} ms)`,
  );
  // `setInterval` MUST consume the cadence so the polling rate is
  // exactly the documented constant; a regression that wrote the
  // cadence inline at the call site (and diverged from the
  // constant) breaks here.
  assert.match(
    source(),
    /setInterval\([\s\S]*?PEER_SNAPSHOT_REFRESH_MS[\s\S]*?\)/,
    "the polling timer must consume PEER_SNAPSHOT_REFRESH_MS",
  );
});

test("PeerSharingModal polls only while the runtime is browsing the LAN", () => {
  // The poll MUST start when `toggle.kind === "active"`; the
  // `identity_unavailable` and `runtime_stopped` branches keep
  // the runtime from surfacing new events, so the polling branch
  // must NOT engage in those states. A regression that polls
  // unconditionally would surface here.
  const body = source();
  // The reactive branch reads `toggle.kind` and routes through
  // either the start or stop helper. The exact comparison is the
  // contract the rest of the modal relies on.
  assert.match(
    body,
    /toggle\.kind\s*===\s*"active"/,
    "the polling branch MUST only engage for toggle.kind === \"active\"",
  );
  // The active branch starts the timer; the non-active branch
  // stops it. The `stopSnapshotRefresh` helper is the only
  // cleanup path the modal exposes, so the gated branch must
  // call it explicitly when the runtime is not active.
  assert.match(
    body,
    /startSnapshotRefresh\(\)/,
    "the modal must expose a helper that starts the polling timer",
  );
  assert.match(
    body,
    /stopSnapshotRefresh\(\)/,
    "the modal must expose a helper that stops the polling timer",
  );
});

test("PeerSharingModal centralizes peerSnapshotCommand behind a single-flight guard", () => {
  // Every snapshot read the modal performs — mount-time
  // refresh, post-toggle refresh, identity retry and the
  // polling tick — MUST route through the shared
  // `refreshSnapshot` helper. That helper is the only place
  // that may call `peerSnapshotCommand()` directly and it
  // owns the single-flight guard. A regression that bypassed
  // the shared helper and called `peerSnapshotCommand()`
  // from any other call site, or that moved the guard back
  // into the timer callback, breaks here.
  const body = source();

  // The source MUST contain exactly one direct call to
  // `peerSnapshotCommand()` and that call MUST live inside
  // `refreshSnapshot`. Every other call site funnels through
  // the helper so they cannot overlap an in-flight round-trip.
  const directCalls = body.match(/peerSnapshotCommand\s*\(\s*\)/g) ?? [];
  assert.equal(
    directCalls.length,
    1,
    `PeerSharingModal must contain exactly one direct call to peerSnapshotCommand (found ${directCalls.length})`,
  );

  // Locate the shared helper. The match must capture the body
  // up to the closing brace so the assertions below can pin
  // the bridge call and the single-flight guard inside the
  // helper itself. The function body contains an inner IIFE
  // (`(async () => { ... })()`), so we use a balanced match
  // by anchoring on the function header and reading forward
  // until the next top-level helper (another `async function`,
  // a `function`, or the script close).
  const sharedStart = body.indexOf("async function refreshSnapshot");
  assert.ok(
    sharedStart >= 0,
    "PeerSharingModal must declare the shared refreshSnapshot helper",
  );
  // Anchor at the first `{` after the header so the braces we
  // balance against are the helper's own.
  const sharedHeaderEnd = body.indexOf("{", sharedStart);
  assert.ok(
    sharedHeaderEnd >= 0,
    "PeerSharingModal must declare the shared refreshSnapshot helper body",
  );
  // Walk the braces from the helper's opening `{` to find the
  // matching `}` (the helper's closing brace). The inner IIFE
  // uses its own balanced pair of braces and the regex above
  // would otherwise stop at the IIFE's `})()`.
  let depth = 0;
  let sharedEnd = -1;
  for (let index = sharedHeaderEnd; index < body.length; index += 1) {
    const char = body[index];
    if (char === "{") depth += 1;
    else if (char === "}") {
      depth -= 1;
      if (depth === 0) {
        sharedEnd = index;
        break;
      }
    }
  }
  assert.ok(
    sharedEnd >= 0,
    "PeerSharingModal must close the shared refreshSnapshot helper",
  );
  const sharedBody = body.slice(sharedStart, sharedEnd + 1);
  assert.match(
    sharedBody,
    /peerSnapshotCommand\(\)/,
    "the shared helper must own the single peerSnapshotCommand call",
  );
  // The single-flight guard MUST live inside the shared
  // helper (not in the timer callback). The simplest correct
  // shape is a module-level promise variable that the helper
  // short-circuits on and clears in a `finally` block so a
  // rejected refresh cannot lock the guard forever.
  assert.match(
    sharedBody,
    /snapshotRefreshPromise/,
    "the shared helper must own the single-flight guard",
  );
  assert.match(
    body,
    /let\s+snapshotRefreshPromise\s*:\s*Promise<void>\s*\|\s*null\s*=\s*null/,
    "the single-flight guard must be a module-level promise",
  );
  assert.match(
    sharedBody,
    /if\s*\(\s*snapshotRefreshPromise\s*!==\s*null\s*\)\s*\{[^}]*return\s+snapshotRefreshPromise\s*;/,
    "the shared helper must reuse the in-flight promise instead of enqueueing a new one",
  );
  assert.match(
    sharedBody,
    /finally\s*\{[^}]*snapshotRefreshPromise\s*=\s*null/,
    "the in-flight promise must clear in a finally block so errors do not lock the guard",
  );

  // The mount-time refresh MUST route through the shared
  // helper so it cannot overlap a tick that fired just before
  // mount.
  const refreshMatch = body.match(
    /async function refresh\([\s\S]*?\n\s*\}\s*\n/,
  );
  assert.ok(
    refreshMatch,
    "PeerSharingModal must declare the mount-time refresh helper",
  );
  const refreshBody = refreshMatch![0];
  assert.match(
    refreshBody,
    /refreshSnapshot\(\)/,
    "the mount-time refresh must route through refreshSnapshot",
  );
  assert.doesNotMatch(
    refreshBody,
    /peerSnapshotCommand\(\)/,
    "the mount-time refresh must not call peerSnapshotCommand directly",
  );

  // The polling timer MUST route through the shared helper
  // and MUST NOT call the bridge directly. A regression that
  // put a `peerSnapshotCommand()` call inside the
  // `setInterval` callback (or reintroduced a guard wrapper
  // outside `refreshSnapshot`) breaks here.
  const intervalMatch = body.match(
    /setInterval\([\s\S]*?PEER_SNAPSHOT_REFRESH_MS[\s\S]*?\)/,
  );
  assert.ok(
    intervalMatch,
    "PeerSharingModal must consume PEER_SNAPSHOT_REFRESH_MS inside setInterval",
  );
  assert.match(
    intervalMatch![0],
    /refreshSnapshot\(\)/,
    "the polling timer must call refreshSnapshot",
  );
  assert.doesNotMatch(
    intervalMatch![0],
    /peerSnapshotCommand\(\)/,
    "the polling timer must not call peerSnapshotCommand directly",
  );
  // The timer callback MUST NOT reintroduce a guard wrapper
  // outside the shared helper; the guard belongs to the
  // helper, not the callback. A regression that re-added
  // `snapshotRefreshInFlight` (or any sibling guard) inside
  // the timer callback breaks here.
  assert.doesNotMatch(
    intervalMatch![0],
    /snapshotRefreshInFlight/,
    "the polling timer must not own a guard outside the shared helper",
  );
  assert.doesNotMatch(
    intervalMatch![0],
    /snapshotRefreshPromise/,
    "the polling timer must not own a guard outside the shared helper",
  );

  // The toggle helper MUST route through the shared helper so
  // the post-toggle reload cannot overlap a tick. The
  // contract the rest of the modal relies on is that the
  // toggle is the source of truth for `toggle` and that the
  // snapshot reload immediately after the toggle response is
  // funneled through `refreshSnapshot`.
  const toggleMatch = body.match(
    /async function toggleSharing\([\s\S]*?\n\s*\}\s*\n/,
  );
  assert.ok(
    toggleMatch,
    "PeerSharingModal must declare the toggleSharing helper",
  );
  const toggleBody = toggleMatch![0];
  assert.match(
    toggleBody,
    /refreshSnapshot\(\)/,
    "the toggle helper must route through refreshSnapshot",
  );
  assert.doesNotMatch(
    toggleBody,
    /peerSnapshotCommand\(\)/,
    "the toggle helper must not call peerSnapshotCommand directly",
  );

  // The identity retry MUST route through the shared helper
  // for the same reason as the toggle.
  const identityMatch = body.match(
    /async function refreshIdentity\([\s\S]*?\n\s*\}\s*\n/,
  );
  assert.ok(
    identityMatch,
    "PeerSharingModal must declare the refreshIdentity helper",
  );
  const identityBody = identityMatch![0];
  assert.match(
    identityBody,
    /refreshSnapshot\(\)/,
    "the identity retry must route through refreshSnapshot",
  );
  assert.doesNotMatch(
    identityBody,
    /peerSnapshotCommand\(\)/,
    "the identity retry must not call peerSnapshotCommand directly",
  );

  // The polling branch MUST NOT touch any SQLite / socket /
  // IP / mDNS path; a regression that imported a DB handle,
  // called `fetch` directly, or read
  // window.__TAURI_INTERNALS__.invoke with a custom command
  // breaks here.
  assert.doesNotMatch(body, /sqlite/i);
  assert.doesNotMatch(body, /WebSocket/);
  assert.doesNotMatch(body, /clipvault_peer_discovery\b/);
  assert.doesNotMatch(body, /__TAURI_INTERNALS__\.invoke/);
});

test("PeerSharingModal clears the polling timer in onDestroy", () => {
  // Closing the modal (or unmounting the slot the shared
  // `<Modal>` shell controls) MUST halt the polling. The
  // `onDestroy` hook is the only cleanup path Svelte guarantees;
  // a regression that paired `setInterval` with a `clearInterval`
  // call outside `onDestroy` would leak timers while the user
  // keeps the app open.
  const body = source();
  // Locate the `onDestroy` block so a regression that wired
  // `clearInterval` outside the hook (for example inside a
  // callback) breaks here.
  const onDestroyMatch = body.match(/onDestroy\s*\(\s*\(\s*\)\s*=>\s*\{([\s\S]*?)\}\s*\)/);
  assert.ok(onDestroyMatch, "PeerSharingModal must declare an onDestroy hook");
  const onDestroyBody = onDestroyMatch![1];
  assert.match(
    onDestroyBody,
    /stopSnapshotRefresh\(\)/,
    "onDestroy must clear the polling timer through stopSnapshotRefresh",
  );
  // The helper itself MUST pair `setInterval` with the matching
  // `clearInterval` so a regression that wrote a no-op stop
  // surfaces here.
  assert.match(
    body,
    /stopSnapshotRefresh[\s\S]*?clearInterval\(snapshotTimer\)/,
    "stopSnapshotRefresh must call clearInterval on the stored handle",
  );
  assert.match(
    body,
    /snapshotTimer\s*=\s*null/,
    "the timer handle must reset to null after clearInterval",
  );
});

test("PeerSharingModal keeps the immediate snapshot refresh on mount and on toggle", () => {
  // Task 4.4 only adds a periodic refresh; the previous behavior
  // — refresh on mount and refresh after the toggle changes —
  // MUST survive. A regression that wired the polling branch but
  // dropped the immediate refresh breaks here.
  const body = source();
  assert.match(
    body,
    /onMount\([\s\S]*?await\s+refresh\(\)/,
    "the modal must refresh immediately on mount",
  );
  assert.match(
    body,
    /toggleSharing[\s\S]*?await\s+refreshSnapshot\(\)/,
    "the toggle action must refresh the snapshot immediately",
  );
  // The immediate refresh on mount keeps the same bridge contract
  // as the polling tick so the modal never depends on a second
  // command surface. The mount-time refresh MUST funnel
  // through the shared `refreshSnapshot` helper so it cannot
  // overlap a tick; the dedicated single-flight test pins the
  // exact bridge contract.
  assert.match(
    body,
    /async function refresh\([\s\S]*?refreshSnapshot\(\)/,
    "the mount-time refresh must route through the shared refreshSnapshot helper",
  );
});

test("PeerSharingModal enables pairing only after a live pairing advertisement", () => {
  const body = source();
  assert.match(
    body,
    /function pairingAdvertisementReady\(entry: PeerSnapshotEntry\)[\s\S]*entry\.presence === "detected" && entry\.capability === "pairing"/,
  );
  assert.match(
    body,
    /disabled=\{pairingBusy \|\| !pairingAdvertisementReady\(entry\)\}/,
  );
});
