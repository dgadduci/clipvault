/**
 * Regression coverage for the `code-language-detection` capability
 * the `code-language-detection` OpenSpec change ships. The
 * companion `codeLanguageDetector.test.ts` suite pins the detection
 * algorithm itself and the security invariants
 * (`renderHighlightedCode` keeps no scripts, event handlers or
 * remote URLs); this file owns the formatting contract the manual
 * macOS test surfaced as the single remaining gap.
 *
 * The manual test confirmed that:
 *
 *   - `code_language` is detected and persisted;
 *   - the `Código · <Lenguaje>` badge appears on cards and in the
 *     preview overlay;
 *   - `highlight.js` produces the expected colorised markup;
 *   - the persisted payload stays unchanged.
 *
 * The single regression the user reported: the highlighted code
 * preview did not keep the original tabs, indentation, empty lines
 * or `\n`/`\r\n` separators the source application produced. The
 * `entryFullPreviewText` helper collapses whitespace runs by
 * design (that contract is documented and the regression the
 * `quick-paste-preview-ui` change pinned asserts it). The fix
 * introduces `entryRawContent` — a parallel helper the highlighted
 * code preview consumes — and pins the visible whitespace with an
 * explicit CSS override on `.cv-preview-code`.
 *
 * The assertions in this file are intentionally representative,
 * not exhaustive: one assertion per behaviour the user described
 * in the manual test, plus the contracts the regression suite
 * already exercises around the helper, the renderer, the CSS
 * anchor and the read-only contract.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

import { renderHighlightedCode } from "../src/lib/codeLanguageDetector.ts";
import {
  entryFullPreviewText,
  entryRawContent,
  isImageEntry,
} from "../src/lib/clipboardAsset.ts";
import type { EntryRecord } from "../src/types.ts";

// ---------------------------------------------------------------------------
// Source snapshots. The component is intentionally inspect-only — no
// Svelte mount — because the test suite already covers the live contract
// through the existing `quickPastePreviewRegressions.test.ts` suite.
// ---------------------------------------------------------------------------

const previewSource = readFileSync(
  resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
  "utf8",
);

const clipboardAssetSource = readFileSync(
  resolvePath(process.cwd(), "src", "lib", "clipboardAsset.ts"),
  "utf8",
);

const clipboardPreviewHelperSource = readFileSync(
  resolvePath(process.cwd(), "src", "lib", "clipboardPreview.ts"),
  "utf8",
);

const codeLanguageDetectorSource = readFileSync(
  resolvePath(process.cwd(), "src", "lib", "codeLanguageDetector.ts"),
  "utf8",
);

// ---------------------------------------------------------------------------
// Entry factory. Mirrors the one the projections / privacy suites
// already expose, with the minimum surface area the formatting
// regression needs.
// ---------------------------------------------------------------------------

const VALID_ASSET_REF = `clipboard/${"a".repeat(64)}.png`;

function textEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 1,
    content: "captured note",
    content_type: "text",
    content_size: 13,
    content_hash: "h".repeat(64),
    created_at: "2026-09-01T10:11:12Z",
    updated_at: "2026-09-01T10:11:12Z",
    last_seen_at: "2026-09-01T10:11:12Z",
    is_pinned: false,
    source_app: "com.example.Editor",
    source_app_name: "Editor",
    source_app_icon_ref: null,
    title: "Captured note",
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
    code_language: null,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// 1. `entryRawContent` preserves the raw payload byte-for-byte.
// ---------------------------------------------------------------------------

test("entryRawContent preserves tabs and newlines verbatim", () => {
  // The highlighted code preview feeds `entryRawContent` (NOT
  // `entryFullPreviewText`) to the renderer, so the existing collapse
  // contract stays in place for the plain-text fallback while the
  // code preview keeps the original whitespace.
  const entry = textEntry({
    content:
      "function add(a, b) {\n\treturn a + b;\n}\n\nconst sum = add(2, 3);\nconsole.log(sum);\n",
  });
  const raw = entryRawContent(entry);
  assert.ok(raw.includes("\t"), "raw content must keep tab characters");
  assert.ok(
    raw.includes("\n\n"),
    "raw content must keep consecutive newlines (empty lines)",
  );
  assert.equal(raw.includes("  "), false, "the raw content has no consecutive spaces");
});

test("entryRawContent preserves CRLF and LF separators", () => {
  const entry = textEntry({
    content: "line1\r\nline2\nline3\r\n",
  });
  const raw = entryRawContent(entry);
  assert.ok(raw.includes("\r\n"), "raw content must keep CRLF separators");
  assert.ok(raw.includes("\n"), "raw content must keep LF separators");
});

test("entryRawContent preserves consecutive tabs", () => {
  const entry = textEntry({ content: "before\t\t\tafter" });
  const raw = entryRawContent(entry);
  assert.ok(raw.includes("\t\t\t"), "three consecutive tabs must survive");
});

test("entryRawContent preserves Python four-space indentation verbatim", () => {
  const entry = textEntry({
    content:
      "def greet(name):\n    message = f'Hello {name}'\n    print(message)\n\ngreet('world')\n",
  });
  const raw = entryRawContent(entry);
  assert.ok(
    raw.includes("\n    "),
    "raw content must keep four-space indentation intact",
  );
  assert.ok(
    raw.includes("\n\ngreet"),
    "raw content must keep the empty line before the call",
  );
});

test("entryRawContent returns empty string for an image row", () => {
  // Mirrors `entryFullPreviewText`: image rows expose the empty
  // sentinel `content` and must never pollute the highlighted
  // preview. The overlay branches on `isImageEntry` first.
  const entry = textEntry({
    content: "",
    content_type: "image",
    asset_ref: VALID_ASSET_REF,
    mime_type: "image/png",
    payload_width: 1,
    payload_height: 1,
  });
  assert.equal(isImageEntry(entry), true);
  assert.equal(entryRawContent(entry), "");
});

test("entryRawContent and entryFullPreviewText share the whitespace-preserving contract", () => {
  // Both helpers preserve the bytes the source application produced
  // so the highlighted code preview and the plain-text fallback
  // render the captured block identically. The previous contract
  // collapsed whitespace in `entryFullPreviewText`; the
  // `preview-interaction-regressions` change makes the plain-text
  // fallback preserve whitespace byte-for-byte too.
  const entry = textEntry({
    content: "a\n\nb\t\tc    d",
  });
  const raw = entryRawContent(entry);
  assert.equal(raw.includes("\n"), true);
  assert.equal(raw.includes("\t"), true);
  assert.equal(raw.includes("  "), true);
  // The plain-text preview helper preserves the same whitespace
  // characters so the `<pre>` mounted with `white-space: pre-wrap`
  // can render them.
  const preview = entryFullPreviewText(entry);
  assert.equal(preview.includes("\n"), true);
  assert.equal(preview.includes("\t"), true);
  assert.equal(preview.includes("  "), true);
});

// ---------------------------------------------------------------------------
// 2. `renderHighlightedCode` keeps tabs and newlines in the produced HTML.
// ---------------------------------------------------------------------------

test("renderHighlightedCode preserves tabs and newlines in the HTML output", () => {
  // The renderer is the single switch the preview overlay mounts
  // through `{@html highlightedHtml}`. highlight.js wraps tokens
  // in `<span>` tags but preserves the surrounding whitespace as
  // text nodes; the regression suite asserts the helper does not
  // introduce a `replace` call that strips them.
  const source =
    "function add(a, b) {\n\treturn a + b;\n}\n\nconst sum = add(2, 3);\nconsole.log(sum);\n";
  const result = renderHighlightedCode(source, "javascript");
  assert.equal(result.language, "javascript");
  assert.ok(result.html.includes("\n"), "the rendered HTML must keep \\n");
  assert.ok(result.html.includes("\t"), "the rendered HTML must keep \\t");
});

test("renderHighlightedCode HTML keeps the same number of lines as the input", () => {
  // The regression the user reported visually: line counts in the
  // preview must match the source. The DOM's `textContent` mirror
  // (with the highlighted tags stripped) is a deterministic way to
  // verify the renderer does not collapse consecutive newlines or
  // strip empty lines.
  const source = [
    "function add(a, b) {",
    "  return a + b;",
    "}",
    "",
    "const sum = add(2, 3);",
    "console.log(sum);",
  ].join("\n");
  const sourceLines = source.split("\n").length;
  const result = renderHighlightedCode(source, "javascript");
  // Strip every tag — the renderer is allowed to wrap tokens in
  // `<span class="hljs-…">`, but the text content mirror (the
  // visible payload once the browser parses the HTML) must keep
  // the original line breaks.
  const visible = result.html
    .replace(/<[^>]+>/g, "")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'");
  const visibleLines = visible.split("\n").length;
  assert.equal(
    visibleLines,
    sourceLines,
    `renderHighlightedCode must keep ${sourceLines} lines, got ${visibleLines}`,
  );
});

test("renderHighlightedCode preserves Python four-space indentation in the output", () => {
  const source =
    "def greet(name):\n    message = f'Hello {name}'\n    print(message)\n\ngreet('world')\n";
  const result = renderHighlightedCode(source, "python");
  assert.equal(result.language, "python");
  // The strip-every-tag mirror must keep the four-space prefix so
  // the `<pre>` renders the block exactly as the user pasted it.
  const visible = result.html
    .replace(/<[^>]+>/g, "")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&");
  assert.ok(
    visible.includes("\n    "),
    "the rendered markup must keep four-space Python indentation",
  );
});

test("renderHighlightedCode preserves nested Rust indentation in the output", () => {
  const source = [
    "fn main() {",
    "    let items = vec![1, 2, 3];",
    "    for item in &items {",
    "        if *item > 1 {",
    "            println!(\"{}\", item);",
    "        }",
    "    }",
    "}",
  ].join("\n");
  const result = renderHighlightedCode(source, "rust");
  assert.equal(result.language, "rust");
  const visible = result.html
    .replace(/<[^>]+>/g, "")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, '"');
  for (const indent of ["\n    ", "\n        "]) {
    assert.ok(
      visible.includes(indent),
      `the rendered markup must keep the ${JSON.stringify(indent)} indent level`,
    );
  }
});

test("renderHighlightedCode keeps CRLF separators unchanged in the output", () => {
  const source = "line1\r\nline2\r\nline3\r\n";
  const result = renderHighlightedCode(source, "javascript");
  assert.equal(result.language, "javascript");
  const visible = result.html
    .replace(/<[^>]+>/g, "")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&");
  assert.equal(
    visible.includes("\r\n"),
    true,
    "the rendered markup must keep CRLF separators",
  );
});

test("renderHighlightedCode preserves consecutive tab characters in the output", () => {
  const source = "before\t\t\tafter";
  const result = renderHighlightedCode(source, "javascript");
  assert.equal(result.language, "javascript");
  const visible = result.html.replace(/<[^>]+>/g, "");
  assert.ok(
    visible.includes("\t\t\t"),
    "the rendered markup must keep three consecutive tab characters",
  );
});

test("renderHighlightedCode keeps sanitisation active while preserving whitespace", () => {
  // The renderer MUST still strip scripts, event handlers and
  // remote URLs even when the input contains whitespace. The
  // regression suite asserts both invariants in the same call so
  // a future refactor cannot trade one off for the other.
  const source =
    "if (true) {\n\treturn 1;\n}\n<script>alert('x')</script>\n";
  const result = renderHighlightedCode(source, "javascript");
  assert.equal(result.language, "javascript");
  assert.ok(result.html.includes("\n"));
  assert.ok(result.html.includes("\t"));
  assert.equal(
    /<script/i.test(result.html),
    false,
    "the rendered HTML must remain free of <script> tags",
  );
});

test("renderHighlightedCode keeps the canonical language in its response", () => {
  // The overlay reads `result.language` to populate the badge and
  // the `data-code-language` attribute. The renderer must keep
  // returning the canonical identifier (alias normalised) so the
  // surfaced label stays in lockstep with the persisted metadata.
  const source =
    "function f() {\n\treturn 'hi';\n}\n\nconst g = f();\nconsole.log(g);\n";
  const result = renderHighlightedCode(source, "js");
  assert.equal(result.language, "javascript");
});

// ---------------------------------------------------------------------------
// 3. ClipboardPreview mounts the highlighted path through entryRawContent.
// ---------------------------------------------------------------------------

test("ClipboardPreview feeds entryRawContent to renderHighlightedCode", () => {
  // The component must use the raw-content helper for the
  // highlighted path. A regression that re-uses the legacy
  // `entryFullPreviewText` (which collapses whitespace) would
  // break the manual test and surface here.
  assert.ok(
    previewSource.includes("entryRawContent"),
    "ClipboardPreview must import entryRawContent",
  );
  assert.ok(
    previewSource.includes("renderHighlightedCode(rawText"),
    "ClipboardPreview must call renderHighlightedCode with the raw text",
  );
  assert.equal(
    previewSource.includes("renderHighlightedCode(fullText"),
    false,
    "ClipboardPreview must NOT call renderHighlightedCode with the collapsed fullText",
  );
});

test("ClipboardPreview keeps code_language and highlighting data attributes", () => {
  // The badge and the highlighted container both depend on the
  // canonical `data-code-language` attribute; a regression that
  // drops it would break every CSS selector and every test that
  // asserts on the rendered overlay.
  assert.ok(
    previewSource.includes("data-code-language={"),
    "the highlighted preview must declare a data-code-language attribute",
  );
  assert.ok(
    previewSource.includes("cv-preview-code"),
    "the highlighted preview must keep the cv-preview-code class",
  );
  assert.ok(
    previewSource.includes("Código · {codeLanguageLabel}"),
    "the highlighted preview must keep the 'Código · <Lenguaje>' badge markup",
  );
});

test("ClipboardPreview keeps the read-only contract (no paste / copy / pin / edit)", () => {
  // The overlay is the read-only surface the manual test
  // confirmed; a future regression that re-introduces a write
  // path would invalidate the change. The assertion mirrors the
  // privacy / mutations contract so the manual test stays
  // documentable as a verified fixture.
  const forbidden = [
    "copyEntryCommand",
    "pasteEntryCommand",
    "runCopyForEntry",
    "runMenuPasteForEntry",
    "runMenuCopyForEntry",
    "setFavoriteCommand",
    "setEntryTitleCommand",
    "deleteEntryCommand",
    "captureTextCommand",
  ];
  for (const token of forbidden) {
    assert.equal(
      previewSource.includes(token),
      false,
      `ClipboardPreview must not reference ${token} (still read-only)`,
    );
  }
});

test("ClipboardPreview pins the code preview to a readonly surface", () => {
  // The highlighted path mounts inside a `<pre>` element the
  // renderer fills through `{@html highlightedHtml}`. A future
  // regression that switches to `<div>` or to a contenteditable
  // element would break the documented contract.
  assert.ok(
    /<pre\s+class="cv-preview-text cv-preview-code"/.test(previewSource),
    "the highlighted preview must mount inside <pre class=\"cv-preview-text cv-preview-code\">",
  );
  assert.ok(
    previewSource.includes("{@html highlightedHtml}"),
    "the highlighted preview must mount through {@html highlightedHtml}",
  );
});

// ---------------------------------------------------------------------------
// 4. CSS pins the formatting the user reported missing.
// ---------------------------------------------------------------------------

test("ClipboardPreview pins white-space and tab-size on .cv-preview-code", () => {
  // The CSS rule MUST explicitly set `white-space: pre-wrap` and
  // `tab-size: 4` on `.cv-preview-code`. A regression that drops
  // them would let a global stylesheet demote the preview to
  // `white-space: normal` and the tabs would collapse — the exact
  // bug the user reported.
  const cssBlock = previewSource.slice(
    previewSource.indexOf("<style>"),
    previewSource.length,
  );
  const rule = cssBlock.match(/\.cv-preview-code\s*\{[^}]*\}/);
  assert.ok(rule, "the .cv-preview-code CSS rule must exist");
  assert.ok(
    /white-space:\s*pre-wrap/.test(rule![0]) ||
      /white-space:\s*pre\b/.test(rule![0]),
    "the .cv-preview-code rule must pin white-space: pre-wrap or pre",
  );
  assert.ok(
    /tab-size:\s*4/.test(rule![0]),
    "the .cv-preview-code rule must pin tab-size: 4",
  );
  assert.ok(
    /-moz-tab-size:\s*4/.test(rule![0]),
    "the .cv-preview-code rule must pin -moz-tab-size: 4 for Firefox",
  );
});

test("ClipboardPreview keeps the highlighted preview scrollable inside the body", () => {
  // A long capture MUST scroll inside the overlay body, not grow
  // the desktop window. The shared rule pins `overflow: auto` on
  // `.cv-preview-body`; the highlighted branch adds
  // `overflow-x: auto` and `max-width: 100%` so horizontal
  // overflow never escapes.
  const cssBlock = previewSource.slice(
    previewSource.indexOf("<style>"),
    previewSource.length,
  );
  const bodyRule = cssBlock.match(/\.cv-preview-body\s*\{[^}]*\}/);
  assert.ok(bodyRule, "the .cv-preview-body rule must exist");
  assert.ok(
    /overflow:\s*auto/.test(bodyRule![0]),
    "the preview body must keep overflow: auto so the overlay scrolls internally",
  );
  const codeRule = cssBlock.match(/\.cv-preview-code\s*\{[^}]*\}/);
  assert.ok(codeRule, "the .cv-preview-code rule must exist");
  assert.ok(
    /overflow-x:\s*auto/.test(codeRule![0]),
    "the .cv-preview-code rule must allow horizontal scroll for long lines",
  );
  assert.ok(
    /max-width:\s*100%/.test(codeRule![0]),
    "the .cv-preview-code rule must cap max-width at 100% so it never escapes the parent",
  );
});

// ---------------------------------------------------------------------------
// 5. The persisted payload is never rewritten by the renderer.
// ---------------------------------------------------------------------------

test("renderHighlightedCode never mutates the input string", () => {
  // The renderer is read-only: it returns the rendered markup
  // alongside the canonical language and MUST never mutate the
  // caller's string. The regression suite asserts the input is
  // byte-identical after the call so a future regression that
  // tries to share an internal buffer cannot leak back into the
  // persisted payload through the persistence layer.
  const source =
    "function f() {\n\treturn 'hi';\n}\n\nconst g = f();\nconsole.log(g);\n";
  const snapshot = source.slice(0);
  renderHighlightedCode(source, "javascript");
  assert.equal(source, snapshot, "renderHighlightedCode must not mutate its input");
});

test("entryRawContent returns the canonical entry.content reference (no trim, no copy)", () => {
  // The helper is intentionally a thin accessor. A regression
  // that introduced a `slice`/`trim`/`replace` step would
  // duplicate the original payload and risk drift with the
  // persisted rows. The assertion inspects the helper source so
  // the contract travels with the change.
  const match = clipboardAssetSource.match(
    /export function entryRawContent[\s\S]+?\n\}/,
  );
  assert.ok(match, "entryRawContent must be declared in clipboardAsset.ts");
  const body = match![0];
  assert.equal(
    /trim\(/.test(body),
    false,
    "entryRawContent must never trim its input",
  );
  assert.equal(
    /\.replace\(/.test(body),
    false,
    "entryRawContent must never mutate its input through replace",
  );
  assert.equal(
    /\.slice\(/.test(body),
    false,
    "entryRawContent must never slice its input (the full string must reach the renderer)",
  );
});

test("entryFullPreviewText preserves whitespace byte-for-byte (no collapse, no trim)", () => {
  // The plain-text preview helper preserves the whitespace the
  // source application produced: it never collapses whitespace
  // runs and never trims the string. A regression that re-introduces
  // the legacy collapse (or that re-uses `entryPreviewText`'s
  // `replace(/\s+/g, " ").trim()` chain) would surface here so the
  // Desktop and Quick Paste overlays stop dropping tabs and empty
  // lines.
  const match = clipboardAssetSource.match(
    /export function entryFullPreviewText[\s\S]+?\n\}/,
  );
  assert.ok(match, "entryFullPreviewText must be declared in clipboardAsset.ts");
  const body = match![0];
  assert.equal(
    /trim\(/.test(body),
    false,
    "entryFullPreviewText must never trim its input",
  );
  assert.equal(
    /\.replace\(.\\s\+\/g/.test(body) ||
      /replace\(\/\\\\s\+\/g/.test(body),
    false,
    "entryFullPreviewText must never collapse whitespace runs",
  );
});

// ---------------------------------------------------------------------------
// 6. Helpers, re-exports and imports stay wired.
// ---------------------------------------------------------------------------

test("lib/clipboardPreview.ts re-exports entryRawContent for shared consumers", () => {
  // The Desktop rail and Quick Paste both consume the helper
  // through the shared `clipboardPreview` module. The re-export
  // keeps a single import surface so a regression that bypasses
  // the helper (or duplicates its collapse logic in another
  // component) surfaces here.
  assert.ok(
    /export\s*\{[^}]*entryRawContent[^}]*\}/.test(clipboardPreviewHelperSource),
    "clipboardPreview.ts must re-export entryRawContent alongside entryFullPreviewText",
  );
});

test("clipboardAsset.ts exports entryRawContent and entryFullPreviewText", () => {
  assert.ok(
    /export function entryRawContent/.test(clipboardAssetSource),
    "clipboardAsset.ts must export entryRawContent",
  );
  assert.ok(
    /export function entryFullPreviewText/.test(clipboardAssetSource),
    "clipboardAsset.ts must keep exporting entryFullPreviewText",
  );
});

test("codeLanguageDetector keeps the renderer and detector contract intact", () => {
  // The renderer MUST keep returning an `{ html, language }`
  // shape and MUST call `hljs.highlight` (not `highlightAuto`) so
  // the canonical `code_language` the user pinned overrides the
  // automatic guess.
  assert.ok(
    /export function renderHighlightedCode\(/.test(codeLanguageDetectorSource),
    "renderHighlightedCode must remain a public export",
  );
  assert.ok(
    /hljs\.highlight\(/.test(codeLanguageDetectorSource),
    "renderHighlightedCode must call hljs.highlight to honour the canonical language",
  );
  assert.ok(
    /sanitiseHighlightedHTML\(/.test(codeLanguageDetectorSource),
    "the renderer must still sanitise the highlight.js output",
  );
});
