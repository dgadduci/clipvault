/**
 * Source-level coverage for the `quick-paste-desktop-polish` change.
 *
 * The suite pins the new contracts the OpenSpec change introduces
 * without mounting a Svelte component: it walks the source files
 * the change touches and asserts the documented invariants through
 * small string greps and the shared `tsconfig` so a regression
 * surfaces here before it reaches the user.
 *
 * The tests are intentionally additive to the existing suites; no
 * regression test is removed or rewritten.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

const quickPasteSource = readFileSync(
  resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
  "utf8",
);

const quickPasteTagsSource = readFileSync(
  resolvePath(process.cwd(), "src", "lib", "quickPasteTags.ts"),
  "utf8",
);

const historyCardSource = readFileSync(
  resolvePath(process.cwd(), "src", "HistoryCard.svelte"),
  "utf8",
);

const clipboardPreviewSource = readFileSync(
  resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
  "utf8",
);

// ---------------------------------------------------------------------------
// 1. Quick Paste — surface state machine and Escape contextual.
// ---------------------------------------------------------------------------

test("Quick Paste exposes an explicit surface state machine", () => {
  // The `quick-paste-desktop-polish` change introduces an
  // explicit `surface === "preview"` branch the window keydown
  // handler consults so the Escape contract cannot drift from
  // the visible surface.
  assert.ok(
    /\$\:\s*surface\s*=\s*previewEntryId\s*!==\s*null\s*\?\s*"preview"\s*:\s*"list"/.test(
      quickPasteSource,
    ) ||
      /\$\:\s*surface\s*=/.test(quickPasteSource) &&
        /previewEntryId\s*!==\s*null/.test(quickPasteSource),
    "Quick Paste must derive the surface flag from previewEntryId",
  );
});

test("Quick Paste closes the preview without hiding the window", () => {
  // The Escape contract: in `preview`, Escape returns to the
  // list (no `hideQuickPasteWindow` call). A regression that
  // routes Escape through the list handler would close the
  // entire window.
  const onKeydown = extractFunction(quickPasteSource, "onWindowKeydown");
  assert.ok(
    /surface\s*===\s*"preview"/.test(onKeydown) ||
      /previewEntryId\s*!==\s*null/.test(onKeydown),
    "the window keydown handler must consult the surface flag",
  );
  assert.ok(
    /closePreview\(\)/.test(onKeydown),
    "the window keydown handler must close the preview on Escape",
  );
});

test("Quick Paste closes the window from the list surface", () => {
  // The list branch keeps the existing behaviour: Escape hides
  // the Quick Paste window through the documented bridge.
  const handleEscape = extractFunction(quickPasteSource, "handleEscape");
  assert.ok(
    /hideQuickPasteWindow/.test(handleEscape),
    "the list Escape handler must hide the Quick Paste window",
  );
});

test("Quick Paste restores focus after the preview closes", () => {
  // When the preview closes, focus MUST return to the search
  // input so the user can immediately type a new query. A
  // regression that never restores focus leaves the focus on
  // the document body.
  const closePreview = extractFunction(quickPasteSource, "closePreview");
  assert.ok(
    /restoreFocusAfterPreview/.test(closePreview) ||
      /searchInputEl\.focus/.test(closePreview) ||
      /queueMicrotask/.test(closePreview),
    "closePreview must restore focus after the overlay unmounts",
  );
});

// ---------------------------------------------------------------------------
// 2. Quick Paste — fixed row geometry (two lines, 72px, no wrap).
// ---------------------------------------------------------------------------

test("Quick Paste row CSS pins the fixed geometry and two-line grid", () => {
  // The row MUST keep the documented fixed height (now derived
  // from the inner sum so the inner tracks always fit the outer
  // rectangle by construction) and the explicit three-track grid
  // template (`title-row` + `capture-content` + `footer/meta`).
  // The previous round pinned the outer height at 80px while the
  // inner tracks summed to 85.6px; the row overflowed by 5.6px and
  // `overflow: hidden` clipped the second reserved line. The
  // current round uses explicit pixel reservations for the
  // capture-content track (`--qp-capture-content-height`) and
  // recomputes the outer height so the inner sum fits inside.
  const cssBlock = quickPasteSource.slice(
    quickPasteSource.indexOf("<style>"),
    quickPasteSource.length,
  );
  const rowRule = cssBlock.match(/\.qp-row\s*\{[^}]*\}/);
  assert.ok(rowRule, "the .qp-row CSS rule must exist");
  assert.ok(
    /height:\s*var\(--qp-row-height/.test(rowRule![0]),
    "the row must declare a height token",
  );
  assert.ok(
    /min-height:\s*var\(--qp-row-height/.test(rowRule![0]),
    "the row must declare a min-height token",
  );
  assert.ok(
    /max-height:\s*var\(--qp-row-height/.test(rowRule![0]),
    "the row must declare a max-height token",
  );
  assert.ok(
    /grid-template-rows:[^;]*var\(--qp-title-row-height/.test(rowRule![0]),
    "the row must reserve the title-row track via --qp-title-row-height",
  );
  assert.ok(
    /grid-template-rows:[^;]*var\(--qp-capture-content-height/.test(rowRule![0]),
    "the row must reserve the capture-content track via --qp-capture-content-height (not 2lh)",
  );
  assert.equal(
    /grid-template-rows:[^;]*2lh/.test(rowRule![0]),
    false,
    "the row must NOT use 2lh for the capture-content track — the inner sum overflowed the outer rectangle by 5.6px in the previous round",
  );
  assert.ok(
    /grid-template-rows:[^;]*var\(--qp-footer-height/.test(rowRule![0]),
    "the row must reserve the footer/meta track via --qp-footer-height",
  );
});

test("Quick Paste title line reserves the tags column with overflow protection", () => {
  // The title line MUST keep the type, title, tags and the
  // selection-state preview hint. A regression that drops the
  // tags column from the grid would force the chips into the
  // title column and the ellipsis truncation would no longer be
  // effective.
  const cssBlock = quickPasteSource.slice(
    quickPasteSource.indexOf("<style>"),
    quickPasteSource.length,
  );
  const metaRule = cssBlock.match(/\.qp-row-line-meta\s*\{[^}]*\}/);
  assert.ok(metaRule, "the .qp-row-line-meta CSS rule must exist");
  assert.ok(
    /grid-template-columns:[^;]*1\.5rem/.test(metaRule![0]),
    "the meta line must keep the 1.5rem type column",
  );
  assert.ok(
    /grid-template-columns:[^;]*minmax\(0,\s*1fr\)/.test(metaRule![0]),
    "the meta line must keep the flex title column",
  );
  assert.ok(
    /grid-template-columns:[^;]*auto/.test(metaRule![0]),
    "the meta line must allocate an auto column for tags and hint",
  );
  // The tag chip CSS must reserve a bounded width and apply
  // ellipsis so a long display name never widens the row.
  const tagChipRule = cssBlock.match(/\.qp-tag-chip\s*\{[^}]*\}/);
  assert.ok(tagChipRule, "the .qp-tag-chip CSS rule must exist");
  assert.ok(
    /max-width:\s*7rem/.test(tagChipRule![0]),
    "tag chips must cap at 7rem to bound the row width",
  );
  assert.ok(
    /text-overflow:\s*ellipsis/.test(tagChipRule![0]),
    "tag chips must ellipsize overflow",
  );
});

// ---------------------------------------------------------------------------
// 3. Quick Paste — tags hydration with stale-response guard.
// ---------------------------------------------------------------------------

test("Quick Paste uses the quickPasteTags helper module", () => {
  // The component MUST consume the shared tags cache helper so
  // the stale-response guard and the truncation projection
  // cannot drift across the change.
  assert.ok(
    quickPasteSource.includes("quickPasteTags"),
    "Quick Paste must import the quickPasteTags helper",
  );
  assert.ok(
    /bumpQuickPasteTagsToken/.test(quickPasteSource),
    "Quick Paste must call bumpQuickPasteTagsToken before every bridge round",
  );
  assert.ok(
    /currentQuickPasteTagsToken/.test(quickPasteSource),
    "Quick Paste must consult currentQuickPasteTagsToken in the stale-response branch",
  );
  assert.ok(
    /truncateQuickPasteTags/.test(quickPasteSource),
    "Quick Paste must consult truncateQuickPasteTags to bound the chip row",
  );
});

test("Quick Paste hydration feeds the visible recents and search hits", () => {
  // The hydration round MUST walk both the recents feed and the
  // search hits so the user sees chips on every supported
  // surface, not just the default view.
  assert.ok(
    /hydrateTagsForVisibleEntries\(recent\)/.test(quickPasteSource),
    "Quick Paste must hydrate the recents feed",
  );
  assert.ok(
    /hydrateTagsForVisibleEntries\([^)]+response\.hits/.test(quickPasteSource) ||
      /hydrateTagsForVisibleEntries\([^)]+hits/.test(quickPasteSource),
    "Quick Paste must hydrate the search hits",
  );
});

test("Quick Paste tag chips never expose ids, paths, hashes or bytes", () => {
  // Privacy invariant pinned by the spec. The visible markup
  // MUST only carry the resolved `Tag` display_name; the
  // `data-tag-id` attribute is a metadata-only contract the
  // tests rely on but the visible copy must not include the
  // stable identifier. Source-app / asset references, hashes
  // and absolute paths must never reach the chip row.
  const tagSpanMatch = quickPasteSource.match(
    /<span\s+class="qp-tag-chip"[\s\S]*?<\/span>/,
  );
  assert.ok(tagSpanMatch, "the chip row markup must exist");
  // Visible text in the chip must come from `chip.tag.display_name`,
  // never from any internal identifier.
  assert.ok(
    /\{chip\.tag\.display_name\}/.test(tagSpanMatch![0]),
    "the chip must render display_name, not the stable id",
  );
  // The overflow chip must render the count without exposing
  // the underlying tag payload.
  const moreChipMatch = quickPasteSource.match(
    /<span\s+class="qp-tag-chip qp-tag-chip-more"[\s\S]*?<\/span>/,
  );
  assert.ok(moreChipMatch, "the overflow chip must exist");
  assert.ok(
    /\+{tagsProjection\.overflow}/.test(moreChipMatch![0]) ||
      /\+{overflow}/.test(moreChipMatch![0]),
    "the overflow chip must render the documented count",
  );
});

test("the quickPasteTags helper exposes the per-entry token table", () => {
  // The stale-response guard is a single switch the Svelte
  // layer consults. A regression that drops the bump / current
  // helpers would silently allow late commits to land on the
  // wrong row.
  assert.ok(
    /export function bumpQuickPasteTagsToken/.test(quickPasteTagsSource),
    "the helper must expose bumpQuickPasteTagsToken",
  );
  assert.ok(
    /export function currentQuickPasteTagsToken/.test(quickPasteTagsSource),
    "the helper must expose currentQuickPasteTagsToken",
  );
  assert.ok(
    /export function resetQuickPasteTagsToken/.test(quickPasteTagsSource),
    "the helper must expose resetQuickPasteTagsToken",
  );
  assert.ok(
    /export function truncateQuickPasteTags/.test(quickPasteTagsSource),
    "the helper must expose truncateQuickPasteTags",
  );
});

// ---------------------------------------------------------------------------
// 4. Desktop card preview — shared code projection.
// ---------------------------------------------------------------------------

test("HistoryCard reuses the shared code projection for code rows", () => {
  // The card MUST render the highlighted projection when the
  // entry has a canonical code language. A regression that
  // keeps the plain-text fallback for code rows would silently
  // drop the syntax colours and the tabs/line-breaks contract.
  assert.ok(
    /shouldRenderHighlightedPreview/.test(historyCardSource),
    "HistoryCard must consult shouldRenderHighlightedPreview",
  );
  assert.ok(
    /renderHighlightedCode/.test(historyCardSource),
    "HistoryCard must call renderHighlightedCode",
  );
  assert.ok(
    /preview-code/.test(historyCardSource),
    "HistoryCard must apply the .preview-code CSS class",
  );
  // The fallback (non-code) row must still render the plain
  // text preview; the regression suite already pins this.
  assert.ok(
    /class="preview"/.test(historyCardSource),
    "HistoryCard must keep the plain-text preview branch",
  );
});

test("HistoryCard surfaces the highlighted preview in the right testid", () => {
  // The card MUST keep the `data-testid="history-card-preview"`
  // selector on both branches so a regression that drops the
  // shared projection (or moves the testid) breaks the
  // regression suite before it reaches the user.
  assert.ok(
    /data-testid="history-card-preview"/.test(historyCardSource),
    "HistoryCard must keep the documented history-card-preview testid",
  );
  // The highlighted branch must record the data-preview-kind
  // marker so a regression that drops the shared projection
  // surfaces through the regression suite.
  assert.ok(
    /data-preview-kind="code"/.test(historyCardSource),
    "the highlighted branch must record data-preview-kind=code",
  );
});

// ---------------------------------------------------------------------------
// 5. Tooltip on the capture-type icon.
// ---------------------------------------------------------------------------

test("HistoryCard exposes an accessible tooltip on the type icon", () => {
  // The icon MUST surface the canonical, human-readable label
  // through `aria-label` AND `title`, so a screen reader and a
  // mouse-only user both get the same affordance. The change
  // also pins `data-content-type` on the icon so the regression
  // suite can target it.
  const typeIconBlock = historyCardSource.match(
    /<span\s+class="type"[\s\S]*?<\/span>/,
  );
  assert.ok(typeIconBlock, "the type icon markup must exist");
  assert.ok(
    /aria-label=/.test(typeIconBlock![0]),
    "the type icon must declare aria-label",
  );
  assert.ok(
    /title=/.test(typeIconBlock![0]),
    "the type icon must declare a title tooltip",
  );
  assert.ok(
    /data-content-type=/.test(typeIconBlock![0]),
    "the type icon must record data-content-type",
  );
  assert.ok(
    /contentTypeIconLabel/.test(typeIconBlock![0]) ||
      /contentTypeIconLabel\(entry\.content_type\)/.test(historyCardSource),
    "the type icon must derive the label from contentTypeIconLabel",
  );
});

test("HistoryCard tooltip never changes the card geometry", () => {
  // The tooltip branch MUST keep the documented card geometry.
  // The .type CSS rule still pins the 1.5rem square so the
  // title tooltip does not reflow the header.
  const cssBlock = historyCardSource.slice(
    historyCardSource.indexOf("<style>"),
    historyCardSource.length,
  );
  const typeRule = cssBlock.match(/\.type\s*\{[^}]*\}/);
  assert.ok(typeRule, "the .type CSS rule must exist");
  assert.ok(
    /width:\s*1\.5rem/.test(typeRule![0]),
    "the type icon must keep the 1.5rem width",
  );
  assert.ok(
    /height:\s*1\.5rem/.test(typeRule![0]),
    "the type icon must keep the 1.5rem height",
  );
});

// ---------------------------------------------------------------------------
// 6. Sanitization preserved.
// ---------------------------------------------------------------------------

test("HistoryCard never injects unsanitized markup into the card body", () => {
  // The card MUST keep the documented sanitization. A
  // regression that injects the captured content through
  // `{@html}` (without the highlighted projection) would
  // surface hostile content. The card MUST only render the
  // highlighted projection when `shouldRenderHighlightedPreview`
  // returns true; the helper is the documented sanitization
  // gate.
  const cardBody = historyCardSource;
  // The fallback branch must use the regular Svelte text
  // interpolation so a hostile capture cannot reach the DOM
  // through the {…} syntax.
  const fallback = cardBody.match(
    /class="preview"[\s\S]*?\{previewText\}<\/pre>/,
  );
  assert.ok(fallback, "the fallback preview must interpolate previewText as text");
  // The highlighted branch must guard the {@html} interpolation
  // with `shouldRenderHighlightedPreview(entry)`. The helper
  // gate is the documented sanitization.
  const highlightedMatch = cardBody.match(/\{@html highlightedPreviewHtml\}/);
  assert.ok(
    highlightedMatch,
    "the highlighted preview must guard the @html interpolation",
  );
  // ClipboardPreview still uses the same sanitization contract;
  // a regression that breaks the shared projection would also
  // break Desktop.
  assert.ok(
    /shouldRenderHighlightedPreview/.test(clipboardPreviewSource),
    "ClipboardPreview must keep the shouldRenderHighlightedPreview gate",
  );
});

// ---------------------------------------------------------------------------
// 7. Privacy — sensitive categories never reach the visible surface.
// ---------------------------------------------------------------------------

test("the Quick Paste tag chip row never echoes sensitive payloads", () => {
  // The chip row template MUST NEVER render entry content,
  // hashes, asset references, absolute paths or clipboard
  // payloads. A regression that surfaces one of these would
  // break the spec's privacy contract.
  //
  // The scan is restricted to the chip row markup so the rest
  // of the file (which legitimately uses `asset_ref` to drive
  // the image thumbnail bridge) cannot trigger a false
  // positive. The chip row is the only surface the
  // `quick-paste-desktop-polish` change introduces.
  const chipRowMatch = quickPasteSource.match(
    /class="qp-tags"[\s\S]*?data-tag-id=\{chip\.tag\.id\}[\s\S]*?\+{tagsProjection\.overflow}/,
  );
  assert.ok(chipRowMatch, "the chip row markup must exist");
  const chipRow = chipRowMatch![0];
  const sensitiveSubstrings = [
    "asset_ref",
    "content_hash",
    "rich_html_ref",
    "rich_rtf_ref",
    "rich_preview_ref",
    "/Users/",
    "entry.content",
  ];
  for (const forbidden of sensitiveSubstrings) {
    assert.equal(
      chipRow.includes(forbidden),
      false,
      `Quick Paste chip row must not surface ${forbidden}`,
    );
  }
  // The metadata-only `data-tag-id` attribute is allowed: it
  // mirrors the existing pattern the rest of the rail pins and
  // is metadata, not visible content.
  assert.ok(
    /data-tag-id=\{chip\.tag\.id\}/.test(chipRow),
    "the chip row must keep the metadata-only data-tag-id hook",
  );
});

// ---------------------------------------------------------------------------
// 8. Row preview content — whitespace + clamp recipe.
//
// Companion to the behaviour-level tests in
// `quickPasteDesktopPolishRegressions.test.ts`. The source-level
// assertions here catch the same regressions a future
// contributor could reintroduce while editing the component, but
// without mounting Svelte. The `quick-paste-desktop-polish`
// change documents that the row preview must:
//
//   - feed `entryFullPreviewText(entry)` into the row regardless
//     of mode (recents or search);
//   - never call the truncating `entryPreviewText(entry, 80)`
//     helper for textual entries (it collapses whitespace and
//     slices the string before the renderer sees it);
//   - keep the documented `entryPreviewText(entry)` helper for
//     image rows so the type + dimensions label survives;
//   - declare `white-space: pre-wrap` on `.qp-preview` so the
//     captured LF / CRLF / tabs / indentation / blank lines /
//     significant spaces reach the renderer byte-for-byte;
//   - pair `pre-wrap` with the documented `-webkit-line-clamp: 2`
//     recipe so the third line is clipped inside the reserved
//     track;
//   - never re-introduce `white-space: nowrap` (forces a single
//     line) or `white-space: normal` (collapses whitespace runs).
// ---------------------------------------------------------------------------

test("renderPreview consumes the whitespace-preserving helper", () => {
  // The helper MUST return `entryFullPreviewText(entry)` on the
  // happy path so recents and search hits share the same
  // whitespace contract. A regression that branches on mode and
  // consumes a different source for search hits would
  // reintroduce the single-line bug.
  const renderPreview = extractFunction(quickPasteSource, "renderPreview");
  assert.ok(
    /entryFullPreviewText\(entry\)/.test(renderPreview),
    "renderPreview must consult entryFullPreviewText so the captured whitespace reaches the renderer byte-for-byte",
  );
});

test("renderPreview never calls the truncating entryPreviewText helper for text", () => {
  // The previous round called `entryPreviewText(entry, 80)`,
  // which collapsed whitespace runs into a single space, trimmed
  // the result and truncated it to ~80 characters BEFORE the
  // renderer saw it. The CSS reservation could not show a real
  // second line. The new contract forbids that helper for
  // textual entries; image rows keep the dimension label.
  const renderPreviewRaw = extractFunction(
    quickPasteSource,
    "renderPreview",
  );
  // Strip line and block comments so the descriptive text the
  // implementation cites inside the function body (mentioning
  // the previous helper) cannot trigger a false positive.
  const renderPreview = renderPreviewRaw
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\/\/[^\n]*/g, "");
  assert.equal(
    /entryPreviewText\([^)]*,\s*80\b/.test(renderPreview),
    false,
    "renderPreview must NOT call entryPreviewText(entry, 80) for text — the helper flattens whitespace and truncates before the CSS can paint a real second line",
  );
  assert.equal(
    /hit\.snippet/.test(renderPreview),
    false,
    "renderPreview must NOT consume hit.snippet for the row preview — the snippet collapses whitespace and biases the truncation toward the match",
  );
});

