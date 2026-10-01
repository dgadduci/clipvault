## ADDED Requirements

### Requirement: Configure whether capture notes are exported to peers

ClipVault SHALL expose an accessible checkbox in the existing General
Settings modal, reachable through the existing `Configuración general` menu
item, for whether this device may export notes attached to captures during
explicit peer imports. The preference SHALL be persisted locally through the
settings store, SHALL default to disabled when unset or invalid, and SHALL
control outgoing notes only. Changing this preference SHALL NOT change local
notes or whether the user can receive a note that its sender chose to share.

#### Scenario: Note export defaults to disabled

- **WHEN** no note-export preference has been saved
- **THEN** the General Settings checkbox is unchecked
- **AND** explicit capture imports from this device omit attached notes

#### Scenario: User enables or disables note export

- **WHEN** the user changes the checkbox in General Settings
- **THEN** ClipVault persists the new local preference
- **AND** it applies to subsequent explicit peer capture imports
- **AND** it does not rewrite, delete or otherwise modify any stored note

#### Scenario: Preference survives restart

- **GIVEN** the user has saved a note-export preference
- **WHEN** ClipVault restarts
- **THEN** General Settings displays the saved value and subsequent exports
  use that value

#### Scenario: Saving the preference fails

- **WHEN** ClipVault cannot persist the requested setting
- **THEN** the last successfully saved preference remains effective
- **AND** the user receives an actionable settings error

#### Scenario: Saving the preference omits unrelated settings fields

- **GIVEN** the user changes only the capture-note sharing checkbox
- **WHEN** General Settings submits that partial update
- **THEN** the settings command accepts the payload without ignored-app lists
- **AND** unrelated settings retain their persisted values
