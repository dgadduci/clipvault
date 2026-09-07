## MODIFIED Requirements

### Requirement: Local full-text, title and fuzzy search

ClipVault SHALL provide a local, in-process search over every currently
supported entry type. Textual entries SHALL be searchable by their canonical
text content and by their persisted custom card title; non-textual entries
SHALL be searchable by their persisted custom card title without inspecting
asset bytes. The search SHALL support exact phrase matches, per-token
substring matches and bounded fuzzy matches, returning deterministic ranked
results without network or external services.

#### Scenario: Text title-only query in Desktop

- **WHEN** the user searches Desktop for a custom card title that does not
  occur in the textual content of the entry
- **THEN** the matching entry appears in the current Desktop scope

#### Scenario: Text title-only query in Quick Paste

- **WHEN** the user searches Quick Paste for a custom card title that does not
  occur in the textual content of the entry
- **THEN** the matching entry appears in Quick Paste through the existing
  local search command and remains selectable

#### Scenario: Image title-only query

- **WHEN** an image entry has a custom title and the query matches that title
  but the entry has no textual content
- **THEN** the image entry appears in the result with its existing image
  metadata and asset reference unchanged, and no image bytes are read by the
  search engine

#### Scenario: Content query remains available

- **WHEN** the query matches canonical content but does not match the custom
  title
- **THEN** the entry appears with the existing content-search behavior

#### Scenario: Missing or blank title

- **WHEN** an entry has no custom title or its title is whitespace-only
- **THEN** the title contributes no match and the entry is searched only by
  its eligible canonical textual content

#### Scenario: Search is restricted to the active scope

- **WHEN** the user searches while Historial or a user collection is active
- **THEN** only entries belonging to that existing scope and its existing tag
  and source-application filters are eligible for the result

### Requirement: Deterministic mixed-field ranking

ClipVault SHALL preserve the existing deterministic ranking for content and
SHALL rank title-only matches below content matches. Exact phrase matches SHALL
rank above all-token substring matches, which SHALL rank above fuzzy matches
within each field; recency and entry id SHALL remain stable tie-breakers.

#### Scenario: Content match outranks title-only match

- **WHEN** one entry matches the query in content and another entry matches
  only in its custom title
- **THEN** the content match ranks first regardless of the title match tier

#### Scenario: Equal title quality uses existing tie-breakers

- **WHEN** multiple entries match only through custom titles with equal match
  quality
- **THEN** the more recent entry ranks first, followed by higher id when
  timestamps are equal

#### Scenario: Repeated query is stable

- **WHEN** Desktop or Quick Paste executes the same query against the same
  snapshot more than once
- **THEN** it returns the same entry order and scores

### Requirement: Shared title-search command path

Desktop and Quick Paste SHALL reuse the existing `SearchService` and
`clipvault_search_entries` command path. Neither frontend SHALL implement a
second title filter or discard hits because the match came from the title.

#### Scenario: Both surfaces consume the same result contract

- **WHEN** a title-only query is executed from Desktop and from Quick Paste
- **THEN** both surfaces receive the same matching entry ids and ranking for
  the same scope, subject only to their existing limits and presentation

#### Scenario: Search changes do not mutate entries

- **WHEN** a title or content search completes
- **THEN** no clipboard payload, title, timestamp, favorite, tag, collection,
  source-application metadata or image asset is modified

## UNCHANGED Requirements

The existing empty-query note, no-match state, limits, debounce, cancellation,
stale-response protection, local-only execution, privacy rules and active-scope
filters remain in force. Existing title editing, image loading, Quick Paste
selection and drag-and-drop contracts remain unchanged.
