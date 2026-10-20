## Purpose

Definir la organización local del historial de ClipVault mediante el sistema de
colecciones planas (con la colección protegida `Historial`) y tags
normalizados, junto con los flujos de asignación, filtrado y mantenimiento que
preservan la privacidad local-first.

## Requirements

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

ClipVault SHALL allow users to create, rename and delete flat user
collections, and every collection SHALL expose a validated persistent color.
Before deleting a user collection, ClipVault SHALL show a metadata-only count
of its entries and favorites and ask whether to preserve those entries in
`Historial` or delete them from all local history. Preserving entries SHALL
remove only the collection and its memberships; entries, `Historial` and other
collection memberships SHALL remain. Choosing to delete entries SHALL remove
all entries currently in that collection from local history and every other
collection, including favorites, in the same transaction as collection
deletion. Tag definitions SHALL remain. The protected system `Historial`
collection SHALL NOT be deleted.

#### Scenario: Create collection

- **WHEN** the user submits a valid new collection name
- **THEN** the collection is persisted locally with exactly one color from the
  configured red, yellow, green or blue palette
- **AND** the color is selected by the backend rather than trusted from the
  frontend
- **AND** the collection appears in the sidebar

#### Scenario: Rename collection

- **WHEN** the user submits a valid new name for a user collection
- **THEN** only that collection's display name changes and its memberships
  remain intact

#### Scenario: Preview collection deletion

- **WHEN** the user opens the delete prompt for a user collection
- **THEN** the prompt displays the number of entries and favorites currently
  associated with that collection without exposing entry content or identifiers
- **AND** it offers separate choices to preserve entries in `Historial` or to
  delete them from all local history

#### Scenario: Delete collection

- **WHEN** the user confirms collection deletion and chooses to preserve entries
- **THEN** the collection and its association rows are removed
- **AND** every associated entry remains in `Historial` and other collections
- **AND** import provenance remains available for preserved imported entries

#### Scenario: Delete collection and its entries

- **WHEN** the user confirms collection deletion and chooses to delete its entries
- **THEN** all currently associated entries, including favorites, are removed
  from history and other collections atomically with the collection
- **AND** tag and unrelated collection definitions remain
- **AND** referenced assets are retained if any remaining entry still uses them

#### Scenario: Collection contents changed after preview

- **GIVEN** the user previewed deletion counts for a collection
- **AND** the total or favorite-entry count changes before confirmation
- **WHEN** the user confirms deletion of the collection's entries
- **THEN** ClipVault leaves the collection and all entries unchanged
- **AND** the prompt receives the updated counts and requires confirmation
  again before deleting entries

#### Scenario: Cancel collection deletion

- **WHEN** the user cancels the delete prompt
- **THEN** the collection, entries, memberships, provenance and assets remain
  unchanged

#### Scenario: Protected system collection

- **WHEN** a user attempts to rename or delete `Historial`
- **THEN** ClipVault rejects the operation with a typed validation result and
  leaves the system collection unchanged

#### Scenario: Duplicate collection name

- **WHEN** the user creates or renames a collection to a name already used
  case-insensitively
- **THEN** ClipVault rejects it without modifying existing collections

#### Scenario: Existing collection receives a migration color

- **WHEN** ClipVault opens a database whose collections predate color support
- **THEN** every existing collection receives a valid default color without
  changing its name, timestamps, memberships or kind

### Requirement: Edit and persist collection colors

Each collection SHALL have one opaque RGB color stored as normalized
`#rrggbb`. The user SHALL be able to edit the color without changing the
collection identity or any entry association.

#### Scenario: Open the color editor from the sidebar

- **WHEN** the user double-clicks a collection color square
- **THEN** ClipVault opens a modal for that collection with its current color
  selected
- **AND** the modal offers an accessible visual color picker, Guardar and
  Cancelar

#### Scenario: Save a user-selected color

- **WHEN** the user selects a valid opaque RGB color and confirms Guardar
- **THEN** ClipVault persists the normalized HEX color for that collection
- **AND** the sidebar square and every visible card label for that collection
  refresh after the command succeeds
