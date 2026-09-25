## ADDED Requirements

### Requirement: Source-app presentation capability is additive

The runtime SHALL advertise support for source-app presentation through a new
additive `caps_extra_v2` discovery TXT field and SHALL preserve the legacy
`capability=pairing` value and existing `caps_extra` tokens. New clients
SHALL combine recognized tokens from both additive fields. Older clients that
ignore the unknown `caps_extra_v2` key SHALL remain able to use existing
pairing and image capabilities. A client SHALL use the source-presentation
route only when the selected peer advertises `source_app_presentation`.
Peers that do not advertise it SHALL remain usable for existing browsing and
imports.

#### Scenario: New peer advertises source-app presentation

- **WHEN** a peer supports the source-app presentation route
- **THEN** its discovery record keeps `capability=pairing`, preserves the
  existing `caps_extra` value and includes `source_app_presentation` in
  `caps_extra_v2`
- **AND** the persisted peer snapshot exposes that additive capability

#### Scenario: Legacy peer does not advertise the capability

- **WHEN** a trusted peer has no `source_app_presentation` token
- **THEN** the client does not request source-app presentation
- **AND** remote previews and explicit imports retain generic/unknown source
  fallbacks without failing

#### Scenario: Older client encounters the new TXT field

- **WHEN** a client understands `caps_extra` but predates
  `caps_extra_v2` and encounters an advertisement containing the new TXT key
- **THEN** it ignores only that unknown key and continues to recognize the
  peer using its existing legacy capability fields

#### Scenario: New client receives an unknown additive token

- **WHEN** discovery receives an unrecognized token in a capability field it
  understands
- **THEN** it follows the existing unsupported-capability policy and does not
  silently grant the source-app presentation route

### Requirement: Visible preview source presentation is separately requested

The host SHALL expose a dedicated authenticated operation that resolves
source-app presentation for one opaque remote entry ID. The request SHALL be
made only for a visible card in the selected peer's remote rail. Browse rows
and image-thumbnail responses SHALL remain free of source-app names,
identifiers, icon references, paths and icon bytes. The route SHALL return
only a validated optional display name and optional PNG bytes; it SHALL never
return the host's local icon reference/path, bundle ID, raw source identifier,
content hash, clipboard payload or unrelated application metadata. The host
SHALL resolve the name and icon only from metadata attached to the listed
entry itself; it SHALL NOT borrow attribution from a `remote_imports` row
belonging to another peer.

#### Scenario: Visible text or image card requests presentation

- **WHEN** a text or image preview card becomes visible and its peer advertises
  `source_app_presentation`
- **THEN** the client requests source presentation for that row's opaque
  `remote_entry_id` through the dedicated route
- **AND** the host revalidates the pinned mTLS identity, trusted/active peer
  state, row eligibility and source-icon namespace before responding
- **AND** the response contains only the optional normalized display name
  and optional bounded PNG bytes

#### Scenario: Card is offscreen or peer lacks the capability

- **WHEN** a card is not visible, or its peer does not advertise
  `source_app_presentation`
- **THEN** the client sends no source-presentation request for that card
- **AND** it renders a generic app icon and an honest unknown-name fallback
  when no prior in-memory result exists

#### Scenario: Source metadata is requested without exposing it in listings

- **WHEN** a client browses history or fetches an image thumbnail
- **THEN** neither response includes source-app name, identifier, icon
  reference, path or icon bytes
- **AND** those fields are available only from the separate visible-card
  route or an explicit import response

#### Scenario: Peer authorization or row eligibility changes

- **WHEN** the caller is revoked, blocked, no longer trusted/active, has a
  mismatched pin, or the row is no longer eligible
- **THEN** the host returns a typed safe unavailable result with no app
  presentation data

#### Scenario: Listed entry has only another peer's import attribution

- **WHEN** the listed entry lacks its own source-app metadata but has a
  `remote_imports` provenance row for a different peer
- **THEN** the host does not relay that row's name or icon to the requester
- **AND** the remote preview uses the generic/unknown fallback

### Requirement: Source-app presentation is bounded and validated

The host and client SHALL trim and validate display names to at most 128
Unicode scalar values with no control characters. Application icon payloads
SHALL be PNG, successfully decoded, at most 512 KiB and no larger than 256 ×
256 pixels. Icon bytes SHALL be base64 encoded only inside a response whose
serialized envelope is capped at 720 KiB. A missing or invalid icon SHALL be
omitted without failing an otherwise valid name lookup. The client and host
SHALL each allow no more than two concurrent presentation requests per peer.

