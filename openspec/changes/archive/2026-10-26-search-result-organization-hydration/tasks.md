# Tasks

## OpenSpec

- [x] 1.1 Record search-result metadata hydration and cache-retention behavior.

## Implementation

- [x] 2.1 Hydrate every visible search result, regardless of subcollection
  visibility settings.
- [x] 2.2 Reconcile organization metadata against the visible search page so
  a base-page refresh does not discard labels for search results.
- [x] 2.3 Add focused regressions for search results outside the base page and
  subsequent hydration while search is active.
- [x] 2.4 Include subcollections as parent-qualified options in the History
  collection filter and retain their ids for filtering.
- [x] 2.5 Query tag filter options across the full active collection scope,
  without applying pagination or search.
- [x] 2.6 Refresh tag options on scope and organization changes while keeping
  page changes from narrowing the option list.
- [x] 2.7 Add frontend and persistence regressions for hierarchical collection
  options and collection-wide tag options.

## Verification

- [x] 3.1 Run focused frontend/backend tests, frontend check/build and protected drag
  and drop regressions.
- [x] 3.2 Validate OpenSpec strictly and review `git diff --check`.

### Verification note

The focused filter, organization-hydration, collection-tree, database, and
protected drag and drop regressions passed. `cargo check` passed for core and
Tauri. The frontend production build passed with 17 existing Svelte warnings.
The full frontend suite reported 118 passes and 7 failures in unrelated test
files (`editableTextCaptures`, `keyboardShortcuts`, `peerImportReadinessFrontend`,
`peerPresenceRefresh`, `previewInteractionRegressions`,
`quickPastePreviewRegressions`, and `quickPasteSelectionAndScroll`).
