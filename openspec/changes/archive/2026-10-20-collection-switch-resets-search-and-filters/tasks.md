## 1. Reset query and filters on collection navigation

- [x] 1.1 Clear search state and invalidate an in-flight search when the
  sidebar switches the active collection.
- [x] 1.2 Keep source-application, tag and History collection filters at
  `Todas` and reload the destination scope.
- [x] 1.3 Add a focused frontend regression for search and filter reset.

## 2. Verification

- [x] 2.1 Run the focused frontend tests and collection/drag-and-drop
  regressions, plus check and build.
  The new regression, collection tests and drag-and-drop tests passed, as did
  `npm run build`. The full frontend suite has one unrelated failure in
  `peerImportReadinessFrontend.test.js`: the environment runs Node 20.20.2,
  where `Promise.withResolvers` is unavailable (the package requires Node 24).
- [x] 2.2 Validate this change with strict OpenSpec validation and review the
  complete diff.
