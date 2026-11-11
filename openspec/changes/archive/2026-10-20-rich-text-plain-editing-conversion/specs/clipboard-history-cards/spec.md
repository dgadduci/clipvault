## MODIFIED Requirements

### Requirement: Open the text editor from an eligible card

Each eligible plain-text history card SHALL expose an accessible `Editar
captura` action through its existing card menu. A rich-text card SHALL expose
the same action, but SHALL require explicit confirmation before opening the
plain-text editor. Image cards SHALL NOT expose this action. Opening the menu,
warning or editor SHALL not select the card or start a drag operation.

#### Scenario: Open editor from the card menu

- **WHEN** the user opens the menu of an eligible plain-text card and chooses
  `Editar captura`
- **THEN** a modal opens with the current content loaded into a native
  `textarea`
- **AND** focus moves to the editor
- **AND** the card keeps its fixed geometry and existing actions

#### Scenario: Rich card requires confirmation before opening editor

- **WHEN** the user chooses `Editar captura` from a rich-text card menu
- **THEN** an accessible warning explains that saving modified text converts
  the capture to plain text and removes its rich representations
- **AND** confirming opens the existing text editor with the canonical plain
  text and resolved capture title
- **AND** the card keeps its fixed geometry and existing actions

#### Scenario: Cancel rich-card confirmation

- **WHEN** the user cancels or closes the rich-text warning
- **THEN** the editor does not open and the history entry remains unchanged
- **AND** focus returns to the card menu trigger

#### Scenario: Edit modal is accessible and cancellable

- **WHEN** the text editor modal is open
- **THEN** it has an accessible title, labelled editor, Guardar and Cancelar
  controls, visible validation errors and a single submit path
- **AND** for a rich conversion Guardar is disabled while the draft is empty
  or byte-for-byte equal to the original canonical text
- **AND** Escape, backdrop, close button or Cancelar closes it without saving
- **AND** focus returns to the menu trigger

#### Scenario: Successful card edit refreshes the card

- **WHEN** the user saves valid changed text
- **THEN** the card renders the updated preview and metadata returned by the
  backend
- **AND** tags, collections, title, favorite state and source-app presentation
  remain unchanged
- **AND** the updated entry is available to search and Quick Paste

#### Scenario: Editor controls do not affect card interactions

- **WHEN** the user types, selects text, presses a key or clicks inside the
  editor
- **THEN** the card surface does not change selection and the drag controller
  does not start
- **AND** the protected card attributes and opaque drag payload remain intact

#### Scenario: Reopen the editor on the same card after closing

- **WHEN** the user closes the editor through Guardar, Cancelar, Escape,
  backdrop or the close button
- **AND** then reopens `Editar captura` on the same card
- **THEN** the modal opens with a draft re-seeded from the persisted entry
- **AND** focus moves into the editor and returns to the menu trigger when the
  modal closes again
- **AND** the open / close / open cycle does not require re-mounting the card

### Requirement: Identify the capture in the editor and expose its shortcut

The text editor modal SHALL use the card's resolved capture title as its
visible dialog title, including the existing fallback when the capture has no
custom title. It SHALL NOT show the generic `Editar captura` as the dialog
title or the existing explanatory summary below the editor. Plain-text cards
and rich-text cards SHALL expose the platform edit shortcut beside
`Editar captura`; rich-text shortcut activation SHALL pass through the
conversion warning before the editor opens.

#### Scenario: Editor title is the capture title

- **WHEN** the user opens the text editor for a capture with resolved title
  `Mi captura`
- **THEN** the dialog heading is `Mi captura`
- **AND** the editor remains associated with that heading through
  `aria-labelledby`
- **AND** the removed explanatory summary is not rendered

#### Scenario: Ctrl+E opens editing for an eligible card

- **WHEN** an eligible plain-text card has keyboard focus and the user presses
  `Ctrl+E`, or a rich-text card has focus and the user presses its platform
  edit shortcut
- **THEN** the editor opens with the current capture content for plain text,
  or the conversion warning opens for rich text
