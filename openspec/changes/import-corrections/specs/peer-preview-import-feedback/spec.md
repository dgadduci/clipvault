## ADDED Requirements

### Requirement: Remote cards communicate import preparation without an error

Remote preview cards SHALL use localized product text and accessible state to
communicate that import is temporarily unavailable while the peer snapshot is
synchronizing. This transient state SHALL be distinct from an actual peer or
transport failure and SHALL be available in every supported UI language.

#### Scenario: Import action is preparing

- **WHEN** a remote card is visible before its peer import state is ready
- **THEN** its Importar control is disabled with localized preparation text
- **AND** no generic “not available” error is shown

#### Scenario: Import readiness completes

- **WHEN** the current peer state is synchronized
- **THEN** the preparation text is removed and the normal import control is
  available

#### Scenario: Host needs an update to import

- **WHEN** an authenticated current peer returns the stable fetch rejection
  `not_available`
- **THEN** the card communicates with localized text that the remote ClipVault
  instance must be updated
- **AND** it does not expose the wire reason or the remote content

#### Scenario: Existing capture is no longer transferable

- **WHEN** an authenticated current peer returns `not_found` or
  `not_transferable` for a previewed text capture
- **THEN** the card shows the existing localized non-transferable feedback
- **AND** it does not present the rejection as a generic transport failure
