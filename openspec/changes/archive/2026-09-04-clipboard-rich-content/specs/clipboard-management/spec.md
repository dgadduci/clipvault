## MODIFIED Requirements

### Requirement: Delete history entries

ClipVault SHALL allow users to delete an individual text or image entry and to
clear non-favorite history through an explicit user action. Deleting an image
entry SHALL also make its local asset eligible for cleanup once no other entry
references it.

#### Scenario: Delete image entry

- **WHEN** the user confirms deletion of an image history entry
- **THEN** the entry is removed from SQLite, disappears from search/quick-paste
  where applicable, and its unreferenced asset can be collected

#### Scenario: Clear mixed history

- **WHEN** the user confirms clearing non-favorite history containing text and
  image entries
- **THEN** all non-favorite entries are removed while favorites and their
  referenced assets remain intact

### Requirement: Apply basic retention policy

ClipVault SHALL apply the configured retention policy to non-favorite text and
image entries, and SHALL collect image assets only after confirming that no
remaining history row references them.

#### Scenario: Retention removes an image entry

- **WHEN** a non-favorite image entry is older than the configured period
- **THEN** the entry is removed and its unreferenced local asset becomes
  eligible for cleanup

#### Scenario: Favorite image is retained

- **WHEN** a favorite image entry reaches the configured retention horizon
- **THEN** the entry and its asset remain available
