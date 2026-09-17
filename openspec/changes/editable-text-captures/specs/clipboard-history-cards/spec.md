# Delta: edición de texto desde las cards

## ADDED Requirements

### Requirement: Open the text editor from an eligible card

Each eligible textual history card SHALL expose an accessible `Editar captura`
action through its existing card menu. Image and rich-text cards SHALL NOT
expose this action. Opening the editor SHALL not select the card or start a
drag operation.

#### Scenario: Open editor from the card menu

- **WHEN** the user opens the menu of an eligible text card and chooses
  `Editar captura`
- **THEN** a modal opens with the current content loaded into a native
  `textarea`
- **AND** focus moves to the editor
- **AND** the card keeps its fixed geometry and existing actions

#### Scenario: Edit modal is accessible and cancellable

- **WHEN** the text editor modal is open
- **THEN** it has an accessible title, labelled editor, Guardar and Cancelar
  controls, visible validation errors and a single submit path
- **AND** Escape, backdrop, close button or Cancelar closes it without saving
- **AND** focus returns to the menu trigger

#### Scenario: Successful card edit refreshes the card

- **WHEN** the user saves valid text
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
title or the existing explanatory summary below the editor. An eligible text
card SHALL support `Ctrl+E` as a card-level shortcut and SHALL show `Ctrl+E`
next to the `Editar captura` menu action.

#### Scenario: Editor title is the capture title

- **WHEN** the user opens the text editor for a capture with resolved title
  `Mi captura`
- **THEN** the dialog heading is `Mi captura`
- **AND** the editor remains associated with that heading through
  `aria-labelledby`
- **AND** the removed explanatory summary is not rendered

#### Scenario: Ctrl+E opens editing for an eligible card

- **WHEN** an eligible text card has keyboard focus and the user presses
  `Ctrl+E`
- **THEN** the editor opens with the current capture content
- **AND** the card/menu interaction is not treated as a drag
- **AND** the `Editar captura` menu action visibly presents `Ctrl+E` and
  exposes `aria-keyshortcuts="Control+E"`

#### Scenario: Ctrl+E is ignored in ineligible or interactive contexts

- **WHEN** the focused card is an image/rich-text card, or the event target is
  an input, textarea, select, button, menu, contenteditable element or the
  editor modal
- **THEN** `Ctrl+E` does not open the text editor and does not interfere with
  the focused control's normal behavior
