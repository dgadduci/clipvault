/** Pure helpers for ephemeral multi-selection in the desktop and Quick Paste lists. */

export function toggleSelectedId(
  current: ReadonlySet<number>,
  id: number,
): Set<number> {
  const next = new Set(current);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return next;
}

/** Select an inclusive range between two IDs in the currently rendered order. */
export function selectIdRange(
  visibleIds: readonly number[],
  anchorId: number | null,
  targetId: number,
): Set<number> {
  const targetIndex = visibleIds.indexOf(targetId);
  if (targetIndex < 0) return new Set();
  const anchorIndex = anchorId === null ? -1 : visibleIds.indexOf(anchorId);
  if (anchorIndex < 0) return new Set([targetId]);
  const start = Math.min(anchorIndex, targetIndex);
  const end = Math.max(anchorIndex, targetIndex);
  return new Set(visibleIds.slice(start, end + 1));
}

export function retainVisibleIds(
  selectedIds: ReadonlySet<number>,
  visibleIds: readonly number[],
): Set<number> {
  const visible = new Set(visibleIds);
  return new Set([...selectedIds].filter((id) => visible.has(id)));
}

/** Keep the order in which the user selected visible IDs, dropping stale IDs. */
export function selectedIdsInSelectionOrder(
  selectedIds: ReadonlySet<number>,
  visibleIds: readonly number[],
): number[] {
  const visible = new Set(visibleIds);
  return [...selectedIds].filter((id) => visible.has(id));
}

/**
 * Resolve the IDs carried by a local drag session. A drag that begins on an
 * unselected card deliberately collapses to that card so a stale selection
 * cannot make an unrelated card carry the group.
 */
export function dragSelectionIds(
  visibleIds: readonly number[],
  selectedIds: ReadonlySet<number>,
  sourceId: number,
): number[] {
  if (!selectedIds.has(sourceId)) return [sourceId];
  const selectedVisible = visibleIds.filter((id) => selectedIds.has(id));
  return selectedVisible.length > 0 ? selectedVisible : [sourceId];
}
