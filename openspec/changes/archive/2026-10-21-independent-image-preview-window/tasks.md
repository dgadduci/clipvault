# Tasks: independent image preview window

## OpenSpec

- [x] 1.1 Specify an independent, resizable image-preview window for Desktop
  and QuickPaste while preserving the fixed QuickPaste geometry.
- [x] 1.2 Specify ID-only loading, explicit close behavior and focus restoration.
- [x] 1.3 Document the native-window sizing decision and its monitor bounds.
- [x] 1.4 Update the requirements after manual testing: direct rendering,
  automatic image fitting, native-window rotation controls and reliable close.
- [x] 1.5 Record that native preview resizing can upscale beyond the image's
  intrinsic dimensions while preserving aspect ratio, and that closing uses
  the OS control or Escape without an in-content close button.

## Core and Tauri

- [x] 2.1 Add a core history-service lookup by entry ID and a representative
  test for existing and missing captures.
- [x] 2.2 Add a thin Tauri command that returns one entry by ID.
- [x] 2.3 Allow the preview surfaces to create the dedicated webview window and
  configure the preview label's close permissions.
- [x] 2.4 Let the dedicated preview window close instead of applying the
  application-wide hide-on-close behavior.
- [x] 2.5 Make native close requests for the preview reliably destroy the
  preview window and return focus to its source.

## Frontend

- [x] 3.1 Add a tested pure helper for initial preview-window sizing, preserving
  image proportions and fitting within the available display area.
- [x] 3.2 Add the dedicated preview page and load its entry through the ID-only
  backend command.
- [x] 3.3 Add one shared bridge to open or replace the image-preview window,
  pass only the entry ID and origin, and return focus when it closes.
- [x] 3.4 Route image previews from Desktop and QuickPaste to the new window;
  keep text and rich-text previews in their current overlay.
- [x] 3.5 Fit the image automatically when the native preview window changes
  size while preserving its aspect ratio.
- [x] 3.6 Add the localized native window title to `en`, `es`, `pt`, `de` and
  `fr`; reuse the existing unavailable-capture message in all locales.
- [x] 3.7 Render image previews directly in the native window without a nested
  modal, remove the inner drag-to-zoom control, and keep rotation and close
  controls in the native window content.
- [x] 3.8 Make the image use the available native-window content area and
  automatically refit on resize and after rotation.
- [x] 3.9 Allow native-window auto-fit to upscale beyond intrinsic image size
  while preserving aspect ratio.
- [x] 3.10 Remove the extra in-content close button from the native image
  preview; retain native-window close and Escape behavior.

## Review

- [x] 4.1 Run relevant core tests, frontend tests, Tauri check, frontend check
  and build, OpenSpec strict validation and `git diff --check`.
- [x] 4.2 Initial manual verification exposed the nested modal and broken close
  behavior; record the requested direct-view and auto-fit corrections in the
  spec.
- [x] 4.3 Manual verification approved by the user: no nested modal or in-content close
  button, image auto-fit including upscaling and proportions, rotation
  controls, Escape and native OS close control (2026-10-21).
- [x] 4.4 Run focused preview regressions, frontend check/build, Rust check,
  strict OpenSpec validation and `git diff --check`; record that the full
  frontend suite still reports failures in unrelated existing areas.
- [x] 4.5 Add and pass focused regressions for upscaling past intrinsic image
  size and for removing in-content close controls from the native preview.
