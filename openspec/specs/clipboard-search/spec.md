## Purpose

Definir la búsqueda local, fuzzy y determinística sobre el historial textual de ClipVault.

## Requirements

### Requirement: Search text history locally

ClipVault SHALL provide local full-text and fuzzy search over stored text entries without embeddings, semantic search or network calls.

#### Scenario: Exact text query

- **WHEN** a user enters a query matching text in an entry
- **THEN** matching entries are returned without requiring an exact case match

#### Scenario: Fuzzy text query

- **WHEN** a user enters a partial or slightly misspelled query
- **THEN** relevant entries with approximate matches are returned when the search index supports them

#### Scenario: No matches

- **WHEN** a query matches no stored entries
- **THEN** the interface shows an empty result state and remains usable for a new query

### Requirement: Deterministic result ranking

Search results SHALL use a deterministic ranking that considers match quality and may incorporate recency, usage frequency and favorite state.

#### Scenario: Exact match versus partial match

- **WHEN** the result set contains an exact match and a partial match
- **THEN** the exact match ranks above the partial match when other factors do not outweigh it

#### Scenario: Equal match quality

- **WHEN** two entries have equivalent match quality
- **THEN** the more recent or more frequently used entry ranks first according to the documented weighting

### Requirement: Search remains local and responsive

The search operation SHALL run against the local data store or local index and SHALL not block clipboard capture or require Internet access.

#### Scenario: Offline search

- **WHEN** the computer is offline
- **THEN** search returns results from the local history normally

#### Scenario: Search while capture is active

- **WHEN** a user searches while new clipboard events arrive
- **THEN** the application remains responsive and preserves both the query state and captured entries

### Requirement: Local full-text, title and fuzzy search

ClipVault SHALL provide a local, in-process search over every currently supported entry type. Textual entries SHALL be searchable by their canonical text content and by their persisted custom card title; non-textual entries SHALL be searchable by their persisted custom card title without inspecting asset bytes. The search SHALL support exact phrase matches, per-token substring matches and bounded fuzzy matches, returning deterministic ranked results without network or external services.

#### Scenario: Exact phrase query

- **WHEN** the user enters a query that matches text in an entry
- **THEN** ClipVault returns the matching entry above partial matches, case-insensitively and without modifying the entry, its timestamps or its favorite state

#### Scenario: Per-token substring query

- **WHEN** the user enters a multi-token query and every token appears as a substring of an entry's content
- **THEN** ClipVault returns the entry ranked below exact phrase matches but above fuzzy-only matches

#### Scenario: Bounded fuzzy query

- **WHEN** the user enters a query whose tokens are within the configured edit distance of tokens in an entry's content
- **THEN** ClipVault returns the entry only when the similarity is sufficient, and never returns tokens whose length is three characters or fewer without an exact match

#### Scenario: Empty or whitespace-only query

- **WHEN** the user submits an empty query or a query containing only whitespace
- **THEN** ClipVault returns no hits, surfaces a stable `empty_query` note, and does not iterate stored entries

#### Scenario: No matches

- **WHEN** the query produces no eligible match
- **THEN** ClipVault returns an empty hit list with the `empty_query` note absent and the interface remains usable for a new query

#### Scenario: Text title-only query in Desktop

- **WHEN** the user searches Desktop for a custom card title that does not occur in the textual content of the entry
- **THEN** the matching entry appears in the current Desktop scope

#### Scenario: Text title-only query in Quick Paste

- **WHEN** the user searches Quick Paste for a custom card title that does not occur in the textual content of the entry
- **THEN** the matching entry appears in Quick Paste through the existing local search command and remains selectable

#### Scenario: Image title-only query

- **WHEN** an image entry has a custom title and the query matches that title but the entry has no textual content
- **THEN** the image entry appears in the result with its existing image metadata and asset reference unchanged, and no image bytes are read by the search engine

#### Scenario: Content query remains available

- **WHEN** the query matches canonical content but does not match the custom title
- **THEN** the entry appears with the existing content-search behavior

#### Scenario: Missing or blank title

- **WHEN** an entry has no custom title or its title is whitespace-only
- **THEN** the title contributes no match and the entry is searched only by its eligible canonical textual content

#### Scenario: Search is restricted to the active scope

- **WHEN** the user searches while Historial or a user collection is active
- **THEN** only entries belonging to that existing scope and its existing tag and source-application filters are eligible for the result

### Requirement: Deterministic mixed-field ranking

ClipVault SHALL preserve the existing deterministic ranking for content and SHALL rank title-only matches below content matches. Exact phrase matches SHALL rank above all-token substring matches, which SHALL rank above fuzzy matches within each field; recency and entry id SHALL remain stable tie-breakers.

