// Shared layout tokens for the desktop toolbar comboboxes.
//
// The source-application filter uses its reduced 168.3px width.
// Tag, collection and capture-type filters use a separate compact token.

import { COLLECTION_ROW_TINT_OPACITY } from "./collectionColor.ts";

export const FILTER_WIDTH_PX = 168.3;
export const FILTER_MIN_WIDTH_REM = "8.415rem";
export const COMPACT_FILTER_WIDTH_PX = 110;
export const COMPACT_FILTER_MIN_WIDTH_REM = "5.5rem";

/** Category colors for selected desktop toolbar filters. */
export const ACTIVE_FILTER_SURFACES = {
  sourceApp: `rgba(37, 99, 235, ${COLLECTION_ROW_TINT_OPACITY})`,
  tag: `rgba(109, 40, 217, ${COLLECTION_ROW_TINT_OPACITY})`,
  collection: `rgba(15, 118, 110, ${COLLECTION_ROW_TINT_OPACITY})`,
  contentType: `rgba(180, 83, 9, ${COLLECTION_ROW_TINT_OPACITY})`,
} as const;

/**
 * CSS declaration block used by the `.source-app-filter` root. The block is
 * intentionally a string so the component can compose canonical
 * width declarations with its active-surface custom property.
 */
export const filterComboboxStyles = `
  flex: 0 0 ${FILTER_WIDTH_PX}px;
  min-width: ${FILTER_MIN_WIDTH_REM};
  max-width: ${FILTER_WIDTH_PX}px;
  width: ${FILTER_WIDTH_PX}px;
`;

/** Width declarations for the compact tag, collection and type filters. */
export const compactFilterComboboxStyles = `
  flex: 0 0 ${COMPACT_FILTER_WIDTH_PX}px;
  min-width: ${COMPACT_FILTER_MIN_WIDTH_REM};
  max-width: ${COMPACT_FILTER_WIDTH_PX}px;
  width: ${COMPACT_FILTER_WIDTH_PX}px;
`;
