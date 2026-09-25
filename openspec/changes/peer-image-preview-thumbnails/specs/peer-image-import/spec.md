## MODIFIED Requirements

### Requirement: Remote image metadata is exposed only by eligible active peers

An active trusted peer SHALL expose transferable image rows only when the
image entry and its local PNG asset pass the host's existing asset, dimension
and size validation. The row SHALL contain only an opaque remote reference,
validated optional title, type, date, byte size and dimensions. It SHALL NOT
contain image bytes, thumbnails, asset references, filesystem paths, hashes,
tags, collections, favorites or source-application metadata. A separately
requested, bounded derived thumbnail MAY be returned only under the
`peer-image-preview-thumbnails` requirements; it is not part of the row or the
image-import response.

#### Scenario: Valid image is listed

- **WHEN** a trusted active peer has a persisted image whose asset is valid
- **THEN** the remote page includes at most one metadata-only image row
- **AND** the client initially renders the common static placeholder and may
  request a separate thumbnail only when the card is visible and the peer
  supports that capability

#### Scenario: Image asset is missing or invalid

- **WHEN** a remote image row has a missing, malformed, oversized or
  out-of-namespace asset
- **THEN** the host omits it from the transferable image set and returns no
  asset bytes or local path

#### Scenario: Peer is not eligible

- **WHEN** the caller is not trusted and active, is revoked or blocked, or
  does not advertise `image_import`
- **THEN** the image listing is denied with a typed safe outcome and no rows
