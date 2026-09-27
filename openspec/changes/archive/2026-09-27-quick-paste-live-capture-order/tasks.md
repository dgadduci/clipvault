## 1. Baseline and contract

- [x] 1.1 Inspect QuickVault recent ordering, recent-entry queries, history
  update subscriptions, and local/text-import/image-import commit events;
  preserve all unrelated work in the tree.
- [x] 1.2 Confirm the existing immutable `created_at`, import dedupe,
  QuickVault search-ranking, selection, and copy/paste contracts.

## 2. Recent ordering and refresh

- [x] 2.1 Update QuickVault recent mode to order all entries by
  `created_at DESC, id DESC`, without favorites overriding chronology; retain
  favorite indicators/actions and SearchService ranking.
- [x] 2.2 Ensure local capture and successful text/image peer-import commits
  trigger the existing metadata-only QuickVault refresh path; do not notify
  QuickVault about a failed or rolled-back import.
- [x] 2.3 Add a stale-response guard so an earlier recent-list request cannot
  overwrite a newer refresh; preserve query mode and current entry identity
  where applicable.
- [x] 2.4 Keep `created_at` immutable for deduplicated imports and avoid
  optimistic frontend timestamps or event payloads containing entry data.

## 3. Regression coverage

- [x] 3.1 Add focused ordering tests for newest-first, equal-timestamp ID
  tie-breaks, and a newer unpinned entry preceding an older pinned entry.
- [x] 3.2 Add frontend regression tests for local-capture and successful
  text/image-import refreshes, including out-of-order completion and failed
  import behavior.
- [x] 3.3 Verify deduplicated import timestamps, active search ranking,
  selection, copy/paste and clipboard behavior remain unchanged.
- [x] 3.4 Run relevant QuickVault tests, frontend check/build, and protected
  drag-and-drop regressions if shared listeners or card/rail code changed;
  report unrelated baseline failures separately.

## 4. Review

- [x] 4.1 Run `git diff --check`, validate this change with strict OpenSpec
  validation, and review the final diff for scope, privacy, and generated
  files.
