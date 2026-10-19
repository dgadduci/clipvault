## ADDED Requirements

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