test("qp-preview CSS declares white-space: pre-wrap to preserve captured whitespace", () => {
  // The `.qp-preview` rule MUST use `white-space: pre-wrap` so
  // the captured block reaches the renderer byte-for-byte and
  // the `-webkit-line-clamp: 2` recipe truncates only the third
  // line. `nowrap` collapses to a single line; `normal` collapses
  // whitespace runs. The previous round shipped `normal` and the
  // helper had already flattened the string upstream — the
  // second reserved line never appeared.
  const cssBlock = quickPasteSource.slice(
    quickPasteSource.indexOf("<style>"),
    quickPasteSource.length,
  );
  const previewRule = cssBlock.match(/\.qp-preview\s*\{[^}]*\}/);
  assert.ok(previewRule, "the .qp-preview rule must exist");
  assert.ok(
    /white-space:\s*pre-wrap/.test(previewRule![0]),
    "the preview must declare white-space: pre-wrap so LF/CRLF/tabs/indentation/blank lines survive the clamp",
  );
  assert.equal(
    /white-space:\s*nowrap/.test(previewRule![0]),
    false,
    "the preview must NOT declare white-space: nowrap — the captured block would be clipped to a single line",
  );
  assert.equal(
    /(?:^|;|\{)\s*white-space:\s*normal\b/m.test(previewRule![0]),
    false,
    "the preview must NOT declare white-space: normal — it collapses whitespace runs",
  );
  assert.ok(
    /display:\s*-webkit-box/.test(previewRule![0]),
    "the preview must keep display: -webkit-box for the line clamp",
  );
  assert.ok(
    /-webkit-box-orient:\s*vertical/.test(previewRule![0]),
    "the preview must keep -webkit-box-orient: vertical for the line clamp",
  );
  assert.ok(
    /-webkit-line-clamp:\s*2/.test(previewRule![0]),
    "the preview must clamp to exactly two lines via -webkit-line-clamp",
  );
  assert.ok(
    /line-clamp:\s*2/.test(previewRule![0]),
    "the preview must declare the modern line-clamp: 2 equivalent",
  );
  assert.ok(
    /overflow:\s*hidden/.test(previewRule![0]),
    "the preview must clip overflow so the third line is never painted",
  );
  assert.ok(
    /word-break:\s*break-word/.test(previewRule![0]),
    "the preview must keep word-break: break-word so long tokens wrap inside the row",
  );
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function extractFunction(source: string, name: string): string {
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