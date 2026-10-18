# Design: collection-subcollections

## Data model and invariants

- Add nullable `collections.parent_id` referencing `collections.id` with
  `ON DELETE CASCADE`, plus an index. `NULL` means top-level.
- Every pre-migration collection remains top-level. The migration changes no
  entry, membership, note, peer binding, clipboard content, or asset reference.
- `Historial` is permanently top-level and cannot own a child. A subcollection
  can reference only an unbound user root. Peer-bound imported collections
  cannot own children, preserving the existing rule that local captures cannot
  be assigned to an imported collection.
- Names remain globally unique case-insensitively. A collection has at most
  one parent; nested children are rejected in the repository/core, not merely
  hidden in the UI.
- Membership stays many-to-many and independent. A capture in a child is not
  automatically added to its parent. New captures still enter `Historial`.
- Existing “clear collection” actions remain scoped to direct memberships;
  clearing a parent does not clear its children. Only deleting the parent
  cascades to its child collection definitions.

## Deletion semantics

- Preview a principal collection using the distinct union of its direct
  memberships and all direct children. Count entries and favorites once even
  when a capture belongs to multiple nodes in the tree.
- Deleting a principal preserves the existing choice: preserving entries adds
  the complete tree's captures to `Historial` before cascading collection
  deletion; deleting entries removes those captures globally. Foreign-key
  cascades remove child definitions and their notes/bindings/memberships.
- Deleting a child has two explicit choices. “Keep in parent” inserts every
  child membership into its parent with `INSERT OR IGNORE`, preserving all
  other memberships and the entry itself. “Delete everywhere” deletes each
  distinct entry associated with the child, which cascades its History,
  parent, other collection, tag and local note memberships according to the
  existing entry deletion rules.
- Preview counts are rechecked atomically before mutation. If they changed,
  return a fresh preview and require confirmation again.
- The migration only adds schema/index/constraints, so rollback can drop the
  new column/index without touching capture data.

## Sidebar interaction

- Render one collection list in parent-then-child order, with a clear child
  indent and `aria-level` metadata. Show direct children only while their
  parent is expanded. `Historial` remains first and cannot expand. Keep the
  existing row/menu actions on both levels.
- Add a disclosure button with an accessible label and `aria-expanded` for
  parents with children; expose “create subcollection” from each eligible
  parent menu. Children have no create-child action.
- Sort names case-insensitively within each level. Expansion state is local to
  the sidebar; creating a child expands its parent and deleting a selected
  child falls back to the parent/history view.
- Preserve the singleton pointer drag controller, mouse fallback, pointer
  capture lifecycle, ghost, cancellation behavior and opaque entry-ID payload.
  Child rows remain independent collection drop targets.
- Deletion confirmation copy distinguishes child deletion, where “keep” means
  keeping captures in the parent, from principal deletion, where it means
  keeping them in `Historial`.

## Localization and verification

- Add all new product/accessibility text to `en`, `es`, `pt`, `de`, `fr` with
  matching keys and interpolation placeholders.
- Add focused database tests for migration preservation, parent validation,
  delete preview de-duplication and both child deletion scopes. Add UI tests
  for hierarchy controls, create-child, scope copy, and a child-row drag drop.
- Run relevant Rust/frontend checks, locale parity, drag/drop regressions,
  OpenSpec validation and `git diff --check`.
