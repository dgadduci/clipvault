## MODIFIED Requirements

### Requirement: Persist one local multiline note per capture

ClipVault SHALL allow one optional multiline plain-text note per persisted
text, rich-text or image history entry and store each note separately. Saving
or removing a note MUST NOT modify the captured payload, hash, type, timestamps,
title, source metadata, favorite, tags, collection memberships or image assets.
Notes SHALL remain local and SHALL NOT enter search, logs, diagnostics, refresh
events or drag payloads. A capture note MAY leave the device only through the
explicit peer-transfer flow when the note-export preference on the sending
device is enabled and the receiving peer supports note transfer. Entry lists
SHALL expose only note presence; load the body when opening the editor.

#### Scenario: Add a note to a text capture

- **WHEN** the user adds and saves a multiline note for a text capture
- **THEN** ClipVault persists it separately from the capture
- **AND** the card reports note presence without carrying the note body in its
  list projection

#### Scenario: Add a note to an image capture

- **WHEN** the user adds and saves a note for an image capture
- **THEN** ClipVault persists it without modifying the image payload, asset
  reference, dimensions or hash

#### Scenario: Open a capture note from its card

- **GIVEN** a card represents an entry with a note
- **WHEN** the user activates the note icon
- **THEN** ClipVault opens the shared note modal with the persisted note
- **AND** the card does not activate paste or drag as a side effect

#### Scenario: Save an edited capture note

- **WHEN** the user edits a capture note and confirms
- **THEN** ClipVault updates only the note and its note-specific modification
  timestamp
- **AND** the card's note indicator reflects the successful result

#### Scenario: Remove a capture note

- **WHEN** the user saves an empty note
- **THEN** ClipVault removes the note association and the card no longer shows
  a note-present icon

#### Scenario: Cancel capture note editing

- **WHEN** the user cancels, presses Escape or closes the backdrop before
  saving
- **THEN** the persisted note remains unchanged

#### Scenario: Capture deletion removes its note

- **WHEN** an entry is deleted by an existing history operation
- **THEN** its local note is deleted atomically with the entry and no orphan
  note remains

#### Scenario: Capture note is not exported by default

- **GIVEN** the sending device has no enabled note-export preference
- **WHEN** a peer explicitly imports a capture that has a note
- **THEN** the host transfers the capture without the note
- **AND** the local note remains available on the sending device

#### Scenario: Capture note is exported by explicit preference

- **GIVEN** the sending device has enabled note export and the receiving peer
  supports note transfer
- **WHEN** that peer explicitly imports a capture that has a note
- **THEN** the transfer may carry the note under the peer capture-note-sharing
  capability
- **AND** the capture payload and note remain separate on both devices

#### Scenario: Capture note stays out of remote browsing

- **WHEN** a peer browses history, searches, or requests an image thumbnail
- **THEN** no capture-note body is included, regardless of the export setting

#### Scenario: Capture note survives restart

- **WHEN** a note has been saved for an entry and ClipVault restarts
- **THEN** the note remains available from that entry's card
