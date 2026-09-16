## MODIFIED Requirements

### Requirement: Delete history entries

ClipVault SHALL allow users to delete an individual text or image entry and to
clear non-favorite history through an explicit user action. Deleting an image
entry SHALL also make its local asset eligible for cleanup once no other entry
references it. When one or more live text entries are removed, the shared
capture watcher SHALL baseline its in-memory change-detection state against the
current clipboard revision (a metadata-only counter from the platform layer)
so the next observation of the same clipboard payload at the same revision is
classified as `Unchanged` instead of being recreated.

#### Scenario: Delete one entry

- **WHEN** the user confirms deletion of a history entry
- **THEN** the entry is removed from SQLite and no longer appears in search or quick paste

#### Scenario: Delete image entry

- **WHEN** the user confirms deletion of an image history entry
- **THEN** the entry is removed from SQLite, disappears from search/quick-paste
  where applicable, and its unreferenced asset can be collected

#### Scenario: Clear history

- **WHEN** the user confirms clearing non-favorite history
- **THEN** all non-favorite entries are removed while favorites remain intact

#### Scenario: Clear mixed history

- **WHEN** the user confirms clearing non-favorite history containing text and
  image entries
- **THEN** all non-favorite entries are removed while favorites and their
  referenced assets remain intact

#### Scenario: Delete missing entry

- **WHEN** a delete request targets an entry that no longer exists
- **THEN** the operation completes idempotently without affecting other entries

#### Scenario: Delete does not auto-recapture unchanged clipboard

- **GIVEN** the watcher has observed text `A` at clipboard revision `R1` and a
  live history row contains `A`
- **WHEN** the user confirms deletion of that row while the clipboard still
  contains `A` at revision `R1`
- **THEN** the successful deletion baselines the watcher against revision `R1`
- **AND** the next watcher evaluation at revision `R1` returns `Unchanged`
  without creating a new history row

#### Scenario: Delete enables recapture when the clipboard is rewritten

- **GIVEN** the watcher has observed text `A` at clipboard revision `R1` and a
  live history row contains `A`
- **WHEN** the user confirms deletion of that row and the clipboard is then
  rewritten with `A` at revision `R2` (R2 > R1)
- **THEN** the successful deletion baselines the watcher against revision `R1`
- **AND** the next watcher evaluation observes revision `R2` and stores `A` as a
  new history row with a new opaque identifier

#### Scenario: Clear enables recapture of removed non-favorite text

- **GIVEN** a non-favorite history row contains the current clipboard text
  observed at revision `R1`
- **WHEN** the user confirms clearing non-favorite history and the clipboard
  still holds the same text at revision `R1`
- **THEN** the watcher state is rebaselined only when at least one row was
  removed
- **AND** the next evaluation at the same revision returns `Unchanged`
- **AND** a later evaluation at a different revision stores a new row while
  favorite rows remain available

#### Scenario: Destructive no-ops leave the watcher untouched

- **WHEN** a destructive call returns `ConfirmationRequired`, `NotFound`, or
  reports zero rows removed
- **THEN** the watcher's revision baseline is not modified
- **AND** the next evaluation at the same revision returns `Unchanged`
