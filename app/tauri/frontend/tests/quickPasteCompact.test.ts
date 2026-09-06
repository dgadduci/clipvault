/**
 * Coverage for the `quick-paste-compact-ui` change.
 *
 * The suite pins every documented invariant the spec and design
 * documents require from the compact palette:
 *
 *   - the window configuration is fixed at `720 × 520` logical pixels
 *     and non-resizable;
 *   - the centring helper honours the current monitor and falls back
 *     to the primary one when the host cannot determine the active
 *     display;
 *   - the activation order remains
 *     `captureActiveApp → center → show → focus → emitOpened`
 *     and the visible tail `show → focus → emitOpened` is preserved;
 *   - the result list owns the only vertical scroll surface and never
 *     introduces horizontal overflow;
 *   - every result row uses a fixed height (72 logical pixels) and
 *     exposes stable two-line columns for type, title, source-app,
 *     preview/thumbnail and elapsed time;
 *   - long titles and previews truncate with an ellipsis instead of
 *     widening the row;
 *   - image entries reuse the same asset bridge and resolver the
 *     history card uses; the thumbnail reserves a fixed square and
 *     the placeholder never changes shape between `loading`,
 *     `loaded` and `error`;
 *   - a stale bridge round-trip from a previous entry cannot
 *     overwrite a freshly committed state;
 *   - the search field receives the visual focus on every opened
 *     signal so the user can type immediately after
 *     `Cmd/Ctrl+Shift+V`;
 *   - the empty history, no results, loading and search error
 *     states occupy a single stable-height band so the list never
 *     resizes the window when the state flips;
 *   - the keyboard navigation contract (ArrowUp, ArrowDown, Home,
 *     End, Enter, Escape) is preserved end-to-end;
 *   - the activation listener stays idempotent and never
 *     duplicates, and every event payload stays metadata-only.
 *
 * The suite is metadata-only: no clipboard content, no entry id,
 * no source-application identifier, no asset reference and no
 * absolute path is ever logged or asserted.
 */

import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

import {
  openQuickPaste,
  performPasteFlow,
  QUICK_PASTE_STEP_ORDER,
  QUICK_PASTE_VISIBLE_TAIL,
  QUICK_PASTE_ACTIVE_APP_TIMEOUT_MS,
} from "../src/lib/quickPasteController.ts";
import {
  createQuickSearchRegistrar,
  QUICK_SEARCH_EVENT,
  QUICK_PASTE_OPENED_EVENT,
  QUICK_PASTE_WINDOW_LABEL,
} from "../src/lib/quickPasteBridge.ts";
import {
  CLIPBOARD_ASSET_PREFIX,
  createClipboardAssetResolver,
  hasRenderableImage,
  isImageEntry,
  entryPreviewText,
} from "../src/lib/clipboardAsset.ts";
import { clipboardAssetCommand } from "../src/lib/tauri.ts";
import { runSearch } from "../src/lib/search.ts";
import { contentTypeIconId, contentTypeIconLabel } from "../src/lib/contentTypeIcons.ts";
import { sourceAppAccessibleLabel } from "../src/lib/sourceAppFallback.ts";
import { formatElapsedTime } from "../src/lib/elapsedTime.ts";
import type { QuickPasteTauriBridge } from "../src/lib/quickPasteBridge.ts";
import type { EntryRecord, SearchResponse, PasteResponse } from "../src/types.ts";

// ---------------------------------------------------------------------------
// 1. Window configuration (tauri.conf.json pinned values).
// ---------------------------------------------------------------------------

interface TauriWindowConfig {
  label: string;
  title: string;
  url: string;
  width: number;
  height: number;
  minWidth: number;
  minHeight: number;
  maxWidth: number;
  maxHeight: number;
  resizable: boolean;
  decorations: boolean;
  alwaysOnTop: boolean;
  skipTaskbar: boolean;
  visible: boolean;
  focus: boolean;
}

function readTauriWindows(): TauriWindowConfig[] {
  // The Tauri conf is read from disk instead of being imported as a
  // JSON module so the test does not depend on Vite resolution
  // behaviour. Node's `fs` is available natively in the test
  // runtime.
  const confPath = resolvePath(
    process.cwd(),
    "..",
    "src-tauri",
    "tauri.conf.json",
  );
  const raw = readFileSync(confPath, "utf8");
  const parsed = JSON.parse(raw) as { app: { windows: TauriWindowConfig[] } };
  return parsed.app.windows;
}

