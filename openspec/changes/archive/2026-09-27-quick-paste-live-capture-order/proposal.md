## Why

When QuickVault is open, a newly captured item or a capture imported from
another ClipVault can remain below older rows or fail to appear at the top.
This breaks the expected newest-to-oldest date order. A refresh can race with
an earlier request, and the current favorites-first grouping can also place an
older pinned row ahead of a newer capture.

## What Changes

- Make QuickVault's recent list globally chronological by `created_at DESC`,
  with `id DESC` as a deterministic tie-breaker; favorite status does not
  override that order.
- Ensure a successful local capture or text/image peer import refreshes the
  open QuickVault recent list from persisted rows, so the newest entry appears
  first without reopening the window.
- Prevent an older in-flight refresh from replacing results from a newer
  refresh.
- Preserve search ranking, deduplication, immutable capture timestamps,
  selection/copy behavior, and metadata-only event privacy.

## Capabilities

### Modified Capabilities

- `quick-paste`: recent entries use strict chronological order and refresh
  reliably after newly persisted local captures and peer imports.

## Impact

- QuickVault frontend refresh and recent-list ordering.
- Existing metadata-only history-update event paths for local capture and
  successful peer import, if an affected path does not currently notify the
  QuickVault webview.
- Focused frontend tests; no database migration or new network protocol is
  expected.
