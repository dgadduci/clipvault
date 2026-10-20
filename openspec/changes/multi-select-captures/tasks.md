## 1. OpenSpec and selection model

- [x] 1.1 Record desktop, QuickPaste, selection gestures, reset rules and
  single-item action compatibility in the change artifacts.
- [x] 1.2 Add small pure helpers for toggling selected IDs, selecting an
  inclusive range and intersecting selection with visible IDs.

## 2. Desktop multi-selection and grouped drop

- [x] 2.1 Extend `HistoryCardRail` and `HistoryCard` to render and update
  multi-selection while preserving the active-card shortcuts and card
  attributes.
- [x] 2.2 Extend the singleton pointer/mouse drag session to capture the
  selected visible IDs in memory while leaving DataTransfer's opaque
  initiating-entry ID unchanged.
- [x] 2.3 Extend the delegated collection drop zone and Desktop handler to add
  the target collection to every valid selected capture using existing
  membership merging and refresh behavior.
- [x] 2.4 Keep Historial, other associations, peer-bound target rejection and
  cleanup behavior intact for grouped drops.

## 3. QuickPaste multi-selection

- [x] 3.1 Add modifier-click and range selection to QuickPaste rows, including
  selection cleanup when result IDs change.
- [x] 3.2 Preserve current single-result copy-only behavior for plain click and
  Enter when zero or one result is selected; selection gestures must not
  invoke copy/paste or hide the window.
- [x] 3.3 Add sequential QuickPaste paste-stack confirmation for multiple
  selected results, stop on first failure and report partial completion.
- [x] 3.4 Localize any new accessible selection and partial-failure text in all
  five catalogs.
- [x] 3.5 Add an explicit per-result selection control to QuickPaste so
  multi-selection does not depend on modifier-click gestures.

## 4. Review and verification

- [x] 4.1 Review selection toggles, range handling, filtering, grouped drag
  membership and paste-stack order against the documented contracts.
- [x] 4.2 Run the frontend static check/build, strict OpenSpec validation and
  `git diff --check`.
- [x] 4.3 Review the full diff and confirm no clipboard contents or image data
  enter drag payloads, and no generated files or persisted image assets change.
- [ ] 4.4 Perform manual Desktop and QuickPaste checks when a desktop session is
  available; record any platform-specific check that cannot be performed.
