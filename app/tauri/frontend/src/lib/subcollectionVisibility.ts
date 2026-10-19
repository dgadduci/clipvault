import type { Collection } from "../types";

type CollectionMembership = { id: number };
type EntryMemberships = { collections: readonly CollectionMembership[] };
type HydrationState = "pending" | "loaded" | "error";

/**
 * Apply the optional parent-view filter to entries that have already been
 * selected by the parent collection's direct membership. Memberships in a
 * child collection remain independent and never add child-only entries here.
 *
 * In hidden mode, entries whose membership metadata is not loaded are omitted
 * until the UI can confirm that they do not belong to a child.
 */
export function projectSubcollectionVisibility<T extends { id: number }>(
  entries: readonly T[],
  parentCollectionId: number | null,
  collections: readonly Pick<Collection, "id" | "parent_id">[],
  entryMemberships: ReadonlyMap<number, EntryMemberships>,
  hydration: ReadonlyMap<number, HydrationState>,
  showSharedCaptures: boolean,
): T[] {
  if (parentCollectionId === null || showSharedCaptures) {
    return [...entries];
  }

  const childIds = new Set(
    collections
      .filter((collection) => collection.parent_id === parentCollectionId)
      .map((collection) => collection.id),
  );
  if (childIds.size === 0) {
    return [...entries];
  }

  return entries.filter((entry) => {
    if (hydration.get(entry.id) !== "loaded") {
      return false;
    }
    const memberships = entryMemberships.get(entry.id);
    if (!memberships) {
      return false;
    }
    return !memberships.collections.some((collection) =>
      childIds.has(collection.id),
    );
  });
}
