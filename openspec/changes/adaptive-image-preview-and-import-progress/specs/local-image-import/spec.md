## ADDED Requirements

### Requirement: Show per-file import progress

When the user confirms an image import batch, ClipVault SHALL display progress
in the image-order review modal until the batch finishes or fails. The modal
SHALL show a progress bar and numeric percentage at the lower left of the
Cancel action while preserving the primary button's localized importing text.
The percentage SHALL primarily reflect encoded bytes read across the selected
files and SHALL include a processing contribution based on completed files.
It SHALL remain below 100 until the import command finishes. Progress events
SHALL contain only an opaque import session identifier and numeric byte/file
counters; they SHALL NOT expose paths, filenames, hashes, image bytes or image
contents.

#### Scenario: Progress advances as files finish

- **WHEN** the user confirms a batch of one or more image files
- **THEN** the progress bar and percentage update while bytes are read and
  after each file has been processed
- **AND** the percentage reaches 100 when the batch completes
- **AND** the primary button continues to show the localized importing text
- **AND** Cancel and order controls remain disabled while the batch is busy

#### Scenario: Ignore progress from another import session

- **WHEN** the frontend receives a progress event whose session identifier
  does not match the open import review
- **THEN** it ignores that event without changing the displayed percentage

#### Scenario: Import progress does not disclose image data

- **WHEN** ClipVault emits progress for an image batch
- **THEN** the event contains only the session identifier, bytes read, total
  bytes and numeric file counts
- **AND** no path, filename, image bytes, hash or image content is sent to the
  frontend

#### Scenario: Processing remains visible after byte reading

- **WHEN** all encoded bytes have been read but image processing or persistence
  for one or more files is still running
- **THEN** the progress bar remains below 100 and reflects completed files
- **AND** it reaches 100 only after the import command completes
