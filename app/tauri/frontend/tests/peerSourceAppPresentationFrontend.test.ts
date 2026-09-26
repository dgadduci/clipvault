import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  isCurrentRemoteSourceAppRequest,
  MAX_REMOTE_SOURCE_APP_ICON_BYTES,
  MAX_REMOTE_SOURCE_APP_NAME_CHARS,
  shouldRequestRemoteSourceAppPresentation,
  supportsRemoteSourceAppPresentation,
  validatedRemoteSourceAppName,
} from "../src/lib/remoteSourceAppPresentation.ts";

const SOURCE = {
  remoteCard: readFileSync(resolve(process.cwd(), "src/RemotePreviewCard.svelte"), "utf8"),
  remoteRail: readFileSync(resolve(process.cwd(), "src/RemoteHistoryRail.svelte"), "utf8"),
  historyCard: readFileSync(resolve(process.cwd(), "src/HistoryCard.svelte"), "utf8"),
  historyRail: readFileSync(resolve(process.cwd(), "src/HistoryCardRail.svelte"), "utf8"),
  app: readFileSync(resolve(process.cwd(), "src/App.svelte"), "utf8"),
  bridge: readFileSync(resolve(process.cwd(), "src/lib/tauri.ts"), "utf8"),
  repository: readFileSync(
    resolve(process.cwd(), "../../../crates/clipvault-db/src/peer_import_repository.rs"),
    "utf8",
  ),
  types: readFileSync(resolve(process.cwd(), "src/types.ts"), "utf8"),
};

const readyVisible = {
  peerId: "peer-a",
  peerStateReady: true,
  capability: "pairing,image_import,source_app_presentation",
  isVisible: true,
  requestInFlight: false,
  phase: "idle" as const,
};

test("source-app request is gated by capability, readiness, selected peer, and visibility", () => {
  assert.equal(shouldRequestRemoteSourceAppPresentation(readyVisible), true);
  assert.equal(
    shouldRequestRemoteSourceAppPresentation({
      ...readyVisible,
      capability: "pairing,image_import",
    }),
    false,
  );
  assert.equal(
    shouldRequestRemoteSourceAppPresentation({ ...readyVisible, isVisible: false }),
    false,
  );
  assert.equal(
    shouldRequestRemoteSourceAppPresentation({ ...readyVisible, peerId: null }),
    false,
  );
  assert.equal(
    shouldRequestRemoteSourceAppPresentation({ ...readyVisible, peerStateReady: false }),
    false,
  );
  assert.equal(
    shouldRequestRemoteSourceAppPresentation({ ...readyVisible, requestInFlight: true }),
    false,
  );
  assert.equal(
    shouldRequestRemoteSourceAppPresentation({ ...readyVisible, phase: "ready" }),
    false,
  );
});

test("capability parser recognizes only the additive source presentation token", () => {
  assert.equal(
    supportsRemoteSourceAppPresentation("pairing, source_app_presentation"),
    true,
  );
  assert.equal(supportsRemoteSourceAppPresentation("pairing,image_import"), false);
  assert.equal(supportsRemoteSourceAppPresentation(null), false);
});

test("source-app names are trimmed, bounded by Unicode characters, and reject controls", () => {
  assert.equal(validatedRemoteSourceAppName("  Safari  "), "Safari");
  assert.equal(validatedRemoteSourceAppName("   "), null);
  assert.equal(validatedRemoteSourceAppName("Safari\nShell"), null);
  assert.equal(
    validatedRemoteSourceAppName("é".repeat(MAX_REMOTE_SOURCE_APP_NAME_CHARS)),
    "é".repeat(MAX_REMOTE_SOURCE_APP_NAME_CHARS),
  );
  assert.equal(
    validatedRemoteSourceAppName("é".repeat(MAX_REMOTE_SOURCE_APP_NAME_CHARS + 1)),
    null,
  );
});

test("stale responses are discarded and limits match the Rust contract", () => {
  assert.equal(isCurrentRemoteSourceAppRequest(7, 7), true);
  assert.equal(isCurrentRemoteSourceAppRequest(7, 8), false);
  assert.equal(MAX_REMOTE_SOURCE_APP_ICON_BYTES, 512 * 1024);
});

test("source presentation is requested by viewport observation and keeps stale URLs scoped to the card", () => {
  assert.match(SOURCE.remoteCard, /sourceAppObserver = new IntersectionObserver/);
  assert.match(SOURCE.remoteCard, /\.remote-history-rail-cards/);
  assert.match(SOURCE.remoteCard, /peerSourceAppPresentationFetchCommand/);
  assert.match(SOURCE.remoteCard, /URL\.revokeObjectURL\(sourceAppIconUrl\)/);
  assert.match(SOURCE.remoteCard, /sourceAppCardIdentity/);
  assert.match(SOURCE.remoteRail, /caps_extra_v2/);
  assert.match(SOURCE.remoteRail, /peerSourceAppPresentationRecordStateCommand/);
  assert.doesNotMatch(
    SOURCE.types.match(/export interface PeerHistoryRow \{[\s\S]*?\n\}/)?.[0] ?? "",
    /source_app_(?:name|icon)/,
  );
  assert.doesNotMatch(
    SOURCE.types.match(/export type PeerImageThumbnailResponse =[\s\S]*?;/)?.[0] ?? "",
    /source_app_(?:name|icon)/,
  );
});

test("imported source attribution renders only from the peer-bound collection projection", () => {
  assert.match(SOURCE.app, /if \(!collection\?\.is_peer_bound\)/);
  assert.match(SOURCE.app, /collection_id: collection\.id/);
  assert.match(SOURCE.historyCard, /history-card-imported-source-app-name/);
  assert.match(SOURCE.historyCard, /IMPORTED_SOURCE_APP_FALLBACK_ICON_SVG/);
  assert.match(SOURCE.historyCard, /source_app_name\?\.trim\(\) \|\| "Aplicación desconocida"/);
  assert.match(SOURCE.historyCard, /sourceAppIconCommand/);
  assert.match(
    SOURCE.historyRail,
    /peerImportedSourceApp=\{peerImportedSourceApps\.get\(entry\.id\) \?\? null\}/,
  );
  assert.match(SOURCE.repository, /JOIN remote_imports ri ON ri\.peer_id = pcb\.peer_id/);
  assert.match(SOURCE.repository, /WHERE pcb\.collection_id = \?1/);
  assert.match(SOURCE.repository, /ORDER BY ri\.imported_at DESC/);
});

test("typed Tauri bridges invoke the source presentation and peer-bound projection with opaque IDs", () => {
  assert.match(
    SOURCE.bridge,
    /invoke<PeerSourceAppPresentationResponse>\(\s*"clipvault_peer_source_app_presentation_fetch",\s*\{\s*peerId: args\.peer_id,\s*remoteEntryId: args\.remote_entry_id/s,
  );
  assert.match(
    SOURCE.bridge,
    /invoke<PeerImportedSourceAppPresentation\[\]>\(\s*"clipvault_peer_import_source_app_presentations",\s*\{\s*collectionId: args\.collection_id,\s*entryIds: args\.entry_ids/s,
  );
  assert.match(SOURCE.app, /\.slice\(0,\s*100\)/);
  assert.match(SOURCE.app, /void refreshPeerImportedSourceApps\(entries\)/);
});
