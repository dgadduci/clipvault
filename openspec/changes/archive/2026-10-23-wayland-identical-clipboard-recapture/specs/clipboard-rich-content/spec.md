## MODIFIED Requirements

### Requirement: Deduplicate image history by persisted bytes

ClipVault SHALL hash the bytes it actually persists on disk for dedupe, shall
not create duplicate rows for identical persisted images, and shall preserve
the existing hash and timestamp semantics for text. Image capture changes
SHALL use the same metadata-only clipboard revision source as text capture.
After an image entry is deleted, a new clipboard selection containing the
same image SHALL be eligible for a new row; the unchanged selection left on
the clipboard SHALL NOT recreate the deleted row.

#### Scenario: Deleted image is recaptured after an identical clipboard copy

- **GIVEN** an image with persisted-byte hash `H` was captured at revision
  `R1` and its history row was deleted
- **AND** the watcher baselined against `R1` while the same image remained on
  the clipboard
- **WHEN** the user copies the same image again and the active transport
  reports revision `R2` (R2 > R1)
- **THEN** ClipVault creates a new image row with a new opaque identifier
- **AND** the image asset pipeline persists or reuses the validated local asset
- **AND** a poll of the unchanged selection at `R1` does not recreate the row

#### Scenario: Same image is copied again

- **WHEN** an identical persisted image is captured again at a new clipboard
  revision
- **THEN** no second history row or asset is created and the existing entry
  is refreshed according to the current duplicate policy

#### Scenario: Different image is copied

- **WHEN** the persisted bytes differ from the image already in history
- **THEN** ClipVault creates a distinct image entry without modifying the
  prior entry's asset or metadata
