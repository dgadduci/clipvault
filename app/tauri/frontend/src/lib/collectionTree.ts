import type { Collection } from "../types";

export interface VisibleCollectionRow {
  collection: Collection;
  level: 1 | 2;
}

/**
 * Flatten one-level collections into their visible sidebar order. The
 * permanent system collection stays first; root collections and each
 * parent's children are sorted case-insensitively. Child rows are returned
 * only while their parent id is expanded.
 */
export function visibleCollectionRows(
  collections: readonly Collection[],
  expandedIds: ReadonlySet<number>,
): VisibleCollectionRow[] {
  const compareNames = (left: Collection, right: Collection) =>
    left.name.toLowerCase().localeCompare(right.name.toLowerCase());
  const systemRoots = collections
    .filter((collection) => collection.kind === "system" && collection.parent_id == null)
    .sort(compareNames);
  const userRoots = collections
    .filter((collection) => collection.kind === "user" && collection.parent_id == null)
    .sort(compareNames);
  const rows: VisibleCollectionRow[] = systemRoots.map((collection) => ({
    collection,
    level: 1,
  }));

  for (const parent of userRoots) {
    rows.push({ collection: parent, level: 1 });
    if (!expandedIds.has(parent.id)) continue;
    const children = collections
      .filter((collection) => collection.parent_id === parent.id)
      .sort(compareNames);
    for (const collection of children) {
      rows.push({ collection, level: 2 });
    }
  }

  return rows;
}

export function collectionChildren(
  collections: readonly Collection[],
  parentId: number,
): Collection[] {
  return collections
    .filter((collection) => collection.parent_id === parentId)
    .sort((left, right) =>
      left.name.toLowerCase().localeCompare(right.name.toLowerCase()),
    );
}
