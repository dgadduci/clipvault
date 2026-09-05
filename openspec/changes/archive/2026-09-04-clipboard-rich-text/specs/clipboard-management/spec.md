## ADDED Requirements

### Requirement: Manage rich-text asset lifecycle

Rich-text assets SHALL follow the existing entry lifecycle. Deleting,
clearing, or expiring an entry SHALL make its rich HTML, RTF and sanitized
preview assets eligible for cleanup only after the database mutation and only
when no remaining entry references them.

#### Scenario: Delete rich entry

- **WHEN** the user confirms deletion of a rich entry
- **THEN** the row is removed and unreferenced rich assets are collected
  without affecting other entries

#### Scenario: Clear mixed history

- **WHEN** clear-history removes non-favorite rich and plain entries
- **THEN** favorite rows and their shared rich assets remain intact and only
  unreferenced rich files are eligible for cleanup

#### Scenario: Retention expires rich entry

- **WHEN** retention removes an old non-favorite rich entry
- **THEN** its rich assets are handled by the same reference-counted cleanup
  path and the retention outcome remains metadata-only

#### Scenario: Shared rich asset

- **WHEN** two entries reference the same rich asset
- **THEN** deleting or expiring one entry never deletes that asset while the
  other reference remains
