## MODIFIED Requirements

### Requirement: Active trusted peers expose only pageable transferable text metadata

An active trusted peer SHALL expose a newest-first cursor-paginated list of
transferable text and image entries. Text rows SHALL contain an opaque remote
reference, optional title, type, date and escaped bounded preview. Image rows
SHALL contain an opaque remote reference, optional title, type, date, byte size
and dimensions, but no image bytes or thumbnail. Neither row SHALL contain
tags, collections, favorites, source metadata, filesystem paths, asset
references or hashes. The host SHALL exclude `ContentType::Html` from the
transferable set even when it is textual. A thumbnail, when supported, SHALL
be requested separately under the requirements of
`peer-image-preview-thumbnails`.

#### Scenario: First recent page over mTLS

- **WHEN** a user selects a trusted active peer from the desktop peer list
- **THEN** ClipVault dials the peer over mTLS, the host authenticates the
  caller against the pinned cert, projects a bounded page ordered newest first
  with at most 50 rows, and renders text previews or image metadata according
  to each row type

#### Scenario: Image or HTML row exists

- **WHEN** the host contains a valid transferable image entry
- **THEN** the row is included with metadata only and the client initially
  renders the common static image placeholder
- **AND** a separate bounded thumbnail request MAY occur only when the card is
  visible and both peers support the thumbnail capability

#### Scenario: Invalid image or HTML row exists

- **WHEN** the host contains an image with an invalid/unavailable asset or an
  HTML entry
- **THEN** that entry is omitted from the transferable page and no disabled
  image row is rendered

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

### Requirement: Remote previews use read-only cards and peer-isolated state

Remote previews SHALL share the visual skeleton of local cards but use a
dedicated read-only component and state independent from local `HistoryCard`
controls and other peers' pages. Browsing SHALL NOT create local history,
collections, clipboard writes, paste events or complete content downloads. A
remote card SHALL have no drag/drop, pin, edit, copy/paste or local menu
actions. For a transferable text or image row it SHALL expose an explicit
`Importar` action; while importing it SHALL show busy state and after failure
it SHALL expose a safe retry. An image row SHALL initially render the common
static placeholder; a supported, visible row MAY replace it with the bounded
thumbnail defined by `peer-image-preview-thumbnails`. The remote card date
SHALL use the same formatter as local cards, and local search/source-app/tag
filters SHALL NOT apply to the remote rail.

#### Scenario: User switches peer while a page is loading

- **WHEN** a response for peer A arrives after the user opened peer B
- **THEN** the response for A is ignored and cannot overwrite B's rows or
  state

#### Scenario: User navigates a page

- **WHEN** the user selects next or previous page
- **THEN** ClipVault requests only the host cursor page and preserves no
  cross-peer cursor or list state

#### Scenario: Per-stream buffers surface before the next host request

- **WHEN** a page response hides rows in `textBuffer` or `imageBuffer` because
  the combined `MAX_COMBINED_PAGE_ROWS` cap clipped them and the user
  activates Siguiente
- **THEN** the rail first promotes the buffered rows into the visible window
  newest-first; only when both buffers are empty does it request the next
  page from the host via `pickNextCursors`

#### Scenario: Siguiente with exhausted cursors and a non-empty buffer

- **WHEN** both `cursor` and `imageCursor` are empty (both streams
  exhausted) but `textBuffer` or `imageBuffer` still contains hidden rows
- **THEN** the rail keeps the Siguiente button enabled, promotes the
  buffered rows to the visible window, and only marks the rail as exhausted
  once both buffers AND both cursors are empty

#### Scenario: Next page preserves the hidden rows as buffered

- **WHEN** a host response produces fewer than `MAX_COMBINED_PAGE_ROWS` rows
  but more rows remain in either buffer or in either stream
- **THEN** the visible window shows the newest 50 rows the host returned
  (without re-injecting previously visible rows), the remaining rows live
  in `textBuffer` / `imageBuffer`, and no `remote_entry_id` is duplicated or
  dropped

#### Scenario: User opens an image preview card

- **WHEN** a listed image card is visible in the rail
- **THEN** it initially shows the common static placeholder
- **AND** it requests only a bounded thumbnail if the peer advertises the
  thumbnail capability
- **AND** it never fetches the complete image unless the user activates
  `Importar`

#### Scenario: User imports an image from the remote card

- **WHEN** the user activates Importar on a valid image row
- **THEN** the card shows busy state, delegates to the image import service and
  refreshes local organization only after a successful commit

#### Scenario: Remote card is not transferable

- **WHEN** a row is no longer eligible for import or the peer lacks the image
  capability
- **THEN** the card does not offer a successful import path and no request for
  complete content or original image bytes is made

#### Scenario: Remote card menu is visible before import exists

- **WHEN** a user opens the menu of a remote preview card
- **THEN** an eligible text or image row exposes the explicit `Importar`
  action, while an ineligible row exposes no successful import action and the
  menu never downloads complete content by itself

#### Scenario: Thumbnail response is stale or unavailable

- **WHEN** the selected peer changes, the card unmounts, or thumbnail loading
  fails
- **THEN** a late response cannot affect the current peer or another row, the
  placeholder remains available, and the remote rail has no global load error

### Requirement: Full text remains unavailable until the import change

Remote browsing SHALL receive only bounded text previews or image metadata in
history-page responses. A separate visible-card thumbnail route MAY transfer
only the bounded derived image specified by `peer-image-preview-thumbnails`.
Complete text or original image bytes SHALL be fetched only after the user
activates Importar for that row. Browsing alone SHALL NOT create local entries
or import records.

#### Scenario: User views a preview

- **WHEN** a remote history page is rendered and an image card becomes
  visible
- **THEN** the client has received only bounded text preview data or image
  metadata in the page, and at most the separately requested bounded image
  thumbnail; no local entry or import record exists

#### Scenario: User imports after browsing

- **WHEN** the user explicitly activates Importar for a transferable row
- **THEN** only that selected row is fetched over the authenticated peer
  transport and the corresponding import capability controls persistence
