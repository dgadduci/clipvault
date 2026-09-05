## ADDED Requirements

### Requirement: Preserve organization during history management

Existing delete, clear-history, retention and favorite operations SHALL preserve
the collection/tag lifecycle defined by `tags-and-collections`.

#### Scenario: Delete organized entry

- **WHEN** the user confirms deletion of an entry with tags and collection
  memberships
- **THEN** the entry and all association rows are removed atomically while
  other entries and definitions remain

#### Scenario: Clear organized history

- **WHEN** clear-history removes non-favorite entries with organization data
- **THEN** their association rows are removed, favorite entries remain in
  their collections and tags, and definitions are not deleted implicitly

#### Scenario: Retention organized entry

- **WHEN** retention removes an expired non-favorite organized entry
- **THEN** its associations are removed in the same logical mutation and the
  existing metadata-only outcome is returned

#### Scenario: Favorite organized entry

- **WHEN** the user toggles favorite state for an organized entry
- **THEN** no tag or collection membership changes

### Requirement: Remove an entry from Historial globally

The existing confirmed destructive delete flow SHALL treat removal from the
system `Historial` collection as deletion of the entry from ClipVault and all
secondary collections.

#### Scenario: Confirm global removal

- **WHEN** the user confirms deletion of an entry while viewing `Historial`
- **THEN** the entry, its associations and any now-unreferenced payload assets
  are handled by the existing atomic deletion flow

#### Scenario: Cancel global removal

- **WHEN** the user cancels the deletion confirmation
- **THEN** the entry and every organization association remain unchanged
