# local-peer-identity Specification

## Purpose
Define how ClipVault provisions and securely stores a stable local peer
identity and manages the visible device name independently of that identity.

## Requirements

### Requirement: Local peer identity is stable and stored securely

ClipVault SHALL create or load one stable cryptographic local peer identity
through a platform secure credential store. The public peer_id and fingerprint
SHALL be deterministically derived from its public key. Private material SHALL
NOT be persisted in SQLite, ordinary files, frontend state, logs, fixtures,
events or error messages. The platform adapter SHALL persist new 32-byte
Ed25519 seeds using the versioned printable-ASCII representation
`clipvault-peer-seed-v1:` followed by exactly 64 lowercase hexadecimal
characters. It SHALL continue to accept and migrate a legacy raw 32-byte seed
without changing the resulting identity. An empty value SHALL be treated as
an unusable identity and replaced with a newly generated seed. Other malformed
non-empty values SHALL fail safely without being overwritten.

#### Scenario: Identity survives restart

- **WHEN** ClipVault loads the local peer profile after a prior successful
  provisioning
- **THEN** it returns the same peer_id and public fingerprint without creating
  a second identity

#### Scenario: Legacy raw seed is migrated

- **WHEN** the secure store contains exactly the 32 raw bytes used by an older
  version
- **THEN** ClipVault derives the same identity and replaces the stored value
  with the versioned ASCII representation

#### Scenario: Empty Secret Service item is recovered

- **WHEN** the secure store returns an existing item with an empty value
- **THEN** ClipVault stores a new versioned seed in the secure store and
  returns the identity derived from that seed

#### Scenario: Unknown non-empty seed encoding is rejected

- **WHEN** the secure store returns a non-empty value that is neither a
  versioned valid seed nor a 32-byte legacy seed
- **THEN** ClipVault returns a typed safe failure and leaves the value intact

#### Scenario: Secure identity store is unavailable

- **WHEN** the platform secure credential store cannot load or create identity
  material
- **THEN** ClipVault returns a typed safe failure and does not write a plaintext
  fallback or start any network capability

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

### Requirement: Identity foundation performs no LAN operation

This change SHALL provide only local identity/profile functionality. It SHALL
NOT advertise, browse, bind a socket, request local-network permission or
connect to another device.

#### Scenario: Identity settings are used

- **WHEN** a user views or changes the local identity profile
- **THEN** no network adapter is constructed and no network traffic occurs
