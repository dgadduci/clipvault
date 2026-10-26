# search-result-organization-hydration Specification

## Purpose

Ensure searched capture cards keep their tag and collection labels even when
they are outside the currently loaded base collection page.

## ADDED Requirements

### Requirement: Hydrate organization labels for visible search results

The desktop SHALL load each visible search result's tags and collection
memberships independently of subcollection visibility settings. A result
outside the base page of its collection SHALL keep its organization metadata
available while it is displayed. The card SHALL show the loading state while
metadata is being fetched and SHALL show the organization error only when the
metadata request fails, not because the result is absent from the base page.

#### Scenario: Search finds an entry outside the base collection page

- **WHEN** a search in a collection returns a capture that is not in the
  currently loaded collection page
- **THEN** its tags and collection labels are loaded for the card
- **AND** the card does not report an organization-loading error solely
  because the result is outside that base page

#### Scenario: Search results are hydrated with shared captures visible

- **WHEN** subcollection captures are configured to remain visible during a
  search
- **THEN** every result page still loads tag and collection labels for its
  cards

#### Scenario: Search page changes while filtering

- **WHEN** the user advances to another search-results page or a refresh
  hydrates the base collection page while a search is active
- **THEN** the organization metadata cache retains the currently displayed
  search results
- **AND** their labels remain visible until the search results change

#### Scenario: Organization metadata request fails

- **WHEN** a tag or collection membership request fails for a visible search
  result
- **THEN** the card shows its existing organization error state

### Requirement: Include subcollections in the History collection filter

The History collection filter SHALL offer every user collection, including
subcollections. A subcollection option SHALL identify its parent in the
displayed label while retaining the subcollection id as its filter value.

#### Scenario: Browse subcollections from History

- **WHEN** the collection filter is shown in Historial and user collections
  have subcollections
- **THEN** the filter offers both parent collections and their subcollections
- **AND** each subcollection label identifies its parent

#### Scenario: Filter History by a subcollection

- **WHEN** the user selects a subcollection in the History collection filter
- **THEN** the History list and search are restricted to captures assigned to
  that subcollection

### Requirement: Load tag filter options across the active collection scope

The tag filter SHALL offer tags associated with captures across the entire
active collection scope, independent of the currently visible page, search
query, selected tag, source-application filter, or content-type filter. In
Historial, the scope SHALL follow the selected collection or unassigned
filter; without either restriction it SHALL include all History captures.
Changing the pagination page SHALL NOT narrow the tag options.

#### Scenario: Tags exist only on later collection pages

- **WHEN** a tag is assigned only to captures beyond the currently visible
  page of the active collection
- **THEN** that tag remains available in the tag filter

#### Scenario: Tags are available across search results

- **WHEN** a search or another filter narrows the visible capture list
- **THEN** tag options continue to represent the full active collection scope

#### Scenario: History tag options follow its collection filter

- **WHEN** the user selects a collection or the unassigned option in the
  History collection filter
- **THEN** tag options include tags attached to all captures matching that
  collection scope, regardless of the current page
