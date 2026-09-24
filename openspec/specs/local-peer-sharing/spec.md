# local-peer-sharing Specification

## Purpose
TBD - created by archiving change local-peer-text-transfer. Update Purpose after archive.

## Requirements

### Requirement: Opt-in local peer sharing has a stable secure identity

ClipVault SHALL keep local peer sharing disabled by default. When the user
explicitly enables it, the system SHALL provision or load a stable
cryptographic peer identity through the platform secure credential store and
SHALL persist only the non-secret sharing preference and validated visible
device name in local settings. The visible name SHALL NOT be used as
authentication or as a persistent key.

#### Scenario: Sharing is disabled on a fresh installation

- **WHEN** no local-peer setting has been persisted
- **THEN** ClipVault does not browse, advertise, bind a LAN listener or request
  local-network access

#### Scenario: User enables sharing

- **WHEN** the user enables Compartir en red local and the secure identity
  store succeeds
- **THEN** ClipVault saves the opt-in, starts the local peer runtime and uses
  its stable derived peer_id independently from future name or IP changes

#### Scenario: Secure store is unavailable

- **WHEN** the identity cannot be loaded or stored securely
- **THEN** sharing remains disabled, no listener or discovery starts, and the
  UI receives a typed safe explanation without private material in the error

#### Scenario: macOS local network declaration

- **WHEN** the packaged application runs on macOS and the user enables sharing
- **THEN** the bundle declares the local-network purpose and the ClipVault
  Bonjour service before the system prompts for access

### Requirement: Discover local peers continuously without scanning

While local peer sharing is enabled, ClipVault SHALL advertise and browse the
versioned _clipvault._tcp.local DNS-SD/mDNS service continuously. It SHALL
persist known peer metadata from current and prior sessions but SHALL treat
network addresses and ports as ephemeral discovery data rather than identity.

#### Scenario: Newly visible peer appears

- **WHEN** a compatible ClipVault peer begins advertising on the same local
  link while browsing is already active
- **THEN** it appears without restarting or scanning a subnet, with its
  validated visible name and No verificado state until paired

#### Scenario: Known peer becomes unavailable

- **WHEN** a known peer is removed from discovery or fails its recent
  authenticated health check
- **THEN** it remains in the list as No disponible, retains no stale route as
  identity, and cannot serve remote history until it is active again

#### Scenario: Multicast is unavailable

- **WHEN** the LAN blocks mDNS multicast or isolates the two devices
- **THEN** ClipVault shows no falsely reachable peer and does not fall back to
  port scanning, custom broadcast or manual-IP connection in this version

#### Scenario: Discovery has no clipboard disclosure

- **WHEN** ClipVault publishes or consumes a service record
- **THEN** the record contains only protocol/identity/presence metadata and
  never clipboard text, previews, counts, content hashes, tags, collections,
  source-app metadata, paths or secret material

### Requirement: Pairing requires reciprocal short-code approval

ClipVault SHALL permit a discovered unverified peer to become trusted only
after both devices explicitly approve one matching six-digit pairing code
within a bounded pairing session. A successful pairing SHALL store the peer's
public identity and grant mutual access; every later session SHALL authenticate
silently with TLS client authentication and pinned public keys.

#### Scenario: Both users approve the same code

- **WHEN** each device displays the same code for a pending request and both
  users confirm it before expiration
- **THEN** both peer registries record the other identity as trusted and either
  side may later request the permitted peer API without another code

#### Scenario: One side cancels or pairing expires

- **WHEN** either user rejects, closes, or does not approve the request before
  the pairing timeout
- **THEN** neither side creates a trusted relationship and no remote history
  becomes accessible

#### Scenario: Untrusted or changed identity connects

- **WHEN** a request presents a certificate whose public identity is not the
  pinned trusted identity for that peer_id
- **THEN** ClipVault rejects it before history access, does not replace the
  stored identity automatically and does not reveal clipboard data

### Requirement: Known peers expose accurate safe states and revocation

ClipVault SHALL list peers discovered in this session and known from earlier
sessions, showing a name, abbreviated fingerprint, typed trust state and
current availability. Activo requires recent discovery plus an authenticated
health check, not merely an old mDNS record. The user SHALL be able to
desvinculate or block a peer without deleting already imported local data.
Each local identity SHALL support N independent known-peer relationships;
changing the state of one peer SHALL NOT change the trust, availability,
collections or imports of another.

#### Scenario: Trusted peer is active

- **WHEN** a trusted compatible peer is discovered and completes a recent
  pinned TLS health check
- **THEN** it is shown as Activo and offers remote-history navigation

#### Scenario: User revokes a peer

- **WHEN** the user desvinculates a trusted peer
- **THEN** subsequent authenticated access is denied until a new reciprocal
  pairing succeeds, while the known-peer observation and local imports remain

#### Scenario: User blocks a peer

- **WHEN** the user blocks a known peer
- **THEN** its existing trust is revoked, future pairing and connection attempts
  for that identity are denied locally, and its local collection and imported
  entries remain unchanged

#### Scenario: Multiple peers have independent state

- **WHEN** one installation has trusted several local peers and the user
  blocks, revokes or loses connectivity with one of them
- **THEN** the remaining trusted peers retain their own state and continue to
  be browsable/importable whenever each is active

### Requirement: Local peer transport is bounded and mutually authenticated

The peer runtime SHALL use direct local TCP protected by TLS. Except for the
strictly bounded reciprocal pairing exchange, the server SHALL require a
trusted pinned client identity. The runtime SHALL enforce protocol-major
compatibility, message size limits, rate limits and timeouts, and SHALL emit
only metadata-safe diagnostics.

#### Scenario: Incompatible protocol version

- **WHEN** discovery or a handshake reports a protocol major unsupported by
  this ClipVault version
- **THEN** the peer is shown as incompatible and cannot be paired, browsed or
  imported from

#### Scenario: Oversized or malformed request

- **WHEN** a peer sends a malformed frame, unsupported field or body above the
  negotiated endpoint limit
- **THEN** the request is rejected, the connection is safely closed as needed,
  and logs/events omit the body and any clipboard content

#### Scenario: Sharing is disabled after use

- **WHEN** the local user disables sharing
- **THEN** ClipVault withdraws its service, stops accepting local peer traffic
  and leaves known peers, imports, collections and normal local history intact
