## ADDED Requirements

### Requirement: Pairing requires reciprocal short-code approval

ClipVault SHALL promote a discovered unverified peer to trusted only after both
devices approve the same six-digit code in one bounded pairing session. A
successful session SHALL pin the other public identity for future mutual TLS
authentication. The six-digit code SHALL be derived from the canonical
SHA-256 transcript over both peers' real nonces, peer_ids, full public-key
fingerprints and the protocol major; the derivation SHALL be order-independent
so both sides compute the same code regardless of who initiated the session.

#### Scenario: Both users approve

- **WHEN** two users confirm the same code before the pairing session expires
- **THEN** both known-peer records become trusted and later health checks
  authenticate without another pairing prompt

#### Scenario: Pairing is cancelled or expires

- **WHEN** either user cancels, closes the modal, loses the connection or fails
  to approve before two minutes
- **THEN** neither peer becomes trusted and all pairing-only state is discarded

### Requirement: Trusted access uses pinned mutual TLS

ClipVault SHALL use direct local TLS with client authentication for trusted
peers. The pinned value is the SHA-256 of the DER-encoded certificate the
remote peer presented during the mTLS handshake; the verifier MUST reject
a connection presenting a different cert at the handshake itself, and the
health probe MUST derive the presented fingerprint from the cert the mTLS
handshake authenticated rather than the string the runtime persisted.
An identity mismatch, unknown identity, revoked peer or blocked peer SHALL
be denied before serving any application API.

#### Scenario: Trusted reconnection

- **WHEN** a trusted peer reconnects with its pinned identity
- **THEN** health succeeds silently and it may be marked Activo after recent
  discovery

#### Scenario: Changed or untrusted identity

- **WHEN** a connection claims a peer_id with a changed, unknown, revoked or
  blocked public identity
- **THEN** ClipVault rejects it without replacing persistence or disclosing
  clipboard data

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
advertised; the listener rejects any value that is not a 64-char hex
string.

#### Scenario: Renderer cannot forge remote approval

- **WHEN** a renderer submits a raw pairing message, certificate fingerprint,
  signature or claimed remote approval through Tauri IPC
- **THEN** no command accepts that input or promotes the peer; only the
  authenticated transport may deliver remote pairing events to the runtime

#### Scenario: Pairing listener starts

- **WHEN** sharing is enabled on a host with a secure local identity
- **THEN** ClipVault binds a real non-zero ephemeral port, advertises pairing
  capability through mDNS, and accepts only the bounded pairing/health
  protocol over TLS

### Requirement: Inbound pairing registers the session before approval

When the pairing listener receives a `Hello` envelope, the transport SHALL
emit a metadata-only event into the runtime carrying the canonical six-digit
SAS, the opaque session id, the remote peer_id, the full public-key
fingerprint, the display name and the session expiration BEFORE the
listener sends `HelloAck` over the open mTLS connection. The transport's
`approve_inbound_session` op releases the listener's bounded wait so the
local user can review the SAS and approve the pairing before any approval
envelope travels the wire. Only after `approve_local` on each side does the
listener sign and send its own `Approve`; the listener MUST send its
`Approve` even when the dialer is the only one expected to sign, so the
dual approval gate cannot be bypassed by a missing acknowledgement.
The metadata-only session snapshot SHALL mark listener-originated sessions as
inbound. The shell SHALL use that marker to surface the same approval dialog
without requiring the receiving user to open the sharing settings or start a
second outbound pairing operation. Closing the dialog SHALL cancel its opaque
session id and suppress only that id until the runtime removes it; a later
invitation with a new id remains eligible to be shown.

#### Scenario: Inbound session appears before HelloAck

- **WHEN** the listener receives a Hello envelope
- **THEN** the runtime receives a metadata-only event with the SAS, peer_id,
  full fingerprint, display name, opaque session id and expiration BEFORE
  the transport sends HelloAck to the peer

#### Scenario: Inbound approval releases the bounded wait

- **WHEN** the runtime calls `approve_inbound_session` after the user accepts
  the SAS
- **THEN** the listener continues the bounded pairing protocol and signs /
  sends its own `Approve` envelope; if no approval lands before the session
  timeout the listener closes the connection without promoting the peer

#### Scenario: Receiver sees an inbound invitation automatically

- **WHEN** a remote peer starts pairing while the receiving desktop is open
  but its sharing settings panel is closed
- **THEN** the shell reads the inbound metadata-only snapshot and opens the
  same SAS dialog for that peer without sending another `Hello` or an
  approval; dismissing it cancels that session and a new future invitation can
  still be displayed

### Requirement: Peer trust state supports N independent relationships