#### Scenario: Valid source name and icon fit the wire limit

- **WHEN** a source name reaches its allowed boundary and the PNG reaches the
  512 KiB / 256 × 256 px limits
- **THEN** the response remains within the 720 KiB envelope cap and both
  values are available to the caller

#### Scenario: Malformed or oversized icon is encountered

- **WHEN** icon bytes have an invalid signature, fail decoding, exceed the
  byte limit or exceed either dimension
- **THEN** those bytes are discarded and no remote/local path or reference is
  returned
- **AND** a valid source name may still be returned

#### Scenario: Invalid source name is encountered

- **WHEN** the source name is empty after trimming, contains control
  characters or exceeds 128 Unicode scalar values
- **THEN** the response omits the name without logging it or failing the
  presentation request

#### Scenario: Per-peer concurrency limit is reached

- **WHEN** a peer already has two presentation requests in flight on the
  client or host
- **THEN** additional work is rejected or safely deferred without exceeding
  the per-peer limit or affecting other peers

### Requirement: Remote preview source icons remain transient

The client SHALL render validated source-app presentation in the corresponding
remote card without persisting preview-only icon bytes or passing any remote
reference/path to the local application-icon resolver. Presentation state
SHALL be scoped to peer and remote entry, stale responses SHALL be ignored,
and generated object URLs SHALL be released on replacement, peer switch or
component destruction. The in-memory cache SHALL retain at most the current
combined page's 50 results per peer and evict results when rows leave that
window or the peer is closed. Missing presentation SHALL use a generic local
app icon and a truthful unknown-name label without changing card geometry.

#### Scenario: Presentation response arrives for the active card

- **WHEN** a valid response for the currently visible peer and entry arrives
- **THEN** the card displays its source-app icon and bounded name within the
  fixed card footprint
- **AND** the remote bytes are held only in bounded in-memory UI state

#### Scenario: Stale response arrives after a peer or row switch

- **WHEN** a response arrives after its peer/card was replaced or unmounted
- **THEN** it cannot update another peer's card and any temporary object URL
  is released

#### Scenario: Icon is missing while the name is present

- **WHEN** a valid source name arrives without valid icon bytes
- **THEN** the card shows the name with a generic local application icon
- **AND** the card remains the same fixed size

### Requirement: Explicit import persists source presentation per provenance

Successful explicit text and image import responses MAY contain the same
validated optional display name and PNG icon bytes. The receiving client SHALL
store valid icon bytes under a locally generated content-addressed reference
inside `application-icons/` and persist that local reference and name only on
the matching `remote_imports` provenance. It SHALL NOT accept a peer-supplied
path/reference or overwrite `clipboard_entries.source_app*`. Icon staging and
the provenance transaction SHALL preserve reused/shared icons and SHALL
release only newly written, unreferenced icons on rollback.

#### Scenario: Explicit import has valid source presentation

- **WHEN** the user imports a text or image row and the host provides valid
  source metadata
- **THEN** the import response carries the optional name and bounded PNG
  bytes in addition to its existing content payload
- **AND** the local icon is stored in `application-icons/` and linked to that
  peer's provenance row
- **AND** the collection bound to that peer displays the original app name
  and local icon

#### Scenario: Content is deduplicated against a local entry

- **WHEN** import reuses an existing local clipboard entry
- **THEN** the source name and icon reference are recorded only on the
  matching peer provenance
- **AND** the entry's existing local source name and icon reference remain
  unchanged

#### Scenario: The same entry is imported from different peers

- **WHEN** two peers provide different source-app presentation for the same
  canonical clipboard entry
- **THEN** each peer-bound collection uses only its own provenance name and
  icon reference
- **AND** general history does not display either peer's attribution

#### Scenario: Import fails while a new icon is staged

- **WHEN** content/provenance commit fails after a new icon was staged
- **THEN** the new icon is released only if it has no committed references
- **AND** an icon reused by another provenance or local capture is preserved

#### Scenario: Older peer or invalid icon is imported

- **WHEN** a peer does not send optional source fields or the icon fails local
  validation
- **THEN** the content import still succeeds when its content is valid
- **AND** the peer collection uses a generic import icon and an honest
  unknown-name fallback when no valid name is available
