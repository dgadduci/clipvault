/**
 * Tests for the clipboard payload asset bridge and its Blob URL
 * lifecycle.
 *
 * The suite pins the contracts the change requires from the frontend:
 *
 * - the command name and the argument the backend expects;
 * - `asset_ref` is forwarded verbatim and stays relative — the frontend
 *   never invents, rewrites or prefixes a path;
 * - a coherent image row resolves to a `blob:` URL backed by an
 *   `image/png` `Blob`, so the card can render a thumbnail;
 * - an incoherent image row (missing reference, wrong namespace, absent
 *   MIME type, zero dimensions) short-circuits to the fallback WITHOUT
 *   a backend round-trip;
 * - a backend rejection collapses to the fallback instead of surfacing
 *   the error;
 * - `releaseFor` / `release` revoke every URL the resolver minted;
 * - the preview text of an image row is a localised placeholder, never
 *   the backend's empty `content` sentinel and never the reference.
 *
 * The tests need no Tauri runtime: a minimal `URL.createObjectURL`
 * shim and an injected loader keep them deterministic.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  CLIPBOARD_ASSET_PREFIX,
  IMAGE_CONTENT_TYPE,
  RICH_TEXT_ASSET_PREFIX,
  RICH_TEXT_PREVIEW_MIME,
  RICH_TEXT_PREVIEW_SUFFIX,
  createClipboardAssetResolver,
  createRichTextPreviewResolver,
  entryPreviewText,
  hasRenderableImage,
  hasRenderableRichText,
  imageDimensionsLabel,
  isImageEntry,
  pasteMenuActionsFor,
} from "../src/lib/clipboardAsset.ts";
import { clipboardAssetCommand, richTextPreviewCommand } from "../src/lib/tauri.ts";
import type { IconLoader } from "../src/lib/iconResolver.ts";
import { performPasteFlow } from "../src/lib/quickPasteController.ts";
import type { EntryRecord, PasteResponse } from "../src/types.ts";

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

type InvokeHandle = (
  cmd: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

function installTauriMock(invoker: InvokeHandle): void {
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: { invoke: invoker },
  };
}

const SHA = "a".repeat(64);
const VALID_REF = `${CLIPBOARD_ASSET_PREFIX}${SHA}.png`;

/** Minimal PNG signature; the frontend never parses the bytes. */
const PNG_BYTES = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

function imageEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 7,
    // The backend's empty sentinel for an image row.
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

function textEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    ...imageEntry(),
    content: "hello world",
    content_type: "text",
    asset_ref: null,
    mime_type: null,
    payload_width: null,
    payload_height: null,
    ...overrides,
  };
}

const RICH_HASH = "b".repeat(64);
const RICH_PREVIEW_REF = `${RICH_TEXT_ASSET_PREFIX}${RICH_HASH}${RICH_TEXT_PREVIEW_SUFFIX}`;
const RICH_HTML_REF = `${RICH_TEXT_ASSET_PREFIX}${RICH_HASH}.html`;

function richEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    ...textEntry(),
    content: "hello rich",
    rich_text_hash: RICH_HASH,
    rich_html_ref: RICH_HTML_REF,
    rich_rtf_ref: null,
    rich_preview_ref: RICH_PREVIEW_REF,
    rich_html_size: 64,
    rich_rtf_size: null,
    ...overrides,
  };
}

function utf8Bytes(value: string): number[] {
  return Array.from(new TextEncoder().encode(value));
}

function loaderReturning(bytes: number[] | null, calls: string[]): IconLoader {
  return {
    async loadIconBytes(ref: string) {
      calls.push(ref);
      return bytes;
    },
  };
}

// ---------------------------------------------------------------------
// Command bridge.
// ---------------------------------------------------------------------

test("clipboardAssetCommand targets the backend command with the assetRef argument", async () => {
  let observed: { cmd: string; args?: Record<string, unknown> } | undefined;
  installTauriMock(async (cmd, args) => {
    observed = { cmd, args };
    return PNG_BYTES;
  });
  await clipboardAssetCommand({ ref: VALID_REF });
  assert.equal(observed?.cmd, "clipvault_clipboard_asset");
  assert.equal(observed?.args?.assetRef, VALID_REF);
});