- **AND** no entry, tag, membership, asset or clipboard payload changes

#### Scenario: Cancel or close the color editor

- **WHEN** the user presses Escape, clicks Cancelar, or closes the backdrop
- **THEN** the previous collection color remains persisted
- **AND** no organization update event is emitted

#### Scenario: Invalid color is rejected

- **WHEN** a color update contains an invalid, transparent or non-HEX value
- **THEN** the backend returns a typed validation error
- **AND** the previous color and all collection memberships remain unchanged

#### Scenario: Historial keeps its protected identity

- **WHEN** the user changes the color of `Historial`
- **THEN** only its color changes
- **AND** its stable key, system kind, name, memberships and protection against
  rename/delete remain unchanged

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

### Requirement: Persist and edit one local note per collection

ClipVault SHALL allow one optional plain-text note per collection, including
Historial, user collections and collections bound to peer imports. A note
SHALL support line breaks and SHALL be persisted locally, independently from
the collection name, color, kind and memberships. The collection list SHALL
identify collections with a saved note using an accessible note icon whose
activation opens the shared note editor. A collection without a note SHALL
provide an accessible action to create one. Collection notes SHALL NOT be
included in peer-to-peer capture transfers, logs, diagnostics or collection
list bodies. Deleting a collection SHALL remove its note with the collection,
even when its entries are preserved.

#### Scenario: Add a collection note

- **WHEN** the user creates and saves a multiline note for a collection
- **THEN** ClipVault persists the note locally
- **AND** the collection row identifies the saved note with an accessible
  note icon

#### Scenario: Open and edit a collection note

- **GIVEN** a collection has a saved note
- **WHEN** the user activates its note icon in the collection list
- **THEN** the shared note modal opens with the persisted note and allows the
  user to edit it

#### Scenario: Create a note for a collection without one

- **GIVEN** a collection has no saved note
- **WHEN** the user activates its add-note action
- **THEN** the shared note modal opens with an empty multiline editor
- **AND** the collection list shows the note icon after a successful save

#### Scenario: Remove a collection note

- **WHEN** the user saves an empty collection note
- **THEN** ClipVault removes the note association and the collection row no
  longer identifies a saved note

#### Scenario: Cancel collection note editing

- **WHEN** the user cancels, presses Escape or closes the backdrop before
  saving
- **THEN** the persisted collection note remains unchanged

#### Scenario: Collection deletion removes its note

- **WHEN** the user deletes a collection while preserving its entries
- **THEN** ClipVault deletes the collection note with the collection
- **AND** the preserved entries, their memberships in other collections and
  their capture notes remain unchanged

#### Scenario: Collection note stays local

- **WHEN** a collection is bound to peer imports and has a local note
- **THEN** the note remains local and is not included in any capture transfer
- **AND** the collection's import binding and entries remain unchanged

### Requirement: Clear a selected user collection from the desktop toolbar

When a user collection is selected, the desktop toolbar SHALL provide an
accessible action to empty that collection. The action SHALL use a
metadata-only preview for the complete collection, independent of search,
tag or other visible filters, and ask whether its captures should also be
deleted from local history. The dialog SHALL offer preserving the captures in
history, deleting them from all local history, and canceling. The existing
Historial toolbar action SHALL continue to clear only eligible unorganized
captures and SHALL retain its current confirmation behavior.

#### Scenario: Empty a user collection and preserve its captures

- **WHEN** the user selects a local user collection, opens its toolbar
  clear action and chooses to keep its captures in Historial
- **THEN** only the selected collection memberships are removed
- **AND** every capture remains in Historial and in its other collections
- **AND** local peer-import provenance, when present, remains available

#### Scenario: Empty a user collection and delete its captures from history

- **WHEN** the user selects the option to also delete the collection's
  captures from local history
- **THEN** all captures currently in that collection are deleted from
  history and their other memberships, including favorites and local import
  provenance
- **AND** the selected collection definition remains present
- **AND** its peer binding and trust state remain unchanged when it is a
  peer-bound collection
- **AND** assets still referenced by another capture are retained

#### Scenario: Preview uses the complete collection scope

