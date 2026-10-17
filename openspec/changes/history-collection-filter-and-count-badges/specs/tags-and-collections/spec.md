## ADDED Requirements

### Requirement: Show capture counts at the end of collection rows

The collection list SHALL show a numeric badge at the trailing end of every
collection row. The badge SHALL display the current total number of distinct
stored captures associated with that collection, including zero. `Historial`
SHALL count every capture in local history. A user collection SHALL count its
own memberships. Counts SHALL include all capture types and favorites and
SHALL NOT be limited by the active query or the number of rendered cards. The
badge SHALL expose a localized, pluralized accessible name.

#### Scenario: Show the total for Historial

- **WHEN** the collection list shows `Historial`
- **THEN** its badge shows the total number of persisted history captures,
  including text, rich-text and image captures

#### Scenario: Show a user collection's membership count

- **WHEN** the collection list shows a user collection with associated
  captures
- **THEN** its badge shows the number of distinct captures associated with
  that collection, including favorites
- **AND** a capture shared with several collections contributes once to each
  collection's count

#### Scenario: Show zero for an empty collection

- **WHEN** a collection has no associated captures
- **THEN** its row displays a badge with `0` and an accessible zero-count label

#### Scenario: Refresh count after capture or membership changes

- **WHEN** a capture is added or removed, or its collection memberships are
  assigned or removed
- **THEN** the affected collection badges refresh from persisted metadata
  without restarting Desktop

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
- **THEN** the badge still reports all persisted memberships in that
  collection

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
