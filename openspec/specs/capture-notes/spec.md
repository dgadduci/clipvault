# capture-notes Specification

## Purpose
Define local multiline notes for history entries without changing the
captured content or sharing note bodies with other devices.

## Requirements

### Requirement: Persist one local multiline note per capture

ClipVault SHALL allow one optional multiline plain-text note per persisted
text, rich-text or image history entry and store each note separately. Saving
or removing it MUST NOT modify the captured payload, hash, type, timestamps,
title, source metadata, favorite, tags, collection memberships or image assets.
Notes MUST stay local and MUST NOT enter search, logs, diagnostics, refresh
events, drag payloads or peer transfers. Entry lists SHALL expose only note
presence; load the body when opening the editor.

#### Scenario: Add a note to a text capture

- **WHEN** the user adds and saves a multiline note for a text capture
- **THEN** ClipVault persists the note separately from the capture content
- **AND** the card reports that a note exists without carrying the note body
  in its list projection

#### Scenario: Add a note to an image capture

- **WHEN** the user adds and saves a note for an image capture
- **THEN** ClipVault persists the note without modifying the image payload,
  asset reference, dimensions or hash

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

#### Scenario: Capture note survives restart

- **WHEN** a note has been saved for an entry
- **AND** ClipVault restarts
- **THEN** the note remains available from that entry's card
