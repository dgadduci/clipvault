/**
 * Regression coverage for the `peer-text-import` eligibility cache.
 *
 * History browsing and explicit import share the same trusted/present peer
 * predicate, but each core service owns its own in-memory cache. A previous
 * implementation refreshed only the history cache when selecting a remote
 * peer, so Importar rejected the already-visible row as `no_known_peer`
 * before it could open the authenticated request.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const FRONTEND_ROOT = process.cwd();

function source(file: string): string {
  return readFileSync(path.join(FRONTEND_ROOT, "src", file), "utf8")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "");
}

const rail = source("RemoteHistoryRail.svelte");

test("remote peer selection synchronizes the import eligibility cache", () => {
  assert.match(
    rail,
    /import\s*\{[\s\S]*?peerImportRecordStateCommand[\s\S]*?\}\s*from\s*["']\.\/lib\/tauri["']/,
    "RemoteHistoryRail must import the import state bridge",
  );

  const refresh = rail.match(
    /(?:async\s+)?function refreshPeerState\([\s\S]*?(?=\n\s*(?:async\s+)?function loadInitialPage\()/,
  );
  assert.ok(refresh, "RemoteHistoryRail must retain refreshPeerState");
  assert.match(
    refresh[0],
    /peerHistoryRecordStateCommand\(peerState\)/,
    "the history cache must continue receiving the selected peer state",
  );
  assert.match(
    refresh[0],
    /peerImportRecordStateCommand\(peerState\)/,
    "the import cache must receive the same selected peer state before Importar",
  );
  assert.match(
    refresh[0],
    /trusted:\s*entry\?\.trust_state\s*===\s*["']trusted["']/,
    "the import cache must use the snapshot's trust state",
  );
  assert.match(
    refresh[0],
    /active:\s*entry\?\.is_present\s*\?\?\s*false/,
    "the import cache must use the snapshot's presence state",
  );
});
