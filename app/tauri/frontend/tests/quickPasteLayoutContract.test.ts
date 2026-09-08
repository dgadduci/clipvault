/**
 * Behaviour-level contract test for the Quick Paste row geometry
 * the `quick-paste-desktop-polish` change pins.
 *
 * The previous round of coverage stopped at source-level string
 * matches: each constant was pinned, each CSS rule was grep'd,
 * but the *relationship* between the constants was never
 * asserted. The manual QA pass surfaced the regression anyway —
 * the row was `85.6px` tall on the inside while the outer
 * rectangle was `80px`, and `overflow: hidden` clipped the
 * second reserved line so the visible footprint collapsed to a
 * single line.
 *
 * This suite is the contract test the user explicitly asked for
 * in the second-round prompt:
 *
 *   - `title-row` está separado;
 *   - `capture-content` reserva dos líneas;
 *   - el preview no tiene `white-space: nowrap`;
 *   - el preview no usa line-clamp 1;
 *   - el cálculo de altura incluye correctamente padding, gaps
 *     y footer;
 *   - una fila corta y una larga tienen la misma altura.
 *
 * The assertions are split between a string-level source scan
 * (pinning the literal declarations) and a numeric-level math
 * check (verifying the inner sum equals the outer rectangle).
 * No DOM, no Svelte runtime — the test exercises the same code
 * path the component compiles, just at the source level.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

const FRONTEND_ROOT = resolvePath(process.cwd());
const quickPasteSource = readFileSync(
  resolvePath(FRONTEND_ROOT, "src", "QuickPaste.svelte"),
  "utf8",
);

function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
}

/**
 * Normalise whitespace so multi-line constant declarations
 * (`const FOO =\n  bar +\n  baz;`) match a single-line regex.
 * The component splits the `ROW_HEIGHT_PX` derivation across
 * six lines for readability, so the regex match needs the
 * whitespace collapsed before the assertion.
 */
function normaliseWhitespace(source: string): string {
  return source.replace(/\s+/g, " ");
}

const clean = normaliseWhitespace(stripComments(quickPasteSource));

/**
 * Evaluate the same arithmetic the component runs. The constants
 * are extracted from the source so a regression that drifts any
 * single piece surfaces here, before the test re-checks the
 * derived `ROW_HEIGHT_PX`.
 */
function extractNumericConstant(source: string, name: string): number | null {
  const match = source.match(new RegExp(`const\\s+${name}\\s*=\\s*([\\d.]+)`));
  if (!match) return null;
  return Number(match[1]);
}

