## ADDED Requirements

### Requirement: Remote image thumbnails are fetched separately and on demand

ClipVault SHALL keep remote image-history pages metadata-only. A client MAY
request a bounded derived thumbnail for a listed image only through a
dedicated authenticated thumbnail route, and only after that image card enters
the visible remote-history viewport. The request SHALL contain only the
opaque remote entry identifier and protocol metadata; it SHALL NOT contain a
local asset reference, path or hash. The thumbnail response SHALL be a
downscaled PNG with its longest side no greater than 256 pixels and an encoded
body no larger than 384 KiB. The serialized response envelope SHALL be capped
at no less than 544 KiB to accommodate base64 framing while remaining bounded.
The thumbnail SHALL be generated from the currently validated host asset in
memory and SHALL NOT be persisted or substituted for the original-image
import.

#### Scenario: Visible image card requests a bounded thumbnail

- **GIVEN** the selected peer is active, trusted, and advertises both
  `image_import` and `image_preview_thumbnail`
- **WHEN** an eligible image card enters the visible remote-history viewport
- **THEN** the client requests its thumbnail through the dedicated mTLS route
- **AND** the host returns only a valid derived PNG within the dimension,
  body and response-envelope limits
- **AND** the history-page response itself remains metadata-only

#### Scenario: Image card has not entered the viewport

- **WHEN** an eligible image row is present on a page but its card is outside
  the visible remote-history viewport
- **THEN** ClipVault makes no thumbnail request for that row
- **AND** the card retains the common static placeholder

#### Scenario: Peer supports image import but not thumbnail capability

- **WHEN** an active trusted peer advertises `image_import` but does not
  advertise `image_preview_thumbnail`
- **THEN** the client does not call the thumbnail route and keeps the static
  placeholder
- **AND** text browsing and explicit image import continue to work

#### Scenario: Host rejects stale or ineligible image reference

- **WHEN** a thumbnail request refers to a deleted, changed, invalid or
  no-longer-transferable image, or its caller is no longer authorized
- **THEN** the host returns a typed safe unavailable outcome without returning
  the original image or exposing an asset reference, path or hash

### Requirement: Thumbnail capability is additive and privacy preserving

The runtime SHALL advertise `image_preview_thumbnail` only through the
additive `caps_extra` field while keeping the legacy `capability` field equal
to `pairing`. Discovery SHALL accept this documented token and continue to
reject unknown tokens. The host SHALL require a trusted active mTLS caller,
the pinned certificate, and both `image_import` and
`image_preview_thumbnail` for every thumbnail request. The host SHALL bound
concurrent image decode/resize work to two requests per peer, and the client
SHALL bound concurrent requests from the active rail to two.

Thumbnail browsing SHALL NOT create local history, collection membership,
import provenance or clipboard changes; it SHALL NOT synthesize paste, alter
the active application, or change local drag-and-drop payloads. Thumbnail
bytes, hashes, asset references and filesystem paths SHALL NOT appear in logs.

#### Scenario: New peer advertises the thumbnail capability compatibly

- **WHEN** a thumbnail-capable peer publishes its discovery record
- **THEN** it keeps `capability = pairing` and includes
  `image_import,image_preview_thumbnail` in `caps_extra`
- **AND** clients that ignore `caps_extra` continue to recognize the peer as a
  legacy pairing peer

#### Scenario: Capability disappears after the page was loaded

- **WHEN** a peer loses either required image capability before a thumbnail
  request is served
- **THEN** the host rejects that request with a typed unavailable outcome
- **AND** the client keeps the static placeholder without showing a global
  history error

#### Scenario: Thumbnail processing exceeds a concurrency bound

- **WHEN** more than two thumbnail operations for one peer would be processed
  concurrently at the host, or more than two are in flight from the active
  rail
- **THEN** the additional operation is deferred or returns a typed busy
  outcome without exceeding the configured bound or blocking the UI

#### Scenario: Thumbnail request and response are private

- **WHEN** a thumbnail succeeds or fails
- **THEN** diagnostics contain only stable outcome metadata and bounded
  dimensions/size where useful
- **AND** no image bytes, content hash, asset reference or filesystem path is
  logged, persisted or sent in an unrelated event

### Requirement: Remote thumbnail UI is bounded and stale-safe

The remote image card SHALL show the existing static placeholder while a
thumbnail is loading and whenever the peer lacks the thumbnail capability or
the request fails. A successful response MAY replace the placeholder only for
the matching active peer and image row. The UI SHALL keep thumbnail bytes only
in memory for the lifetime of the rendered card, revoke any Object URL when
the card/page/peer is replaced or unmounted, and SHALL NOT fetch a thumbnail
through a remote URL or the local asset resolver. Thumbnail failure SHALL NOT
disable the explicit `Importar` action or surface as a global rail error.

#### Scenario: Thumbnail response arrives after peer or row changes

- **WHEN** the selected peer, visible page, or card changes before a thumbnail
  response arrives
- **THEN** the stale response is discarded and cannot replace the placeholder
  or thumbnail of another row
- **AND** any allocated Object URL is released

#### Scenario: Thumbnail request fails or returns invalid bytes

- **WHEN** the host reports unavailable/busy or the client rejects malformed,
  oversized, or invalid PNG thumbnail bytes
- **THEN** the common static placeholder remains visible
- **AND** the history rail remains usable and the explicit import path remains
  independent

#### Scenario: User imports after viewing a thumbnail

- **WHEN** the user activates `Importar` on the same remote image
- **THEN** ClipVault fetches and validates the original PNG through the
  existing image-import route, not the thumbnail response
- **AND** only the existing successful import transaction changes local
  history, collection membership and provenance
- **AND** clipboard and paste state remain unchanged
