/**
 * Typography unification regression.
 *
 * The `quick-paste-preview-ui` change mandates that the Quick Paste
 * palette and the desktop rail share the same UI typography system.
 * The source of truth lives in `lib/visualTokens.ts`; every other
 * component reads the values through `var(--cv-*, fallback)` so a
 * future theme change updates both surfaces in lock-step.
 *
 * The suite pins the contract by:
 *
 * 1. Asserting the desktop (`App.svelte`) and Quick Paste
 *    (`QuickPaste.svelte`) both source their tokens from the
 *    shared `visualTokenCss()` helper.
 * 2. Asserting neither surface inlines a parallel `--cv-*` block
 *    (a regression that re-declares the values drifts the
 *    typography between webviews).
 * 3. Pinning the family and the documented sizes / weights so a
 *    future contributor cannot re-introduce a second scale.
 * 4. Verifying the `font-family` references inside Quick Paste
 *    point at `var(--cv-font-family, …)` instead of a literal
 *    family — the only way the palette can stay in lock-step
 *    with the desktop.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

import { visualTokenCss, VISUAL_TOKENS } from "../src/lib/visualTokens.ts";

const appSource = readFileSync(
  resolvePath(process.cwd(), "src", "App.svelte"),
  "utf8",
);
const quickPasteSource = readFileSync(
  resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
  "utf8",
);

test("the desktop injects the visual tokens through <svelte:head> using the shared helper", () => {
  // `App.svelte` is the desktop webview. It MUST mount the
  // documented token block at the document root through the shared
  // `visualTokenCss()` helper so modals, toolbars and the rail all
  // read the same values.
  assert.ok(
    /svelte:head/.test(appSource),
    "App.svelte must inject the visual tokens through <svelte:head>",
  );
  assert.ok(
    /visualTokenCss\(/.test(appSource),
    "App.svelte must source the visual tokens through the shared helper",
  );
});

test("Quick Paste injects the visual tokens through <svelte:head> using the shared helper", () => {
  // Quick Paste runs inside its own Tauri webview so the `:root`
  // block on the desktop window is NOT visible there. The component
  // MUST mount the same block at its own document root through the
  // shared helper.
  assert.ok(
    /svelte:head/.test(quickPasteSource),
    "QuickPaste.svelte must inject the visual tokens through <svelte:head>",
  );
  assert.ok(
    /visualTokenCss\(/.test(quickPasteSource),
    "QuickPaste.svelte must source the visual tokens through the shared helper",
  );
});

test("neither surface inlines a parallel --cv-* root block", () => {
  // The visual tokens live in `lib/visualTokens.ts` only. The two
  // surfaces inject the helper's output; inlining a second block
  // (a `:global(:root) { --cv-* … }` rule) is the documented drift
  // regression.
  assert.equal(
    /:global\(:root\)\s*\{[\s\S]*?--cv-font-family/.test(appSource),
    false,
    "App.svelte must NOT inline a parallel :global(:root) --cv-* block",
  );
  assert.equal(
    /:global\(:root\)\s*\{[\s\S]*?--cv-font-family/.test(quickPasteSource),
    false,
    "QuickPaste.svelte must NOT inline a parallel :global(:root) --cv-* block",
  );
});

test("the documented --cv-* tokens are emitted by the shared helper", () => {
  // The regression catches a contributor that drops one of the
  // documented tokens from `visualTokenCss()`: the desktop rail
  // and the Quick Paste palette would render with the literal
  // fallback colour or size.
  const css = visualTokenCss();
  for (const name of [
    "--cv-font-family",
    "--cv-body",
    "--cv-muted",
    "--cv-title",
    "--cv-title-md",
    "--cv-title-sm",
    "--cv-control",
    "--cv-tag",
    "--cv-preview",
    "--cv-bg-surface",
    "--cv-fg",
    "--cv-border",
    "--cv-card-size",
    "--cv-card-rail-height",
  ]) {
    assert.ok(
      css.includes(name),
      `visualTokenCss() must include ${name} so Quick Paste inherits the same value`,
    );
  }
});

test("the desktop references the --cv-font-family token, never a literal UI family", () => {
  // A regression that hard-codes the UI font family inside `App.svelte`
  // would let the desktop drift from the visual system. The desktop
  // MUST read the value through `var(--cv-font-family, fallback)`
  // or `inherit` (CSS inheritance). The only allowed deviation is
  // the documented monospace family the preview text uses
  // (semantic intent — see HistoryCard.svelte).
  const cssBlock = appSource.slice(
    appSource.indexOf("<style>"),
    appSource.length,
  );
  const fontFamilyRules = cssBlock.match(/font-family\s*:[^;]+;/g) ?? [];
  assert.ok(
    fontFamilyRules.length > 0,
    "App.svelte must declare a font-family rule",
  );
  for (const rule of fontFamilyRules) {
    const isTokenReference = rule.includes("var(--cv-font-family");
    const isInherit = /font-family\s*:\s*inherit/.test(rule);
    const isMonospace = /ui-monospace|SFMono-Regular|Menlo|Consolas/.test(rule);
    assert.ok(
      isTokenReference || isInherit || isMonospace,
      `every font-family declaration must reference --cv-font-family, inherit, or use the documented monospace fallback (got "${rule}")`,
    );
  }
});

test("Quick Paste references the --cv-font-family token, never a literal UI family", () => {
  // Same contract as the desktop: the palette MUST read the UI family
  // through `var(--cv-font-family, fallback)` or `inherit` (CSS
  // inheritance). The only allowed deviation is the documented
  // monospace family the preview text uses (semantic intent — see
  // HistoryCard.svelte and the `.qp-preview-text` rule below).
  const cssBlock = quickPasteSource.slice(
    quickPasteSource.indexOf("<style>"),
    quickPasteSource.length,
  );
  const fontFamilyRules = cssBlock.match(/font-family\s*:[^;]+;/g) ?? [];
  assert.ok(
    fontFamilyRules.length > 0,
    "QuickPaste.svelte must declare a font-family rule",
  );
  for (const rule of fontFamilyRules) {
    const isTokenReference = rule.includes("var(--cv-font-family");
    const isInherit = /font-family\s*:\s*inherit/.test(rule);
    const isMonospace = /ui-monospace|SFMono-Regular|Menlo|Consolas/.test(rule);
    assert.ok(
      isTokenReference || isInherit || isMonospace,
      `every font-family declaration must reference --cv-font-family, inherit, or use the documented monospace fallback (got "${rule}")`,
    );
  }
});

test("Quick Paste row title and metadata sizes use the documented tokens", () => {
  // The Quick Paste palette MUST use the same size scale the
  // desktop rail consumes — a hard-coded literal would let the
  // palette drift from the rail on a future theme change. The
  // row title consumes `--cv-control` (the documented card-title
  // weight `HistoryCard.svelte` uses), the body preview and the
  // status band consume `--cv-muted`, the elapsed / footer time
  // consume `--cv-tag`, and the search hint consumes
  // `--cv-preview`. The desktop-card-preview change extracts the
  // preview overlay to a shared component so the typography
  // tokens the preview consumes live in `ClipboardPreview.svelte`
  // — the assertion consults both files so a regression that
  // drops a token from one side still surfaces here.
  const previewSource = readFileSync(
    resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
    "utf8",
  );
  const cssBlock =
    quickPasteSource.slice(
      quickPasteSource.indexOf("<style>"),
      quickPasteSource.length,
    ) +
    "\n" +
    previewSource.slice(
      previewSource.indexOf("<style>"),
      previewSource.length,
    );
  const requiredTokens = [
    "--cv-body",
    "--cv-control",
    "--cv-muted",
    "--cv-tag",
    "--cv-preview",
    "--cv-title-md",
  ];
  for (const name of requiredTokens) {
    assert.ok(
      cssBlock.includes(`var(${name}`),
      `Quick Paste must consume ${name} so the typography stays in lock-step with the rail`,
    );
  }
});

test("VISUAL_TOKENS is the only source of truth and stays inert", () => {
  // The constant is what every consumer reads; a regression that
  // leaks clipboard content, hashes or paths into the token block
  // is a privacy regression the spec explicitly forbids.
  const rendered = visualTokenCss();
  assert.equal(rendered.includes("content"), false);
  assert.equal(rendered.includes("hash"), false);
  assert.equal(rendered.includes("path"), false);
  // The helper and the constant must agree on every property the
  // shell relies on. A drift between the two would surface here.
  for (const name of [
    "--cv-font-family",
    "--cv-body",
    "--cv-muted",
    "--cv-title",
    "--cv-title-lg",
    "--cv-title-md",
    "--cv-title-sm",
    "--cv-control",
    "--cv-tag",
    "--cv-preview",
    "--cv-bg-surface",
    "--cv-fg",
    "--cv-border",
    "--cv-card-size",
  ]) {
    assert.ok(
      rendered.includes(name),
      `VISUAL_TOKENS / visualTokenCss must include ${name}`,
    );
  }
});

test("Quick Paste's monospace preview keeps semantic intent", () => {
  // The capture preview (`<pre>`) MAY use a monospace family — that
  // is the only allowed deviation from the UI font, mirroring the
  // desktop's `HistoryCard.svelte`. The shared
  // `ClipboardPreview.svelte` owns the preview surface since the
  // `desktop-card-preview` change extracted it from `QuickPaste.svelte`,
  // so the assertion inspects the shared component for the
  // monospace family on the text block.
  const previewSource = readFileSync(
    resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
    "utf8",
  );
  const cssBlock = previewSource.slice(
    previewSource.indexOf("<style>"),
    previewSource.length,
  );
  const previewTextRule = cssBlock.match(/\.cv-preview-text\s*\{[^}]*\}/);
  assert.ok(previewTextRule, ".cv-preview-text rule must exist");
  assert.ok(
    /ui-monospace/.test(previewTextRule![0]),
    ".cv-preview-text may keep the semantic monospace family",
  );
  const titleRule = cssBlock.match(/\.cv-preview-title\s*\{[^}]*\}/);
  assert.ok(titleRule, ".cv-preview-title rule must exist");
  assert.equal(
    /monospace/.test(titleRule![0]),
    false,
    ".cv-preview-title MUST NOT use the monospace family — it inherits the UI family",
  );
});

test("the visual token helper agrees with the public VISUAL_TOKENS constant", () => {
  // The helper concatenates the constant's values into a CSS block;
  // the regression catches a drift between the two by asserting the
  // canonical `--cv-font-family` and `--cv-bg-surface` strings land
  // verbatim into the rendered CSS.
  const rendered = visualTokenCss();
  assert.ok(
    rendered.includes(`--cv-font-family: ${VISUAL_TOKENS.fontFamily};`),
    "the helper must emit the canonical font-family declaration from VISUAL_TOKENS",
  );
  assert.ok(
    rendered.includes(`--cv-bg-surface: ${VISUAL_TOKENS.surfaceBg};`),
    "the helper must emit the canonical surface background declaration",
  );
  assert.ok(
    rendered.includes(`--cv-card-size: ${VISUAL_TOKENS.cardSize};`),
    "the helper must emit the canonical card-size declaration",
  );
});
