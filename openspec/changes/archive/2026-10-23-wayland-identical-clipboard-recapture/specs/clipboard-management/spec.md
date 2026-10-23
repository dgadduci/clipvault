## MODIFIED Requirements

### Requirement: Delete history entries

ClipVault SHALL allow users to delete individual text, rich-text, or image
entries and to clear non-favorite history through an explicit user action.
When one or more rows are removed, the shared capture watcher SHALL baseline
its in-memory change-detection state against the current metadata-only
clipboard revision. The next observation of the same clipboard selection at
that revision SHALL be `Unchanged`; a later selection event at a new revision
SHALL make the same text or image eligible for a fresh row.

#### Scenario: Delete one entry

- **WHEN** the user confirms deletion of a history entry
- **THEN** the entry is removed from SQLite and no longer appears in search or
  quick paste

#### Scenario: Delete image entry

- **WHEN** the user confirms deletion of an image history entry
- **THEN** the entry is removed from SQLite, disappears from search and
  quick-paste where applicable, and its unreferenced asset can be collected

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
- **THEN** the operation completes idempotently without affecting other
  entries

#### Scenario: Deleting an image does not auto-recapture the unchanged selection

- **GIVEN** the clipboard contains image `I` at revision `R1` and a live image
  row refers to it
- **WHEN** the user deletes that image row while `I` remains selected at `R1`
- **THEN** the watcher baselines against `R1`
- **AND** the next poll at `R1` does not create a new row

#### Scenario: Deleting text or image enables recapture at a new revision

- **GIVEN** the user deleted a text or image row for payload `P` at revision
  `R1`
- **WHEN** the user copies the same payload `P` again and the platform reports
  a new revision `R2`
- **THEN** the watcher sends `P` through the normal persistence pipeline
- **AND** persistence creates a row with a new identifier because the deleted
  row is no longer live
