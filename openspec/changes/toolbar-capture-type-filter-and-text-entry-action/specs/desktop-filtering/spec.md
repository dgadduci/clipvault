## ADDED Requirements

### Requirement: Filter captures by their content type

The desktop SHALL preserve its existing capture-type filter in Historial and
every local collection. The filter SHALL offer «Todas» and the capture types
available in the active scope, with the same type icons used by cards.
Selecting a type SHALL combine with the active collection, source-application
filter, tag filter and search query for both the card rail and search results.
Changing collection scope SHALL reset the selected type to «Todas». The
toolbar SHALL NOT render a duplicate capture-type selector.

#### Scenario: Existing filter offers the types available in each scope

- **WHEN** the user opens the capture-type filter in Historial or a collection
- **THEN** the existing filter offers «Todas» and each capture type present
  in that scope
- **AND** every type option uses the existing card type icon and label
- **AND** no second capture-type select is rendered

#### Scenario: Type selection filters the rail

- **GIVEN** the active scope contains captures with different content types
- **WHEN** the user selects one type
- **THEN** the rail shows only captures of that type within the active scope
- **AND** the source-application and tag selections remain applied

#### Scenario: Type selection composes with search

- **GIVEN** the user has selected a content type and entered a search query
- **WHEN** search results are loaded
- **THEN** matching results have the selected type and satisfy the other
  active filters

#### Scenario: A type is absent from the active collection

- **WHEN** the user selects a valid type with no captures in the active scope
- **THEN** the normal empty-results state is shown without an error

#### Scenario: Collection change resets the type filter

- **GIVEN** the user selected a content type
- **WHEN** the user changes to another collection or Historial
- **THEN** the filter returns to «Todas» and the new scope loads unfiltered by
  content type
