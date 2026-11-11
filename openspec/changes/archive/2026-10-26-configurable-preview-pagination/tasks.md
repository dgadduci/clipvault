# Tasks

## OpenSpec

- [x] 1.1 Record local History/collection pagination, shared page-size
  persistence, filtering/search behavior, and remote import merge limits.

## Settings

- [x] 2.1 Add a validated, persisted `preview_page_size` setting with default
  20 and range 1–50.
- [x] 2.2 Add the General Settings control and translations in `en`, `es`,
  `pt`, `de`, and `fr`; notify the active desktop view after saving.

## Local pagination

- [x] 3.1 Add filtered local query offset support and a lookahead row for
  detecting the next page.
- [x] 3.2 Add offset paging for ranked search results.
- [x] 3.3 Add Previous/Next controls for History and every local collection;
  reset to page one when the collection, filters, search or page size changes.
- [x] 3.4 Keep the paginated local preview rail in one horizontal row with
  horizontal scrolling and controls outside the rail.

## Import preview pagination

- [x] 4.1 Pass the configured page size to both remote text and image browse
  endpoints and apply it to merged pages and stream buffers.
- [x] 4.2 Reset the active remote preview to its first page when the setting
  changes.

## Regression and verification

- [x] 5.1 Add focused tests for settings validation/persistence, local query
  offsets, search offsets, page navigation, configured remote merge sizes,
  and the horizontal local preview rail.
- [x] 5.2 Run affected backend/frontend tests, frontend check/build and the
  protected drag and drop regressions.
- [x] 5.3 Validate OpenSpec strictly and review `git diff --check`.

### Verification note

Focused pagination, settings, search, remote-merge and drag-and-drop tests
passed. The horizontal-rail and protected drag-and-drop regressions pass 7/7.
The full frontend suite reports 119 passing and 6 failures in
`editableTextCaptures`, `keyboardShortcuts`, `peerImportReadinessFrontend`,
`previewInteractionRegressions`, `quickPastePreviewRegressions`, and
`quickPasteSelectionAndScroll`. The peer readiness test calls
`Promise.withResolvers`, unavailable in this workspace's Node 20.20.2 runtime
(the frontend package requires Node 24). The frontend check/build and strict
OpenSpec validation pass.
