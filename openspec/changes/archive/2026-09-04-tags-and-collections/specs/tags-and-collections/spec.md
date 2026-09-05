## ADDED Requirements

### Requirement: Maintain the permanent Historial collection

ClipVault SHALL create a protected system collection named `Historial`. Every
history entry SHALL belong to it, and every new capture SHALL join it in the
same transaction as the entry. A user SHALL be able to associate an entry with
multiple additional flat collections.

#### Scenario: New capture enters Historial

- **WHEN** an allowed clipboard capture is stored
- **THEN** the new entry belongs to `Historial` before the operation completes

#### Scenario: Existing database is upgraded

- **WHEN** ClipVault opens a database created before collections existed
- **THEN** it creates `Historial`, associates every existing entry with it and
  preserves all existing entry data

#### Scenario: Entry belongs to several collections

- **WHEN** the user assigns an entry to `Trabajo` and `Clientes`
- **THEN** the entry remains in `Historial` and is also visible in both user
  collections

#### Scenario: Historial is protected

- **WHEN** a user attempts to rename or delete `Historial`
- **THEN** ClipVault rejects the operation with a typed validation result and
  leaves the system collection unchanged

### Requirement: Manage flat collections

ClipVault SHALL allow users to create, rename and delete flat user collections.
Deleting a user collection SHALL remove only its memberships and SHALL NOT
delete entries, tags, `Historial` or memberships in other collections.

#### Scenario: Create collection

- **WHEN** the user submits a valid new collection name
- **THEN** the collection is persisted locally and appears in the sidebar

#### Scenario: Rename collection

- **WHEN** the user submits a valid new name for a user collection
- **THEN** only that collection's display name changes and its memberships
  remain intact

#### Scenario: Delete collection

- **WHEN** the user confirms deletion of a user collection
- **THEN** the collection and its association rows are removed while every
  associated entry remains in `Historial` and other collections

#### Scenario: Duplicate collection name

- **WHEN** the user creates or renames a collection to a name already used
  case-insensitively
- **THEN** ClipVault rejects it without modifying existing collections

### Requirement: Manage user tags

ClipVault SHALL allow users to create, rename and delete local tags. Tag
identity SHALL be normalized for whitespace and case-insensitive uniqueness;
deleting a tag SHALL remove only its associations and SHALL NOT delete entries.

#### Scenario: Create normalized tag

- **WHEN** the user creates a valid tag with surrounding or repeated spaces
- **THEN** ClipVault stores the normalized identity, preserves a readable
  display name and shows one tag definition

#### Scenario: Duplicate tag

- **WHEN** the user creates a tag whose normalized name already exists
- **THEN** ClipVault rejects or reuses the existing definition idempotently and
  never creates a duplicate tag

#### Scenario: Delete tag

- **WHEN** the user confirms deletion of a tag
- **THEN** its associations disappear and all tagged entries remain unchanged

### Requirement: Assign tags and collections from cards

Each history card SHALL provide accessible multi-select flows to assign tags and
collections, with search, checkboxes, save and cancel. Association mutations
SHALL be atomic and SHALL not modify clipboard payload or capture metadata.

#### Scenario: Assign multiple tags

- **WHEN** the user selects several tags and confirms
- **THEN** all selected tag associations are saved together and the card
  refreshes once

#### Scenario: Assign multiple collections

- **WHEN** the user selects several user collections and confirms
- **THEN** all selected collection associations are saved while the `Historial`
  membership remains present

#### Scenario: Cancel selector

- **WHEN** the user changes checkboxes and cancels
- **THEN** no association changes are persisted

#### Scenario: Remove from secondary collection

- **WHEN** the user views an entry inside a secondary collection and chooses
  `Quitar de esta colección`
- **THEN** only that collection membership is removed and the entry remains in
  `Historial` and any other collection

#### Scenario: Remove from Historial

- **WHEN** the user chooses the existing destructive delete action from
  `Historial` and confirms
