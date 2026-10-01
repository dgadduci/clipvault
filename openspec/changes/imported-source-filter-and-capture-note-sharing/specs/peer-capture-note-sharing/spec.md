## ADDED Requirements

### Requirement: Share capture notes only through explicit owner consent

ClipVault SHALL include a capture note in a text or image fetch response only
when the sending device's persisted note-export preference is enabled and the
requesting peer advertises the additive `capture_note_sharing` capability.
The capability SHALL be advertised through a separate `caps_extra_v3` TXT
field so existing peers that strictly validate `caps_extra_v2` keep pairing.
It SHALL not replace pairing, image-import or source-app-presentation
capabilities. Browsing, search, remote
history rows, previews, thumbnails and diagnostics SHALL NOT return note
bodies. Omitting a note SHALL NOT prevent an otherwise valid capture import.

#### Scenario: Compatible peer imports a text capture with a note

- **GIVEN** the host's note-export preference is enabled
- **AND** the requesting peer advertises `capture_note_sharing`
- **WHEN** the peer explicitly imports a text capture with a saved note
- **THEN** the explicit fetch response carries the note separately from the
  text payload
- **AND** no history-browse or search response carries that note

#### Scenario: Compatible peer imports an image capture with a note

- **GIVEN** the host's note-export preference is enabled
- **AND** the requesting peer advertises `capture_note_sharing`
- **WHEN** the peer explicitly imports an image capture with a saved note
- **THEN** the explicit original-image fetch response carries the note
  separately from the image bytes
- **AND** thumbnail and browse responses carry no note

#### Scenario: Host has not opted into sharing notes

- **GIVEN** the host's note-export preference is disabled or absent
- **WHEN** a peer explicitly imports a capture with a saved note
- **THEN** the capture imports successfully without its note
- **AND** the saved note remains unchanged on the host

#### Scenario: Requesting peer does not support note transfer

- **GIVEN** the requesting peer does not advertise
  `capture_note_sharing`
- **WHEN** it imports a capture from a host with note export enabled
- **THEN** the host uses the legacy-compatible response without a note
- **AND** the capture import succeeds

### Requirement: Store an imported capture note without replacing local work

A receiving peer that supports `capture_note_sharing` SHALL validate and
persist an included note separately from the imported capture. Importing a
capture whose content deduplicates to an existing local entry SHALL NOT
overwrite an existing local note. If the existing entry has no note, the
received note MAY be attached. If the response has no note, the import SHALL
leave local note state unchanged. Note bodies SHALL remain absent from logs,
diagnostics, event payloads and collection-note records.

#### Scenario: New imported capture includes an opted-in note

- **WHEN** a valid capture and optional note are imported from a compatible
  peer that opted in
- **THEN** the capture is stored or deduplicated under the existing import
  rules and the note is stored separately when no local note exists
- **AND** note persistence does not alter the capture hash or payload

#### Scenario: Duplicate import preserves a local note

- **GIVEN** an identical local capture already has a note
- **WHEN** a peer reimports that capture with a different note
- **THEN** the local note remains unchanged
- **AND** the capture still follows existing deduplication and provenance
  behavior

#### Scenario: Duplicate import can fill a missing note

- **GIVEN** an identical local capture exists without a note
- **WHEN** a compatible peer imports it with an opted-in note
- **THEN** the received note is attached without creating a duplicate entry

#### Scenario: Invalid or oversized note does not discard the capture

- **WHEN** a peer response includes a note that fails validation or exceeds
  the transport's supported note-size bound
- **THEN** ClipVault omits the note, returns the normal capture-import result
  and does not log or expose the note body

#### Scenario: Older peer imports without note support

- **WHEN** a peer using the legacy protocol imports or serves a capture
- **THEN** the existing capture-transfer behavior remains available
- **AND** no note is required or exposed to that peer
