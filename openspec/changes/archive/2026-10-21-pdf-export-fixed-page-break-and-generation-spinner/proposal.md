# Proposal: fixed PDF page breaks and generation spinner

## Why

PDF export currently lets users choose whether selected captures share a page,
although the desired export layout is one capture per page. The export also
provides no centered visual feedback while the PDF is being generated.

## What Changes

- Remove the page-break preference from General Settings and always start each
  selected capture on a new page, while allowing an individual capture to
  continue onto additional pages when its content needs them.
- Show an accessible rotating spinner in the center of the Desktop or
  QuickPaste window that started the export. Hide it as soon as PDF generation
  finishes, before the native save dialog opens.
- Keep the PDF action disabled until saving, cancellation or failure finishes,
  and route the generation-complete signal only to the initiating window.

## Capabilities

### Modified Capabilities

- `capture-pdf-export`: make one-capture-per-page behavior fixed and add
  generation progress feedback to both export surfaces.

## Impact

- Removes the obsolete page-break field from the settings contract and its
  localized General Settings copy; old stored values become inert.
- Updates the core PDF renderer and Tauri export adapter, plus the Desktop and
  QuickPaste frontend export flows.
- Adds no dependencies, database migration, or remote communication. The
  generation-complete event carries no capture data and targets only the
  initiating window.
