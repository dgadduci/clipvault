/**
 * Regression coverage for the preview/import synchronization race. A remote
 * preview is metadata-only, so it may appear before the runtime import caches
 * receive the current peer state. The UI must not invoke either import route
 * during that short interval.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

import { canImportRemoteEntry } from "../src/lib/remoteImportReadiness.ts";

const frontendRoot = process.cwd();

function source(...segments: string[]): string {
  return readFileSync(path.join(frontendRoot, ...segments), "utf8");
}

test("remote import waits for the current peer-state synchronization", () => {
  const pendingText = {
    peerId: "peer-a",
    peerStateReady: false,
    isImageRow: false,
    peerSupportsImageImport: true,
    busy: false,
  };

  assert.equal(canImportRemoteEntry(pendingText), false);
  assert.equal(canImportRemoteEntry({ ...pendingText, peerStateReady: true }), true);
  assert.equal(
    canImportRemoteEntry({ ...pendingText, peerStateReady: true, isImageRow: true, peerSupportsImageImport: false }),
    false,
  );
});

test("remote card disables import and its handler cannot bridge before readiness", () => {
  const card = source("src", "RemotePreviewCard.svelte");
  const rail = source("src", "RemoteHistoryRail.svelte");

  assert.match(card, /importReady = canImportRemoteEntry\(/);
  assert.match(card, /if \(peerId === null \|\| !importReady\) return;/);
  assert.match(card, /disabled=\{!importReady\}/);
  assert.match(card, /remote\.import\.preparing/);
  assert.match(rail, /peerImportStateReady = false;/);
  assert.match(rail, /peerImportStateReady = true;/);
  assert.match(rail, /peerStateReady=\{peerImportStateReady\}/);
});

test("import preparation copy is localized in every supported language", () => {
  for (const locale of ["en", "es", "pt", "de", "fr"]) {
    const catalog = JSON.parse(source("src", "locales", `${locale}.json`));
    assert.equal(typeof catalog["remote.import.preparing"], "string");
    assert.ok(catalog["remote.import.preparing"].trim().length > 0);
  }
});

test("text import explains an older host instead of showing a generic unavailable error", () => {
  const card = source("src", "RemotePreviewCard.svelte");
  const types = source("src", "types.ts");

  assert.match(types, /kind: "host_import_unavailable"/);
  assert.match(card, /case "host_import_unavailable":\s+return "remote\.import\.host_update_required"/);
  for (const locale of ["en", "es", "pt", "de", "fr"]) {
    const catalog = JSON.parse(source("src", "locales", `${locale}.json`));
    assert.equal(typeof catalog["remote.import.host_update_required"], "string");
    assert.ok(catalog["remote.import.host_update_required"].trim().length > 0);
  }
});
