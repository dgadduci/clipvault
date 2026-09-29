## MODIFIED Requirements

### Requirement: Pairing requires reciprocal short-code approval

ClipVault SHALL promote a discovered unverified peer to trusted only after both
devices approve the same six-digit code in one bounded pairing session. A
successful session SHALL pin the other public identity for future mutual TLS
authentication. The six-digit code SHALL be derived from the canonical
SHA-256 transcript over both peers' real nonces, peer_ids, full public-key
fingerprints and the protocol major; the derivation SHALL be order-independent
so both sides compute the same code regardless of who initiated the session.
Each device SHALL allow its user to approve locally until that device's local
approval is recorded, regardless of whether the remote approval arrived first.
ClipVault SHALL promote the relationship as soon as both approvals are
recorded, regardless of which approval arrives second. If sending a local
approval fails, ClipVault SHALL keep the peer untrusted, make the local
approval retryable, and show the typed failure instead of a waiting status.

#### Scenario: Both users approve

- **WHEN** two users confirm the same code before the pairing session expires,
  including when one approval arrives before the other user confirms
- **THEN** the second user can still approve locally, that second approval
  completes promotion without requiring another remote event, both known-peer
  records become trusted, and later health checks authenticate without another
  pairing prompt; the metadata-only peer snapshot reports `trust_state =
  trusted` so both dialogs replace any transient waiting state with the
  successful-link confirmation

#### Scenario: Local approval transport failure

- **WHEN** a user confirms the SAS but the local transport cannot send that
  approval
- **THEN** the peer remains untrusted, the session does not claim a successful
  local approval, the modal shows the typed failure and permits a retry while
  the session remains valid

#### Scenario: Pairing is cancelled or expires

- **WHEN** either user cancels, closes the modal, loses the connection or fails
  to approve before two minutes
- **THEN** neither peer becomes trusted and all pairing-only state is discarded
