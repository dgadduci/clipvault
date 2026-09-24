## Purpose

Garantizar que cada entrada importada desde un par vinculado permanezca visible en la colección local vinculada al `peer_id` de origen sin filtrar identificadores, certificados ni datos sensibles en la proyección de colecciones.

## Requirements

### Requirement: Imported entries remain discoverable by remote origin

ClipVault SHALL keep every imported entry in the protected `Historial`
collection and SHALL also attach it to the collection bound to its source
`peer_id`. The bound collection SHALL be visible in the local collection
projection after the import succeeds.

#### Scenario: First import from a peer

- **WHEN** a user successfully imports a transferable entry from peer X
- **THEN** the entry remains in `Historial`
- **AND** the entry is visible in one collection bound to X
- **AND** the collection appears in the sidebar after the organization refresh

#### Scenario: Import failure

- **WHEN** fetch, validation or persistence fails before commit
- **THEN** no peer collection, membership or provenance is created as a
  partial side effect

### Requirement: Remote origin is immutable while the display name is editable

The origin of a peer collection SHALL be determined only by its persisted
binding to `peer_id`. The user MAY edit the collection display name, but a
rename SHALL NOT change or remove that binding. The UI SHALL show an accessible
origin marker derived from the current visible peer name without exposing the
raw `peer_id`.

#### Scenario: Rename a peer collection

- **WHEN** the user renames the collection bound to peer X
- **THEN** the new display name is persisted
- **AND** the collection remains bound to X
- **AND** the sidebar continues to show that it is imported from X

#### Scenario: Remote peer changes its visible name

- **WHEN** peer X changes its visible name
- **THEN** the bound collection keeps the user's chosen collection name
- **AND** the origin marker may show the new peer name
- **AND** no second collection is created

### Requirement: Peer bindings are independent and stable across lifecycle changes

Each peer SHALL have at most one active collection binding. Bindings SHALL be
resolved by `peer_id`, not by case-insensitive collection name. Restarting the
application SHALL reconstruct the same binding, and deleting the collection
SHALL preserve imported entries and provenance while allowing a later import
to create a new binding.

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

- **WHEN** the user deletes X's peer collection
- **THEN** imported entries, `Historial` membership and provenance remain
- **AND** a later import from X creates a new binding instead of selecting a
  collection only by its old name

### Requirement: Collection projection and refresh remain metadata-only

The collection projection MAY include `is_peer_bound` and a safe visible peer
name, but SHALL NOT include peer identifiers, certificates, endpoints,
fingerprints, content, hashes, assets, paths or secrets. The organization
refresh SHALL not alter clipboard state, paste state, drag payloads or local
entry metadata.

#### Scenario: Organization refresh after import

- **WHEN** the import transaction commits and the organization event is
  delivered
- **THEN** the sidebar refreshes the collection projection
- **AND** no clipboard, paste, drag or entry-content mutation occurs

#### Scenario: Unknown peer display name

- **WHEN** a persisted binding has no current visible peer name
- **THEN** the UI displays a generic safe remote-origin label
- **AND** it does not display the raw peer identifier or network data