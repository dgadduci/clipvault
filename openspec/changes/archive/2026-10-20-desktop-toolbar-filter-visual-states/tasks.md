# Implementation tasks

## 1. Toolbar sizing and active states

- [x] 1.1 Reduce source-application filter width and minimum width by 15%,
  including shared tokens, markup metadata and component CSS.
- [x] 1.2 Add category-specific semi-transparent active backgrounds, white
  text and currentColor icon styling for source-application, tag, collection
  and capture-type filters, driven by each filter's existing selected value.
- [x] 1.3 Keep neutral styling for unselected filters and the `Todas` state.
- [x] 1.4 Use the same shared opacity token for filter and collection-row
  color tints.

## 2. Verification

- [x] 2.1 Add or update focused frontend regression coverage for sizing and
  active/neutral filter visual states.
- [x] 2.2 Run relevant frontend checks/build and protected drag-and-drop
  regressions required for toolbar/layout changes.
- [x] 2.3 Validate this OpenSpec change and review the diff.
