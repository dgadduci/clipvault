## MODIFIED Requirements

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
