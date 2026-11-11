## 1. Correct collection count projection

- [x] 1.1 Update the aggregate used by collection list and lookup so parent
  counts add parent-only direct memberships to each child's direct count,
  while child counts remain direct.
- [x] 1.2 Add focused repository coverage for expected parent totals and count
  changes after clearing/deleting memberships, including captures shared by
  multiple subcollections.

## 2. Verification

- [x] 2.1 Run the focused Rust persistence tests and formatting checks.
- [x] 2.2 Run the frontend collection and drag-and-drop regressions, checks and
  build required for changes affecting collections.
  The collection and drag-and-drop tests and `npm run build` passed. The full
  frontend suite has one unrelated failure in
  `peerImportReadinessFrontend.test.js`: this environment runs Node 20.20.2,
  where `Promise.withResolvers` is unavailable (the package requires Node 24).
- [x] 2.3 Validate this change with strict OpenSpec validation and review the
  complete diff.
