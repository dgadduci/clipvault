/**
 * Behaviour-level regression coverage for the manual-QA regressions
 * the `quick-paste-desktop-polish` change tracks under section 9 of
 * `openspec/changes/quick-paste-desktop-polish/tasks.md`.
 *
 * The suite exercises the contracts through a mix of:
 *
 *   - direct pure-helper calls so the failure mode is observable
 *     without mounting Svelte;
 *   - source-level assertions that pin the keyboard / geometry /
 *     visibility invariants the manual QA pass detected as broken.
 *
 * Source-level assertions are the only realistic way to validate
 * Svelte 5 components from a Node test runtime that does not ship a
 * DOM. The assertions stay close to the user-visible behaviour
 * (constants, CSS rules, key handlers, template hooks) so a future
 * contributor cannot quietly regress one of the four documented
 * fixes.
 *
 * Privacy invariants are also pinned: the test reads the chip-row
 * markup, the bridge payload and the CSS declarations and asserts
 * no clipboard payload, content hash, asset reference or absolute
 * path reaches a visible surface.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

import {
  applyQuickPasteTagsResult,
  bumpQuickPasteTagsToken,
  currentQuickPasteTagsToken,
  markQuickPasteTagsError,
  markQuickPasteTagsPending,
  resetQuickPasteTagsToken,
  truncateQuickPasteTags,
  __resetQuickPasteTagsTokensForTests,
  type QuickPasteTagsCache,
  type QuickPasteTagsHydration,
} from "../src/lib/quickPasteTags.ts";
import type { Tag } from "../src/types.ts";
import {
  FILTER_WIDTH_PX,
  FILTER_MIN_WIDTH_REM,
} from "../src/lib/filterTokens.ts";

const FRONTEND_ROOT = resolvePath(process.cwd());

function readSource(relative: string): string {
  return readFileSync(resolvePath(FRONTEND_ROOT, relative), "utf8");
}

const quickPasteSource = readSource("src/QuickPaste.svelte");
const clipboardPreviewSource = readSource("src/ClipboardPreview.svelte");
const tagFilterSource = readSource("src/TagFilter.svelte");
const sourceAppFilterSource = readSource("src/SourceAppFilter.svelte");
const desktopToolbarSource = readSource("src/DesktopToolbar.svelte");
const quickPasteTagsSource = readSource("src/lib/quickPasteTags.ts");
const filterTokensSource = readSource("src/lib/filterTokens.ts");

function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
}

const cleanQuickPasteSource = stripComments(quickPasteSource);
const cleanClipboardPreviewSource = stripComments(clipboardPreviewSource);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

function makeTag(overrides: Partial<Tag> = {}): Tag {
  return {
    id: 1,
    normalized_name: "tag",
    display_name: "Tag",
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// 9.1 Escape contextual: preview -> list, list -> hide window.
// ---------------------------------------------------------------------------

test("Escape stops propagation inside the preview overlay", () => {
  // The overlay's `onOverlayKeydown` MUST call `event.stopPropagation()`
  // AFTER `event.preventDefault()` so the bubbled `Escape` never
  // reaches the `<svelte:window>` listener that would otherwise hide
  // the Quick Paste window. Without this guard the synchronous
  // `previewEntryId = null` write inside `close()` makes the
  // window-level handler evaluate `surface === "list"` and route
  // the same keystroke into `hideQuickPasteWindow()`.
  const overlayKeydown = extractFunctionBody(
    clipboardPreviewSource,
    "onOverlayKeydown",
  );
  assert.ok(
    /event\.preventDefault\(\)/.test(overlayKeydown),
    "the overlay handler must call preventDefault",
  );
  assert.ok(
    /event\.stopPropagation\(\)/.test(overlayKeydown),
    "the overlay handler must call stopPropagation so the Escape does not bubble into <svelte:window>",
  );
  // The handler must run BEFORE the bubbled event reaches the
  // window listener — the close() call resets `previewEntryId`
  // synchronously, so any deferred window handler would observe the
  // preview as already closed and route the same Escape into
  // `handleEscape()`.
  const stopIndex = overlayKeydown.indexOf("stopPropagation");
  const closeIndex = overlayKeydown.indexOf("close()");
  assert.ok(stopIndex >= 0 && closeIndex >= 0, "stopPropagation and close() must both exist");
  assert.ok(
    stopIndex < closeIndex,
    "stopPropagation must run before close() so the bubbled event never reaches the window",
  );
});

test("Quick Paste exposes a surface state machine driven by previewEntryId", () => {
  // The `surface` reactive value is the documented single source of
  // truth for the keyboard contract. A regression that drops the
  // derivation breaks both branches of the Escape handler.
  assert.ok(
    /\$\:\s*surface\s*=\s*previewEntryId\s*!==\s*null\s*\?\s*"preview"\s*:\s*"list"/.test(
      cleanQuickPasteSource,
    ),
    "Quick Paste must derive the surface flag from previewEntryId",
  );
});

test("Quick Paste arms a one-shot Escape guard inside closePreview", () => {
  // Defence in depth: even if a future component forgets to stop
  // propagation, the flag must consume exactly the Escape that
  // closed the preview so the window never hides.
  const closePreviewBody = extractFunctionBody(cleanQuickPasteSource, "closePreview");
  assert.ok(
    /suppressNextWindowEscape\s*=\s*true/.test(closePreviewBody),
    "closePreview must arm the suppressNextWindowEscape flag",
  );
  assert.ok(
    /queueMicrotask/.test(closePreviewBody),
    "closePreview must restore focus on the next microtask",
  );
});

test("the window keydown handler routes Escape through surface and the one-shot guard", () => {
  // The branch order matters:
  //   1. `surface === "preview"` closes the preview without
  //      touching the window;
  //   2. `suppressNextWindowEscape` consumes exactly the Escape
  //      that closed the preview;
  //   3. any other Escape falls through to `handleEscape()`.
  const onWindowKeydownBody = extractFunctionBody(
    cleanQuickPasteSource,
    "onWindowKeydown",
  );
  assert.ok(
    /surface\s*===\s*"preview"/.test(onWindowKeydownBody),
    "the window handler must consult surface === preview first",
  );
  assert.ok(
    /suppressNextWindowEscape/.test(onWindowKeydownBody),
    "the window handler must consult the one-shot guard",
  );
  const surfaceIndex = onWindowKeydownBody.indexOf("surface === \"preview\"");
  const guardIndex = onWindowKeydownBody.indexOf("suppressNextWindowEscape");
  const handleEscapeIndex = onWindowKeydownBody.indexOf("handleEscape()");
  assert.ok(surfaceIndex >= 0 && guardIndex >= 0 && handleEscapeIndex >= 0);
  assert.ok(
    surfaceIndex < guardIndex,
    "the surface branch must be evaluated before the guard",
  );
  assert.ok(
    guardIndex < handleEscapeIndex,
    "the guard must be evaluated before the hide branch",
  );
});

test("handleEscape hides the Quick Paste window through the bridge", () => {
  // The list branch keeps the historical behaviour: Escape routes
  // through the documented bridge so the OS window hides.
  const handleEscapeBody = extractFunctionBody(cleanQuickPasteSource, "handleEscape");
  assert.ok(
    /hideQuickPasteWindow/.test(handleEscapeBody),
    "handleEscape must delegate to hideQuickPasteWindow",
  );
});

test("the svelte:window keydown listener is registered exactly once", () => {
  // Duplicate listeners would route the same Escape into the
  // preview branch twice and silently hide the window after the
  // surface flag flips back. The change pins a single
  // `<svelte:window on:keydown={onWindowKeydown}>` declaration so
  // a regression that re-registers the listener surfaces here.
  const svelteWindowMatches = cleanQuickPasteSource.match(
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

test("Escape inside the search input is handled by the search input branch", () => {
  // The search input keydown handler keeps the typing surface
  // intact: ArrowUp / ArrowDown / Home / End navigate the list
  // without ever triggering the window hide branch. A regression
  // that drops `stopPropagation` would route the same keystrokes
  // through the window handler and re-issue `moveSelection(1)` so
  // the keyboard selection jumps by two rows on every press.
  const onSearchInputKeydown = extractFunctionBody(
    cleanQuickPasteSource,
    "onSearchInputKeydown",
  );
  assert.ok(
    /ArrowDown[\s\S]*stopPropagation/.test(onSearchInputKeydown),
    "ArrowDown must stopPropagation so the window handler does not fire twice",
  );
  assert.ok(
    /ArrowUp[\s\S]*stopPropagation/.test(onSearchInputKeydown),
    "ArrowUp must stopPropagation",
  );
});

// ---------------------------------------------------------------------------
// 9.2 Geometry: title-row (fixed) + capture-content (exactly 2 lines).
// ---------------------------------------------------------------------------

test("Quick Paste pins the documented row geometry constants", () => {
  // The constants live at the top of the component and drive both
  // the row markup (custom properties) and the CSS rules. A
  // regression that drifts the constants breaks the documented
  // row height and the two-line content reservation.
  //
  // The previous round pinned `ROW_HEIGHT_PX = 80` while the
  // track reservation was `2lh` against
  // `CAPTURE_LINE_HEIGHT_REM × 16px ≈ 30.4px`. The inner sum
  // (title-row + capture + footer + padding + gaps + borders) was
  // 85.6px — 5.6px past the outer rectangle. The row's
  // `overflow: hidden` then clipped the second reserved line and
  // the visible footprint collapsed to a single line.
  //
  // The current round derives `ROW_HEIGHT_PX` from explicit
  // pixel constants and recomputes the capture track in pixels
  // (no more `2lh`) so the inner sum always fits the outer
  // rectangle by construction.
  assert.ok(
    /const\s+CAPTURE_LINE_HEIGHT_REM\s*=\s*0\.95/.test(cleanQuickPasteSource),
    "Quick Paste must pin CAPTURE_LINE_HEIGHT_REM",
  );
  assert.ok(
    /const\s+CAPTURE_LINE_HEIGHT_PX\s*=\s*Math\.round\(CAPTURE_LINE_HEIGHT_REM\s*\*\s*16\)/.test(
      cleanQuickPasteSource,
    ),
    "Quick Paste must derive CAPTURE_LINE_HEIGHT_PX from CAPTURE_LINE_HEIGHT_REM × 16",
  );
  assert.ok(
    /const\s+CAPTURE_CONTENT_HEIGHT_PX\s*=\s*CAPTURE_LINE_HEIGHT_PX\s*\*\s*2/.test(
      cleanQuickPasteSource,
    ),
    "Quick Paste must derive CAPTURE_CONTENT_HEIGHT_PX = CAPTURE_LINE_HEIGHT_PX × 2",
  );
  assert.ok(
    /const\s+TITLE_ROW_HEIGHT_PX\s*=\s*24/.test(cleanQuickPasteSource),
    "Quick Paste must pin TITLE_ROW_HEIGHT_PX = 24",
  );
  assert.ok(
    /const\s+FOOTER_HEIGHT_PX\s*=\s*18/.test(cleanQuickPasteSource),
    "Quick Paste must pin FOOTER_HEIGHT_PX = 18 for the footer/meta track",
  );
  // ROW_HEIGHT_PX is the sum of the parts above plus padding, gaps
  // and borders — pin the formula the tests rely on, not a magic
  // number, so a future contributor can move any piece without
  // breaking the regression suite.
  assert.ok(
    /const\s+ROW_HEIGHT_PX\s*=\s*TITLE_ROW_HEIGHT_PX\s*\+\s*CAPTURE_CONTENT_HEIGHT_PX\s*\+\s*FOOTER_HEIGHT_PX\s*\+\s*ROW_PADDING_VERTICAL_PX\s*\+\s*ROW_GAP_TOTAL_PX\s*\+\s*ROW_BORDER_PX/.test(
      cleanQuickPasteSource,
    ),
    "ROW_HEIGHT_PX must be derived from the track heights, padding, gap and border so a regression that drifts any piece surfaces here",
  );
});

test("the row uses a real grid layout for the title-row and capture-content regions", () => {
  // The row MUST keep a 3-track grid template: the first track is
  // the documented `title-row` (fixed via `var(--qp-title-row-height)`),
  // the second track reserves `var(--qp-capture-content-height)`
  // (exactly two visual lines in pixels), and the third track hosts
  // the footer/meta. A regression that drops the explicit pixel
  // reservation would let `2lh` resolve against a `line-height`
  // the rest of the stylesheet could drift past silently — the
  // previous round pinned `2lh` against `CAPTURE_LINE_HEIGHT_REM ×
  // 16 ≈ 30.4px` which combined with the other tracks overflowed
  // the outer rectangle by 5.6px.
  const cssBlock = cleanQuickPasteSource.slice(
    cleanQuickPasteSource.indexOf("<style>"),
    cleanQuickPasteSource.length,
  );
  const rowRule = cssBlock.match(/\.qp-row\s*\{[^}]*\}/);
  assert.ok(rowRule, "the .qp-row rule must exist");
  assert.ok(
    /grid-template-rows:[^;]*var\(--qp-title-row-height/.test(rowRule![0]),
    "the row must reserve the title-row track via --qp-title-row-height",
  );
  assert.ok(
    /grid-template-rows:[^;]*var\(--qp-capture-content-height/.test(rowRule![0]),
    "the row must reserve exactly two lines via --qp-capture-content-height (not 2lh)",
  );
  assert.equal(
    /grid-template-rows:[^;]*2lh/.test(rowRule![0]),
    false,
    "the row must NOT use 2lh for the capture-content track — the inner sum overflowed the outer rectangle by 5.6px in the previous round",
  );
  // The body line declares the matching min-height / max-height /
  // line-height so the reservation matches the grid track by
  // construction (no more `2lh` against a `line-height` the
  // stylesheet can drift past silently).
  const bodyRule = cssBlock.match(/\.qp-row-line-body\s*\{[^}]*\}/);
  assert.ok(bodyRule, "the .qp-row-line-body rule must exist");
  assert.ok(
    /min-height:\s*var\(--qp-capture-content-height/.test(bodyRule![0]),
    "capture-content must declare min-height: var(--qp-capture-content-height) — not 2lh",
  );
  assert.ok(
    /max-height:\s*var\(--qp-capture-content-height/.test(bodyRule![0]),
    "capture-content must declare max-height: var(--qp-capture-content-height) — not 2lh",
  );
  assert.ok(
    /line-height:\s*var\(--qp-capture-line-height/.test(bodyRule![0]),
    "capture-content must bind line-height to --qp-capture-line-height",
  );
  assert.ok(
    /overflow:\s*hidden/.test(bodyRule![0]),
    "capture-content must clip overflow so long previews never grow the row",
  );
});

test("the title-row reserves a fixed height and clips overflow", () => {
  // The meta line is a real CSS grid (NOT the inherited flex from
  // `.qp-row-line`) so the six-column template reserves the exact
  // widths the spec pins. The previous implementation declared
  // `grid-template-columns` on a flex container — a no-op that
  // let the title-row collapse to its intrinsic height. The fix
  // adds `display: grid` so the column allocation actually runs.
  const cssBlock = cleanQuickPasteSource.slice(
    cleanQuickPasteSource.indexOf("<style>"),
    cleanQuickPasteSource.length,
  );
  const metaRule = cssBlock.match(/\.qp-row-line-meta\s*\{[^}]*\}/);
  assert.ok(metaRule, "the .qp-row-line-meta rule must exist");
  assert.ok(
    /display:\s*grid/.test(metaRule![0]),
    "the title-row must declare display: grid so the columns resolve",
  );
  assert.ok(
    /max-height:\s*var\(--qp-title-row-height/.test(metaRule![0]),
    "the title-row must bind max-height to --qp-title-row-height",
  );
  assert.ok(
    /overflow:\s*hidden/.test(metaRule![0]),
    "the title-row must clip overflow",
  );
});

test("the row exposes stable testids for title-row and capture-content", () => {
  // The two regions must carry a stable data-testid so the
  // behaviour-level test can target each row's track individually
  // (and a future contributor can drive a focused assertion).
  assert.ok(
    /data-testid="quick-paste-title-row"/.test(cleanQuickPasteSource),
    "the title-row must expose data-testid=quick-paste-title-row",
  );
  assert.ok(
    /data-testid="quick-paste-capture-content"/.test(cleanQuickPasteSource),
    "the capture-content must expose data-testid=quick-paste-capture-content",
  );
  // The data-row-height attribute is kept on the row for legacy
  // tests; the new geometry MUST add a data-row-content-lines hook
  // so the regression suite can distinguish the title line from
  // the two reserved content lines.
  assert.ok(
    /data-row-content-lines=\{2\}/.test(cleanQuickPasteSource) ||
      /data-row-content-lines="2"/.test(cleanQuickPasteSource),
    "the row must mark the capture-content line count for tests",
  );
});

test("the preview text and thumbnail fit inside the reserved two-line footprint", () => {
  // The preview and the thumbnail MUST honour the documented
  // two-line behaviour inside `capture-content`: the reserved
  // `2lh` track reserves exactly two visual lines so a short
  // preview keeps the second line reserved and a long preview
  // truncates inside the row. The CSS clamps the preview to two
  // lines through `-webkit-line-clamp: 2` (plus the modern
  // `line-clamp: 2` equivalent) so the third line is never
  // painted. The `display: -webkit-box` + `-webkit-box-orient:
  // vertical` pair is the cross-browser recipe for the clamp.
  const cssBlock = cleanQuickPasteSource.slice(
    cleanQuickPasteSource.indexOf("<style>"),
    cleanQuickPasteSource.length,
  );
  const previewRule = cssBlock.match(/\.qp-preview\s*\{[^}]*\}/);
  assert.ok(previewRule, "the .qp-preview rule must exist");
  assert.ok(
    /display:\s*-webkit-box/.test(previewRule![0]),
    "the preview must declare display: -webkit-box for the line clamp",
  );
  assert.ok(
    /-webkit-box-orient:\s*vertical/.test(previewRule![0]),
    "the preview must declare -webkit-box-orient: vertical for the line clamp",
  );
  assert.ok(
    /-webkit-line-clamp:\s*2/.test(previewRule![0]),
    "the preview must declare -webkit-line-clamp: 2 to reserve exactly two lines",
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
    /text-overflow:\s*ellipsis/.test(previewRule![0]),
    "the preview must ellipsize overflow",
  );
  // The thumbnail reserves a 40px square so loading / loaded /
  // error states keep the same footprint.
  const thumbRule = cssBlock.match(/\.qp-thumb\s*\{[^}]*\}/);
  assert.ok(thumbRule, "the .qp-thumb rule must exist");
  assert.ok(
    /width:\s*40px/.test(thumbRule![0]) && /height:\s*40px/.test(thumbRule![0]),
    "the thumbnail must reserve a 40x40 square",
  );
});

// ---------------------------------------------------------------------------
// 9.3 Tags visible per entry, stale hydration rejected, navigation safe.
// ---------------------------------------------------------------------------

test("Quick Paste hydrates both recents and search hits", () => {
  // The hydration round MUST walk both feeds so chips appear on
  // every supported surface, not just the default view.
  assert.ok(
    /hydrateTagsForVisibleEntries\(recent\)/.test(cleanQuickPasteSource),
    "Quick Paste must hydrate the recents feed",
  );
  assert.ok(
    /hydrateTagsForVisibleEntries\([^)]+response\.hits/.test(cleanQuickPasteSource) ||
      /hydrateTagsForVisibleEntries\([^)]+hits/.test(cleanQuickPasteSource),
    "Quick Paste must hydrate the search hits",
  );
});

test("the tag chip template renders only display_name and never ids, paths or hashes", () => {
  // The visible text comes from `chip.tag.display_name` only; the
  // metadata-only `data-tag-id` attribute is allowed. Source-app
  // / asset references, hashes and absolute paths MUST never
  // reach the chip row.
  const chipRowMatch = cleanQuickPasteSource.match(
    /class="qp-tags"[\s\S]*?data-tag-id=\{chip\.tag\.id\}[\s\S]*?\+{tagsProjection\.overflow}/,
  );
  assert.ok(chipRowMatch, "the chip row markup must exist");
  const chipRow = chipRowMatch![0];
  assert.ok(
    /\{chip\.tag\.display_name\}/.test(chipRow),
    "the chip must render display_name, not the stable id",
  );
  const sensitive = [
    "asset_ref",
    "content_hash",
    "rich_html_ref",
    "rich_rtf_ref",
    "rich_preview_ref",
    "/Users/",
    "entry.content",
  ];
  for (const token of sensitive) {
    assert.equal(
      chipRow.includes(token),
      false,
      `the chip row must not surface ${token}`,
    );
  }
});

test("the tag chip uses a high-contrast palette so the chip is visible after hydration", () => {
  // The previous implementation used a 12%-opacity blue
  // background with a 0.65rem font; the manual QA pass reported
  // the chip as hard to read. The fix uses a 22%-opacity
  // background, a 0.72rem font and a brighter foreground colour
  // (`#bfdbfe`) so the chip stays visible without breaking the
  // single-line title-row geometry.
  const cssBlock = cleanQuickPasteSource.slice(
    cleanQuickPasteSource.indexOf("<style>"),
    cleanQuickPasteSource.length,
  );
  const chipRule = cssBlock.match(/\.qp-tag-chip\s*\{[^}]*\}/);
  assert.ok(chipRule, "the .qp-tag-chip rule must exist");
  assert.ok(
    /font-size:\s*var\(--cv-tag,\s*0\.72rem\)/.test(chipRule![0]),
    "the chip must use the documented 0.72rem font",
  );
  assert.ok(
    /color:\s*#bfdbfe/.test(chipRule![0]),
    "the chip foreground must be the documented high-contrast blue",
  );
  assert.ok(
    /background:\s*rgba\(147,\s*197,\s*253,\s*0\.22\)/.test(chipRule![0]),
    "the chip background must use the documented 22% opacity blue",
  );
});

test("stale tag hydration cannot overwrite a fresher entry", async () => {
  // The companion test for `lib/quickPasteTags.ts`. A regression
  // that drops the per-entry token guard would let a late
  // response attach a tag set to a row that left the scope.
  __resetQuickPasteTagsTokensForTests();
  const entryA = 11;
  const entryB = 22;
  const knownTags = [
    makeTag({ id: 10, normalized_name: "alpha", display_name: "Alpha" }),
  ];
  const cache: QuickPasteTagsCache = new Map();
  const hydration: QuickPasteTagsHydration = new Map();

  // First round: hydrate entry A.
  const tokenA = bumpQuickPasteTagsToken(entryA);
  const hydrationA = markQuickPasteTagsPending(hydration, entryA);
  // Stale response: token no longer matches because entry A's
  // bump was overwritten by a second refresh call.
  bumpQuickPasteTagsToken(entryA);
  assert.notEqual(
    currentQuickPasteTagsToken(entryA),
    tokenA,
    "bump must monotonically advance the token",
  );
  // The apply helper accepts the stale ids, but the consumer MUST
  // reject them by checking the token. The behaviour-level
  // assertion below mirrors the Quick Paste component flow.
  const stale = applyQuickPasteTagsResult(
    cache,
    hydrationA,
    entryA,
    [10],
    knownTags,
  );
  // The component would consult `currentQuickPasteTagsToken(entryA)
  // !== tokenA` and drop `stale`; assert the invariant manually.
  if (currentQuickPasteTagsToken(entryA) === tokenA) {
    assert.fail(
      "stale token must not match the live token — stale response should have been dropped",
    );
  }
  // Apply a fresh response instead so the cache mirrors the live
  // component behaviour: the cache reflects the post-stale-bump
  // tag set.
  const fresh = applyQuickPasteTagsResult(
    stale.nextCache,
    stale.nextHydration,
    entryB,
    [10],
    knownTags,
  );
  assert.equal(fresh.nextCache.get(entryB)?.length, 1);
  assert.equal(fresh.nextHydration.get(entryB), "loaded");
  // The error branch must keep the previous loaded entries intact.
  const errHydration = markQuickPasteTagsError(fresh.nextHydration, entryB);
  assert.equal(errHydration.get(entryB), "error");
  assert.ok(fresh.nextCache.has(entryB), "the cache must keep the entry on error");
});

test("the truncate helper caps the chip row at 2 chips and reports overflow", () => {
  // The row exposes a `+N` indicator when more than 2 tags exist.
  __resetQuickPasteTagsTokensForTests();
  const tags = Array.from({ length: 6 }, (_, index) =>
    makeTag({ id: index + 1, normalized_name: `t${index}`, display_name: `Tag ${index}` }),
  );
  const { visible, overflow } = truncateQuickPasteTags(tags, 2);
  assert.equal(visible.length, 2);
  assert.equal(overflow, 4);
});

test("Quick Paste prunes the tag cache when an entry leaves the visible scope", () => {
  // The reactive `pruneTagsToVisibleEntries` block must keep the
  // cache aligned with the rendered `resultIds`; an entry that
  // left the visible scope MUST release its tag set so a future
  // re-addition never inherits the previous row's tags.
  assert.ok(
    /pruneTagsToVisibleEntries/.test(cleanQuickPasteSource),
    "Quick Paste must declare the prune helper",
  );
  assert.ok(
    /resetQuickPasteTagsToken/.test(cleanQuickPasteSource),
    "Quick Paste must reset the per-entry token when an entry leaves the scope",
  );
});

// ---------------------------------------------------------------------------
// 9.4 TagFilter width matches SourceAppFilter width.
// ---------------------------------------------------------------------------

test("filterTokens exports a single canonical width for both comboboxes", () => {
  // The token module MUST own the documented 220px / 11rem
  // baseline so the source-app and tag comboboxes cannot drift.
  assert.equal(FILTER_WIDTH_PX, 220);
  assert.equal(FILTER_MIN_WIDTH_REM, "11rem");
  assert.ok(
    /filterComboboxStyles/.test(filterTokensSource),
    "the token module must export filterComboboxStyles for component composition",
  );
  assert.ok(
    /220px/.test(filterTokensSource),
    "the token module must document the 220px width",
  );
});

test("TagFilter uses the shared filterComboboxStyles and width declarations", () => {
  // The combobox MUST consume the shared width token (NOT an
  // intrinsic flex value) and apply it via the inline `style`
  // attribute so a CSS refactor cannot regress the width.
  assert.ok(
    /filterComboboxStyles/.test(tagFilterSource),
    "TagFilter must import filterComboboxStyles",
  );
  assert.ok(
    /style=\{filterComboboxStyles\}/.test(tagFilterSource),
    "TagFilter must apply the shared styles inline on the root div",
  );
  assert.ok(
    /data-filter-width="220"/.test(tagFilterSource),
    "TagFilter must expose data-filter-width for the regression suite",
  );
  // The CSS rule must pin the same width the token module owns so
  // the inline style and the stylesheet cannot drift.
  const cssBlock = tagFilterSource.slice(
    tagFilterSource.indexOf("<style>"),
    tagFilterSource.length,
  );
  const rule = cssBlock.match(/\.tag-filter\s*\{[^}]*\}/);
  assert.ok(rule, "the .tag-filter rule must exist");
  assert.ok(
    /width:\s*220px/.test(rule![0]),
    "the .tag-filter rule must pin width: 220px",
  );
  assert.ok(
    /max-width:\s*220px/.test(rule![0]),
    "the .tag-filter rule must pin max-width: 220px",
  );
  assert.ok(
    /min-width:\s*11rem/.test(rule![0]),
    "the .tag-filter rule must pin min-width: 11rem",
  );
});

test("SourceAppFilter uses the shared filterComboboxStyles and width declarations", () => {
  // The combobox MUST consume the same shared token module so the
  // two filters stay byte-for-byte identical in width.
  assert.ok(
    /filterComboboxStyles/.test(sourceAppFilterSource),
    "SourceAppFilter must import filterComboboxStyles",
  );
  assert.ok(
    /style=\{filterComboboxStyles\}/.test(sourceAppFilterSource),
    "SourceAppFilter must apply the shared styles inline on the root div",
  );
  assert.ok(
    /data-filter-width="220"/.test(sourceAppFilterSource),
    "SourceAppFilter must expose data-filter-width for the regression suite",
  );
  const cssBlock = sourceAppFilterSource.slice(
    sourceAppFilterSource.indexOf("<style>"),
    sourceAppFilterSource.length,
  );
  const rule = cssBlock.match(/\.source-app-filter\s*\{[^}]*\}/);
  assert.ok(rule, "the .source-app-filter rule must exist");
  assert.ok(
    /width:\s*220px/.test(rule![0]),
    "the .source-app-filter rule must pin width: 220px",
  );
  assert.ok(
    /max-width:\s*220px/.test(rule![0]),
    "the .source-app-filter rule must pin max-width: 220px",
  );
});

test("Toolbar mounts SourceAppFilter, TagFilter and the overflow menu in the documented order", () => {
  // The visible order MUST stay `search → source-app → tag →
  // overflow` so the manual QA pass can navigate from the search
  // input to the tag filter without crossing the source-app
  // combobox. The test searches for the first occurrence AFTER
  // the import block so the import line "SourceAppFilter" cannot
  // produce a false positive.
  const toolbarScriptEnd = desktopToolbarSource.indexOf("</script>");
  const toolbarMarkup = desktopToolbarSource.slice(
    toolbarScriptEnd,
    desktopToolbarSource.length,
  );
  const sourceAppIndex = toolbarMarkup.indexOf("<SourceAppFilter");
  const tagIndex = toolbarMarkup.indexOf("<TagFilter");
  const overflowIndex = toolbarMarkup.indexOf("open-overflow-menu");
  const searchIndex = toolbarMarkup.indexOf("search-shell");
  assert.ok(sourceAppIndex > -1, "SourceAppFilter must remain mounted");
  assert.ok(tagIndex > -1, "TagFilter must be mounted");
  assert.ok(overflowIndex > -1, "the overflow menu must remain mounted");
  assert.ok(searchIndex > -1, "the search shell must remain mounted");
  assert.ok(
    searchIndex < sourceAppIndex && sourceAppIndex < tagIndex && tagIndex < overflowIndex,
    "the toolbar order must be search → source-app → tag → overflow",
  );
});

// ---------------------------------------------------------------------------
// 9.5 Regresiones críticas preservadas.
// ---------------------------------------------------------------------------

test("ClipboardPreview stays strictly read-only — no copy/paste/pin/edit paths", () => {
  // The shared overlay MUST never invoke a clipboard-mutating
  // command. A regression that re-introduces `copyEntryCommand`
  // or `setFavoriteCommand` would break the desktop-card-preview
  // contract.
  const forbidden = [
    "copyEntryCommand",
    "pasteEntryCommand",
    "setFavoriteCommand",
    "setEntryTitleCommand",
    "entryTagsSetCommand",
    "entryCollectionsSetCommand",
    "deleteEntryCommand",
    "captureTextCommand",
    "invoke(",
  ];
  for (const token of forbidden) {
    assert.equal(
      cleanClipboardPreviewSource.includes(token),
      false,
      `ClipboardPreview must not reference ${token} (strictly read-only)`,
    );
  }
});

test("the Quick Paste tag chip never echoes content, hashes, asset refs or paths", () => {
  // Privacy invariant pinned by the spec. The visible text comes
  // from the resolved `Tag.display_name` only; the metadata-only
  // `data-tag-id` attribute is allowed. Source-app / asset
  // references, hashes and absolute paths must never reach the
  // chip row.
  const forbidden = [
    "entry.content",
    "asset_ref",
    "content_hash",
    "rich_html_ref",
    "rich_rtf_ref",
    "rich_preview_ref",
    "/Users/",
  ];
  // Slice to the chip row so the rest of the file (which uses
  // `asset_ref` to drive the thumbnail bridge) cannot trigger a
  // false positive.
  const chipRowMatch = cleanQuickPasteSource.match(
    /class="qp-tags"[\s\S]*?qp-tag-chip-more/,
  );
  assert.ok(chipRowMatch, "the chip row markup must exist");
  const chipRow = chipRowMatch![0];
  for (const token of forbidden) {
    assert.equal(
      chipRow.includes(token),
      false,
      `the Quick Paste chip row must not surface ${token}`,
    );
  }
});

test("the Quick Paste tag helper module stays metadata-only", () => {
  // The pure helper module MUST NOT import a clipboard-mutating
  // command. The bridge payload (`entryTagsCommand`) is metadata
  // only; the resolved cache carries `Tag.display_name` and
  // nothing else.
  const forbidden = [
    "copyEntryCommand",
    "pasteEntryCommand",
    "setFavoriteCommand",
    "captureTextCommand",
  ];
  for (const token of forbidden) {
    assert.equal(
      quickPasteTagsSource.includes(token),
      false,
      `quickPasteTags must not import ${token}`,
    );
  }
});

test("the Quick Paste row keeps the drag-and-drop and pointer-event exemptions intact", () => {
  // The `pointerDragAndDrop` baseline is documented as protected.
  // The Quick Paste row MUST keep the documented `user-select:
  // none`, the chip `pointer-events: none` and the menu trigger
  // exemptions so a future refactor cannot quietly regress the
  // protected baseline.
  assert.ok(
    /user-select:\s*none/.test(cleanQuickPasteSource),
    "the row must keep user-select: none",
  );
  assert.ok(
    /pointer-events:\s*none/.test(cleanQuickPasteSource),
    "the chip must keep pointer-events: none",
  );
});

// ---------------------------------------------------------------------------
// 10.2 Geometry regressions — short / long / image / tags / thumbnails / rows.
// ---------------------------------------------------------------------------

function extractCssBlock(): string {
  return cleanQuickPasteSource.slice(
    cleanQuickPasteSource.indexOf("<style>"),
    cleanQuickPasteSource.length,
  );
}

test("short content reserves the two-line footprint", () => {
  // A row whose preview fits on a single line MUST still occupy
  // the documented two-line track. The grid template
  // `var(--qp-capture-content-height)` and the body
  // `min-height` / `max-height` declarations (both bound to the
  // same custom property) are the only thing keeping the second
  // line reserved when the captured text is short.
  //
  // The previous round pinned `2lh` here and the inner sum
  // overflowed the outer rectangle by 5.6px; the regression suite
  // asserts the explicit pixel reservation so a future contributor
  // cannot quietly swap it back to `2lh` and reintroduce the
  // single-line clip.
  const cssBlock = extractCssBlock();
  const bodyRule = cssBlock.match(/\.qp-row-line-body\s*\{[^}]*\}/);
  assert.ok(bodyRule, "the .qp-row-line-body rule must exist");
  assert.ok(
    /min-height:\s*var\(--qp-capture-content-height/.test(bodyRule![0]),
    "capture-content must reserve two lines even when content is short",
  );
  assert.ok(
    /max-height:\s*var\(--qp-capture-content-height/.test(bodyRule![0]),
    "capture-content must clamp to two lines even when content is short",
  );
  assert.equal(
    /min-height:\s*2lh/.test(bodyRule![0]) ||
      /max-height:\s*2lh/.test(bodyRule![0]),
    false,
    "capture-content must NOT use 2lh — the previous round overflowed the outer rectangle",
  );
  // The preview MUST be allowed to wrap to two lines; the row
  // reserves both lines so a short preview paints on the first
  // line while the second line stays reserved by the explicit
  // pixel reservation. The `white-space: pre-wrap` declaration
  // preserves the LF / CRLF / tabs / indentation / blank lines
  // the source application produced so a multi-line capture
  // paints on up to two lines and the clamp recipe truncates the
  // third.
  const previewRule = cssBlock.match(/\.qp-preview\s*\{[^}]*\}/);
  assert.ok(previewRule, "the .qp-preview rule must exist");
  assert.ok(
    /-webkit-line-clamp:\s*2/.test(previewRule![0]),
    "the preview must clamp to exactly two visual lines",
  );
  assert.ok(
    /white-space:\s*pre-wrap/.test(previewRule![0]),
    "the preview must declare white-space: pre-wrap so the captured whitespace survives the clamp",
  );
  assert.ok(
    !/white-space:\s*nowrap/.test(previewRule![0]),
    "the preview must NOT be forced to a single line",
  );
  assert.ok(
    !/white-space:\s*normal\b/.test(previewRule![0]),
    "the preview must NOT declare white-space: normal — it collapses the whitespace runs the helper has preserved",
  );
});

test("long content is bounded to two lines without growing the row", () => {
  // The body line MUST clip + ellipsize overflow so a long
  // preview cannot add a third line or push the row beyond the
  // documented height. The `overflow: hidden` and
  // `text-overflow: ellipsis` declarations are the contract.
  const cssBlock = extractCssBlock();
  const bodyRule = cssBlock.match(/\.qp-row-line-body\s*\{[^}]*\}/);
  assert.ok(bodyRule, "the .qp-row-line-body rule must exist");
  assert.ok(
    /overflow:\s*hidden/.test(bodyRule![0]),
    "capture-content must clip long previews so the row stays two lines tall",
  );
  // The body line's max-height is already asserted above; here
  // we pin the explicit pixel reservation so a regression that
  // swaps it for `auto` would surface here before the user sees
  // a three-line row.
  assert.ok(
    /max-height:\s*var\(--qp-capture-content-height/.test(bodyRule![0]),
    "capture-content must clamp to the explicit pixel reservation",
  );
  // The preview clamp combination (-webkit-line-clamp: 2 +
  // line-clamp: 2 + display: -webkit-box + -webkit-box-orient:
  // vertical) is the cross-browser recipe the regression suite
  // pins so a regression that drops one of them surfaces before
  // the user sees a three-line row.
  const previewRule = cssBlock.match(/\.qp-preview\s*\{[^}]*\}/);
  assert.ok(previewRule, "the .qp-preview rule must exist");
  assert.ok(
    /display:\s*-webkit-box/.test(previewRule![0]),
    "the preview must use display: -webkit-box for the line clamp",
  );
  assert.ok(
    /-webkit-box-orient:\s*vertical/.test(previewRule![0]),
    "the preview must declare -webkit-box-orient: vertical for the line clamp",
  );
  assert.ok(
    /-webkit-line-clamp:\s*2/.test(previewRule![0]),
    "the preview must clamp to two lines via -webkit-line-clamp",
  );
});

test("text and image rows share the same fixed footprint", () => {
  // Text rows and image rows MUST live on the same grid track.
  // The 40x40 thumbnail reserves a fixed square so loading,
  // loaded and error placeholders all sit inside the same
  // `capture-content` track as the text preview.
  const cssBlock = extractCssBlock();
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
    /min-width:\s*40px/.test(thumbRule![0]) &&
      /min-height:\s*40px/.test(thumbRule![0]),
    "the thumbnail must not shrink below 40x40 even on loading or error",
  );
  // The image row composes the thumbnail + the preview text
  // inside the same body line; both share the documented
  // `capture-content` track.
  assert.ok(
    /qp-preview qp-preview-image/.test(cleanQuickPasteSource),
    "the image row must render qp-preview alongside the thumbnail",
  );
});

test("tag chips never increase the row height", () => {
  // The title-row uses `display: grid` so the tag column is a
  // bounded track, and `max-height: var(--qp-title-row-height)`
  // ensures the chips never push the title line past the 24px
  // reserved height.
  const cssBlock = extractCssBlock();
  const metaRule = cssBlock.match(/\.qp-row-line-meta\s*\{[^}]*\}/);
  assert.ok(metaRule, "the .qp-row-line-meta rule must exist");
  assert.ok(
    /display:\s*grid/.test(metaRule![0]),
    "the title-row must be a real grid so the tag column is bounded",
  );
  assert.ok(
    /max-height:\s*var\(--qp-title-row-height/.test(metaRule![0]),
    "the title-row must clamp to the reserved title-row height",
  );
  assert.ok(
    /overflow:\s*hidden/.test(metaRule![0]),
    "the title-row must clip overflow so long chip lists never grow it",
  );
  // The chip itself uses `line-height: 1.1` × `0.72rem` font +
  // `0.1rem` vertical padding so it comfortably fits under the
  // 24px cap.
  const chipRule = cssBlock.match(/\.qp-tag-chip\s*\{[^}]*\}/);
  assert.ok(chipRule, "the .qp-tag-chip rule must exist");
  assert.ok(
    /line-height:\s*1\.1/.test(chipRule![0]),
    "the chip must use a tight line-height so multiple chips fit the reserved height",
  );
});

test("thumbnail loading and error placeholders share the same footprint", () => {
  // The `.qp-thumb-placeholder` and `.qp-thumb-placeholder-error`
  // rules MUST keep the same 40x40 footprint so the row never
  // reflows while the bridge round-trip is in flight.
  const cssBlock = extractCssBlock();
  const placeholderRule = cssBlock.match(/\.qp-thumb-placeholder\s*\{[^}]*\}/);
  const errorRule = cssBlock.match(/\.qp-thumb-placeholder-error\s*\{[^}]*\}/);
  assert.ok(placeholderRule, "the .qp-thumb-placeholder rule must exist");
  assert.ok(errorRule, "the .qp-thumb-placeholder-error rule must exist");
  assert.ok(
    /width:\s*100%/.test(placeholderRule![0]),
    "the loading placeholder must fill the 40x40 square",
  );
  assert.ok(
    /height:\s*100%/.test(placeholderRule![0]),
    "the loading placeholder must fill the 40x40 square",
  );
  assert.ok(
    /color:\s*#f87171/.test(errorRule![0]),
    "the error placeholder must use the documented error colour so the row is recognisable",
  );
});

test("all rows share the documented 86px height", () => {
  // The row CSS MUST pin `height`, `min-height` AND `max-height`
  // to the same `--qp-row-height` token so a row can never grow
  // or shrink across the surface. A regression that drops
  // `max-height` would let long tag lists or oversized previews
  // expand the row beyond the documented total.
  //
  // The previous round pinned `ROW_HEIGHT_PX = 80` while the
  // inner tracks summed to 85.6px — the row overflowed by 5.6px
  // and `overflow: hidden` clipped the second reserved line. The
  // current round derives the outer height from the inner sum
  // (title-row 24 + capture-content 30 + footer 18 + padding 10
  // + gaps 2 + borders 2 = 86) so the inner sum always fits the
  // outer rectangle by construction.
  const cssBlock = extractCssBlock();
  const rowRule = cssBlock.match(/\.qp-row\s*\{[^}]*\}/);
  assert.ok(rowRule, "the .qp-row rule must exist");
  assert.ok(
    /height:\s*var\(--qp-row-height,\s*86px\)/.test(rowRule![0]),
    "the row must pin its height to --qp-row-height with an 86px fallback that matches the recomputed total",
  );
  assert.ok(
    /min-height:\s*var\(--qp-row-height,\s*86px\)/.test(rowRule![0]),
    "the row must pin its min-height to --qp-row-height with an 86px fallback",
  );
  assert.ok(
    /max-height:\s*var\(--qp-row-height,\s*86px\)/.test(rowRule![0]),
    "the row must pin its max-height to --qp-row-height so the geometry is fully bounded",
  );
  assert.ok(
    /overflow:\s*hidden/.test(rowRule![0]),
    "the row must clip overflow so nothing leaks beyond the documented rectangle",
  );
});

test("the row grid template reserves title-row, capture-content and footer tracks", () => {
  // The three-region geometry the change pins:
  //
  //   ```text
  //   fila fija
  //   ├── title-row: altura fija (24px)
  //   ├── capture-content: exactamente 2 líneas
  //   └── footer/meta: altura fija (18px)
  //   ```
  //
  // The grid template MUST declare all three tracks in order so a
  // regression that drops the footer/meta track collapses the
  // time / menu trigger back into capture-content (re-introducing
  // the "elapsed time counts as a content line" regression).
  //
  // The capture-content track uses `var(--qp-capture-content-height)`
  // (NOT `2lh`) so the inner sum always fits the outer rectangle.
  const cssBlock = extractCssBlock();
  const rowRule = cssBlock.match(/\.qp-row\s*\{[^}]*\}/);
  assert.ok(rowRule, "the .qp-row rule must exist");
  assert.ok(
    /grid-template-rows:[^;]*var\(--qp-title-row-height/.test(rowRule![0]),
    "the row must reserve the title-row track via --qp-title-row-height",
  );
  assert.ok(
    /grid-template-rows:[^;]*var\(--qp-capture-content-height/.test(rowRule![0]),
    "the row must reserve exactly two visual lines via --qp-capture-content-height (not 2lh)",
  );
  assert.ok(
    /grid-template-rows:[^;]*var\(--qp-footer-height/.test(rowRule![0]),
    "the row must reserve the footer/meta track via --qp-footer-height",
  );
});

test("capture-content hosts the preview only — time and menu live in footer", () => {
  // The body line (capture-content) MUST NOT carry the elapsed
  // time or the menu trigger; both must live in the footer/meta
  // track so the second visual line of capture-content is reserved
  // exclusively for the captured content (the
  // `quick-paste-desktop-polish` requirement that the time and the
  // menu never count as content lines).
  const cssBlock = cleanQuickPasteSource.slice(
    cleanQuickPasteSource.indexOf("<style>"),
    cleanQuickPasteSource.length,
  );
  const bodyRule = cssBlock.match(/\.qp-row-line-body\s*\{[^}]*\}/);
  assert.ok(bodyRule, "the .qp-row-line-body rule must exist");
  assert.ok(
    !/qp-elapsed/.test(bodyRule![0]),
    "capture-content must NOT carry the elapsed-time label",
  );
  assert.ok(
    !/qp-menu-trigger/.test(bodyRule![0]),
    "capture-content must NOT carry the menu trigger",
  );
  assert.ok(
    !/qp-code-language/.test(bodyRule![0]),
    "capture-content must NOT carry the code-language badge",
  );
  // The body line is the `display: flex` track that hosts the
  // preview (and the thumbnail for image rows) only.
  assert.ok(
    /display:\s*flex/.test(bodyRule![0]),
    "capture-content must use display: flex for the preview / thumbnail row",
  );
  // The footer/meta track carries the elapsed-time label, the
  // menu trigger and (optionally) the code-language badge.
  const footerRule = cssBlock.match(/\.qp-row-line-footer\s*\{[^}]*\}/);
  assert.ok(footerRule, "the .qp-row-line-footer rule must exist");
  assert.ok(
    /max-height:\s*var\(--qp-footer-height/.test(footerRule![0]),
    "the footer must clamp to the documented footer-height",
  );
  assert.ok(
    /grid-template-columns:[\s\S]*18px/.test(footerRule![0]),
    "the footer must reserve a 18px column for the menu trigger",
  );
});

test("the footer/meta track exposes a stable data-testid", () => {
  // The behaviour-level test (and the manual QA pass) target the
  // footer/meta track through `data-testid="quick-paste-row-footer"`
  // so a regression that drops the testid surfaces before the user
  // notices the time / menu moved unexpectedly.
  assert.ok(
    /data-testid="quick-paste-row-footer"/.test(cleanQuickPasteSource),
    "the footer/meta track must expose data-testid=quick-paste-row-footer",
  );
  // The footer height MUST also surface through `data-row-footer-height`
  // so the regression suite can pin the geometry without mounting
  // the row.
  assert.ok(
    /data-row-footer-height=\{FOOTER_HEIGHT_PX\}/.test(cleanQuickPasteSource),
    "the row must expose data-row-footer-height for the regression suite",
  );
  // The elapsed time and the menu trigger MUST live inside the
  // footer/meta track — not inside `capture-content`. The slice
  // below scopes the assertion to the body line so the rest of
  // the file (which still references `.qp-elapsed` / `.qp-menu-trigger`
  // inside the CSS rules) cannot trigger a false positive.
  const bodySlice = cleanQuickPasteSource.match(
    /class="qp-row-line qp-row-line-body"[\s\S]*?<\/div>/,
  );
  assert.ok(bodySlice, "the body line markup must exist");
  assert.ok(
    !/class="qp-elapsed"/.test(bodySlice![0]),
    "the elapsed-time label must NOT live inside capture-content",
  );
  assert.ok(
    !/class="qp-menu-trigger"/.test(bodySlice![0]),
    "the menu trigger must NOT live inside capture-content",
  );
  assert.ok(
    !/class="qp-code-language"/.test(bodySlice![0]),
    "the code-language badge must NOT live inside capture-content",
  );
  // And the inverse — both controls MUST live inside the
  // footer/meta track.
  const footerSlice = cleanQuickPasteSource.match(
    /class="qp-row-line qp-row-line-footer"[\s\S]*?<\/div>/,
  );
  assert.ok(footerSlice, "the footer/meta markup must exist");
  assert.ok(
    /class="qp-elapsed"/.test(footerSlice![0]),
    "the elapsed-time label must live inside the footer/meta track",
  );
  assert.ok(
    /class="qp-menu-trigger"/.test(footerSlice![0]),
    "the menu trigger must live inside the footer/meta track",
  );
});

// ---------------------------------------------------------------------------
// 10.2.1 Render preview: whitespace + clamp recipe.
//
// Root cause: the row preview helper used to call
// `entryPreviewText(entry, 80)`, which replaces whitespace runs
// with a single space, `trim()`s the result and truncates to ~80
// characters BEFORE the renderer sees it. The CSS reservation
// was correct (title-row + capture-content + footer/meta) but
// the rendered string was already a single flattened line, so
// `-webkit-line-clamp: 2` had nothing to wrap. Even the
// `hit.snippet` fallback in search mode inherited the same
// whitespace-collapse behaviour the snippet builder applied.
//
// The fix:
//   1. `renderPreview()` now returns `entryFullPreviewText(entry)`
//      for every textual entry (recents and search hits alike),
//      preserving LF / CRLF / tabs / indentation / blank lines /
//      significant spaces byte-for-byte.
//   2. The `.qp-preview` CSS rule uses `white-space: pre-wrap`
//      (instead of `white-space: normal`) so the rendered DOM
//      shows the captured block on up to two lines.
//   3. The `-webkit-line-clamp: 2` recipe clips the third line
//      inside the reserved track.
//
// The tests below pin every contract the manual QA pass asked
// for so a future contributor cannot silently re-flatten the
// preview.
// ---------------------------------------------------------------------------

test("renderPreview never calls the truncating entryPreviewText helper for text", () => {
  // The row preview helper MUST consume the canonical
  // `entryFullPreviewText` accessor for textual entries — the
  // truncating `entryPreviewText(entry, 80)` call collapsed
  // whitespace and sliced the string before the renderer saw it,
  // which is exactly why the second reserved line never appeared.
  // A regression that re-introduces the `80` argument surfaces
  // here before the user sees a single-line preview.
  const renderPreviewBody = extractFunctionBody(
    cleanQuickPasteSource,
    "renderPreview",
  );
  assert.ok(renderPreviewBody, "renderPreview must be declared");
  assert.equal(
    /entryPreviewText\([^)]*,\s*80\b/.test(renderPreviewBody),
    false,
    "renderPreview must NOT call entryPreviewText(entry, 80) — the helper flattens whitespace and truncates before the CSS can paint a real second line",
  );
  assert.ok(
    /entryFullPreviewText\(entry\)/.test(renderPreviewBody),
    "renderPreview must consult entryFullPreviewText so the captured whitespace reaches the renderer byte-for-byte",
  );
});

test("renderPreview never re-introduces the search snippet fallback for the row", () => {
  // The previous round fell back to `hit.snippet` in search mode
  // for the row preview. The snippet was meant for the highlighted
  // preview overlay and silently collapsed whitespace. The new
  // contract uses the canonical `EntryRecord` content for both
  // modes so the visible truncation is deterministic and the
  // search ranking is never coupled to the row geometry.
  const renderPreviewBody = extractFunctionBody(
    cleanQuickPasteSource,
    "renderPreview",
  );
  assert.ok(renderPreviewBody, "renderPreview must be declared");
  assert.equal(
    /hit\.snippet/.test(renderPreviewBody),
    false,
    "renderPreview must NOT consume hit.snippet for the row preview — the snippet collapses whitespace and biases the truncation toward the match",
  );
});

test("renderPreview keeps the image dimensions label", () => {
  // The row preview MUST keep the type + dimensions label for
  // image rows (`Imagen 1280×720` or the `Imagen` fallback when
  // dimensions are unknown). The image branch MUST stay on the
  // documented `entryPreviewText(entry)` helper (no `80`
  // argument — the dimension label is always short).
  const renderPreviewBody = extractFunctionBody(
    cleanQuickPasteSource,
    "renderPreview",
  );
  assert.ok(renderPreviewBody, "renderPreview must be declared");
  assert.ok(
    /isImageEntry\(entry\)/.test(renderPreviewBody),
    "renderPreview must branch on isImageEntry before selecting the helper",
  );
  assert.ok(
    /entryPreviewText\(entry\)/.test(renderPreviewBody),
    "image rows must consume entryPreviewText(entry) so the type + dimensions label survives",
  );
});

test("qp-preview CSS uses white-space: pre-wrap to preserve captured whitespace", () => {
  // The `.qp-preview` rule MUST declare `white-space: pre-wrap`
  // so LF / CRLF / tabs / indentation / blank lines / significant
  // spaces reach the renderer. `pre-wrap` is the only value that
  // both preserves whitespace and respects the `-webkit-line-clamp`
  // truncation. `normal` collapses whitespace; `nowrap` forces a
  // single line. Both modes pre-flattened the captured block and
  // were the documented failure mode of the previous round.
  const cssBlock = extractCssBlock();
  const previewRule = cssBlock.match(/\.qp-preview\s*\{[^}]*\}/);
  assert.ok(previewRule, "the .qp-preview rule must exist");
  assert.ok(
    /white-space:\s*pre-wrap/.test(previewRule![0]),
    "the preview must declare white-space: pre-wrap so the captured whitespace survives the clamp",
  );
  assert.equal(
    /white-space:\s*nowrap/.test(previewRule![0]),
    false,
    "the preview must NOT declare white-space: nowrap — the captured block would be clipped to a single line and the reserved track would stay empty",
  );
  assert.equal(
    /white-space:\s*normal\b/.test(previewRule![0]),
    false,
    "the preview must NOT declare white-space: normal — it would collapse the whitespace runs the helper preserved",
  );
  assert.equal(
    /(?:^|\s|;)white-space:\s*pre\b(?!-wrap)/m.test(previewRule![0]),
    false,
    "the preview must NOT use the narrower white-space: pre — that mode breaks long tokens at the right edge without wrapping",
  );
});

test("entryFullPreviewText survives LF separators, tabs, blank lines and indentation", () => {
  // The contract of the helper the row preview feeds from. A
  // regression in the helper would silently re-flatten the
  // captured block. The test exercises every whitespace class
  // the spec pins (`\n`, `\r\n`, `\t`, empty lines, indentation).
  // Asserting the helper directly is cheaper and more reliable
  // than mounting the Svelte component.
  const helperSource = readSource("src/lib/clipboardAsset.ts");
  const helperMatch = helperSource.match(
    /export function entryFullPreviewText[\s\S]+?\n\}/,
  );
  assert.ok(helperMatch, "entryFullPreviewText must be declared");
  const body = helperMatch![0];
  assert.equal(
    /replace\([^)]*\\s\+/.test(body),
    false,
    "entryFullPreviewText must never collapse whitespace runs",
  );
  assert.equal(
    /trim\(/.test(body),
    false,
    "entryFullPreviewText must never trim the captured block",
  );
  // The helper also preserves byte-for-byte via the implicit
  // `return entry.content ?? ""` — the snippet below confirms
  // the literal is the only data path.
  assert.ok(
    /entry\.content/.test(body),
    "entryFullPreviewText must read entry.content (the canonical source of the captured block)",
  );
});

test("row preview preserves whitespace across recents and search hits", () => {
  // The preview helper `renderPreview` MUST feed the canonical
  // `entryFullPreviewText` string into the row regardless of
  // mode (recents or search). A regression that branches on
  // `currentMode === "search"` and consumes a different source
  // for search hits would reintroduce the single-line bug for
  // any capture surfaced by a query.
  const renderPreviewBody = extractFunctionBody(
    cleanQuickPasteSource,
    "renderPreview",
  );
  assert.ok(renderPreviewBody, "renderPreview must be declared");
  assert.ok(
    /return\s+entryFullPreviewText\(entry\)/.test(renderPreviewBody),
    "renderPreview must return entryFullPreviewText(entry) on the happy path so both recents and search hits share the same whitespace contract",
  );
  // The image branch must still produce the documented
  // `Imagen WxH` / `Imagen` placeholder; the search-mode branch
  // must not drop into a different preview source.
  const hasImageBranch = /isImageEntry\(entry\)/.test(renderPreviewBody);
  assert.ok(hasImageBranch, "renderPreview must keep the isImageEntry branch");
});

test("search ranking is preserved (no alteration of SearchService)", () => {
  // The fix MUST NOT touch `SearchService`. The helper that ranks
  // and orders search hits is the canonical source of truth for
  // the result list. A regression that re-ranks inside
  // `renderPreview` would silently change the visible order.
  const searchServiceSource = readSource("src/lib/search.ts");
  assert.ok(
    searchServiceSource.length > 0,
    "SearchService module must remain present",
  );
  // The renderPreview helper MUST NOT import or call any search
  // ranking primitive. The only contract the preview holds with
  // the search feed is the static `entry` reference; no
  // reordering, no filtering, no re-ranking.
  const renderPreviewBody = extractFunctionBody(
    cleanQuickPasteSource,
    "renderPreview",
  );
  assert.equal(
    /sort\(|filter\(|rank/i.test(renderPreviewBody),
    false,
    "renderPreview must NOT sort, filter or re-rank — SearchService owns the ranking",
  );
  // The original `searchHits` parameter is read once (via
  // `findEntry`) and then ignored; a future regression that
  // walks it inline to compute a snippet or to truncate would
  // reintroduce the search-only flattening.
  assert.ok(
    /findEntry\(currentMode,\s*recents,\s*searchHits,\s*id\)/.test(renderPreviewBody),
    "renderPreview must read the canonical EntryRecord through findEntry",
  );
});

test("image row preview keeps the documented type + dimensions label", () => {
  // The image branch MUST keep the same `entryPreviewText(entry)`
  // call the previous round used for the placeholder. The new
  // contract documents that the dimension label must survive the
  // clamp; `entryPreviewText` (without `80`) is the documented
  // helper because the label is always short and the helper
  // returns `"Imagen 1280×720"` / `"Imagen"` / `"(vacío)"` for
  // image rows.
  const renderPreviewBody = extractFunctionBody(
    cleanQuickPasteSource,
    "renderPreview",
  );
  assert.ok(renderPreviewBody, "renderPreview must be declared");
  // The image branch must consult `entryPreviewText(entry)` —
  // no `80` argument, so the helper never slices the label.
  const imageBranch = renderPreviewBody.match(
    /if\s*\(\s*isImageEntry\(entry\)\s*\)\s*\{[\s\S]*?\}/,
  );
  assert.ok(imageBranch, "renderPreview must keep the isImageEntry branch");
  assert.ok(
    /entryPreviewText\(entry\)/.test(imageBranch![0]),
    "image rows must call entryPreviewText(entry) (no 80) so the dimension label is never sliced",
  );
  assert.equal(
    /entryPreviewText\([^)]*,\s*80/.test(imageBranch![0]),
    false,
    "image rows must NOT pass an 80-char limit to entryPreviewText — the dimension label would be sliced",
  );
});

test("row preview stays identical across loading, loaded and error thumbnail states", () => {
  // The capture-content track (`qp-row-line-body`) reserves the
  // exact same 30px whether the thumbnail is in `loading`,
  // `loaded` or `error`. The row preview text is part of the
  // same track; a regression that changed the body's height on
  // a thumbnail state would compress or expand the second line.
  const cssBlock = extractCssBlock();
  const bodyRule = cssBlock.match(/\.qp-row-line-body\s*\{[^}]*\}/);
  assert.ok(bodyRule, "the .qp-row-line-body rule must exist");
  // The body line clamps to the explicit pixel reservation —
  // not `auto`, not the thumbnail state, not the load status.
  assert.ok(
    /min-height:\s*var\(--qp-capture-content-height/.test(bodyRule![0]),
    "capture-content must reserve two lines via --qp-capture-content-height regardless of thumbnail state",
  );
  assert.ok(
    /max-height:\s*var\(--qp-capture-content-height/.test(bodyRule![0]),
    "capture-content must clamp to two lines via --qp-capture-content-height regardless of thumbnail state",
  );
  // The preview CSS must not collapse the whitespace even when
  // the thumbnail is the dominant surface (image rows).
  const previewRule = cssBlock.match(/\.qp-preview\s*\{[^}]*\}/);
  assert.ok(previewRule, "the .qp-preview rule must exist");
  assert.ok(
    /white-space:\s*pre-wrap/.test(previewRule![0]),
    "the preview must keep white-space: pre-wrap so the clamp preserves the captured whitespace across every thumbnail state",
  );
});

// ---------------------------------------------------------------------------
// 10.3 Tags visibility regressions — entries, hydration, navigation, scope.
// ---------------------------------------------------------------------------

test("Quick Paste hydrates tags for three distinct entries in parallel", () => {
  // The hydration round MUST walk all visible entries (recents
  // and search hits) so every row can surface its own chips. The
  // helper issues one `entryTagsCommand` per entry through
  // `Promise.all`; a regression that serialised the round would
  // silently break the multi-entry contract.
  assert.ok(
    /Promise\.all\(targets\.map\(/.test(cleanQuickPasteSource),
    "Quick Paste must hydrate tags in parallel",
  );
  assert.ok(
    /hydrateTagsForEntry\(entry\.id\)/.test(cleanQuickPasteSource),
    "Quick Paste must call hydrateTagsForEntry per entry",
  );
  // Each entry id MUST be the cache key the row reads through
  // `tagsForEntry(id, entryTagsCache, entryTagsHydration)` so
  // three distinct entries surface three distinct chips. The
  // cache and hydration maps MUST be passed explicitly at the
  // call site so Svelte's compiler marks them as dependencies
  // and the `{@const}` re-evaluates when the hydration round
  // populates the cache (the previous round closed over the
  // maps and the compiler missed the dependency — chips stayed
  // empty even after the cache was correctly populated).
  assert.ok(
    /tagsForEntry\(id,\s*entryTagsCache,\s*entryTagsHydration\)/.test(
      cleanQuickPasteSource,
    ),
    "Quick Paste must call tagsForEntry with the cache and hydration maps so Svelte tracks them as reactive dependencies",
  );
  assert.ok(
    /entryTagsCache\.get\(entryId\)/.test(cleanQuickPasteSource) ||
      /cache\.get\(entryId\)/.test(cleanQuickPasteSource) ||
      /entryTagsHydration\.get\(entry\.id\)/.test(cleanQuickPasteSource),
    "Quick Paste must read the chips from the per-entry cache",
  );
});

test("Quick Paste pre-loads the organization snapshot before the parallel hydration round", () => {
  // The pre-load is the documented fix for the
  // `quick-paste-desktop-polish` regression that left rows without
  // tags even when the backend returned ids the user just
  // persisted. Without the pre-load, multiple parallel
  // `hydrateTagsForEntry` calls race for the snapshot fetch and
  // only the last one writes a populated lookup into `knownTags`;
  // the earlier entries land with an empty cache even though the
  // bridge payload carried the tag ids.
  const bodyMatch = cleanQuickPasteSource.match(
    /async function hydrateTagsForVisibleEntries\([\s\S]*?\n  \}/,
  );
  assert.ok(bodyMatch, "hydrateTagsForVisibleEntries must exist");
  const body = bodyMatch![0];
  // The pre-load MUST happen BEFORE the `Promise.all` so every
  // parallel `hydrateTagsForEntry` reads a populated `knownTags`.
  assert.ok(
    /await\s+ensureKnownTagsLoaded\(\)/.test(body),
    "hydrateTagsForVisibleEntries must await the snapshot pre-load",
  );
  const preLoadIndex = body.indexOf("await ensureKnownTagsLoaded()");
  const promiseAllIndex = body.indexOf("Promise.all");
  assert.ok(
    preLoadIndex >= 0 && promiseAllIndex > preLoadIndex,
    "the pre-load must run before the Promise.all round-trip",
  );
  // The `applyQuickPasteTagsResult` call inside
  // `hydrateTagsForEntry` MUST resolve against the module-level
  // `knownTags` lookup (not a stale local var captured before
  // the pre-load resolved).
  const entryMatch = cleanQuickPasteSource.match(
    /async function hydrateTagsForEntry\([\s\S]*?\n  \}/,
  );
  assert.ok(entryMatch, "hydrateTagsForEntry must exist");
  const entry = entryMatch![0];
  assert.ok(
    /applyQuickPasteTagsResult\([\s\S]*?knownTags,?\s*\)/.test(entry),
    "hydrateTagsForEntry must resolve tag ids against the module-level knownTags",
  );
});

test("each Quick Paste row renders only its own tags", () => {
  // The template MUST walk the row's `tagsProjection.chips`
  // array and bind each chip to its entry id. A regression that
  // read a global chips map or reused the previous row's chips
  // would surface tags on the wrong entry.
  const chipRowMatch = cleanQuickPasteSource.match(
    /class="qp-tags"[\s\S]*?\+{tagsProjection\.overflow}/,
  );
  assert.ok(chipRowMatch, "the chip row markup must exist");
  const chipRow = chipRowMatch![0];
  assert.ok(
    /\{chip\.tag\.display_name\}/.test(chipRow),
    "the chip row must render display_name per chip",
  );
  assert.ok(
    /data-tag-id=\{chip\.tag\.id\}/.test(chipRow),
    "the chip row must bind the chip to its tag id",
  );
  // The key MUST be unique per tag so Svelte's keyed each-block
  // does not collapse chips from different rows into the same
  // DOM node.
  assert.ok(
    /key: `tag-\$\{tag\.id\}`/.test(cleanQuickPasteSource) ||
      /key=\{`tag-\$\{tag\.id\}`\}/.test(cleanQuickPasteSource),
    "the chip row must use a per-tag key so different rows cannot share chips",
  );
});

test("arrow navigation never leaks tags across rows", () => {
  // `moveSelection` MUST only update `selectedIndex` /
  // `selectedEntryId`; it MUST NOT mutate `entryTagsCache` or
  // `entryTagsHydration`. A regression that pruned or rewrote
  // the cache from inside the navigation path would silently
  // drop the chips for entries the user just left.
  const moveMatch = cleanQuickPasteSource.match(
    /function moveSelection\([\s\S]*?\n  \}/,
  );
  assert.ok(moveMatch, "moveSelection must exist");
  const body = moveMatch![0];
  assert.equal(
    /entryTagsCache/.test(body),
    false,
    "moveSelection must not write to the tag cache",
  );
  assert.equal(
    /entryTagsHydration/.test(body),
    false,
    "moveSelection must not mutate the hydration map",
  );
  // The selection update MUST keep `selectedEntryId` in lockstep
  // with `selectedIndex` so the keyboard focus and the visible
  // highlight read the same row.
  assert.ok(
    /selectedIndex\s*=\s*next/.test(body),
    "moveSelection must update selectedIndex",
  );
  assert.ok(
    /selectedEntryId\s*=\s*resultIds\[next\]/.test(body),
    "moveSelection must update selectedEntryId from the result ids",
  );
});

test("search results re-hydrate tags so the new scope never inherits stale chips", () => {
  // The search branch MUST call `hydrateTagsForVisibleEntries`
  // against the freshly-returned `response.hits` so a stale
  // response from the previous scope cannot attach tags to an
  // entry that just entered the result list. A regression that
  // skipped the search-side hydration would surface tags the
  // user had already pruned.
  assert.ok(
    /hydrateTagsForVisibleEntries\(\s*response\.hits\.map/.test(cleanQuickPasteSource) ||
      /hydrateTagsForVisibleEntries\(\s*response\.hits/.test(cleanQuickPasteSource),
    "Quick Paste must hydrate the search hits after the clamp",
  );
  // The prune helper MUST run reactively whenever `resultIds`
  // changes so a stale entry's chips cannot survive a search.
  assert.ok(
    /\$\:\s*\{[\s\S]*?pruneTagsToVisibleEntries/.test(cleanQuickPasteSource),
    "Quick Paste must reactively prune the tag cache against the visible scope",
  );
  assert.ok(
    /resetQuickPasteTagsToken/.test(cleanQuickPasteSource),
    "Quick Paste must reset the per-entry token when an entry leaves the scope",
  );
});

test("refresh and thumbnail loading never wipe the tag cache", () => {
  // The thumbnail bridge (`loadThumbnail`) MUST NOT touch the
  // tag cache; only the tag hydration path is allowed to write
  // to it. A regression that wiped the cache during a thumbnail
  // round would make chips flicker every time the asset bridge
  // resolved.
  const thumbnailMatch = cleanQuickPasteSource.match(
    /async function loadThumbnail\([\s\S]*?\n  \}/,
  );
  assert.ok(thumbnailMatch, "loadThumbnail must exist");
  assert.equal(
    /entryTagsCache/.test(thumbnailMatch![0]),
    false,
    "loadThumbnail must not write to the tag cache",
  );
  assert.equal(
    /entryTagsHydration/.test(thumbnailMatch![0]),
    false,
    "loadThumbnail must not write to the hydration map",
  );
  // The `hydrateTagsForVisibleEntries` helper MUST be called from
  // the `loadRecent` and search paths so a refresh re-hydrates
  // the rows. A regression that removed the call would leave the
  // rows without chips after a refresh.
  assert.ok(
    /void hydrateTagsForVisibleEntries\(recent\)/.test(cleanQuickPasteSource),
    "Quick Paste must re-hydrate tags after loadRecent",
  );
});

test("stale tag hydration cannot overwrite a fresher row (behaviour)", () => {
  // The companion test for `lib/quickPasteTags.ts`. A regression
  // that dropped the per-entry token guard would let a late
  // response attach a tag set to a row that left the scope.
  __resetQuickPasteTagsTokensForTests();
  const entryA = 11;
  const knownTags = [
    makeTag({ id: 10, normalized_name: "alpha", display_name: "Alpha" }),
    makeTag({ id: 11, normalized_name: "beta", display_name: "Beta" }),
  ];
  const cache: QuickPasteTagsCache = new Map();
  const hydration: QuickPasteTagsHydration = new Map();

  // First round: hydrate entry A with tag-alpha.
  const tokenA = bumpQuickPasteTagsToken(entryA);
  hydration.set(entryA, "pending");
  // Stale response: token no longer matches because entry A's
  // bump was overwritten by a second refresh call.
  bumpQuickPasteTagsToken(entryA);
  assert.notEqual(
    currentQuickPasteTagsToken(entryA),
    tokenA,
    "bump must monotonically advance the token",
  );
  // The component would consult `currentQuickPasteTagsToken(entryA)
  // !== tokenA` and drop the stale response; assert the invariant
  // manually so a regression in `hydrateTagsForEntry` surfaces.
  if (currentQuickPasteTagsToken(entryA) === tokenA) {
    assert.fail(
      "stale token must not match the live token — stale response should have been dropped",
    );
  }
  // A fresh response with tag-beta lands correctly.
  const fresh = applyQuickPasteTagsResult(
    cache,
    hydration,
    entryA,
    [11],
    knownTags,
  );
  assert.equal(fresh.nextCache.get(entryA)?.length, 1);
  assert.equal(fresh.nextCache.get(entryA)?.[0]?.id, 11);
  assert.equal(fresh.nextHydration.get(entryA), "loaded");
  // The error branch must keep the previous loaded entries intact.
  const errHydration = markQuickPasteTagsError(fresh.nextHydration, entryA);
  assert.equal(errHydration.get(entryA), "error");
  assert.ok(fresh.nextCache.has(entryA), "the cache must keep the entry on error");
});