# Delta: corrección del atajo de edición de capturas

## ADDED Requirements

### Requirement: Open text editing with the platform shortcut

An eligible textual history card SHALL open the existing text editor through
`Ctrl+E` on Linux, including Wayland and X11, and through `Cmd+E` on macOS.
The shortcut SHALL work when the eligible card is focused and when the rail's
current selection identifies the eligible card without requiring the card's
`article` element to receive the event directly.

#### Scenario: Linux opens editing with Ctrl+E

- **WHEN** an eligible text capture is focused or selected in the desktop rail
  on Linux
- **AND** the user presses `Ctrl+E`
- **THEN** the existing `EntryTextEditorModal` opens for that capture
- **AND** the current persisted content is loaded into the editor
- **AND** the shortcut is consumed exactly once

#### Scenario: macOS opens editing with Cmd+E

- **WHEN** an eligible text capture is focused or selected in the desktop rail
  on macOS
- **AND** the user presses `Cmd+E`
- **THEN** the existing `EntryTextEditorModal` opens for that capture
- **AND** the current persisted content is loaded into the editor
- **AND** the shortcut is consumed exactly once

#### Scenario: Menu hint matches the platform

- **WHEN** the `Editar captura` action is rendered for an eligible capture
- **THEN** Linux displays `Ctrl+E` and exposes `aria-keyshortcuts="Control+E"`
- **AND** macOS displays `⌘E` and exposes `aria-keyshortcuts="Meta+E"`
- **AND** both labels and the matcher use the same platform resolution

#### Scenario: Shortcut does not steal interactive input

- **WHEN** the event target is an input, textarea, select, contenteditable,
  button, menu, menuitem, title editor, selector, chip, dialog or another
  protected interactive control
- **THEN** the editor does not open
- **AND** the event's normal behavior is preserved
- **AND** no card selection or drag operation is started

#### Scenario: Ineligible entries show an informational notice

- **WHEN** the focused or selected entry is an image/rich-text capture
- **AND** the user presses the valid platform edit shortcut
- **THEN** the text editor does not open
- **AND** the shortcut is consumed so a stale card-level handler cannot open
  the last edited capture
- **AND** an informational dialog tells the user that this capture is not
  editable
- **AND** closing the dialog does not issue a backend command or mutate the
  capture

#### Scenario: Invalid modifiers and missing targets are ignored

- **WHEN** no eligible entry is selected, or the user presses the wrong
  platform modifier, `Alt`, or `Shift`
- **THEN** the text editor does not open
- **AND** no backend command or state mutation is issued

#### Scenario: Existing card and drag baselines remain intact

- **WHEN** the shortcut listener is mounted, used, unmounted and mounted again
- **THEN** only one shared listener handles the shortcut
- **AND** `data-testid="history-card"`, `data-entry-id`,
  `draggable="false"`, pointer capture, mouse fallback, cancellation and the
  opaque ID-only drag payload remain unchanged

### Requirement: Route each shortcut to the requested capture

The edit shortcut SHALL preserve the identity of the requested entry from the
desktop listener through the rail and into `EntryTextEditorModal`. A later
shortcut for another eligible capture SHALL never reuse the first capture's
entry, title, draft or baseline.

#### Scenario: Switching from capture A to capture B

- **WHEN** the user opens editing for eligible capture A with the platform
  shortcut, closes or replaces that editor, selects eligible capture B and
  invokes the platform shortcut again
- **THEN** the modal is bound to capture B's `entry.id`
- **AND** its title, draft and baseline content come from B
- **AND** saving updates B only
- **AND** capture A's content and modal state are not reused

#### Scenario: A shortcut request carries an exact target identity

- **WHEN** the shared listener emits a shortcut request with `entryId`
- **THEN** the rail forwards it only to the mounted card with the same
  `data-entry-id`
- **AND** the receiving card validates the ID before opening its modal
- **AND** an absent, stale or mismatched ID is ignored
- **AND** the request contains no capture content or other sensitive payload

#### Scenario: Current selection wins over stale card focus

- **WHEN** the DOM focus remains on capture A after its editor was closed, but
  the rail's current selection is capture B
- **AND** the user invokes the platform edit shortcut
- **THEN** the shortcut request targets B
- **AND** the focused-card fallback for A is not used while a current rail
  selection exists

#### Scenario: Repeated target changes reset editor state

- **WHEN** the target changes from one entry to another while the editor flow
  is being reopened or replaced
- **THEN** `draft`, `baselineEntry`, accessible title IDs and return-focus
  target correspond to the new entry
- **AND** no text from the previous entry is submitted for the new target
- **AND** at most one text editor modal remains active

#### Scenario: Closing the editor restores focus to the current card

- **WHEN** the user closes, cancels, escapes from, or completes editing a
  textual capture
- **THEN** focus returns to the `<article>` of the card for that same capture
- **AND** the rail selection identifies that same capture
- **AND** the card uses the blue keyboard-navigation outline rather than a
  focus/error style from another card
- **AND** the card remains keyboard-focusable for the next interaction

#### Scenario: A stale focus does not create a second selected card

- **WHEN** the editor closes and focus returns to card A
- **AND** the user changes the rail selection to card B with `ArrowLeft` or
  `ArrowRight`
- **THEN** only card B is visually marked as the current selection
- **AND** card A loses the blue keyboard-navigation outline even if its DOM
  element still retains focus
- **AND** the WebView's native focus ring does not replace that outline with a
  second red or otherwise non-selection marker
