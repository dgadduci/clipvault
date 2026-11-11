# Tasks: rotate image captures from preview

## OpenSpec and behavior

- [x] 1.1 Define persistent 90° clockwise and counterclockwise rotation in the
  shared Desktop and QuickPaste preview, including error and accessibility
  behavior.
- [x] 1.2 Document the conditional same-entry update, content-addressed asset
  ownership and metadata preserved by rotation.

## Core persistence

- [x] 2.1 Add an image-asset replacement operation to the entry repository that
  checks the expected old reference and updates only image payload metadata.
- [x] 2.2 Implement pixel rotation and PNG normalization in the history service;
  persist the new asset and collect an old asset only after the database commit
  proves no entries reference it.
- [x] 2.3 Return typed outcomes and the refreshed metadata-only entry while
  preserving the original entry identity and unrelated metadata.

## Tauri and preview controls

- [x] 3.1 Add a thin Tauri command that accepts an entry ID and rotation
  direction, delegates to the core and emits the history update event after a
  successful commit.
- [x] 3.2 Add clockwise/counterclockwise icon buttons to the shared image
  preview; refresh the displayed asset after success and leave it unchanged on
  failure.
- [x] 3.3 Add localized accessible labels and rotation error text in all five
  locale catalogs.
- [x] 3.4 Show an accessible spinner from the start of rotation through the
  refreshed PNG load, keeping the initial image-loading text for first opens.

## Review

- [x] 4.1 Run frontend static checks, relevant Rust `cargo check` commands,
  strict OpenSpec validation and `git diff --check`; do not run test suites.
- [x] 4.2 Review the scoped diff for asset ownership, preserved capture
  metadata, stale responses and localized product text.
- [x] 4.3 Record that manual rotation, close/reopen and app-restart checks
  remain pending for a desktop session.

## Verification outcome

- `npm run check`: passed with 0 errors; Svelte reported 18 warnings across 12
  files. The preview dialog's accessibility warning was resolved.
- `cargo check -p clipvault-core`: passed.
- `cargo check --manifest-path app/tauri/src-tauri/Cargo.toml`: passed.
- `cargo fmt --all --check`, strict OpenSpec validation, locale-key parity and
  `git diff --check`: passed.
- Test suites were not run. Manual UI and restart verification remains
  pending.
- The rotation/reload spinner refinement passes `npm run check` with 0 errors
  and the existing 18 Svelte warnings; strict OpenSpec validation and
  `git diff --check` pass.
- Manual Tauri UI and restart verification was not performed; it remains a
  follow-up for a desktop session.
