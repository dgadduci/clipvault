// Shared layout tokens for the desktop toolbar comboboxes.
//
// The source-application filter retains its established 220px width.
// Tag and collection filters use a separate compact token so their
// widths can change without changing the application filter.

export const FILTER_WIDTH_PX = 220;
export const FILTER_MIN_WIDTH_REM = "11rem";
export const COMPACT_FILTER_WIDTH_PX = 110;
export const COMPACT_FILTER_MIN_WIDTH_REM = "5.5rem";

/**
 * CSS declaration block used by the `.source-app-filter` root. The block is
 * intentionally a string so the two components can compose it
 * with their own layout overrides without inheriting Svelte's
 * scoped CSS gymnastics.
 */
export const filterComboboxStyles = `
  flex: 0 0 ${FILTER_WIDTH_PX}px;
  min-width: ${FILTER_MIN_WIDTH_REM};
  max-width: ${FILTER_WIDTH_PX}px;
  width: ${FILTER_WIDTH_PX}px;
`;

/** Width declarations for the compact tag and collection filters. */
export const compactFilterComboboxStyles = `
  flex: 0 0 ${COMPACT_FILTER_WIDTH_PX}px;
  min-width: ${COMPACT_FILTER_MIN_WIDTH_REM};
  max-width: ${COMPACT_FILTER_WIDTH_PX}px;
  width: ${COMPACT_FILTER_WIDTH_PX}px;
`;
