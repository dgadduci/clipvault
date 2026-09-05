## MODIFIED Requirements

### Requirement: Capture text clipboard changes

ClipVault SHALL capture new text content when the operating system clipboard
changes, SHALL capture supported raster images when no non-empty text
representation is available, SHALL ignore unsupported non-text content without
terminating the application, and SHALL ignore payloads whose source
application matches the configured ignored-application blacklist. A permitted
capture MAY enrich its persisted entry with source-application presentation
metadata without altering the payload or privacy identifier.

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
