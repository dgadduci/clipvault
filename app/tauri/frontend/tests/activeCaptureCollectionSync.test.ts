import { test } from "node:test";
import assert from "node:assert/strict";

import {
  createActiveCaptureCollectionSync,
  resolveActiveCaptureCollectionId,
} from "../src/lib/activeCaptureCollectionSync.ts";

test("active collection scope clears for peers and restores local selection", () => {
  const collections = [
    { id: 17, kind: "user", is_peer_bound: false },
    { id: 18, kind: "user", is_peer_bound: true },
    { id: 19, kind: "user", parent_id: 17, is_peer_bound: false },
    { id: 20, kind: "system", is_peer_bound: false },
  ];
  assert.equal(resolveActiveCaptureCollectionId(17, null, collections), 17);
  assert.equal(resolveActiveCaptureCollectionId(19, null, collections), 19);
  assert.equal(resolveActiveCaptureCollectionId(17, "peer-a", collections), null);
  assert.equal(resolveActiveCaptureCollectionId(18, null, collections), null);
  assert.equal(resolveActiveCaptureCollectionId(20, null, collections), null);
  assert.equal(resolveActiveCaptureCollectionId(null, null, collections), null);
});

test("capture collection updates are applied in selection order", async () => {
  const calls: Array<number | null> = [];
  let releaseFirst: (() => void) | undefined;
  const firstUpdate = new Promise<void>((resolve) => {
    releaseFirst = resolve;
  });
  const sync = createActiveCaptureCollectionSync(async (collectionId) => {
    calls.push(collectionId);
    if (calls.length === 1) await firstUpdate;
  });

  const first = sync(17);
  const second = sync(null);
  await Promise.resolve();
  assert.deepEqual(calls, [17]);

  releaseFirst?.();
  await Promise.all([first, second]);
  assert.deepEqual(calls, [17, null]);
});

test("a failed update does not block the next selection", async () => {
  const calls: Array<number | null> = [];
  const sync = createActiveCaptureCollectionSync(async (collectionId) => {
    calls.push(collectionId);
    if (collectionId === 3) throw new Error("unavailable");
  });

  await assert.rejects(sync(3), /unavailable/);
  await sync(4);
  assert.deepEqual(calls, [3, 4]);
});