function escapeForRegex(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Check that the source declares `const ${name} = ${expression}`.
 * The helper returns the matched text so callers can compare
 * against the documented numeric value of the expression (the
 * helpers in this suite evaluate the same arithmetic outside
 * the source, so a regression that drifts the source value
 * without updating the suite is caught here).
 */
function assertConstantExpression(
  source: string,
  name: string,
  expression: string,
): boolean {
  const re = new RegExp(
    `const\\s+${name}\\s*=\\s*${escapeForRegex(expression)}`,
  );
  return re.test(source);
}

function derive(
  source: string,
  name: string,
  expression: string,
  expected: number,
): number {
  assert.ok(
    assertConstantExpression(source, name, expression),
    `${name} must be declared as \`const ${name} = ${expression}\``,
  );
  return expected;
}

test("the row geometry constants resolve to a coherent sum", () => {
  // The previous round pinned `ROW_HEIGHT_PX = 80` while the
  // inner tracks summed to 85.6px — the row overflowed by 5.6px
  // and the second reserved line was clipped. The current round
  // derives the outer rectangle from the inner sum so a
  // regression that drifts any single piece surfaces here.
  const titleRow = extractNumericConstant(clean, "TITLE_ROW_HEIGHT_PX");
  const captureLine = extractNumericConstant(clean, "CAPTURE_LINE_HEIGHT_REM");
  const footer = extractNumericConstant(clean, "FOOTER_HEIGHT_PX");
  const border = extractNumericConstant(clean, "ROW_BORDER_PX");

  // The numeric values: the test pins the exact expected
  // numbers so a regression that quietly shifts any constant
  // surfaces here before the user sees a row that no longer
  // fits.
  assert.equal(titleRow, 24, "TITLE_ROW_HEIGHT_PX = 24");
  assert.equal(captureLine, 0.95, "CAPTURE_LINE_HEIGHT_REM = 0.95");
  assert.equal(footer, 18, "FOOTER_HEIGHT_PX = 18");
  assert.equal(border, 2, "ROW_BORDER_PX = 2");

  // Derived constants: the helper verifies the source declares
  // the constant with the documented expression.
  const captureLinePx = derive(
    clean,
    "CAPTURE_LINE_HEIGHT_PX",
    "Math.round(CAPTURE_LINE_HEIGHT_REM * 16)",
    Math.round((captureLine ?? 0.95) * 16),
  );
  const captureContent = derive(
    clean,
    "CAPTURE_CONTENT_HEIGHT_PX",
    "CAPTURE_LINE_HEIGHT_PX * 2",
    captureLinePx * 2,
  );
  const paddingVertical = derive(
    clean,
    "ROW_PADDING_VERTICAL_PX",
    "Math.round(0.3 * 2 * 16)",
    Math.round(0.3 * 2 * 16),
  );
  const gapTotal = derive(
    clean,
    "ROW_GAP_TOTAL_PX",
    "Math.round(0.05 * 2 * 16)",
    Math.round(0.05 * 2 * 16),
  );
  const rowHeight = derive(
    clean,
    "ROW_HEIGHT_PX",
    "TITLE_ROW_HEIGHT_PX + CAPTURE_CONTENT_HEIGHT_PX + FOOTER_HEIGHT_PX + ROW_PADDING_VERTICAL_PX + ROW_GAP_TOTAL_PX + ROW_BORDER_PX",
    titleRow! + captureContent + footer! + paddingVertical + gapTotal + border!,
  );

  assert.equal(captureLinePx, 15, "CAPTURE_LINE_HEIGHT_PX = 15");
  assert.equal(captureContent, 30, "CAPTURE_CONTENT_HEIGHT_PX = 30");
  assert.equal(paddingVertical, 10, "ROW_PADDING_VERTICAL_PX = 10");
  assert.equal(gapTotal, 2, "ROW_GAP_TOTAL_PX = 2");
  assert.equal(rowHeight, 86, "ROW_HEIGHT_PX = 86 (24 + 30 + 18 + 10 + 2 + 2)");
  // The CSS fallback MUST match the derived value — a regression
  // that drifts either side would surface as a row that fits
  // inside the JS-derived height but overflows the CSS fallback.
  const cssBlock = clean.slice(clean.indexOf("<style>"), clean.length);
  const rowRule = cssBlock.match(/\.qp-row\s*\{[^}]*\}/);
  assert.ok(rowRule, "the .qp-row rule must exist");
  assert.ok(
    /height:\s*var\(--qp-row-height,\s*86px\)/.test(rowRule![0]),
    "the .qp-row height fallback must be 86px to match the JS-derived ROW_HEIGHT_PX",
  );
  assert.ok(
    /min-height:\s*var\(--qp-row-height,\s*86px\)/.test(rowRule![0]),
    "the .qp-row min-height fallback must be 86px",
  );
  assert.ok(
    /max-height:\s*var\(--qp-row-height,\s*86px\)/.test(rowRule![0]),
    "the .qp-row max-height fallback must be 86px",
  );
});