test("clipboardAssetCommand returns the raw byte array unchanged", async () => {
  installTauriMock(async () => PNG_BYTES);
  const bytes = await clipboardAssetCommand({ ref: VALID_REF });
  assert.deepEqual(bytes, PNG_BYTES);
});

test("clipboardAssetCommand forwards only a relative reference", async () => {
  let captured: Record<string, unknown> | undefined;
  installTauriMock(async (_cmd, args) => {
    captured = args;
    return PNG_BYTES;
  });
  await clipboardAssetCommand({ ref: VALID_REF });
  const ref = (captured as { assetRef: string }).assetRef;
  assert.equal(ref.startsWith("/"), false, "assetRef must stay relative");
  assert.equal(ref.startsWith(CLIPBOARD_ASSET_PREFIX), true);
});

test("clipboardAssetCommand propagates a backend rejection", async () => {
  installTauriMock(async () => {
    throw { kind: "invalid_asset_ref", message: "out_of_scope" };
  });
  await assert.rejects(
    clipboardAssetCommand({ ref: "/etc/passwd" }),
    (error: unknown) => {
      assert.equal((error as { kind: string }).kind, "invalid_asset_ref");
      return true;
    },
  );
});

// ---------------------------------------------------------------------
// Metadata predicates.
// ---------------------------------------------------------------------

test("isImageEntry branches on content_type only", () => {
  assert.equal(isImageEntry(imageEntry()), true);
  assert.equal(isImageEntry(textEntry()), false);
  assert.equal(isImageEntry({ content_type: "url" }), false);
});

test("hasRenderableImage accepts a coherent image row", () => {
  assert.equal(hasRenderableImage(imageEntry()), true);
});

test("hasRenderableImage rejects an incoherent image row", () => {
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

test("hasRenderableImage never treats a textual row as an image", () => {
  // Even a textual row that somehow carries a reference stays textual.
  assert.equal(
    hasRenderableImage(textEntry({ asset_ref: VALID_REF, mime_type: "image/png" })),
    false,
  );
});

test("imageDimensionsLabel formats known dimensions and rejects unknown ones", () => {
  assert.equal(imageDimensionsLabel(imageEntry()), "640×480");
  assert.equal(imageDimensionsLabel(imageEntry({ payload_width: null })), null);
  assert.equal(imageDimensionsLabel(imageEntry({ payload_height: 0 })), null);
});

// ---------------------------------------------------------------------
// Preview text: the sentinel must never reach the DOM.
// ---------------------------------------------------------------------

test("entryPreviewText never returns the empty content sentinel for an image", () => {
  const preview = entryPreviewText(imageEntry());
  assert.equal(preview, "Imagen 640×480");
  assert.notEqual(preview, "");
  assert.equal(preview.includes(SHA), false, "the hash must not be shown");
  assert.equal(
    preview.includes(CLIPBOARD_ASSET_PREFIX),
    false,
    "the asset reference must not be shown",
  );
});

test("entryPreviewText falls back to a bare label when dimensions are unknown", () => {
  assert.equal(entryPreviewText(imageEntry({ payload_width: null })), "Imagen");
});

test("entryPreviewText keeps the existing textual behaviour", () => {
  assert.equal(entryPreviewText(textEntry()), "hello world");
  assert.equal(
    entryPreviewText(textEntry({ content: "  spaced   out \n text " })),
    "spaced out text",
  );
  assert.equal(entryPreviewText(textEntry({ content: "   " })), "(vacío)");
  const long = "x".repeat(400);
  const truncated = entryPreviewText(textEntry({ content: long }));
  // Matches the card's pre-existing budget exactly: 117 characters
  // plus the ellipsis.
  assert.equal(truncated.length, 118);
  assert.equal(truncated.endsWith("…"), true);
});

// ---------------------------------------------------------------------
// Resolver: thumbnail, fallback and Blob URL lifecycle.
// ---------------------------------------------------------------------

test("resolver mints an image/png blob URL for a persisted asset", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(
    loaderReturning(PNG_BYTES, calls),
  );

  const resolution = await resolver.resolve(VALID_REF);
  assert.equal(resolution.ok, true);
  assert.equal(typeof resolution.url, "string");
  assert.equal(resolution.url?.startsWith("blob:"), true);
  assert.equal(resolution.blob?.type, "image/png");
  assert.deepEqual(calls, [VALID_REF], "the reference is forwarded verbatim");
  assert.equal(hub.created.length, 1);
});

test("resolver reuses the cached URL for the same reference", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(
    loaderReturning(PNG_BYTES, calls),
  );

  const first = await resolver.resolve(VALID_REF);
  const second = await resolver.resolve(VALID_REF);
  assert.equal(second.url, first.url);
  assert.equal(calls.length, 1, "a cached asset must not be re-fetched");
  assert.equal(hub.created.length, 1, "no second blob URL is minted");
});

