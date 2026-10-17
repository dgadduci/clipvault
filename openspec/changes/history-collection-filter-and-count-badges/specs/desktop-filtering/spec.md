## ADDED Requirements

### Requirement: Filter Historial by collection from the desktop toolbar

When `Historial` is the active local view, Desktop SHALL provide an accessible
single-select collection filter alongside the existing source-application and
tag filters. The selector SHALL list `Todas` first, followed by the available
user collections in sidebar order. Selecting a collection SHALL immediately
filter History entries by membership in that collection. The collection
predicate SHALL combine with the current search query, source-application
filter and tag filter without clearing or changing them.

#### Scenario: Collection options are available in Historial

- **WHEN** the user opens the collection filter while `Historial` is active
- **THEN** the selector lists `Todas` and the available user collections by
  display name in sidebar order
- **AND** it does not list the system `Historial` collection as a redundant
  filter option

#### Scenario: Filter History by one collection

- **WHEN** the user selects `Trabajo` in the collection filter
- **THEN** only entries associated with `Trabajo` remain in the History rail
- **AND** the search query, application filter and tag filter remain active

#### Scenario: Collection filter combines with existing filters

- **WHEN** a collection, a source application, a tag and a text query are
  selected
- **THEN** only entries matching all active predicates are shown

#### Scenario: Clear only the collection filter

- **WHEN** the user selects `Todas`
- **THEN** the collection predicate is removed
- **AND** the search query, source-application filter and tag filter retain
  their current values

#### Scenario: Collection options refresh after collection changes

- **WHEN** the user creates or modifies a collection while Desktop is open
- **THEN** the collection filter options reflect the updated organization
  snapshot without restarting the application
- **AND** modifying a selected collection keeps that selection by its stable ID
- **AND** renaming a selected collection displays its new name

#### Scenario: Deleted selected collection clears the filter

- **GIVEN** the collection filter selects `Trabajo`
- **WHEN** `Trabajo` is deleted
- **THEN** the option disappears and the filter returns to `Todas`
- **AND** the History rail no longer applies the deleted collection ID

#### Scenario: Collection filter resets outside Historial

- **WHEN** the user navigates from `Historial` to a different collection
- **THEN** the collection filter resets to `Todas` and does not narrow the
  selected collection's existing view
- **AND** navigating back to `Historial` starts without a stale collection
  filter

#### Scenario: Collection filter refresh avoids stale responses

- **WHEN** collection options refresh while a previous refresh is still in
  flight
- **THEN** the latest organization snapshot controls the options and no
  duplicate event listener is installed

#### Scenario: Existing filter behavior remains available

- **WHEN** the collection filter is added to the toolbar
- **THEN** source-application and tag filters retain their existing options,
  keyboard behavior and immediate application
- **AND** clearing the collection filter does not clear either existing filter
