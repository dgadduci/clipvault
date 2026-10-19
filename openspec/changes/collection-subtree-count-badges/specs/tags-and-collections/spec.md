## ADDED Requirements

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
