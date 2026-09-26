## ADDED Requirements

### Requirement: Source-app presentation capability is additive

The runtime SHALL advertise support for source-app presentation through a new
additive `caps_extra_v2` discovery TXT field and SHALL preserve the legacy
`capability=pairing` value and existing `caps_extra` tokens. New clients
SHALL combine recognized tokens from both additive fields. Older clients that
ignore the unknown `caps_extra_v2` key SHALL remain able to use existing
pairing and image capabilities. A new client SHALL display a source-app name
from a browse row only when the selected peer advertises
`source_app_presentation`. Peers that do not advertise it SHALL remain usable
for existing browsing and imports.

#### Scenario: New peer advertises source-app presentation

- **WHEN** a peer supports source-app names in browse rows
- **THEN** its discovery record keeps `capability=pairing`, preserves the
  existing `caps_extra` value and includes `source_app_presentation` in
  `caps_extra_v2`
- **AND** the persisted peer snapshot exposes that additive capability

#### Scenario: Legacy peer does not advertise the capability

- **WHEN** a trusted peer has no `source_app_presentation` token
- **THEN** the client ignores any optional source-app name in browse rows
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

### Requirement: Browse rows carry a validated source-app display name only

The host SHALL include an optional trimmed source-app display name of at most
128 Unicode scalar values and no control characters in text/image history
browse rows when the source entry has valid metadata and the requesting trusted
peer advertises `source_app_presentation`. Rows SHALL NOT contain
icon bytes, icon references/paths, bundle IDs, raw source identifiers, content
hashes or clipboard content. Image-thumbnail responses SHALL remain free of
source-app metadata. The client SHALL display the optional name only when the
selected peer advertises `source_app_presentation`; it SHALL make no separate
per-card request and SHALL never request, receive or render a source-app icon
for remote previews.

#### Scenario: Source name arrives with browse results

- **WHEN** the host has a valid source-app name and the peer advertises
  `source_app_presentation`
- **THEN** the name is included in the corresponding text or image browse row
- **AND** the card displays it as soon as that page renders, without a second
  network request or icon transfer

#### Scenario: Older peer or unavailable name

- **WHEN** a legacy peer omits the optional field, the peer lacks the
  capability, or the host entry has no valid source-app name
- **THEN** the new client keeps the history row and shows an honest
  unknown-application fallback without delaying the card

#### Scenario: Browse metadata remains bounded and source-local

- **WHEN** a history page is requested
- **THEN** a source-app name is validated and taken only from that entry's
  own metadata, never another peer's `remote_imports` provenance
- **AND** the row includes no icon bytes/reference/path or other application
  identifier, and thumbnail responses include no source-app fields

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
- **AND** general history uses the most recently imported provenance for the
  entry without displaying a peer identifier

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

#### Scenario: Re-import enriches legacy provenance without replacing attribution

- **WHEN** a peer re-sends a successful import for an existing provenance row
  whose source-app name or icon reference is `NULL`
- **THEN** the import fills only those missing fields from the validated
  response
- **AND** any source-app value already recorded for that provenance remains
  unchanged
