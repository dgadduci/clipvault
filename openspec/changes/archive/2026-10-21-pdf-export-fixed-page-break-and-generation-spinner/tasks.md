## 1. OpenSpec

- [x] 1.1 Specify fixed one-capture-per-page PDF layout and centered generation
  feedback for Desktop and QuickPaste.
- [x] 1.2 Document compatibility for obsolete stored preferences and the
  metadata-free, window-targeted completion signal.

## 2. PDF page layout and settings

- [x] 2.1 Make the core PDF export API always start each capture on a new page
  and update focused page-boundary coverage.
- [x] 2.2 Remove the page-break preference from settings models, persistence,
  frontend types, General Settings and all five locale catalogs.
- [x] 2.3 Update the Tauri export command to ignore obsolete stored values and
  preserve natural pagination within an individual capture.

## 3. Generation progress

- [x] 3.1 Emit a payload-free PDF-generation-complete event to the initiating
  Tauri window after document generation and before opening the save dialog.
- [x] 3.2 Add a reusable centered spinner overlay and localized accessible label.
- [x] 3.3 Track generation separately from the full export in Desktop and
  QuickPaste; show the spinner only during generation and keep each button
  disabled until the command finishes.
- [x] 3.4 Add focused frontend regressions for visibility, event dismissal,
  cancellation/error cleanup and both export surfaces.

## 4. Verification

- [x] 4.1 Run focused core and frontend tests plus protected drag-and-drop
  regressions, relevant checks/builds and locale parity validation.
- [x] 4.2 Validate the OpenSpec change strictly, review the diff, and run
  `git diff --check` without disturbing unrelated branch work.
- [x] 4.3 Record the user's manual verification approved on 2026-10-21.

## Verification outcome

- PDF core tests passed (9 tests), including fixed capture boundaries and a
  multi-page capture followed by another capture. The obsolete-setting
  compatibility test passed.
- `cargo check -p clipvault-app`, `cargo fmt --all -- --check`, strict OpenSpec
  validation, locale validation and `git diff --check` passed.
- `npm run build` passed with 0 Svelte errors and 18 existing warnings across
  unrelated components. The focused PDF frontend tests passed.
- The full frontend suite reported 114 passed and 6 failed in
  `editableTextCaptures`, `keyboardShortcuts`, `peerImportReadinessFrontend`,
  `previewInteractionRegressions`, `quickPastePreviewRegressions` and
  `quickPasteSelectionAndScroll`. The protected drag-and-drop regressions
  passed.
- The user approved the manual verification of the PDF generation spinner and
  its dismissal before the native save dialog on 2026-10-21.
