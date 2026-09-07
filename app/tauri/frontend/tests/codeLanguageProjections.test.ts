/**
 * Regression coverage for the `code-language-detection` projection
 * helpers. The change surfaced after the manual macOS test
 * reported that the badge, the language label and the syntax
 * highlighting never appeared on the Desktop rail, the
 * `ClipboardPreview` overlay or the Quick Paste palette. The
 * root cause was a chain of mismatches:
 *
 *   - `App.svelte::refresh()` never re-ran the detector on the
 *     entries that survived a restart, so legacy rows started
 *     with `code_language = null` and never left that state;
 *   - the inline patch the hydration helper ran updated
 *     `entries` but never the `visibleEntries` slice the rail
 *     rendered;
 *   - the cards and the preview both required
 *     `entry.content_type === "code"` before showing the
 *     metadata, but the metadata-only bridge persists
 *     `code_language` without changing `content_type`, so a
 *     detected snippet can keep `content_type = "text"` and
 *     still need the badge.
 *
 * The pure helpers in `lib/codeLanguageProjections.ts` are the
 * single switch every UI surface now consults. The tests below
 * pin the invariants the regression relied on:
 *
 *   1. `shouldShowCodeLanguageBadge` is true for a textual
 *      entry with a canonical `code_language`, regardless of
 *      `content_type`. It refuses images and aliases.
 *   2. `applyCodeLanguageToDesktopProjections` patches
 *      `entries` and `visibleEntries` in lockstep, leaves every
 *      sibling row untouched and never loses titles, tags,
 *      favourites, source-app, thumbnails or image metadata.
 *   3. `applyCodeLanguageToRecentAndHits` mirrors the same
 *      invariant for the Quick Paste palette.
 *
 * The source-level tests at the bottom pin the
 * `entry.content_type === "code"` regression so a future
 * contributor cannot silently re-introduce the old
 * conditional.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  applyCodeLanguageToDesktopProjections,
  applyCodeLanguageToRecentAndHits,
  shouldRenderHighlightedPreview,
  shouldShowCodeLanguageBadge,
} from "../src/lib/codeLanguageProjections.ts";
import type { EntryRecord, SearchHit } from "../src/types.ts";

const FRONTEND_ROOT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);

function makeEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 1,
    content: "captured note",
    content_type: "text",
    content_size: 13,
    content_hash: "h",
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
    last_seen_at: "2026-01-02T03:04:05Z",
    is_pinned: false,
    source_app: "test",
    source_app_name: null,
    source_app_icon_ref: null,
    title: null,
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

function makeHit(entry: EntryRecord): SearchHit {
  return {
    entry_id: entry.id,
    snippet: entry.content.slice(0, 24),
    score: 1,
    record: entry,
  };
}

// ---------------------------------------------------------------------------
// shouldShowCodeLanguageBadge
// ---------------------------------------------------------------------------

test("shouldShowCodeLanguageBadge accepts text entries with a canonical language", () => {
  // The detector promotes a textual capture to a code
  // classification through the metadata-only bridge. The backend
  // does NOT change the historical `content_type`, so the badge
  // rule MUST accept `content_type = "text"` rows.
  const entry = makeEntry({
    id: 7,
    content: "const x = 1;\nfunction f() { return x; }\n",
    content_type: "text",
    code_language: "javascript",
  });
  assert.equal(shouldShowCodeLanguageBadge(entry), true);
  assert.equal(shouldRenderHighlightedPreview(entry), true);
});

test("shouldShowCodeLanguageBadge accepts code entries with a canonical language", () => {
  // The legacy `content_type === "code"` branch MUST still work;
  // the follow-up keeps the existing rendering intact and only
  // widens the rule to also accept the text-typed captures the
  // detector promotes.
  const entry = makeEntry({
    id: 8,
    content: "def greet(name):\n    print(name)\n",
    content_type: "code",
    code_language: "python",
  });
  assert.equal(shouldShowCodeLanguageBadge(entry), true);
});

test("shouldShowCodeLanguageBadge refuses image entries", () => {
  const entry = makeEntry({
    id: 9,
    content: "",
    content_type: "image",
    asset_ref: "clipboard/abcdef.png",
    mime_type: "image/png",
    code_language: "javascript",
  });
  // Even if a future regression stored `code_language` on an
  // image row, the badge must never appear — the canvas can never
  // carry a code language and the chip would be misleading.
  assert.equal(shouldShowCodeLanguageBadge(entry), false);
  assert.equal(shouldRenderHighlightedPreview(entry), false);
});

test("shouldShowCodeLanguageBadge refuses unknown or empty languages", () => {
  const unknown = makeEntry({ code_language: "perl" });
  assert.equal(shouldShowCodeLanguageBadge(unknown), false);
  const empty = makeEntry({ code_language: null });
  assert.equal(shouldShowCodeLanguageBadge(empty), false);
});

test("shouldShowCodeLanguageBadge refuses aliases", () => {
  // `py` is an alias that the detector normalises; the badge
  // helper, however, only accepts canonical identifiers so the
  // SQL index, the label and the preview overlay agree.
  const alias = makeEntry({ code_language: "py" });
  assert.equal(shouldShowCodeLanguageBadge(alias), false);
});

// ---------------------------------------------------------------------------
// applyCodeLanguageToDesktopProjections
// ---------------------------------------------------------------------------

test("applyCodeLanguageToDesktopProjections patches entries and visibleEntries in lockstep", () => {
  const entries = [
    makeEntry({ id: 1, code_language: null }),
    makeEntry({ id: 2, code_language: null }),
    makeEntry({ id: 3, code_language: null }),
  ];
  const visibleEntries = [...entries];

  const patched = applyCodeLanguageToDesktopProjections(
    entries,
    visibleEntries,
    2,
    "python",
    { isFiltering: false },
  );

  assert.equal(patched.applied, true);
  assert.equal(patched.nextEntries[0]?.code_language, null);
  assert.equal(patched.nextEntries[1]?.code_language, "python");
  assert.equal(patched.nextEntries[2]?.code_language, null);
  // When not filtering, `nextVisibleEntries` IS `nextEntries`; the
  // rail reads from `visibleEntries` and reflects the patch
  // immediately.
  assert.equal(patched.nextVisibleEntries.length, 3);
  assert.equal(patched.nextVisibleEntries[1]?.code_language, "python");
  // The original lists are untouched so Svelte's reactivity is
  // the only path that observes the patch.
  assert.notEqual(patched.nextEntries, entries);
  assert.notEqual(patched.nextVisibleEntries, visibleEntries);
});

test("applyCodeLanguageToDesktopProjections patches visibleEntries independently during a search", () => {
  const entries = [
    makeEntry({ id: 1 }),
    makeEntry({ id: 4 }),
    makeEntry({ id: 5 }),
  ];
  // The search controller narrows the rail to entries 1 and 4.
  const visibleEntries = [entries[0]!, entries[1]!];

  const patched = applyCodeLanguageToDesktopProjections(
    entries,
    visibleEntries,
    1,
    "javascript",
    { isFiltering: true },
  );

  assert.equal(patched.applied, true);
  assert.equal(patched.nextEntries[0]?.code_language, "javascript");
  assert.equal(patched.nextEntries[1]?.code_language, null);
  // visibleEntries mirrors entry 1's patch AND keeps entry 4
  // untouched because it never matched the search.
  assert.equal(patched.nextVisibleEntries.length, 2);
  assert.equal(patched.nextVisibleEntries[0]?.code_language, "javascript");
  assert.equal(patched.nextVisibleEntries[1]?.code_language, null);
});

test("applyCodeLanguageToDesktopProjections preserves titles, favourites and metadata", () => {
  const entry = makeEntry({
    id: 11,
    title: "My snippet",
    is_pinned: true,
    source_app_name: "Visual Studio Code",
    source_app_icon_ref: "application-icons/com.microsoft.VSCode.png",
    asset_ref: "clipboard/abcdef.png",
    mime_type: "image/png",
    payload_width: 800,
    payload_height: 600,
  });
  const patched = applyCodeLanguageToDesktopProjections(
    [entry],
    [entry],
    11,
    "rust",
    { isFiltering: false },
  );
  const next = patched.nextEntries[0]!;
  assert.equal(next.code_language, "rust");
  assert.equal(next.title, "My snippet");
  assert.equal(next.is_pinned, true);
  assert.equal(next.source_app_name, "Visual Studio Code");
  assert.equal(
    next.source_app_icon_ref,
    "application-icons/com.microsoft.VSCode.png",
  );
  assert.equal(next.asset_ref, "clipboard/abcdef.png");
  assert.equal(next.mime_type, "image/png");
  assert.equal(next.payload_width, 800);
  assert.equal(next.payload_height, 600);
});

test("applyCodeLanguageToDesktopProjections refuses non-canonical languages", () => {
  // A malicious or stale bridge response must NOT poison the
  // local state. The helper collapses unknown values to `null`
  // so the badge rule stays a no-op.
  const entries = [makeEntry({ id: 1 })];
  const patched = applyCodeLanguageToDesktopProjections(
    entries,
    entries,
    1,
    "perl",
    { isFiltering: false },
  );
  assert.equal(patched.applied, true);
  assert.equal(patched.nextEntries[0]?.code_language, null);
});

test("applyCodeLanguageToDesktopProjections returns a no-op when the entry id is unknown", () => {
  const entries = [makeEntry({ id: 1 })];
  const patched = applyCodeLanguageToDesktopProjections(
    entries,
    entries,
    99,
    "python",
    { isFiltering: false },
  );
  assert.equal(patched.applied, false);
  assert.equal(patched.nextEntries[0]?.code_language, null);
});

// ---------------------------------------------------------------------------
// applyCodeLanguageToRecentAndHits
// ---------------------------------------------------------------------------

test("applyCodeLanguageToRecentAndHits patches recent and hits in lockstep", () => {
  const recent = [
    makeEntry({ id: 1, code_language: null }),
    makeEntry({ id: 2, code_language: null }),
  ];
  const hits = [makeHit(recent[0]!), makeHit(recent[1]!)];

  const patched = applyCodeLanguageToRecentAndHits(recent, hits, 2, "go");
  assert.equal(patched.applied, true);
  assert.equal(patched.nextRecent[0]?.code_language, null);
  assert.equal(patched.nextRecent[1]?.code_language, "go");
  assert.equal(patched.nextHits[0]?.record.code_language, null);
  assert.equal(patched.nextHits[1]?.record.code_language, "go");
});

test("applyCodeLanguageToRecentAndHits preserves every other row", () => {
  const recent = [
    makeEntry({ id: 1, title: "Critical snippet", is_pinned: true }),
    makeEntry({ id: 2, title: "Helper snippet" }),
  ];
  const hits = [makeHit(recent[0]!), makeHit(recent[1]!)];
  const patched = applyCodeLanguageToRecentAndHits(recent, hits, 1, "rust");
  assert.equal(patched.nextRecent[0]?.title, "Critical snippet");
  assert.equal(patched.nextRecent[0]?.is_pinned, true);
  assert.equal(patched.nextRecent[1]?.code_language, null);
  assert.equal(patched.nextHits[1]?.record.code_language, null);
});

test("applyCodeLanguageToRecentAndHits refuses non-canonical languages", () => {
  const recent = [makeEntry({ id: 1 })];
  const hits = [makeHit(recent[0]!)];
  const patched = applyCodeLanguageToRecentAndHits(recent, hits, 1, "perl");
  assert.equal(patched.applied, true);
  assert.equal(patched.nextRecent[0]?.code_language, null);
  assert.equal(patched.nextHits[0]?.record.code_language, null);
});

// ---------------------------------------------------------------------------
// Source-level invariants — never re-introduce the legacy guard.
// ---------------------------------------------------------------------------

function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
}

test("HistoryCard.svelte no longer requires content_type === 'code' for the badge", () => {
  // The legacy conditional
  // `{#if entry.content_type === "code" && entry.code_language !== null}`
  // must NEVER come back. The detector stores `code_language` on
  // entries whose `content_type` is still `text`, so the rule has
  // to widen.
  const source = stripComments(
    readFileSync(
      path.join(FRONTEND_ROOT, "src/HistoryCard.svelte"),
      "utf8",
    ),
  );
  assert.equal(
    /entry\.content_type\s*===\s*"code"\s*&&\s*entry\.code_language/.test(
      source,
    ),
    false,
    "HistoryCard must not gate the badge on content_type === 'code'",
  );
  assert.match(source, /shouldShowCodeLanguageBadge\(entry\)/);
});

test("ClipboardPreview.svelte no longer requires content_type === 'code' for highlighting", () => {
  // The legacy gate
  // `entry.content_type !== "code" || fullText.length === 0`
  // refused to highlight text-typed entries even when the
  // detector had accepted them. The follow-up routes the
  // conditional through `shouldRenderHighlightedPreview` so the
  // preview overlay mirrors the badge rule.
  const source = stripComments(
    readFileSync(
      path.join(FRONTEND_ROOT, "src/ClipboardPreview.svelte"),
      "utf8",
    ),
  );
  assert.equal(
    /record\.content_type\s*!==\s*"code"/.test(source),
    false,
    "ClipboardPreview must not gate the highlighted preview on content_type !== 'code'",
  );
  assert.match(source, /shouldRenderHighlightedPreview\(record\)/);
  assert.match(source, /shouldShowCodeLanguageBadge\(record\)/);
});

test("App.svelte::hydrateCodeLanguages patches visibleEntries", () => {
  // The legacy inline patch only updated `entries`, leaving
  // `visibleEntries` (the list the rail renders) on the stale
  // object. The follow-up routes the patch through the pure
  // helper so the rail reacts to the new metadata on the same
  // render.
  const source = stripComments(
    readFileSync(path.join(FRONTEND_ROOT, "src/App.svelte"), "utf8"),
  );
  assert.match(
    source,
    /applyCodeLanguageToDesktopProjections\(\s*entries,\s*visibleEntries/,
  );
});

test("App.svelte::refresh bootstraps code-language hydration", () => {
  // The legacy bootstrap never called the hydration helper, so
  // entries that survived a restart kept `code_language = null`
  // until the user typed into the search box. The follow-up
  // schedules a non-blocking round on every `refresh()` so the
  // Desktop rail hydrates legacy rows on the same paint that
  // shows them.
  const source = stripComments(
    readFileSync(path.join(FRONTEND_ROOT, "src/App.svelte"), "utf8"),
  );
  const refreshMatch = source.match(
    /async function refresh\(\)[\s\S]*?\n  \}/,
  );
  assert.ok(refreshMatch, "refresh must remain defined in App.svelte");
  const body = refreshMatch?.[0] ?? "";
  assert.match(
    body,
    /hydrateCodeLanguages\(\)/,
    "refresh must schedule a code-language hydration round",
  );
  // The round is anchored on a monotonic token and never blocks
  // the loading flag — a quick user must not wait for the
  // detector to finish before the rail becomes interactive.
  assert.match(body, /\bvoid\s+hydrateCodeLanguages\(\)/);
});

test("QuickPaste.svelte hydrates recent and hits in lockstep", () => {
  const source = stripComments(
    readFileSync(path.join(FRONTEND_ROOT, "src/QuickPaste.svelte"), "utf8"),
  );
  assert.match(source, /applyCodeLanguageToRecentAndHits\(\s*recent,\s*hits/);
  assert.match(source, /shouldShowCodeLanguageBadge\(entry\)/);
  assert.match(source, /quick-paste-code-language/);
});
