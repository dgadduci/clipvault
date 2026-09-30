# Spec Delta

## MODIFIED Requirements

### Requirement: Imported entries remain discoverable by remote origin

ClipVault SHALL keep every successfully imported entry in the protected
`Historial` collection and SHALL also attach it to the collection bound to its
source `peer_id`. This invariant SHALL hold for text and image imports, both
new and deduplicated local entries, and SHALL be committed atomically with the
import provenance. The bound collection SHALL be visible in the local
collection projection after the import succeeds.

#### Scenario: First import from a peer

- **WHEN** a user successfully imports a transferable entry from peer X
- **THEN** the entry belongs to `Historial`
- **AND** the entry is visible in one collection bound to X
- **AND** the collection appears in the sidebar after the organization refresh

#### Scenario: Deduplicated import repairs History membership

- **GIVEN** an imported payload resolves to an existing local entry that is
  missing its `Historial` membership
- **WHEN** the import transaction commits
- **THEN** the existing entry belongs to both `Historial` and the collection
  bound to the importing peer
- **AND** its existing content, title, favorite state and other memberships
  remain unchanged

#### Scenario: Import failure

- **WHEN** fetch, validation or persistence fails before commit
- **THEN** no peer collection, membership or provenance is created as a
  partial side effect

### Requirement: Peer bindings are independent and stable across lifecycle changes

Each peer SHALL have at most one active collection binding. Bindings SHALL be
resolved by `peer_id`, not by case-insensitive collection name. Restarting the
application SHALL reconstruct the same binding. Deleting a peer-bound
collection SHALL require an explicit choice: preserve its imported entries,
`Historial` memberships and provenance, or delete its current entries from all
local history and their dependent provenance. Either choice SHALL remove the
old binding so a later import creates a new binding. Deleting entries SHALL
NOT delete the peer record, alter pairing trust or affect another peer's
unrelated entries.

#### Scenario: Multiple peers with equal visible names

- **WHEN** peers X and Y have the same visible name and both are imported
- **THEN** X and Y receive independent collections
- **AND** a collision-safe initial display name is chosen
- **AND** importing from X never attaches an entry to Y's collection

#### Scenario: Restart after import

- **WHEN** ClipVault restarts after importing from peer X
- **THEN** the same peer-bound collection and origin marker appear without a
  duplicate collection

#### Scenario: Collection is deleted

- **WHEN** the user deletes X's peer collection and chooses to preserve its
  entries
- **THEN** the collection and binding are removed
- **AND** imported entries, `Historial` membership, provenance and other
  collection memberships remain
- **AND** a later import from X creates a new binding instead of selecting a
  collection only by its old name

#### Scenario: Delete a peer collection and its entries

- **WHEN** the user deletes X's peer collection and explicitly chooses to
  delete its entries from history
- **THEN** every current entry in that collection is removed from local history
  and all collection memberships, including favorites
- **AND** its local import provenance is removed with each entry
- **AND** X remains a known peer with its existing trust state
- **AND** a later import from X creates a new collection binding
