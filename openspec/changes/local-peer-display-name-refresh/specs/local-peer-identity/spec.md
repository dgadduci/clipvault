## MODIFIED Requirements

### Requirement: User can manage a validated visible device name

ClipVault SHALL persist a local visible device name independently from the
cryptographic identity. The name SHALL be trimmed, non-empty, free of control
characters and at most 64 Unicode characters. Changing it SHALL NOT alter
peer_id or fingerprint. When local peer sharing is active, saving a valid name
SHALL refresh the current DNS-SD/mDNS advertisement and the local name used by
new pairing exchanges without restarting the runtime. When sharing is
disabled, the saved value SHALL be used the next time sharing starts.

#### Scenario: User changes visible name

- **WHEN** the user saves a valid new device name
- **THEN** Settings returns the updated local profile while its peer_id and
  fingerprint remain unchanged

#### Scenario: Active sharing publishes a renamed device

- **GIVEN** local peer sharing is active
- **WHEN** the user saves a different valid visible device name
- **THEN** the active discovery advertisement is updated with the new name
- **AND** its peer_id, public-key fingerprint, capabilities, pairing trust and
  runtime availability remain unchanged
- **AND** discovery browsing and the pairing listener remain active without
  requiring an application restart

#### Scenario: New pairing exchange uses the current name

- **GIVEN** the user changed the visible device name while sharing is active
- **WHEN** a later pairing exchange begins
- **THEN** the exchange uses the current validated name
- **AND** the stable peer_id and fingerprint remain unchanged

#### Scenario: Sharing starts after a name change

- **GIVEN** the user saves a valid name while local peer sharing is disabled
- **WHEN** the user later enables sharing
- **THEN** the first published record uses the saved name and existing identity

#### Scenario: Invalid device name

- **WHEN** the submitted name is empty, control-only or exceeds the limit
- **THEN** no setting or active advertisement is changed and the frontend
  receives a stable validation outcome without echoing the rejected value
