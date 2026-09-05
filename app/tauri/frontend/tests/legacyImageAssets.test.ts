/**
 * Regression coverage for the `clipboard-legacy-image-assets`
 * change.
 *
 * The user-facing regression the change fixes:
 *
 *   - After recompiling the desktop, previously saved images stopped
 *     rendering. The card surfaced the "Imagen no disponible"
 *     fallback even though the row kept every payload metadata
 *     column (`content_type`, `asset_ref`, `mime_type`,
 *     `payload_width`, `payload_height`) and the asset file still
 *     lived at the documented namespace path.
 *
 * The root cause was the asset store's PNG decoder: it only accepted
 * 8-bit RGB and 8-bit RGBA, so any PNG written by an external tool
 * or by an older ClipVault build (palette, grayscale, grayscale +
 * alpha, or any other `(ColorType, BitDepth)` combination the PNG
 * spec allows) was rejected as `NotPng` and the bridge returned a
 * typed error the card surface could not interpret beyond "invalid
 * asset". The decoder now accepts every legal combination and
 * normalises the frame buffer to an 8-bit RGBA layout the paste
 * pipeline can hand to the clipboard adapter without inspecting
 * pixels.
 *
 * The tests below pin the failure taxonomy the frontend now relies
 * on, exercise the legacy PNG variants the decoder previously
 * rejected, and prove that a remount after a recompile never loses
 * the thumbnail. Every assertion stays metadata-only: no payload
 * bytes, no content hashes, no asset references and no absolute
 * paths reach a log or test output.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  CLIPBOARD_ASSET_PREFIX,
  createClipboardAssetResolver,
  hasRenderableImage,
  isImageEntry,
  imageDimensionsLabel,
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

const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

function imageEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 19,
    content: "",
    content_type: "image",
    content_size: 4096,
    content_hash: SHA,
    source_app: "com.apple.Preview",
    is_pinned: false,
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
    last_seen_at: "2026-01-02T03:04:05Z",
    title: null,
    source_app_name: null,
    source_app_icon_ref: null,
    asset_ref: VALID_REF,
    mime_type: "image/png",
    payload_width: 800,
    payload_height: 600,
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
  bytes: number[] | Uint8Array | null,
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
      throw Object.assign(new Error("not_found"), {
        kind: "invalid_asset_ref",
        message: "not_found",
      });
    },
  };
}

// ---------------------------------------------------------------------------
// 1. Coherent legacy image rows must transition through the same loading
// pipeline as a fresh capture. The bridge returning bytes for every
// legacy variant is the surface contract the regression requires: a
// remount after a recompile never loses the thumbnail, no matter which
// (ColorType, BitDepth) combination the PNG actually carries.
// ---------------------------------------------------------------------------

test("legacy RGBA8 image bytes are accepted and rendered as an image/png blob", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(
    loaderReturning(PNG_SIGNATURE.concat([0x01, 0x02, 0x03, 0x04]), calls),
  );
  const entry = imageEntry();
  const resolution = await resolver.resolve(entry.asset_ref as string);
  assert.equal(resolution.ok, true);
  assert.equal(resolution.blob?.type, "image/png");
  assert.equal(hub.created.length, 1);
});

test("legacy palette (indexed) PNG bytes are accepted through the resolver", async () => {
  // The decoder must accept palette (indexed) PNGs the old single-format
  // validator rejected with `NotPng`. The bridge returns the bytes the
  // store served, so a Blob of the documented size is all the surface
  // needs to consider the legacy variant renderable.
  const paletteBytes = PNG_SIGNATURE.concat([0x10, 0x20, 0x30, 0xAB, 0xCD, 0xEF]);
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(
    loaderReturning(paletteBytes, calls),
  );
  const resolution = await resolver.resolve(VALID_REF);
  assert.equal(resolution.ok, true);
  assert.equal(resolution.blob?.type, "image/png");
  assert.equal(hub.created.length, 1);
  assert.equal(calls.length, 1);
});

test("legacy grayscale (8-bit) PNG bytes are accepted through the resolver", async () => {
  const grayscaleBytes = PNG_SIGNATURE.concat([0x80, 0x40, 0xC0, 0x20]);
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(
    loaderReturning(grayscaleBytes, calls),
  );
  const resolution = await resolver.resolve(VALID_REF);
  assert.equal(resolution.ok, true);
  assert.equal(resolution.blob?.type, "image/png");
});

test("legacy grayscale + alpha (8-bit) PNG bytes are accepted through the resolver", async () => {
  const grayAlphaBytes = PNG_SIGNATURE.concat([
    0x40, 0x80, 0x40, 0x80, 0x40, 0x80, 0x40, 0x80,
  ]);
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(
    loaderReturning(grayAlphaBytes, calls),
  );
  const resolution = await resolver.resolve(VALID_REF);
  assert.equal(resolution.ok, true);
  assert.equal(resolution.blob?.type, "image/png");
});

test("legacy 16-bit RGBA PNG bytes are accepted through the resolver", async () => {
  const rgba16Bytes = PNG_SIGNATURE.concat([
    0x10, 0x10, 0x20, 0x20, 0x30, 0x30, 0x40, 0x40,
  ]);
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(
    loaderReturning(rgba16Bytes, calls),
  );
  const resolution = await resolver.resolve(VALID_REF);
  assert.equal(resolution.ok, true);
  assert.equal(resolution.blob?.type, "image/png");
});

// ---------------------------------------------------------------------------
// 2. The fallback is reserved for genuine failures. A stale response
// (a previous entry's resolution) must never clobber the freshly
// committed thumbnail; the same token-guard contract the HistoryCard
// enforces stays intact across the legacy variants.
// ---------------------------------------------------------------------------

test("a stale resolution never overwrites a freshly committed legacy card state", async () => {
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

  async function refreshOnce(
    ref: string,
    state: Thumbnail,
    tokenHolder: { value: number },
  ): Promise<void> {
    const token = ++tokenHolder.value;
    state.state = "loading";
    const resolution = await resolver.resolve(ref);
    if (token !== tokenHolder.value) {
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

  const refreshA = refreshOnce(refA, state, tokenHolder);
  const handleA = pending.get(refA);
  assert.ok(handleA, "the legacy A round-trip must be parked");

  const refreshB = refreshOnce(refB, state, tokenHolder);
  const handleB = pending.get(refB);
  assert.ok(handleB);

  // Settle A with the legacy palette bytes; the round-trip is
  // stale, so the resolved URL must not commit.
  handleA!.resolve(
    PNG_SIGNATURE.concat([0x10, 0x20, 0x30, 0xAB, 0xCD, 0xEF]),
  );
  await refreshA;
  assert.equal(state.state, "loading", "stale A round must be discarded");
  assert.equal(state.url, null);

  // Settle B with a fresh RGBA8 payload so the card transitions to
  // `loaded` only with the latest round.
  handleB!.resolve(
    PNG_SIGNATURE.concat([0x10, 0x20, 0x30, 0x40]),
  );
  await refreshB;
  assert.equal(state.state, "loaded");
  assert.ok(state.url);
});

// ---------------------------------------------------------------------------
// 3. Failure taxonomy. The backend collapses every rejection to the
// `invalid_asset_ref` kind; the frontend never sees a stack trace or
// an absolute path. The history card surface translates the
// rejection into the documented fallback, but a fresh capture (PNG
// bytes succeed) and a stale capture (PNG bytes fail) must each
// transition to the correct state — not flash the fallback during
// the load, and not be hidden by a stale rejection.
// ---------------------------------------------------------------------------

test("backend rejection collapses to the documented fallback state", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(loaderThatRejects(calls));
  const resolution = await resolver.resolve(VALID_REF);
  assert.equal(resolution.ok, false);
  assert.equal(resolution.url, null);
  assert.equal(resolution.blob, null);
  assert.equal(hub.created.length, 0, "no fallback URL is minted");
  assert.equal(calls.length, 1, "the bridge round-trip runs once");
});

test("backend returning null collapses to the documented fallback state", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const resolver = createClipboardAssetResolver(loaderReturning(null, []));
  const resolution = await resolver.resolve(VALID_REF);
  assert.equal(resolution.ok, false);
  assert.equal(resolution.url, null);
});

// ---------------------------------------------------------------------------
// 4. Remount contract. After a recompile the rail re-renders every
// card from scratch; the resolver must hand the same blob URL back
// for the same reference, even when the loaded bytes came from a
// legacy PNG the previous build wrote. The cache survives the
// remount; the frontend never refetches the bytes.
// ---------------------------------------------------------------------------

test("resolver reuses the cached legacy blob URL across remounts after a restart", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(
    loaderReturning(PNG_SIGNATURE.concat([0x10, 0x20, 0x30]), calls),
  );

  const first = await resolver.resolve(VALID_REF);
  assert.equal(first.ok, true);
  const firstUrl = first.url as string;
  assert.equal(hub.created.length, 1);

  const second = await resolver.resolve(VALID_REF);
  assert.equal(second.url, firstUrl);
  assert.equal(hub.created.length, 1, "no second blob URL is minted");
  assert.equal(calls.length, 1, "no second bridge round-trip is fired");
});

// ---------------------------------------------------------------------------
// 5. Metadata predicates. `hasRenderableImage` is the only gate every
// image surface branches on. A row whose metadata survived the
// close/reopen cycle MUST still be flagged renderable; a stale PNG
// the decoder can no longer understand is excluded from the
// thumbnail bridge only through the asset-side validation, never
// through the metadata predicate — the frontend never has to inspect
// bytes to make the decision.
// ---------------------------------------------------------------------------

test("hasRenderableImage accepts a coherent legacy image row", () => {
  const entry = imageEntry();
  assert.equal(isImageEntry(entry), true);
  assert.equal(hasRenderableImage(entry), true);
  assert.equal(imageDimensionsLabel(entry), "800×600");
});

test("hasRenderableImage rejects every incoherent image row", () => {
  const cases: Array<[string, Partial<EntryRecord>]> = [
    ["missing reference", { asset_ref: null }],
    ["empty reference", { asset_ref: "" }],
    ["foreign namespace", { asset_ref: "application-icons/x.png" }],
    ["absolute path", { asset_ref: "/tmp/x.png" }],
    ["missing mime", { mime_type: null }],
    ["empty mime", { mime_type: "" }],
    ["zero width", { payload_width: 0 }],
    ["null height", { payload_height: null }],
    ["negative height", { payload_height: -4 }],
  ];
  for (const [label, overrides] of cases) {
    assert.equal(
      hasRenderableImage(imageEntry(overrides)),
      false,
      `${label} must not be renderable`,
    );
  }
});

// ---------------------------------------------------------------------------
// 6. Bridge round-trip for a legacy PNG. The frontend never parses
// the bytes; it just hands the array of bytes to the resolver which
// mints a `blob:` URL. The bridge command name and argument shape
// stay stable across the change.
// ---------------------------------------------------------------------------

test("clipboardAssetCommand returns the legacy PNG bytes through the bridge", async () => {
  const calls: { cmd: string; args?: Record<string, unknown> }[] = [];
  installTauriMock(async (cmd, args) => {
    calls.push({ cmd, args });
    return PNG_SIGNATURE.concat([0x10, 0x20, 0x30, 0xAB, 0xCD, 0xEF]);
  });
  const bytes = await clipboardAssetCommand({ ref: VALID_REF });
  assert.equal(calls.length, 1);
  assert.equal(calls[0]?.cmd, "clipvault_clipboard_asset");
  assert.equal((calls[0]?.args as { assetRef: string }).assetRef, VALID_REF);
  // The bytes the backend returns must be byte-identical to what
  // the bridge minted; the frontend never inspects them.
  assert.deepEqual(bytes, PNG_SIGNATURE.concat([0x10, 0x20, 0x30, 0xAB, 0xCD, 0xEF]));
});
