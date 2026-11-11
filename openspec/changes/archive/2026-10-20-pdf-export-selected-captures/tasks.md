# Tasks: pdf-export-selected-captures

## 1. PDF service and settings

- [x] 1.1 Add the `printpdf` dependency and a core PDF export service that
  consumes ordered capture IDs, reads records and validated PNG assets, and
  returns one PDF document without involving Tauri or the frontend; bundle a
  Unicode font with its license notice for deterministic output.
- [x] 1.2 Render text/rich text as escaped plain text with line breaks and
  images as PNG; support ordered flow and a page break before each capture
  when configured.
- [x] 1.3 Add `pdf_page_break_between_captures` to settings, default it to
  `true`, and persist/load it through `SettingsService`.
- [x] 1.4 Add focused core tests for content order, one-document output,
  text line breaks, PNG inclusion, pagination modes, duplicate IDs and
  invalid/missing content.
- [x] 1.5 Add settings tests for the default and persisted round-trip.

## 2. Native save adapter

- [x] 2.1 Add `tauri-plugin-dialog` 2.8.3 and register it in the application
  builder.
- [x] 2.2 Add a thin Tauri command that delegates generation to the core,
  opens one native `.pdf` save dialog after generation and writes exactly one
  file to the selected path; cancellation writes nothing.
- [x] 2.3 Add a frontend command wrapper with typed saved/cancelled results;
  surface command failures through one localized error and do not pass capture
  content or image bytes through UI state.

## 3. User interface

- [x] 3.1 Add the red semi-transparent PDF button to Desktop between the
  overflow menu and delete button, using the collection-row tint opacity.
- [x] 3.2 Add the PDF action to QuickPaste alongside the selected-count
  indicator and preserve selection order from the existing ordered set.
- [x] 3.3 Add the general setting for page breaks between captures, with
  default on and immediate persistence.
- [x] 3.4 Add and verify all button, settings, cancellation and error strings
  in `en`, `es`, `pt`, `de` and `fr`.
- [x] 3.5 Add focused frontend checks for button placement/state, one-click
  command dispatch, selected ID order and persisted settings control.

## 4. Review and verification

- [x] 4.1 Run relevant Rust unit/integration tests and frontend static check,
  protected drag-and-drop regressions and build checks.
- [x] 4.2 Run strict OpenSpec validation and `git diff --check`.
- [x] 4.3 Review the full diff for local-only file handling, no content/path
  logging, preserved capture assets and no generated files.
- [x] 4.4 Record whether the native save dialog could be checked manually in
  this environment.

## 5. User acceptance

- [x] 5.1 Record the user's manual approval of the PDF export change
  (2026-10-20).

## Verification outcome

- Core PDF tests: 6 passed. Page-break default/persistence tests: 2 passed.
- `cargo check -p clipvault-app` passed. `npm run build` passed; the static
  check reported 0 errors and existing accessibility/style warnings.
- Focused PDF frontend tests: 4 passed. Protected drag-and-drop regressions:
  4 test files passed.
- The full frontend suite reported 111 passing and 4 failing test files:
  `editableTextCaptures`, `peerImportReadinessFrontend`,
  `previewInteractionRegressions` and `quickPasteSelectionAndScroll`. Three
  contain existing selection/preview assertions that do not match this
  worktree; `peerImportReadinessFrontend` uses `Promise.withResolvers`, which
  is unavailable in the environment's Node 20.20.2 (the frontend requires
  Node 24 or newer). The PDF-focused checks and protected drag-and-drop checks
  passed.
- The native save dialog was not exercised manually; its Tauri integration
  compiled successfully. This environment's build checks do not interact with
  a visible native dialog.
- The user reported the manual PDF test as approved on 2026-10-20.
- `openspec validate --strict pdf-export-selected-captures`,
  `cargo fmt --all -- --check` and `git diff --check` passed.
