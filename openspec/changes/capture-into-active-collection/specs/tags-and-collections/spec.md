## ADDED Requirements

### Requirement: Assign automatic captures to the active local collection

ClipVault SHALL associate each newly captured clipboard entry with `Historial`
and, when a local user collection or subcollection is active at capture time,
with that active collection as well. The rule SHALL apply to new entries and
to existing entries returned by deduplication. Adding the active membership
SHALL preserve all existing collection memberships.

#### Scenario: Capture while a local collection is active

- **GIVEN** a local user collection is selected
- **WHEN** ClipVault stores a new clipboard capture
- **THEN** the entry belongs to both `Historial` and the selected collection

#### Scenario: Capture while a subcollection is active

- **GIVEN** a local subcollection is selected
- **WHEN** ClipVault stores a new clipboard capture
- **THEN** the entry belongs to both `Historial` and that subcollection
- **AND** ClipVault does not implicitly add the entry to the parent collection

#### Scenario: Capture duplicates an existing entry

- **GIVEN** a local user collection or subcollection is active
- **AND** the clipboard capture resolves to an existing history entry
- **WHEN** ClipVault completes the capture
- **THEN** the existing entry belongs to the active collection as well as
  `Historial`
- **AND** its other collection memberships remain unchanged

#### Scenario: Capture while Historial is active

- **GIVEN** `Historial` is active or no local collection is selected
- **WHEN** ClipVault stores a clipboard capture
- **THEN** the entry belongs to `Historial`
- **AND** ClipVault does not add it to a user collection

#### Scenario: Capture while a linked team history is active

- **GIVEN** the user is viewing a linked team's history
- **WHEN** ClipVault stores a local clipboard capture
- **THEN** the entry belongs to `Historial`
- **AND** ClipVault does not add it to the previously selected local collection

#### Scenario: Capture while a peer-bound collection is selected

- **GIVEN** the selected collection is bound to a linked peer
- **WHEN** ClipVault stores a local clipboard capture
- **THEN** the entry belongs to `Historial`
- **AND** ClipVault does not add it to the peer-bound collection

#### Scenario: Active local collection was removed or is no longer eligible

- **GIVEN** the active collection ID no longer identifies an eligible local
  user collection
- **WHEN** ClipVault stores a clipboard capture
- **THEN** the entry remains stored in `Historial`
- **AND** the invalid additional membership is ignored

#### Scenario: Restart with no active collection

- **WHEN** ClipVault starts a new session
- **THEN** the active capture destination is empty until the user selects a
  local collection
- **AND** clipboard captures continue to belong to `Historial`

### Requirement: Track the active capture collection with the visible scope

ClipVault SHALL keep the active capture destination synchronized with the
visible local collection scope. Selecting a local collection or subcollection
SHALL set that destination; selecting `Historial` or opening a linked team's
history SHALL clear it. Closing the linked team's history SHALL restore the
previous local selection as the destination. This runtime selection SHALL NOT
be persisted across application restarts.

#### Scenario: Switch between local collection scopes

- **WHEN** the user selects a different local collection or subcollection
- **THEN** subsequent captures use that newly selected collection

#### Scenario: Restore local scope after closing a linked history

- **GIVEN** a local collection is selected
- **WHEN** the user opens and then closes a linked team's history
- **THEN** subsequent captures use the previously selected local collection
