import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  MAX_REMOTE_SOURCE_APP_NAME_CHARS,
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
  types: readFileSync(resolve(process.cwd(), "src/types.ts"), "utf8"),
  textHistory: readFileSync(
    resolve(process.cwd(), "../../../crates/clipvault-core/src/peer_text_history.rs"),
    "utf8",
  ),
  imageHistory: readFileSync(
    resolve(process.cwd(), "../../../crates/clipvault-core/src/peer_image_history.rs"),
    "utf8",
  ),
  transport: readFileSync(
    resolve(process.cwd(), "../../../crates/clipvault-platform/src/peer_transport.rs"),
    "utf8",
  ),
  repository: readFileSync(
    resolve(process.cwd(), "../../../crates/clipvault-db/src/peer_import_repository.rs"),
    "utf8",
  ),
  commands: readFileSync(
    resolve(process.cwd(), "../src-tauri/src/commands.rs"),
    "utf8",
  ),
};

test("source-app capability and name validation stay bounded", () => {
  assert.equal(
    supportsRemoteSourceAppPresentation("pairing, source_app_presentation"),
    true,
  );
  assert.equal(supportsRemoteSourceAppPresentation("pairing,image_import"), false);
  assert.equal(supportsRemoteSourceAppPresentation(null), false);
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

test("remote preview displays the browse-row name without fetching or rendering a source icon", () => {
  assert.match(
    SOURCE.remoteCard,
    /sourceAppName = peerSupportsSourceAppPresentation\s*\?\s*validatedRemoteSourceAppName\(row\.source_app_name \?\? null\)/,
  );
  assert.match(SOURCE.remoteCard, /data-testid="remote-preview-card-source-app-name"/);
  assert.doesNotMatch(SOURCE.remoteCard, /peerSourceAppPresentationFetchCommand/);
  assert.doesNotMatch(SOURCE.remoteCard, /remote-preview-card-source-app-icon/);
  assert.doesNotMatch(SOURCE.remoteCard, /sourceAppIconUrl|sourceAppObserver/);
  assert.doesNotMatch(SOURCE.remoteRail, /peerSourceAppPresentationRecordStateCommand/);
  assert.match(SOURCE.remoteRail, /source_app_name: item\.row\.source_app_name/);
});

test("text and image browse contracts carry only an optional source-app name", () => {
  for (const contract of [
    SOURCE.types.match(/export interface PeerHistoryRow \{[\s\S]*?\n\}/)?.[0] ?? "",
    SOURCE.types.match(/export interface PeerImageBrowseRow \{[\s\S]*?\n\}/)?.[0] ?? "",
  ]) {
    assert.match(contract, /source_app_name: string \| null/);
    assert.doesNotMatch(contract, /source_app_icon|asset_ref|path:/);
  }
  assert.match(SOURCE.textHistory, /pub source_app_name: Option<String>/);
  assert.match(SOURCE.imageHistory, /pub source_app_name: Option<String>/);
  assert.match(SOURCE.transport, /pub source_app_name: Option<String>/);
  assert.match(SOURCE.commands, /pub struct PeerHistoryRow \{[\s\S]*?pub source_app_name: Option<String>/);
  assert.match(SOURCE.commands, /pub struct PeerImageBrowseRow \{[\s\S]*?pub source_app_name: Option<String>/);
  assert.match(SOURCE.transport, /#\[serde\(default\)\]\s*pub source_app_name/);
  assert.doesNotMatch(
    SOURCE.types.match(/export type PeerImageThumbnailResponse =[^;]*;/)?.[0] ?? "",
    /source_app_name|source_app_icon/,
  );
});

test("imported source name loads by selected collection and occupies a visible row", () => {
  const refresh = SOURCE.app.match(
    /async function refreshPeerImportedSourceApps\([\s\S]*?\n  \}/,
  )?.[0] ?? "";
  assert.match(refresh, /if \(collectionId === null\)/);
  assert.doesNotMatch(refresh, /is_peer_bound/);
  assert.match(refresh, /collection_id: collectionId/);
  assert.match(SOURCE.app, /void refreshPeerImportedSourceApps\(entries\)/);
  assert.match(SOURCE.historyCard, /history-card-imported-source-app-name/);
  assert.match(SOURCE.historyCard, /source_app_name\?\.trim\(\) \|\| "Aplicación desconocida"/);
  assert.match(SOURCE.historyCard, /class:source-app-imported=\{peerImportedSourceApp !== null\}/);
  assert.match(SOURCE.historyCard, /\.source-app\.source-app-imported\s*\{[\s\S]*?grid-column: 1 \/ -1/);
  assert.match(
    SOURCE.historyRail,
    /peerImportedSourceApp=\{peerImportedSourceApps\.get\(entry\.id\) \?\? null\}/,
  );
  assert.match(SOURCE.repository, /JOIN remote_imports ri ON ri\.peer_id = pcb\.peer_id/);
  assert.match(SOURCE.repository, /WHERE pcb\.collection_id = \?1/);
});

test("peer-bound attribution bridge stays scoped to opaque local entry IDs", () => {
  assert.match(
    SOURCE.bridge,
    /invoke<PeerImportedSourceAppPresentation\[\]>\(\s*"clipvault_peer_import_source_app_presentations",\s*\{\s*collectionId: args\.collection_id,\s*entryIds: args\.entry_ids/s,
  );
  assert.match(SOURCE.app, /\.slice\(0,\s*100\)/);
});