test("resolver falls back to null when the loader rejects", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const resolver = createClipboardAssetResolver({
    async loadIconBytes() {
      throw new Error("asset missing on disk");
    },
  });
  const resolution = await resolver.resolve(VALID_REF);
  assert.equal(resolution.ok, false);
  assert.equal(resolution.url, null);
  assert.equal(hub.created.length, 0, "a failed load must not mint a URL");
});

test("resolver falls back to null when the loader returns no bytes", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(loaderReturning(null, calls));
  const resolution = await resolver.resolve(VALID_REF);
  assert.equal(resolution.ok, false);
  assert.equal(resolution.url, null);
});

test("resolver short-circuits a null reference without touching the loader", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(
    loaderReturning(PNG_BYTES, calls),
  );
  const resolution = await resolver.resolve(null);
  assert.equal(resolution.ok, false);
  assert.equal(calls.length, 0, "no round-trip for a missing reference");
});

test("releaseFor revokes the blob URL of a single entry", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(
    loaderReturning(PNG_BYTES, calls),
  );

  const resolution = await resolver.resolve(VALID_REF);
  assert.equal(resolver.cacheSize(), 1);
  resolver.releaseFor(VALID_REF);
  assert.deepEqual(hub.revoked, [resolution.url]);
  assert.equal(resolver.cacheSize(), 0);

  // A subsequent resolve mints a fresh URL rather than handing back a
  // revoked one.
  const again = await resolver.resolve(VALID_REF);
  assert.notEqual(again.url, resolution.url);
  assert.equal(calls.length, 2);
});

test("release revokes every blob URL the resolver holds", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createClipboardAssetResolver(
    loaderReturning(PNG_BYTES, calls),
  );

  const first = await resolver.resolve(`${CLIPBOARD_ASSET_PREFIX}${"1".repeat(64)}.png`);
  const second = await resolver.resolve(`${CLIPBOARD_ASSET_PREFIX}${"2".repeat(64)}.png`);
  assert.equal(resolver.cacheSize(), 2);

  resolver.release();
  assert.equal(resolver.cacheSize(), 0);
  assert.equal(hub.revoked.length, 2);
  assert.equal(hub.revoked.includes(first.url as string), true);
  assert.equal(hub.revoked.includes(second.url as string), true);
});

test("resolver never leaks an unrevoked URL after a full release cycle", async () => {
  // Regression guard for the memory contract: every URL the resolver
  // created must appear in the revoked list once the owner is gone.
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const resolver = createClipboardAssetResolver(
    loaderReturning(PNG_BYTES, []),
  );
  for (let index = 0; index < 4; index += 1) {
    await resolver.resolve(
      `${CLIPBOARD_ASSET_PREFIX}${String(index).repeat(64)}.png`,
    );
  }
  resolver.release();
  assert.equal(hub.created.length, 4);
  assert.deepEqual(
    [...hub.created].sort(),
    [...hub.revoked].sort(),
    "created and revoked sets must match",
  );
});

