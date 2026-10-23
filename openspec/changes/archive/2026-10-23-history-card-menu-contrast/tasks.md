## OpenSpec

- [x] 1.1 Specify the expected foreground colors for the card menu trigger and
  enabled option labels while preserving existing state colors.

## Implementation

- [x] 2.1 Set explicit light foreground colors for the ellipsis trigger and
  menu surface.
- [x] 2.2 Add a focused frontend regression for the card-menu colors.

## Verification

- [x] 3.1 Run frontend checks and the protected card drag-and-drop regressions.
- [x] 3.2 Run strict OpenSpec validation and `git diff --check`.

The production frontend build passed. The focused visual-integrity regression
and all four protected card drag-and-drop test files passed. The full
`npm run test` run reported 116 passing and 7 failing tests in
`desktopCardPreviewNoMutations`, `editableTextCaptures`, `keyboardShortcuts`,
`peerImportReadinessFrontend`, `previewInteractionRegressions`,
`quickPastePreviewRegressions` and `quickPasteSelectionAndScroll`; those
failures are recorded for follow-up and the focused menu and drag-and-drop
regressions remain green. Strict OpenSpec validation and `git diff --check`
passed.
