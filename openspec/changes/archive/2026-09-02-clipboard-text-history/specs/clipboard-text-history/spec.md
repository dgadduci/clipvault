## ADDED Requirements

### Requirement: Capture text clipboard changes

ClipVault SHALL capture new text content when the operating system clipboard changes and SHALL ignore non-text content in MVP v0.1 without terminating the application.

#### Scenario: New text is copied

- **WHEN** a user copies text from an application that is not ignored
- **THEN** ClipVault stores the text in the local history with its capture timestamp

#### Scenario: Non-text content is copied

- **WHEN** a user copies an image, file or unsupported clipboard representation
- **THEN** ClipVault does not create a text history entry and continues running

#### Scenario: Clipboard read fails

- **WHEN** the platform clipboard cannot be read or returns malformed data
- **THEN** ClipVault records a local diagnostic event without crashing or corrupting existing history

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

Captured text SHALL remain available after the application restarts until it is explicitly deleted or removed by the configured expiration policy.

#### Scenario: History is reopened

- **WHEN** the application starts after previously capturing text
- **THEN** the persisted entries are available to the search and quick-paste workflows
