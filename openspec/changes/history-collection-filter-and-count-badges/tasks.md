## 1. Collection filter in Historial

- [x] 1.1 Add independent collection-filter state to the desktop and an
  accessible single-select control to the existing toolbar, visible in
  `Historial` with `Todas` first and user collections in sidebar order.
- [x] 1.2 Apply the selected collection immediately through the existing
  filtered recents and search paths, preserving the current source-application,
  tag and text-query filters.
- [x] 1.3 Refresh selector options after collection creation, modification and
  deletion; preserve selection by ID on modification and reset to `Todas` when
  the selected collection is deleted or the active sidebar scope changes.
- [x] 1.4 Add collection-filter labels and accessible copy to `en`, `es`, `pt`,
  `de` and `fr`, and verify locale key and placeholder parity.
- [ ] 1.5 Add focused frontend tests for keyboard use and filter composition.

## 2. Collection capture counts

- [x] 2.1 Extend the metadata-only collection snapshot with an aggregate count
  of associated captures, including a total for `Historial`, without loading
  capture bodies or querying once per collection.
- [x] 2.2 Refresh collection summaries after collection lifecycle, capture and
  membership changes through existing metadata-only update flows.
- [x] 2.3 Add a trailing numeric badge to every collection row, including zero,
  with a localized pluralized accessible name; preserve row sizing, menu,
  selection, scroll and drag-and-drop interactions.
- [ ] 2.4 Add focused backend/frontend tests for all content types, favorites,
  empty collections, lifecycle refresh and membership changes.

## 3. Verification

- [ ] 3.1 Run the focused collection-filter, toolbar, organization-summary and
  sidebar tests, including drag-and-drop regressions for collection rows.
- [x] 3.2 Verify locale key and placeholder parity, the affected frontend and
  backend checks/builds, and `git diff --check`.
- [x] 3.3 Validate this change with
  `openspec validate history-collection-filter-and-count-badges --strict
  --type change` and review the complete diff.
