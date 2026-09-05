## ADDED Requirements

### Requirement: Tauri command surface for history management

ClipVault SHALL expose thin Tauri commands for history management that
delegate to the Rust core, return serializable results, and never accept
clipboard content as input. The shell SHALL NOT persist, log or echo the
content of any history entry through these commands.

#### Scenario: Set favorite through Tauri

- **WHEN** the frontend invokes `clipvault_set_favorite` with a numeric entry
  identifier and a boolean value
- **THEN** the core updates the `is_pinned` flag of that entry, returns the
  updated entry summary without its full content, and the command completes
  even if the entry was already in the requested state

#### Scenario: Delete a single entry through Tauri

- **WHEN** the frontend invokes `clipvault_delete_entry` with a numeric entry
  identifier
- **THEN** the core removes the entry, the search index reflects the removal
  on the next query, and calling the command again with the same identifier
  completes without error and without affecting other entries

#### Scenario: Clear non-favorite history through Tauri

- **WHEN** the frontend invokes `clipvault_clear_history`
- **THEN** the core removes every non-favorite entry in a single transaction
  while leaving favorite entries intact, and the response reports the number
  of removed entries

#### Scenario: Apply retention policy through Tauri

- **WHEN** the frontend or core schedules `clipvault_apply_retention`
- **THEN** the core reads the configured retention period, removes only
  non-favorite entries older than that period, and returns the number of
  removed entries; favorite entries SHALL NOT be removed

### Requirement: Confirmation for destructive actions

ClipVault SHALL require an explicit confirmation in the user interface before
performing a destructive action such as deleting a single entry or clearing
non-favorite history. The core SHALL accept a confirmation flag on the
destructive commands so the shell cannot trigger them by accident.

#### Scenario: Delete without confirmation

- **WHEN** the frontend invokes `clipvault_delete_entry` without
  `confirm: true`
- **THEN** the core rejects the request locally without modifying the
  database and returns a `confirmation_required` outcome

#### Scenario: Clear history without confirmation

- **WHEN** the frontend invokes `clipvault_clear_history` without
  `confirm: true`
- **THEN** the core rejects the request locally, leaves the database
  untouched and returns a `confirmation_required` outcome

#### Scenario: Confirmed destructive action

- **WHEN** the user confirms the action in the UI and the frontend invokes
  the command with `confirm: true`
- **THEN** the core performs the mutation, returns the number of removed
  entries and never logs or returns the content of the removed entries

### Requirement: Idempotent favorite toggling

ClipVault SHALL treat favorite operations as idempotent: applying the same
target state to an entry multiple times SHALL leave the row in that state
and SHALL NOT create duplicate history rows or unintended side effects.

#### Scenario: Mark an already favorite entry

- **WHEN** the user marks as favorite an entry that is already favorite
- **THEN** the entry keeps `is_pinned = true`, the updated timestamp reflects
  the last change and no additional row is created

#### Scenario: Unmark a non-favorite entry

- **WHEN** the user unmarks a non-favorite entry
- **THEN** the entry keeps `is_pinned = false` and no error is raised

### Requirement: Retention policy is read from local configuration

ClipVault SHALL read the retention period from the local configuration
defined by the `privacy-settings` capability and SHALL NOT query a remote
service, environment variable outside the documented paths, or hard-coded
default at runtime when the user has set a preference.

#### Scenario: User selected 30 days

- **WHEN** the configured retention is 30 days and `clipvault_apply_retention`
  runs
- **THEN** non-favorite entries older than 30 days are removed and entries
  inside the window remain available

#### Scenario: User selected forever

- **WHEN** the configured retention is `forever`
- **THEN** `clipvault_apply_retention` removes no non-favorite entry
  automatically and returns `removed: 0`

#### Scenario: Missing configuration

- **WHEN** the configuration is missing the retention key on first start
- **THEN** the core applies the documented MVP default (30 days) and records
  the resolved value locally without prompting the user through a modal

### Requirement: Atomic history mutations in SQLite

ClipVault SHALL perform every history mutation (favorite toggle, single
delete, clear history, retention purge) inside a single SQLite transaction,
and SHALL roll back automatically on any failure inside that transaction.

#### Scenario: Successful mutation

- **WHEN** the core applies any history mutation
- **THEN** the change is committed atomically and a subsequent query observes
  the new state without observing a partial intermediate state

#### Scenario: Mutation fails halfway

- **WHEN** a history mutation fails after some statements executed
- **THEN** the transaction is rolled back, the database state is identical to
  the pre-call snapshot, the call returns a typed error and the search index
  is not updated for the failed operation

### Requirement: History operations do not leak clipboard content

ClipVault SHALL NOT include the content, hash, snippet, source application
identifier or any other clipboard payload in the responses, logs, events or
diagnostics produced by history management commands.

#### Scenario: Tauri response shape

- **WHEN** a history management command returns to the frontend
- **THEN** the response contains only identifiers, flags, counts, timestamps
  and metadata safe to expose, and never the entry content or a snippet
  longer than what the existing `clipboard-search` capability already
  documents

#### Scenario: Local diagnostic logs

- **WHEN** the core writes a diagnostic event for a history mutation
- **THEN** the log line contains the operation kind, the affected entry
  identifier and the result, and SHALL NOT contain the entry content, the
  content hash, the source application bundle identifier or any other
  clipboard-derived value
