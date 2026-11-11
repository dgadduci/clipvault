/**
 * Tracks completion of image previews for one collection load. Only visible
 * entry IDs should remain in the barrier; a failed image is still terminal
 * because its card renders the accessible fallback.
 */
export type CollectionPreviewLoadBarrier = {
  readonly promise: Promise<void>;
  readonly pendingCount: number;
  markReady(entryId: number): void;
  retainVisible(entryIds: Iterable<number>): void;
  cancel(): void;
};

export function createCollectionPreviewLoadBarrier(
  entryIds: Iterable<number>,
): CollectionPreviewLoadBarrier {
  const pending = new Set(
    [...entryIds].filter((entryId) => Number.isInteger(entryId) && entryId > 0),
  );
  let settlePromise: (() => void) | null = null;
  let settled = false;
  const promise = new Promise<void>((resolve) => {
    settlePromise = resolve;
  });

  const settleWhenReady = (): void => {
    if (settled || pending.size > 0) return;
    settled = true;
    settlePromise?.();
    settlePromise = null;
  };

  settleWhenReady();

  return {
    promise,
    get pendingCount() {
      return pending.size;
    },
    markReady(entryId) {
      pending.delete(entryId);
      settleWhenReady();
    },
    retainVisible(entryIds) {
      const visible = new Set(entryIds);
      for (const entryId of pending) {
        if (!visible.has(entryId)) pending.delete(entryId);
      }
      settleWhenReady();
    },
    cancel() {
      pending.clear();
      settleWhenReady();
    },
  };
}
