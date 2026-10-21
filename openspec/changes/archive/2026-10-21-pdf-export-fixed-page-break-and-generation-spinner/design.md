# Design: fixed PDF page breaks and generation spinner

## Page layout

- The core always renders each selected capture as a separate document and
  appends those documents in selection order. Natural pagination may give one
  long capture multiple pages; the next capture still starts on a fresh page.
- Remove `pdf_page_break_between_captures` from `Settings`, `SettingsUpdate`,
  settings persistence, the frontend settings types, and General Settings.
  Existing rows for this obsolete key are ignored, so no migration is needed.
- Remove the renderer's boolean page-break argument. This makes the fixed
  behavior explicit at the core API boundary and prevents old persisted values
  from changing exports.

## Generation feedback

- Desktop and QuickPaste keep separate state for PDF generation and the full
  export action. The generation state controls a shared centered overlay; the
  full-action state continues to disable the PDF button through the native save
  dialog and file write.
- Before invoking the command, the frontend updates Svelte and yields a browser
  frame so the spinner can render before CPU work begins.
- After `PdfExportService::generate` returns successfully, the Tauri command
  emits a payload-free event targeted to the `WebviewWindow` that invoked it.
  The frontend dismisses the centered spinner on that event, then the command
  opens the existing native save dialog. `finally` paths clear both states for
  cancellation and errors.
- If listening for the completion event fails, export still proceeds and the
  spinner falls back to clearing when the command returns. The event contains
  no IDs, capture content, paths, filenames, or image data.
- Reuse `CircularLoadingIndicator` in a fixed viewport overlay with a localized
  accessible label. The overlay is scoped to the invoking webview, so the other
  selection surface does not show progress for this export.

## Verification

- Cover fixed page boundaries for ordered text and image captures, including a
  capture whose content naturally spans pages.
- Cover settings serialization/update behavior after removing the obsolete
  field and verify all five catalogs no longer expose its setting strings.
- Add focused frontend regressions for centered spinner visibility during
  generation, event-driven dismissal before the save dialog, and button
  disabling through the full export action on both surfaces.
- Run relevant core and frontend checks, strict OpenSpec validation, and
  `git diff --check`. Preserve existing card drag-and-drop regressions because
  the Desktop and QuickPaste components are touched.
