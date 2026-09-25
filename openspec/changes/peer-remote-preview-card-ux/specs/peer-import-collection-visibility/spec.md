## ADDED Requirements

### Requirement: Imported source attribution is scoped to its peer collection

For an entry displayed in a peer-bound collection, ClipVault SHALL use the
optional source-application display name and locally stored icon reference
from that peer's import provenance. It SHALL render the actual application
icon when valid, with a deterministic local import icon as fallback. It SHALL
not expose raw peer identifiers or resolve icon bytes from a remote path.
Missing or invalid names SHALL use an honest unknown-source label. A
provenance record for one peer SHALL NOT be used to label the same entry in
another peer's collection.

#### Scenario: Entry is displayed in its source peer's collection

- **WHEN** an imported entry with a valid source-application name is shown in
  the collection bound to the peer that supplied the import
- **THEN** the card shows that name and the validated local application icon
- **AND** it does not include a peer ID, application identifier, remote icon
  reference or path

#### Scenario: Same entry has provenance from multiple peers

- **WHEN** the same local entry is a member of peer-bound collections for two
  different peers, each with its own import provenance
- **THEN** each collection view shows only the source-application name and
  icon recorded for its own peer's provenance

#### Scenario: Imported source name is absent

- **WHEN** the matching provenance has no valid source-application display
  name
- **THEN** the card uses the static imported-origin icon when no valid local
  application icon exists and an unknown-source fallback when no valid name
  exists
- **AND** it does not borrow metadata from another peer or local capture
