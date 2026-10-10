## ADDED Requirements

### Requirement: Remote import waits for synchronized peer state

ClipVault SHALL keep a remote preview visible while its selected peer state is
being synchronized, but SHALL NOT enable a text or image import until that same
peer's trusted and active state has been registered with the corresponding
import service and is both trusted and active. A peer change, block, unlink,
loss of presence or stale synchronization response MUST revoke readiness before
it can enable an import.

#### Scenario: Preview appears before import state is ready

- **WHEN** a remote preview row is rendered while peer-state synchronization is
  still pending
- **THEN** the preview remains readable and the import action is unavailable
- **AND** activating it sends no fetch request and creates no local entry

#### Scenario: Current peer state finishes synchronization

- **WHEN** synchronization for the currently selected trusted active peer
  completes
- **THEN** its remote rows enable the matching explicit import action

#### Scenario: Stale synchronization completes after peer change

- **WHEN** the user selects, blocks or unlinks another peer before an earlier
  synchronization finishes
- **THEN** the earlier completion does not enable import for the current rows

#### Scenario: Visible preview belongs to the current eligible peer

- **WHEN** a remote preview card is visible
- **THEN** the selected peer remains trusted and present according to the rail
- **AND** a later fetch failure is not attributed to a cached inactive peer
