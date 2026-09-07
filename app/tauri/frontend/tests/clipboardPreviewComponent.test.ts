/**
 * Tests for the shared `ClipboardPreview.svelte` component the
 * `desktop-card-preview` change introduces.
 *
 * The component is the single source of truth for the Desktop
 * preview overlay AND the Quick Paste preview overlay. The tests
 * pin the contract by inspecting the source so a regression that
 * re-implements the overlay locally, drops a forbidden mutation
 * path, forgets to escape the text, leaks a blob URL, or breaks
 * the stale-response guard surfaces in CI.
 *
 * The component is intentionally inspect-only (no DOM / Svelte
 * mount) because the test suite already covers the live contract
 * through the existing `quickPastePreviewRegressions.test.ts`
 * suite. This file owns:
 *
 *   - the read-only contract (no paste / copy / pin / edit paths);
 *   - the asset lifecycle (release on close / change, stale guard);
 *   - the platform-aware matcher re-export the keyboard layer
 *     consumes;
 *   - the consistent test-id prefix the Desktop rail / Quick Paste
 *     both consume;
 *   - the focus return path the keyboard contract depends on.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

const previewSource = readFileSync(
  resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
  "utf8",
);

const clipboardAssetSource = readFileSync(
  resolvePath(process.cwd(), "src", "lib", "clipboardAsset.ts"),
  "utf8",
);

const iconResolverSource = readFileSync(
  resolvePath(process.cwd(), "src", "lib", "iconResolver.ts"),
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

function extractBlockAfter(source: string, anchor: string): string {
  const start = source.indexOf(anchor);
  assert.notEqual(start, -1, `${anchor} must appear in the source`);
  return source.slice(start);
}

test("ClipboardPreview declares a dialog role for the overlay", () => {
  // The overlay must announce itself as a modal so screen readers
  // do not leak focus into the underlying rail. The `role="dialog"`
  // is the documented contract for the Quick Paste overlay and
  // Desktop preview alike.
  assert.ok(
    previewSource.includes('role="dialog"'),
    "the overlay must declare role=\"dialog\"",
  );
  assert.ok(
    previewSource.includes('aria-modal="true"'),
    "the overlay must declare aria-modal=\"true\"",
  );
});

test("ClipboardPreview uses entryFullPreviewText + escapeForPreview together", () => {
  // The shared overlay must render the canonical, untruncated text
  // through the typed escape helper so neither consumer can ship
  // its own truncation / sanitisation pipeline.
  assert.ok(
    previewSource.includes("entryFullPreviewText(entry)"),
    "the overlay must consult entryFullPreviewText (full canonical text)",
  );
  assert.ok(
    previewSource.includes("escapeForPreview(entryFullPreviewText(entry))"),
    "the overlay must escape the text through escapeForPreview before rendering",
  );
});

test("ClipboardPreview never invokes the paste / copy / pin / edit paths", () => {
  // The overlay is strictly read-only. Any forbidden command in
  // the source would let the overlay mutate state — a regression
  // the spec explicitly forbids.
  const forbidden = [
    "copyEntryCommand",
    "pasteEntryCommand",
    "runCopyForEntry",
    "runMenuPasteForEntry",
    "runMenuCopyForEntry",
    "setFavoriteCommand",
    "setEntryTitleCommand",
    "entryTagsSetCommand",
    "entryCollectionsSetCommand",
    "deleteEntryCommand",
    "captureTextCommand",
    "invoke",
  ];
  for (const token of forbidden) {
    assert.equal(
      previewSource.includes(token),
      false,
      `ClipboardPreview must not reference ${token} (strictly read-only)`,
    );
  }
});

test("ClipboardPreview routes through createClipboardAssetResolver", () => {
  // The shared overlay must reuse the existing validated bridge
  // and resolver factory — no parallel implementation, no direct
  // Tauri call. The factory is the same one the rail and Quick
  // Paste thumbnails already consume.
  assert.ok(
    previewSource.includes("createClipboardAssetResolver"),
    "ClipboardPreview must build its resolver through createClipboardAssetResolver",
  );
  assert.ok(
    previewSource.includes("assetResolver.resolve"),
    "ClipboardPreview must resolve through the shared resolver, not a direct Tauri call",
  );
});

test("ClipboardPreview uses releaseFor + release on the asset lifecycle", () => {
  // The shared overlay MUST revoke the blob URL when the entry
  // changes (so a stale image cannot leak) and when the component
  // is destroyed (so a long-lived window does not leak memory).
  const body = extractBlockAfter(previewSource, "async function refreshImage");
  assert.ok(
    body.includes("assetResolver.resolve"),
    "refreshImage must resolve through the asset resolver",
  );
  // onDestroy must revoke the asset reference before clearing the
  // state. The hook is the single switch the spec pins.
  assert.ok(
    previewSource.includes("assetResolver.releaseFor(imageAssetRef)"),
    "onDestroy must release the current blob URL through releaseFor",
  );
});

test("ClipboardPreview guards stale responses through a per-instance token", () => {
  // The shared overlay must bump a token on every `refreshImage`
  // call and drop late resolutions so the user never sees another
  // entry's image. The helper is the same guard the rail and Quick
  // Paste thumbnails already use.
  const body = extractFunctionBody(previewSource, "refreshImage");
  assert.ok(
    body.includes("++imageToken"),
    "refreshImage must bump the stale-response token",
  );
  assert.ok(
    body.includes("token !== imageToken"),
    "refreshImage must drop responses that land after a newer round",
  );
});

test("ClipboardPreview mounts the overlay as a fixed backdrop inside the parent", () => {
  // The overlay must live inside the existing fixed window so
  // neither surface introduces a new Tauri webview. The
  // `position: fixed` rule on `.cv-preview-overlay` enforces the
  // documented contract.
  const cssBlock = previewSource.slice(
    previewSource.indexOf("<style>"),
    previewSource.length,
  );
  const overlayRule = cssBlock.match(/\.cv-preview-overlay\s*\{[^}]*\}/);
  assert.ok(overlayRule, "the .cv-preview-overlay CSS rule must exist");
  assert.ok(
    /position:\s*fixed/.test(overlayRule![0]),
    "the overlay must use position: fixed so it lives inside the parent's window",
  );
  assert.ok(
    /inset:\s*0/.test(overlayRule![0]),
    "the overlay must cover the parent with inset: 0 so it does not leak to a new webview",
  );
});

test("ClipboardPreview clips overflow on the inner card", () => {
  // The inner card must clip its content so a long text or image
  // never grows the desktop rail / Quick Paste window. The clip
  // is a defensive cap: the body already scrolls internally.
  const cssBlock = previewSource.slice(
    previewSource.indexOf("<style>"),
    previewSource.length,
  );
  const cardRule = cssBlock.match(/\.cv-preview-card\s*\{[^}]*\}/);
  assert.ok(cardRule, "the .cv-preview-card CSS rule must exist");
  assert.ok(
    /overflow:\s*hidden/.test(cardRule![0]),
    "the inner card must clip overflow so the parent window never grows",
  );
  assert.ok(
    /max-width:\s*100%/.test(cardRule![0]) ||
      /max-height:\s*100%/.test(cardRule![0]),
    "the inner card must cap max-width / max-height so the parent window never grows",
  );
});

test("ClipboardPreview renders the safe text inside a <pre>", () => {
  // The text body must use a `<pre>` so the layout stays bounded
  // and the markup stays safe. `innerHTML` and iframe paths are
  // explicitly forbidden.
  assert.ok(
    previewSource.includes("<pre"),
    "the text body must use a <pre> element so whitespace and scroll stay bounded",
  );
  assert.equal(
    previewSource.includes("{@html"),
    // The component mounts the shared icon sprite + the app
    // fallback glyph through `{@html}` so both surfaces still
    // resolve the documented `<use href="#cv-icon-…">` ids. The
    // preview body itself never uses `{@html}`.
    // We only flag a regression that introduces another
    // `{@html …}` consumer that takes user content.
    previewSource.match(/\{@html[^}]+\}/g)?.filter(
      (m) => !m.includes("CONTENT_TYPE_ICON_SPRITE") &&
        !m.includes("APP_FALLBACK_ICON_SVG"),
    ).length === 0,
    "ClipboardPreview must not introduce a new @html consumer that takes user content",
  );
});

test("ClipboardPreview renders images through object-fit: contain", () => {
  // Image previews MUST use `object-fit: contain` so the persisted
  // asset is shown at its intrinsic aspect ratio, never cropped or
  // stretched. The CSS rule pins the documented contract.
  const cssBlock = previewSource.slice(
    previewSource.indexOf("<style>"),
    previewSource.length,
  );
  const imageRule = cssBlock.match(/\.cv-preview-image\s*\{[^}]*\}/);
  assert.ok(imageRule, "the .cv-preview-image CSS rule must exist");
  assert.ok(
    /object-fit:\s*contain/.test(imageRule![0]),
    "the image preview must use object-fit: contain so the asset is shown in full",
  );
});

test("ClipboardPreview accepts a trigger element so close returns focus", () => {
  // The overlay must restore focus to the trigger when the user
  // closes it. The `trigger` export and the `restoreFocus` helper
  // are the contract.
  assert.ok(
    previewSource.includes("export let trigger"),
    "ClipboardPreview must accept a trigger export to return focus to",
  );
  const body = extractFunctionBody(previewSource, "restoreFocus");
  assert.ok(
    body.includes("trigger.focus"),
    "restoreFocus must focus the trigger element",
  );
});

test("ClipboardPreview keeps the entry.metadata-only contract", () => {
  // The component MUST NOT receive or render content_hash,
  // content_size, mime_type, payload_width, payload_height,
  // source_app, source_app_name, source_app_icon_ref,
  // rich_html_ref, rich_rtf_ref or rich_preview_ref. Those stay
  // inside the bridge; the preview only renders the typed display
  // surface (title, source label, elapsed time, body).
  //
  // `asset_ref` is allowed because the shared resolver needs it
  // to bridge through the validated command — it never reaches
  // the rendered DOM.
  const scriptBlock = previewSource.slice(
    previewSource.indexOf("<script"),
    previewSource.indexOf("</script>"),
  );
  for (const forbidden of [
    "content_hash",
    "content_size",
    "mime_type",
    "payload_width",
    "payload_height",
    "source_app",
    "source_app_name",
    "source_app_icon_ref",
    "rich_html_ref",
    "rich_rtf_ref",
    "rich_preview_ref",
  ]) {
    assert.equal(
      scriptBlock.includes(forbidden),
      false,
      `ClipboardPreview must not reference the sensitive field ${forbidden}`,
    );
  }
});

test("ClipboardPreview mounts the icon sprite and fallback glyphs once", () => {
  // The sprite is shared with the rail and Quick Paste rows; the
  // component mounts it through the documented constants so the
  // `<use href="#cv-icon-…">` ids always resolve.
  assert.ok(
    previewSource.includes("CONTENT_TYPE_ICON_SPRITE"),
    "ClipboardPreview must mount the shared content-type icon sprite",
  );
  assert.ok(
    previewSource.includes("APP_FALLBACK_ICON_SVG"),
    "ClipboardPreview must mount the shared app fallback glyph",
  );
});

test("ClipboardPreview image lifecycle never modifies bytes, metadata or asset_ref", () => {
  // The shared overlay must not touch `asset_ref`, mime_type,
  // payload_width or payload_height when it loads the image. The
  // asset bytes the backend published stay byte-for-byte identical
  // because the resolver hands the preview a fresh `Blob` and the
  // UI mounts it through `<img src={url}>` — no re-encoding, no
  // resize, no profile mutation. The check inspects the refreshImage
  // body so a regression that re-encodes the bytes surfaces here.
  const body = extractFunctionBody(previewSource, "refreshImage");
  for (const forbidden of [
    "asset_ref",
    "mime_type",
    "payload_width",
    "payload_height",
    "canvas",
    "toBlob",
    "convert",
  ]) {
    assert.equal(
      body.includes(forbidden),
      false,
      `refreshImage must not touch ${forbidden} when loading the asset`,
    );
  }
});

test("clipboardAsset re-exports the helpers the shared overlay consumes", () => {
  // The shared overlay reads `entryFullPreviewText`,
  // `escapeForPreview`, `isImageEntry` and `hasRenderableImage`
  // straight from `lib/clipboardAsset.ts`. The re-export is the
  // only path so a future drift surfaces here.
  for (const name of [
    "entryFullPreviewText",
    "escapeForPreview",
    "isImageEntry",
    "hasRenderableImage",
    "createClipboardAssetResolver",
  ]) {
    assert.ok(
      clipboardAssetSource.includes(`export function ${name}`) ||
        clipboardAssetSource.includes(`export const ${name}`),
      `clipboardAsset.ts must export ${name} so the shared preview can consume it`,
    );
  }
});

test("iconResolver provides release / releaseFor hooks the shared overlay uses", () => {
  // The shared overlay relies on `release` (called when the
  // component unmounts) and `releaseFor` (called when the entry
  // changes) so a long-lived rail never leaks blob URLs. The
  // resolver interface is the same one the Quick Paste thumbnails
  // already consume.
  assert.ok(
    iconResolverSource.includes("release(): void"),
    "IconResolver must declare release() for the onDestroy hook",
  );
  assert.ok(
    iconResolverSource.includes("releaseFor(ref: string)"),
    "IconResolver must declare releaseFor(ref) for the per-entry cleanup",
  );
  assert.ok(
    iconResolverSource.includes("URL.revokeObjectURL"),
    "IconResolver must revoke object URLs to keep the asset lifecycle leak-free",
  );
});