Each local identity SHALL retain N independent peer trust states. Revoking or
blocking one peer SHALL close only that peer's sessions, disarm the per-peer
cert fingerprint pin and SHALL NOT change the health, trust, collection or
import records of any other peer.

#### Scenario: One of many peers is blocked

- **WHEN** a user blocks one trusted peer while other peers remain active
- **THEN** the blocked peer cannot pair or health-check, the pin the runtime
  armed for the now-blocked peer is disarmed and the other peers keep their
  independent trusted availability

### Requirement: Pairing transport exposes no history content

Before the history-browser change, the local peer listener SHALL expose only
pairing and metadata-only health. It SHALL reject history, fetch and import
requests regardless of trust state.

#### Scenario: Trusted peer asks for history early

- **WHEN** a trusted peer requests a history endpoint not implemented in this
  change
- **THEN** the server returns a typed unavailable outcome without any entry
  metadata or content

### Requirement: Discovery and pairing share one mDNS adapter lifecycle

The local peer discovery runtime and the productive pairing transport MUST
operate on the same concrete `MdnsPeerDiscoveryAdapter` instance. The runtime
owns the adapter lifecycle (only it invokes `start` with the runtime sink
and `stop` on toggle transitions); the pairing transport invokes
`reconfigure` on the running adapter after binding the ephemeral TLS
listener so the published record flips to `capability = pairing` with the
real port WITHOUT restarting the browse loop, WITHOUT replacing the sink
and WITHOUT forcing the runtime to re-install its worker. The browse loop
must keep feeding `DiscoveryEvent::Observed` and `DiscoveryEvent::Removed`
to the runtime while pairing is active so a remote peer becomes present
on both sides regardless of which host toggled first. `withdraw` MUST
reconfigure the adapter back to `capability = discovery_only` with the
discovery-only port placeholder; it MUST NOT call `stop` on the adapter
because the runtime owns the lifecycle. A pairing `publish` or
`reconfigure` against a stopped adapter MUST surface the typed reason
the runtime already documents (`MalformedAdvertisement` /
`AlreadyRunning`) instead of silently downgrading the published record. The
DNS-SD instance name and the SRV target hostname MUST be derived from the
stable local `peer_id`, while the editable visible name remains TXT metadata.
The SRV target MUST be a DNS-safe `.local.` hostname and MUST NOT reuse the
service fullname containing `_clipvault._tcp`.

#### Scenario: Toggle ON keeps the discovery browse loop alive

- **WHEN** the user toggles `Compartir en red local` ON and the runtime is
  already running OR starts first
- **THEN** the pairing transport flips the published record via
  `reconfigure`, the runtime's browse loop continues feeding events into
  the runtime sink and a remote peer becomes `available` in the local
  presence table while the pairing listener is bound

#### Scenario: Toggle OFF leaves the adapter under runtime control

- **WHEN** the user toggles sharing OFF
- **THEN** the runtime stops the adapter, the pairing transport's `withdraw`
  collapses to a no-op because the adapter is already stopped and the
  pairing listener is torn down without disturbing any other adapter
  lifecycle

#### Scenario: DNS-SD service resolves across implementations

- **WHEN** two peers have equal visible device names or one browser is a
  stricter DNS-SD implementation than the announcing host
- **THEN** each peer receives a resolvable SRV/TXT/A-or-AAAA record set and
  reaches `Detected` presence using the peer-id-derived instance and hostname

### Requirement: Restart with toggle active reinstalls the pairing listener

When the persisted `local_peer_sharing_enabled` setting is `true` at startup,
the bootstrap MUST reinstall the productive pairing transport (listener,
advertisement and mDNS resolver) in addition to the discovery runtime so the
toggle contract remains consistent across restarts. The bootstrap MUST start
the discovery runtime BEFORE the pairing transport so the shared mDNS
adapter is running with the runtime sink when the pairing install path
calls `reconfigure`. The toggle response returned by
`clipvault_peer_sharing_toggle_get` MUST reflect both the discovery runtime
state and the productive pairing listener state; a toggle reported as
`active` while the pairing listener never bound is a contract violation.

#### Scenario: Toggle active survives a restart

- **WHEN** ClipVault starts with `local_peer_sharing_enabled = true`
- **THEN** the bootstrap reinstalls the discovery runtime AND the pairing
  listener before the first UI poll; `clipvault_peer_sharing_toggle_get`
  reports `active` only when both the discovery runtime and the pairing
  listener are bound

#### Scenario: Pairing install fails after restart

- **WHEN** the bootstrap restart reinstalls the discovery runtime but the
  productive pairing listener fails to bind
- **THEN** `clipvault_peer_sharing_toggle_get` reports `runtime_stopped`
  rather than `active` so the UI surfaces the documented degraded copy
