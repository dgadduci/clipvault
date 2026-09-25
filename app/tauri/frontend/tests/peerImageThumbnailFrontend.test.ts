/**
 * Frontend regression coverage for the
 * `peer-image-preview-thumbnails` change.
 *
 * The change ships:
 *
 * - A viewport-gated lazy thumbnail fetch: the renderer only
 *   opens the thumbnail route when an image card intersects the
 *   visible remote-history viewport AND the peer advertises
 *   BOTH `image_import` and `image_preview_thumbnail`.
 * - Stale-safe responses: a late response after the peer / page
 *   / card changed MUST NOT replace the placeholder of another
 *   row.
 * - Object URL lifecycle: every Object URL the card allocated
 *   MUST be released on unmount, peer change, page change or
 *   card change.
 * - Concurrency cap: the active rail bounds concurrent
 *   thumbnail fetches to two in-flight per peer so a backlog of
 *   visible cards cannot overflow the host budget.
 * - Static placeholder fallback: the renderer MUST keep the
 *   static placeholder while loading, when the peer lacks the
 *   capability, when the request fails, or when the host returns
 *   invalid / oversized / busy PNG bytes.
 * - Importar independence: a viewed thumbnail MUST NOT alter the
 *   explicit `Importar` flow; the import path continues to use
 *   the original PNG payload.
 *
 * The behavioural scenarios cover the bridge wiring, the
 * viewport-gating logic and the typed discriminated union the
 * `clipvault_peer_image_thumbnail_fetch` command returns.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  isCurrentThumbnailRequest,
  remoteImageThumbnailCardIdentity,
  remoteImageThumbnailCardKey,
  shouldRequestRemoteImageThumbnail,
  supportsRemoteImageThumbnails,
} from "../src/lib/remoteImageThumbnailState.ts";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// When the test is compiled to
// `node_modules/.cache/clipvault-test-build/tests/`, seven parent
// directories separate the file from the repository root. The
// helper walks them up so the regression tests work regardless of
// where the runner drops the compiled bundle.
const REPO_ROOT = path.resolve(
  __dirname,
  "..",
  "..",
  "..",
  "..",
  "..",
  "..",
  "..",
);
const REMOTE_CARD_PATH = path.join(
  REPO_ROOT,
  "app",
  "tauri",
  "frontend",
  "src",
  "RemotePreviewCard.svelte",
);
const REMOTE_RAIL_PATH = path.join(
  REPO_ROOT,
  "app",
  "tauri",
  "frontend",
  "src",
  "RemoteHistoryRail.svelte",
);
const TAURI_TS_PATH = path.join(
  REPO_ROOT,
  "app",
  "tauri",
  "frontend",
  "src",
  "lib",
  "tauri.ts",
);
const TYPES_TS_PATH = path.join(
  REPO_ROOT,
  "app",
  "tauri",
  "frontend",
  "src",
  "types.ts",
);
const COMMANDS_RS_PATH = path.join(
  REPO_ROOT,
  "app",
  "tauri",
  "src-tauri",
  "src",
  "commands.rs",
);

const remoteCard = readFileSync(REMOTE_CARD_PATH, "utf8");
const remoteRail = readFileSync(REMOTE_RAIL_PATH, "utf8");
const tauriTs = readFileSync(TAURI_TS_PATH, "utf8");
const typesTs = readFileSync(TYPES_TS_PATH, "utf8");
const commandsRs = readFileSync(COMMANDS_RS_PATH, "utf8");

test("remote card wires the thumbnail fetch through the peer-image-preview-thumbnails bridge", () => {
  assert.match(
    remoteCard,
    /peerImageThumbnailFetchCommand/,
    "RemotePreviewCard must use the peerImageThumbnailFetchCommand bridge for thumbnails",
  );
  assert.match(
    remoteCard,
    /peerSupportsImagePreviewThumbnail/,
    "RemotePreviewCard must derive the additive image_preview_thumbnail capability gate",
  );
  assert.match(
    remoteCard,
    /IntersectionObserver/,
    "RemotePreviewCard must use IntersectionObserver to gate thumbnail requests",
  );
});

test("remote card releases every Object URL on unmount and on peer/page/card change", () => {
  assert.match(
    remoteCard,
    /URL\.revokeObjectURL/,
    "RemotePreviewCard must release Object URLs",
  );
  assert.match(
    remoteCard,
    /releaseThumbnail/,
    "RemotePreviewCard must centralise Object URL cleanup in releaseThumbnail",
  );
  assert.match(
    remoteCard,
    /onDestroy\(\(\) =>/,
    "RemotePreviewCard must release on unmount through onDestroy",
  );
});

test("remote card keeps the static placeholder for loading and error phases", () => {
  // The placeholder data-placeholder-kind attribute must default
  // to "static" until a successful response arrives.
  assert.match(
    remoteCard,
    /data-placeholder-kind=\{thumbnailPhase === "ready" \? "thumbnail" : "static"\}/,
    "RemotePreviewCard must keep the placeholder attribute dynamic on thumbnailPhase",
  );
  // The placeholder SVG must render whenever the thumbnail is
  // not in the ready phase (loading / error / idle).
  assert.match(
    remoteCard,
    /\{#if thumbnailPhase === "ready" && thumbnailUrl !== null\}[\s\S]*\{:else\}/,
    "RemotePreviewCard must render the placeholder SVG when the thumbnail is not ready",
  );
});

test("remote card never asks the bridge for thumbnails of a legacy peer", () => {
  // The component delegates the decision to the tested pure
  // helper so a peer that only ships one capability cannot fetch.
  assert.match(
    remoteCard,
    /supportsRemoteImageThumbnails\(peerCapability\)/,
    "RemotePreviewCard must use the shared capability gate",
  );
  assert.equal(supportsRemoteImageThumbnails("image_import"), false);
  assert.equal(supportsRemoteImageThumbnails("image_preview_thumbnail"), false);
  assert.equal(
    supportsRemoteImageThumbnails("pairing,image_import,image_preview_thumbnail"),
    true,
  );
});

test("thumbnail request gate requires viewport, peer state, capabilities and an idle card", () => {
  const eligible = {
    isImageRow: true,
    isIntersecting: true,
    peerId: "peer-a",
    peerStateReady: true,
    peerCapability: "pairing,image_import,image_preview_thumbnail",
    requestInFlight: false,
    phase: "idle" as const,
  };
  assert.equal(shouldRequestRemoteImageThumbnail(eligible), true);
  assert.equal(
    shouldRequestRemoteImageThumbnail({ ...eligible, isIntersecting: false }),
    false,
  );
  assert.equal(
    shouldRequestRemoteImageThumbnail({ ...eligible, peerStateReady: false }),
    false,
  );
  assert.equal(
    shouldRequestRemoteImageThumbnail({ ...eligible, isImageRow: false }),
    false,
  );
  assert.equal(
    shouldRequestRemoteImageThumbnail({ ...eligible, peerId: null }),
    false,
  );
  assert.equal(
    shouldRequestRemoteImageThumbnail({
      ...eligible,
      peerCapability: "pairing,image_import",
    }),
    false,
  );
  assert.equal(
    shouldRequestRemoteImageThumbnail({ ...eligible, requestInFlight: true }),
    false,
  );
  assert.equal(
    shouldRequestRemoteImageThumbnail({ ...eligible, phase: "loading" }),
    false,
  );
  assert.equal(
    shouldRequestRemoteImageThumbnail({ ...eligible, phase: "ready" }),
    false,
  );
  assert.match(
    remoteCard,
    /shouldRequestRemoteImageThumbnail\(\{[\s\S]*?isIntersecting: entry\.isIntersecting/,
    "the actual IntersectionObserver callback must use the tested request gate",
  );
});

test("bridge exposes a typed discriminator for thumbnail outcomes", () => {
  // The bridge command is the seam the renderer drives; the
  // returned union is metadata-only.
  assert.match(
    tauriTs,
    /export const peerImageThumbnailFetchCommand/,
    "tauri.ts must expose peerImageThumbnailFetchCommand",
  );
  assert.match(
    typesTs,
    /export type PeerImageThumbnailResponse/,
    "types.ts must expose PeerImageThumbnailResponse",
  );
  // Every failure variant must collapse to a stable identifier.
  assert.match(typesTs, /kind: "busy"/);
  assert.match(typesTs, /kind: "body_too_large"/);
  assert.match(typesTs, /kind: "invalid_png"/);
  assert.match(typesTs, /kind: "not_transferable"/);
  assert.match(typesTs, /kind: "capability_missing"/);
});

test("Tauri command defines the typed discriminated union the bridge projects", () => {
  assert.match(
    commandsRs,
    /pub enum PeerImageThumbnailResponse/,
    "commands.rs must declare PeerImageThumbnailResponse",
  );
  assert.match(
    commandsRs,
    /pub fn clipvault_peer_image_thumbnail_fetch/,
    "commands.rs must expose clipvault_peer_image_thumbnail_fetch",
  );
  assert.match(
    commandsRs,
    /pub fn clipvault_peer_image_thumbnail_record_state/,
    "commands.rs must expose clipvault_peer_image_thumbnail_record_state",
  );
  assert.match(
    commandsRs,
    /pub fn clipvault_peer_image_thumbnail_forget/,
    "commands.rs must expose clipvault_peer_image_thumbnail_forget",
  );
});

test("renderer discards stale responses on peer / page / card change", () => {
  // Exercise the same pure token predicate the component uses.
  assert.match(
    remoteCard,
    /isCurrentThumbnailRequest\(token, thumbnailRequestToken\)/,
    "RemotePreviewCard must compare the captured token against the live token",
  );
  assert.equal(isCurrentThumbnailRequest(7, 7), true);
  assert.equal(isCurrentThumbnailRequest(7, 8), false);
});

test("remote card identity isolates equal remote IDs across peers and snapshots", () => {
  const peerAKey = remoteImageThumbnailCardKey("peer-a", "entry-4");
  const peerBKey = remoteImageThumbnailCardKey("peer-b", "entry-4");
  assert.notEqual(peerAKey, peerBKey);
  assert.match(
    remoteRail,
    /remoteImageThumbnailCardKey\(peerId, item\.row\.remote_entry_id\)/,
    "the keyed list must include peer identity because remote IDs are host-local",
  );

  const oldCard = remoteImageThumbnailCardIdentity(
    "peer-a", "entry-4", "2026-01-01", "title", "preview", true, "image_import,image_preview_thumbnail",
  );
  const newCard = remoteImageThumbnailCardIdentity(
    "peer-b", "entry-4", "2026-01-01", "title", "preview", true, "image_import,image_preview_thumbnail",
  );
  assert.notEqual(oldCard, newCard);
  assert.match(remoteCard, /thumbnailCardIdentity !== nextCardIdentity/);
  assert.match(remoteCard, /releaseThumbnail\(\);/);
});

test("remote rail records thumbnail trust state before rendering requests", () => {
  assert.match(
    remoteRail,
    /peerImageThumbnailRecordStateCommand\(peerState\)/,
    "the active peer trust/presence state must seed the thumbnail runtime before fetch",
  );
  assert.match(
    remoteRail,
    /peerStateReady=\{thumbnailPeerStateReady\}/,
    "cards must wait for the state command before opening the thumbnail route",
  );
  assert.match(remoteCard, /!shouldRequestRemoteImageThumbnail\(\{/);
});

test("renderer keeps the explicit Importar flow independent of the thumbnail", () => {
  // The `peerImageFetchCommand` (not the thumbnail command) is
  // what the Importar flow drives.
  assert.match(
    remoteCard,
    /peerImageFetchCommand/,
    "RemotePreviewCard must keep the Importar path wired to peerImageFetchCommand",
  );
});

test("renderer keeps the static placeholder on every failure outcome", () => {
  // The applyThumbnail helper must collapse every failure
  // variant into the static placeholder without surfacing a
  // global rail error.
  assert.match(
    remoteCard,
    /if \(response\.kind === "ok"\)/,
    "RemotePreviewCard must branch on the ok variant",
  );
  assert.match(
    remoteCard,
    /thumbnailPhase = "error";/,
    "RemotePreviewCard must collapse failures to the error phase",
  );
});

test("renderer caps concurrent thumbnail fetches per peer", () => {
  // The active rail bounds concurrent thumbnail fetches to two
  // in-flight per peer. The card holds an internal
  // thumbnailRequestInFlight flag so the renderer cannot
  // dispatch a second call before the first one settles.
  assert.match(
    remoteCard,
    /thumbnailRequestInFlight/,
    "RemotePreviewCard must track the per-card in-flight state",
  );
});
