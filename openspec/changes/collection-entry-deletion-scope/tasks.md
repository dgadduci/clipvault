# Tasks: collection-entry-deletion-scope

## 1. Import history membership repair

- [x] 1.1 Add an additive, idempotent data migration that attaches every
  `remote_imports.local_entry_id` to `Historial` when the membership is absent;
  verify existing content, favorites, provenance and other memberships remain
  unchanged in the migration test.
- [x] 1.2 Ensure new and deduplicated peer image imports add or repair the
  `Historial` membership in the same transaction as the peer collection and
  provenance; verify both memberships and rollback behavior in focused DB tests.
- [x] 1.3 Verify peer text imports already preserve the same invariant and add
  a focused regression test if the existing coverage does not prove it.
- [x] 1.4 Cover the reported sequence: delete a peer-bound collection while
  preserving captures, count and clear the non-favorite imported entry from
  `Historial`, and confirm import provenance cascades with entry deletion.

## 2. Collection deletion scope

- [x] 2.1 Add a metadata-only collection deletion preview with total and
  favorite-entry counts; verify it returns no entry content, identifiers,
  hashes, provenance or asset references.
- [x] 2.2 Add confirmed atomic collection deletion with preserve-entries and
  delete-entries choices; verify stale counts require reconfirmation and any
  failed operation leaves the collection and entries unchanged.
- [x] 2.3 Route deleted-entry asset collection and watcher invalidation through
  the existing history-management path; verify favorites are included only in
  the explicitly confirmed collection-wide delete and shared assets remain.
- [x] 2.4 Cover preservation and deletion for ordinary and peer-bound
  collections, including other collection memberships, local import
  provenance, pair trust and future peer imports.

## 3. Scoped card deletion UI

- [x] 3.1 Add accessible collection-delete choices showing the safe entry and
  favorite counts, preserving the collection and every row on cancel; verify
  errors and stale-preview refresh remain recoverable.
- [x] 3.2 Make the card `Eliminar` action in a user collection offer
  collection-only removal, global history deletion and cancel; verify the
  collection-only branch preserves `Historial` and other memberships.
- [x] 3.3 Preserve the existing global delete confirmation from `Historial`
  and the separate `Quitar de esta colección` action; verify card privacy,
  keyboard access, focus return and successful refresh behavior.
- [x] 3.4 Run the required card drag-and-drop regressions for pointer capture,
  mouse fallback, ghost cleanup, Escape/blur/pointercancel, interactive-control
  exclusion and drops on scrolled collections.

## 4. Verification

- [x] 4.1 Run focused Rust tests for migration, peer imports, collection
  deletion transactions, confirmation and asset/reference handling.
- [x] 4.2 Run the affected frontend tests, frontend checks and build for the
  collection and card confirmation flows.
- [x] 4.3 Validate this OpenSpec change and affected specs, run
  `git diff --check`, and review the scoped diff for unrelated changes,
  generated files, network access and clipboard-content exposure.
