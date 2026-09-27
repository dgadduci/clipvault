## MODIFIED Requirements

### Requirement: Order recent Quick Paste results chronologically

Quick Paste SHALL render its recent-mode result list in strict deterministic
chronological order across all entries: `created_at DESC`, then `id DESC` as
the tie-breaker. Favorite status SHALL NOT move an older entry ahead of a
newer entry. After a newly persisted local capture or successful text/image
peer import, an open Quick Paste window SHALL refresh from persisted rows and
show the newest entry in its chronological position without requiring the
window to be reopened. When refresh requests overlap, a stale response SHALL
NOT replace the result from the latest applicable refresh. Search-mode hits
SHALL retain the ranking returned by `SearchService` instead of being reordered
by date.

#### Scenario: A new local capture becomes the newest recent result

- **WHEN** Quick Paste is open in recent mode and a new local capture commits
- **THEN** the metadata-only history update causes Quick Paste to reload its
  persisted recent rows
- **AND** the new entry appears first when it has the newest `created_at`
- **AND** the remaining entries continue in `created_at DESC, id DESC` order

#### Scenario: A successful peer import becomes the newest recent result

- **WHEN** a text or image peer import commits a new local entry while Quick
  Paste is open in recent mode
- **THEN** Quick Paste refreshes its persisted recent rows after the commit
- **AND** the imported entry appears first when its local `created_at` is the
  newest
- **AND** a failed or rolled-back import does not appear as a new result

#### Scenario: An older refresh resolves after a newer refresh

- **WHEN** two recent-list requests overlap and the earlier request resolves
  after a later request
- **THEN** the earlier response is discarded and cannot move the newest entry
  away from its correct position

#### Scenario: Favorites do not override capture chronology

- **WHEN** an older entry is pinned and a newer entry is not pinned
- **THEN** the newer entry appears before the older pinned entry in recent mode
- **AND** the pin indicator and pin action remain available

#### Scenario: Recent favorites are newest first

- **WHEN** recent Quick Paste results contain multiple favorite entries
- **THEN** favorite entries appear from the most recently captured to the
  oldest, using descending entry ID only when capture timestamps tie

#### Scenario: Recent non-favorites are newest first

- **WHEN** recent Quick Paste results contain multiple non-favorite entries
- **THEN** non-favorite entries appear from the most recently captured to the
  oldest, using descending entry ID only when capture timestamps tie

#### Scenario: Equal timestamps have a deterministic order

- **WHEN** multiple recent entries have the same `created_at`
- **THEN** Quick Paste orders those entries by descending entry ID

#### Scenario: A deduplicated import preserves the original capture time

- **WHEN** a peer import resolves to an entry that already exists locally
- **THEN** the existing entry's `created_at` is not changed merely to promote
  it in Quick Paste
- **AND** recent mode continues to order it by that original timestamp

#### Scenario: Search ranking is preserved

- **WHEN** the user has an active Quick Paste search query and history changes
- **THEN** Quick Paste refreshes the search results without replacing
  `SearchService` ranking with chronological ordering

#### Scenario: Refresh and hydration do not scramble order

- **WHEN** Quick Paste refreshes after `history-updated`, completes metadata
  hydration, toggles a favorite, or clears the search query
- **THEN** recent mode remains in strict `created_at DESC, id DESC` order and
  search mode preserves `SearchService` ranking

#### Scenario: Copy does not change capture chronology

- **WHEN** the user copies an entry from Quick Paste without creating a new
  capture
- **THEN** the entry remains in its chronological position and its
  `created_at` value is not changed
