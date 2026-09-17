# Delta: edición persistente de capturas textuales

## ADDED Requirements

### Requirement: Edit an eligible textual history entry

ClipVault SHALL allow a user to replace the content of an eligible textual
history entry in place. An eligible entry SHALL have a textual
`content_type`, no image asset metadata and no rich-text payload references.
The edit SHALL preserve the entry identifier and all organization and
presentation metadata that is unrelated to the text.

#### Scenario: Save edited text

- **WHEN** the user changes an eligible entry and activates Guardar
- **THEN** ClipVault persists the new text in the same SQLite row and keeps the
  same entry ID
- **AND** it recomputes `content_size`, `content_hash` and `content_type` using
  the same deterministic rules as capture
- **AND** it updates `updated_at` without changing `created_at` or
  `last_seen_at`

#### Scenario: Text edit preserves metadata

- **WHEN** an entry with a title, favorite state, tags, collections,
  source-app metadata or local asset references is edited
- **THEN** those fields remain unchanged
- **AND** the edit does not create a second entry, asset or membership

#### Scenario: Empty content is rejected

- **WHEN** the user attempts to save an empty string
- **THEN** ClipVault returns `empty_content`
- **AND** the previous row remains unchanged

#### Scenario: Editing to the same text is idempotent

- **WHEN** the submitted text is byte-for-byte equal to the persisted content
- **THEN** ClipVault returns a `noop` result
- **AND** it does not create a new row or emit a content mutation event

#### Scenario: Edited text conflicts with another entry

- **WHEN** the new text has a canonical hash already owned by another history
  entry
- **THEN** ClipVault returns `duplicate_content`
- **AND** it does not merge, delete or rewrite either entry

#### Scenario: Entry type is reclassified

- **WHEN** edited text changes its deterministic classification, such as from
  plain text to JSON or URL
- **THEN** the stored `content_type` becomes the newly detected textual type
- **AND** the same entry remains searchable through the textual history path

#### Scenario: Entry does not exist

- **WHEN** an edit targets a missing entry ID
- **THEN** ClipVault returns `not_found`
- **AND** no other row is modified

#### Scenario: Non-textual or rich entry is protected

- **WHEN** an edit targets an image or an entry with rich-text payload
  references
- **THEN** ClipVault returns `not_editable`
- **AND** it does not change content, metadata or assets

### Requirement: Persist and expose text-edit results safely

The text-edit mutation SHALL run in one local SQLite transaction and SHALL be
available through a thin Tauri command and a typed TypeScript bridge. The
command MAY accept the user-entered draft because editing requires it, but the
draft SHALL NOT be written to logs, diagnostics, drag payloads or event
payloads.

#### Scenario: Successful edit refreshes consumers

- **WHEN** an edit commits successfully
- **THEN** the command returns a typed updated result and the existing
  `clipvault://history-updated` event is emitted with an empty payload or an
  equivalent metadata-only signal
- **AND** the card, history, search and Quick Paste use the persisted text

#### Scenario: Failed edit is atomic

- **WHEN** validation, duplicate detection or SQLite persistence fails
- **THEN** the transaction rolls back
- **AND** the frontend keeps the draft visible with a safe error
- **AND** the previous persisted entry remains available

#### Scenario: Edit survives restart

- **WHEN** the user saves a valid edit and restarts ClipVault
- **THEN** the new text, hash, size and type are loaded from SQLite
- **AND** the entry remains available to history, search and Quick Paste
