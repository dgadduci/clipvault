## Context

QuickVault is a separate Tauri webview. It loads recent rows through the
existing Tauri recent-entries command and listens for the metadata-only
`clipvault://history-updated` event. Local capture and peer-import persistence
already have explicit commit boundaries; QuickVault must display the current
persisted result rather than infer row data from an event.

The current recent-order contract places favorites before non-favorites and
sorts only within each group. That conflicts with the requested strict
newest-to-oldest order whenever an older favorite precedes a newer capture.

## Decisions

### Recent-list ordering

Recent mode SHALL sort all returned entries by `created_at DESC`, then `id
DESC`, independent of `is_pinned`. The pin/favorite state remains available to
render its existing indicator and actions, but does not outrank a newer
capture. Search mode remains ordered by `SearchService` relevance and is not
re-sorted by date.

Use the persisted entry timestamps and the existing chronological comparator
where possible. Do not optimistically prepend an event payload or fabricate a
timestamp in the frontend; the history event remains metadata-only and the
recent command is the source of truth.

### Refresh after capture and import

After a local capture or a successful text/image peer-import transaction,
QuickVault refreshes its current data through its existing event/requery path.
Import notification must occur only after persistence commits; a failed or
rolled-back import must not be presented as a new row. If the user has an
active search, refresh its search results while preserving the existing
relevance ordering.

### Overlapping requests

Each recent refresh belongs to a monotonically increasing request generation
or equivalent stale-response guard. Only the latest applicable request may
replace visible rows. This covers the initial window load racing with an
`opened` or `history-updated` refresh. Existing stale-response protections for
search and source-app metadata remain intact.

### Timestamp and deduplication invariants

A newly inserted local capture or imported entry uses the timestamp assigned
by the existing persistence path. Do not rewrite `created_at` to force an
entry to the top. If an import deduplicates to an existing local entry, retain
that entry's original `created_at`; changing provenance or `last_seen_at`
alone does not make it a new capture. Equal timestamps use descending entry
ID for stable ordering.

### Protected behavior

Do not change clipboard contents, copy/paste behavior, entry identity,
deduplication, QuickVault recent/search modes beyond the ordering and refresh
contract, or the metadata-only event payload. Avoid touching the protected
desktop card drag-and-drop implementation.

## Verification

Use focused pure tests for strict date/ID ordering and frontend tests for
capture/import refreshes completing out of order. Verify local capture and
text/image import notifications refresh persisted results, that failed imports
do not create visible rows, and that search ranking, duplicate timestamps,
dedupe timestamps, selection and copy behavior remain unchanged. Run the
QuickVault checks/build and protected drag-and-drop regressions if the change
touches shared listeners or card/rail code.
