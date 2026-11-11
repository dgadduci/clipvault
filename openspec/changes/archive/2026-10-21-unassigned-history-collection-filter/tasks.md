## 1. OpenSpec

- [x] 1.1 Specify the meaning of `Sin colección`, including the system
  Historial membership exception.
- [x] 1.2 Specify the visible disabled state outside Historial and reset behavior
  when returning to Historial.

## 2. Persistence and core

- [x] 2.1 Add a metadata-only query path that filters out entries with any
  user-collection membership while preserving existing query APIs.
- [x] 2.2 Apply the same membership filter to recent entries, search and
  available content types; add focused persistence/core coverage.

## 3. Frontend

- [x] 3.1 Add the `unassigned` collection-filter value and localized option in
  all five language catalogs.
- [x] 3.2 Keep the collection filter mounted in the toolbar and disable it
  outside Historial without changing active collection results.
- [x] 3.3 Send the unassigned-only filter through recent and search requests;
  preserve search, source-app, tag and content-type selections.
- [x] 3.4 Add focused frontend regressions for filter selection, disabled state
  and reset on collection navigation.

## 4. Verification

- [x] 4.1 Run focused persistence/core and frontend tests, including collection
  row drag-and-drop regressions.
- [x] 4.2 Verify locale parity, relevant Rust and frontend checks/builds,
  strict OpenSpec validation and `git diff --check`.
- [x] 4.3 Record user approval of the manual test (2026-10-21).
