## 1. Toolbar layout and widths

- [x] 1.1 Keep the toolbar row constrained to its column and horizontally
  scrollable without wrapping or page-level horizontal overflow.
- [x] 1.2 Add compact 110 px / 5.5rem tokens for tag and collection filters;
  preserve the source-application filter at 220 px / 11rem.
- [x] 1.3 Set search to a 180 px base and 9rem minimum without flex growth.
- [x] 1.4 Present the create-text shortcut as a badge while preserving its
  accessible shortcut metadata, tooltip and action.
- [x] 1.5 Shorten `toolbar.search.placeholder` in all five locale catalogs and
  verify key and placeholder parity.
- [x] 1.6 Preserve the overflow menu anchor and all existing card and toolbar
  interactions.

## 2. Verification

- [x] 2.1 Run focused toolbar sizing/overflow and drag-and-drop regressions.
- [x] 2.2 Run the frontend check and build, `git diff --check`, and strict
  OpenSpec validation; review the complete diff.
- [x] 2.3 Verify horizontal scrolling, compact widths, and card-size and
  interaction preservation with the frontend layout and drag-and-drop
  regression tests. A live desktop visual pass was attempted, but the app did
  not expose a window in this session.

## 3. Full names in filter options

- [x] 3.1 Add complete-name hover tooltips to tag and collection filter items.

## 4. Tag creation and deletion in card selector

- [x] 4.1 Reset the tag selection only on modal open so a newly created tag
  stays selected during organization snapshot updates.
- [x] 4.2 Add a per-tag delete icon, accessible name and explicit localized
  confirmation in the card selector.
- [x] 4.3 Persist confirmed deletion through the existing command, remove the
  deleted ID from the local selection, and refresh every loaded card and the
  toolbar filter from the organization update event.
- [x] 4.4 Add localized delete confirmation and error strings to all five
  catalogs and verify key/placeholder parity.
- [x] 4.5 Verify cancellation and command failure leave persistent and UI
  state unchanged; run the focused selector-management and organization-update
  regressions.
