## Purpose

Definir la captura local de texto del portapapeles, su persistencia, metadata y prevención de duplicados para el MVP.

## Requirements

### Requirement: Capture text clipboard changes

The existing requirement is modified so a non-empty textual clipboard with
HTML or RTF SHALL be captured as `RichText` by the shared clipboard pipeline.
Plain text SHALL remain the compatibility representation and the existing
privacy, source-application, deduplication and non-fatal error contracts
MUST continue to apply.

#### Scenario: New rich text is copied

- **WHEN** a user copies formatted text from an application that is not
  ignored and the clipboard exposes HTML or RTF
- **THEN** ClipVault stores one rich-text history entry with plain text in
  `content` and validated rich metadata/assets

#### Scenario: Rich text is copied from an ignored application

- **WHEN** the source application matches the ignored-application blacklist
- **THEN** ClipVault persists neither plain nor rich payload data and creates
  no rich asset

#### Scenario: Rich support is unavailable

- **WHEN** a platform exposes only plain text or cannot read its rich flavor
- **THEN** ClipVault stores a normal `Text` entry and keeps the existing text
  capture path usable

#### Scenario: Existing capture contracts remain

- **WHEN** the capture is rich, plain, image, unsupported, duplicated or has
  an unavailable source application
- **THEN** the existing non-crashing, metadata-only diagnostics,
  history-updated, blacklist and source-app contracts continue to hold

#### Scenario: New text is copied

- **WHEN** a user copies text from an application that is not ignored
- **THEN** ClipVault stores the text in the local history with its existing
  timestamp, type detection and deduplication behavior

#### Scenario: Supported image is copied without text

- **WHEN** a user copies a supported raster image and no non-empty text is
  available
- **THEN** ClipVault stores an image history entry through the
  `clipboard-rich-content` asset pipeline

#### Scenario: Unsupported non-text content is copied

- **WHEN** a user copies a file, audio, video, RTF-only or unsupported binary
  representation without supported text or image data
- **THEN** ClipVault creates no history entry and continues running

#### Scenario: Clipboard event is blacklisted

- **WHEN** the source application identifier matches an ignored application
- **THEN** ClipVault persists neither text nor image data, creates no asset and
  does not expose the payload through history or search

#### Scenario: Source application is unavailable

- **WHEN** the operating system does not provide a source application identifier
- **THEN** ClipVault treats the event as unclassified and the matcher's "unknown source" contract keeps the capture accessible to the persistence path; the failure surface is recorded through the metadata-only diagnostics endpoint instead of forcing a fall-back to Discard

#### Scenario: Clipboard read fails

- **WHEN** the platform clipboard cannot be read or returns malformed data
- **THEN** ClipVault records a local diagnostic event without crashing or corrupting existing history

#### Scenario: Background loop and manual tick share the same dedupe state

- **WHEN** the background capture loop has already evaluated a clipboard change (including events the `PrivacyGate` discards) and the user later forces a watcher tick
- **THEN** the manual tick observes the loop's last seen hash, returns the unchanged outcome and never re-evaluates the discarded payload against the blacklist with a stale source-application snapshot taken after the user clicked the button

#### Scenario: Blacklisted capture must not leak via Tick capture

- **WHEN** a blacklisted application copies text to the clipboard while it is focused
- **THEN** the background loop alone decides whether to persist the event; even if the user switches to ClipVault and forces a tick, the watcher keeps the discard decision, never invokes the persistence path, and the SQLite history remains untouched for that payload

#### Scenario: Source-app snapshot must come from the original capture

- **WHEN** the `PrivacyGate` evaluates an event
- **THEN** it relies on the source-application snapshot/cached probe that was associated with the capture (refreshed by the platform thread) and never falls back to inventing a source identifier simply because the user interface regained focus later in the flow

#### Scenario: Capture loop refreshes the active-app cache on the main thread

- **WHEN** the background capture loop iterates over the watcher
- **THEN** it schedules a synchronous refresh of the cached active-application probe on the Tauri main thread before reading the clipboard, so the matcher observes an identifier that is at most one tick stale; the previous fire-and-forget background refresh thread is removed and the only background refresh path is this synchronous one

#### Scenario: Refresh failure is surfaced, not swallowed

- **WHEN** the synchronous refresh fails (Tauri refuses to schedule, the main thread does not pick it up inside the timeout, or the inner probe returns a backend error)
- **THEN** the diagnostics endpoint reports `refresh_outcome = "failed"` with the actionable reason, the capture loop continues running, and the matcher keeps evaluating with the last known identifier (or the "unknown source" allow rule when the cache is still empty)

#### Scenario: Allowed capture stores source application presentation metadata

- **WHEN** a permitted text capture has an available source application identifier
- **THEN** the capture pipeline may enrich the entry with the source application's display name and a controlled icon reference for the history card, while preserving the existing text, hash, source identifier and content-type behavior

#### Scenario: Application metadata failure does not block text capture

- **WHEN** the source application's name or icon cannot be resolved or stored
- **THEN** ClipVault persists the valid text capture with nullable metadata and the frontend uses a generic fallback

#### Scenario: Blacklisted capture does not create application metadata

- **WHEN** the PrivacyGate rejects a text capture from an ignored application
- **THEN** ClipVault does not persist the payload or create a new source-application icon asset as a side effect

### Requirement: Persist clipboard entry metadata

Each text history entry SHALL persist an identifier, content, content type `text`, creation and update timestamps, source application when available, content size and a deterministic content hash.

#### Scenario: Text entry is persisted

- **WHEN** a valid text clipboard event is captured
- **THEN** the entry and its metadata are committed to SQLite in a transaction

#### Scenario: Source application is unavailable

- **WHEN** the operating system does not provide the source application
- **THEN** ClipVault stores a null or explicit unknown value and still persists the text entry

### Requirement: Prevent duplicate history entries

ClipVault SHALL use the content hash to prevent identical consecutive or repeated text content from creating redundant history rows.

#### Scenario: Identical content is copied again

- **WHEN** text with a hash already present in history is copied again
- **THEN** ClipVault does not create a second identical row and updates the existing entry's latest-seen metadata as defined by the data model

#### Scenario: Different content is copied

- **WHEN** the clipboard content differs and produces a different hash
- **THEN** ClipVault creates a new history entry without modifying the prior entry's content

### Requirement: Maintain history across restarts

Captured text SHALL remain available after the application restarts until it is explicitly deleted, removed by the configured expiration policy, or pruned by an enforced retention sweep.

#### Scenario: History is reopened

- **WHEN** the application starts after previously capturing text
- **THEN** the persisted entries are available to the search and quick-paste workflows

#### Scenario: Retention policy expires entries

- **WHEN** the retention policy is set to a bounded value (e.g. 7, 30 or 90 days) and the application starts
- **THEN** ClipVault removes or marks as expired any entry whose capture timestamp is older than the configured horizon, except entries that the user marked as favorite

#### Scenario: Retention sweep runs while running

- **WHEN** the retention sweep executes while the application is running
- **THEN** ClipVault deletes (or marks expired) the same set of entries it would on startup, keeps favorites regardless of age and records a local diagnostic event describing how many entries were affected

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