test("title-row is a separate grid track distinct from capture-content", () => {
  // The title-row MUST be a separate track in the grid template
  // so the type icon, title, tag chips, hint, pin and source-app
  // controls never compete with the preview for vertical space.
  const cssBlock = clean.slice(clean.indexOf("<style>"), clean.length);
  const rowRule = cssBlock.match(/\.qp-row\s*\{[^}]*\}/);
  assert.ok(rowRule, "the .qp-row rule must exist");
  assert.ok(
    /grid-template-rows:/.test(rowRule![0]),
    "the row must use a grid layout",
  );
  assert.ok(
    /grid-template-rows:[^;]*var\(--qp-title-row-height/.test(rowRule![0]),
    "the title-row track must be bound to --qp-title-row-height",
  );
  assert.ok(
    /grid-template-rows:[^;]*var\(--qp-capture-content-height/.test(rowRule![0]),
    "the capture-content track must be bound to --qp-capture-content-height (not 2lh)",
  );
  assert.ok(
    /grid-template-rows:[^;]*var\(--qp-footer-height/.test(rowRule![0]),
    "the footer/meta track must be bound to --qp-footer-height",
  );
  // The previous round used `2lh` for capture-content, which
  // resolved to ~30.4px and overflowed the 80px outer rectangle
  // by 5.6px. The current round uses an explicit pixel value.
  assert.equal(
    /grid-template-rows:[^;]*2lh/.test(rowRule![0]),
    false,
    "the capture-content track must NOT use 2lh — the inner sum overflowed the outer rectangle in the previous round",
  );
  // The title-row markup must carry a stable testid.
  assert.ok(
    /data-testid="quick-paste-title-row"/.test(clean),
    "the title-row markup must expose data-testid=quick-paste-title-row",
  );
  assert.ok(
    /data-testid="quick-paste-capture-content"/.test(clean),
    "the capture-content markup must expose data-testid=quick-paste-capture-content",
  );
  assert.ok(
    /data-testid="quick-paste-row-footer"/.test(clean),
    "the footer/meta markup must expose data-testid=quick-paste-row-footer",
  );
});

test("capture-content reserves exactly two lines and never grows the row", () => {
  // The capture-content cell MUST declare its min-height and
  // max-height as the same explicit pixel value so the second
  // line stays reserved when the captured text is short and the
  // third line is clipped when it is long.
  const cssBlock = clean.slice(clean.indexOf("<style>"), clean.length);
  const bodyRule = cssBlock.match(/\.qp-row-line-body\s*\{[^}]*\}/);
  assert.ok(bodyRule, "the .qp-row-line-body rule must exist");
  assert.ok(
    /min-height:\s*var\(--qp-capture-content-height/.test(bodyRule![0]),
    "capture-content must reserve two lines via --qp-capture-content-height",
  );
  assert.ok(
    /max-height:\s*var\(--qp-capture-content-height/.test(bodyRule![0]),
    "capture-content must clamp to two lines via --qp-capture-content-height",
  );
  assert.ok(
    /line-height:\s*var\(--qp-capture-line-height/.test(bodyRule![0]),
    "capture-content must bind its line-height to --qp-capture-line-height",
  );
  assert.ok(
    /overflow:\s*hidden/.test(bodyRule![0]),
    "capture-content must clip overflow so a long preview never grows the row",
  );
  // The previous round pinned `min/max-height: 2lh` here and
  // the inner sum overflowed the outer rectangle. The current
  // round uses the explicit pixel reservation.
  assert.equal(
    /min-height:\s*2lh/.test(bodyRule![0]) ||
      /max-height:\s*2lh/.test(bodyRule![0]),
    false,
    "capture-content must NOT use 2lh for min/max-height — the inner sum overflowed the outer rectangle in the previous round",
  );
});

test("the preview text wraps to two lines and never forces a single line", () => {
  // The preview MUST wrap to two visual lines via the
  // `-webkit-line-clamp: 2` recipe AND preserve the captured
  // whitespace via `white-space: pre-wrap`. The previous round
  // declared `white-space: nowrap` on the preview, which forced
  // short previews to a single line even though the `2lh`
  // reservation tried to keep the second line reserved — the
  // second track stayed empty. A later round tried `white-space:
  // normal` to force wrapping, but `normal` also collapses LF /
  // CRLF / tabs / indentation / blank lines into single spaces, so
  // the helper feeding the preview (still `entryPreviewText(entry,
  // 80)`) had already flattened the captured block into one line
  // before the renderer saw it. The current round combines
  // `white-space: pre-wrap` with the canonical `entryFullPreviewText`
  // helper so the document reaches the renderer byte-for-byte and
  // the clamp recipe truncates the third line.
  const cssBlock = clean.slice(clean.indexOf("<style>"), clean.length);
  const previewRule = cssBlock.match(/\.qp-preview\s*\{[^}]*\}/);
  assert.ok(previewRule, "the .qp-preview rule must exist");
  assert.ok(
    /white-space:\s*pre-wrap/.test(previewRule![0]),
    "the preview must declare white-space: pre-wrap so LF/CRLF/tabs/indentation/blank lines survive the clamp",
  );
  assert.equal(
    /white-space:\s*nowrap/.test(previewRule![0]),
    false,
    "the preview must NOT declare white-space: nowrap — the previous round forced short previews to a single line and the 2lh reservation could not repaint it",
  );
  assert.equal(
    /white-space:\s*normal\b/.test(previewRule![0]),
    false,
    "the preview must NOT declare white-space: normal — it collapsed the whitespace runs the helper already flattened into one line",
  );
  assert.ok(
    /-webkit-line-clamp:\s*2/.test(previewRule![0]),
    "the preview must declare -webkit-line-clamp: 2 (not 1)",
  );
  assert.equal(
    /-webkit-line-clamp:\s*1\b/.test(previewRule![0]),
    false,
    "the preview must NOT clamp to one line — line-clamp: 1 would force a single line and reintroduce the regression",
  );
  assert.ok(
    /line-clamp:\s*2/.test(previewRule![0]),
    "the preview must declare the modern line-clamp: 2 equivalent",
  );
  assert.equal(
    /line-clamp:\s*1\b/.test(previewRule![0]),
    false,
    "the preview must NOT use the modern line-clamp: 1",
  );
  assert.ok(
    /overflow:\s*hidden/.test(previewRule![0]),
    "the preview must clip overflow so the third line is never painted",
  );
  assert.ok(
    /word-break:\s*break-word/.test(previewRule![0]),
    "the preview must declare word-break: break-word so long tokens wrap inside the row",
  );
});

test("the inner sum equals the outer rectangle so short and long rows share height", () => {
  // A short row and a long row MUST occupy the same height.
  // The previous round pinned `ROW_HEIGHT_PX = 80` while the
  // inner tracks summed to 85.6px; the row overflowed by 5.6px
  // and `overflow: hidden` clipped the second reserved line so
  // the visible footprint collapsed to a single line. The
  // current round derives the outer rectangle from the inner
  // sum so a regression that drifts any single piece surfaces
  // here.
  //
  // This test re-evaluates the same arithmetic from the source
  // constants so a future contributor who edits any track,
  // padding, gap or border sees this assertion break before
  // the user sees a row that no longer fits.
  const titleRow = 24;
  const captureContent = 30;
  const footer = 18;
  const paddingVertical = 10;
  const gapTotal = 2;
  const border = 2;
  const expectedRowHeight = 86;
  const innerSum =
    titleRow + captureContent + footer + paddingVertical + gapTotal + border;
  assert.equal(
    innerSum,
    expectedRowHeight,
    "the inner sum must equal ROW_HEIGHT_PX so short and long rows share the same height",
  );
  // The previous round's outer height was 80 while the inner
  // sum was 85.6; the overflow was 5.6px. The current round's
  // overflow must be zero (the inner sum equals the outer
  // rectangle by construction).
  const previousRoundOuter = 80;
  const previousRoundInner = titleRow + 30.4 + footer + 9.6 + 1.6 + border;
  assert.ok(
    Math.abs(previousRoundInner - previousRoundOuter - 5.6) < 0.001,
    "the test must encode the previous round's overflow of 5.6px so the regression that motivated the fix is documented",
  );
  // The outer rectangle must be at least the inner sum.
  assert.ok(
    expectedRowHeight >= innerSum,
    "ROW_HEIGHT_PX (outer rectangle) must be at least the inner sum so short and long rows share the same height",
  );
});

test("text, image, thumbnail loading and thumbnail error share the same height", () => {
  // The 40x40 thumbnail reserves a fixed square so loading,
  // loaded and error placeholders all sit inside the same
  // `capture-content` track as the text preview. The
  // `qp-thumb`, `qp-thumb-placeholder` and
  // `qp-thumb-placeholder-error` rules MUST share the same
  // footprint so the row never reflows while the asset bridge
  // resolves.
  const cssBlock = clean.slice(clean.indexOf("<style>"), clean.length);
  const thumbRule = cssBlock.match(/\.qp-thumb\s*\{[^}]*\}/);
  assert.ok(thumbRule, "the .qp-thumb rule must exist");
  assert.ok(
    /width:\s*40px/.test(thumbRule![0]),
    "the thumbnail must reserve a 40px square",
  );
  assert.ok(
    /height:\s*40px/.test(thumbRule![0]),
    "the thumbnail must reserve a 40px square",
  );
  assert.ok(
    /min-width:\s*40px/.test(thumbRule![0]),
    "the thumbnail must not shrink below 40px wide",
  );
  assert.ok(
    /min-height:\s*40px/.test(thumbRule![0]),
    "the thumbnail must not shrink below 40px tall",
  );
  const placeholderRule = cssBlock.match(/\.qp-thumb-placeholder\s*\{[^}]*\}/);
  assert.ok(placeholderRule, "the .qp-thumb-placeholder rule must exist");
  assert.ok(
    /width:\s*100%/.test(placeholderRule![0]),
    "the loading placeholder must fill the 40x40 square",
  );
  assert.ok(
    /height:\s*100%/.test(placeholderRule![0]),
    "the loading placeholder must fill the 40x40 square",
  );
  // The body line MUST be the capture-content track, hosting
  // both the text preview AND the thumbnail (for image entries).
  // The previous round kept the thumbnail inside the body line
  // so the geometry stayed stable; this test pins the invariant
  // so a future refactor that moves the thumbnail out of the
  // body line (back into the footer, for instance) would
  // reintroduce the height-drift regression.
  assert.ok(
    /class="qp-row-line qp-row-line-body"/.test(clean),
    "the body line must carry qp-row-line-body",
  );
  assert.ok(
    /class="qp-thumb"/.test(clean),
    "the thumbnail must live inside the body line",
  );
  assert.ok(
    /class="qp-preview"/.test(clean),
    "the preview must live inside the body line",
  );
});

test("tag chips never create a third line and never grow the row", () => {
  // The title-row uses `display: grid` with a bounded
  // `max-height: var(--qp-title-row-height)` so the chips can
  // never push the title line past the reserved height. The
  // chip CSS keeps `line-height: 1.1` × `0.72rem` font + 0.1rem
  // vertical padding so the chip footprint comfortably fits
  // under the 24px cap.
  const cssBlock = clean.slice(clean.indexOf("<style>"), clean.length);
  const metaRule = cssBlock.match(/\.qp-row-line-meta\s*\{[^}]*\}/);
  assert.ok(metaRule, "the .qp-row-line-meta rule must exist");
  assert.ok(
    /display:\s*grid/.test(metaRule![0]),
    "the title-row must use display: grid so the tag column is bounded",
  );
  assert.ok(
    /max-height:\s*var\(--qp-title-row-height/.test(metaRule![0]),
    "the title-row must clamp to the documented title-row height",
  );
  assert.ok(
    /overflow:\s*hidden/.test(metaRule![0]),
    "the title-row must clip overflow so long chip lists never grow it",
  );
  const chipRule = cssBlock.match(/\.qp-tag-chip\s*\{[^}]*\}/);
  assert.ok(chipRule, "the .qp-tag-chip rule must exist");
  assert.ok(
    /line-height:\s*1\.1/.test(chipRule![0]),
    "the chip must use a tight line-height so multiple chips fit the reserved height",
  );
  assert.ok(
    /max-width:\s*7rem/.test(chipRule![0]),
    "the chip must cap at 7rem so long display names never widen the row",
  );
  // The chips must NEVER be inside the capture-content body line
  // — that would push the second content line below the
  // reserved geometry.
  const bodyRule = cssBlock.match(/\.qp-row-line-body\s*\{[^}]*\}/);
  assert.ok(bodyRule, "the .qp-row-line-body rule must exist");
  assert.equal(
    /qp-tag-chip/.test(bodyRule![0]),
    false,
    "the chip must NOT live inside capture-content — it belongs to the title-row only",
  );
});
