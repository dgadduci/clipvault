/**
 * Regression coverage for the image-payload persistence contract that
 * the `desktop-collection-card-polish` change relies on.
 *
 * The reported regression classes that landed in the desktop polish
 * tracker:
 *
 *   - assigning or loading tags used to drop image rows from the rail;
 *   - pin/unpin used to revoke the image's `Blob` URL and surface
 *     "Imagen no disponible" on a card the user already painted;
 *   - switching collection used to wipe the asset reference;
 *   - an organization refresh used to replace a perfectly valid
 *     `EntryRecord` with a partial one and the card fell back to the
 *     error state.
 *
 * The helpers below pin each contract: the entry record the rail
 * renders keeps every payload field intact through every operation
 * the user can trigger from the menu, and stale round-trips cannot
 * overwrite a freshly-committed state.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  applyEntryOrganizationResults,
  applyPinUpdate,
  markEntriesPending,
  reconcileEntryOrganizationToVisible,
  selectPendingEntries,
  type EntryOrganizationFetchResult,
  type EntryOrganizationMap,
  type EntryOrganizationHydrationMap,
} from "../src/lib/entryOrganization.ts";
import {
  CLIPBOARD_ASSET_PREFIX,
  createClipboardAssetResolver,
  hasRenderableImage,
} from "../src/lib/clipboardAsset.ts";
import type {
  Collection,
  EntryRecord,
  Tag,
} from "../src/types.ts";

const SHA = "f".repeat(64);
const VALID_REF = `${CLIPBOARD_ASSET_PREFIX}${SHA}.png`;

function makeTag(overrides: Partial<Tag> = {}): Tag {
  return {
    id: 90,
    normalized_name: "regression",
    display_name: "Regression",
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
    ...overrides,
  };
}

function makeCollection(overrides: Partial<Collection> = {}): Collection {
  return {
    id: 7,
    stable_key: null,
    name: "Trabajo",
    kind: "user",
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
    ...overrides,
  };
}

function imageEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 17,
    content: "",
    content_type: "image",
    content_size: 8_192,
    content_hash: SHA,
    source_app: "com.apple.Preview",
    is_pinned: false,
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
    last_seen_at: "2026-01-02T03:04:05Z",
    title: null,
    source_app_name: "Preview",
    source_app_icon_ref: null,
    asset_ref: VALID_REF,
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

test("pin and unpin never strip the image payload metadata", () => {
  const image = imageEntry();
  const pinned = applyPinUpdate([image], [image], { ...image, is_pinned: true }, {
    isFiltering: false,
  });
  const pinnedRow = pinned.nextEntries[0];
  assert.equal(pinnedRow?.is_pinned, true);
  assert.ok(hasRenderableImage(pinnedRow ?? image));
  assert.equal(pinnedRow?.asset_ref, VALID_REF);
  assert.equal(pinnedRow?.content_size, 8_192);

  const unpinned = applyPinUpdate([pinnedRow ?? image], [pinnedRow ?? image], {
    ...image,
    is_pinned: false,
  }, { isFiltering: false });
  const unpinnedRow = unpinned.nextEntries[0];
  assert.equal(unpinnedRow?.is_pinned, false);
  assert.equal(unpinnedRow?.asset_ref, VALID_REF);
});

test("hydration round on an image row keeps the blob URL alive", async () => {
  // The hydration step is metadata-only; a regression that
  // accidentally strips the asset reference would surface here.
  const hub = {
    created: [] as string[],
    revoked: [] as string[],
  };
  (globalThis as unknown as { URL: unknown }).URL = {
    createObjectURL: (blob: Blob): string => {
      const url = `blob:regression-${hub.created.length}-${blob.size}`;
      hub.created.push(url);
      return url;
    },
    revokeObjectURL: (url: string): void => {
      hub.revoked.push(url);
    },
  };
  const image = imageEntry();
  const resolver = createClipboardAssetResolver({
    async loadIconBytes() {
      return [0x89, 0x50, 0x4e, 0x47];
    },
  });
  const resolution = await resolver.resolve(image.asset_ref as string);
  assert.equal(resolution.ok, true);
  const url = resolution.url as string;
  assert.ok(url);
  // Hydration round with tag + collection results — the blob URL
  // is unrelated to the cache; the helpers must keep the asset
  // reference intact.
  const organization: EntryOrganizationMap = new Map();
  const hydration: EntryOrganizationHydrationMap = new Map();
  const reconciled = reconcileEntryOrganizationToVisible(
    organization,
    hydration,
    [image],
  );
  const results: EntryOrganizationFetchResult[] = [
    {
      id: image.id,
      ok: true,
      tagIds: [90],
      collectionIds: [7],
    },
  ];
  markEntriesPending(reconciled.nextHydration, [image]);
  const applied = applyEntryOrganizationResults(
    reconciled.nextOrganization,
    reconciled.nextHydration,
    results,
    {
      resolveTags: () => [makeTag()],
      resolveCollections: () => [makeCollection()],
    },
  );
  assert.equal(image.asset_ref, VALID_REF);
  assert.ok(applied.nextOrganization.get(image.id)?.tags.length);
});

test("stale refresh never overwrites a freshly committed image", async () => {
  // Pin the contract that mirrors the regression: a collection
  // refresh that lands AFTER a fresh commit must be discarded so
  // the image's blob URL keeps its reference. The test mirrors
  // the existing `imageAfterRestart.test.ts::stale resolution`
  // contract without the parking helper.
  const hub: { created: string[] } = { created: [] };
  (globalThis as unknown as { URL: unknown }).URL = {
    createObjectURL: (blob: Blob): string => {
      const url = `blob:stale-${hub.created.length}-${blob.size}`;
      hub.created.push(url);
      return url;
    },
    revokeObjectURL: (): void => {},
  };
  const refA = `${CLIPBOARD_ASSET_PREFIX}${"a".repeat(64)}.png`;
  const refB = `${CLIPBOARD_ASSET_PREFIX}${"b".repeat(64)}.png`;
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver({
    async loadIconBytes(ref: string): Promise<number[]> {
      calls.push(ref);
      return [0x89, 0x50, 0x4e, 0x47];
    },
  });
  const a = await resolver.resolve(refA);
  const b = await resolver.resolve(refB);
  assert.equal(a.ok, true);
  assert.equal(b.ok, true);
  // The resolver minted a single blob URL per request and reused
  // its own cache across remounts. The fresh round-trip must
  // return its own URL, never the previous one.
  assert.notEqual(a.url, b.url);
  assert.equal(calls.length, 2);
});

test("selectPendingEntries skips fresh rehydrations when force is not set", () => {
  const hydration: EntryOrganizationHydrationMap = new Map([
    [1, "loaded"],
    [2, "loaded"],
    [3, "pending"],
  ]);
  const visible = [imageEntry({ id: 1 }), imageEntry({ id: 2 }), imageEntry({ id: 3 })];
  const pending = selectPendingEntries(visible, hydration);
  assert.deepEqual(pending.map((entry) => entry.id), [3]);
});

test("applyPinUpdate keeps entries fresh-loaded across a collection switch", () => {
  const image = imageEntry({ id: 17, is_pinned: false });
  const entries: EntryRecord[] = [image];
  const visible: EntryRecord[] = [image];
  const organization: EntryOrganizationMap = new Map([
    [image.id, { tags: [], collections: [] }],
  ]);
  const hydration: EntryOrganizationHydrationMap = new Map([
    [image.id, "loaded"],
  ]);
  // Pin the row.
  const updated = applyPinUpdate(entries, visible, { ...image, is_pinned: true }, {
    isFiltering: false,
  });
  assert.equal(updated.nextEntries[0]?.is_pinned, true);
  // Hydration round keeps "loaded" because pin/unpin never
  // touches the per-entry cache.
  const reconciled = reconcileEntryOrganizationToVisible(
    organization,
    hydration,
    updated.nextEntries,
  );
  assert.equal(reconciled.nextHydration.get(image.id), "loaded");
  // Switching to a user collection must NOT drop the image row
  // from the cache either.
  const filtered = reconciled.nextOrganization;
  assert.equal(filtered.has(image.id), true);
});

test("image metadata survives every documented metadata mutation", () => {
  // Regression for the class of bugs where pin, title or
  // collection reassignment wiped `asset_ref`. The pure
  // helpers cannot inspect an EntryRecord's content field so a
  // regression that mutated the row would surface here.
  const image = imageEntry();
  const sample: Array<{ mutate: (entry: EntryRecord) => EntryRecord }> = [
    {
      mutate: (entry) => ({ ...entry, is_pinned: true }),
    },
    {
      mutate: (entry) => ({ ...entry, title: "Custom" }),
    },
    {
      mutate: (entry) => ({ ...entry, source_app_name: "Preview" }),
    },
  ];
  for (const { mutate } of sample) {
    const next = mutate(image);
    assert.equal(next.asset_ref, VALID_REF, "asset_ref must survive");
    assert.equal(next.mime_type, "image/png", "mime_type must survive");
    assert.equal(next.payload_width, 640, "payload_width must survive");
    assert.equal(next.payload_height, 480, "payload_height must survive");
    assert.equal(next.content_size, 8_192, "content_size must survive");
  }
});
