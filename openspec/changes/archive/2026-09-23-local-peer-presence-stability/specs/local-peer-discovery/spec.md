## MODIFIED Requirements

### Requirement: Discovery presence is adapter-authoritative and remains live while resolvable

ClipVault SHALL persist only validated non-content discovery metadata and treat
IP addresses and ports as transient runtime data. A valid resolved service
marks its peer present; a browser-reported DNS-SD goodbye or cache-expiration
removal marks it unavailable. The core and adapter SHALL NOT mark a peer absent
because a locally chosen interval elapsed or a periodic forced reconfirmation
timed out.

#### Scenario: Multiple peers are independently observed

- **WHEN** several local peers are resolved, removed and resolved again
- **THEN** each peer retains its own first/last-seen metadata and one peer's
  disappearance does not remove or alter another peer

#### Scenario: A quiet healthy peer remains present

- **WHEN** a compatible peer remains published and resolvable for more than
  three former 120-second presence windows without interaction
- **THEN** it remains Detectado or Activo unless the browser emits
  `ServiceRemoved`

#### Scenario: A known peer disappears

- **WHEN** the browser receives a DNS-SD goodbye or cache-expiration
  `ServiceRemoved` for a known peer
- **THEN** the UI changes it to No disponible while retaining its known-peer
  metadata

### Requirement: Continuous discovery uses bounded DNS-SD liveness confirmation

After local peer identity exists, ClipVault SHALL keep local peer sharing
disabled by default. When the user enables it, ClipVault SHALL continuously
advertise and browse the versioned _clipvault._tcp.local DNS-SD/mDNS service.
When disabled, it SHALL stop discovery and withdraw its advertisement without
removing known peers. While enabled, the platform adapter SHALL consume the
browser's resolved and removed events without scheduling application-level
DNS-SD verification, subnet or port scans, history requests, or TLS health
polling.

#### Scenario: Newly joined peer is discovered

- **WHEN** another compatible ClipVault installation enables sharing on the
  same local link while browsing is active
- **THEN** it appears without restarting, subnet scanning, custom broadcast or
  manual IP entry

#### Scenario: Sharing is disabled

- **WHEN** the user disables local peer sharing
- **THEN** ClipVault stops advertising and browsing, sends its ordered DNS-SD
  goodbye where available, and retains only the persisted metadata of
  previously known peers

#### Scenario: A quiet peer remains announced

- **WHEN** sharing remains enabled for more than one host-record TTL without
  user interaction
- **THEN** the adapter reannounces the current service record before its TTL
  expires and does not call `ServiceDaemon::verify`
