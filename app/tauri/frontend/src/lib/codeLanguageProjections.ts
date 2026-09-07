/**
 * Pure, immutable projection helpers the `code-language-detection`
 * capability needs to keep the Desktop rail, the search results and
 * the Quick Paste palette in lockstep with the canonical language
 * the backend persisted.
 *
 * The helpers in this module are deliberately narrow:
 *
 *   - `shouldShowCodeLanguageBadge` decides whether an
 *     `EntryRecord` should surface a code-language badge. The
 *     detector promotes a textual capture to a code classification
 *     through the metadata-only bridge; the backend does NOT
 *     change the historical `content_type`, so the badge rule is
 *     independent of `content_type === "code"`. An image or rich
 *     row never carries a code language; the predicate refuses
 *     those payloads explicitly.
 *   - `applyCodeLanguageToDesktopProjections` patches the
 *     canonical `entries` list and the `visibleEntries` projection
 *     the rail renders. The helper never replaces a list with a
 *     subset and never mutates a record in place; every patch is
 *     an immutable copy of the affected row so Svelte's
 *     reactivity picks the change up without affecting siblings.
 *   - `applyCodeLanguageToRecentAndHits` mirrors the same
 *     invariant for the Quick Paste palette: the `recent` feed
 *     and the `hits` array are patched in lockstep, the
 *     `SearchHit.record` reference is replaced with a fresh
 *     object, and no other entry is touched.
 *
 * The helpers never touch clipboard content, hashes, asset
 * references or the canonical payload, so a future regression that
 * adds another field to `EntryRecord` cannot accidentally surface
 * the new field through the bridge.
 */

import type { EntryRecord, SearchHit } from "../types.ts";
import { isImageEntry } from "./clipboardAsset.ts";
import { isAllowedLanguage } from "./codeLanguageDetector.ts";

/**
 * Whether an entry should surface the `Código · …` badge. The rule
 * mirrors the `desktop-card-preview` and `quick-paste-preview-ui`
 * specs:
 *
 *   - the entry must carry a canonical, allowlist-accepted
 *     `code_language`;
 *   - the entry must NOT be an image (the canvas would never
 *     carry a code language and the badge would be misleading);
 *   - the entry must carry non-empty textual content — an image
 *     payload can have `content_type === "image"` with a sentinel
 *     `content`, so the explicit `isImageEntry` check is the
 *     canonical guard;
 *   - the badge is shown for both `code` and `text` rows because
 *     the metadata-only bridge persists `code_language` without
 *     modifying `content_type`.
 */
export function shouldShowCodeLanguageBadge(entry: EntryRecord): boolean {
  if (!entry) return false;
  if (isImageEntry(entry)) return false;
  if (!isAllowedLanguage(entry.code_language)) return false;
  return typeof entry.content === "string";
}

/**
 * Whether the `ClipboardPreview` overlay should mount the
 * highlighted markup instead of the escaped plain-text fallback.
 * The helper mirrors `shouldShowCodeLanguageBadge` so the preview
 * and the card badge agree byte-for-byte: a textual entry with a
 * canonical `code_language` always renders the highlighted
 * surface, regardless of `content_type`.
 */
export function shouldRenderHighlightedPreview(entry: EntryRecord): boolean {
  return shouldShowCodeLanguageBadge(entry);
}

/**
 * Patch the Desktop projections (`entries`, `visibleEntries`) with
 * a freshly persisted `code_language`. The helper is intentionally
 * side-effect free and returns fresh arrays so the parent can
 * reassign them in lockstep. Every entry the helper does NOT
 * touch keeps its previous identity (titles, tags, collections,
 * favourites, source-app, thumbnail and image metadata stay
 * intact).
 *
 * The `isFiltering` flag tells the helper how to keep
 * `visibleEntries` in lockstep with `entries`: when the rail is
 * rendering a search result, `visibleEntries` is patched
 * independently because the search controller owns its own
 * snapshot of the row.
 */
export function applyCodeLanguageToDesktopProjections(
  entries: readonly EntryRecord[],
  visibleEntries: readonly EntryRecord[],
  entryId: number,
  language: string | null,
  options: { isFiltering: boolean } = { isFiltering: false },
): {
  nextEntries: EntryRecord[];
  nextVisibleEntries: EntryRecord[];
  applied: boolean;
} {
  const safeLanguage =
    typeof language === "string" && isAllowedLanguage(language)
      ? language
      : null;

  let applied = false;
  const patch = (existing: EntryRecord): EntryRecord => {
    if (existing.id !== entryId) return existing;
    applied = true;
    return { ...existing, code_language: safeLanguage };
  };

  const nextEntries = entries.map(patch);
  const nextVisibleEntries = options.isFiltering
    ? visibleEntries.map(patch)
    : nextEntries;

  return { nextEntries, nextVisibleEntries, applied };
}

/**
 * Patch the Quick Paste projections (`recent`, `hits`) for the
 * entry id the backend returned. The helper is the Quick Paste
 * analogue of `applyCodeLanguageToDesktopProjections` and mirrors
 * its invariants — every other row keeps its previous identity
 * (titles, tags, favourites, source-app, thumbnails, image
 * metadata, snippets).
 */
export function applyCodeLanguageToRecentAndHits(
  recent: readonly EntryRecord[],
  hits: readonly SearchHit[],
  entryId: number,
  language: string | null,
): { nextRecent: EntryRecord[]; nextHits: SearchHit[]; applied: boolean } {
  const safeLanguage =
    typeof language === "string" && isAllowedLanguage(language)
      ? language
      : null;

  let applied = false;
  const nextRecent = recent.map((existing) => {
    if (existing.id !== entryId) return existing;
    applied = true;
    return { ...existing, code_language: safeLanguage };
  });
  const nextHits = hits.map((hit) => {
    if (hit.entry_id !== entryId) return hit;
    applied = true;
    return {
      ...hit,
      record: { ...hit.record, code_language: safeLanguage },
    };
  });
  return { nextRecent, nextHits, applied };
}
