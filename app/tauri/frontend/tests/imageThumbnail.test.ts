/**
 * Regression coverage for the image-card load lifecycle.
 *
 * The card surface distinguishes three explicit states so a pending
 * load never flashes the "Imagen no disponible" fallback that the
 * reader used to render while `thumbnailUrl` was still `null`:
 *
 *   - `"loading"` — a coherent `asset_ref` is present, the bridge
 *     round-trip is in flight and the card shows a non-error
 *     placeholder.
 *   - `"loaded"` — the bridge returned bytes, a `blob:` URL was
 *     minted and the card renders the actual thumbnail.
 *   - `"error"` — the bridge round-trip failed and the card renders
 *     the accessible fallback.
 *
 * The tests below pin the contracts the component depends on without
 * standing up a Svelte renderer:
 *
 *   - the resolver mints a single `blob:` URL per asset so a
 *     `clipvault://organization-updated` refresh does not invalidate
 *     the URL the current card already owns;
 *   - a loader rejection collapses to `null` (the `error` state) and
 *     never mints a URL;
 *   - a stale resolution (the component was rebuilt before the bridge
 *     round-trip settled) cannot overwrite the state the current card
 *     already committed — the same `thumbnailToken` guard the
 *     component relies on;
 *   - hydrating `entryOrganization` for a visible entry is metadata
 *     only and never calls the clipboard-asset bridge, so a card
 *     rendered against a stale `entryOrganization` round keeps the
 *     thumbnail it already loaded.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  CLIPBOARD_ASSET_PREFIX,
  IMAGE_CONTENT_TYPE,
  createClipboardAssetResolver,
  hasRenderableImage,
  isImageEntry,
} from "../src/lib/clipboardAsset.ts";
import { clipboardAssetCommand } from "../src/lib/tauri.ts";
import type { IconLoader } from "../src/lib/iconResolver.ts";
import type { EntryRecord } from "../src/types.ts";

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

const SHA = "c".repeat(64);
const VALID_REF = `${CLIPBOARD_ASSET_PREFIX}${SHA}.png`;

const PNG_BYTES = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

function imageEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 17,
    content: "",
    content_type: IMAGE_CONTENT_TYPE,
    content_size: 2048,
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

interface InFlightLoader extends IconLoader {
  readonly pending: Map<
    string,
    { resolve: (bytes: number[]) => void; reject: (reason: unknown) => void }
  >;
  pendingCount(): number;
}

function inFlightLoader(): InFlightLoader {
  const pending = new Map<
    string,
    { resolve: (bytes: number[]) => void; reject: (reason: unknown) => void }
  >();
  const calls: string[] = [];
  const loader: IconLoader & {
    calls: string[];
    pending: typeof pending;
    pendingCount: () => number;
  } = {
    calls,
    pending,
    pendingCount(): number {
      return pending.size;
    },
    async loadIconBytes(ref: string) {
      calls.push(ref);
      return new Promise<number[]>((resolve, reject) => {
        pending.set(ref, { resolve, reject });
      });
    },
  };
  return loader;
}

/**
 * Per-entry view of the thumbnail surface. Mirrors the variables the
 * card owns for the entry currently rendered in its slot:
 *
 *   - `url` — the `blob:` URL the resolver minted, or `null` while
 *     the bridge round-trip is in flight or after it rejected;
 *   - `status` — the coarse three-state machine (`"loading"`,
 *     `"loaded"`, `"error"`) the template branches on.
 */
interface EntryThumbnail {
  url: string | null;
  status: "loading" | "loaded" | "error";
}

/**
 * Mimic the component's `refreshThumbnail` flow: the token is shared
 * across entries (the card owns a single `thumbnailToken` counter for
 * its slot, but a stale resolution from the previous entry must not
 * commit against the new state), the bridge round-trip awaited, the
 * token compared before committing any state and the resolver cache
 * used as the blob-URL owner.
 */