// ---------------------------------------------------------------------
// Quick-paste: an image entry reuses the exact transient flow.
//
// The controller is payload-agnostic by design, so these tests pin that
// property rather than duplicating the generic ordering coverage in
// `quickPasteController.test.ts`: an image selection must hide the
// window BEFORE the paste command runs, stay hidden on success, and
// re-show with the existing guidance on a `clipboard_write_image`
// capability failure.
// ---------------------------------------------------------------------

interface StepLog {
  steps: string[];
}

function bridgeRecording(log: StepLog) {
  return {
    captureActiveApp: async () => {
      log.steps.push("captureActiveApp");
      return { available: false, name: null, identifier: null };
    },
    show: async () => {
      log.steps.push("show");
    },
    focus: async () => {
      log.steps.push("focus");
    },
    emitOpened: async () => {
      log.steps.push("emitOpened");
    },
    hide: async () => {
      log.steps.push("hide");
    },
  };
}

test("selecting an image in quick-paste hides the window before pasting", async () => {
  const log: StepLog = { steps: [] };
  const entry = imageEntry();
  const outcome = await performPasteFlow({
    bridge: bridgeRecording(log),
    pasteFn: async () => {
      log.steps.push("paste");
      return {
        kind: "pasted",
        id: entry.id,
        capability: null,
        error_kind: null,
        message: null,
        guidance: null,
      };
    },
  });

  assert.deepEqual(log.steps, ["hide", "paste"]);
  assert.equal(outcome.kind, "pasted");
  assert.equal(outcome.windowStaysHidden, true);
});

test("an unavailable image write re-shows quick-paste with the existing guidance", async () => {
  const log: StepLog = { steps: [] };
  const outcome = await performPasteFlow({
    bridge: bridgeRecording(log),
    pasteFn: async () => {
      log.steps.push("paste");
      return {
        kind: "capability_unavailable",
        id: null,
        capability: "clipboard_write_image",
        error_kind: null,
        message: null,
        guidance: {
          capability: "clipboard_write_image",
          kind: "unsupported_session",
          title: "Esta sesión no admite imágenes",
          summary: "El portapapeles de la sesión no transporta imágenes.",
          steps: ["Usa una sesión X11 o copia el contenido como texto."],
          retryable: false,
          can_open_settings: false,
          settings_target: null,
        },
      };
    },
  });

  assert.deepEqual(log.steps, ["hide", "paste", "show"]);
  assert.equal(outcome.kind, "failed");
  assert.equal(outcome.windowStaysHidden, false);
  const response = outcome.response as PasteResponse;
  assert.equal(response.capability, "clipboard_write_image");
  // The guidance must describe a session limit, not a permission the
  // user could grant — a false permission prompt is the failure mode
  // this assertion guards against.
  assert.equal(response.guidance?.kind, "unsupported_session");
  assert.equal(response.guidance?.can_open_settings, false);
});

test("a failed image paste never reports success and carries no payload", async () => {
  const log: StepLog = { steps: [] };
  const outcome = await performPasteFlow({
    bridge: bridgeRecording(log),
    pasteFn: async () => {
      log.steps.push("paste");
      return {
        kind: "failed",
        id: null,
        capability: null,
        error_kind: "asset_read",
        message: "not_found",
        guidance: null,
      };
    },
  });

  assert.equal(outcome.kind, "failed");
  const serialised = JSON.stringify(outcome.response);
  assert.equal(serialised.includes(SHA), false, "no content hash");
  assert.equal(serialised.includes("clipboard/"), false, "no asset reference");
  assert.equal(serialised.includes("/Users/"), false, "no absolute path");
});

