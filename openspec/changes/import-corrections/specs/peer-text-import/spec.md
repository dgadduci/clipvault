## ADDED Requirements

### Requirement: Explicit text import requires a synchronized peer snapshot

Before the renderer sends an explicit text import request, it SHALL confirm
that the selected peer's current trusted and active snapshot has completed
synchronization with the text import service and remains trusted and active.
The core SHALL retain its existing eligibility gate as defense in depth.

#### Scenario: User activates import while synchronization is pending

- **WHEN** a text preview is visible but its peer snapshot has not completed
  synchronization
- **THEN** ClipVault does not invoke the text-import command
- **AND** it does not render `peer_unavailable` for that local transient state

#### Scenario: User activates import after synchronization

- **WHEN** the current trusted active peer snapshot is synchronized and the
  user activates Importar for a text row
- **THEN** ClipVault invokes the existing authenticated text import flow
- **AND** existing typed outcomes for actual peer or transport failures remain
  unchanged

#### Scenario: Host does not implement text fetch

- **WHEN** the authenticated remote host replies to the text-fetch request with
  `not_available`
- **THEN** the import outcome identifies an unavailable host capability
- **AND** no local entry is created

#### Scenario: Existing capture no longer has a transferable body

- **WHEN** the authenticated remote host replies with `not_found` or
  `not_transferable`
- **THEN** the import outcome is `NotTransferable`
- **AND** no local entry is created
