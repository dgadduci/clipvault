# Tasks: adaptive image preview and import progress

## OpenSpec

- [x] 1.1 Define the responsive image preview size, viewport bounds and 10 px
  inner spacing while preserving QuickPaste's fixed window geometry.
- [x] 1.2 Define per-file import progress and the metadata-only event contract.
- [x] 1.3 Define byte-based import progress with a file-processing phase and
  metadata-only counters.
- [x] 1.4 Define accessible image-preview resizing and viewport bounds while
  preserving QuickPaste's fixed native window.
- [x] 1.5 Specify that outside interaction keeps the preview open and that
  only the close button or `Escape` dismisses it.

## Adaptive image preview

- [x] 2.1 Add proportional sizing for the shared image preview using intrinsic
  dimensions, a maximum 80% scale and available viewport bounds.
- [x] 2.2 Fit image preview card width to the rendered image and apply 10 px
  inner spacing without changing text preview geometry.
- [x] 2.3 Recalculate image dimensions after rotation and viewport resizing.
- [x] 2.4 Add a pointer and keyboard resize control to image previews, clamp the
  card to the available viewport, and refit the image without distortion.
- [x] 2.5 Remove outside-click dismissal so releasing a resize drag beyond the
  modal does not close the preview; retain close-button and `Escape` behavior.

## Image import progress

- [x] 3.1 Add a core progress callback invoked after each file outcome while
  preserving the existing ordered-import API.
- [x] 3.2 Emit a session-scoped progress event from the Tauri import command
  using only counts and the session identifier.
- [x] 3.3 Subscribe before import, ignore unrelated sessions and display an
  accessible localized progress bar and percentage beside the modal actions.
- [x] 3.4 Add matching progress label translations to `en`, `es`, `pt`, `de`
  and `fr`.
- [x] 3.5 Count bytes read by the image decoder and report cumulative batch
  bytes alongside completed-file counters.
- [x] 3.6 Extend the session-scoped Tauri event with byte counters and throttle
  read updates while preserving the metadata-only contract.
- [x] 3.7 Compute byte-based progress with a completed-file processing phase;
  keep the bar below 100% until the import command finishes.
- [x] 3.8 Localize the accessible resize-control label in `en`, `es`, `pt`,
  `de` and `fr`.

## Review

- [x] 4.1 Run relevant frontend and core checks, frontend preview/import and
  drag-and-drop regressions, OpenSpec validation and `git diff --check`.
- [x] 4.2 Review viewport sizing, rotation resizing, event privacy and all
  five translation catalogs.
- [x] 4.3 Record manual preview and multi-file import verification status.
- [x] 4.4 Run relevant core and frontend checks, including drag-and-drop
  regressions, strict OpenSpec validation and `git diff --check`.
- [x] 4.5 Review byte-progress privacy, resize bounds and keyboard accessibility;
  record manual checks that require a desktop session.
- [ ] 4.6 Verify in Tauri that releasing the resize drag outside the modal keeps
  the preview open and that the close button and `Escape` still dismiss it.

## Verification outcome — byte progress and resizing

- `cargo test -p clipvault-core --lib ordered_image_import_reports_`: passed
  (2 tests, including byte updates during decoding and per-file completion).
- `cargo check --manifest-path app/tauri/src-tauri/Cargo.toml`: passed.
- `npm run build`: passed; Svelte reported 0 errors and 18 existing warnings.
- Nine focused frontend suites passed, covering progress calculation, preview
  sizing and accessibility, plus pointer drag-and-drop regressions.
- `cargo fmt --all --check`, `git diff --check` and strict OpenSpec validation
  passed. All five locale catalogs retain matching keys and placeholders.
- The full `npm test` run reported failures in source-contract suites; one was
  the expected value in the new progress test and was corrected. Focused feature
  and drag-and-drop suites pass after that correction. The other reported
  failures assert toolbar, selection or legacy preview details, and are not
  assertions for byte progress or preview resizing.
- Manual Tauri checks for pointer/keyboard resizing and live byte progress
  remain pending for a desktop session.

## Verification outcome

- `npm run build`: passed; Svelte reported 0 errors and 18 warnings across 12
  files. No warning points to the changed preview or import modal.
- `cargo check -p clipvault-core` and
  `cargo check --manifest-path app/tauri/src-tauri/Cargo.toml`: passed.
- The core ordered-import progress test passed. Six selected frontend test
  suites passed, including image sizing, shared preview, Quick Paste, import
  progress and drag-and-drop regressions.
- `cargo fmt --all --check`, strict OpenSpec validation and `git diff --check`:
  passed. All five locale catalogs contain matching keys and placeholders.
- Manual Tauri visual checks for portrait/landscape sizing, rotation resizing
  and a multi-file import remain pending for a desktop session.