// ---------------------------------------------------------------------
// `clipboard-rich-text`: rich preview bridge and metadata predicates.
//
// The rich-text surface is intentionally a thin extension of the
// clipboard-asset one: a sanitised HTML preview served through a
// dedicated command, with the metadata-only predicates the card needs
// to decide whether to render the preview or fall back to the plain
// text. Every assertion here is metadata-only — no original HTML, no
// RTF, no content hashes reach the renderer.
// ---------------------------------------------------------------------

test("richTextPreviewCommand targets the backend with the previewRef argument", async () => {
  let observed: { cmd: string; args?: Record<string, unknown> } | undefined;
  installTauriMock(async (cmd, args) => {
    observed = { cmd, args };
    return utf8Bytes("<p>safe</p>");
  });
  await richTextPreviewCommand({ ref: RICH_PREVIEW_REF });
  assert.equal(observed?.cmd, "clipvault_rich_text_preview");
  assert.equal(observed?.args?.previewRef, RICH_PREVIEW_REF);
});

test("richTextPreviewCommand forwards only a relative preview reference", async () => {
  let captured: Record<string, unknown> | undefined;
  installTauriMock(async (_cmd, args) => {
    captured = args;
    return utf8Bytes("<p>safe</p>");
  });
  await richTextPreviewCommand({ ref: RICH_PREVIEW_REF });
  const ref = (captured as { previewRef: string }).previewRef;
  assert.equal(ref.startsWith("/"), false, "previewRef must stay relative");
  assert.equal(ref.startsWith(RICH_TEXT_ASSET_PREFIX), true);
  assert.equal(ref.endsWith(RICH_TEXT_PREVIEW_SUFFIX), true);
});

test("hasRenderableRichText accepts a coherent rich row", () => {
  assert.equal(hasRenderableRichText(richEntry()), true);
});

test("hasRenderableRichText rejects an incoherent rich row", () => {
  const cases: Array<[string, Partial<EntryRecord>]> = [
    ["missing rich_text_hash", { rich_text_hash: null }],
    ["missing html/rtf reference", { rich_html_ref: null, rich_rtf_ref: null }],
    ["missing preview reference", { rich_preview_ref: null }],
    ["preview outside the rich-text namespace", {
      rich_preview_ref: `clipboard/${RICH_HASH}${RICH_TEXT_PREVIEW_SUFFIX}`,
    }],
    ["preview without the .preview.html suffix", {
      rich_preview_ref: `${RICH_TEXT_ASSET_PREFIX}${RICH_HASH}.html`,
    }],
    ["absolute preview reference", {
      rich_preview_ref: `/etc/rich-text/${RICH_HASH}${RICH_TEXT_PREVIEW_SUFFIX}`,
    }],
  ];
  for (const [label, overrides] of cases) {
    assert.equal(
      hasRenderableRichText(richEntry(overrides)),
      false,
      `${label} must not be renderable`,
    );
  }
});

test("entryPreviewText keeps the plain-text fallback for a rich row", () => {
  // The card uses the plain-text preview while the rich preview
  // bridge is in flight or unavailable. The function MUST return the
  // canonical `content`, never the rich bytes or the hash.
  const entry = richEntry({ content: "  Hello rich  " });
  assert.equal(entryPreviewText(entry), "Hello rich");
  assert.equal(entryPreviewText(entry).includes(RICH_HASH), false);
});

test("rich card fallback keeps canonical plain text when preview loading fails", async () => {
  const dangerousText = '<script>alert(1)</script> & "quoted"';
  const entry = richEntry({ content: dangerousText });
  const resolver = createRichTextPreviewResolver({
    async loadIconBytes() {
      return null;
    },
  });
  const resolution = await resolver.resolve(entry.rich_preview_ref);
  assert.equal(resolution.ok, false);
  assert.equal(entryPreviewText(entry), dangerousText);
  assert.equal(entryPreviewText(entry).includes(RICH_HASH), false);
});

