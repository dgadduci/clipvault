## MODIFIED Requirements

### Requirement: Trust promotion originates only from a live authenticated transport

ClipVault SHALL bind a non-zero ephemeral TCP listener only while local sharing
is active and advertise its pairing capability through mDNS. The pairing
advertisement carries the full SHA-256 public-key fingerprint (64 hex chars)
alongside the short UI projection; the discovery-only advertisement carries
only the short fingerprint. A pairing record may become trusted only after
the live mTLS transport verifies both approvals over the same transcript.
The TLS identity SHALL be stable and bound to the persisted local peer
identity across restarts. The Hello envelope the runtime hands to the
transport MUST carry the canonical full fingerprint the discovery layer
advertised; the listener rejects any value that is not a 64-char hex string.
The persistence layer SHALL treat `discovery_only` → `pairing` for a peer with
unchanged peer_id, matching short and known full fingerprints, compatible
protocol and a valid compatible capability as a live-capability upgrade,
even when its validated display name has changed. It SHALL persist the new
name without changing trust or pairing state. The reverse capability
transition updates current capability without erasing the learned full
fingerprint or the latest valid name. The renderer SHALL offer a new pairing
attempt only while a detected peer is currently advertising `pairing`. While
the productive listener is IPv4-only, the private mDNS resolver SHALL select
an advertised IPv4 address for the pairing route; it SHALL NOT choose an
arbitrary AAAA record merely because it appears first in the resolver's
unordered address set.

#### Scenario: Renderer cannot forge remote approval

- **WHEN** a renderer submits a raw pairing message, certificate fingerprint,
  signature or claimed remote approval through Tauri IPC
- **THEN** no command accepts that input or promotes the peer; only the
  authenticated transport may deliver remote pairing events to the runtime

#### Scenario: Pairing listener starts

- **WHEN** sharing is enabled on a host with a secure local identity
- **THEN** ClipVault binds a real non-zero ephemeral TCP listener, advertises
  pairing capability through mDNS, and accepts only the bounded pairing/health
  protocol over TLS

#### Scenario: Discovery record upgrades to pairing

- **WHEN** a peer is first observed with `discovery_only` and then re-advertises
  `pairing` with the same stable identity and a valid full fingerprint
- **THEN** its persisted row is upgraded with that full fingerprint and the
  renderer can offer Vincular; withdrawing back to `discovery_only` removes
  that offer until a fresh pairing advertisement arrives

#### Scenario: Renamed peer remains trusted

- **GIVEN** a peer is already paired and advertises a different valid visible
  name with the same peer_id and public-key fingerprint
- **WHEN** the updated observation is persisted
- **THEN** ClipVault updates the visible name while keeping the peer trusted,
  preserving its pinned TLS certificate and pairing metadata
- **AND** no new pairing prompt is shown

#### Scenario: Changed identity remains a conflict

- **WHEN** a peer_id is announced with a different public-key fingerprint
- **THEN** ClipVault rejects the observation and preserves the existing trust
  and pairing record regardless of the announced display name

#### Scenario: Dual-stack discovery dials the IPv4 listener

- **WHEN** a pairing-capable peer advertises both A and AAAA records while its
  productive listener is bound only to IPv4
- **THEN** the resolver selects its advertised IPv4 address and the pairing
  attempt reaches the listener instead of failing due to arbitrary address-set
  iteration order
