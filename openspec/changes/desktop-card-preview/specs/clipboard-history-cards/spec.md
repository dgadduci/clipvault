## ADDED Requirements

### Requirement: Preview capture from Desktop cards

Every Desktop history card SHALL expose a read-only `Previsualizar` action in
its existing menu and SHALL support the same platform-aware shortcut used by
Quick Paste: `Cmd+Enter` on macOS and `Ctrl+Enter` on Linux when the card is
focused. Both entry points SHALL open the same shared preview implementation
used by Quick Paste.

#### Scenario: Preview text from the card menu

- **WHEN** the user opens a text card menu and chooses `Previsualizar`
- **THEN** the Desktop opens the shared preview overlay for that exact entry,
  closes the card menu once and leaves the clipboard, history and card
  metadata unchanged

#### Scenario: Preview image from the card menu

- **WHEN** the user opens an image card menu and chooses `Previsualizar`
- **THEN** the Desktop opens the shared image preview and displays the complete
  persisted image inside the bounded preview area without changing the asset,
  its metadata or the clipboard

#### Scenario: Preview with the platform shortcut

- **WHEN** a Desktop card has focus and the user presses `Cmd+Enter` on macOS
  or `Ctrl+Enter` on Linux
- **THEN** the shared preview opens for that card without invoking copy, paste,
  pin, delete, title editing or drag-and-drop

#### Scenario: Invalid shortcut context is ignored

- **WHEN** the shortcut is pressed with no focused card or while focus is in a
  text editor, input, menu, button or other interactive control
- **THEN** no preview opens and the active control keeps its normal behavior

### Requirement: Shared preview behavior and presentation

Desktop and Quick Paste SHALL render text, rich text and image previews through
one shared component/helper path. The shared preview SHALL preserve the
existing Quick Paste behavior, including full safe text, existing rich-preview
policy, validated image assets, loading/error states, bounded scrolling and
consistent type/title/source metadata.

#### Scenario: Full text is not truncated

- **WHEN** a captured text entry is longer than the card or row summary and
  the user opens its preview
- **THEN** the preview shows the complete canonical text with internal scroll
  and never uses the truncated card/row summary as its source

#### Scenario: Text preview is safe

- **WHEN** the captured text contains HTML-active characters or markup-like
  content
- **THEN** the preview displays it as inert text according to the existing
  escaping policy and executes no script, handler or navigation

#### Scenario: Image preview preserves the asset

- **WHEN** the preview loads a persisted image asset
- **THEN** it uses the existing validated asset bridge, shows the full image
  with contain-style layout, preserves intrinsic dimensions and releases its
  Blob URL when the preview closes or changes entry

#### Scenario: Preview asset failure is isolated

- **WHEN** the text or image preview asset is unavailable, invalid or resolves
  after the entry changed
- **THEN** the preview shows the existing safe fallback or loading/error state,
  ignores stale responses and leaves other cards and Quick Paste usable

#### Scenario: Shared implementation is actually reused

- **WHEN** tests inspect or exercise Desktop and Quick Paste preview flows
- **THEN** both surfaces call the same preview component/helpers and there is
  no duplicate renderer, sanitizer, shortcut matcher or asset lifecycle

### Requirement: Accessible preview lifecycle

The Desktop preview SHALL behave as a bounded accessible overlay without
changing the fixed geometry of the application.

#### Scenario: Escape closes the preview first

- **WHEN** the Desktop preview is open and the user presses Escape
- **THEN** the preview closes first, the underlying Desktop remains available,
  and focus returns to the triggering card/control when possible

#### Scenario: Outside click closes the preview

- **WHEN** the user clicks outside the preview surface
- **THEN** the preview closes without mutating the entry; a click inside the
  preview does not close it accidentally

#### Scenario: Repeated open and close is leak-free

- **WHEN** the user opens and closes previews repeatedly or changes the active
  collection while one is open
- **THEN** only one preview/listener set remains active, stale entries are not
  rendered, and all owned Blob URLs/listeners are cleaned up

### Requirement: Card contracts remain intact

Adding Desktop preview SHALL preserve the existing card, organization,
search, image, paste and drag-and-drop contracts.

#### Scenario: Preview does not mutate the entry

- **WHEN** the user opens, reads and closes a preview
- **THEN** content, title, timestamp, favorite state, tags, collections,
  source-app metadata, asset references and clipboard contents remain unchanged

#### Scenario: Existing card controls remain isolated

- **WHEN** the user activates pin, title editor, paste, delete, tags,
  collections or drag-and-drop controls
- **THEN** those controls perform their existing actions and do not open a
  preview unless the user explicitly chooses `Previsualizar` or the shortcut
  on the card surface

#### Scenario: Search and collection scope remain intact

- **WHEN** a preview is opened from a search result, Historial or a collection
- **THEN** the preview shows that exact visible entry and does not replace,
  broaden or reorder the active card scope

## UNCHANGED Requirements

The existing Quick Paste window, its fixed geometry, search, selection,
copy-only behavior, direct menu actions, focus-loss close, image fidelity,
title/content search, tags, collections, favorites, persistence and
drag-and-drop behavior remain unchanged except for consuming the shared
preview implementation.
