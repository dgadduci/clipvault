# Tasks

## OpenSpec

- [x] 1.1 Document standalone image scrolling, zoom sizing, aspect-ratio, and
  embedded-preview boundaries.

## Implementation

- [x] 2.1 Make standalone image dimensions reflect the active zoom factor and
  allow overflow in both axes within the preview body.
- [x] 2.2 Keep a fitting image centered and preserve fixed header controls and
  embedded preview behavior.
- [x] 2.3 Add focused frontend regressions for two-axis scrolling and
  proportional zoom sizing.

## Verification

- [x] 3.1 Run the relevant frontend checks and preview regressions.
- [x] 3.2 Validate the OpenSpec change strictly and review `git diff --check`.
