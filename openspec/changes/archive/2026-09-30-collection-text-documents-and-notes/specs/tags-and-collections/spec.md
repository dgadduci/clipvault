## ADDED Requirements

### Requirement: Persist and edit one local note per collection

ClipVault SHALL allow one optional plain-text note per collection, including
Historial, user collections and collections bound to peer imports. A note
SHALL support line breaks and SHALL be persisted locally, independently from
the collection name, color, kind and memberships. The collection list SHALL
identify collections with a saved note using an accessible note icon whose
activation opens the shared note editor. A collection without a note SHALL
provide an accessible action to create one. Collection notes SHALL NOT be
included in peer-to-peer capture transfers, logs, diagnostics or collection
list bodies. Deleting a collection SHALL remove its note with the collection,
even when its entries are preserved.

#### Scenario: Add a collection note

- **WHEN** the user creates and saves a multiline note for a collection
- **THEN** ClipVault persists the note locally
- **AND** the collection row identifies the saved note with an accessible
  note icon

#### Scenario: Open and edit a collection note

- **GIVEN** a collection has a saved note
- **WHEN** the user activates its note icon in the collection list
- **THEN** the shared note modal opens with the persisted note and allows the
  user to edit it

#### Scenario: Create a note for a collection without one

- **GIVEN** a collection has no saved note
- **WHEN** the user activates its add-note action
- **THEN** the shared note modal opens with an empty multiline editor
- **AND** the collection list shows the note icon after a successful save

#### Scenario: Remove a collection note

- **WHEN** the user saves an empty collection note
- **THEN** ClipVault removes the note association and the collection row no
  longer identifies a saved note

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
