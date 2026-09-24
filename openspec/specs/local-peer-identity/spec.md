# local-peer-identity Specification

## Purpose
TBD - created by archiving change local-peer-identity-foundation. Update Purpose after archive.

## Requirements

### Requirement: Local peer identity is stable and stored securely

ClipVault SHALL create or load one stable cryptographic local peer identity
through a platform secure credential store. The public peer_id and fingerprint
SHALL be deterministically derived from its public key. Private material SHALL
NOT be persisted in SQLite, ordinary files, frontend state, logs, fixtures,
events or error messages.

#### Scenario: Identity survives restart

- **WHEN** ClipVault loads the local peer profile after a prior successful
  provisioning
- **THEN** it returns the same peer_id and public fingerprint without creating
  a second identity

#### Scenario: Secure identity store is unavailable

- **WHEN** the platform secure credential store cannot load or create identity
  material
- **THEN** ClipVault returns a typed safe failure and does not write a plaintext
  fallback or start any network capability

### Requirement: User can manage a validated visible device name

ClipVault SHALL persist a local visible device name independently from the
cryptographic identity. The name SHALL be trimmed, non-empty, free of control
characters and at most 64 Unicode characters. Changing it SHALL NOT alter
peer_id or fingerprint.

#### Scenario: User changes visible name

- **WHEN** the user saves a valid new device name
- **THEN** Settings returns the updated local profile while its peer_id and
  fingerprint remain unchanged

#### Scenario: Invalid device name

- **WHEN** the submitted name is empty, control-only or exceeds the limit
- **THEN** no setting is changed and the frontend receives a stable validation
  outcome without echoing the rejected value

### Requirement: Identity foundation performs no LAN operation

This change SHALL provide only local identity/profile functionality. It SHALL
NOT advertise, browse, bind a socket, request local-network permission or
connect to another device.

#### Scenario: Identity settings are used

- **WHEN** a user views or changes the local identity profile
- **THEN** no network adapter is constructed and no network traffic occurs
