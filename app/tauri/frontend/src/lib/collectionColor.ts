// Pure helper for normalising the persisted `Collection.color_hex`
// value the `clipboard-history-cards` change renders as a chip's text
// colour and as the modal's swatch.
//
// The backend rejects malformed values at write time, but a legacy
// database that pre-dates the colour migration, a manual schema edit
// or any other unforeseen regression can land in the rail with a value
// we cannot trust. Returning a safe fallback keeps the card readable
// without ever leaking clipboard content or collection metadata to
// the user. The fallback is intentionally a neutral grey that reads
// as "missing data" rather than as a meaningful colour so a future
// regression surfaces visually instead of being mistaken for a
// legitimate assignment.

const FALLBACK_HEX = "#94a3b8";
const HEX_RE = /^#[0-9a-f]{6}$/;

/**
 * Validate and normalise a `Collection.color_hex` value. Returns the
 * lowercase `#rrggbb` representation when the input matches the
 * canonical opaque RGB hex shape; returns a neutral fallback for
 * any other shape (null, undefined, non-strings, alpha values, named
 * colours, short forms, whitespace, missing `#`). The helper never
 * throws so the card surface can branch on the result without
 * wrapping the call in a try/catch.
 */
export function collectionColor(value: unknown): string {
  if (typeof value !== "string") return FALLBACK_HEX;
  const body = value.trim().toLowerCase();
  if (HEX_RE.test(body)) return body;
  return FALLBACK_HEX;
}

/**
 * Stable fallback the card and the modal surface when a value cannot
 * be trusted. Exposed so tests can pin the exact constant instead of
 * duplicating the literal.
 */
export const COLLECTION_COLOR_FALLBACK = FALLBACK_HEX;

/**
 * Whether the supplied value matches the canonical `#rrggbb` shape.
 * Pure predicate the rail and the modal share so the render surface
 * never has to re-implement the regex.
 */
export function isValidCollectionColor(value: unknown): value is string {
  return typeof value === "string" && HEX_RE.test(value.trim().toLowerCase());
}