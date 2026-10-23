# Tasks

## OpenSpec

- [x] 1.1 Document the current hidden-rail delay, progressive preview contract,
  interaction lock and stale-load behavior.

## Progressive preview completion

- [x] 2.1 Add a testable generation-scoped barrier for visible image preview
  completion, including cancellation and filtered-out entries.
- [x] 2.2 Have image cards report metadata-only ready/error completion through
  the rail without exposing asset references or image bytes.
- [x] 2.3 Reveal the current collection's cards as soon as its entry snapshot
  is published, paint the rail before secondary metadata requests and hydrate
  memberships progressively.
- [x] 2.4 Keep the workspace inert and keyboard shortcuts blocked while a
  collection switch is active; keep the existing spinner visible through
  metadata and visible image-preview completion.
- [x] 2.5 Preserve subcollection visibility and stale-request protections when
  deciding which previews the active load awaits.

## Regression and verification

- [x] 3.1 Add focused regressions for preview barrier completion, cancellation,
  first-frame rendering, progressive membership hydration and interaction lock.
- [x] 3.2 Run frontend check/build and card/collection drag-and-drop regressions.
- [x] 3.3 Run strict OpenSpec validation and `git diff --check`; review the
  complete diff and preserve all pre-existing local work.
