/**
 * Tests for the `search-title-matching` change. The contract the
 * desktop and Quick Paste surfaces rely on is:
 *
 * - `searchEntriesCommand` targets `clipvault_search_entries` and
 *   forwards `query`, `limit`, `collectionId`, `tagIds` and the
 *   optional `sourceApp` filter unchanged.
 * - `runSearch` resolves with a `SearchResponse` whose hits carry
 *   the full `EntryRecord`, including `title`, `asset_ref`,
 *   `mime_type`, `payload_width` and `payload_height`. A title-only
 *   hit (text or image) MUST surface through the helper without
 *   filtering; both consumers (App.svelte and QuickPaste.svelte)
 *   depend on the record staying intact so the card or item can
 *   render its thumbnail.
 * - The helper propagates `note`, debounces, cancels stale
 *   responses and rejects with the backend error otherwise. The
 *   title-search contract adds no extra behaviours on top of the
 *   pre-existing search contract: it is purely additive on the
 *   backend and the bridge must remain a transparent passthrough.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  CLIPBOARD_ASSET_PREFIX,
} from "../src/lib/clipboardAsset.ts";
import { runSearch } from "../src/lib/search.ts";
import { searchEntriesCommand } from "../src/lib/tauri.ts";
import type { EntryRecord, SearchResponse } from "../src/types.ts";

type InvokeHandle = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

function installTauriMock(invoker: InvokeHandle): void {
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: { invoke: invoker },
  };
}

function imageRecord(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 17,
    content: "",
    content_type: "image",
    content_size: 2048,
    content_hash: "f".repeat(64),
    source_app: "com.apple.Preview",
    is_pinned: false,
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
    last_seen_at: "2026-01-02T03:04:05Z",
    title: "Mockup Final",
    source_app_name: "Preview",
    source_app_icon_ref: null,
    asset_ref: `${CLIPBOARD_ASSET_PREFIX}${"f".repeat(64)}.png`,
    mime_type: "image/png",
    payload_width: 640,
    payload_height: 480,
    rich_text_hash: null,
    rich_html_ref: null,
    rich_rtf_ref: null,
    rich_preview_ref: null,
    rich_html_size: null,
    rich_rtf_size: null,
    ...overrides,
  };
}

function textRecord(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 1,
    content: "captured note",
    content_type: "text",
    content_size: 14,
    content_hash: "h",
    source_app: "test",
    is_pinned: false,
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
    last_seen_at: "2026-01-02T03:04:05Z",
    title: "Proyecto Alfa",
    source_app_name: null,
    source_app_icon_ref: null,
    asset_ref: null,
    mime_type: null,
    payload_width: null,
    payload_height: null,
    rich_text_hash: null,
    rich_html_ref: null,
    rich_rtf_ref: null,
    rich_preview_ref: null,
    rich_html_size: null,
    rich_rtf_size: null,
    ...overrides,
  };
}

function hitFrom(record: EntryRecord, score: number): SearchResponse["hits"][number] {
  return {
    entry_id: record.id,
    snippet: record.content_type === "image" ? "" : `snippet-${record.id}`,
    score,
    record,
  };
}

test("searchEntriesCommand targets clipvault_search_entries with the typed query", async () => {
  let observed: { cmd: string; args?: Record<string, unknown> } | undefined;
  installTauriMock(async (cmd, args) => {
    observed = { cmd, args };
    return { note: "ok", hits: [] };
  });
  await searchEntriesCommand({ query: "proyecto", limit: 25 });
  assert.equal(observed?.cmd, "clipvault_search_entries");
  assert.equal(observed?.args?.query, "proyecto");
  assert.equal(observed?.args?.limit, 25);
  assert.equal(observed?.args?.collectionId, null);
  assert.deepEqual(observed?.args?.tagIds, []);
  assert.equal(observed?.args?.sourceApp, null);
});

test("runSearch delivers a title-only text hit to both consumers", async () => {
  const text = textRecord({ title: "Proyecto Alfa" });
  const response: SearchResponse = {
    note: "ok",
    hits: [hitFrom(text, 800)],
  };
  const controller = runSearch({
    query: "proyecto",
    debounceMs: 0,
    invoke: async () => response,
  });
  const result = await controller.result;

  assert.equal(result.note, "ok");
  assert.equal(result.hits.length, 1);
  // The full record survives: a title-only hit must keep the title
  // visible to App.svelte and QuickPaste.svelte.
  const record = result.hits[0].record;
  assert.equal(record.id, text.id);
  assert.equal(record.title, "Proyecto Alfa");
  assert.equal(record.content_type, "text");
});

test("runSearch delivers a title-only image hit with the full image metadata", async () => {
  const image = imageRecord({
    title: "Mockup Final",
    asset_ref: `${CLIPBOARD_ASSET_PREFIX}${"a".repeat(64)}.png`,
    payload_width: 320,
    payload_height: 200,
  });
  const response: SearchResponse = {
    note: "ok",
    hits: [hitFrom(image, 800)],
  };
  const controller = runSearch({
    query: "mockup",
    debounceMs: 0,
    invoke: async () => response,
  });
  const result = await controller.result;

  assert.equal(result.note, "ok");
  assert.equal(result.hits.length, 1);
  const record = result.hits[0].record;
  assert.equal(record.id, image.id);
  assert.equal(record.title, "Mockup Final");
  // Image metadata must travel intact so HistoryCard and QuickPaste
  // can resolve the thumbnail bridge. A regression that dropped the
  // asset reference here would surface as a broken thumbnail without
  // any other symptom.
  assert.equal(record.content_type, "image");
  assert.equal(
    record.asset_ref,
    `${CLIPBOARD_ASSET_PREFIX}${"a".repeat(64)}.png`,
  );
  assert.equal(record.mime_type, "image/png");
  assert.equal(record.payload_width, 320);
  assert.equal(record.payload_height, 200);
});

test("runSearch keeps the existing debounce and cancellation contract for title queries", async () => {
  // The title-only contract adds no behaviour on top of the existing
  // helper. Reuse the same shape exercised in `search.test.ts` but
  // with a title-only image response so a future regression that
  // dropped title-only responses on the cancel path surfaces here.
  type Deferred = {
    resolve: (response: SearchResponse) => void;
    reject: (error: unknown) => void;
  };
  const deferreds: Deferred[] = [];
  const invoke = (_q: string): Promise<SearchResponse> =>
    new Promise<SearchResponse>((resolve, reject) => {
      deferreds.push({ resolve, reject });
    });

  const image = imageRecord({ title: "Mockup Final" });
  const first = runSearch({
    query: "mockup",
    debounceMs: 0,
    invoke,
  });
  first.cancel();
  const second = runSearch({
    query: "mockup",
    debounceMs: 0,
    invoke,
  });

  // Resolve the cancelled invocation first; it must not settle.
  deferreds[0]!.resolve({
    note: "ok",
    hits: [hitFrom(image, 800)],
  });
  const firstSettled = await Promise.race([
    first.result
      .then(() => "resolved")
      .catch(() => "rejected"),
    new Promise((resolve) => setImmediate(() => resolve("pending"))),
  ]);
  assert.equal(firstSettled, "pending");

  // The newer invocation resolves normally with the image record.
  deferreds[1]!.resolve({
    note: "ok",
    hits: [hitFrom(image, 800)],
  });
  const secondResult = await second.result;
  assert.equal(secondResult.hits.length, 1);
  assert.equal(secondResult.hits[0].record.content_type, "image");
  assert.equal(secondResult.hits[0].record.title, "Mockup Final");
});

test("runSearch surfaces empty_query for whitespace title queries without losing state", async () => {
  let invocations = 0;
  const controller = runSearch({
    query: "   ",
    debounceMs: 0,
    invoke: async () => {
      invocations += 1;
      return { note: "empty_query", hits: [] };
    },
  });
  const result = await controller.result;
  assert.equal(invocations, 1);
  assert.equal(result.note, "empty_query");
  assert.equal(result.hits.length, 0);
});

test("runSearch returns both text and image title-only hits in the same response", async () => {
  // Both surfaces iterate `hits[].record` without filtering, so a
  // title-only response must round-trip a mixed-type hit list
  // intact. Pin the contract that title-only hits never get dropped
  // for either type.
  const text = textRecord({ id: 1, title: "Proyecto Alfa" });
  const image = imageRecord({ id: 2, title: "Mockup Final" });
  const controller = runSearch({
    query: "title-only",
    debounceMs: 0,
    invoke: async () => ({
      note: "ok",
      hits: [
        hitFrom(text, 800),
        hitFrom(image, 600),
      ],
    }),
  });
  const result = await controller.result;
  assert.equal(result.note, "ok");
  assert.equal(result.hits.length, 2);
  const records = result.hits.map((h) => h.record);
  assert.equal(records[0].content_type, "text");
  assert.equal(records[0].title, "Proyecto Alfa");
  assert.equal(records[1].content_type, "image");
  assert.equal(records[1].title, "Mockup Final");
  assert.ok(records[1].asset_ref, "image hit must carry an asset_ref");
});