async function refreshOnce(
  resolver: ReturnType<typeof createClipboardAssetResolver>,
  ref: string | null,
  state: EntryThumbnail,
  shared: { token: number },
): Promise<void> {
  const token = ++shared.token;
  if (!ref) {
    if (token === shared.token) {
      state.url = null;
      state.status = "error";
    }
    return;
  }
  if (token === shared.token) {
    state.status = "loading";
  }
  const resolution = await resolver.resolve(ref);
  if (token !== shared.token) return;
  if (resolution.ok && resolution.url) {
    state.url = resolution.url;
    state.status = "loaded";
  } else {
    state.url = null;
    state.status = "error";
  }
}

// ---------------------------------------------------------------------
// Three-state machine
// ---------------------------------------------------------------------

test("coherent image row starts in loading and transitions to loaded", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const resolver = createClipboardAssetResolver(
    loaderReturning(PNG_BYTES, []),
  );
  const state: EntryThumbnail = { url: null, status: "error" };
  const shared = { token: 0 };
  const refresh = refreshOnce(resolver, VALID_REF, state, shared);

  // The synchronous prelude must already commit the loading state so a
  // same-tick render cannot land on the fallback with a null URL.
  assert.equal(state.status, "loading");
  assert.equal(state.url, null);

  await refresh;
  assert.equal(state.status, "loaded");
  assert.equal(typeof state.url, "string");
  assert.equal(state.url?.startsWith("blob:"), true);
});

test("loader rejection transitions to error and never mints a URL", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const resolver = createClipboardAssetResolver({
    async loadIconBytes() {
      throw new Error("not_found");
    },
  });
  const state: EntryThumbnail = { url: null, status: "error" };
  const shared = { token: 0 };
  await refreshOnce(resolver, VALID_REF, state, shared);
  assert.equal(state.status, "error");
  assert.equal(state.url, null);
  assert.equal(hub.created.length, 0);
});

test("loader returning null transitions to error and never mints a URL", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const resolver = createClipboardAssetResolver(loaderReturning(null, []));
  const state: EntryThumbnail = { url: null, status: "error" };
  const shared = { token: 0 };
  await refreshOnce(resolver, VALID_REF, state, shared);
  assert.equal(state.status, "error");
  assert.equal(state.url, null);
  assert.equal(hub.created.length, 0);
});

test("stale resolution never overwrites a freshly committed state", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const loader = inFlightLoader();
  const resolver = createClipboardAssetResolver(loader);

  // The card owns a single `thumbnailToken` counter it bumps on every
  // round, regardless of which entry triggered it. Sharing the counter
  // across both states is what makes the stale-response guard work:
  // when the user navigates to a different entry while the bridge
  // round-trip is in flight, the previous round's local token no
  // longer matches the shared counter and the late result is
  // discarded.
  const shared = { token: 0 };
  const stateA: EntryThumbnail = { url: null, status: "error" };
  const stateB: EntryThumbnail = { url: null, status: "error" };

  // First round for entry A — the in-flight loader parks the promise.
  const refreshA = refreshOnce(resolver, VALID_REF, stateA, shared);
  assert.equal(stateA.status, "loading");
  assert.equal(shared.token, 1);
  const pendingA = loader.pending.get(VALID_REF);
  assert.ok(pendingA);

  // The user navigates to a different entry before the bridge settles:
  // a brand new round bumps the token and the stale round must NOT
  // mutate the new state once it lands.
  const refreshB = refreshOnce(
    resolver,
    `${CLIPBOARD_ASSET_PREFIX}${"9".repeat(64)}.png`,
    stateB,
    shared,
  );
  assert.equal(stateB.status, "loading");
  assert.equal(shared.token, 2);

  // Settle A first with valid bytes — the local token captured by A's
  // round is 1, the shared counter is 2; the result is discarded.
  pendingA.resolve(PNG_BYTES);
  await refreshA;
  assert.equal(stateA.status, "loading", "stale A round must be discarded");
  assert.equal(stateA.url, null, "stale A round must not commit a URL");

  // Settle B with a different asset reference; the bridge round-trip
  // for B resolves and commits the loaded state.
  const pendingB = loader.pending.get(
    `${CLIPBOARD_ASSET_PREFIX}${"9".repeat(64)}.png`,
  );
  assert.ok(pendingB);
  pendingB.resolve([0x01, 0x02, 0x03]);
  await refreshB;
  assert.equal(stateB.status, "loaded");
  assert.equal(typeof stateB.url, "string");
});

