# Tasks: quick-paste-actions

## 1. Baseline and contract

- [x] 1.1 Read this change, the archived quick-paste implementation and the
  active `quick-paste-compact-ui` baseline.
- [x] 1.2 Confirm that `Enter`/`Shift+Enter` use a copy-only operation and do
  not call `pasteEntryCommand` when that command triggers synthetic paste.
- [x] 1.3 Confirm that direct menu actions remain separate from the keyboard
  copy-only flow.
- [x] 1.4 Do not use reset, destructive checkout or deletion of the existing
  Quick Paste implementation.

## 2. Copy-only keyboard flow

- [x] 2.1 Add a thin typed command/service for writing an existing entry to the
  system clipboard without invoking any synthetic paste controller.
- [x] 2.2 Reuse the existing payload loading, rich/plain/image writers and
  autocapture suppression logic.
- [x] 2.3 Return metadata-only copied/failed/capability-unavailable outcomes.
- [x] 2.4 Wire Enter and Shift+Enter to the copy-only operation.
- [x] 2.5 Implement rich/plain/image mode selection and image Shift+Enter no-op.
- [x] 2.6 Hide Quick Paste after successful copy and leave the clipboard
  unchanged afterward.
- [x] 2.7 Show typed guidance on copy failure without mutating history.

## 3. Favorites and direct menu

- [x] 3.1 Render an accessible pin in every visible item.
- [x] 3.2 Reuse the existing favorite command and preserve organization data.
- [x] 3.3 Prioritize favorites while preserving order within each result group.
- [x] 3.4 Keep focus and selection deterministic after pin/unpin.
- [x] 3.5 Add the `...` menu inside the fixed item geometry.
- [x] 3.6 Derive direct menu actions from actual entry capabilities.
- [x] 3.7 Keep image menu entries limited to direct `Pegar`.
- [x] 3.8 Preserve the existing direct menu paste lifecycle separately from
  keyboard copy-only.

## 4. Regression tests

- [x] 4.1 Test rich Enter copies without calling the synthetic paste controller.
- [x] 4.2 Test plain Shift+Enter copies without synthetic paste.
- [x] 4.3 Test plain Enter and image Enter copy the expected representation.
- [x] 4.4 Test image-only Shift+Enter and no-selection Enter are no-ops.
- [x] 4.5 Test clipboard remains available for a later manual Cmd/Ctrl+V.
- [x] 4.6 Test copy does not create a history card or mutate the source entry.
- [x] 4.7 Test direct menu actions and their existing hide/paste/guidance flow.
- [x] 4.8 Test favorite ordering, selection stability and metadata preservation.
- [x] 4.9 Test images, tags, collections, privacy and listener idempotence.

## 6. Selection, title search and image-rendering regressions

- [x] 6.1 Make the non-interactive surface of every Quick Paste row select that
  entry on pointer/mouse click.
- [x] 6.2 Stop row click propagation from pin and overflow-menu controls, and
  verify those controls do not trigger an accidental selection or confirmation.
- [x] 6.3 Keep ArrowUp, ArrowDown, Home and End selection visible with
  `scrollIntoView({ block: "nearest" })` or an equivalent scoped mechanism on
  the Quick Paste list container only.
- [x] 6.4 Preserve selection by stable entry id across search, favorite toggle
  and refresh; never act on a stale selected index.
- [x] 6.5 Extend the local search document/service path used by Quick Paste so
  custom card titles are searchable alongside canonical content, preserving
  deterministic ranking and privacy constraints.
- [x] 6.6 Make initial image thumbnail resolution independent per entry, with
  explicit loading/loaded/error states, stale-response protection and Blob URL
  cleanup; do not change persisted asset references.
- [x] 6.7 Add frontend/bridge/core regression tests for click selection,
  control isolation, scoped autoscroll, title-only search, deterministic mixed
  field results and all-visible-image settlement.
- [x] 6.8 Run the complete existing Quick Paste, image, search, organization,
  drag-and-drop and privacy regression suites and inspect the diff for
  unintended behavior changes.

## 7. Pointer confirmation parity regression

- [x] 7.1 Make a click on the non-interactive surface of a result row invoke the
  exact same copy-only action as Enter, including type-appropriate
  representation, hide-after-success and availability for later Cmd/Ctrl+V.
- [x] 7.2 Keep pin and overflow-menu controls isolated so clicking them does not
  invoke the row confirmation path.
- [x] 7.3 Add frontend/controller tests proving click, Enter and their failure
  handling share the same behavior and that click never invokes synthetic
  paste.
- [x] 7.4 Re-run the image, copy-only, direct-menu, favorite, title-search,
  selection-scroll, drag-and-drop and privacy regression suites.

## 5. Verification

- [x] 5.1 Run `cargo fmt --all -- --check`.
- [x] 5.2 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 5.3 Run `cargo test --workspace`.
- [x] 5.4 Run `npm run check`, `npm run build` and `npm test` in
  `app/tauri/frontend`.
- [x] 5.5 Run `openspec validate quick-paste-actions --strict --type change`.
- [ ] 5.6 Perform the real macOS manual test for copy-only keyboard behavior
  and direct menu behavior; do not mark it completed without evidence.
- [x] 5.7 Do not synchronize or archive automatically.
