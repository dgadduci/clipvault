## MODIFIED Requirements

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