#### Scenario: Quality tiers

- **WHEN** multiple entries match the query with different qualities
- **THEN** exact phrase matches rank above all-token substring matches, which rank above fuzzy-only matches

#### Scenario: Recency tiebreak

- **WHEN** two entries share the same match quality
- **THEN** the entry with the more recent `updated_at` ranks first

#### Scenario: Stable tiebreak

- **WHEN** two entries still tie after quality and recency
- **THEN** the entry with the higher `id` ranks first and the ordering is identical across repeated runs

#### Scenario: Content match outranks title-only match

- **WHEN** one entry matches the query in content and another entry matches only in its custom title
- **THEN** the content match ranks first regardless of the title match tier

#### Scenario: Equal title quality uses existing tie-breakers

- **WHEN** multiple entries match only through custom titles with equal match quality
- **THEN** the more recent entry ranks first, followed by higher id when timestamps are equal

#### Scenario: Repeated query is stable

- **WHEN** Desktop or Quick Paste executes the same query against the same snapshot more than once
- **THEN** it returns the same entry order and scores

### Requirement: Search does not perturb other subsystems

ClipVault SHALL keep clipboard capture, history persistence, favorites and the OS clipboard untouched while a search query is running.

#### Scenario: Search while capture is active

- **WHEN** a user searches while new clipboard events arrive
- **THEN** ClipVault returns the search result from a stable snapshot, continues to capture new entries, and never deletes or rewrites existing entries during the query

#### Scenario: Search leaves history intact

- **WHEN** a search query completes
- **THEN** no entry, timestamp, favorite or clipboard payload is changed by the search itself

### Requirement: Bounded result set

ClipVault SHALL bound the number of search hits returned to a configurable limit with a stable maximum.

#### Scenario: Default and maximum limits

- **WHEN** the frontend calls `clipvault_search_entries` without an explicit limit
- **THEN** ClipVault applies a deterministic default limit and never returns more than 500 hits per call, mirroring `clipvault_recent_entries`

#### Scenario: Caller-supplied limit is clamped

- **WHEN** the frontend supplies a limit greater than 500 or smaller than 1
- **THEN** ClipVault clamps the limit to the supported range and still returns a deterministic ordering

### Requirement: Local search without sensitive payload leakage

ClipVault SHALL run search entirely against the local SQLite history and SHALL NOT log full query text, snippet text or full entry content.

#### Scenario: Query and snippets stay local

- **WHEN** a search query or snippet is computed
- **THEN** the result is produced from local data only, never sent to a network endpoint, and the local logs do not contain the full query text, the full snippet or the full entry content

### Requirement: Filter local search by organization metadata

The existing local search SHALL accept an optional collection filter, zero or
more tag filters and an optional source-application filter. Collection
filtering SHALL select one collection at a time, multiple tags SHALL use AND
semantics, and the source-application filter SHALL match the stable persisted
`source_app` identifier or the explicit unknown-source state. Existing ranking,
fuzzy matching, limits and offline guarantees SHALL remain unchanged.

#### Scenario: Collection and source application filter

- **WHEN** search runs in collection `Trabajo` with source application
  `com.example.Editor`
- **THEN** only entries associated with `Trabajo` and that exact source
  application are eligible for the existing deterministic ranking

#### Scenario: Multiple organization filters

- **WHEN** search runs with collection `Trabajo`, tags `código` and `pendiente`,
  and source application `com.example.Editor`
- **THEN** an entry is eligible only when it belongs to the collection, carries
  every requested tag and has the requested source application

#### Scenario: Unknown source application filter

- **WHEN** the user selects the unknown-application option
- **THEN** only entries whose persisted source application identifier is null
  or empty are eligible, without treating the state as the `Todas` state

#### Scenario: All source applications

- **WHEN** the source-application filter is `Todas`
- **THEN** the search applies the active collection and tag filters but imposes
  no source-application restriction

#### Scenario: Historial with source application filter

- **WHEN** search runs with the system collection `Historial` and a known
  source application
- **THEN** all history entries from that application are eligible regardless
  of their secondary collection memberships

#### Scenario: Empty organization filters preserve behavior

- **WHEN** no collection, tag or source-application filters are provided
- **THEN** search behaves exactly as the existing global history search

#### Scenario: Search remains local and bounded

- **WHEN** any source-application filter is applied online or offline
- **THEN** the result is computed locally, respects the existing result limit,
  and no query, source identifier, name or entry content is sent to a remote
  service

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
