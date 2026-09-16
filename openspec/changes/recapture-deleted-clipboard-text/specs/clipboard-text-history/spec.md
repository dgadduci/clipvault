## MODIFIED Requirements

### Requirement: Prevent duplicate history entries

ClipVault SHALL use a metadata-only clipboard revision counter combined with a
content hash to prevent redundant history rows. The shared capture watcher
treats a poll as a fresh observation only when the clipboard revision reported
by the platform layer differs from the revision recorded for the previous
observation. The persistence layer deduplicates identical text against live
history rows only; once a row is removed, the next observation at a new
revision is stored as a new row with a fresh opaque identifier.

#### Scenario: Identical content is copied again

- **WHEN** text with a hash already present in history is copied again and the
  clipboard reports a new revision
- **THEN** ClipVault does not create a second identical row and updates the existing entry's latest-seen metadata as defined by the data model

#### Scenario: Same text is republished in a different representation

- **GIVEN** a live history row contains text `A` captured as rich text or as
  plain text
- **WHEN** a new clipboard revision publishes the same canonical text `A` in
  the other representation
- **THEN** ClipVault refreshes that live row and returns `Duplicate` instead
  of creating a second history card
- **AND** a rich rendition that resolves as `Duplicate` writes no new rich
  asset files

#### Scenario: Different content is copied

- **WHEN** the clipboard content differs and produces a different hash and the
  clipboard reports a new revision
- **THEN** ClipVault creates a new history entry without modifying the prior entry's content

#### Scenario: Clipboard content stays unchanged between polls

- **WHEN** the clipboard reports the same revision across two consecutive
  polls, regardless of whether the text matches the previous observation
- **THEN** the watcher returns `Unchanged` without persisting anything

#### Scenario: macOS composite preserves the native revision for a deletion baseline

- **GIVEN** the macOS runtime uses the composite clipboard backend and
  `NSPasteboard.changeCount` is `R`
- **WHEN** a history row is deleted and the watcher establishes its
  metadata-only baseline
- **THEN** the composite exposes `R` to that baseline and a later poll at
  the same `R` returns `Unchanged` without recreating the deleted row

#### Scenario: Deleted text is recaptured after a clipboard rewrite

- **GIVEN** a text capture with hash `H` existed at revision `R1`, was
  deleted, and the watcher's baseline was rebaselined against `R1`
- **WHEN** the clipboard is rewritten with the same text `H` at revision `R2`
  (R2 > R1)
- **THEN** the next watcher evaluation observes revision `R2` and the
  persistence layer creates a new text row with a new opaque identifier
- **AND** the outcome is `Stored`, not `Unchanged`, `Duplicate` or an update of
  the deleted row

#### Scenario: Deleting another entry does not create duplicates

- **GIVEN** the clipboard contains text whose live history row was not
  deleted and the watcher has baselined against the current revision
- **WHEN** another history entry is deleted and the watcher evaluates the
  current clipboard payload at a new revision
- **THEN** persistence still recognizes the live row by its hash and creates
  no second row
- **AND** the outcome is `Duplicate` against the live row's id

#### Scenario: Clipboard revision unavailable degrades gracefully

- **WHEN** the platform layer cannot report a stable clipboard revision for the
  current session
- **THEN** the watcher MUST NOT silently treat every poll as a change
- **AND** the limitation MUST be documented in OpenSpec instead of simulated
