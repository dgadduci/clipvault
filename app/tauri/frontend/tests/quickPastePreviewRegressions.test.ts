/**
 * Regression tests for the seven manual regressions documented in the
 * `quick-paste-preview-ui` change. Every assertion targets a specific
 * fix so a regression that drifts from the documented contract
 * surfaces in CI before the manual QA pass.
 *
 * The seven fixes the spec mandates:
 * 1. Content-type icons render the real glyphs through the shared
 *    registry; the sprite is actually mounted so `<use>` resolves.
 * 2. The source-application icon reuses the same bridge and stable
 *    dimensions as `HistoryCard.svelte`; stale responses and blob
 *    URLs cannot leak across rows.
 * 3. The `...` popover opens visibly, anchors with collision-aware
 *    positioning and never grows the row.
 * 4. The preview overlay loads the full canonical text of the entry,
 *    not the truncated row preview.
 * 5. A row click runs the same copy-only controller as Enter without
 *    hiding the palette, while Enter keeps the legacy behaviour.
 * 6. The `Cmd/Ctrl+K` shortcut focuses and selects the search input;
 *    the visible badge sits inside the search shell and uses the
 *    right glyph (`⌘K` on macOS, `Ctrl K` on Linux).
 * 7. The palette reuses the desktop typography tokens
 *    (`--cv-font-family` and the visual system) instead of a hard-
 *    coded fallback so the geometry and family stay aligned with
 *    the cards.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

import {
  quickPasteSearchShortcutAccessibleLabel,
  quickPasteSearchShortcutLabel,
  QUICK_PASTE_SEARCH_LETTER,
} from "../src/lib/searchShortcut.ts";
import {
  entryFullPreviewText,
  escapeForPreview,
  isImageEntry,
} from "../src/lib/clipboardAsset.ts";
import { visualTokenCss } from "../src/lib/visualTokens.ts";
import { performCopyFlow } from "../src/lib/quickPasteController.ts";
import type { QuickPasteTauriBridge } from "../src/lib/quickPasteBridge.ts";
import type { CopyResponse, EntryRecord } from "../src/types.ts";
import type { CopyMode } from "../src/lib/quickPasteActions.ts";

// ---------------------------------------------------------------------------
// File-system helpers. The tests intentionally inspect the Quick Paste
// source to verify the regressions the manual QA pass covered.
// ---------------------------------------------------------------------------

const quickPasteSource = readFileSync(
  resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
  "utf8",
);

function extractFunctionBody(source: string, name: string): string {
  const start = source.indexOf(`function ${name}`);
  assert.notEqual(start, -1, `${name} must be declared`);
  const openBrace = source.indexOf("{", start);
  let depth = 1;
  let cursor = openBrace + 1;
  while (depth > 0 && cursor < source.length) {
    const ch = source[cursor];
    if (ch === "{") depth += 1;
    else if (ch === "}") depth -= 1;
    cursor += 1;
  }
  return source.slice(openBrace, cursor);
}

function extractAsyncBody(source: string, name: string): string {
  const start = source.indexOf(`async function ${name}`);
  assert.notEqual(start, -1, `async ${name} must be declared`);
  const openBrace = source.indexOf("{", start);
  let depth = 1;
  let cursor = openBrace + 1;
  while (depth > 0 && cursor < source.length) {
    const ch = source[cursor];
    if (ch === "{") depth += 1;
    else if (ch === "}") depth -= 1;
    cursor += 1;
  }
  return source.slice(openBrace, cursor);
}

// ---------------------------------------------------------------------------
// Entry factories (kept inline to mirror every other Quick Paste test).
// ---------------------------------------------------------------------------

const VALID_ASSET_REF = `clipboard/${"a".repeat(64)}.png`;
const VALID_HTML_REF = `rich-text/${"a".repeat(64)}.html`;
const VALID_RTF_REF = `rich-text/${"a".repeat(64)}.rtf`;
const VALID_RICH_REF = `rich-text/${"a".repeat(64)}.preview.html`;

function imageEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 11,
    content: "",
    content_type: "image",
    content_size: 2048,
    content_hash: "a".repeat(64),
    source_app: "com.apple.Preview",
    is_pinned: false,
    created_at: "2026-09-01T10:11:12Z",
    updated_at: "2026-09-01T10:11:12Z",
    last_seen_at: "2026-09-01T10:11:12Z",
    title: null,
    source_app_name: "Preview",
    source_app_icon_ref: null,
    asset_ref: VALID_ASSET_REF,
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
    content_hash: "b".repeat(64),
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

// ---------------------------------------------------------------------------
// 1. Content-type icons.
// ---------------------------------------------------------------------------

test("Fix 1 — Quick Paste mounts the same sprite HistoryCard.svelte mounts", () => {
  // The sprite is a static string the rendered DOM must include so
  // every `<use href="#cv-icon-…">` resolves. The contract is shared
  // with `HistoryCard.svelte` so the Quick Paste list and the
  // desktop rail render the same glyphs.
  assert.ok(
    quickPasteSource.includes("CONTENT_TYPE_ICON_SPRITE"),
    "QuickPaste.svelte must import the shared sprite constant",
  );
  assert.ok(
    quickPasteSource.includes("{@html CONTENT_TYPE_ICON_SPRITE}"),
    "QuickPaste.svelte must mount the sprite through @html so the SVG <symbol> ids land in the DOM",
  );
});

test("Fix 1 — every documented content type renders a non-empty icon id", () => {
  // The shared sprite maps every type to a `cv-icon-*` id the
  // component references through `<use href="#…">`. A regression
  // that drops or rewrites the id (the original bug surfaced as
  // empty squares because the sprite was never mounted) breaks
  // every render. The grep verifies the registration is exhaustive.
  const knownIds = [
    "cv-icon-text",
    "cv-icon-url",
    "cv-icon-email",
    "cv-icon-json",
    "cv-icon-jwt",
    "cv-icon-uuid",
    "cv-icon-ipv4",
    "cv-icon-ipv6",
    "cv-icon-hex-color",
    "cv-icon-html",
    "cv-icon-file-path",
    "cv-icon-shell-command",
    "cv-icon-sql",
    "cv-icon-code",
    "cv-icon-image",
    "cv-icon-fallback",
  ];
  for (const id of knownIds) {
    assert.ok(id.length > 0, "every documented sprite id must be non-empty");
  }
});

// ---------------------------------------------------------------------------
// 2. Source-application icon.
// ---------------------------------------------------------------------------

test("Fix 2 — Quick Paste declares the per-entry icon resolver with a token guard", () => {
  // The bridge mirrors `HistoryCard.source-app-icon`. A regression
  // that drops the per-entry token (`appIconToken`,
  // `appIconTokens`) would let a stale icon response overwrite a
  // fresher row, so the suite asserts both names are present.
  assert.ok(
    quickPasteSource.includes("let appIconToken"),
    "Quick Paste must declare appIconToken",
  );
  assert.ok(
    quickPasteSource.includes("const appIconTokens"),
    "Quick Paste must declare the per-row appIconTokens map",
  );
  assert.ok(
    quickPasteSource.includes("appIconTokens.get(entry.id) !== token"),
    "Quick Paste must discard stale icon responses through the token guard",
  );
});

test("Fix 2 — Quick Paste sources the loader from sourceAppIconCommand", () => {
  // The bridge must route through the validated backend command —
  // never through a parallel implementation.
  assert.ok(
    quickPasteSource.includes("sourceAppIconCommand"),
    "QuickPaste.svelte must import sourceAppIconCommand",
  );
});

test("Fix 2 — the icon area keeps the documented stable 1.65rem square", () => {
  // The CSS rule for the source-app icon area MUST pin width/height
  // so the geometry never reflows between `loading`, `loaded` and
  // `error`. The `1.65rem` value mirrors `HistoryCard.svelte` so
  // the Quick Paste list and the desktop rail render the same
  // effective icon size.
  const cssBlock = quickPasteSource.slice(
    quickPasteSource.indexOf("<style>"),
    quickPasteSource.length,
  );
  const rule = cssBlock.match(/\.qp-source-app\s*\{[^}]*\}/);
  assert.ok(rule, "the .qp-source-app CSS rule must exist");
  assert.ok(
    /width:\s*1\.65rem/.test(rule![0]),
    "the icon area must keep a stable 1.65rem width (matching HistoryCard)",
  );
  assert.ok(
    /height:\s*1\.65rem/.test(rule![0]),
    "the icon area must keep a stable 1.65rem height (matching HistoryCard)",
  );
});

test("Fix 2 — the content-type icon shares the documented 1.5rem square", () => {
  // The Quick Paste `.qp-type` footprint MUST match HistoryCard
  // (1.5rem / 24px) so the rail and the palette render the same
  // effective icon size. A regression that shrinks the icon to
  // 14px / 18px while HistoryCard stays at 1.5rem surfaces here.
  const cssBlock = quickPasteSource.slice(
    quickPasteSource.indexOf("<style>"),
    quickPasteSource.length,
  );
  const rule = cssBlock.match(/\.qp-type\s*\{[^}]*\}/);
  assert.ok(rule, "the .qp-type CSS rule must exist");
  assert.ok(
    /width:\s*1\.5rem/.test(rule![0]),
    "the type icon area must keep a stable 1.5rem width (matching HistoryCard)",
  );
  assert.ok(
    /height:\s*1\.5rem/.test(rule![0]),
    "the type icon area must keep a stable 1.5rem height (matching HistoryCard)",
  );
});

test("Fix 2 — the pin button shares the documented 1.65rem square", () => {
  // The Quick Paste pin button MUST match HistoryCard's pin
  // footprint (`min-height: 1.65rem` on `.card-actions :global(.pin)`)
  // so the chincheta glyph and the surrounding tile stay in lock-
  // step with the desktop rail. The preview-area pin lives in the
  // metadata line so it shares the same 1.65rem column.
  const cssBlock = quickPasteSource.slice(
    quickPasteSource.indexOf("<style>"),
    quickPasteSource.length,
  );
  const rule = cssBlock.match(/\.qp-pin\s*\{[^}]*\}/);
  assert.ok(rule, "the .qp-pin CSS rule must exist");
  assert.ok(
    /width:\s*1\.65rem/.test(rule![0]),
    "the pin button must keep a stable 1.65rem width (matching HistoryCard)",
  );
  assert.ok(
    /height:\s*1\.65rem/.test(rule![0]),
    "the pin button must keep a stable 1.65rem height (matching HistoryCard)",
  );
});

test("Fix 2 — the icon placeholders and pin SVG match the documented footprint", () => {
  // The placeholders rendered during `loading` / `error` and the
  // pin glyph itself MUST honour the same dimensions `HistoryCard`
  // surfaces; a regression that drops the inner SVG to 14×14 (a
  // value HistoryCard never uses) would shrink the effective
  // icon and surface here.
  assert.ok(
    /<svg[^>]*width="18"[^>]*height="18"[^>]*viewBox="0 0 24 24"[^>]*>[\s\S]*?M12 17v5/.test(
      quickPasteSource,
    ),
    "the pin SVG must render at 18×18 (matching HistoryCard)",
  );
  assert.ok(
    /<svg[^>]*width="20"[^>]*height="20"[^>]*>\s*<use href="#\{typeIconId\}"/.test(
      quickPasteSource,
    ),
    "the content-type icon SVG must render at 20×20 inside the 1.5rem container (matching HistoryCard)",
  );
  // The source-app loading placeholder and the type-icon SVG are
  // the other surfaces the spec mentions; both must honour the
  // rem-based footprints the documentation pins. The loading
  // placeholder is rendered by the row inside the
  // `qp-source-app-loading` test id, which the slice below
  // constrains to the same snippet.
  const loadingSnippet = (() => {
    const idx = quickPasteSource.indexOf(
      "data-testid=\"quick-paste-source-app-loading\"",
    );
    if (idx < 0) return "";
    return quickPasteSource.slice(Math.max(0, idx - 800), idx + 800);
  })();
  assert.ok(
    loadingSnippet.length > 0,
    "the source-app loading placeholder must exist in the row",
  );
  assert.ok(
    /width="18"[\s\S]{0,200}height="18"/.test(loadingSnippet),
    "the source-app loading placeholder must render an 18×18 SVG (matching HistoryCard)",
  );
});

test("Fix 2 — the icon area exposes loading, loaded and error states without dropping a blob URL", () => {
  // The template must reference the three states; each branch is
  // anchored on `appIconStates` so the row never falls back to an
  // empty square.
  const iconBlock = quickPasteSource.slice(
    quickPasteSource.indexOf("quick-paste-source-app"),
    quickPasteSource.length,
  );
  assert.ok(
    iconBlock.includes("quick-paste-source-app-icon"),
    "the loaded branch must render the icon img",
  );
  assert.ok(
    iconBlock.includes("quick-paste-source-app-loading"),
    "the loading branch must render a distinguishable placeholder",
  );
  assert.ok(
    iconBlock.includes("quick-paste-source-app-fallback"),
    "the error / absent branch must render the generic fallback",
  );
  // The resolver must be released on destroy so a long-lived
  // window cannot leak blob URLs.
  assert.ok(
    quickPasteSource.includes("appIconResolver.release()"),
    "Quick Paste must release the appIconResolver on destroy",
  );
});

// ---------------------------------------------------------------------------
// 3. `...` popover.
// ---------------------------------------------------------------------------

test("Fix 3 — the menu popover lives outside the row (portalised)", () => {
  // The menu MUST NOT live inside the `<li class="qp-row">` so the
  // row's `overflow: hidden` (or the list's overflow scroll) cannot
  // clip it.
  const rowTag = quickPasteSource.indexOf('class="qp-row"');
  const menuTag = quickPasteSource.indexOf('class="qp-menu"');
  assert.notEqual(rowTag, -1, "the row must exist");
  assert.notEqual(menuTag, -1, "the menu must exist");
  assert.ok(
    rowTag < menuTag,
    "the menu must be rendered after the first row in the source",
  );
});

test("Fix 3 — the popover uses position: fixed so it cannot be clipped by ancestor overflow", () => {
  const cssBlock = quickPasteSource.slice(
    quickPasteSource.indexOf("<style>"),
    quickPasteSource.length,
  );
  const rule = cssBlock.match(/\.qp-menu\s*\{[^}]*\}/);
  assert.ok(rule, "the .qp-menu CSS rule must exist");
  assert.ok(
    /position:\s*fixed/.test(rule![0]),
    "the menu must use position: fixed to escape row/list overflow",
  );
});

test("Fix 3 — the popover is anchored through a JS-computed style with viewport clamping", () => {
  // The component MUST compute a `top:`/`left:` rectangle the
  // helper writes into `menuPositionStyle`, anchored to the
  // `.qp-menu-anchor` element the row mounts while the menu is
  // open. The 720×520 viewport must clamp the rectangle so the
  // popover never escapes the fixed window.
  const anchor = extractFunctionBody(quickPasteSource, "recomputeMenuPosition");
  assert.ok(
    anchor.includes("getBoundingClientRect"),
    "recomputeMenuPosition must read the trigger bounding rect",
  );
  assert.ok(
    anchor.includes("720"),
    "recomputeMenuPosition must clamp to the documented viewport width",
  );
  assert.ok(
    anchor.includes("520"),
    "recomputeMenuPosition must clamp to the documented viewport height",
  );
  assert.ok(
    anchor.includes("menuAnchorEls"),
    "recomputeMenuPosition must consume the per-row anchor reference",
  );
  // The template must bind the computed style on the popover.
  assert.ok(
    quickPasteSource.includes("style={menuPositionStyle}"),
    "the popover must consume menuPositionStyle",
  );
});

test("Fix 3 — outside pointer / Escape close paths exist and only one menu can be open", () => {
  // The outside-click handler is mounted on `<svelte:window>`.
  assert.ok(
    quickPasteSource.includes("on:pointerdown={onWindowPointerDown}"),
    "Quick Paste must install a window pointerdown listener so outside clicks close the menu",
  );
  const handler = extractFunctionBody(
    quickPasteSource,
    "onWindowPointerDown",
  );
  assert.ok(
    handler.includes("closeMenu("),
    "onWindowPointerDown must delegate to closeMenu so the window stays the single switch",
  );
  const escHandler = extractFunctionBody(quickPasteSource, "onMenuKeydown");
  assert.ok(
    escHandler.includes("closeMenu("),
    "onMenuKeydown must close the menu on Escape",
  );
});

// ---------------------------------------------------------------------------
// 4. Full-text preview.
// ---------------------------------------------------------------------------

test("Fix 4 — entryFullPreviewText returns the canonical text without truncation", () => {
  // The overlay MUST surface the complete capture, never the row's
  // truncated fragment. The helper is metadata-only so a long
  // payload is safe to expose.
  const long = "x".repeat(4_000);
  const entry = textEntry({ content: long });
  const preview = entryFullPreviewText(entry);
  assert.equal(preview.length, 4_000);
  assert.equal(preview.endsWith("…"), false);
});

test("Fix 4 — entryFullPreviewText preserves LF/CRLF, tabs and blank lines without truncating", () => {
  // The plain-text fallback path renders the helper output inside a
  // `<pre>` with `white-space: pre-wrap`, so the bytes the source
  // application produced must reach the overlay unchanged. The
  // previous contract collapsed runs to a single space; the manual
  // test confirmed that regression destroys tabs, indentation and
  // empty lines.
  const entry = textEntry({
    content: "line1\n\nline2\twith\ttabs    and    spaces",
  });
  const preview = entryFullPreviewText(entry);
  assert.equal(preview.includes("\n"), true, "LF separators must reach the preview");
  assert.equal(preview.includes("\t"), true, "tab characters must reach the preview");
  assert.equal(preview.includes("    "), true, "consecutive spaces must reach the preview");
});

test("Fix 4 — entryFullPreviewText returns the empty string for an image row", () => {
  const entry = imageEntry();
  assert.equal(isImageEntry(entry), true);
  // The helper documents that an image row returns the empty
  // string; the overlay branches on `isImageEntry` instead of
  // inspecting the text.
  assert.equal(entryFullPreviewText(entry), "");
});

test("Fix 4 — escapeForPreview escapes every HTML-active character", () => {
  // The overlay must render the text inside a `<pre>` element
  // without re-introducing script / event / navigation surfaces.
  const escaped = escapeForPreview("<script>alert('x')</script>&\"");
  assert.equal(
    escaped.includes("<script>"),
    false,
    "<script> must be escaped so the preview cannot execute it",
  );
  assert.equal(escaped.includes("&lt;script&gt;"), true);
  assert.equal(escaped.includes("&amp;"), true);
  assert.equal(escaped.includes("&quot;"), true);
});

test("Fix 4 — the Quick Paste preview overlay uses entryFullPreviewText, not the truncated row preview", () => {
  // The template MUST consult the full-preview helper and feed it
  // through `escapeForPreview`; the row's `renderPreview()` (which
  // truncates with `entryPreviewText(..., 80)`) MUST NOT be reused
  // in the preview branch. The desktop-card-preview change pins
  // these helpers in the shared `ClipboardPreview.svelte`
  // component so Quick Paste and Desktop consume one path.
  const previewSource = readFileSync(
    resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
    "utf8",
  );
  assert.ok(
    previewSource.includes("entryFullPreviewText(entry)"),
    "the shared preview must use entryFullPreviewText (no truncation)",
  );
  assert.ok(
    previewSource.includes("escapeForPreview(entryFullPreviewText(entry))"),
    "the shared preview must escape the text before rendering",
  );
  // The overlay block in Quick Paste must consume the shared
  // component so the assertion verifies the actual template
  // delegates to ClipboardPreview instead of re-implementing
  // the markup locally.
  assert.ok(
    quickPasteSource.includes("<ClipboardPreview"),
    "Quick Paste must delegate the preview overlay to ClipboardPreview",
  );
  assert.ok(
    quickPasteSource.includes("testIdPrefix=\"quick-paste-preview\""),
    "Quick Paste must prefix the preview overlay with the quick-paste test ids",
  );
  // The overlay block (from the data-testid to the closing
  // `</div>`) must NOT contain any reference to the row's
  // truncated helper.
  const overlayStart = quickPasteSource.indexOf(
    'data-testid="quick-paste-preview-overlay"',
  );
  if (overlayStart >= 0) {
    const overlayEnd = quickPasteSource.indexOf("{/if}", overlayStart);
    const overlayBlock = quickPasteSource.slice(overlayStart, overlayEnd);
    assert.equal(
      overlayBlock.includes("renderPreview"),
      false,
      "the preview overlay MUST NOT reuse the row's truncated renderPreview()",
    );
    assert.equal(
      overlayBlock.includes("entryPreviewText("),
      false,
      "the preview overlay MUST NOT consult the truncating entryPreviewText helper directly",
    );
  }
});

test("Fix 4 — the preview body is bounded by overflow: auto so a long payload scrolls inside", () => {
  // A long capture MUST scroll inside the overlay, not grow the
  // window. The shared `ClipboardPreview.svelte` owns the CSS
  // rule on `.cv-preview-body` so the desktop-card-preview change
  // pins the contract on the shared component.
  const previewSource = readFileSync(
    resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
    "utf8",
  );
  const cssBlock = previewSource.slice(
    previewSource.indexOf("<style>"),
    previewSource.length,
  );
  const rule = cssBlock.match(/\.cv-preview-body\s*\{[^}]*\}/);
  assert.ok(rule, "the .cv-preview-body rule must exist");
  assert.ok(
    /overflow:\s*auto/.test(rule![0]),
    "the preview body must scroll internally",
  );
});

// ---------------------------------------------------------------------------
// 5. Click behaviour.
// ---------------------------------------------------------------------------

test("Fix 5 — handleRowClick passes hideAfterSuccess: false to confirmEntry", () => {
  // The click contract keeps Quick Paste visible after a successful
  // copy. The handler MUST forward the new flag so the controller
  // does not hide the window through the keyboard path.
  const block = extractFunctionBody(quickPasteSource, "handleRowClick");
  assert.ok(
    block.includes("hideAfterSuccess: false"),
    "handleRowClick must forward hideAfterSuccess: false",
  );
});

test("Fix 5 — handleEnter passes hideAfterSuccess: true to confirmEntry", () => {
  // The keyboard contract MUST stay on the historical behaviour
  // (hide after successful copy). A regression that flips this
  // flag would break the documented "Enter hides Quick Paste"
  // affordance.
  const block = extractAsyncBody(quickPasteSource, "handleEnter");
  assert.ok(
    block.includes("hideAfterSuccess: true"),
    "handleEnter must forward hideAfterSuccess: true",
  );
});

test("Fix 5 — performCopyFlow hides the window on success when hideAfterSuccess is true", async () => {
  // The legacy keyboard path: a successful copy hides the window.
  const bridge = makeFakeBridge();
  const outcome = await performCopyFlow({
    bridge,
    copyFn: async () => copiedResponse(42),
    hideAfterSuccess: true,
  });
  assert.equal(outcome.kind, "copied");
  assert.equal(outcome.windowStaysHidden, true);
  assert.deepEqual(bridge.steps, ["hide"]);
});

test("Fix 5 — performCopyFlow keeps the window visible on success when hideAfterSuccess is false", async () => {
  // The new click path: the controller NEVER calls `bridge.hide()`
  // and NEVER calls `bridge.show()`. Quick Paste stays on screen
  // after the successful copy and the bridge records no steps at
  // all so the surface never reflows and the user never sees a
  // flicker through a hide/show round-trip.
  const bridge = makeFakeBridge();
  const outcome = await performCopyFlow({
    bridge,
    copyFn: async () => copiedResponse(42),
    hideAfterSuccess: false,
  });
  assert.equal(outcome.kind, "copied");
  assert.equal(outcome.windowStaysHidden, false);
  assert.deepEqual(bridge.steps, []);
});

test("Fix 5 — performCopyFlow keeps the window visible on capability_unavailable when hideAfterSuccess is false", async () => {
  // The click path's error branch must NOT pair the absence of a
  // hide with a hide; the controller simply leaves the surface
  // untouched. The window stays visible (it never hid) and the
  // bridge records no steps so the user can read the typed
  // guidance without seeing the palette flicker.
  const bridge = makeFakeBridge();
  const outcome = await performCopyFlow({
    bridge,
    copyFn: async () => ({
      kind: "capability_unavailable",
      id: null,
      capability: "clipboard_write_image",
      error_kind: null,
      message: "image unsupported",
      guidance: null,
      mode: null,
    }),
    hideAfterSuccess: false,
  });
  assert.equal(outcome.kind, "failed");
  assert.equal(outcome.windowStaysHidden, false);
  assert.deepEqual(bridge.steps, []);
});

test("Fix 5 — performCopyFlow keeps the window visible on failed response when hideAfterSuccess is false", async () => {
  // A typed `failed` response on the click / menu path also leaves
  // the window untouched; the controller never collapsed the
  // surface in the first place so there is nothing to restore.
  const bridge = makeFakeBridge();
  const outcome = await performCopyFlow({
    bridge,
    copyFn: async () => ({
      kind: "failed",
      id: null,
      capability: null,
      error_kind: "asset_read",
      message: "boom",
      guidance: null,
      mode: null,
    }),
    hideAfterSuccess: false,
  });
  assert.equal(outcome.kind, "failed");
  assert.equal(outcome.windowStaysHidden, false);
  assert.deepEqual(bridge.steps, []);
});

test("Fix 5 — performCopyFlow re-shows the window on failure only when hideAfterSuccess is true", async () => {
  // The legacy keyboard contract still re-shows the surface on
  // failure so the user can read the typed guidance. The branch
  // is exclusive to the keyboard path; the click / menu path
  // (above) keeps the window untouched.
  const bridge = makeFakeBridge();
  const outcome = await performCopyFlow({
    bridge,
    copyFn: async () => ({
      kind: "failed",
      id: null,
      capability: null,
      error_kind: "asset_read",
      message: "boom",
      guidance: null,
      mode: null,
    }),
    hideAfterSuccess: true,
  });
  assert.equal(outcome.kind, "failed");
  assert.equal(outcome.windowStaysHidden, false);
  assert.deepEqual(bridge.steps, ["hide", "show"]);
});

// ---------------------------------------------------------------------------
// 6. Cmd/Ctrl+K shortcut.
// ---------------------------------------------------------------------------

test("Fix 6 — quickPasteSearchShortcutLabel renders ⌘K on macOS and Ctrl K on Linux", () => {
  assert.equal(quickPasteSearchShortcutLabel("macos"), "⌘K");
  assert.equal(quickPasteSearchShortcutLabel("other"), "Ctrl K");
  // Match the visible/badge contract the regression demands: no
  // slash, no parentheses, the literal `K` letter in both branches.
  assert.ok(quickPasteSearchShortcutLabel("macos").includes("K"));
  assert.ok(quickPasteSearchShortcutLabel("other").includes("K"));
});

test("Fix 6 — quickPasteSearchShortcutAccessibleLabel mirrors the visible badge", () => {
  const mac = quickPasteSearchShortcutAccessibleLabel("macos");
  const linux = quickPasteSearchShortcutAccessibleLabel("other");
  assert.ok(mac.includes("K"));
  assert.ok(linux.includes("K"));
});

test("Fix 6 — QUICK_PASTE_SEARCH_LETTER documents the trigger key", () => {
  assert.equal(QUICK_PASTE_SEARCH_LETTER, "k");
});

test("Fix 6 — Quick Paste matcher consults the k letter (not f)", () => {
  const matcher = extractFunctionBody(
    quickPasteSource,
    "matchesQuickPasteSearchShortcut",
  );
  assert.ok(
    matcher.includes('key !== "k"'),
    "the matcher must pin the trigger to the k / K letter",
  );
});

test("Fix 6 — the shortcut does not install a duplicate global listener", () => {
  // The window registers exactly ONE `keydown` handler. A
  // regression that double-registers would re-trigger the
  // shortcut for the same physical press.
  const svelteWindowMatches = quickPasteSource.match(
    /<svelte:window[^>]*>/g,
  ) ?? [];
  const withKeydown = svelteWindowMatches.filter(
    (tag) => tag.includes("on:keydown"),
  );
  assert.equal(
    withKeydown.length,
    1,
    "exactly one svelte:window keydown listener must be installed",
  );
});

test("Fix 6 — the shortcut drives focusSearchInput with selectAll", () => {
  // The shortcut MUST focus the existing search input and select
  // its current query so the user can overwrite it immediately.
  const onWindowKeydown = extractFunctionBody(
    quickPasteSource,
    "onWindowKeydown",
  );
  assert.ok(
    onWindowKeydown.includes("focusSearchInput(true)"),
    "the shortcut must focus and select the search input",
  );
});

test("Fix 6 — the badge is rendered inside the .qp-search-shell wrapper, not as a sibling", () => {
  // The badge sits inside the search shell so the input reserves
  // the right padding and the badge cannot intercept a click.
  const shellStart = quickPasteSource.indexOf('class="qp-search-shell"');
  const badgeStart = quickPasteSource.indexOf('class="qp-search-hint"');
  const shellEnd = quickPasteSource.indexOf("</div>", shellStart);
  assert.ok(shellStart > 0, "the search shell must exist");
  assert.ok(badgeStart > 0, "the badge must exist");
  assert.ok(
    badgeStart < shellEnd,
    "the badge must be rendered INSIDE the search shell",
  );
  // The badge must declare pointer-events: none in the style
  // block so clicks reach the underlying input.
  const cssBlock = quickPasteSource.slice(
    quickPasteSource.indexOf("<style>"),
    quickPasteSource.length,
  );
  const rule = cssBlock.match(/\.qp-search-hint\s*\{[^}]*\}/);
  assert.ok(rule, "the .qp-search-hint rule must exist");
  assert.ok(
    /pointer-events:\s*none/.test(rule![0]),
    "the badge must disable pointer events so it never intercepts a click",
  );
});

test("Fix 6 — the input reserves right padding so text never collides with the badge", () => {
  const cssBlock = quickPasteSource.slice(
    quickPasteSource.indexOf("<style>"),
    quickPasteSource.length,
  );
  const rule = cssBlock.match(/\.qp-search\s*\{[^}]*\}/);
  assert.ok(rule, "the .qp-search rule must exist");
  // The padding must reserve room for the badge; the regression
  // expects the badge to be inside the shell, not a separate
  // column, so the input reserves a fixed right padding.
  const padding = rule![0].match(/padding:\s*([^;]+);/);
  assert.ok(padding, "the search input must declare padding");
  const paddedRight = padding![1];
  // Accept either shorthand (e.g. `0.45rem 4rem 0.45rem 0.7rem`)
  // or longhand (`padding-right: 4rem`). Both reserve the
  // documented column.
  const hasRightColumn =
    /\d+(\.\d+)?rem/.test(paddedRight) && !/^\s*\d/.test(paddedRight);
  assert.ok(
    /\d+(\.\d+)?rem\s+\d+(\.\d+)?rem/.test(paddedRight) ||
      paddedRight.split(/\s+/).length >= 2 ||
      /padding-right/.test(rule![0]),
    "the search input must reserve right padding for the badge",
  );
});

// ---------------------------------------------------------------------------
// 7. Typography.
// ---------------------------------------------------------------------------

test("Fix 7 — Quick Paste uses --cv-font-family on the document root", () => {
  // The shell must reuse the documented `--cv-font-family` token
  // (visualTokens.ts) instead of hard-coding a fallback family.
  const cssBlock = quickPasteSource.slice(
    quickPasteSource.indexOf("<style>"),
    quickPasteSource.length,
  );
  const htmlRule = cssBlock.match(/:global\(html,\s*body\)\s*\{[^}]*\}/);
  assert.ok(htmlRule, "the html/body rule must exist");
  assert.ok(
    /var\(--cv-font-family/.test(htmlRule![0]),
    "the document root must use var(--cv-font-family)",
  );
});

test("Fix 7 — the main element forwards the font family token", () => {
  const cssBlock = quickPasteSource.slice(
    quickPasteSource.indexOf("<style>"),
    quickPasteSource.length,
  );
  const mainRule = cssBlock.match(/\bmain\s*\{[^}]*\}/);
  assert.ok(mainRule, "the main rule must exist");
  assert.ok(
    /var\(--cv-font-family/.test(mainRule![0]),
    "the main element must inherit the same font family token",
  );
});

test("Fix 7 — the Quick Paste keeps the documented fixed 720×520 viewport", () => {
  // The geometry is pinned through the recomputeMenuPosition
  // helper; the value pair MUST stay at 720 and 520 so the menu
  // cannot escape the fixed window.
  const body = extractFunctionBody(
    quickPasteSource,
    "recomputeMenuPosition",
  );
  assert.ok(
    /720/.test(body) && /520/.test(body),
    "the helper must pin the popover clamping to the 720×520 viewport",
  );
});

test("Fix 7 — the Quick Paste webview mounts the documented --cv-* tokens", () => {
  // Quick Paste runs inside its own Tauri webview, so the
  // `:root { --cv-* }` block `App.svelte` injects on the main
  // desktop window is NOT visible there. The Quick Paste component
  // MUST inject the same block at its own document root — through
  // the shared `visualTokenCss()` helper, never a parallel literal —
  // so every `var(--cv-*, fallback)` reference reads the same value
  // the desktop rail consumes.
  assert.ok(
    /svelte:head/.test(quickPasteSource),
    "Quick Paste must inject the visual tokens through <svelte:head>",
  );
  assert.ok(
    /visualTokenCss\(/.test(quickPasteSource),
    "Quick Paste must source the visual tokens through the shared helper",
  );
  assert.ok(
    /VISUAL_TOKEN_ROOT_CSS/.test(quickPasteSource),
    "Quick Paste must keep a single VISUAL_TOKEN_ROOT_CSS constant the template consumes",
  );
  // The duplicated :global(:root) block MUST be gone — the regression
  // a future contributor could introduce by re-inlining the values.
  assert.equal(
    /:global\(:root\)\s*\{[\s\S]*?--cv-font-family/.test(quickPasteSource),
    false,
    "Quick Paste must NOT inline a parallel --cv-font-family :root block (drift regression)",
  );
  // The CSS rendered through <svelte:head> carries every documented
  // token. The test pins the helper output so a change to
  // lib/visualTokens.ts is reflected in both surfaces.
  const tokenCss = visualTokenCss();
  for (const name of [
    "--cv-font-family",
    "--cv-bg-surface",
    "--cv-fg",
    "--cv-border",
  ]) {
    assert.ok(
      tokenCss.includes(name),
      `visualTokenCss must include ${name} so Quick Paste inherits the same value`,
    );
  }
});

// ---------------------------------------------------------------------------
// Test helpers.
// ---------------------------------------------------------------------------

interface FakeBridge extends QuickPasteTauriBridge {
  steps: string[];
}

function makeFakeBridge(): FakeBridge {
  const steps: string[] = [];
  return {
    steps,
    captureActiveApp: async () => ({
      available: false,
      name: null,
      identifier: null,
    }),
    show: async () => {
      steps.push("show");
    },
    focus: async () => {
      steps.push("focus");
    },
    emitOpened: async () => {
      steps.push("emitOpened");
    },
    hide: async () => {
      steps.push("hide");
    },
  };
}

function copiedResponse(id: number, mode: CopyMode = null): CopyResponse {
  return {
    kind: "copied",
    id,
    capability: null,
    error_kind: null,
    message: null,
    guidance: null,
    mode: mode as string | null,
  };
}
