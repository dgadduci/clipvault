## ADDED Requirements

### Requirement: Capability advertisement is additive and backwards compatible

The TXT advertisement the runtime publishes SHALL continue to set the
`capability` field to one of the canonical values (`pairing`,
`discovery_only`) so legacy clients that only accept the exact value
`pairing` keep recognizing the record unchanged. Additive capabilities
discovered after the legacy contract shipped (such as `image_import`) SHALL
be published through a separate TXT field (`caps_extra`) the legacy parser
ignores. A runtime that supports the new contract SHALL recognise both the
legacy `capability` field and the additive `caps_extra` field, combine the
two token lists, and reject any token outside the documented allowlist.

#### Scenario: Legacy peer advertises only pairing

- **WHEN** a host's TXT record carries `capability = pairing` and no
  `caps_extra`
- **THEN** the discovery layer treats the peer as having exactly the
  `pairing` capability and ignores the absent `caps_extra`

#### Scenario: New peer advertises pairing plus image_import

- **WHEN** a host's TXT record carries `capability = pairing` and
  `caps_extra = image_import`
- **THEN** the discovery layer resolves the peer as having both `pairing`
  and `image_import` capabilities and the productive image routes accept
  the request

#### Scenario: Unknown token in caps_extra is rejected

- **WHEN** a host's TXT record carries an additive token outside the
  documented allowlist (e.g. `caps_extra = image_import,future_token`)
- **THEN** the discovery layer rejects the advertisement as
  `UnsupportedCapability` so a misconfigured host cannot silently inject
  capabilities the runtime did not opt into

## MODIFIED Requirements

### Requirement: Active trusted peers expose only pageable transferable text metadata

An active trusted peer SHALL expose a newest-first cursor-paginated list of
transferable text and image entries. Text rows SHALL contain an opaque remote
reference, optional title, type, date and escaped bounded preview. Image rows
SHALL contain an opaque remote reference, optional title, type, date, byte size
and dimensions, but no image bytes or thumbnail. Neither row SHALL contain
tags, collections, favorites, source metadata, filesystem paths, asset
references or hashes. The host SHALL exclude `ContentType::Html` from the
transferable set even when it is textual.

#### Scenario: First recent page over mTLS

- **WHEN** a user selects a trusted active peer from the desktop peer list
- **THEN** ClipVault dials the peer over mTLS, the host authenticates the
  caller against the pinned cert, projects a bounded page ordered newest first
  with at most 50 rows, and renders text previews or image metadata according
  to each row type

#### Scenario: Image or HTML row exists

- **WHEN** the host contains a valid transferable image entry
- **THEN** the row is included with metadata only and the client renders the
  common static image placeholder without fetching image bytes

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

- **WHEN** a client submits a cursor that was not emitted by that host, carries
  a manipulated timestamp, or was signed under a rotated secret
- **THEN** the host returns a typed invalid cursor outcome without entry data

#### Scenario: Revoked, blocked, not trusted or pin mismatch

- **WHEN** the caller is revoked, blocked, not trusted, presents a different
  cert fingerprint than the pinned value, or is not currently present
- **THEN** the host returns a typed unavailability outcome and no rows

#### Scenario: Trusted peer survives an application restart

- **GIVEN** a peer is trusted and its canonical TLS certificate fingerprint is
  persisted locally
- **WHEN** ClipVault restarts and local sharing starts again
- **THEN** ClipVault restores that pin before accepting or dialing mTLS history
  sessions, and browsing does not fail solely because the verifier map was
  recreated

#### Scenario: Presence arrives before the pairing endpoint

- **WHEN** a trusted peer is observed through an initial `discovery_only`
  mDNS record or while its pairing record is refreshed
- **THEN** ClipVault does not dial port `0`, retries only within a bounded
  transient window, and never reports endpoint absence as `not_trusted`

### Requirement: Remote previews use read-only cards and peer-isolated state

Remote previews SHALL share the visual skeleton of local cards but use a
dedicated read-only component and state independent from local `HistoryCard`
controls and other peers' pages. Browsing SHALL NOT create local history,
collections, clipboard writes, paste events or complete content downloads. A
remote card SHALL have no drag/drop, pin, edit, copy/paste or local menu
actions. For a transferable text or image row it SHALL expose an explicit
`Importar` action; while importing it SHALL show busy state and after failure
it SHALL expose a safe retry. An image row SHALL render the common static
placeholder, not a remote thumbnail. The remote card date SHALL use the same
formatter as local cards, and local search/source-app/tag filters SHALL NOT
apply to the remote rail.

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

- **WHEN** a listed image row is rendered
- **THEN** the card shows the static placeholder and metadata, and opening the
  card does not download the image until the user activates Importar

#### Scenario: User imports an image from the remote card

- **WHEN** the user activates Importar on a valid image row
- **THEN** the card shows busy state, delegates to the image import service and
  refreshes local organization only after a successful commit

#### Scenario: Remote card is not transferable

- **WHEN** a row is no longer eligible for import or the peer lacks the image
  capability
- **THEN** the card does not offer a successful import path and no request for
  complete content or image bytes is made

#### Scenario: Remote card menu is visible before import exists

- **WHEN** a user opens the menu of a remote preview card
- **THEN** an eligible text or image row exposes the explicit `Importar`
  action, while an ineligible row exposes no successful import action and the
  menu never downloads complete content by itself

### Requirement: Full text remains unavailable until the import change

Remote browsing SHALL receive only bounded text previews or image metadata. The
separate text and image import capabilities MAY fetch complete content only
after the user activates Importar for that row. Browsing alone SHALL NOT create
local entries or import records.

#### Scenario: User views a preview

- **WHEN** a remote history page is rendered
- **THEN** the client has received only bounded text preview data or image
  metadata and no local entry or import record exists

#### Scenario: User imports after browsing

- **WHEN** the user explicitly activates Importar for a transferable row
- **THEN** only that selected row is fetched over the authenticated peer
  transport and the corresponding import capability controls persistence
