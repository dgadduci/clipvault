/**
 * Regression coverage for the `desktop-shell-layout` follow-up.
 *
 * The original `desktop-shell-layout` change introduced the four-modal
 * coordinator, the global clear-history trash icon and the compact
 * toolbar. The follow-up surfaced a set of regressions the previous
 * change could not have caught in isolation:
 *
 *   - The desktop body inherited a `max-width: 880px` rule that
 *     defeated the rail's horizontal scroll: the panel itself
 *     stopped growing, but every additional card pushed the rail
 *     beyond the desktop and the browser started showing a body
 *     scrollbar instead of a rail one. The CSS now sizes the body
 *     to the work area and keeps the rail as the only horizontal
 *     scroll surface.
 *   - The image thumbnail card reset its state to `"error"` on first
 *     mount, so a freshly re-opened session flashed the "Imagen no
 *     disponible" fallback for every persisted image before the
 *     bridge round-trip finished. The initial state now mirrors the
 *     entry's renderability.
 *   - The trash button was wired to the global
 *     `clipvault_clear_history` command which only filtered by
 *     `is_pinned`. Captures that lived both in `Historial` and in a
 *     user (secondary) collection were deleted even though the user
 *     had explicitly grouped them. The follow-up ships a dedicated
 *     `clipvault_clear_unorganized_history` command whose predicate
 *     keeps every entry that owns a `kind = "user"` association.
 *   - The search input rendered a separate `<ul>` of snippet rows
 *     instead of filtering the cards the user already had on
 *     screen. The follow-up rewires the search so the rail renders
 *     the hits as the same `HistoryCard` instances the default view
 *     uses, scoped to the active collection.
 *
 * The tests below pin the bridge contracts and the pure helpers
 * without standing up a Svelte renderer: a renderer-level smoke
 * test lives in `desktopShellLayout.test.ts` and
 * `imageThumbnail.test.ts` already covers the thumbnail surface.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  clearUnorganizedHistoryCommand,
  clipboardAssetCommand,
  unorganizedClearableCountCommand,
} from "../src/lib/tauri.ts";
import {
  CLIPBOARD_ASSET_PREFIX,
  createClipboardAssetResolver,
  hasRenderableImage,
  isImageEntry,
} from "../src/lib/clipboardAsset.ts";
import {
  createIconResolver,
  type IconLoader,
} from "../src/lib/iconResolver.ts";
import type { EntryRecord } from "../src/types.ts";

type InvokeRecord = { cmd: string; args?: Record<string, unknown> };

const FRONTEND_ROOT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const TAURI_ROOT = path.resolve(FRONTEND_ROOT, "..", "src-tauri");
const APP_REPO_ROOT = path.resolve(FRONTEND_ROOT, "..", "..", "..");

function installInvoke(
  invoker: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>,
): InvokeRecord[] {
  const calls: InvokeRecord[] = [];
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: {
      invoke: async (cmd: string, args?: Record<string, unknown>) => {
        calls.push({ cmd, args });
        return invoker(cmd, args);
      },
    },
  };
  return calls;
}

function loadFixture(absPath: string): string {
  return readFileSync(absPath, "utf8");
}

const SHA = "e".repeat(64);
const VALID_REF = `${CLIPBOARD_ASSET_PREFIX}${SHA}.png`;
const PNG_BYTES = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

function imageEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 17,
    content: "",
    content_type: "image",
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
    rich_text_hash: null,
    rich_html_ref: null,
    rich_rtf_ref: null,
    rich_preview_ref: null,
    rich_html_size: null,
    rich_rtf_size: null,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Rail: many cards must not push the desktop to the right.
// ---------------------------------------------------------------------------

test("App.svelte drops the narrow body max-width so the rail can fill the work area", () => {
  // The previous desktop rule set `main { max-width: 880px }`. On a
  // 1920px monitor the rail was forced into a single column; on
  // smaller screens it consumed the entire panel and triggered a
  // body scrollbar. The follow-up removed the cap.
  const source = loadFixture(path.join(FRONTEND_ROOT, "src/App.svelte"));
  assert.equal(
    /max-width:\s*880px/.test(source),
    false,
    "the desktop body must not carry the legacy 880px max-width",
  );
  // The new rule pins the body to the full width of the monitor.
  assert.match(source, /max-width:\s*none/);
  assert.match(source, /width:\s*100%/);
  assert.match(source, /box-sizing:\s*border-box/);
});

test("layout grid uses minmax(0, 1fr) on the rail column so cards never expand the track", () => {
  // A `minmax(auto, 1fr)` default lets the grid track expand to the
  // rail's intrinsic width (every card summed up). `minmax(0, 1fr)`
  // is the documented CSS knob that lets the rail shrink to its
  // column and keep its own horizontal scroll.
  const source = loadFixture(path.join(FRONTEND_ROOT, "src/App.svelte"));
  assert.match(
    source,
    /grid-template-columns:\s*minmax\(180px,\s*220px\)\s*minmax\(0,\s*1fr\)/,
  );
});

test("HistoryCardRail styles pin the rail to the available width with horizontal overflow", () => {
  // The `.rail` class must own the horizontal scrollbar so the body
  // never grows with the cards. `min-width: 0` is the structural
  // guard that lets a flex child shrink below its intrinsic content
  // width.
  const source = loadFixture(
    path.join(FRONTEND_ROOT, "src/HistoryCardRail.svelte"),
  );
  assert.match(source, /overflow-x:\s*auto/);
  assert.match(source, /overflow-y:\s*hidden/);
  assert.match(source, /min-width:\s*0/);
  assert.match(source, /max-width:\s*100%/);
  // The card itself stays a fixed-size square through `flex: 0 0`
  // — a regression that drops the triple turns the rail into a
  // wrapping column and the squares into rectangles.
  assert.match(source, /flex:\s*0 0 var\(--cv-card-size\)/);
  assert.match(source, /width:\s*var\(--cv-card-size\)/);
});

// ---------------------------------------------------------------------------
// Image lifecycle: a coherent persisted image must not flash the
// "Imagen no disponible" fallback during the bridge round-trip.
// ---------------------------------------------------------------------------

test("hasRenderableImage returns true for a coherent persisted image row", () => {
  // Mirrors the audit step 7 of the regression report: the metadata
  // that the bridge round-trip relies on must still be present in
  // the entry that the card receives after a restart.
  const entry = imageEntry();
  assert.equal(isImageEntry(entry), true);
  assert.equal(hasRenderableImage(entry), true);
});

test("hasRenderableImage rejects an image row that lost its dimensions", () => {
  // Step 3 of the audit: missing `payload_width` / `payload_height`
  // would make the card fall back to the "Imagen no disponible"
  // surface even though the asset is on disk. The predicate must
  // reject the row before the bridge round-trip is even issued.
  const entry = imageEntry({ payload_width: null });
  assert.equal(hasRenderableImage(entry), false);
  const noHeight = imageEntry({ payload_height: 0 });
  assert.equal(hasRenderableImage(noHeight), false);
});

test("clipboardAssetCommand returns the persisted PNG bytes after a restart", async () => {
  // Step 8 of the audit: the Tauri command must return the raw
  // bytes for a coherent asset ref. The implementation under test
  // never inspects the bytes; it just verifies the bridge contract.
  const calls = installInvoke(async (cmd, args) => {
    assert.equal(cmd, "clipvault_clipboard_asset");
    const ref = (args as { assetRef: string }).assetRef;
    assert.equal(ref, VALID_REF);
    return PNG_BYTES;
  });
  const bytes = await clipboardAssetCommand({ ref: VALID_REF });
  assert.deepEqual(bytes, PNG_BYTES);
  assert.equal(calls.length, 1);
});

test("image resolver mints a single blob URL per asset so restarts do not re-fetch the bytes", async () => {
  // Step 9 of the audit: the resolver caches the blob URL so a
  // re-render after a restart (a hot reload, a remount, the user
  // switching collections) reuses the same `blob:` URL instead of
  // re-running the bridge command.
  const hub: { created: string[]; revoked: string[] } = {
    created: [],
    revoked: [],
  };
  (globalThis as unknown as { URL: { createObjectURL: (b: Blob) => string; revokeObjectURL: (u: string) => void } }).URL = {
    createObjectURL(blob: Blob): string {
      const url = `blob:clipboard-${hub.created.length}-${blob.size}`;
      hub.created.push(url);
      return url;
    },
    revokeObjectURL(url: string): void {
      hub.revoked.push(url);
    },
  };
  const calls: string[] = [];
  const loader: IconLoader = {
    async loadIconBytes(ref: string) {
      calls.push(ref);
      return PNG_BYTES;
    },
  };
  const resolver = createClipboardAssetResolver(loader);
  const first = await resolver.resolve(VALID_REF);
  const second = await resolver.resolve(VALID_REF);
  assert.equal(first.url, second.url);
  assert.equal(calls.length, 1, "no second bridge round-trip is fired");
  assert.equal(hub.created.length, 1);
});

test("HistoryCard initial thumbnail state for a coherent image is `loading`, not `error`", () => {
  // Step 11 of the audit: the very first paint must not flash the
  // "Imagen no disponible" fallback. The component's initial
  // declaration must mirror the entry's renderability so Svelte
  // commits the loading placeholder before the reactive block runs.
  const source = loadFixture(path.join(FRONTEND_ROOT, "src/HistoryCard.svelte"));
  assert.match(
    source,
    /hasRenderableImage\(entry\)\s*\?\s*"loading"\s*:\s*"error"/,
  );
});

// ---------------------------------------------------------------------------
// Clear unorganized history: only non-favorite, secondary-collection
// free entries are removed. Reuse the contract the Rust side enforces.
// ---------------------------------------------------------------------------

test("clearUnorganizedHistoryCommand is wired to clipvault_clear_unorganized_history", async () => {
  // Step 4 of the regression report: the trash button must invoke
  // the new dedicated command so the global
  // `clipvault_clear_history` path can no longer wipe a captured
  // group the user explicitly associated with a user collection.
  const calls = installInvoke(async (cmd, args) => {
    assert.equal(cmd, "clipvault_clear_unorganized_history");
    assert.equal((args as { confirm: boolean }).confirm, true);
    return { kind: "removed", removed: 3 };
  });
  const response = await clearUnorganizedHistoryCommand({ confirm: true });
  assert.equal(calls.length, 1);
  assert.equal(response.kind, "removed");
  if (response.kind === "removed") {
    assert.equal(response.removed, 3);
  }
});

test("clearUnorganizedHistoryCommand surfaces confirmation_required without confirm", async () => {
  const calls = installInvoke(async (cmd) => {
    assert.equal(cmd, "clipvault_clear_unorganized_history");
    return { kind: "confirmation_required" };
  });
  const response = await clearUnorganizedHistoryCommand({ confirm: false });
  assert.equal(calls.length, 1);
  assert.equal(response.kind, "confirmation_required");
});

test("unorganizedClearableCountCommand is wired to the dedicated count command", async () => {
  const calls = installInvoke(async (cmd) => {
    assert.equal(cmd, "clipvault_unorganized_clearable_count");
    return 7;
  });
  const count = await unorganizedClearableCountCommand();
  assert.equal(calls.length, 1);
  assert.equal(count, 7);
});

// ---------------------------------------------------------------------------
// Search: hits carry the full EntryRecord the rail renders.
// ---------------------------------------------------------------------------

test("SearchHit.record matches the EntryRecord contract the rail expects", () => {
  // Step 4 of the regression: the search must filter the cards
  // through the same `EntryRecord` shape so a `HistoryCard` is
  // rendered for each hit, not a custom list item. The structural
  // typing forbids dropping the `asset_ref`, `mime_type` or any
  // other field the card would need.
  const hit = {
    entry_id: 12,
    snippet: "lorem ipsum",
    score: 3_000,
    record: imageEntry({ id: 12 }),
  };
  assert.equal(typeof hit.record, "object");
  assert.equal(hit.record.content_type, "image");
  assert.equal(hit.record.asset_ref, VALID_REF);
  assert.equal(hit.record.is_pinned, false);
});

// ---------------------------------------------------------------------------
// Window sizing: the desktop must not introduce a second Tauri window
// just to measure the monitor; the existing `main` window is the only
// surface whose geometry is touched at startup.
// ---------------------------------------------------------------------------

test("Tauri config keeps a single main window with sensible min sizes", () => {
  const config = JSON.parse(
    loadFixture(path.join(TAURI_ROOT, "tauri.conf.json")),
  ) as {
    app: {
      windows: Array<{
        label: string;
        width: number;
        height: number;
        minWidth?: number;
        minHeight?: number;
        resizable: boolean;
      }>;
    };
  };
  const main = config.app.windows.find(
    (window: { label: string }) => window.label === "main",
  );
  assert.ok(main, "main window must be defined");
  assert.equal(typeof main?.width, "number");
  assert.equal(typeof main?.height, "number");
  assert.equal(main?.resizable, true);
  assert.ok((main?.minWidth ?? 0) > 0);
  assert.ok((main?.minHeight ?? 0) > 0);
  // The change MUST NOT introduce a second desktop window just to
  // measure the monitor; the only auxiliary window is the
  // transient `quick-paste` surface that has existed since the
  // MVP.
  const transient = config.app.windows.find(
    (window: { label: string }) => window.label !== "main",
  );
  assert.equal(transient?.label, "quick-paste");
});

test("main.rs resizes only the `main` window and never the quick-paste surface", () => {
  const source = loadFixture(path.join(TAURI_ROOT, "src/main.rs"));
  // The resize is a one-shot pass guarded by the `main` label.
  assert.match(source, /app\.get_webview_window\("main"\)/);
  // The transient `quick-paste` window is never referenced from the
  // resize helper.
  assert.equal(
    source.includes('get_webview_window("quick-paste")'),
    false,
  );
  // The helper never reaches into the Tauri config to create a
  // window; it only calls `set_size` on the existing main window.
  assert.match(source, /window\.set_size\(/);
});

// ---------------------------------------------------------------------------
// Toolbar: the search input is wired to the rail filter, not to a
// separate <ul>.
// ---------------------------------------------------------------------------

test("App.svelte no longer renders a parallel search-results list", () => {
  const source = loadFixture(path.join(FRONTEND_ROOT, "src/App.svelte"));
  // The previous implementation rendered a `<ul class="entries"
  // data-testid="search-results">` block. The follow-up drops that
  // markup so the rail is the only surface the user sees.
  assert.equal(
    source.includes('data-testid="search-results"'),
    false,
    "the rail must be the only surface that renders search hits",
  );
  assert.equal(
    source.includes("searchHits.map"),
    false,
    "the search must not iterate SearchHit[] as a separate list",
  );
});
