## ADDED Requirements

### Requirement: Discovery presence is adapter-authoritative and remains live while resolvable

ClipVault SHALL remember N discovered peer identities from the current and
previous sessions. It SHALL persist only validated non-content metadata and
SHALL treat IP addresses and ports as transient runtime data, never as an
identity or persisted route. Presence is an in-memory projection of the
platform discovery adapter: a valid resolved service marks its peer present,
and a DNS-SD goodbye, cache expiration, or bounded failed DNS-SD verification
marks it unavailable. The core SHALL NOT turn a present peer unavailable only
because a locally chosen interval elapsed since the last resolution event.

#### Scenario: Multiple peers are independently observed

- **WHEN** several local peers are resolved, removed and resolved again
- **THEN** each peer retains its own first/last-seen metadata and one peer's
  disappearance does not remove or alter another peer

#### Scenario: A quiet healthy peer remains present

- **WHEN** a compatible peer remains published and resolvable on the local
  link for more than three former 120-second presence windows without user
  interaction
- **THEN** it remains Detectado or Activo and is not downgraded solely because
  no duplicate resolution event reached the core

#### Scenario: A known peer disappears

- **WHEN** the adapter receives a DNS-SD goodbye, cache-expiration removal, or
  its bounded DNS-SD verification confirms that a known peer is gone
- **THEN** the UI changes it to No disponible while retaining its history in
  the known-peer list

### Requirement: Continuous discovery uses bounded DNS-SD liveness confirmation

After local peer identity exists, ClipVault SHALL keep local peer sharing
disabled by default. When the user enables it, ClipVault SHALL continuously
advertise and browse the versioned _clipvault._tcp.local DNS-SD/mDNS service.
When disabled, it SHALL stop discovery and withdraw its advertisement without
removing known peers. While enabled, the platform adapter SHALL perform only
bounded DNS-SD verification of already-resolved service instances as needed to
distinguish an abrupt disappearance; it SHALL NOT scan subnets or ports,
accept manual IP input, invoke the history API, or perform TLS health polling.

#### Scenario: Newly joined peer is discovered

- **WHEN** another compatible ClipVault installation enables sharing on the
  same local link while browsing is active
- **THEN** it appears without restarting, subnet scanning, custom broadcast or
  manual IP entry

#### Scenario: Sharing is disabled

- **WHEN** the user disables local peer sharing
- **THEN** ClipVault cancels pending presence verification, stops advertising
  and browsing, sends its ordered DNS-SD goodbye where available, and retains
  only the persisted metadata of previously known peers