- **THEN** the entry is removed from SQLite, all collections and all related
  association tables

### Requirement: Filter history by collection and tags

The main history view SHALL filter entries by one selected collection and zero
or more selected tags. Multiple selected tags SHALL use AND semantics. The
existing local text query, ranking, limits and offline behavior SHALL remain
unchanged after filtering.

#### Scenario: Select Historial

- **WHEN** the user selects `Historial`
- **THEN** the rail shows all eligible history entries regardless of secondary
  collection membership

#### Scenario: Select user collection

- **WHEN** the user selects `Trabajo`
- **THEN** the rail shows only entries associated with `Trabajo`

#### Scenario: Filter with several tags

- **WHEN** the user selects `código` and `pendiente`
- **THEN** only entries carrying both tags remain eligible

#### Scenario: Query and filters combine

- **WHEN** the user enters a text query while a collection and tags are
  selected
- **THEN** search applies all filters locally before using the existing ranking
  algorithm

#### Scenario: Empty collection or filter

- **WHEN** a valid collection or tag combination has no matching entries
- **THEN** the UI shows a clear empty state and the rest of the application
  remains usable

### Requirement: Display compact tag metadata in cards

The existing square history card SHALL display at most two assigned tag chips
and a `+N` indicator for additional tags. A card with no tags SHALL NOT reserve
an empty tag row or change the fixed card dimensions.

#### Scenario: Card with one or two tags

- **WHEN** an entry has one or two tags
- **THEN** the card shows those tags compactly without displacing its existing
  title, type icon, source-app icon or actions

#### Scenario: Card with more than two tags

- **WHEN** an entry has more than two tags
- **THEN** the card shows two tags and an accessible `+N` indicator for the
  remaining count

#### Scenario: Card without tags

- **WHEN** an entry has no tags
- **THEN** the card remains visually balanced without an empty reserved area

### Requirement: Preserve organization through management operations

Delete, clear-history and retention SHALL remove association rows for removed
entries atomically while preserving user tag and collection definitions. Favorite
operations SHALL not alter memberships.

#### Scenario: Delete entry with memberships

- **WHEN** the user confirms deletion of an entry assigned to several
  collections and tags
- **THEN** the entry and all its associations are removed without affecting
  other entries or definitions

#### Scenario: Clear mixed history

- **WHEN** clear-history removes non-favorite entries with organization data
- **THEN** favorites and their memberships remain, removed entries leave no
  association rows and collection/tag definitions remain available

#### Scenario: Retention removes an organized entry

- **WHEN** retention expires a non-favorite entry that has tags or collection
  memberships
- **THEN** the entry and associations are removed atomically according to the
  existing retention policy

#### Scenario: Favorite toggle

- **WHEN** the user pins or unpins an organized entry
- **THEN** its tag and collection memberships remain unchanged

### Requirement: Keep quick-paste global

The existing quick-paste interaction SHALL continue searching the global
history set and SHALL NOT require a collection selector in this change.

#### Scenario: Quick-paste after organization

- **WHEN** an entry belongs to `Historial` and one or more secondary
  collections
- **THEN** quick-paste can still find it through the existing global search

#### Scenario: Organization does not change keyboard flow

- **WHEN** the user opens quick-paste, navigates and pastes
- **THEN** collection/tag UI does not add steps or change the existing transient
  window, focus, privacy or paste behavior

### Requirement: Keep organization commands local and private

Organization commands SHALL accept only validated IDs, names and filter values.
They SHALL NOT accept clipboard content or return rich payload bytes, and logs
and events SHALL remain free of clipboard content, hashes, snippets and paths.

#### Scenario: Association command response

- **WHEN** an association command succeeds
- **THEN** the response contains only safe summaries, identifiers, names,
  counts and statuses needed by the UI

#### Scenario: Invalid or stale target

- **WHEN** a command targets a missing entry, tag or collection
- **THEN** it returns a typed non-fatal result and does not modify unrelated
  history