- **GIVEN** the selected collection is narrowed by a search query, tag or
  another visible filter
- **WHEN** the user opens the clear action
- **THEN** the confirmation preview describes every capture in the collection
  and does not expose capture content, IDs, hashes, provenance or asset paths

#### Scenario: Collection contents change before confirmation

- **GIVEN** the user has opened a clear confirmation with metadata counts
- **AND** the collection's entry or favorite count changes before commit
- **WHEN** the user confirms
- **THEN** ClipVault leaves the collection and its entries unchanged
- **AND** presents the updated metadata preview and requires confirmation
  again

#### Scenario: Cancel clearing a collection

- **WHEN** the user cancels the clear confirmation
- **THEN** collection memberships, entries, favorites, provenance and assets
  remain unchanged

#### Scenario: Clear an empty collection

- **WHEN** the selected user collection contains no captures
- **THEN** the toolbar does not submit a destructive operation
- **AND** the collection remains available for future assignments or imports

#### Scenario: Clear action in Historial

- **WHEN** the user opens the toolbar clear action in Historial
- **THEN** it continues to remove only captures eligible under the existing
  unorganized-history rule
- **AND** it does not clear secondary user or peer-bound collections

### Requirement: Delete tags from the card assignment selector

The tag selector opened from a history card SHALL provide a per-tag delete
action. Before deleting a tag, it SHALL explain that the tag and its
associations will be removed from every capture and require confirmation.
After confirmation, ClipVault SHALL persistently remove the tag definition
and all of its associations without deleting captures. The organization
snapshot and every loaded capture's tag metadata SHALL refresh after success.
On failure, the tag and its associations SHALL remain unchanged and the
selector SHALL remain open with a localized error.

#### Scenario: Cancel tag deletion

- **GIVEN** the user requested deletion of a tag from the card selector
- **WHEN** the user cancels the confirmation
- **THEN** the tag definition, its associations and the selection remain
  unchanged

#### Scenario: Delete a tag used by several captures

- **GIVEN** the tag is associated with the current capture and other captures
- **WHEN** the user confirms deletion from the card selector
- **THEN** the tag definition and all its associations are removed in
  persistent storage
- **AND** no capture is deleted or has its clipboard content changed
- **AND** every loaded card refreshes and no longer displays the tag
- **AND** the tag disappears from the toolbar filter options

#### Scenario: Delete the currently filtered tag

- **GIVEN** the desktop toolbar filters history by a tag
- **WHEN** that tag is deleted from a card selector
- **THEN** the toolbar filter resets to `Todas`
- **AND** the history reloads without applying the deleted tag ID

#### Scenario: Failed tag deletion

- **WHEN** the delete command fails
- **THEN** the modal displays a localized error and stays open
- **AND** the tag definition, associations and current selection remain intact

### Requirement: Filter shared captures in a principal collection view

The desktop collection list SHALL provide an accessible visibility toggle on
each user principal collection that has direct subcollections. The toggle
SHALL control whether captures directly associated with the principal remain
visible when they are also associated with one or more of its direct
subcollections. It SHALL default to showing those captures and SHALL retain
its state per principal collection for the current application session.
Collection memberships SHALL remain independent: a capture assigned only to
a subcollection SHALL NOT become a member of its principal collection.

#### Scenario: Show captures also assigned to a subcollection

- **GIVEN** a capture belongs directly to a principal collection and to one
  of its subcollections
- **WHEN** the principal collection's visibility toggle is active
- **THEN** the capture remains visible in that principal collection
- **AND** the principal's other directly associated captures remain visible

#### Scenario: Hide captures also assigned to a subcollection

- **GIVEN** a capture belongs directly to a principal collection and to one
  of its subcollections
- **WHEN** the principal collection's visibility toggle is inactive
- **THEN** the capture is hidden from the principal collection view
- **AND** a capture belonging directly to the principal but to no child
  remains visible

#### Scenario: Consider every direct subcollection

- **GIVEN** a capture belongs directly to the principal and to any one of its
  direct subcollections
