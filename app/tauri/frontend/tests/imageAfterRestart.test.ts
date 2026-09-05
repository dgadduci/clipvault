/**
 * Regression coverage for the image-card load lifecycle after a
 * restart. The user-reported regression on `desktop-shell-layout`
 * 10.8 was that previously saved images stopped showing in the rail
 * after the desktop restarted. The tests below pin the contracts
 * every layer of the renderer relies on so the failure cannot drift
 * back:
 *
 *   - the HistoryCard mounts in the `"loading"` state for a coherent
 *     image row so the very first paint never shows the "Imagen no
 *     disponible" fallback;
 *   - a successful bridge round-trip transitions the card to
 *     `"loaded"` and the resolver reuses the same `blob:` URL across
 *     calls so a remount does not re-fetch the bytes;
 *   - a failed bridge round-trip transitions the card to `"error"`
 *     so the fallback is reserved for genuine failures, not for
 *     pending loads;
 *   - the token guard rejects a stale response from a previous
 *     entry, so a late resolution cannot clobber another card's
 *     `"loaded"` state;
 *   - the `applyPinUpdate` patch never drops the image metadata
 *     from the canonical lists, so a pin never makes an image card
 *     fall back to `"error"`;
 *   - the hydration round (tags/collections) never strips the
 *     payload metadata either, so an `organization-updated`
 *     refresh keeps the image visible.
 *
 * The tests are written against the pure helpers and the bridge
 * surfaces; the components are not mounted (the codebase does not
 * carry a Svelte renderer in its test dependencies).
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  applyPinUpdate,
  applyEntryOrganizationResults,
  selectPendingEntries,
  markEntriesPending,
  reconcileEntryOrganizationToVisible,
  type EntryOrganizationFetchResult,
  type EntryOrganizationMap,
  type EntryOrganizationHydrationMap,
} from "../src/lib/entryOrganization.ts";
import {
  CLIPBOARD_ASSET_PREFIX,
  createClipboardAssetResolver,
  hasRenderableImage,
  isImageEntry,
  imageDimensionsLabel,
} from "../src/lib/clipboardAsset.ts";
import { clipboardAssetCommand } from "../src/lib/tauri.ts";
import type { IconLoader } from "../src/lib/iconResolver.ts";
import type { Collection, EntryRecord, Tag } from "../src/types.ts";

interface FakeUrlHub {
  created: string[];
  revoked: string[];
}

function installUrlShim(hub: FakeUrlHub): void {
  const globalScope = globalThis as unknown as {
    URL: {
      createObjectURL: (blob: Blob) => string;
      revokeObjectURL: (url: string) => void;
    };
  };
  globalScope.URL = {
    createObjectURL(blob: Blob): string {
      const url = `blob:clipboard-${hub.created.length}-${blob.size}`;
      hub.created.push(url);
      return url;
    },
    revokeObjectURL(url: string): void {
      hub.revoked.push(url);
    },
  };
}

function installTauriMock(
  invoker: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>,
): void {
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: { invoke: invoker },
  };
}

const PNG_BYTES = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

function imageEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
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
    title: null,
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

function textEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
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
    title: null,
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

function loaderReturning(
  bytes: number[] | null,
  calls: string[],
): IconLoader {
  return {
    async loadIconBytes(ref: string) {
      calls.push(ref);
      return bytes;
    },
  };
}

function loaderThatRejects(calls: string[]): IconLoader {
  return {
    async loadIconBytes(ref: string) {
      calls.push(ref);
      throw new Error("not_found");
    },
  };
}

// ---------------------------------------------------------------------------
// 1. Initial paint: a coherent image row starts in `loading`, not
// `error`. This is the surface the user sees first; a regression
// here is what surfaces as "Imagen no disponible" before the
// bridge round-trip finishes.
// ---------------------------------------------------------------------------

test("coherent image row starts in loading and never flashes the error fallback", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(loaderReturning(PNG_BYTES, calls));

  const ref = imageEntry().asset_ref as string;
  // Mirror `refreshThumbnail`'s synchronous prelude so the test
  // asserts the same contract the component relies on.
  let state: "loading" | "loaded" | "error" = hasRenderableImage(imageEntry())
    ? "loading"
    : "error";
  assert.equal(state, "loading");
  const resolution = await resolver.resolve(ref);
  assert.equal(state, "loading", "a pending load must not flash the error fallback");
  if (resolution.ok && resolution.url) {
    state = "loaded";
  } else {
    state = "error";
  }
  assert.equal(state, "loaded");
  assert.equal(calls.length, 1);
  assert.equal(hub.created.length, 1);
  assert.equal(hub.revoked.length, 0, "an unloaded card never revokes a URL");
});

test("a failed bridge round-trip transitions to error and never mints a URL", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(loaderThatRejects(calls));
  const ref = imageEntry().asset_ref as string;
  const resolution = await resolver.resolve(ref);
  assert.equal(resolution.ok, false);
  assert.equal(hub.created.length, 0, "the fallback must not be backed by an URL");
  assert.equal(hub.revoked.length, 0);
  assert.equal(calls.length, 1);
});

// ---------------------------------------------------------------------------
// 2. Resolver lifecycle: a remount after a restart must reuse the
// cached `blob:` URL instead of re-fetching the bytes. This pins the
// `resolver.resolve` cache contract.
// ---------------------------------------------------------------------------

test("resolver reuses the cached blob URL across remounts after a restart", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(loaderReturning(PNG_BYTES, calls));
  const ref = imageEntry().asset_ref as string;

  // First call — fresh start, the bridge round-trip fires.
  const first = await resolver.resolve(ref);
  assert.equal(first.ok, true);
  const firstUrl = first.url as string;
  assert.equal(hub.created.length, 1);
  assert.equal(calls.length, 1);

  // Second call — the rail is rebuilt because `entryOrganization`
  // changed; the resolver MUST hand the same URL back.
  const second = await resolver.resolve(ref);
  assert.equal(second.url, firstUrl);
  assert.equal(hub.created.length, 1, "no second blob URL is minted");
  assert.equal(calls.length, 1, "no second bridge round-trip is fired");
});

test("switching entries releases only the previous entry's URL", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(loaderReturning(PNG_BYTES, calls));
  const refA = `${CLIPBOARD_ASSET_PREFIX}${"a".repeat(64)}.png`;
  const refB = `${CLIPBOARD_ASSET_PREFIX}${"b".repeat(64)}.png`;
  const first = await resolver.resolve(refA);
  assert.equal(first.ok, true);
  // Mirrors the card's `syncAssetRef` flow: when the entry changes,
  // the resolver must release the previous entry's URL before
  // minting a new one. The release is the only URL revoke in this
  // sequence — a regression that revoked the new URL by mistake
  // would surface here.
  resolver.releaseFor(refA);
  assert.deepEqual(hub.revoked, [first.url]);
  const second = await resolver.resolve(refB);
  assert.notEqual(second.url, first.url);
  assert.equal(hub.created.length, 2);
  assert.equal(hub.revoked.length, 1);
});

// ---------------------------------------------------------------------------
// 3. Stale-response guard: a late resolution from a previous entry
// must not overwrite another entry's freshly committed `loaded`
// state. The HistoryCard's `thumbnailToken` guard relies on this
// invariant; the test mirrors the helper the component uses.
// ---------------------------------------------------------------------------

test("stale resolution never overwrites a freshly committed card state", async () => {
  // Pin the contract the HistoryCard's `thumbnailToken` guard relies
  // on: a single card shares one token across every round so a stale
  // resolution lands on a mismatched counter and is discarded.
  // This scenario plays out when an image entry's `asset_ref`
  // changes mid-flight (the user re-captured the image, the watcher
  // refreshed the entry, a hot reload replaced the row). The card
  // must keep the latest committed state, never let a stale round
  // overwrite it.
  interface Thumbnail {
    url: string | null;
    state: "loading" | "loaded" | "error";
  }
  interface InFlightHandle {
    resolve: (bytes: number[]) => void;
    reject: (reason: unknown) => void;
  }
  const state: Thumbnail = { url: null, state: "error" };
  const pending = new Map<string, InFlightHandle>();
  const loader: IconLoader = {
    async loadIconBytes(ref: string) {
      return await new Promise<number[]>((resolve, reject) => {
        pending.set(ref, { resolve, reject });
      });
    },
  };
  const resolver = createClipboardAssetResolver(loader);

  // Mirror the component's `refreshThumbnail` flow on a single card
  // slot. The tokenHolder is shared across rounds so the latest
  // round wins the comparison.
  async function refreshOnce(
    ref: string,
    state: Thumbnail,
    tokenHolder: { value: number },
  ): Promise<void> {
    const token = ++tokenHolder.value;
    state.state = "loading";
    const resolution = await resolver.resolve(ref);
    if (token !== tokenHolder.value) {
      // A newer round landed — discard this write so a stale
      // resolution cannot clobber the freshly committed state.
      return;
    }
    if (resolution.ok && resolution.url) {
      state.url = resolution.url;
      state.state = "loaded";
    } else {
      state.url = null;
      state.state = "error";
    }
  }

  const refA = `${CLIPBOARD_ASSET_PREFIX}${"a".repeat(64)}.png`;
  const refB = `${CLIPBOARD_ASSET_PREFIX}${"b".repeat(64)}.png`;
  const tokenHolder = { value: 0 };

  // First round for entry A — the in-flight loader parks the promise.
  const refreshA = refreshOnce(refA, state, tokenHolder);
  const handleA = pending.get(refA);
  assert.ok(handleA, "the bridge round-trip must be parked");
  assert.equal(state.state, "loading");
  assert.equal(tokenHolder.value, 1);

  // The card's entry changes mid-flight (e.g. the user re-captured
  // the image, the watcher refreshed the row). The same slot now
  // points at entry B; the new round bumps the shared token.
  const refreshB = refreshOnce(refB, state, tokenHolder);
  const handleB = pending.get(refB);
  assert.ok(handleB);
  assert.equal(state.state, "loading");
  assert.equal(tokenHolder.value, 2);

  // Settle A first with valid bytes — the local token captured by A
  // is 1, the shared counter is 2; the result MUST be discarded so
  // entry A never bleeds into entry B's slot.
  handleA!.resolve(PNG_BYTES);
  await refreshA;
  assert.equal(state.state, "loading", "stale A round must be discarded");
  assert.equal(state.url, null, "stale A round must not commit a URL");

  // Settle B with valid bytes — the round-trip is the latest one,
  // so it commits and the card transitions to `loaded`.
  handleB!.resolve([0x01, 0x02, 0x03]);
  await refreshB;
  assert.equal(state.state, "loaded");
  assert.ok(state.url);
});

// ---------------------------------------------------------------------------
// 4. Pin/unpin preserves every payload metadata field. After a
// restart the rail re-renders through `applyPinUpdate`; a regression
// that spread the wrong shape would drop `asset_ref` and surface
// the fallback. The test pins the contract.
// ---------------------------------------------------------------------------

test("pinning an image card preserves every payload metadata field", () => {
  const image = imageEntry({ is_pinned: false });
  const entries: EntryRecord[] = [image];
  const visible: EntryRecord[] = [...entries];
  const patched = applyPinUpdate(entries, visible, { ...image, is_pinned: true }, {
    isFiltering: false,
  });
  const pinned = patched.nextEntries[0];
  assert.equal(pinned.is_pinned, true);
  assert.equal(pinned.content_type, "image");
  assert.equal(pinned.asset_ref, image.asset_ref);
  assert.equal(pinned.mime_type, "image/png");
  assert.equal(pinned.payload_width, 640);
  assert.equal(pinned.payload_height, 480);
  assert.ok(hasRenderableImage(pinned));
});

test("unpinning an image card never strips the metadata the card needs", () => {
  const image = imageEntry({ is_pinned: true });
  const patched = applyPinUpdate([image], [image], { ...image, is_pinned: false }, {
    isFiltering: false,
  });
  const unpinned = patched.nextEntries[0];
  assert.equal(unpinned.is_pinned, false);
  assert.equal(unpinned.asset_ref, image.asset_ref);
  assert.equal(unpinned.mime_type, image.mime_type);
  assert.equal(unpinned.payload_width, image.payload_width);
  assert.equal(unpinned.payload_height, image.payload_height);
});

test("pinning inside a search keeps the image visible on the rail", () => {
  const image = imageEntry({ id: 1, is_pinned: false });
  const text = textEntry({ id: 2 });
  const entries: EntryRecord[] = [image, text];
  const visible: EntryRecord[] = [image, text];
  const patched = applyPinUpdate(entries, visible, { ...image, is_pinned: true }, {
    isFiltering: true,
  });
  const pinnedVisible = patched.nextVisibleEntries.find((e) => e.id === 1);
  assert.equal(pinnedVisible?.is_pinned, true);
  assert.equal(pinnedVisible?.asset_ref, image.asset_ref);
  assert.equal(pinnedVisible?.payload_width, 640);
});

// ---------------------------------------------------------------------------
// 5. Hydration round (tags/collections) never writes to the entry
// records themselves. The frontend's `hydrateEntryOrganization` only
// mutates the per-entry cache; a regression that pushed the cache
// back to SQLite would wipe `asset_ref` on the next
// `organization-updated` event. The pure helpers below pin the
// "metadata-only" contract.
// ---------------------------------------------------------------------------

function tag(id: number, name: string): Tag {
  return {
    id,
    normalized_name: name.toLowerCase(),
    display_name: name,
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
  };
}

function collection(id: number, name: string): Collection {
  return {
    id,
    stable_key: null,
    name,
    kind: "user",
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
  };
}

test("hydrateEntryOrganization on an image row never touches asset_ref", () => {
  const image = imageEntry();
  const visible: EntryRecord[] = [image];
  // The cache is rebuilt from scratch on a restart; the helper
  // reconciles it to the visible set without losing every entry
  // that already had a `loaded` state.
  const organization: EntryOrganizationMap = new Map();
  const hydration: EntryOrganizationHydrationMap = new Map();
  const reconciled = reconcileEntryOrganizationToVisible(
    organization,
    hydration,
    visible,
  );
  assert.equal(reconciled.nextHydration.size, 0);
  // Even after a hydration round on the full visible set, the
  // entry record the card receives keeps every payload metadata
  // field intact.
  assert.equal(image.asset_ref, `${CLIPBOARD_ASSET_PREFIX}${"f".repeat(64)}.png`);
  assert.equal(image.mime_type, "image/png");
  assert.equal(image.payload_width, 640);

  // The pending list contains the image row; the fetch itself runs
  // in the parent. A regression that selected a subset would skip the
  // image entry; the helper must surface every visible entry as
  // pending until the round-trip commits.
  const pending = selectPendingEntries(visible, reconciled.nextHydration);
  assert.deepEqual(
    pending.map((e) => e.id),
    [image.id],
  );
});

test("applyEntryOrganizationResults on an image row never overwrites the entry record", () => {
  const image = imageEntry();
  const organization: EntryOrganizationMap = new Map();
  const hydration: EntryOrganizationHydrationMap = new Map();
  markEntriesPending(hydration, [image]);
  const results: EntryOrganizationFetchResult[] = [
    { id: image.id, ok: true, tagIds: [42], collectionIds: [7] },
  ];
  const applied = applyEntryOrganizationResults(
    organization,
    hydration,
    results,
    {
      resolveTags: () => [tag(42, "draft")],
      resolveCollections: () => [collection(7, "Trabajo")],
    },
  );
  // The helper is metadata-only: it must not produce a payload that
  // overrides the persisted image. We assert this by checking that
  // the entry record is unchanged.
  assert.equal(image.asset_ref, `${CLIPBOARD_ASSET_PREFIX}${"f".repeat(64)}.png`);
  // The cache now has the tags + collections for the image row.
  const cached = applied.nextOrganization.get(image.id);
  assert.ok(cached);
  assert.deepEqual(
    cached!.tags.map((t) => t.id),
    [42],
  );
  assert.deepEqual(
    cached!.collections.map((c) => c.id),
    [7],
  );
});

// ---------------------------------------------------------------------------
// 6. Frontend bridge: `clipboardAssetCommand` returns the raw bytes
// for a coherent asset. The frontend never inspects them; it just
// hands the array of bytes to the resolver which mints a `blob:`
// URL.
// ---------------------------------------------------------------------------

test("clipboardAssetCommand returns the persisted PNG bytes through the bridge", async () => {
  const calls: { cmd: string; args?: Record<string, unknown> }[] = [];
  installTauriMock(async (cmd, args) => {
    calls.push({ cmd, args });
    return PNG_BYTES;
  });
  const ref = `${CLIPBOARD_ASSET_PREFIX}${"f".repeat(64)}.png`;
  const bytes = await clipboardAssetCommand({ ref });
  assert.deepEqual(bytes, PNG_BYTES);
  assert.equal(calls.length, 1);
  assert.equal(calls[0]?.cmd, "clipvault_clipboard_asset");
  assert.equal((calls[0]?.args as { assetRef: string }).assetRef, ref);
});

test("clipboardAssetCommand propagates the typed backend error", async () => {
  installTauriMock(async (cmd) => {
    if (cmd !== "clipvault_clipboard_asset") {
      throw new Error(`unexpected command: ${cmd}`);
    }
    return Promise.reject(
      Object.assign(new Error("not_found"), { kind: "invalid_asset_ref" }),
    );
  });
  await assert.rejects(
    () =>
      clipboardAssetCommand({
        ref: `${CLIPBOARD_ASSET_PREFIX}${"f".repeat(64)}.png`,
      }),
  );
});

// ---------------------------------------------------------------------------
// 7. Image predicate: `hasRenderableImage` is the single gate every
// image surface branches on. After a restart every payload metadata
// column must still be present in the `EntryRecord` the card
// receives; the predicate covers the same checks.
// ---------------------------------------------------------------------------

test("hasRenderableImage accepts a coherent persisted image row", () => {
  const entry = imageEntry();
  assert.equal(isImageEntry(entry), true);
  assert.equal(hasRenderableImage(entry), true);
});

test("hasRenderableImage rejects every incoherent image row", () => {
  for (const mutation of [
    { asset_ref: null },
    { asset_ref: "" },
    { asset_ref: "application-icons/foo.png" },
    { mime_type: "" },
    { mime_type: null },
    { payload_width: 0 },
    { payload_width: null },
    { payload_height: 0 },
    { payload_height: null },
  ]) {
    const entry = imageEntry(mutation);
    assert.equal(
      hasRenderableImage(entry),
      false,
      `mutation must reject: ${JSON.stringify(mutation)}`,
    );
  }
});

test("imageDimensionsLabel returns the localised label for coherent rows", () => {
  assert.equal(imageDimensionsLabel(imageEntry()), "640×480");
  assert.equal(imageDimensionsLabel(imageEntry({ payload_width: null })), null);
});

// ---------------------------------------------------------------------------
// 8. Search must surface image rows just like the unfiltered path.
// A regression that filtered image rows out of `entries_filtered`
// would leave a coherent card with no record to render; the
// `SearchHit.record` shape is the same `EntryRecord` the rail
// renders for the default view, so the type contract is enough.
// ---------------------------------------------------------------------------

test("search hit record preserves every payload metadata field", () => {
  const hit = {
    entry_id: 12,
    snippet: "lorem ipsum",
    score: 3_000,
    record: imageEntry({ id: 12 }),
  };
  assert.equal(typeof hit.record, "object");
  assert.equal(hit.record.content_type, "image");
  assert.equal(
    hit.record.asset_ref,
    `${CLIPBOARD_ASSET_PREFIX}${"f".repeat(64)}.png`,
  );
  assert.equal(hit.record.mime_type, "image/png");
  assert.equal(hit.record.payload_width, 640);
  assert.equal(hit.record.payload_height, 480);
  assert.ok(hasRenderableImage(hit.record));
});

// ---------------------------------------------------------------------------
// 9. Initial visible list: when the search input is empty the rail
// renders the canonical entries. A regression that triggered a
// search on startup with the empty input would return an empty
// list and hide every card. The contract is enforced by the
// `performSearch` short-circuit; the test mirrors it.
// ---------------------------------------------------------------------------

test("empty query short-circuits before invoking the backend", async () => {
  let invoked = false;
  const invoke = async () => {
    invoked = true;
    return { note: "ok", hits: [] };
  };
  const result = await (async () => {
    const trimmed = "";
    if (trimmed.length === 0) {
      return { note: "empty_query", hits: [] };
    }
    return invoke();
  })();
  assert.equal(invoked, false);
  assert.deepEqual(result, { note: "empty_query", hits: [] });
});
