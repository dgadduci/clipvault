## OpenSpec

- [x] 1.1 Specify image-only zoom, visible rotation controls, button placement,
  bounds and reset behavior.
- [x] 1.2 Document touchpad gesture interception for supported webviews and
  localization requirements.

## Image zoom behavior

- [x] 2.1 Add a pure zoom-scale helper with bounded scale and focused boundary
  tests.
- [x] 2.2 Apply zoom transforms only to the image and reset zoom when the
  preview switches to another capture.
- [x] 2.3 Intercept touchpad zoom events for the native image preview and clean
  up listeners when the component is destroyed.
- [x] 2.4 Add zoom-in and zoom-out icon buttons beside rotation controls, with
  localized accessible names and boundary states.
- [x] 2.5 Add regression coverage for image-only scaling and toolbar visibility.
- [x] 2.6 Update all five locale catalogs and validate key parity.
- [x] 2.7 Enable zoom buttons reactively after image loading and intercept
  native webview zoom before applying image-only zoom.
- [x] 2.8 Preserve the horizontal capture rail and existing typography sizes;
  restore white collection badge numbers.

## Verification

- [x] 3.1 Run focused frontend tests, protected drag-and-drop regressions,
  frontend build, strict OpenSpec validation, Tauri permission compilation and
  `git diff --check`.
- [x] 3.2 Re-run the manual native preview test: zoom controls work, the
  touchpad affects only the image, and rotation controls stay visible.
- [x] 3.3 Confirm captures remain horizontal, desktop typography stays at its
  established scale and collection badge numbers are white.

## Verification outcome

- Strict OpenSpec validation, locale parity (873 keys), `git diff --check`,
  `npm run check` (0 errors; 18 existing warnings), `npm run build` and
  `cargo check -p clipvault-app --locked --offline` passed.
- Focused zoom, native image-window, shared preview and desktop visual tests
  passed (5 suites); the protected drag-and-drop regressions passed (3 suites).
- The existing `quickPastePreviewRegressions.test.ts` still has one known
  baseline failure: its Fix 4 assertion expects canonical preview text logic in
  `QuickPaste.svelte`, although the shared `ClipboardPreview.svelte` owns that
  behavior. The failure is unrelated to the page-zoom guard added here.
- Manual test approved: touchpad and toolbar zoom affect only the image,
  rotation controls remain visible, the capture rail is horizontal, desktop
  typography retains its established scale, and collection badge numbers are
  white.