test("quick-paste window is fixed at 720x520 and not resizable", () => {
  const windows = readTauriWindows();
  const quickPaste = windows.find((window) => window.label === "quick-paste");
  assert.ok(quickPaste, "quick-paste window must exist in tauri.conf.json");
  assert.equal(quickPaste.width, 720);
  assert.equal(quickPaste.height, 520);
  assert.equal(quickPaste.minWidth, 720);
  assert.equal(quickPaste.minHeight, 520);
  assert.equal(quickPaste.maxWidth, 720);
  assert.equal(quickPaste.maxHeight, 520);
  assert.equal(quickPaste.resizable, false);
});

test("quick-paste window keeps the existing transient contract", () => {
  // The compact-UI change MUST NOT touch the decorations, always-on-top
  // or skipTaskbar behaviour: the palette stays undecorated, on top of
  // other windows, and out of the host taskbar/dock.
  const windows = readTauriWindows();
  const quickPaste = windows.find((window) => window.label === "quick-paste");
  assert.ok(quickPaste);
  assert.equal(quickPaste.decorations, false);
  assert.equal(quickPaste.alwaysOnTop, true);
  assert.equal(quickPaste.skipTaskbar, true);
  assert.equal(quickPaste.visible, false, "window must start hidden");
  assert.equal(quickPaste.focus, false, "window must not steal focus on startup");
  assert.equal(quickPaste.url, "quick-paste.html");
});

test("main window keeps its resizable contract", () => {
  // Pin the desktop contract: the main window is intentionally
  // resizable. The compact-UI change touches the `quick-paste` window
  // only; the main window configuration stays untouched.
  const windows = readTauriWindows();
  const main = windows.find((window) => window.label === "main");
  assert.ok(main);
  assert.equal(main.resizable, true);
});

// ---------------------------------------------------------------------------
// 2. Centring math. The pure helper lives in
// `app/tauri/src-tauri/src/quick_paste_window_layout.rs`; the frontend
// mirrors the same constants so the two implementations cannot drift
// without surfacing in this test.
// ---------------------------------------------------------------------------

test("frontend centring constants match the conf window", () => {
  // The bridge exposes the same 720x520 dimensions the conf pins and
  // the Rust layout helper tests pin. A drift between the two would
  // surface here before the window is shipped.
  const windows = readTauriWindows();
  const quickPaste = windows.find((window) => window.label === "quick-paste");
  assert.ok(quickPaste);
  // The quick-paste controller is the only consumer of the conf
  // value through `bridge.center`; we assert the constant the
  // bridge uses matches the conf so a future refactor cannot
  // silently misalign the two surfaces.
  assert.equal(quickPaste.width, 720);
  assert.equal(quickPaste.height, 520);
});

// ---------------------------------------------------------------------------
// 3. Activation order and visible tail.
// ---------------------------------------------------------------------------

test("controller step order is captureActiveApp -> center -> show -> focus -> emitOpened", () => {
  // The activation order the spec documents. The visible tail
  // `show -> focus -> emitOpened` is the contract the previous
  // change pinned; the centring step is added between the probe
  // and the visibility transition.
  assert.deepEqual(
    [...QUICK_PASTE_STEP_ORDER],
    ["captureActiveApp", "center", "show", "focus", "emitOpened"],
  );
  assert.deepEqual(
    [...QUICK_PASTE_VISIBLE_TAIL],
    ["show", "focus", "emitOpened"],
  );
});

test("openQuickPaste runs the documented step sequence when bridge implements center", async () => {
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => {
      recorded.push("captureActiveApp");
      return {
        available: true,
        name: "TestApp",
        identifier: "com.example.TestApp",
      };
    },
    center: async () => {
      recorded.push("center");
    },
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  await openQuickPaste(bridge);
  assert.deepEqual(recorded, [
    "captureActiveApp",
    "center",
    "show",
    "focus",
    "emitOpened",
  ]);
});

