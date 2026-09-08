// Shared layout tokens for the desktop toolbar comboboxes.
//
// `quick-paste-desktop-polish` introduced the `TagFilter` combobox
// between the existing `SourceAppFilter` and the overflow menu.
// The two comboboxes MUST share the exact same width so the
// toolbar never reflows when the user toggles between them or
// selects a long tag / app label. The visual tokens used to live
// inline inside each component, which caused the previous drift
// the manual QA pass detected (the tag combobox collapsed to its
// intrinsic width while the source-app combobox kept a fixed
// 220px column).
//
// The token module owns the canonical values:
//
//   - `FILTER_WIDTH_PX` is the reserved pixel width for both
//     comboboxes. The value matches the previous `SourceAppFilter`
//     baseline so the source-app combobox never reflows when the
//     tag combobox mounts.
//   - `FILTER_MIN_WIDTH_REM` is the minimum width the toolbar
//     respects when the available space shrinks. The value matches
//     the previous baseline (`11rem`).
//
// Both combobox components import this module and apply the same
// CSS rules so a future width change is a single-file edit.

export const FILTER_WIDTH_PX = 220;
export const FILTER_MIN_WIDTH_REM = "11rem";

/**
 * CSS declaration block the toolbar comboboxes paste into their
 * `.source-app-filter` / `.tag-filter` root. The block is
 * intentionally a string so the two components can compose it
 * with their own layout overrides without inheriting Svelte's
 * scoped CSS gymnastics.
 */
export const filterComboboxStyles = `
  flex: 0 1 ${FILTER_WIDTH_PX}px;
  min-width: ${FILTER_MIN_WIDTH_REM};
  max-width: ${FILTER_WIDTH_PX}px;
  width: ${FILTER_WIDTH_PX}px;
`;