## MODIFIED Requirements

### Requirement: Edit and persist collection colors

Each collection SHALL have one opaque RGB color stored as normalized
`#rrggbb`. The user SHALL be able to edit the color without changing the
collection identity or any entry association. The color editor SHALL be opened
from the collection actions menu, and the sidebar row background and menu
swatch SHALL reflect the persisted color.

#### Scenario: Open the color editor from the actions menu

- **WHEN** the user selects the change-color action in a collection's menu
- **THEN** ClipVault opens a modal for that collection with its current color
  selected
- **AND** the modal offers an accessible visual color picker, Guardar and
  Cancelar

#### Scenario: Save a user-selected color

- **WHEN** the user selects a valid opaque RGB color and confirms Guardar
- **THEN** ClipVault persists the normalized HEX color for that collection
- **AND** the sidebar row background, menu swatch and every visible card label
  for that collection refresh after the command succeeds
- **AND** no entry, tag, membership, asset or clipboard payload changes

#### Scenario: Cancel or close the color editor

- **WHEN** the user presses Escape, clicks Cancelar, or closes the backdrop
- **THEN** the previous collection color remains persisted
- **AND** no organization update event is emitted

#### Scenario: Invalid color is rejected

- **WHEN** a color update contains an invalid, transparent or non-HEX value
- **THEN** the backend returns a typed validation error
- **AND** the previous color and all collection memberships remain unchanged

#### Scenario: Historial keeps its protected identity

- **WHEN** the user changes the color of `Historial`
- **THEN** only its color changes
- **AND** its stable key, system kind, name, memberships and protection against
  rename/delete remain unchanged

### Requirement: Persist and edit one local note per collection

ClipVault SHALL allow one optional plain-text note per collection, including
Historial, user collections and collections bound to peer imports. A note
SHALL support line breaks and SHALL be persisted locally, independently from
the collection name, color, kind and memberships. The collection actions menu
SHALL NOT contain the note action. Each collection row SHALL show an accessible
note icon between its capture-count badge and actions-menu button. The icon
SHALL open the shared note editor and identify whether it will add or edit a
note. Collection notes SHALL NOT be included in peer-to-peer capture transfers,
logs, diagnostics or collection list bodies. Deleting a collection SHALL
remove its note with the collection, even when its entries are preserved.

#### Scenario: Add a collection note

- **WHEN** the user creates and saves a multiline note for a collection
- **THEN** ClipVault persists the note locally
- **AND** the collection row shows its note icon as having a saved note

#### Scenario: Open and edit a collection note

- **GIVEN** a collection has a saved note
- **WHEN** the user activates its note icon in the collection row
- **THEN** the shared note modal opens with the persisted note and allows the
  user to edit it
- **AND** focus returns to that note icon when the modal closes

#### Scenario: Create a note for a collection without one

- **GIVEN** a collection has no saved note
- **WHEN** the user activates its note icon in the collection row
- **THEN** the shared note modal opens with an empty multiline editor
- **AND** the row's note icon indicates the saved note after a successful save

#### Scenario: Remove a collection note

- **WHEN** the user saves an empty collection note
- **THEN** ClipVault removes the note association
- **AND** the row's note icon indicates that no note is saved

#### Scenario: Cancel collection note editing

- **WHEN** the user cancels, presses Escape or closes the backdrop before
  saving
- **THEN** the persisted collection note remains unchanged

#### Scenario: Collection deletion removes its note

- **WHEN** the user deletes a collection while preserving its entries
- **THEN** ClipVault deletes the collection note with the collection
- **AND** the preserved entries, their memberships in other collections and
  their capture notes remain unchanged

#### Scenario: Collection note stays local

- **WHEN** a collection is bound to peer imports and has a local note
- **THEN** the note remains local and is not included in any capture transfer
- **AND** the collection's import binding and entries remain unchanged

## ADDED Requirements

### Requirement: Present collection rows with their assigned color and a compact actions menu

The desktop collection list SHALL render each row with a translucent background
derived from that collection's validated assigned color. The row SHALL show the
collection name, its applicable `system` or `imported` badge, its capture-count
badge, a note icon and one actions-menu button, in that order. The menu SHALL
contain the existing color action and applicable delete action without
changing their persistence or confirmation flows.

#### Scenario: Render a colored collection row

- **WHEN** the desktop collection list renders a collection with an assigned
  color
- **THEN** its row background uses that color at 18% opacity
- **AND** its name and capture count remain visible
- **AND** the selected and drag-over states remain distinguishable

#### Scenario: Show system and imported labels in the row

- **WHEN** the sidebar renders `Historial`
- **THEN** its row shows the `system` badge beside its name and capture count
- **AND** its actions menu does not offer deletion
- **WHEN** the sidebar renders a collection bound to a remote peer
- **THEN** its row shows the `imported` badge beside its name and capture count
- **AND** the row does not expose the raw peer identifier

#### Scenario: Open collection actions

- **WHEN** the user opens a collection's actions menu
- **THEN** the menu offers the existing change-color action
- **AND** it offers delete only for user collections
- **AND** each action uses the existing modal, event and confirmation flow
- **AND** the note action remains available through the row's note icon

#### Scenario: Navigate and dismiss the actions menu

- **WHEN** the user opens the actions menu by pointer or keyboard
- **THEN** its trigger exposes an accessible localized name and expanded state
- **AND** keyboard focus enters the menu and can move between its actions
- **WHEN** the user presses Escape
- **THEN** the menu closes and focus returns to its trigger
- **WHEN** the user clicks outside the menu
- **THEN** the menu closes and focus remains on the clicked control
- **WHEN** keyboard focus moves outside the menu
- **THEN** the menu closes without taking focus from the newly focused control

#### Scenario: Preserve collection navigation and drop behavior

- **WHEN** the user selects a collection or drops a history card on a visible
  user-collection row, including a row in the scrolled list
- **THEN** selection and drop continue to use the existing row and delegated
  drop-zone behavior
- **AND** opening or using the actions menu does not change drag payloads,
  collection membership rules, or clipboard data