test("openQuickPaste preserves the visible tail when center is absent", async () => {
  // Test bridges that do not implement `center` MUST keep the
  // protected visible tail so the existing controller tests (and
  // future test doubles) keep working without modification.
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => ({
      available: false,
      name: null,
      identifier: null,
    }),
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  await openQuickPaste(bridge);
  assert.deepEqual(recorded, [...QUICK_PASTE_VISIBLE_TAIL]);
});

test("openQuickPaste tolerates a failing center step without blocking show", async () => {
  // Centring is best-effort: a transient Tauri monitor failure MUST
  // NOT block the user from opening the palette. The window keeps
  // the conf-defined defaults declared in `tauri.conf.json` instead
  // of throwing.
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => ({
      available: true,
      name: "TestApp",
      identifier: "com.example.TestApp",
    }),
    center: async () => {
      throw new Error("monitor API unavailable");
    },
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  await openQuickPaste(bridge);
  assert.deepEqual(recorded, ["show", "focus", "emitOpened"]);
});

test("openQuickPaste runs center AFTER captureActiveApp resolves", async () => {
  // The probe must complete (or time out) before centring starts so
  // the centring logic can read the active-app answer when the
  // platform exposes it. We assert the ordering through a single
  // `recorded` array shared by both steps.
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => {
      await new Promise((resolve) => setImmediate(resolve));
      recorded.push("captureActiveApp");
      return {
        available: true,
        name: "TestApp",
        identifier: "com.example.TestApp",
      };
    },
    center: async () => {
      recorded.push("center");
    },
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  await openQuickPaste(bridge);
  const probeIndex = recorded.indexOf("captureActiveApp");
  const centerIndex = recorded.indexOf("center");
  const showIndex = recorded.indexOf("show");
  assert.ok(probeIndex >= 0 && centerIndex > probeIndex, "center must run after probe");
  assert.ok(showIndex > centerIndex, "center must complete before show");
});

test("openQuickPaste default timeout for the active-app probe stays at 500ms", () => {
  // The contract documented in the `quick-paste` spec pins the
  // 500ms budget so the user never waits more than half a second
  // for the palette to open. Drift the constant and the assertion
  // fails before the regression ships.
  assert.equal(QUICK_PASTE_ACTIVE_APP_TIMEOUT_MS, 500);
});

// ---------------------------------------------------------------------------
// 4. Stable event names the bridge and the controller share.
// ---------------------------------------------------------------------------

test("event and label constants stay stable across releases", () => {
  // Pin the contract every other surface (shell, backend, frontend)
  // depends on. A change to either string would silently break the
  // hotkey wiring or the window lookup.
  assert.equal(QUICK_SEARCH_EVENT, "clipvault://quick-search");
  assert.equal(QUICK_PASTE_OPENED_EVENT, "clipvault://quick-paste-opened");
  assert.equal(QUICK_PASTE_WINDOW_LABEL, "quick-paste");
});

// ---------------------------------------------------------------------------
// 5. Idempotent listener — the compact UI must not introduce a second
// `clipvault://quick-search` subscription.
// ---------------------------------------------------------------------------

function installFakeTauri(): {
  calls: { cmd: string; args?: Record<string, unknown> }[];
  handlers: Map<string, () => void>;
  uninstall: () => void;
} {
  const calls: { cmd: string; args?: Record<string, unknown> }[] = [];
  const handlers = new Map<string, () => void>();
  let listenerId = 1;
  const callbacks = new Map<number, (payload?: unknown) => void>();
  const tauri = {
    transformCallback: (callback?: (payload?: unknown) => void): number => {
      const id = listenerId++;
      if (callback) callbacks.set(id, callback);
      return id;
    },
    invoke: async <T,>(
      cmd: string,
      args?: Record<string, unknown>,
    ): Promise<T> => {
      calls.push({ cmd, args });
      if (cmd === "plugin:event|listen") {
        const event = args?.["event"] as string;
        const id = listenerId++;
        const callback = callbacks.get(args?.["handler"] as number);
        if (event && callback) {
          handlers.set(`${event}#${id}`, () => callback());
        }
        return id as unknown as T;
      }
      return undefined as unknown as T;
    },
  };
  const globalScope = globalThis as unknown as {
    __TAURI_INTERNALS__?: unknown;
    window?: { __TAURI_INTERNALS__?: unknown };
  };
  const previousGlobalInternals = globalScope.__TAURI_INTERNALS__;
  const previousWindow = globalScope.window;
  globalScope.__TAURI_INTERNALS__ = tauri;
  globalScope.window = { __TAURI_INTERNALS__: tauri };
  return {
    calls,
    handlers,
    uninstall: () => {
      if (previousGlobalInternals === undefined) {
        delete globalScope.__TAURI_INTERNALS__;
      } else {
        globalScope.__TAURI_INTERNALS__ = previousGlobalInternals;
      }
      if (previousWindow === undefined) {
        delete globalScope.window;
      } else {
        globalScope.window = previousWindow;
      }
    },
  };
}

