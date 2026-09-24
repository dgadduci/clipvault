## MODIFIED Requirements

### Requirement: Continuous discovery uses bounded DNS-SD liveness confirmation

After local peer identity exists, ClipVault SHALL keep local peer sharing
disabled by default. When the user enables it, ClipVault SHALL continuously
advertise and browse the versioned _clipvault._tcp.local DNS-SD/mDNS service.
When disabled, it SHALL stop discovery and withdraw its advertisement without
removing known peers. While enabled, the platform adapter SHALL reannounce its
currently published DNS-SD record before the host-record TTL expires, and SHALL
consume browser resolved and removed events without scheduling application-level
DNS-SD verification, subnet or port scans, history requests, or TLS health
polling.

#### Scenario: Newly joined peer is discovered

- **WHEN** another compatible ClipVault installation enables sharing on the
  same local link while browsing is active
- **THEN** it appears without restarting, subnet scanning, custom broadcast or
  manual IP entry

#### Scenario: Sharing is disabled

- **WHEN** the user disables local peer sharing
- **THEN** ClipVault stops its renewal worker before advertising shutdown,
  sends its ordered DNS-SD goodbye where available, and retains only the
  persisted metadata of previously known peers

#### Scenario: A quiet peer remains announced

- **WHEN** sharing remains enabled for more than one host-record TTL without
  user interaction
- **THEN** the adapter reannounces the current service record before its TTL
  expires and does not call `ServiceDaemon::verify`
