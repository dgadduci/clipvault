# local-image-import Specification

## Purpose

Define cómo se importan archivos de imagen locales como capturas de ClipVault.

## ADDED Requirements

### Requirement: Import local image files

ClipVault SHALL let the user open a native file picker, select one or more
local image files, and import the selected files as image captures.

#### Scenario: Import supported image files

- **WHEN** the user selects supported PNG, JPEG, WebP, GIF, BMP, or TIFF files
  and confirms the import
- **THEN** ClipVault decodes each file locally and creates or reuses its image
  capture in the local History
- **AND** ClipVault persists each image through the existing normalized PNG
  asset store and its existing deduplication rules
- **AND** the selected collection is added as an additional local membership
  when a local capture collection is active
- **AND** the original file path and bytes are not sent to the frontend or
  retained as the capture's asset

### Requirement: Preserve the confirmed import order

ClipVault SHALL let the user review and set the final order of a multi-file
image selection before importing it, and SHALL preserve that order for newly
created captures in the History list.

#### Scenario: Confirm a custom order for multiple files

- **WHEN** the user selects multiple image files and reorders them in the
  import review before confirming
- **THEN** ClipVault imports the files in the confirmed logical order
- **AND** newly created captures from that batch appear in History in the same
  order
- **AND** a capture that already existed before the batch keeps its existing
  position in History
- **AND** repeated files with the same image content within the batch produce
  one capture at the first occurrence's position

#### Scenario: Native picker does not report click order

- **WHEN** the operating system returns selected files in an order that differs
  from the order in which the user clicked them
- **THEN** ClipVault shows the returned files in an editable order review
- **AND** the user can establish and confirm the intended order before import

#### Scenario: Invalid order confirmation

- **WHEN** the frontend submits an order with missing, duplicate, or unknown
  file identifiers
- **THEN** ClipVault rejects the confirmation and imports none of the pending
  files

#### Scenario: Unsupported or invalid file

- **WHEN** a selected file is HEIF/HEIC, has an unsupported format, cannot be
  decoded, or exceeds the existing image limits
- **THEN** ClipVault skips that file and continues processing the remaining
  selected files
- **AND** reports only aggregate counts to the frontend without revealing a
  path or image contents

#### Scenario: Duplicate image

- **WHEN** an imported image matches an existing image under the current
  normalized image hash
- **THEN** ClipVault reuses the existing capture according to the current
  deduplication behavior
- **AND** adds the active local collection membership when applicable

#### Scenario: User cancels file selection

- **WHEN** the user dismisses the native file picker without selecting files
- **THEN** ClipVault performs no import and leaves the current history intact

#### Scenario: User cancels order review

- **WHEN** the user cancels the review after selecting files
- **THEN** ClipVault performs no import and discards the pending local paths

### Requirement: Report import outcome

ClipVault SHALL show a localized aggregate result for a completed selection,
including how many files were imported, matched an existing capture, or were
skipped. The result SHALL NOT include source paths or file names.

During order review, ClipVault MAY send opaque per-file identifiers and
basename labels to the frontend so the user can distinguish and order the
selection. It SHALL NOT send parent directories, full paths, image bytes,
hashes or image contents, and SHALL NOT persist those identifiers or names.

#### Scenario: Show aggregate import result

- **WHEN** processing of a selected file batch completes
- **THEN** ClipVault displays the localized imported, duplicate, and skipped
  counts
- **AND** the result contains no file paths or names