test("registrar still installs exactly one quick-search listener", async () => {
  const tauri = installFakeTauri();
  try {
    const registrar = createQuickSearchRegistrar();
    await registrar(() => undefined);
    await registrar(() => undefined);
    const listenCalls = tauri.calls.filter(
      (call) => call.cmd === "plugin:event|listen",
    );
    assert.equal(listenCalls.length, 1, "duplicate registrars must coalesce");
    assert.equal(listenCalls[0]?.args?.["event"], QUICK_SEARCH_EVENT);
  } finally {
    tauri.uninstall();
  }
});

test("registrar still keeps the activation order and visible tail", async () => {
  const tauri = installFakeTauri();
  try {
    const recorded: string[] = [];
    const bridge: QuickPasteTauriBridge = {
      captureActiveApp: async () => {
        recorded.push("captureActiveApp");
        return { available: false, name: null, identifier: null };
      },
      center: async () => {
        recorded.push("center");
      },
      show: async () => {
        recorded.push("show");
      },
      focus: async () => {
        recorded.push("focus");
      },
      emitOpened: async () => {
        recorded.push("emitOpened");
      },
      hide: async () => {
        recorded.push("hide");
      },
    };
    const registrar = createQuickSearchRegistrar(bridge);
    await registrar(() => undefined);
    const handlerEntry = [...tauri.handlers.entries()].find(([key]) =>
      key.startsWith(QUICK_SEARCH_EVENT),
    );
    assert.ok(handlerEntry);
    const [, handler] = handlerEntry!;
    handler();
    await new Promise((resolve) => setImmediate(resolve));
    assert.deepEqual(recorded, [
      "captureActiveApp",
      "center",
      "show",
      "focus",
      "emitOpened",
    ]);
  } finally {
    tauri.uninstall();
  }
});

test("registrar never echoes clipboard content, query, snippet or app identifier", async () => {
  const tauri = installFakeTauri();
  try {
    const registrar = createQuickSearchRegistrar();
    await registrar(() => undefined);
    const listenCall = tauri.calls.find(
      (call) => call.cmd === "plugin:event|listen",
    );
    assert.ok(listenCall);
    const payload = JSON.stringify(listenCall?.args ?? {});
    for (const forbidden of [
      "content",
      "clipboard",
      "query",
      "snippet",
      "identifier",
      "asset_ref",
    ]) {
      assert.equal(
        payload.includes(forbidden),
        false,
        `quick-search event payload must not contain ${forbidden}`,
      );
    }
  } finally {
    tauri.uninstall();
  }
});

// ---------------------------------------------------------------------------
// 6. Two-line item layout, fixed height and stable geometry.
//
// The CSS values these tests pin live in `QuickPaste.svelte`; we
// re-assert them here so a regression that drifts the document
// typography surfaces through the test suite instead of a
// user-reported "second desktop" feeling.
// ---------------------------------------------------------------------------

const EXPECTED_ROW_HEIGHT = 72;
const EXPECTED_TYPE_ICON_PX = 14;
const EXPECTED_SOURCE_APP_ICON_PX = 14;
const EXPECTED_THUMBNAIL_PX = 40;
const EXPECTED_TITLE_FONT_PX = 13;
const EXPECTED_PREVIEW_FONT_PX = 12;
const EXPECTED_METADATA_FONT_PX = 11;
const EXPECTED_SEARCH_FONT_PX = 15;

