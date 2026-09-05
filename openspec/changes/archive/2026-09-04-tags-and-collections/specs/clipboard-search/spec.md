## ADDED Requirements

### Requirement: Filter local search by organization metadata

The existing local search SHALL accept an optional collection filter and zero
or more tag filters. Collection filtering SHALL select one collection at a
time, while multiple tags SHALL use AND semantics. Existing ranking, fuzzy
matching, limits and offline guarantees SHALL remain unchanged.

#### Scenario: Collection filter

- **WHEN** search runs with collection `Trabajo`
- **THEN** only entries associated with that collection are eligible for the
  existing deterministic ranking

#### Scenario: Multiple tag filters

- **WHEN** search runs with tags `código` and `pendiente`
- **THEN** an entry is eligible only when it has both tags

#### Scenario: Historial filter

- **WHEN** search runs with the system collection `Historial`
- **THEN** all entries in local history are eligible regardless of secondary
  collection memberships

#### Scenario: Organization filters are empty

- **WHEN** no collection or tag filters are provided
- **THEN** search behaves exactly as the existing global history search

#### Scenario: Search stays local

- **WHEN** organization filters are applied online or offline
- **THEN** the result is computed locally and no query, tag name or entry
  content is sent to a remote service
