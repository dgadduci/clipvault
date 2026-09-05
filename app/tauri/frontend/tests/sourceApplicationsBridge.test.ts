/**
 * Tests for the source-application filter bridge the combobox
 * consumes. The contract the desktop relies on is:
 *
 * - `sourceApplicationsCommand` targets `clipvault_source_applications`
 *   and forwards `collectionId` and `tagIds` unchanged;
 * - `recentEntriesFilteredCommand` and `searchEntriesCommand`
 *   forward the optional `sourceApp` filter (or `null`) so the
 *   backend can apply the new facet without an extra round-trip;
 * - the combobox only ever sees metadata: `source_app`,
 *   `display_name`, `icon_ref` and the `fallback` flag. The bridge
 *   must never echo clipboard content, hashes or snippets through
 *   the response.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  sourceApplicationsCommand,
  recentEntriesFilteredCommand,
  searchEntriesCommand,
} from "../src/lib/tauri.ts";

type InvokeHandle = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

function installTauriMock(invoker: InvokeHandle): void {
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: { invoke: invoker },
  };
}

test("sourceApplicationsCommand targets the new Tauri command", async () => {
  let observed: { cmd: string; args?: Record<string, unknown> } | undefined;
  installTauriMock(async (cmd, args) => {
    observed = { cmd, args };
    return { options: [], scope: { collection_id: null, tag_ids: [] } };
  });
  await sourceApplicationsCommand({ collectionId: null, tagIds: [] });
  assert.equal(observed?.cmd, "clipvault_source_applications");
  assert.equal(observed?.args?.collectionId, null);
  assert.deepEqual(observed?.args?.tagIds, []);
});

test("sourceApplicationsCommand forwards collection id and tag ids verbatim", async () => {
  let observed: Record<string, unknown> | undefined;
  installTauriMock(async (_cmd, args) => {
    observed = args;
    return { options: [], scope: { collection_id: 7, tag_ids: [1, 2, 3] } };
  });
  await sourceApplicationsCommand({ collectionId: 7, tagIds: [1, 2, 3] });
  assert.equal(observed?.collectionId, 7);
  assert.deepEqual(observed?.tagIds, [1, 2, 3]);
});

test("recentEntriesFilteredCommand forwards the source-app filter", async () => {
  let observed: Record<string, unknown> | undefined;
  installTauriMock(async (_cmd, args) => {
    observed = args;
    return [];
  });
  await recentEntriesFilteredCommand({
    limit: 25,
    collectionId: 2,
    tagIds: [],
    sourceApp: { kind: "known", source_app: "com.example.Editor" },
  });
  assert.equal(observed?.limit, 25);
  assert.equal(observed?.collectionId, 2);
  assert.deepEqual(observed?.tagIds, []);
  assert.equal(
    (observed?.sourceApp as { kind: string }).kind,
    "known",
  );
  assert.equal(
    (observed?.sourceApp as { source_app: string }).source_app,
    "com.example.Editor",
  );
});

test("recentEntriesFilteredCommand passes null when the filter is absent", async () => {
  let observed: Record<string, unknown> | undefined;
  installTauriMock(async (_cmd, args) => {
    observed = args;
    return [];
  });
  await recentEntriesFilteredCommand({
    limit: 25,
    collectionId: null,
    tagIds: [],
  });
  assert.equal(observed?.sourceApp, null);
});

test("searchEntriesCommand forwards the source-app filter", async () => {
  let observed: Record<string, unknown> | undefined;
  installTauriMock(async (_cmd, args) => {
    observed = args;
    return { note: "ok", hits: [] };
  });
  await searchEntriesCommand({
    query: "hello",
    limit: 25,
    collectionId: null,
    tagIds: [],
    sourceApp: { kind: "unknown" },
  });
  assert.equal(
    (observed?.sourceApp as { kind: string }).kind,
    "unknown",
  );
});

test("searchEntriesCommand passes null when the filter is absent", async () => {
  let observed: Record<string, unknown> | undefined;
  installTauriMock(async (_cmd, args) => {
    observed = args;
    return { note: "ok", hits: [] };
  });
  await searchEntriesCommand({
    query: "hello",
    limit: 25,
    collectionId: null,
    tagIds: [],
  });
  assert.equal(observed?.sourceApp, null);
});