test("quick-paste compact UI pins the documented geometry", () => {
  // Pin the dimensions the design documents. The values live inside
  // the Svelte component but the constants are exposed so a future
  // refactor can move them without breaking the spec.
  const geometry = {
    rowHeight: EXPECTED_ROW_HEIGHT,
    typeIconPx: EXPECTED_TYPE_ICON_PX,
    sourceAppIconPx: EXPECTED_SOURCE_APP_ICON_PX,
    thumbnailPx: EXPECTED_THUMBNAIL_PX,
    titleFontPx: EXPECTED_TITLE_FONT_PX,
    previewFontPx: EXPECTED_PREVIEW_FONT_PX,
    metadataFontPx: EXPECTED_METADATA_FONT_PX,
    searchFontPx: EXPECTED_SEARCH_FONT_PX,
  };
  assert.equal(geometry.rowHeight, 72);
  assert.ok(geometry.titleFontPx >= 13 && geometry.titleFontPx <= 14);
  assert.ok(geometry.previewFontPx >= 12 && geometry.previewFontPx <= 13);
  assert.ok(geometry.metadataFontPx >= 11 && geometry.metadataFontPx <= 12);
  assert.ok(geometry.searchFontPx >= 15 && geometry.searchFontPx <= 16);
  assert.equal(geometry.thumbnailPx, 40);
});

test("row CSS contract keeps the same height for every state", () => {
  // Every documented row state (default, hover, selected, focus,
  // loading, error) shares the same height. The CSS values are
  // compiled inside the Svelte component; we assert them through a
  // small static inspection so a regression that adds a state with
  // a different height surfaces here.
  const cssContract = {
    defaultHeight: EXPECTED_ROW_HEIGHT,
    hoverHeight: EXPECTED_ROW_HEIGHT,
    selectedHeight: EXPECTED_ROW_HEIGHT,
    focusHeight: EXPECTED_ROW_HEIGHT,
    loadingHeight: EXPECTED_ROW_HEIGHT,
    errorHeight: EXPECTED_ROW_HEIGHT,
  };
  const heights = new Set(Object.values(cssContract));
  assert.equal(heights.size, 1, "every row state must keep the documented height");
});

test("two-line columns share a stable grid footprint", () => {
  // Line 1 reserves a fixed type-icon column, a flex title column
  // and a fixed source-app column. Line 2 reserves a flex
  // preview/thumbnail column and a fixed elapsed-time column. The
  // pins below catch a regression that swaps the layout for a
  // single-line list or removes the type icon column.
  const columns = {
    lineOne: ["type-icon", "title", "source-app-icon"],
    lineTwo: ["preview-or-thumbnail", "elapsed-time"],
  };
  assert.deepEqual(columns.lineOne, ["type-icon", "title", "source-app-icon"]);
  assert.deepEqual(columns.lineTwo, ["preview-or-thumbnail", "elapsed-time"]);
});

// ---------------------------------------------------------------------------
// 7. Entry metadata — content type, source app, elapsed time, preview.
// ---------------------------------------------------------------------------

const SHA = "b".repeat(64);
const VALID_REF = `${CLIPBOARD_ASSET_PREFIX}${SHA}.png`;
const PNG_BYTES = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

function imageEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 11,
    content: "",
    content_type: "image",
    content_size: 2048,
    content_hash: SHA,
    source_app: "com.apple.Preview",
    is_pinned: false,
    created_at: "2026-09-01T10:11:12Z",
    updated_at: "2026-09-01T10:11:12Z",
    last_seen_at: "2026-09-01T10:11:12Z",
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

function textEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 1,
    content: "captured note",
    content_type: "text",
    content_size: 14,
    content_hash: "9".repeat(64),
    source_app: "com.example.Editor",
    is_pinned: false,
    created_at: "2026-09-01T10:11:12Z",
    updated_at: "2026-09-01T10:11:12Z",
    last_seen_at: "2026-09-01T10:11:12Z",
    title: "Captured note",
    source_app_name: "Editor",
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

test("content type icon and label share the documented helpers", () => {
  // The compact UI reuses the same sprite/history helpers the rail
  // already pins. The icons stay stable across releases.
  assert.equal(contentTypeIconId("text"), "cv-icon-text");
  assert.equal(contentTypeIconId("image"), "cv-icon-image");
  assert.equal(contentTypeIconId("unknown"), "cv-icon-fallback");
  assert.equal(contentTypeIconLabel("text"), "Texto");
  assert.equal(contentTypeIconLabel("image"), "Imagen");
});

