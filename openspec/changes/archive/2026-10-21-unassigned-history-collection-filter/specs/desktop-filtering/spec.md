## ADDED Requirements

### Requirement: Filter Historial captures with no user collection

When Historial is active, the Desktop collection filter SHALL offer a
`Sin colección` option in addition to `Todas` and user collections. Selecting
`Sin colección` SHALL show only captures with no membership in any user
collection. The automatic membership in the system `Historial` collection
SHALL NOT count as a user collection membership. The filter SHALL combine with
the search query, source-application, tag and content-type filters without
changing those selections or persisting any membership changes.

#### Scenario: Show captures that belong only to Historial

- **GIVEN** a capture has no membership in a user collection
- **WHEN** the user selects `Sin colección` in Historial
- **THEN** the capture remains visible even though it belongs to the system
  `Historial` collection

#### Scenario: Exclude captures assigned to a user collection

- **GIVEN** a capture belongs to a user collection
- **WHEN** the user selects `Sin colección` in Historial
- **THEN** the capture is not shown
- **AND** removing its user-collection membership makes it eligible again

#### Scenario: Compose the unassigned filter with other filters

- **WHEN** the user selects `Sin colección` together with a text query, source
  application, tag or content type
- **THEN** the History rail shows only captures that satisfy every selected
  filter
- **AND** changing the collection filter does not clear the other selections

### Requirement: Keep the collection filter visible outside Historial

The Desktop collection filter SHALL remain visible in the toolbar when a
user collection is active, in the same position used in Historial. While a
collection other than Historial is active, the control SHALL be disabled and
SHALL NOT change the active collection scope or its results. Returning to
Historial SHALL enable the control with the collection-filter state reset to
`Todas`.

#### Scenario: Collection filter is disabled in a user collection

- **WHEN** the user navigates from Historial to a user collection
- **THEN** the collection filter remains visible in the toolbar
- **AND** its native control is disabled
- **AND** the active collection's results remain governed by that collection

#### Scenario: Collection filter becomes available again in Historial

- **WHEN** the user navigates from a user collection back to Historial
- **THEN** the collection filter is enabled
- **AND** its selection is `Todas`
