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

### Requirement: Imported source attribution is scoped to its peer collection

For an entry displayed in a peer-bound collection, ClipVault SHALL use the
optional source-application display name and locally stored icon reference
from that peer's import provenance. In general history, it SHALL use the
most-recent import provenance for the entry with a stable tie-breaker, without
exposing the peer identifier. In both views the card SHALL render the
application icon when valid, with a deterministic local import icon as
fallback, and expose the name only through the icon's accessible label and
tooltip, not as visible text. ClipVault SHALL NOT resolve icon bytes from a
remote path. Missing or invalid names SHALL use an honest unknown-source
label. A provenance record for one peer SHALL NOT be used in another peer's
bound collection.

#### Scenario: Entry is displayed in its source peer's collection

- **WHEN** an imported entry with a valid source-application name is shown in
  the collection bound to the peer that supplied the import
- **THEN** the card shows the validated local application icon and exposes
  that name through its accessible label and tooltip
- **AND** the source name is not rendered as visible card text
- **AND** it does not include a peer ID, application identifier, remote icon
  reference or path

#### Scenario: Same entry has provenance from multiple peers

- **WHEN** the same local entry is a member of peer-bound collections for two
  different peers, each with its own import provenance
- **THEN** each collection view shows only the source-application name and
  icon recorded for its own peer's provenance

#### Scenario: General history shows the latest imported source

- **WHEN** an imported entry with provenance from one or more peers is shown
  in general history
- **THEN** the card uses the provenance with the greatest `imported_at`, with
  a stable tie-breaker when timestamps match
- **AND** it shows the local icon or static import fallback and exposes the
  source name only as the icon's accessible label and tooltip
- **AND** it does not expose a peer ID

#### Scenario: Imported source name is absent

- **WHEN** the matching provenance has no valid source-application display
  name
- **THEN** the card uses the static imported-origin icon when no valid local
  application icon exists and an unknown-source fallback when no valid name
  exists
- **AND** it does not borrow metadata from another peer or local capture