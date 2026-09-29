## MODIFIED Requirements

### Requirement: Browse rows carry a validated source-app display name only

The host SHALL include an optional trimmed source-app display name of at most
128 Unicode scalar values and no control characters in text/image history
browse rows when the effective source presentation has a valid name and the
requesting trusted peer advertises `source_app_presentation`. The effective
source presentation SHALL prefer valid `clipboard_entries.source_app*`
metadata; when it is absent, the host SHALL use the most recent metadata-bearing
`remote_imports` provenance for that same local entry. Rows SHALL NOT contain
icon bytes, icon references/paths, bundle IDs, raw source identifiers, content
hashes, clipboard content or the peer identifier recorded in provenance.
Image-thumbnail responses SHALL remain free of source-app metadata. The client
SHALL display the optional name only when the selected peer advertises
`source_app_presentation`; it SHALL make no separate per-card request and
SHALL never request, receive or render a source-app icon for remote previews.

#### Scenario: Source name arrives with browse results

- **WHEN** the host has a valid source-app name in the local entry or its
  selected import provenance and the peer advertises
  `source_app_presentation`
- **THEN** the name is included in the corresponding text or image browse row
- **AND** the card displays it as soon as that page renders, without a second
  network request or icon transfer

#### Scenario: Local capture metadata takes precedence

- **WHEN** a local entry has valid source-app metadata and also has imported
  provenance for the same content
- **THEN** the host uses the local entry's source-app metadata for peer history
- **AND** the entry's local metadata is not changed

#### Scenario: Imported attribution is forwarded without peer identity

- **WHEN** a local entry has no source-app metadata and has one or more
  metadata-bearing import provenance rows
- **THEN** the host uses the most recent deterministic provenance for its
  source-app name
- **AND** browse rows contain no peer identifier, icon bytes/reference/path
  or other application identifier

#### Scenario: Older peer or unavailable name

- **WHEN** a legacy peer omits the optional field, the peer lacks the
  capability, or neither the local entry nor its import provenance has a valid
  source-app name
- **THEN** the new client keeps the history row and shows an honest
  unknown-application fallback without delaying the card

#### Scenario: Browse metadata remains bounded and source-local

- **WHEN** a history page is requested
- **THEN** source-app names are validated and taken only from that entry's own
  metadata or its own imported provenance
- **AND** the page resolves imported names in a bounded batch and includes no
  icon bytes/reference/path, peer identifier, content hash or clipboard
  content; thumbnail responses include no source-app fields

### Requirement: Explicit import persists source presentation per provenance

Successful explicit text and image import responses MAY contain the same
validated optional display name and PNG icon bytes. The host SHALL resolve the
effective source presentation by preferring valid local entry metadata and
falling back to the most recent metadata-bearing `remote_imports` provenance
for that entry. It SHALL NOT transmit the source peer identifier, a
peer-supplied path/reference, bundle ID or raw source identifier. The receiving
client SHALL store valid icon bytes under a locally generated
content-addressed reference inside `application-icons/` and persist that local
reference and name only on the matching `remote_imports` provenance. It SHALL
NOT accept a peer-supplied path/reference or overwrite
`clipboard_entries.source_app*`. Icon staging and the provenance transaction
SHALL preserve reused/shared icons and SHALL release only newly written,
unreferenced icons on rollback.

#### Scenario: Explicit import has valid source presentation

- **WHEN** the user imports a text or image row and the host has valid source
  metadata locally or in that entry's import provenance
- **THEN** the import response carries the optional validated name and bounded
  PNG bytes in addition to its existing content payload
- **AND** the local icon is stored in `application-icons/` and linked to that
  new peer's provenance row
- **AND** the collection bound to that peer displays the original app name and
  local icon

#### Scenario: Multi-hop import preserves original application attribution

- **GIVEN** machine A imported a capture from machine B with a valid source-app
  name and icon
- **WHEN** machine C explicitly imports that capture from machine A
- **THEN** C records the same source-app name and icon on its provenance for A
- **AND** the displayed attribution does not become machine A or unknown
- **AND** neither B's peer identifier nor A's local icon reference is sent

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
