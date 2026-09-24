## ADDED Requirements

### Requirement: User can opt in to continuous same-link peer discovery

After local peer identity exists, ClipVault SHALL keep local peer sharing
disabled by default. When the user enables it, ClipVault SHALL continuously
advertise and browse the versioned _clipvault._tcp.local DNS-SD/mDNS service.
When disabled, it SHALL stop discovery and withdraw its advertisement without
removing known peers.

#### Scenario: Newly joined peer is discovered

- **WHEN** another compatible ClipVault installation enables sharing on the
  same local link while browsing is active
- **THEN** it appears without restarting, subnet scanning, custom broadcast or
  manual IP entry

#### Scenario: Sharing is disabled

- **WHEN** the user disables local peer sharing
- **THEN** ClipVault stops advertising and browsing and retains only the
  persisted metadata of previously known peers

### Requirement: Discovery persists N independent peers without endpoint identity

ClipVault SHALL remember N discovered peer identities from the current and
previous sessions. It SHALL persist only validated non-content metadata and
SHALL treat IP addresses and ports as transient runtime data, never as an
identity or persisted route.

#### Scenario: Multiple peers are independently observed

- **WHEN** several local peers are resolved, removed and resolved again
- **THEN** each peer retains its own first/last-seen metadata and one peer's
  disappearance does not remove or alter another peer

#### Scenario: A known peer disappears

- **WHEN** a known peer is no longer observed before its presence TTL expires
- **THEN** the UI changes it to No disponible while retaining its history in
  the known-peer list

### Requirement: Discovery publishes no clipboard data and exposes no access API

The discovery record SHALL contain only version, capability, local public
identity metadata and validated visible name. This change SHALL NOT provide a
pairing endpoint, trust operation, TLS history endpoint or text transfer.

#### Scenario: Unverified peer is displayed

- **WHEN** a valid unknown peer is discovered
- **THEN** it is shown as No verificado or Detectado and offers no action that
  can read or transfer remote content

#### Scenario: Malformed or conflicting announcement

- **WHEN** a service record is malformed, refers to the local peer_id or claims
  an existing peer_id with a different identity
- **THEN** ClipVault ignores it safely without replacing trusted/persisted data
  or logging record contents
