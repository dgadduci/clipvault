## Purpose

Definir la captura, persistencia, deduplicación, asset store, presentación y pegado de imágenes del portapapeles, manteniendo la coexistencia con el pipeline de texto y las restricciones de privacidad del MVP.

## Requirements

### Requirement: Capture supported image clipboard payloads

The existing text-first rule is refined: rich textual content MUST be
considered the highest-priority textual representation. Images MUST remain
the fallback only when no non-empty rich or plain text exists.

#### Scenario: Rich text and image are both available

- **WHEN** the clipboard exposes non-empty plain text with HTML or RTF and an
  image for the same operation
- **THEN** ClipVault captures one rich-text entry and does not create an image
  entry for that operation

#### Scenario: Plain text and image are both available

- **WHEN** the clipboard exposes non-empty plain text without a supported rich
  representation and an image
- **THEN** ClipVault captures text using the existing text pipeline and does
  not create an additional image entry

#### Scenario: Image remains the fallback

- **WHEN** the clipboard has no non-empty rich or plain text and contains a
  supported raster image
- **THEN** ClipVault captures the image through the existing image asset
  pipeline

#### Scenario: RTF-only is no longer treated as unsupported

- **WHEN** the clipboard exposes non-empty plain text plus a valid RTF flavor
  and no HTML
- **THEN** ClipVault captures it as `RichText` rather than ignoring it or
  reducing it to a plain-only entry

### Requirement: Persist image metadata and a local asset reference

Every persisted image entry SHALL use `content_type = image`, retain its
dimensions, MIME type, byte size and deterministic hash, and store only a
validated relative `asset_ref` instead of an absolute filesystem path or raw
image bytes in the textual column.

#### Scenario: Valid image is committed

- **WHEN** a supported image passes validation and the local database commit
  succeeds
- **THEN** SQLite contains an image entry with `asset_ref`, `mime_type`,
  width, height, size and hash metadata, and the asset exists locally

#### Scenario: Existing textual rows are reopened

- **WHEN** ClipVault opens a database created before image support
- **THEN** existing textual rows remain readable, searchable and pasteable
  without requiring a destructive migration

#### Scenario: Asset persistence fails

- **WHEN** the image cannot be validated, encoded or written to the local asset
  store
- **THEN** ClipVault returns a typed non-fatal failure, does not create a
  partially valid image row and leaves existing history usable

### Requirement: Use a controlled local image asset store

Image assets SHALL be stored below `<data_dir>/assets/clipboard/` with a
reference of the form `clipboard/<lowercase-sha256>.png`, written atomically,
and served only after namespace, traversal, symlink, PNG, dimension and size
validation.

#### Scenario: Asset reference is safe

- **WHEN** the frontend requests a persisted image using its `asset_ref`
- **THEN** the backend accepts only a relative reference inside the clipboard
  asset namespace and never returns an absolute path

#### Scenario: Malicious reference is supplied

- **WHEN** the asset command receives an absolute path, traversal component,
  wrong namespace, escaped symlink, invalid PNG or oversized image
- **THEN** it rejects the request with a stable typed error and returns no file
  bytes

#### Scenario: Duplicate asset already exists

- **WHEN** the normalized image hash maps to an existing valid asset
- **THEN** ClipVault reuses that asset and does not create a second file

### Requirement: Deduplicate image history by normalized bytes

ClipVault SHALL hash the canonical PNG representation of an image for dedupe,
shall not create duplicate rows for identical normalized images, and shall
preserve the existing hash and timestamp semantics for text.

#### Scenario: Same image is copied again

- **WHEN** an identical normalized image is captured again
- **THEN** no second history row or asset is created and the existing entry is
  refreshed according to the current duplicate policy

#### Scenario: Different image is copied

- **WHEN** the normalized PNG bytes differ
- **THEN** ClipVault creates a distinct image entry without modifying the prior
  entry's asset or metadata

### Requirement: Apply image lifecycle to management operations

Image entries SHALL participate in favorites, deletion, clear-history and
retention using the existing confirmation and favorite rules, and local image
assets SHALL be collected only when no remaining entry references them.

#### Scenario: Image entry is deleted

- **WHEN** the user confirms deletion of an image entry
- **THEN** the entry disappears from history and any now-unreferenced image
  asset becomes eligible for local cleanup

#### Scenario: Favorite image reaches retention

- **WHEN** a favorite image reaches the configured retention horizon
- **THEN** the entry and its referenced asset remain available

#### Scenario: Cleanup sees a shared asset

- **WHEN** an asset is still referenced by another history row
- **THEN** cleanup does not delete it

### Requirement: Reuse the existing history cards for images