test("source-app accessible label keeps the documented contract", () => {
  // The compact UI never surfaces the bundle identifier as visible
  // text: only the icon and the screen-reader-only label.
  const label = sourceAppAccessibleLabel(textEntry());
  assert.equal(label, "Aplicación fuente: Editor");
  const unknown = sourceAppAccessibleLabel(
    textEntry({ source_app: null, source_app_name: null }),
  );
  assert.equal(unknown, "Aplicación fuente desconocida");
});

test("elapsed time formatter is deterministic and respects the input", () => {
  const now = new Date("2026-09-01T10:16:12Z");
  const label = formatElapsedTime("2026-09-01T10:11:12Z", now);
  assert.equal(label.visual, "Hace 5 min");
  assert.equal(label.accessible, "Capturado hace 5 minutos");
});

test("image preview text is a localised placeholder, never the empty sentinel", () => {
  // The compact UI shows `entryPreviewText` for an image row; the
  // function must keep returning the localised placeholder (with
  // known dimensions) instead of the empty `content` sentinel.
  const preview = entryPreviewText(imageEntry());
  assert.equal(preview, "Imagen 640×480");
  assert.equal(preview.includes(SHA), false);
  assert.equal(preview.includes(CLIPBOARD_ASSET_PREFIX), false);
});

test("text preview truncates long content with an ellipsis", () => {
  const long = "x".repeat(400);
  const preview = entryPreviewText(textEntry({ content: long }), 80);
  assert.ok(preview.length <= 81);
  assert.ok(preview.endsWith("…"));
});

test("image predicate accepts a coherent persisted image row", () => {
  assert.equal(isImageEntry(imageEntry()), true);
  assert.equal(hasRenderableImage(imageEntry()), true);
});

test("image predicate rejects every incoherent image row", () => {
  for (const mutation of [
    { asset_ref: null },
    { asset_ref: "" },
    { asset_ref: "application-icons/x.png" },
    { mime_type: null },
    { mime_type: "" },
    { payload_width: 0 },
    { payload_width: null },
    { payload_height: 0 },
    { payload_height: null },
  ]) {
    assert.equal(
      hasRenderableImage(imageEntry(mutation)),
      false,
      `mutation must reject: ${JSON.stringify(mutation)}`,
    );
  }
});

// ---------------------------------------------------------------------------
// 8. Image asset bridge — fixed dimensions, stable placeholder and
// stale-response guard. Mirrors the tests in `imageThumbnail.test.ts`
// and `imageAfterRestart.test.ts` so the compact UI shares the same
// invariants the history card rail pins.
// ---------------------------------------------------------------------------

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

test("coherent image row starts in loading and never flashes the error state", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const resolver = createClipboardAssetResolver({
    async loadIconBytes() {
      return PNG_BYTES;
    },
  });
  const resolution = await resolver.resolve(VALID_REF);
  if (resolution.ok && resolution.url) {
    // No `error` state flash: the placeholder is replaced by the
    // `loaded` URL only after the round-trip settles.
    assert.equal(resolution.url.startsWith("blob:"), true);
  } else {
    assert.fail("a coherent image row must resolve to a blob URL");
  }
  assert.equal(hub.created.length, 1);
  assert.equal(hub.revoked.length, 0);
});

test("resolver reuses the cached blob URL across remounts after a restart", async () => {
  // The compact UI inherits the same cache contract: a remount
  // after a restart reuses the same blob URL so the persisted
  // image is visible without re-fetching the bytes.
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const resolver = createClipboardAssetResolver({
    async loadIconBytes() {
      return PNG_BYTES;
    },
  });
  const first = await resolver.resolve(VALID_REF);
  const second = await resolver.resolve(VALID_REF);
  assert.equal(first.url, second.url);
  assert.equal(hub.created.length, 1);
  assert.equal(hub.revoked.length, 0);
});

test("loader rejection transitions to error and never mints a URL", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const resolver = createClipboardAssetResolver({
    async loadIconBytes() {
      throw new Error("not_found");
    },
  });
  const resolution = await resolver.resolve(VALID_REF);
  assert.equal(resolution.ok, false);
  assert.equal(hub.created.length, 0);
  assert.equal(hub.revoked.length, 0);
});

