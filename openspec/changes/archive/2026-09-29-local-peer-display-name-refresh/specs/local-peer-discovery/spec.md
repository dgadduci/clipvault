## MODIFIED Requirements

### Requirement: Discovery persists N independent peers without endpoint identity

ClipVault SHALL remember N discovered peer identities from the current and
previous sessions. It SHALL persist only validated non-content discovery
metadata and SHALL treat IP addresses and ports as transient runtime data,
never as identity or persisted routes. A valid observation for an existing
peer SHALL update its visible name when the peer_id, public-key fingerprint,
protocol major and capability transition remain compatible. A name change
alone SHALL NOT be treated as an identity conflict.

#### Scenario: Multiple peers are independently observed

- **WHEN** several local peers are resolved, removed and resolved again
- **THEN** each peer retains its own first/last-seen metadata and one peer's
  disappearance does not remove or alter another peer

#### Scenario: A known peer changes its visible name

- **GIVEN** a peer is already stored with a valid identity and visible name
- **WHEN** a new DNS-SD/mDNS observation has the same peer_id, matching public
  key fingerprints, compatible protocol/capability and a different valid name
- **THEN** ClipVault updates that peer's visible name and observation timestamp
- **AND** preserves trust state, pinned certificate, paired metadata,
  first_seen_at and peer-bound collections/imports
- **AND** displays the new name without requiring another pairing

#### Scenario: A previously absent peer returns with a new name

- **GIVEN** a known peer is unavailable and retains its previous visible name
- **WHEN** it is resolved again with the same identity and a different valid
  visible name
- **THEN** the persisted peer row and UI use the new name while retaining its
  known trust and first-seen metadata

#### Scenario: A name change does not hide an identity conflict

- **WHEN** an observation reuses a known peer_id with a different public-key
  fingerprint, an incompatible protocol, or an invalid identity record
- **THEN** ClipVault reports/rejects the conflict without changing the stored
  identity, display name, trust state or paired metadata

#### Scenario: A known peer disappears

- **WHEN** a known peer is no longer observed before its presence TTL expires
- **THEN** the UI changes it to No disponible while retaining its history in
  the known-peer list
