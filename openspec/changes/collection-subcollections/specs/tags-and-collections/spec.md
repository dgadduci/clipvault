## MODIFIED Requirements

### Requirement: Manage flat collections

ClipVault SHALL allow users to create, rename and delete user collections. A
top-level user collection MAY own direct subcollections; subcollections SHALL
NOT own children. Every collection SHALL expose a validated persistent color.
Before deleting a top-level user collection, ClipVault SHALL show metadata-only
counts for the distinct captures in that collection and its direct children,
then ask whether to preserve those captures in `Historial` or delete them from
all local history. The protected system `Historial` collection SHALL NOT be
deleted.

#### Scenario: Preview parent collection deletion

- **WHEN** the user opens the delete prompt for a top-level collection
- **THEN** the prompt displays the number of distinct captures and favorites
  associated with the collection or any direct child without exposing capture
  content or identifiers
- **AND** it offers separate choices to keep captures in `Historial` or
  delete them from all local history

#### Scenario: Delete parent and preserve captures

- **WHEN** the user deletes a top-level collection and chooses to preserve its
  captures
- **THEN** its direct child collections are deleted with it
- **AND** all captures associated with the parent or its children remain in
  `Historial` and retain their other collection memberships

#### Scenario: Delete parent and its captures

- **WHEN** the user deletes a top-level collection and chooses to delete its
  captures
- **THEN** its direct child collections are deleted with it
- **AND** every distinct capture associated with the parent or a child is
  removed from local history and every collection

## ADDED Requirements

### Requirement: Manage one-level subcollections

ClipVault SHALL allow a user collection to own zero or more direct
subcollections. Each subcollection SHALL belong to exactly one user collection,
and SHALL NOT own subcollections. Its parent SHALL be an unbound local user
collection; peer-bound imported collections SHALL NOT own children.
`Historial` SHALL remain a protected top-level system collection. A
subcollection SHALL behave like an existing user collection for naming,
colors, notes, capture membership, filtering and capture drop operations.
Membership SHALL remain independent: adding a capture to a subcollection SHALL
NOT implicitly add it to its parent. Existing collections SHALL remain
top-level after migration, and existing captures, memberships, notes, peer
bindings and asset references SHALL be preserved.
Existing clear-collection actions SHALL continue to affect only direct
memberships; clearing a parent SHALL NOT implicitly clear its children.

#### Scenario: Existing collections migrate intact

- **WHEN** ClipVault opens a database created before subcollections
- **THEN** every existing collection has no parent
- **AND** capture rows, memberships, collection metadata, notes and asset
  references are unchanged

#### Scenario: Create a subcollection

- **WHEN** the user creates a subcollection under an unbound local user
  collection
- **THEN** it is persisted with exactly that parent
- **AND** it appears beneath the parent in the expanded collection tree
- **AND** it can be selected, assigned captures, renamed, recolored, annotated
  and used as a drop target like any other user collection

#### Scenario: Reject invalid parent relationships

- **WHEN** a caller tries to create a subcollection under `Historial` or under
  another subcollection or a peer-bound imported collection
- **THEN** ClipVault rejects the request without changing the collection tree

#### Scenario: Independent membership

- **WHEN** a capture is assigned to a subcollection
- **THEN** its membership in the parent is unchanged
- **AND** its existing membership in `Historial` is preserved

#### Scenario: Expand and collapse a parent

- **WHEN** the user activates a parent's disclosure control
- **THEN** its child list is shown or hidden and the control exposes the
  matching expanded state to assistive technology

#### Scenario: Delete a parent collection

- **WHEN** the user confirms deletion of a principal collection
- **THEN** the principal and every direct subcollection are deleted
- **AND** the existing preserve-in-`Historial` or delete-everywhere choice is
  applied once to the distinct captures associated with the full tree

### Requirement: Delete a subcollection with an explicit capture scope

Before deleting a subcollection, ClipVault SHALL preview its distinct capture
and favorite counts and ask whether to keep those captures in the parent or
delete them everywhere. Keeping SHALL preserve each capture and every
pre-existing membership while adding its parent membership. Deleting
everywhere SHALL remove the captures from `Historial` and all collections.
Subcollection deletion SHALL NOT delete its parent. The preview SHALL be
rechecked atomically before mutation.

#### Scenario: Keep child captures in the parent

- **WHEN** the user confirms subcollection deletion and chooses to keep its
  captures
- **THEN** the child definition and its own memberships are removed
- **AND** each associated capture remains in the parent and every other
  collection it already belonged to
- **AND** capture data, tags, notes and referenced assets remain intact

#### Scenario: Delete child captures everywhere

- **WHEN** the user confirms subcollection deletion and chooses to delete its
  captures everywhere
- **THEN** each distinct capture in the child is deleted from local history,
  including `Historial`, the parent and other collections
- **AND** the parent collection remains

#### Scenario: Child contents changed after preview

- **GIVEN** the user previewed a child deletion
- **AND** its distinct capture or favorite count changes before confirmation
- **WHEN** the user confirms either deletion scope
- **THEN** ClipVault leaves the child and captures unchanged
- **AND** returns updated counts for a new confirmation

#### Scenario: Cancel child deletion

- **WHEN** the user cancels the child deletion prompt
- **THEN** the child, parent, capture data, memberships and assets remain
  unchanged
