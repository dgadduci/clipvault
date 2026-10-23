# Design: progressive desktop collection previews

## Current behavior

`selectCollectionById` clears the old cards and starts the collection loading
indicator. `refreshEntries` fetches entry records, note IDs and available
content types together. It publishes the new records, then awaits imported
source-app metadata and `hydrateEntryOrganization` for the entire result. The
rail remains `visibility: hidden` until `refreshEntries` finishes. Each image
card already requests its asset asynchronously and shows a loading placeholder,
but those states are hidden behind the stale-content guard.

## Decision

Keep the existing single metadata query. Once its response is current, publish
the entry list and release the stale-content guard before awaiting secondary
metadata. Paint one frame with the new cards and spinner before starting
membership requests. The existing card asset resolver then loads image previews
in parallel, with each card reporting only its entry ID and the active preview
generation when the image reaches loaded or error state.

For a collection switch, membership hydration uses a worker pool of at most
eight entries and applies each result as it arrives. This avoids queuing all
membership bridge calls at once and lets the parent collection's subcollection
filter reveal eligible cards progressively. Other hydration callers keep the
existing all-at-once behavior.

Successful previews signal readiness after the browser loads the image from
its local blob URL. Bridge failures, invalid assets and browser decode errors
signal readiness after the card switches to its existing fallback.

The desktop owns a generation-scoped barrier for renderable image IDs. After
entry organization has loaded, it removes entries excluded by the selected
subcollection visibility rule and waits only for previews that can appear in
the rail. Missing or corrupt assets count as terminal because the card already
shows its accessible fallback. A newer collection load cancels the older
barrier so stale completions cannot settle the current load.

During a collection switch, the desktop workspace remains inert and a
window-capture keyboard guard suppresses global shortcuts. A separate live
status remains available to assistive technology while the spinner is inside
the inert workspace. The existing collection spinner stays visible through
entry hydration and image-preview completion. The lock is released in the
collection-switch `finally` path, including failures.

## Alternatives

- Backend cursor pagination is unnecessary: the delay comes from keeping
  already-fetched cards hidden during frontend metadata hydration, while image
  bytes already load independently per card.
- Removing the spinner would allow interaction while the selected scope and
  its metadata are still settling, contrary to the requested behavior.
- Showing only skeleton cards would not reveal text previews or images that
  have already finished loading.

## Verification

- Unit-test the preview barrier for completion, filtered-out IDs, duplicate
  completion events and cancellation.
- Add a focused frontend contract regression for publishing the rail before
  metadata hydration while preserving the spinner and interaction lock.
- Run the frontend check/build and protected collection/card drag-and-drop
  regressions, strict OpenSpec validation and `git diff --check`.
