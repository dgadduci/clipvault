## 1. Storage and business rules

- [x] 1.1 Add migration 0022 with nullable `parent_id`, self-reference,
  cascade delete, one-level/root-only constraints and an index; preserve all
  existing rows and capture/asset data.
- [x] 1.2 Extend collection projections and repository operations with parent
  metadata, validated child creation, and parent-aware listing.
- [x] 1.3 Implement distinct subtree deletion previews and transactional parent
  deletion behavior, preserving the existing `Historial` choice.
- [x] 1.4 Implement child deletion scopes: move memberships to the parent or
  globally delete distinct child captures, with stale-preview protection.
- [x] 1.5 Add focused database/core tests for migration preservation, parent
  constraints, independent membership, both deletion scopes and previews.

## 2. Tauri and frontend

- [x] 2.1 Add the create-subcollection service and thin Tauri command; refresh
  organization and history through existing update events.
- [x] 2.2 Render the sidebar as an accessible one-level tree with expand/
  collapse and create-subcollection controls, preserving existing actions,
  selection, color, note, counts and per-row drop targets.
- [x] 2.3 Distinguish child deletion confirmation copy and resulting navigation;
  preserve parent deletion copy and behavior.
- [x] 2.4 Add all new product/accessibility strings in `en`, `es`, `pt`, `de`,
  and `fr`, and check locale parity.
- [x] 2.5 Add focused frontend tests for tree interactions and child-row drag
  and drop regression.

## 3. Verification

- [ ] 3.1 Run focused database/core tests and frontend checks/builds plus the
  collection drag-and-drop regressions.
- [ ] 3.2 Run locale parity, OpenSpec strict validation, and `git diff --check`;
  review the full change for asset/data preservation.