- **WHEN** the visibility toggle is inactive
- **THEN** that capture is hidden even if it is not in the principal's other
  subcollections
- **AND** memberships in unrelated collections do not affect the result

#### Scenario: Keep subcollection-only captures independent

- **GIVEN** a capture belongs to a subcollection but not its principal
- **WHEN** the user selects the principal, with either toggle state
- **THEN** the capture is not added to the principal's view by this toggle
- **AND** all persisted memberships remain unchanged

#### Scenario: Apply visibility together with search and filters

- **GIVEN** the user has disabled shared captures for the selected principal
- **WHEN** recent entries or text-search results are displayed with tag or
  source-application filters
- **THEN** captures also assigned to any direct subcollection remain hidden
- **AND** the existing search, tag, source-application and ordering behavior
  is otherwise preserved

#### Scenario: Expose the toggle accessibly

- **WHEN** a principal collection with direct subcollections is rendered
- **THEN** its eye control appears immediately after the collection name and
  before the row's badges
- **AND** the note control and actions-menu control appear after the badges
- **AND** it has localized action text and an `aria-pressed` state
- **AND** the state and action text describe whether shared captures are
  currently shown or will be hidden
- **AND** Historial, child collections and principals without children do
  not show the control

#### Scenario: Widen the collection and linked-team panel

- **WHEN** the desktop organization panel is rendered
- **THEN** the grid column containing collections and linked teams has a
  minimum width of `198px` and a maximum width of `242px`
- **AND** its mobile single-column layout remains unchanged

#### Scenario: Keep the choice local to the session

- **WHEN** the user switches away from a principal and selects it again
- **THEN** its visibility choice remains in effect for that session
- **WHEN** ClipVault starts a new session
- **THEN** shared captures are visible by default

### Requirement: Include subcollection captures in parent badge counts

The collection list SHALL show a numeric badge at the trailing end of every
collection row. The badge SHALL display the current capture total for that
collection's defined count scope, including zero. `Historial` SHALL count
every capture in local history. A top-level user collection SHALL count its
direct captures that are not associated with any direct subcollection, plus
the sum of each direct subcollection's count. A subcollection SHALL count only
its own distinct direct memberships. A capture associated with multiple
subcollections SHALL contribute once to each such subcollection and therefore
once per subcollection to the parent total. Counts SHALL include all capture
types and favorites and SHALL NOT be limited by the active query or the number
of rendered cards. The badge SHALL expose a localized, pluralized accessible
name.

#### Scenario: Show the total for Historial

- **WHEN** the collection list shows `Historial`
- **THEN** its badge shows the total number of persisted history captures,
  including text, rich-text and image captures

#### Scenario: Show a parent collection's subtree total

- **GIVEN** captures are associated directly with a top-level collection or
  with one or more of its direct subcollections
- **WHEN** the collection list shows that parent
- **THEN** its badge adds the direct captures that are not in any child to the
  count of each child
- **AND** a capture associated with multiple children contributes once for
  each child it belongs to
- **AND** a capture in both the parent and a child is counted through the
  child's count, not again as a parent-only capture

#### Scenario: Show a subcollection's direct membership count

- **WHEN** the collection list shows a subcollection
- **THEN** its badge counts the distinct captures associated directly with
  that subcollection, regardless of membership in its parent or siblings

#### Scenario: Show zero for an empty collection

- **WHEN** a collection has no captures represented by its count scope
- **THEN** its row displays a badge with `0` and an accessible zero-count label

#### Scenario: Refresh count after capture or membership changes

- **WHEN** a capture is added or removed, or its collection memberships are
  assigned or removed
- **THEN** the affected collection badges refresh from persisted metadata
  without restarting Desktop

#### Scenario: Refresh counts after clearing or deleting collections

- **WHEN** a parent or subcollection is cleared or deleted, or captures are
  deleted through that operation
- **THEN** the next organization snapshot reports counts from the remaining
  collection rows and memberships
- **AND** no deleted membership remains in a parent's total

#### Scenario: Refresh rows after collection changes

- **WHEN** a collection is created, modified or deleted
- **THEN** the collection list refreshes its rows and counts from the current
  organization snapshot