test("rich preview resolver stamps text/html on the produced blob", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const calls: string[] = [];
  const resolver = createRichTextPreviewResolver(
    loaderReturning(utf8Bytes("<p>safe</p>"), calls),
  );
  const resolution = await resolver.resolve(RICH_PREVIEW_REF);
  assert.equal(resolution.ok, true);
  assert.equal(resolution.url?.startsWith("blob:"), true);
  // The blob MIME is what the iframe uses to decide between rendering
  // HTML and treating the resource as a download. Pin the MIME so a
  // regression that drops back to `image/png` surfaces here.
  assert.equal(resolution.blob?.type, RICH_TEXT_PREVIEW_MIME);
  assert.deepEqual(calls, [RICH_PREVIEW_REF]);
});

test("rich preview resolver falls back to null on a backend rejection", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const resolver = createRichTextPreviewResolver({
    async loadIconBytes() {
      throw { kind: "invalid_preview_ref", message: "missing file" };
    },
  });
  const resolution = await resolver.resolve(RICH_PREVIEW_REF);
  assert.equal(resolution.ok, false);
  assert.equal(resolution.url, null);
  assert.equal(hub.created.length, 0);
});

test("rich preview resolver releases the blob URL through release / releaseFor", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const resolver = createRichTextPreviewResolver(
    loaderReturning(utf8Bytes("<p>safe</p>"), []),
  );
  const resolution = await resolver.resolve(RICH_PREVIEW_REF);
  resolver.releaseFor(RICH_PREVIEW_REF);
  assert.deepEqual(hub.revoked, [resolution.url]);
  assert.equal(resolver.cacheSize(), 0);
  resolver.release();
});

// ---------------------------------------------------------------------
// Card menu: image rows expose a single `Paste` action.
//
// Regression suite for the
// `clipboard-rich-text` / `clipboard-history-cards` spec: an image
// card must NEVER show the two textual paste actions because the
// bitmap does not carry rich flavours and the plain fallback would
// publish an empty content sentinel. The helper that drives the
// `HistoryCard` ellipsis menu is the only source of truth; the menu
// itself renders each item from the list the helper returns so the
// regression cannot drift back through a hard-coded button.
//
// Every assertion below is metadata-only — no asset reference, no
// content hash, no payload bytes ever reach a log or test output.
// ---------------------------------------------------------------------

test("image card menu exposes a single Paste action with a null mode", () => {
  const actions = pasteMenuActionsFor(imageEntry(), "Captura de imagen", {
    richPasteEnabled: false,
    pasteBusy: false,
  });
  assert.equal(actions.length, 1, "image rows render exactly one paste action");
  const [only] = actions;
  assert.equal(only.kind, "image-paste");
  assert.equal(only.label, "Paste");
  assert.equal(only.testId, "history-card-paste");
  // The legacy quick-paste contract: `mode = null` lets the Rust
  // paste service run the default mode, which is the documented path
  // for image rows (the bitmap write ignores the textual mode).
  assert.equal(only.mode, null);
  assert.equal(only.disabled, false);
  assert.equal(only.ariaLabel, "Pegar imagen de Captura de imagen");
  // The helper MUST NOT leak the asset reference or the content hash
  // into any user-visible string.
  assert.equal(only.tooltip.includes(SHA), false);
  assert.equal(only.tooltip.includes(CLIPBOARD_ASSET_PREFIX), false);
  assert.equal(only.ariaLabel.includes(SHA), false);
});

test("image card menu never exposes Paste de texto enriquecido or Paste de texto plano", () => {
  const actions = pasteMenuActionsFor(imageEntry(), "Captura", {
    richPasteEnabled: true,
    pasteBusy: false,
  });
  const labels = actions.map((action) => action.label);
  assert.equal(
    labels.includes("Paste de texto enriquecido"),
    false,
    "the image card must not render the rich-text action",
  );
  assert.equal(
    labels.includes("Paste de texto plano"),
    false,
    "the image card must not render the plain-text action",
  );
  // Test selectors are stable across releases; the regression suite
  // pins the absence of the legacy text selectors.
  const testIds = actions.map((action) => action.testId);
  assert.equal(testIds.includes("history-card-paste-rich"), false);
  assert.equal(testIds.includes("history-card-paste-plain"), false);
});

