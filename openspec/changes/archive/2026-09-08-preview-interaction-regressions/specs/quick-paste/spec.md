## ADDED Requirements

### Requirement: Order recent Quick Paste results chronologically

Quick Paste SHALL render its recent-mode result list in deterministic
chronological order. The existing favorites-first behavior SHALL remain: the
favorite group comes first, and both the favorite and non-favorite groups SHALL
be ordered by capture time descending (`created_at DESC`) with `id DESC` as the
tie-breaker. Search-mode hits SHALL retain the ranking returned by
`SearchService` instead of being reordered by date.

#### Scenario: Recent favorites are newest first

- **WHEN** recent Quick Paste results contain multiple favorite entries
- **THEN** the favorite entries appear from the most recently captured to the
  oldest, using the entry id only when capture timestamps tie

#### Scenario: Recent non-favorites are newest first

- **WHEN** recent Quick Paste results contain multiple non-favorite entries
- **THEN** the non-favorite entries appear from the most recently captured to
  the oldest, using the entry id only when capture timestamps tie

#### Scenario: Search ranking is preserved

- **WHEN** the user has an active Quick Paste search query
- **THEN** the list preserves the ranking and tie-breaks returned by
  `SearchService` rather than replacing relevance with chronological order

#### Scenario: Refresh and hydration do not scramble order

- **WHEN** Quick Paste refreshes after `history-updated`, completes metadata
  hydration, toggles a favorite or clears the search query
- **THEN** the visible list remains deterministic and follows the applicable
  recent or search ordering contract

#### Scenario: Copy does not change capture chronology

- **WHEN** the user copies an entry from Quick Paste without creating a new
  capture
- **THEN** the entry remains in its chronological position and its
  `created_at` value is not changed
