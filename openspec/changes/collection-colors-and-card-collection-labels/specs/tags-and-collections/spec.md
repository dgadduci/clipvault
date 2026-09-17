# Delta: colores persistentes de colecciones

## MODIFIED Requirements

### Requirement: Manage flat collections

ClipVault SHALL allow users to create, rename and delete flat user
collections, and every collection SHALL expose a validated persistent color.
Deleting a user collection SHALL remove only its memberships and SHALL NOT
delete entries, tags, `Historial` or memberships in other collections.

#### Scenario: Create collection

- **WHEN** the user submits a valid new collection name
- **THEN** the collection is persisted locally with exactly one color from the
  configured red, yellow, green or blue palette
- **AND** the color is selected by the backend rather than trusted from the
  frontend
- **AND** the collection appears in the sidebar

#### Scenario: Rename collection

- **WHEN** the user submits a valid new name for a user collection
- **THEN** only that collection's display name changes and its memberships
  remain intact

#### Scenario: Delete collection

- **WHEN** the user confirms deletion of a user collection
- **THEN** the collection and its association rows are removed while every
  associated entry remains in `Historial` and other collections

#### Scenario: Duplicate collection name

- **WHEN** the user creates or renames a collection to a name already used
  case-insensitively
- **THEN** ClipVault rejects it without modifying existing collections

#### Scenario: Existing collection receives a migration color

- **WHEN** ClipVault opens a database whose collections predate color support
- **THEN** every existing collection receives a valid default color without
  changing its name, timestamps, memberships or kind

## ADDED Requirements

### Requirement: Edit and persist collection colors

Each collection SHALL have one opaque RGB color stored as normalized
`#rrggbb`. The user SHALL be able to edit the color without changing the
collection identity or any entry association.

#### Scenario: Open the color editor from the sidebar

- **WHEN** the user double-clicks a collection color square
- **THEN** ClipVault opens a modal for that collection with its current color
  selected
- **AND** the modal offers an accessible visual color picker, Guardar and
  Cancelar

#### Scenario: Save a user-selected color

- **WHEN** the user selects a valid opaque RGB color and confirms Guardar
- **THEN** ClipVault persists the normalized HEX color for that collection
- **AND** the sidebar square and every visible card label for that collection
  refresh after the command succeeds
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
