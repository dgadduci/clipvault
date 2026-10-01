## ADDED Requirements

### Requirement: Pause and resume local clipboard capture

ClipVault SHALL let the user pause and resume local operating-system clipboard
capture for every supported text and image payload. Pausing SHALL prevent new
local clipboard entries from being stored while preserving all existing
history entries and assets. The effective state SHALL be loaded before the
capture loop can store payloads and SHALL remain consistent across capture
entry points.

#### Scenario: Capture is enabled by default

- **WHEN** ClipVault starts with no saved capture preference
- **THEN** local clipboard capture is enabled, preserving the current default
  behavior

#### Scenario: Pause local capture

- **WHEN** the user pauses local clipboard capture
- **THEN** no new local text, rich-text or image clipboard entry is stored once
  the pause operation reports success
- **AND** existing history entries and their assets remain unchanged

#### Scenario: Clipboard changes while paused

- **GIVEN** local clipboard capture is paused
- **WHEN** the user copies supported text or an image
- **THEN** ClipVault does not store the payload or expose it through history,
  search, events, logs or diagnostics

#### Scenario: Resume does not backfill paused clipboard contents

- **GIVEN** the current clipboard value was set while local capture was paused
- **WHEN** the user resumes local clipboard capture without changing the
  clipboard afterward
- **THEN** ClipVault does not create a history entry for that existing value
- **AND** subsequent local clipboard changes are captured normally

#### Scenario: Pause persists across restart

- **GIVEN** local clipboard capture is paused and its setting was saved
- **WHEN** ClipVault exits and starts again
- **THEN** local clipboard capture remains paused until the user resumes it

#### Scenario: Setting persistence fails

- **WHEN** ClipVault cannot persist a requested pause or resume operation
- **THEN** the last successfully applied capture state remains effective and
  the user is informed that the requested change did not take effect

#### Scenario: In-flight capture at pause boundary

- **WHEN** a local payload is being processed as the user pauses capture
- **THEN** the pause operation does not report success until the payload can no
  longer be committed as a new local history entry
