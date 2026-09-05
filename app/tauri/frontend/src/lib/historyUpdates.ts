// Bridge for the metadata-only `clipvault://history-updated` event the
// shell emits after a successful capture (Stored or Duplicate).
//
// Design contract:
//   - The payload is `null` (or an empty object). The event MUST NOT
//     carry clipboard content, snippets, hashes, source identifiers or
//     any other sensitive value.
//   - The event fires only for `HistoryOutcome::Stored` and
//     `HistoryOutcome::Duplicate`. The shell intentionally does not
//     emit it for `Ignored` or `Failed` outcomes because nothing
//     changed in the history the frontend is allowed to render.
//   - The registrar is idempotent: hot-reloading the frontend or
//     remounting `App.svelte` does not multiply listeners.

import { listen } from "@tauri-apps/api/event";

/**
 * Stable event identifier. Pinned so backend and frontend agree
 * across releases and so accidental renames surface as test
 * failures instead of silently dropping notifications.
 */
export const HISTORY_UPDATED_EVENT = "clipvault://history-updated";

/**
 * Payload shape carried by the event. The shell always sends
 * `null`, but the type allows future non-sensitive additions (for
 * example a monotonic version counter) without breaking consumers.
 * The `null` default keeps the bridge symmetric with the other
 * metadata-only events in `quickPasteBridge.ts`.
 */
export type HistoryUpdatedPayload = null | Record<string, never>;

/** Subscribe to a Tauri event. Resolves with the unlisten handle. */
export function listenHistoryUpdated(
  handler: () => void,
): Promise<() => void> {
  return listen(HISTORY_UPDATED_EVENT, () => {
    try {
      handler();
    } catch (error) {
      console.error("history-updated handler threw", error);
    }
  });
}

/**
 * Optional bridge so tests can inject a fake `listen` function. The
 * production default wires the listener to Tauri's real runtime so
 * callers (typically `App.svelte`) do not have to construct the
 * bridge by hand.
 */
export interface HistoryUpdatedTauriBridge {
  listen: (
    event: string,
    handler: () => void,
  ) => Promise<() => void>;
}

/** Default bridge wired to the real Tauri shell. */
export const defaultHistoryUpdatedBridge: HistoryUpdatedTauriBridge = {
  listen: (_event, handler) => listenHistoryUpdated(handler),
};

/**
 * Subscribe to the history-updated event exactly once per registrar
 * instance. Repeated calls return the existing unlisten handle
 * without registering a second listener.
 *
 * The `App.svelte` mount cycle can fire `onMount` more than once
 * during hot-reload or when the component is intentionally remounted;
 * the registrar guarantees a single Tauri subscription in all of
 * those cases.
 */
export function createHistoryUpdatedRegistrar(
  bridge: HistoryUpdatedTauriBridge = defaultHistoryUpdatedBridge,
) {
  let installed = false;
  let activeUnlisten: (() => void) | null = null;

  return async function register(
    onHistoryUpdated: () => Promise<void> | void,
  ): Promise<() => void> {
    if (installed && activeUnlisten) {
      // Already installed: return the same unlisten handle so the
      // caller can detach the subscription that the first install
      // registered. We track `installed` separately so the second
      // call observes the existing handle instead of installing a
      // second listener.
      return activeUnlisten;
    }
    installed = true;
    try {
      const handle = await bridge.listen(HISTORY_UPDATED_EVENT, () => {
        try {
          onHistoryUpdated();
        } catch (error) {
          console.error("history-updated callback threw", error);
        }
      });
      activeUnlisten = handle;
      return handle;
    } catch (error) {
      installed = false;
      activeUnlisten = null;
      throw error;
    }
  };
}