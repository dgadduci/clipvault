/** Resolve the local capture destination for the visible scope. Peer history,
 * Historial, system collections and peer-bound collections have no target. */
export function resolveActiveCaptureCollectionId(
  selectedCollectionId: number | null,
  activePeerId: string | null,
  collections: readonly {
    id: number;
    kind: string;
    is_peer_bound?: boolean;
  }[],
): number | null {
  if (activePeerId !== null || selectedCollectionId === null) return null;
  const selected = collections.find(({ id }) => id === selectedCollectionId);
  return selected?.kind === "user" && selected.is_peer_bound !== true
    ? selectedCollectionId
    : null;
}

/** Serialize updates to the process-local capture destination. Tauri invokes
 * may finish out of order when the user changes collections quickly; keeping
 * one queue ensures the backend ends with the latest UI selection. */
export function createActiveCaptureCollectionSync(
  setCollection: (collectionId: number | null) => Promise<unknown>,
): (collectionId: number | null) => Promise<void> {
  let pending: Promise<void> = Promise.resolve();

  return (collectionId) => {
    const update = pending.then(async () => {
      await setCollection(collectionId);
    });
    pending = update.catch(() => undefined);
    return update;
  };
}