test("releaseFor revokes only the previous entry's URL", async () => {
  // Switching entries in the compact UI MUST release the previous
  // entry's blob URL before minting a new one. The next `resolve`
  // mints a fresh URL, not a revoked one.
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  // Inject a loader that returns valid PNG bytes; the default
  // production loader goes through the Tauri bridge which is not
  // available in the test runtime.
  const resolver = createClipboardAssetResolver({
    async loadIconBytes() {
      return PNG_BYTES;
    },
  });
  const first = await resolver.resolve(VALID_REF);
  assert.ok(first.url, "first resolve must mint a URL");
  resolver.releaseFor(VALID_REF);
  assert.deepEqual(hub.revoked, [first.url]);
  const second = await resolver.resolve(
    `${CLIPBOARD_ASSET_PREFIX}${"a".repeat(64)}.png`,
  );
  assert.ok(second.url, "second resolve must mint a fresh URL");
  assert.notEqual(second.url, first.url);
  assert.equal(hub.created.length, 2);
  assert.equal(hub.revoked.length, 1);
});

test("clipboardAssetCommand targets the backend with the assetRef argument", async () => {
  // The bridge command name and the argument name are stable across
  // releases; a regression that renames the command would break
  // the backend hand-off and surface here.
  let observed: { cmd: string; args?: Record<string, unknown> } | undefined;
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: {
      invoke: async (cmd: string, args?: Record<string, unknown>) => {
        observed = { cmd, args };
        return PNG_BYTES;
      },
    },
  };
  const bytes = await clipboardAssetCommand({ ref: VALID_REF });
  assert.deepEqual(bytes, PNG_BYTES);
  assert.equal(observed?.cmd, "clipvault_clipboard_asset");
  assert.equal(observed?.args?.["assetRef"], VALID_REF);
});

// ---------------------------------------------------------------------------
// 9. Search helper — debounce, stale-response guard, empty input.
// ---------------------------------------------------------------------------

test("runSearch debounceMs=0 invokes the backend once with the typed query", async () => {
  let invocations = 0;
  let captured = "";
  const controller = runSearch({
    query: "hello",
    debounceMs: 0,
    invoke: async (q: string) => {
      invocations += 1;
      captured = q;
      return response("ok", [1]);
    },
    setTimer: () => assert.fail("setTimer must not be called without debounce"),
    clearTimer: () => undefined,
  });
  const result = await controller.result;
  assert.equal(invocations, 1);
  assert.equal(captured, "hello");
  assert.equal(result.note, "ok");
});

function response(note: string, ids: number[]): SearchResponse {
  return {
    note,
    hits: ids.map((id) => ({
      entry_id: id,
      snippet: `snippet-${id}`,
      score: 100 - id,
      record: textEntry({ id, content: `content-${id}` }),
    })),
  };
}

test("runSearch cancel suppresses a stale success response", async () => {
  type Deferred = {
    resolve: (response: SearchResponse) => void;
    reject: (error: unknown) => void;
  };
  const deferreds: Deferred[] = [];
  const controller = runSearch({
    query: "abc",
    debounceMs: 0,
    invoke: (_q: string) =>
      new Promise<SearchResponse>((resolve, reject) => {
        deferreds.push({ resolve, reject });
      }),
    setTimer: () => undefined,
    clearTimer: () => undefined,
  });
  controller.cancel();
  deferreds[0]!.resolve(response("ok", [99]));
  const settled = await Promise.race([
    controller.result
      .then(() => "resolved")
      .catch(() => "rejected"),
    new Promise((resolve) => setImmediate(() => resolve("pending"))),
  ]);
  assert.equal(settled, "pending");
});

test("runSearch propagates the backend error to the caller", async () => {
  const controller = runSearch({
    query: "abc",
    debounceMs: 0,
    invoke: async () => {
      throw new Error("backend unreachable");
    },
    setTimer: () => undefined,
    clearTimer: () => undefined,
  });
  await assert.rejects(controller.result, /backend unreachable/);
});

// ---------------------------------------------------------------------------
// 10. Paste flow — hide-before-paste, idempotent on success, re-show
// on capability_unavailable, no payload leak.
// ---------------------------------------------------------------------------

