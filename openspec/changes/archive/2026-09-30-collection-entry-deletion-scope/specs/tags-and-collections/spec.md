# Spec Delta

## MODIFIED Requirements

### Requirement: Manage flat collections

ClipVault SHALL allow users to create, rename and delete flat user
collections, and every collection SHALL expose a validated persistent color.
Before deleting a user collection, ClipVault SHALL show a metadata-only count
of its entries and favorites and ask whether to preserve those entries in
`Historial` or delete them from all local history. Preserving entries SHALL
remove only the collection and its memberships; entries, `Historial` and other
collection memberships SHALL remain. Choosing to delete entries SHALL remove
all entries currently in that collection from local history and every other
collection, including favorites, in the same transaction as collection
deletion. Tag definitions SHALL remain. The protected system `Historial`
collection SHALL NOT be deleted.

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

#### Scenario: Preview collection deletion

- **WHEN** the user opens the delete prompt for a user collection
- **THEN** the prompt displays the number of entries and favorites currently
  associated with that collection without exposing entry content or identifiers
- **AND** it offers separate choices to preserve entries in `Historial` or to
  delete them from all local history

#### Scenario: Delete collection

- **WHEN** the user confirms collection deletion and chooses to preserve entries
- **THEN** the collection and its association rows are removed
- **AND** every associated entry remains in `Historial` and other collections
- **AND** import provenance remains available for preserved imported entries

#### Scenario: Delete collection and its entries

- **WHEN** the user confirms collection deletion and chooses to delete its entries
- **THEN** all currently associated entries, including favorites, are removed
  from history and other collections atomically with the collection
- **AND** tag and unrelated collection definitions remain
- **AND** referenced assets are retained if any remaining entry still uses them

#### Scenario: Collection contents changed after preview

- **GIVEN** the user previewed deletion counts for a collection
- **AND** the total or favorite-entry count changes before confirmation
- **WHEN** the user confirms deletion of the collection's entries
- **THEN** ClipVault leaves the collection and all entries unchanged
- **AND** the prompt receives the updated counts and requires confirmation
  again before deleting entries

#### Scenario: Cancel collection deletion

- **WHEN** the user cancels the delete prompt
- **THEN** the collection, entries, memberships, provenance and assets remain
  unchanged

#### Scenario: Protected system collection

- **WHEN** a user attempts to rename or delete `Historial`
- **THEN** ClipVault rejects the operation with a typed validation result and
  leaves the system collection unchanged

#### Scenario: Duplicate collection name

- **WHEN** the user creates or renames a collection to a name already used
  case-insensitively
- **THEN** ClipVault rejects it without modifying existing collections

#### Scenario: Existing collection receives a migration color

- **WHEN** ClipVault opens a database whose collections predate color support
- **THEN** every existing collection receives a valid default color without
  changing its name, timestamps, memberships or kind