test("rich-text card menu keeps both paste actions when rich metadata is available", () => {
  const entry = richEntry();
  const actions = pasteMenuActionsFor(entry, "Recibo", {
    richPasteEnabled: true,
    pasteBusy: false,
  });
  assert.equal(actions.length, 2);
  const [rich, plain] = actions;
  assert.equal(rich.kind, "text-rich-paste");
  assert.equal(rich.label, "Paste de texto enriquecido");
  assert.equal(rich.testId, "history-card-paste-rich");
  assert.equal(rich.mode, "rich");
  assert.equal(rich.disabled, false);
  assert.equal(plain.kind, "text-plain-paste");
  assert.equal(plain.label, "Paste de texto plano");
  assert.equal(plain.testId, "history-card-paste-plain");
  assert.equal(plain.mode, "plain");
  assert.equal(plain.disabled, false);
});

test("plain-text card menu disables the rich action but keeps the plain action enabled", () => {
  const entry = textEntry();
  const actions = pasteMenuActionsFor(entry, "Saludo", {
    richPasteEnabled: false,
    pasteBusy: false,
  });
  assert.equal(actions.length, 2);
  const [rich, plain] = actions;
  // A plain-text entry has no rich metadata: the rich action stays
  // in the menu (so the user can see the affordance exists) but is
  // visibly and accessibly disabled. The plain action remains
  // enabled because the user can always paste plain text.
  assert.equal(rich.disabled, true);
  assert.equal(rich.mode, "rich");
  assert.equal(plain.disabled, false);
  assert.equal(plain.mode, "plain");
  // The tooltip for the disabled rich action explains why without
  // revealing clipboard content.
  assert.equal(rich.tooltip, "Esta entrada no tiene texto enriquecido.");
});

test("pasteBusy disables both text paste actions and never the image paste action", () => {
  const rich = pasteMenuActionsFor(textEntry(), "x", {
    richPasteEnabled: true,
    pasteBusy: true,
  });
  assert.equal(rich[0].disabled, true, "rich disabled while a paste is in flight");
  assert.equal(rich[1].disabled, true, "plain disabled while a paste is in flight");
  // The image action is single and stays enabled; the existing
  // `pasteBusy` guard inside `runPaste` rejects duplicate calls so
  // the user cannot double-fire even when the button is clickable.
  const image = pasteMenuActionsFor(imageEntry(), "x", {
    richPasteEnabled: false,
    pasteBusy: true,
  });
  assert.equal(image[0].disabled, false, "image paste button stays enabled");
});

test("pasteMenuActionsFor only inspects metadata and never copies content / hashes", () => {
  // Drive every branch and serialise the output; the regression
  // suite guards against any future change that would accidentally
  // embed an asset reference, a content hash, the raw content or an
  // absolute path into a menu action.
  const cases: Array<{ label: string; entry: EntryRecord; richEnabled: boolean }> = [
    { label: "image", entry: imageEntry(), richEnabled: false },
    { label: "rich", entry: richEntry(), richEnabled: true },
    { label: "plain", entry: textEntry({ content: "hello world" }), richEnabled: false },
  ];
  for (const { label, entry, richEnabled } of cases) {
    const actions = pasteMenuActionsFor(entry, label, {
      richPasteEnabled: richEnabled,
      pasteBusy: false,
    });
    const serialised = JSON.stringify(actions);
    assert.equal(serialised.includes(SHA), false, `${label}: no content hash`);
    assert.equal(
      serialised.includes(CLIPBOARD_ASSET_PREFIX),
      false,
      `${label}: no clipboard asset reference`,
    );
    assert.equal(
      serialised.includes(RICH_TEXT_ASSET_PREFIX),
      false,
      `${label}: no rich-text asset reference`,
    );
    assert.equal(serialised.includes("/Users/"), false, `${label}: no absolute path`);
    assert.equal(
      serialised.includes("hello world"),
      false,
      `${label}: no payload content`,
    );
  }
});