- **AND** the card/menu interaction is not treated as a drag
- **AND** the `Editar captura` menu action visibly presents the matching
  shortcut and exposes its matching `aria-keyshortcuts`

#### Scenario: Shortcut warns before editing a rich-text card

- **WHEN** a rich-text card has keyboard focus and the user presses the
  platform edit shortcut
- **THEN** the conversion warning opens before the editor
- **AND** the `Editar captura` menu action visibly presents the same shortcut
  and exposes its matching `aria-keyshortcuts`

#### Scenario: Ctrl+E is ignored in ineligible or interactive contexts

- **WHEN** the focused card is an image card or the event target is an input,
  textarea, select, button, menu, contenteditable element or the editor modal
- **THEN** the text editor does not open and the focused control's normal
  behavior is preserved

### Requirement: Open text editing with the platform shortcut

An eligible plain-text history card SHALL open the existing text editor through
`Ctrl+E` on Linux, including Wayland and X11, and through `Cmd+E` on macOS. A
rich-text history card SHALL open the conversion warning through the same
platform-aware shortcut and SHALL open the editor only after confirmation.
The shortcut SHALL work when the target card is focused and when the rail's
current selection identifies the target without requiring its `article`
element to receive the event directly.

#### Scenario: Linux opens editing with Ctrl+E

- **WHEN** an eligible plain-text capture is focused or selected in the
  desktop rail on Linux
- **AND** the user presses `Ctrl+E`
- **THEN** the existing `EntryTextEditorModal` opens for that capture
- **AND** the current persisted content is loaded into the editor
- **AND** the shortcut is consumed exactly once

#### Scenario: macOS opens editing with Cmd+E

- **WHEN** an eligible plain-text capture is focused or selected in the
  desktop rail on macOS
- **AND** the user presses `Cmd+E`
- **THEN** the existing `EntryTextEditorModal` opens for that capture
- **AND** the current persisted content is loaded into the editor
- **AND** the shortcut is consumed exactly once

#### Scenario: Platform shortcut requests rich conversion

- **WHEN** a rich-text capture is focused or selected in the desktop rail on
  Linux or macOS
- **AND** the user presses `Ctrl+E` on Linux or `Cmd+E` on macOS
- **THEN** the warning opens before any editor or backend mutation
- **AND** confirming opens the editor with the canonical plain text
- **AND** cancelling leaves the capture rich and unchanged

#### Scenario: Menu hint matches the platform

- **WHEN** the `Editar captura` action is rendered for a plain-text or
  rich-text capture
- **THEN** Linux displays `Ctrl+E` and exposes `aria-keyshortcuts="Control+E"`
- **AND** macOS displays `⌘E` and exposes `aria-keyshortcuts="Meta+E"`
- **AND** both labels and the matcher use the same platform resolution

#### Scenario: Shortcut does not steal interactive input

- **WHEN** the event target is an input, textarea, select, contenteditable,
  button, menu, menuitem, title editor, selector, chip, dialog or another
  protected interactive control
- **THEN** the editor or conversion warning does not open
- **AND** the event's normal behavior is preserved
- **AND** no card selection or drag operation is started

#### Scenario: Ineligible entries show an informational notice

- **WHEN** the focused or selected entry is an image and the user presses the
  valid platform edit shortcut
- **THEN** the text editor does not open
- **AND** the shortcut is consumed so a stale card-level handler cannot open
  the last edited capture
- **AND** an informational dialog tells the user that this capture is not
  editable
- **AND** closing the dialog does not issue a backend command or mutate the
  capture

#### Scenario: Invalid modifiers and missing targets are ignored

- **WHEN** no entry is selected, or the user presses the wrong platform
  modifier, `Alt` or `Shift`
- **THEN** no editor, conversion warning or backend mutation is issued

#### Scenario: Existing card and drag baselines remain intact

- **WHEN** the shortcut listener is mounted, used, unmounted and mounted again
- **THEN** only one shared listener handles the shortcut
- **AND** `data-testid="history-card"`, `data-entry-id`,
  `draggable="false"`, pointer capture, mouse fallback, cancellation and the
  opaque ID-only drag payload remain unchanged