The main recent-history rail SHALL render image entries through the existing
`HistoryCard` and `HistoryCardRail` foundation, preserving fixed dimensions,
title, source-app icon-only presentation, pin/unpin and overflow actions.

#### Scenario: Image card is rendered

- **WHEN** an image entry appears in recent history
- **THEN** its card shows a bounded thumbnail, the local Image type icon, title,
  source application icon when available and the existing actions

#### Scenario: Thumbnail cannot be loaded

- **WHEN** the asset is missing, invalid or the Blob URL cannot be created
- **THEN** the card shows an accessible visual fallback, does not show raw
  bytes or the internal asset reference, and the rail remains usable

#### Scenario: Text card remains unchanged

- **WHEN** the recent history contains a textual entry
- **THEN** its current compact preview, metadata, title, source icon and
  actions continue to work without image-specific placeholders

### Requirement: Paste selected image entries

`paste_entry` SHALL write an image entry to the platform clipboard and then
reuse the existing paste controller, while leaving the history entry unchanged
when image writing or synthetic paste fails.

#### Scenario: Image paste succeeds

- **WHEN** the user selects an image entry in quick-paste or the main history
  and the platform supports image writing and synthetic paste
- **THEN** ClipVault writes the image, triggers paste into the previously active
  application and returns a typed success outcome

#### Scenario: Image paste capability is unavailable

- **WHEN** the current session cannot write image data or cannot perform the
  required synthetic paste
- **THEN** ClipVault returns a typed capability outcome with platform guidance
  when actionable, without converting the image to text or mutating history

### Requirement: Extend quick-paste without changing its interaction model

Quick-paste SHALL show recent image entries using the same keyboard navigation,
selection, transient-window, hide-before-paste and guidance behavior already
defined for text entries.

#### Scenario: Image is selected in quick-paste

- **WHEN** the user navigates to an image entry and presses Enter
- **THEN** the transient window hides before the image paste command runs and
  the image is pasted or the existing guidance is shown

#### Scenario: Image history is empty

- **WHEN** there are no image or text entries
- **THEN** quick-paste remains usable and shows its existing empty state

### Requirement: Report image capabilities by platform

ClipVault SHALL expose distinct image clipboard capabilities and SHALL not
claim image read/write support merely because text clipboard support is
available. macOS, Linux X11 and Linux Wayland may return different typed
availability outcomes.

#### Scenario: Runtime supports image clipboard

- **WHEN** the platform adapter successfully verifies image read or write
  support
- **THEN** the corresponding capability is reported available and the
  operation can be attempted

#### Scenario: Runtime does not support image clipboard

- **WHEN** the current display/session cannot provide the requested image
  operation
- **THEN** the capability is reported unavailable, the rest of text history
  remains usable and no false permission instruction is presented

### Requirement: Preserve privacy for image capture

The PrivacyGate SHALL be evaluated before permanent image metadata or asset
creation, and no image bytes, thumbnails, hashes, paths or clipboard contents
may be placed in logs, Tauri events or user-facing error messages.

#### Scenario: Blacklisted application copies an image

- **WHEN** the source application matches the ignored-application blacklist
- **THEN** ClipVault creates no image row, no image asset and no history-updated
  event for that capture

#### Scenario: Image capture is allowed

- **WHEN** PrivacyGate allows the source application
- **THEN** ClipVault may persist the image and source-app presentation metadata
  locally, but diagnostics and events remain metadata-only

### Requirement: Paste Mode::Plain publishes only plain text

`PasteMode::Plain` MUST publish only the canonical plain text
flavour. On macOS the adapter MUST call `clearContents()` before
declaring the type set so residual `public.html`, `public.rtf` and
any prior rich flavours are dropped. The composite clipboard MUST
NOT delegate the plain write to a backend that leaves rich
flavours on the pasteboard.

#### Scenario: Plain paste clears residual rich flavours

- **WHEN** the user selects `Paste de texto plano` for any entry
- **THEN** the resulting pasteboard contains only the canonical
  plain text flavour; `public.html`, `public.rtf` and the previous
  capture's rich bytes are absent

#### Scenario: Plain paste does not write an artificial RTF

- **WHEN** the plain path is exercised
- **THEN** the contract reports `pasted` (or a typed failure); no
  artificial RTF wrapper is published to mask the missing colours
  or fonts of the source

#### Scenario: Rich paste still publishes original rich bytes

- **WHEN** the user selects `Paste de texto enriquecido` for a
  rich entry
- **THEN** the adapter publishes the original HTML and RTF
  representations and reports `pasted`; the plain-text fallback
  is only reported when the host cannot publish the rich
  flavours