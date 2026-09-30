# Spec Delta

## ADDED Requirements

### Requirement: Clear unorganized history includes preserved peer imports

ClipVault SHALL apply the unorganized-history predicate to every history entry,
including entries created by peer imports. A non-favorite entry that belongs to
`Historial` and no user collection SHALL be counted and removed by the existing
clear-unorganized-history operation even when local import provenance remains.
Favorites and entries with any user-collection membership SHALL remain.

#### Scenario: Clear imported entries after deleting their peer collection

- **GIVEN** an imported entry belongs to `Historial` and a peer-bound user
  collection
- **AND** the user deletes that collection while choosing to preserve its
  captures in `Historial`
- **WHEN** the user confirms `Eliminar capturas no organizadas`
- **THEN** the non-favorite imported entry is included in the clearable count
  and is removed from local history
- **AND** its local import provenance is removed with the entry
- **AND** the remote peer, trust relationship and unrelated imported entries
  remain unchanged

#### Scenario: Clear imported entries that retain provenance

- **GIVEN** a non-favorite imported entry belongs only to `Historial` while a
  local provenance record still references it
- **WHEN** the clear-unorganized-history operation is confirmed
- **THEN** the entry and its dependent provenance are removed atomically
- **AND** an asset is collected only if no remaining history row references it

#### Scenario: Preserve favorite imported entries

- **GIVEN** a favorite imported entry belongs only to `Historial`
- **WHEN** the clear-unorganized-history operation is confirmed
- **THEN** the favorite entry, its provenance and referenced assets remain

### Requirement: Delete an entry from a user collection with explicit scope

When the user activates the card's `Eliminar` action inside a user collection,
ClipVault SHALL ask whether to remove the entry only from that collection or to
delete it from all of local history. The collection-only choice SHALL preserve
the entry's `Historial` membership and every other collection membership. The
global choice SHALL remove the entry and all of its associations and local
import provenance. Cancelling SHALL make no change. The `Eliminar` action in
`Historial` SHALL retain its existing confirmed global-delete behavior.

#### Scenario: Keep an entry in history from a user collection

- **GIVEN** an entry is shown in a user collection and belongs to `Historial`
- **WHEN** the user chooses to remove it only from the active collection
- **THEN** the active collection membership is removed
- **AND** the entry remains in `Historial` and any other user collections
- **AND** its local import provenance and referenced assets remain intact

#### Scenario: Delete an entry globally from a user collection

- **GIVEN** an entry is shown in a user collection
- **WHEN** the user explicitly chooses to delete it from local history
- **THEN** the entry is removed from `Historial` and every user collection
- **AND** its association and local import-provenance rows are removed
- **AND** any image asset is collected only when no remaining entry references
  it

#### Scenario: Cancel scoped entry deletion

- **GIVEN** the scope-choice prompt is open for an entry in a user collection
- **WHEN** the user cancels
- **THEN** the entry, all memberships, provenance and assets remain unchanged
