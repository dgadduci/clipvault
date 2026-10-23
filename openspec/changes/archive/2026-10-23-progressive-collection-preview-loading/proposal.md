# Progressive collection preview loading

## Why

Selecting a local collection currently clears the old rail, fetches the new
entries, and then keeps the new cards hidden while every entry's tag and
collection membership is hydrated. Image bytes are already fetched
asynchronously by each card, but the user cannot see any of that progress
until the full refresh finishes.

## What Changes

- Reveal the newly selected collection's cards as soon as its entry snapshot
  arrives, paint the rail before secondary membership requests begin, and
  hydrate membership in small concurrent batches so eligible cards appear
  progressively. Image previews fill in as their per-card asset requests
  finish.
- Keep the collection loading spinner visible until the active collection's
  metadata hydration and visible image previews reach a terminal state.
- Keep the workspace non-interactive during a collection switch and ignore
  stale preview completions from superseded loads.

## Impact

- Affects the desktop collection-switch flow, history card/rail preview
  callbacks, and frontend loading-state tests.
- Does not require backend pagination, schema changes, new dependencies, or
  additional product text.
- Existing image asset references remain opaque and are not exposed through
  the new completion callback.