- **AND** a new empty collection initially displays `0`
- **AND** a renamed collection retains its count
- **AND** a deleted collection's row disappears

#### Scenario: Count does not depend on visible search results

- **GIVEN** a collection has captures outside the current query or card limit
- **WHEN** the collection list displays its badge
- **THEN** the badge still reports all persisted memberships in its count scope

#### Scenario: Collection row interactions remain intact

- **WHEN** the user selects, renames, opens actions for, or drags a capture
  over a collection row
- **THEN** the badge does not intercept the row's existing keyboard, pointer,
  menu or drop interactions

#### Scenario: Count projection remains metadata-only

- **WHEN** Desktop loads collection counts
- **THEN** the backend returns aggregate counts without loading capture content
  or issuing one count query per collection
- **AND** the snapshot, events and logs contain no capture content, hashes,
  snippets, paths or capture identifiers

### Requirement: Assign automatic captures to the active local collection

ClipVault SHALL associate each newly captured clipboard entry with `Historial`
and, when a local user collection or subcollection is active at capture time,
with that active collection as well. The rule SHALL apply to new entries and
to existing entries returned by deduplication. Adding the active membership
SHALL preserve all existing collection memberships.

#### Scenario: Capture while a local collection is active

- **GIVEN** a local user collection is selected
- **WHEN** ClipVault stores a new clipboard capture
- **THEN** the entry belongs to both `Historial` and the selected collection

#### Scenario: Capture while a subcollection is active

- **GIVEN** a local subcollection is selected
- **WHEN** ClipVault stores a new clipboard capture
- **THEN** the entry belongs to both `Historial` and that subcollection
- **AND** ClipVault does not implicitly add the entry to the parent collection

#### Scenario: Capture duplicates an existing entry

- **GIVEN** a local user collection or subcollection is active
- **AND** the clipboard capture resolves to an existing history entry
- **WHEN** ClipVault completes the capture
- **THEN** the existing entry belongs to the active collection as well as
  `Historial`
- **AND** its other collection memberships remain unchanged

#### Scenario: Capture while Historial is active

- **GIVEN** `Historial` is active or no local collection is selected
- **WHEN** ClipVault stores a clipboard capture
- **THEN** the entry belongs to `Historial`
- **AND** ClipVault does not add it to a user collection

#### Scenario: Capture while a linked team history is active

- **GIVEN** the user is viewing a linked team's history
- **WHEN** ClipVault stores a local clipboard capture
- **THEN** the entry belongs to `Historial`
- **AND** ClipVault does not add it to the previously selected local collection

#### Scenario: Capture while a peer-bound collection is selected

- **GIVEN** the selected collection is bound to a linked peer
- **WHEN** ClipVault stores a local clipboard capture
- **THEN** the entry belongs to `Historial`
- **AND** ClipVault does not add it to the peer-bound collection

#### Scenario: Active local collection was removed or is no longer eligible

- **GIVEN** the active collection ID no longer identifies an eligible local
  user collection
- **WHEN** ClipVault stores a clipboard capture
- **THEN** the entry remains stored in `Historial`
- **AND** the invalid additional membership is ignored

#### Scenario: Restart with no active collection

- **WHEN** ClipVault starts a new session
- **THEN** the active capture destination is empty until the user selects a
  local collection
- **AND** clipboard captures continue to belong to `Historial`

### Requirement: Track the active capture collection with the visible scope

ClipVault SHALL keep the active capture destination synchronized with the
visible local collection scope. Selecting a local collection or subcollection
SHALL set that destination; selecting `Historial` or opening a linked team's
history SHALL clear it. Closing the linked team's history SHALL restore the
previous local selection as the destination. This runtime selection SHALL NOT
be persisted across application restarts.

#### Scenario: Switch between local collection scopes

- **WHEN** the user selects a different local collection or subcollection
- **THEN** subsequent captures use that newly selected collection

#### Scenario: Restore local scope after closing a linked history

- **GIVEN** a local collection is selected
- **WHEN** the user opens and then closes a linked team's history
- **THEN** subsequent captures use the previously selected local collection
