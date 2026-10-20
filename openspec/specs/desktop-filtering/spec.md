## Purpose

Definir el filtrado local de entradas del escritorio por aplicación fuente y tags
dentro del ámbito activo.

## Requirements

### Requirement: Filter Desktop entries by tag in the active scope

Desktop SHALL provide an accessible tag combobox between the source-application
filter and the overflow menu. Its first option SHALL be `Todas`. The options
and filtering scope MUST match the active History or collection view. Selecting
a tag MUST filter immediately without an Apply button.

#### Scenario: Tag filter lists History tags

- **WHEN** the active scope is History and the tag filter is opened
- **THEN** it lists the tags present in visible History entries and starts with
  `Todas`

#### Scenario: Tag filter lists collection tags

- **WHEN** a user collection is active and the tag filter is opened
- **THEN** it lists only tags relevant to that collection's entries and does not
  include tags from unrelated scopes

#### Scenario: Selecting a tag filters immediately

- **WHEN** the user selects a tag
- **THEN** the Desktop cards update immediately to entries in the active scope
  associated with that tag, without a separate confirmation button

#### Scenario: Source-app and tag filters combine

- **WHEN** both a source application and a tag are selected
- **THEN** only entries matching both filters remain visible

#### Scenario: All tags clears only the tag filter

- **WHEN** the user selects `Todas`
- **THEN** the tag constraint is removed while the active scope, search query
  and source-application filter retain their existing values

#### Scenario: Scope changes reset invalid tag selection

- **WHEN** the user changes collection and the selected tag is not present in the
  new scope
- **THEN** the tag filter resets safely to `Todas` and the new scope is shown

#### Scenario: Refresh does not install duplicate listeners

- **WHEN** the history-updated event refreshes entries or tag options
- **THEN** the tag filter stays synchronized with the active scope without
  duplicate event listeners or stale responses

### Requirement: Filter captures by their content type

The desktop SHALL render one capture-type filter in Historial and every local
collection. The filter SHALL offer «Todas» and the capture types available in
the active scope, with the same type icons and translated labels used by
cards. It SHALL NOT render duplicate capture-type selectors.
Selecting a type SHALL combine with the active collection, source-application
filter, tag filter and search query for both the card rail and search results.
Changing collection scope SHALL reset the selected type to «Todas». The filter
options SHALL be derived from all entries in the active collection scope, not
from the paginated card rail.

#### Scenario: Filter offers the types available in each scope

- **WHEN** the user opens the capture-type filter in Historial or a collection
- **THEN** the filter offers «Todas» and each capture type present
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

### Requirement: Reset search and toolbar filters when switching collections

When the user switches the active collection from the sidebar, Desktop SHALL
clear the search text and reset the source-application, tag and History
collection filters to `Todas`. Desktop SHALL cancel and invalidate any
in-flight search so its results cannot replace entries from the destination
collection. The newly selected collection SHALL remain the active scope, and
Desktop SHALL reload its application options and entries without the previous
query or filters.

#### Scenario: Switch collection with search and filters active

- **GIVEN** a search query and one or more toolbar filters are selected
- **WHEN** the user selects another collection from the sidebar
- **THEN** the search field is blank
- **AND** the source-application, tag and History collection filters show
  `Todas`
- **AND** the destination collection remains selected and its unfiltered
  entries are loaded

#### Scenario: Ignore a search response from the previous collection

- **GIVEN** a search request is still in flight for the current collection
- **WHEN** the user switches to another collection
- **THEN** Desktop cancels or invalidates the pending search
- **AND** a late response from the old collection cannot replace the new
  collection's entries

#### Scenario: Change a toolbar filter without switching collections

- **WHEN** the user changes a toolbar filter while the active sidebar
  collection remains the same
- **THEN** the collection switch reset does not run
- **AND** the current query and other toolbar filters retain their values

### Requirement: Active toolbar filters have distinct visual states

Desktop SHALL show a semi-transparent, category-specific background and white
text when a toolbar filter has a selected value. Source-application, tag,
collection and capture-type filters SHALL use blue, violet, teal and amber
surfaces `rgba(37, 99, 235, alpha)`, `rgba(109, 40, 217, alpha)`,
`rgba(15, 118, 110, alpha)` and `rgba(180, 83, 9, alpha)`, respectively.
Each surface SHALL use the same alpha as collection-row tints
(`COLLECTION_ROW_TINT_OPACITY`, currently `0.18`). The corresponding active
filter icon SHALL also be white where it follows the text color. Unselected
filters and the `Todas` option SHALL retain the neutral background.

#### Scenario: Selected filters use distinct colors

- **GIVEN** one or more toolbar filters have selected values
- **WHEN** the toolbar is displayed
- **THEN** each selected source-application, tag, collection or capture-type
  control uses its defined color surface and white text
- **AND** simultaneous active filters remain distinguishable by hue

#### Scenario: Clearing a filter restores its neutral appearance

- **WHEN** the user selects `Todas` or clears an active filter
- **THEN** that control returns to the neutral background and default text
  color
- **AND** other selected filters keep their category-specific active colors
