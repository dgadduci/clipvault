// Bridge for the metadata-only `clipvault://organization-updated`
// event the shell emits after every successful organization
// mutation (tag create/rename/delete, collection create/rename/
// delete, entry_tags_set, entry_collections_set,
// entry_remove_from_collection, entry_upsert_tag).
//
// Design contract (mirrors the `history-updated` bridge in
// `historyUpdates.ts`):
//
//   - The payload is `null` / `()`. The event MUST NOT carry
//     clipboard content, snippets, tag bodies, hashes, source
//     identifiers or any other sensitive value.
//   - The event fires only after a successful `Ok(...)` return from
//     the corresponding Tauri command. The shell intentionally does
//     not emit it when the backend rejects the mutation, because the
//     persistent state then did not change.
//   - The registrar is idempotent: hot-reloading the frontend or
//     remounting `App.svelte` does not multiply listeners.

import { listen } from "@tauri-apps/api/event";

/**
 * Stable event identifier. Pinned so backend and frontend agree
 * across releases and so accidental renames surface as test
 * failures instead of silently dropping notifications.
 */
export const ORGANIZATION_UPDATED_EVENT = "clipvault://organization-updated";

/**
 * Payload shape carried by the event. The shell always sends
 * `null`, but the type allows future non-sensitive additions (for
 * example a stable operation counter) without breaking consumers.
 * The `null` default keeps the bridge symmetric with the
 * `history-updated` bridge.
 */
export type OrganizationUpdatedPayload = null | Record<string, never>;

/** Subscribe to a Tauri event. Resolves with the unlisten handle. */
export function listenOrganizationUpdated(
  handler: () => void,
): Promise<() => void> {
  return listen(ORGANIZATION_UPDATED_EVENT, () => {
    try {
      handler();
    } catch (error) {
      console.error("organization-updated handler threw", error);
    }
  });
}

/**
 * Optional bridge so tests can inject a fake `listen` function. The
 * production default wires the listener to Tauri's real runtime so
 * callers (typically `App.svelte`) do not have to construct the
 * bridge by hand.
 */
export interface OrganizationUpdatedTauriBridge {
  listen: (
    event: string,
    handler: () => void,
  ) => Promise<() => void>;
}

/** Default bridge wired to the real Tauri shell. */
export const defaultOrganizationUpdatedBridge: OrganizationUpdatedTauriBridge = {
  listen: (_event, handler) => listenOrganizationUpdated(handler),
};

/**
 * Subscribe to the organization-updated event exactly once per
 * registrar instance. Repeated calls return the existing unlisten
 * handle without registering a second listener.
 *
 * The `App.svelte` mount cycle can fire `onMount` more than once
 * during hot-reload or when the component is intentionally
 * remounted; the registrar guarantees a single Tauri subscription
 * in all of those cases.
 */
export function createOrganizationUpdatedRegistrar(
  bridge: OrganizationUpdatedTauriBridge = defaultOrganizationUpdatedBridge,
) {
  let installed = false;
  let activeUnlisten: (() => void) | null = null;

  return async function register(
    onOrganizationUpdated: () => Promise<void> | void,
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
      const handle = await bridge.listen(ORGANIZATION_UPDATED_EVENT, () => {
        try {
          onOrganizationUpdated();
        } catch (error) {
          console.error("organization-updated callback threw", error);
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
