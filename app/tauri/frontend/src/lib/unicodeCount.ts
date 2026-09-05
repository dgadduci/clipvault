// Deterministic Unicode-aware character counter used by the card's
// metadata row. The helper is intentionally local — no DOM, no
// platform-specific count — so it can render before the card mounts
// and the unit tests can exercise every branch.
//
// The counting rule mirrors the one documented in the
// `desktop-collection-card-polish` change: count Unicode code points,
// not UTF-16 code units, so a row with emoji or supplementary
// characters reports the same number the user perceives. The
// implementation uses `Array.from`, which iterates code points (and
// surrogate pairs) without needing a regex.

/**
 * Count the Unicode code points in `value`. `null` / `undefined` and
 * non-string inputs return `0` so the helper never throws.
 *
 * Multi-codepoint emoji (e.g. 👨‍👩‍👧) count as a single character so the
 * user sees the same number the renderer paints on the card.
 */
export function unicodeCount(value: unknown): number {
  if (typeof value !== "string") return 0;
  return Array.from(value).length;
}
