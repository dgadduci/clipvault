## ADDED Requirements

### Requirement: Clear a selected user collection from the desktop toolbar

When a user collection is selected, the desktop toolbar SHALL provide an
accessible action to empty that collection. The action SHALL use a
metadata-only preview for the complete collection, independent of search,
tag or other visible filters, and ask whether its captures should also be
deleted from local history. The dialog SHALL offer preserving the captures in
history, deleting them from all local history, and canceling. The existing
Historial toolbar action SHALL continue to clear only eligible unorganized
captures and SHALL retain its current confirmation behavior.

#### Scenario: Empty a user collection and preserve its captures

- **WHEN** the user selects a local user collection, opens its toolbar
  clear action and chooses to keep its captures in Historial
- **THEN** only the selected collection memberships are removed
- **AND** every capture remains in Historial and in its other collections
- **AND** local peer-import provenance, when present, remains available

#### Scenario: Empty a user collection and delete its captures from history

- **WHEN** the user selects the option to also delete the collection's
  captures from local history
- **THEN** all captures currently in that collection are deleted from
  history and their other memberships, including favorites and local import
  provenance
- **AND** the selected collection definition remains present
- **AND** its peer binding and trust state remain unchanged when it is a
  peer-bound collection
- **AND** assets still referenced by another capture are retained

#### Scenario: Preview uses the complete collection scope

- **GIVEN** the selected collection is narrowed by a search query, tag or
  another visible filter
- **WHEN** the user opens the clear action
- **THEN** the confirmation preview describes every capture in the collection
  and does not expose capture content, IDs, hashes, provenance or asset paths

#### Scenario: Collection contents change before confirmation

- **GIVEN** the user has opened a clear confirmation with metadata counts
- **AND** the collection's entry or favorite count changes before commit
- **WHEN** the user confirms
- **THEN** ClipVault leaves the collection and its entries unchanged
- **AND** presents the updated metadata preview and requires confirmation
  again

#### Scenario: Cancel clearing a collection

- **WHEN** the user cancels the clear confirmation
- **THEN** collection memberships, entries, favorites, provenance and assets
  remain unchanged

#### Scenario: Clear an empty collection

- **WHEN** the selected user collection contains no captures
- **THEN** the toolbar does not submit a destructive operation
- **AND** the collection remains available for future assignments or imports

#### Scenario: Clear action in Historial

- **WHEN** the user opens the toolbar clear action in Historial
- **THEN** it continues to remove only captures eligible under the existing
  unorganized-history rule
- **AND** it does not clear secondary user or peer-bound collections
