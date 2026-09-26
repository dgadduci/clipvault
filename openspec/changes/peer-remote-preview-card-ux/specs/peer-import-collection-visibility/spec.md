## ADDED Requirements

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
