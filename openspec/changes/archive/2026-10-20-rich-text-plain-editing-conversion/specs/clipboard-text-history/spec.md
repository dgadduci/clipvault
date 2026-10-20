## MODIFIED Requirements

### Requirement: Edit an eligible textual history entry

ClipVault SHALL allow a user to replace the content of an eligible textual
history entry in place. An eligible plain-text entry SHALL have a textual
`content_type`, no image asset metadata and no rich-text payload references.
A rich-text entry SHALL be editable only through an explicit conversion flow:
the user must confirm that the edit will remove rich formatting, change the
canonical plain text, and activate Guardar. Editing SHALL preserve the entry
identifier and all organization and presentation metadata unrelated to the
text.

#### Scenario: Save edited text

- **WHEN** the user changes an eligible plain-text entry and activates Guardar
- **THEN** ClipVault persists the new text in the same SQLite row and keeps the
  same entry ID
- **AND** it recomputes `content_size`, `content_hash` and `content_type` using
  the same deterministic rules as capture
- **AND** it updates `updated_at` without changing `created_at` or
  `last_seen_at`

#### Scenario: Rich entry requests explicit conversion

- **WHEN** the user selects `Editar captura` for an entry with rich-text
  payload references
- **THEN** ClipVault warns that saving a modified draft will convert the
  capture to plain text and remove its rich representations
- **AND** confirming the warning opens the existing text editor with the
  entry's canonical plain text and resolved title
- **AND** neither opening the warning nor confirming it changes the entry

#### Scenario: Cancel rich conversion before saving

- **WHEN** the user cancels the warning or closes the editor without saving
- **THEN** the rich entry, its metadata and its rich asset references remain
  unchanged
- **AND** no conversion command is committed

#### Scenario: Save changed text and convert rich entry

- **WHEN** the user confirms the warning, changes the canonical plain text to
  a non-empty different value and activates Guardar
- **THEN** ClipVault converts the same SQLite row to a plain-text entry
- **AND** it recomputes `content_size`, `content_hash` and `content_type` using
  the same deterministic rules as capture
- **AND** it clears the row's rich-text hash, representation references,
  preview reference and rich size metadata
- **AND** it updates `updated_at` without changing `created_at` or
  `last_seen_at`

#### Scenario: Unchanged draft cannot convert a rich entry

- **WHEN** the user confirms the warning but leaves the canonical plain text
  unchanged, including after reverting edits to the original text
- **THEN** Guardar remains disabled and no backend mutation is requested
- **AND** the entry remains rich with all persisted metadata and assets intact

#### Scenario: Text edit preserves metadata

- **WHEN** an entry with a title, favorite state, tags, collections,
  source-app metadata or local asset references is edited
- **THEN** those fields remain unchanged unless they are rich-text references
  cleared by a successful conversion
- **AND** the edit does not create a second entry or membership

#### Scenario: Empty content is rejected

- **WHEN** the user attempts to save an empty string
- **THEN** ClipVault returns `empty_content`
- **AND** the previous row remains unchanged

#### Scenario: Editing to the same text is idempotent

- **WHEN** the submitted text for an eligible plain-text entry is byte-for-byte
  equal to the persisted content
- **THEN** ClipVault returns a `noop` result
- **AND** it does not create a new row or emit a content mutation event

#### Scenario: Rich conversion rejects unchanged text at the backend

- **WHEN** a conversion request reaches the backend with text byte-for-byte
  equal to the rich entry's canonical plain text
- **THEN** ClipVault returns a `noop` result and leaves the row rich and
  unchanged
- **AND** no history update event is emitted

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

- **WHEN** the plain-text edit operation targets an image entry or a rich
  entry without an explicit conversion request
- **THEN** ClipVault returns `not_editable`
- **AND** it does not change content, metadata or assets

### Requirement: Persist and expose text-edit results safely

The text-edit and rich-to-plain conversion mutations SHALL run in one local
SQLite transaction and SHALL be available through thin Tauri commands and
typed TypeScript bridges. A command MAY accept the user-entered draft because
editing requires it, but the draft SHALL NOT be written to logs, diagnostics,
drag payloads or event payloads. Rich conversion SHALL validate the current
row and its rich references in the backend rather than trusting frontend
eligibility state.

#### Scenario: Successful edit refreshes consumers

- **WHEN** a plain-text edit or rich-to-plain conversion commits successfully
- **THEN** the command returns a typed updated result and the existing
  `clipvault://history-updated` event is emitted with an empty payload or an
  equivalent metadata-only signal
- **AND** the card, history, search and Quick Paste use the persisted text
- **AND** the converted entry is available only as plain text through its
  capture actions

#### Scenario: Failed edit is atomic

- **WHEN** validation, duplicate detection or SQLite persistence fails
- **THEN** the transaction rolls back and the entry's prior content, metadata
  and rich references remain intact
- **AND** the frontend keeps the draft visible with a safe error
- **AND** the previous persisted entry remains available

#### Scenario: Rich assets are not removed while shared

- **WHEN** a successful conversion clears rich references from one entry and
  an asset is still referenced by another entry
- **THEN** the shared asset remains available to the other entry
- **AND** an asset is collected only after commit and after the repository
  confirms that no entry references it

#### Scenario: Edit survives restart

- **WHEN** the user saves a valid edit or rich conversion and restarts ClipVault
- **THEN** the new text, hash, size and type are loaded from SQLite
- **AND** the entry remains available to history, search and Quick Paste
- **AND** a converted entry has no rich-text representation available