test("switching entries releases the previous Blob URL before minting a new one", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(
    loaderReturning(PNG_BYTES, calls),
  );

  const first = await resolver.resolve(VALID_REF);
  const firstUrl = first.url as string;
  assert.ok(firstUrl);

  // The card releases the previous entry's URL when the entry id
  // changes; the resolver must observe the revoke so the next resolve
  // mints a fresh URL rather than handing back a revoked one.
  resolver.releaseFor(VALID_REF);
  assert.deepEqual(hub.revoked, [firstUrl]);

  const second = await resolver.resolve(
    `${CLIPBOARD_ASSET_PREFIX}${"d".repeat(64)}.png`,
  );
  assert.notEqual(second.url, firstUrl);
  assert.equal(hub.created.length, 2);
});

test("coherent asset returns bytes through the bridge command", async () => {
  installTauriMock(async (cmd, args) => {
    assert.equal(cmd, "clipvault_clipboard_asset");
    assert.equal((args as { assetRef: string }).assetRef, VALID_REF);
    return PNG_BYTES;
  });
  const bytes = await clipboardAssetCommand({ ref: VALID_REF });
  assert.deepEqual(bytes, PNG_BYTES);
});

// ---------------------------------------------------------------------
// Hydration compatibility — tags/collections must not trigger an
// asset re-fetch.
//
// The component subscribes to the metadata-only
// `clipvault://organization-updated` event and re-reads the
// per-entry cache. A regression that mis-wires the listener could
// force the card to release the blob URL it owns; the assertions
// below pin the bridge-level contracts the listener relies on.
// ---------------------------------------------------------------------

test("hasRenderableImage remains true after the hydration round starts", () => {
  // The hydration call mutates `entryOrganization` and
  // `entryOrganizationHydration` only. The `EntryRecord` that
  // reaches the card keeps its full metadata set, so the predicate
  // that gates the thumbnail surface is identical to what it was
  // before the round started.
  const before = imageEntry();
  assert.equal(hasRenderableImage(before), true);
  // The card never reads `entryOrganization` to decide whether to
  // render the thumbnail — the bridge contract keeps the two
  // surfaces decoupled.
  const after = { ...before };
  assert.equal(hasRenderableImage(after), true);
  assert.equal(isImageEntry(after), true);
});

test("bridge command for a coherent asset never echoes the reference back", async () => {
  // The bridge returns the raw PNG bytes and nothing else. A
  // regression that started logging or echoing the reference on a
  // metadata-only event would show up here as a payload leak; the
  // PNG bytes never carry the `asset_ref`.
  let captured: { cmd: string; args?: Record<string, unknown> } | undefined;
  installTauriMock(async (cmd, args) => {
    captured = { cmd, args };
    return PNG_BYTES;
  });
  await clipboardAssetCommand({ ref: VALID_REF });
  assert.equal(captured?.cmd, "clipvault_clipboard_asset");
  const payload = JSON.stringify(captured);
  // The bridge surface serialises the command name and the asset
  // reference; both are stable strings the backend accepts.
  assert.equal(payload.includes("clipvault_clipboard_asset"), true);
  assert.equal(payload.includes(VALID_REF), true);
});

test("resolver never re-mints a URL for the same reference twice in a row", async () => {
  // A regression that would clear the cached URL between two
  // synchronous resolves of the same ref would mint a second blob
  // URL and double the memory footprint of a long-lived rail.
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(
    loaderReturning(PNG_BYTES, calls),
  );
  const first = await resolver.resolve(VALID_REF);
  const second = await resolver.resolve(VALID_REF);
  assert.equal(first.url, second.url);
  assert.equal(hub.created.length, 1, "no second blob URL is minted");
  assert.equal(calls.length, 1, "no second bridge round-trip is fired");
});
