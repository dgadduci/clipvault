## ADDED Requirements

### Requirement: Active trusted peers expose only pageable transferable text metadata

An active trusted peer SHALL expose a newest-first cursor-paginated list of
transferable text entries only. The cursor SHALL be an opaque, server-signed
HMAC-SHA256 token bound to the requesting peer; a row SHALL contain an opaque
remote reference, optional title, type, date and escaped bounded preview, but
SHALL NOT contain the complete content, tags, collections, favorites, source
metadata or hashes. The host SHALL exclude `ContentType::Html` from the
transferable set even when it is textual.

#### Scenario: First recent page over mTLS

- **WHEN** a user selects a trusted active peer from the desktop peer list
- **THEN** ClipVault dials the peer over mTLS, the host authenticates the
  caller against the pinned cert, the host projects the bounded transferable
  text page ordered newest first with at most 50 rows, and each card carries a
  two-line preview no longer than 300 characters

#### Scenario: Image or HTML row exists

- **WHEN** the host contains image or HTML entries
- **THEN** those entries are omitted from the remote page and no disabled row
  is rendered

#### Scenario: Text entry also has a rich representation

- **WHEN** the host contains a textual entry with rich-text metadata and a
  normalized plain-text value
- **THEN** the host includes only the bounded escaped preview built from that
  plain-text value, and does not expose rich references or rich bytes

#### Scenario: Forged or rotated cursor

- **WHEN** a client submits a cursor that was not emitted by that host, that
  carries a manipulated timestamp, or that was signed under a rotated secret
- **THEN** the host returns a typed invalid cursor outcome without entry data

#### Scenario: Revoked, blocked, not trusted or pin mismatch

- **WHEN** the caller is in `Revoked`, `Blocked`, not `Trusted`, presents a
  different cert fingerprint than the pinned value, or is not currently
  present
- **THEN** the host returns a typed unavailability outcome and no rows

#### Scenario: Trusted peer survives an application restart

- **GIVEN** a peer is `trusted` and its canonical TLS certificate
  fingerprint is persisted locally
- **WHEN** ClipVault restarts and local sharing starts again
- **THEN** ClipVault restores that pin before accepting or dialing mTLS
  history sessions, and browsing the trusted active peer does not fail
  solely because the verifier map was recreated

#### Scenario: Presence arrives before the pairing endpoint

- **WHEN** a trusted peer is observed through its initial `discovery_only`
  mDNS record or while its pairing record is being refreshed
- **THEN** ClipVault does not dial port `0`, retries resolution/connections only
  within a bounded transient window, and never reports the endpoint absence as
  `not_trusted`

### Requirement: Linked peers are a reactive desktop source inside the sidebar

The desktop SHALL render a `Linked peers` list **inside** the sidebar, below
the existing list of collections. Both lists SHALL be independent vertical
scrollers. The desktop SHALL keep exactly two columns: the sidebar and the
main panel. The list SHALL include every peer with persisted `trusted` state
and update without an application reload after a peer becomes trusted, after
its presence/health state changes, and explicitly after the pairing modal
closes. Each peer SHALL show a small green circle only while it is
`trusted && is_present`, and a small gray circle while it is trusted but
unavailable. Selecting an active peer SHALL replace the current local history
or collection content in the main panel with that peer's remote previews;
selecting a local collection or `Historial` SHALL clear the active peer and
restore the local rail; selecting an unavailable peer SHALL render its
unavailable state without sending a history request. There SHALL NOT be a
primary `Volver` button — return happens by selecting `Historial` or a local
collection.

The desktop shell (`App.svelte`) SHALL be the sole owner of the peer snapshot
the linked list and the remote history rail consume. It SHALL poll
`peerSnapshotCommand` on a bounded, locally-defined cadence while the desktop
is mounted, starting on `onMount` and stopping on `onDestroy`, and SHALL run
an initial refresh as soon as the desktop mounts. The `LinkedPeers` and
`RemoteHistoryRail` components SHALL NOT open `peerSnapshotCommand` (or any
equivalent peer-snapshot bridge call) on their own — they SHALL only render
the snapshot the parent supplies. The shared refresh SHALL be coalesced
through a single-flight guard so overlapping refresh requests (initial
refresh, post-pairing refresh, polling tick) reuse the same in-flight round
trip; a failed refresh SHALL keep the previous snapshot and MUST NOT break
the desktop or surface an intrusive error.

#### Scenario: New pairing updates the desktop list

- **WHEN** reciprocal approval promotes a peer to `trusted`, including when
  triggered through the pairing modal
- **THEN** ClipVault refreshes the peer snapshot when the pairing modal
  closes, the peer appears below collections in the sidebar without reload,
  with green or gray status derived from its current `trusted && is_present`
  state

#### Scenario: Remote presence flip is reflected on the desktop

- **WHEN** a paired peer's remote announcement arrives or disappears and the
  peer discovery runtime updates `is_present` for a row already marked as
  `trusted`
- **THEN** the linked list and the remote history rail update to reflect
  the new presence within the desktop's polling interval, without the user
  reopening the pairing modal or reloading the application

#### Scenario: Peer becomes unavailable

- **WHEN** a trusted peer loses recent presence or authenticated health
- **THEN** its dot becomes gray and selecting it does not request a remote
  page

#### Scenario: Local collection or Historial restores the local rail

- **WHEN** the user selects a local collection or `Historial` while a remote
  peer is open in the main panel
- **THEN** `activePeerId` is cleared and the local rail is restored without
  any remote request

### Requirement: Remote previews use read-only cards and peer-isolated state

Remote previews SHALL share the visual skeleton of local cards but use a
dedicated read-only component and state independent from local `HistoryCard`
controls and from other peers' pages. Browsing SHALL NOT create local history,
collections, clipboard writes, paste events or any content download beyond the
bounded preview. A remote card SHALL have no drag/drop, pin, edit, copy/paste
or local menu actions. Its only menu entry SHALL be a disabled `Importar
(próximamente)` placeholder. The remote card date SHALL be formatted with the
same helper the local card uses, not the raw RFC 3339 string. Local search /
source-app / tag filters SHALL NOT be applied to the remote rail.

#### Scenario: User switches peer while a page is loading

- **WHEN** a response for peer A arrives after the user opened peer B
- **THEN** the response for A is ignored and cannot overwrite B's rows or state

#### Scenario: User navigates a page

- **WHEN** the user selects next or previous page
- **THEN** ClipVault requests only the host cursor page and preserves no
  cross-peer cursor or list state

#### Scenario: Remote card menu is visible before import exists

- **WHEN** a user opens a remote preview card's menu
- **THEN** it contains only disabled `Importar (próximamente)` and neither
  downloads full text nor changes local persistence

### Requirement: Full text remains unavailable until the import change

This change SHALL NOT implement fetch_text, enabled Importar, peer collection
binding or remote provenance. A remote preview SHALL NOT cause complete content
to cross the transport.

#### Scenario: User views a preview

- **WHEN** a remote history page is rendered
- **THEN** the client has received only its bounded preview and no local entry
  or import record exists
