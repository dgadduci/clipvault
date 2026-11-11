# desktop-preview-pagination Specification

## Purpose
Define configurable page sizing for local capture collections and linked
device import previews.

## Requirements

### Requirement: Paginate local history and collections

The desktop SHALL paginate captures in every local collection, including
Historial, using the configured preview page size. Pagination SHALL apply
after the active search and collection, tag, source-application, content-type,
and unassigned-history filters. It SHALL preserve the existing newest-first
ordering and subcollection visibility rules. Preview cards SHALL remain in a
single horizontal row with horizontal scrolling; pagination controls SHALL
remain outside the card rail.

#### Scenario: Navigate the default history page

- **WHEN** Historial contains more captures than the configured page size
- **THEN** the desktop shows no more than that page size and enables the next
  page control
- **AND** the user can return to the preceding page

#### Scenario: Paginate a local collection

- **WHEN** a local collection contains more captures than the configured page
  size
- **THEN** the collection shows its captures in newest-first pages
- **AND** the page controls expose the next page only when more matching
  captures exist

#### Scenario: Keep local preview cards horizontal

- **WHEN** a page contains multiple captures
- **THEN** the preview cards appear in one horizontal row
- **AND** additional cards remain reachable by horizontal scrolling
- **AND** the row does not wrap into vertical columns

#### Scenario: Paginate filtered or searched captures

- **WHEN** a search or supported filter is active in Historial or a local
  collection
- **THEN** each page contains matching captures only
- **AND** changing the collection, filter, or search starts at the first page

### Requirement: Configure and persist the shared preview page size

General Settings SHALL let the user set the preview page size from 1 through
50. The value SHALL persist locally and apply to both local collection pages
and linked device import previews. The default SHALL be 20 when no valid
stored setting exists.

#### Scenario: Fresh installation uses 20 previews

- **WHEN** no valid preview page size has been stored
- **THEN** local collections, Historial, and linked device import previews use
  20 previews per page

#### Scenario: User changes page size

- **WHEN** the user saves a valid page size in General Settings
- **THEN** the new value is persisted and used by local collections and
  linked device import previews
- **AND** the active view restarts at its first page

#### Scenario: Invalid page size is rejected

- **WHEN** a value below 1 or above 50 is submitted
- **THEN** Settings rejects the update and retains the previously stored
  value

### Requirement: Apply the configured size to linked device import previews

Linked device import previews SHALL use the configured page size across the
existing independently paginated text and image streams. Combining those
streams SHALL produce no more than the configured number of visible rows per
page and SHALL surface buffered rows before requesting another host page.

#### Scenario: Combined remote page respects configured size

- **WHEN** text and image import streams together return more rows than the
  configured size
- **THEN** the visible page contains at most the configured number of rows
- **AND** the remaining rows are available on subsequent pages without being
  skipped or duplicated