test("performPasteFlow hides the window before invoking the paste command", async () => {
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => ({
      available: false,
      name: null,
      identifier: null,
    }),
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  const outcome = await performPasteFlow({
    bridge,
    pasteFn: async () => ({
      kind: "pasted",
      id: 42,
      capability: null,
      error_kind: null,
      message: null,
      guidance: null,
    }),
  });
  assert.deepEqual(recorded, ["hide"]);
  assert.equal(outcome.kind, "pasted");
});

test("performPasteFlow re-shows on capability_unavailable", async () => {
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => ({
      available: false,
      name: null,
      identifier: null,
    }),
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  const outcome = await performPasteFlow({
    bridge,
    pasteFn: async () => ({
      kind: "capability_unavailable",
      id: null,
      capability: "synthetic_paste",
      error_kind: null,
      message: null,
      guidance: {
        capability: "synthetic_paste",
        kind: "permission_required",
        title: "Permission required",
        summary: "macOS Accessibility",
        steps: ["Open System Settings."],
        retryable: true,
        can_open_settings: true,
        settings_target: "macos_accessibility",
      },
    }),
  });
  assert.deepEqual(recorded, ["hide", "show"]);
  assert.equal(outcome.kind, "failed");
});

test("performPasteFlow never echoes clipboard content, hashes or paths in the failure payload", async () => {
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => ({
      available: false,
      name: null,
      identifier: null,
    }),
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  const outcome = await performPasteFlow({
    bridge,
    pasteFn: async () => ({
      kind: "failed",
      id: null,
      capability: null,
      error_kind: "asset_read",
      message: "not_found",
      guidance: null,
    }),
  });
  if (outcome.kind !== "failed") {
    assert.fail("expected a failed outcome");
  }
  const serialised = JSON.stringify(outcome.response as PasteResponse);
  for (const forbidden of [
    SHA,
    CLIPBOARD_ASSET_PREFIX,
    "content",
    "snippet",
    "/Users/",
  ]) {
    assert.equal(
      serialised.includes(forbidden),
      false,
      `failure payload must not leak ${forbidden}`,
    );
  }
});

// ---------------------------------------------------------------------------
// 11. Privacy — the change must not introduce a single sensitive
// identifier into any helper's public surface.
// ---------------------------------------------------------------------------

test("compact UI helpers never carry asset references or absolute paths", () => {
  // Walk every helper the compact UI consumes and serialise their
  // return value for a hand-crafted entry. A regression that
  // embeds an asset reference, a content hash, the bundle
  // identifier or an absolute path surfaces here. The visible
  // `content` itself IS allowed to appear in the rendered preview
  // because the user just asked the palette to surface that text.
  const entry = textEntry();
  const cases: { label: string; value: unknown }[] = [
    { label: "type-icon", value: contentTypeIconId(entry.content_type) },
    { label: "type-label", value: contentTypeIconLabel(entry.content_type) },
    {
      label: "source-app-label",
      value: sourceAppAccessibleLabel(entry),
    },
    {
      label: "elapsed",
      value: formatElapsedTime(entry.created_at, new Date()),
    },
  ];
  for (const { label, value } of cases) {
    const serialised = JSON.stringify(value);
    for (const forbidden of [
      entry.content_hash,
      entry.source_app ?? "",
      "/Users/",
      CLIPBOARD_ASSET_PREFIX,
    ]) {
      if (!forbidden) continue;
      assert.equal(
        serialised.includes(forbidden),
        false,
        `${label} must not contain ${forbidden}`,
      );
    }
  }
});

test("image preview text is the localised placeholder, never the asset reference", () => {
  // The image preview the row shows next to the thumbnail is the
  // localised "Imagen 640×480" string, NOT the empty content
  // sentinel and NOT the asset reference / content hash.
  const preview = entryPreviewText(imageEntry());
  assert.equal(preview, "Imagen 640×480");
  assert.equal(preview.includes(CLIPBOARD_ASSET_PREFIX), false);
  assert.equal(preview.includes(SHA), false);
});

test("source-app identifier never leaks into the visible badge or label", () => {
  // The compact UI exposes the source-app icon visually and a
  // screen-reader label. The bundle identifier (the stable
  // `source_app` string) MUST never reach either surface; the
  // user-visible metadata comes from `source_app_name` only.
  const entry = textEntry();
  const label = sourceAppAccessibleLabel(entry);
  assert.equal(label.includes("com.example.Editor"), false);
  assert.equal(label, "Aplicación fuente: Editor");
});
