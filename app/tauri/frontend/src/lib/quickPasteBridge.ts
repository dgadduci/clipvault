// Bridge between the frontend and the Tauri shell for the quick-paste
// window.
//
// Responsibilities:
//   - Register an idempotent listener for the global hotkey event
//     (`clipvault://quick-search`) emitted by the shell.
//   - Capture the previously focused application before showing the
//     quick-paste window so the synthetic paste targets the right host.
//   - Show, focus and signal the quick-paste window in the documented
//     order (`show` → `focus` → `emit opened`).
//   - Hide the quick-paste window before the shell invokes the paste
//     command so the host application regains focus.
//
// The module intentionally does **not** carry query, snippet or
// clipboard payload in any event; events use a `null` payload so the
// frontend cannot accidentally surface the wrong content.

import { activeApplicationCommand } from "./tauri.ts";
import { openQuickPaste } from "./quickPasteController.ts";
import { emitTo, listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { ActiveApplicationResponse } from "../types.ts";

export const QUICK_SEARCH_EVENT = "clipvault://quick-search";
export const QUICK_PASTE_OPENED_EVENT = "clipvault://quick-paste-opened";
export const MAIN_WINDOW_LABEL = "main";
export const QUICK_PASTE_WINDOW_LABEL = "quick-paste";

async function getQuickPasteWindow(): Promise<WebviewWindow> {
  const window = await WebviewWindow.getByLabel(QUICK_PASTE_WINDOW_LABEL);
  if (!window) {
    throw new Error(
      `ClipVault quick-paste window not found: ${QUICK_PASTE_WINDOW_LABEL}`,
    );
  }
  return window;
}

/** Subscribe to a Tauri event. Resolves with the unlisten handle. */
export function listenQuickSearch(
  handler: () => void,
): Promise<() => void> {
  return listen(QUICK_SEARCH_EVENT, () => {
    try {
      handler();
    } catch (error) {
      console.error("quick-search handler threw", error);
    }
  }, { target: MAIN_WINDOW_LABEL });
}

/** Show the quick-paste window. */
export function showQuickPasteWindow(): Promise<void> {
  return getQuickPasteWindow().then((window) => window.show());
}

/** Focus the quick-paste window so the search input receives keys. */
export function focusQuickPasteWindow(): Promise<void> {
  return getQuickPasteWindow().then((window) => window.setFocus());
}

/** Hide the quick-paste window so the host app regains focus. */
export function hideQuickPasteWindow(): Promise<void> {
  return getQuickPasteWindow().then((window) => window.hide());
}

/**
 * Signal the quick-paste window that it just became visible. The
 * payload is intentionally `null`; the quick-paste frontend reads no
 * clipboard content from this event.
 */
export function emitQuickPasteOpened(): Promise<void> {
  return emitTo(QUICK_PASTE_WINDOW_LABEL, QUICK_PASTE_OPENED_EVENT, null);
}

/**
 * Step-by-step orchestration used by the controller. Splitting each
 * step into its own function makes the controller testable with a
 * fake Tauri surface (see `quickPasteController.ts`).
 */
export interface QuickPasteTauriBridge {
  captureActiveApp: () => Promise<ActiveApplicationResponse>;
  show: () => Promise<void>;
  focus: () => Promise<void>;
  emitOpened: () => Promise<void>;
  hide: () => Promise<void>;
}

/** Default bridge wired to the real Tauri shell. */
export const defaultQuickPasteBridge: QuickPasteTauriBridge = {
  captureActiveApp: activeApplicationCommand,
  show: showQuickPasteWindow,
  focus: focusQuickPasteWindow,
  emitOpened: emitQuickPasteOpened,
  hide: hideQuickPasteWindow,
};

/**
 * Subscribe to the global hotkey event exactly once per `bridge`
 * instance. Repeated calls return the same `unlisten` handle without
 * registering a second listener.
 *
 * The registrar is idempotent by design: hot-reloading the frontend or
 * accidentally mounting `App.svelte` twice should not multiply event
 * listeners.
 */
export function createQuickSearchRegistrar(
  bridge: QuickPasteTauriBridge = defaultQuickPasteBridge,
  onActivationError?: (error: unknown) => void,
) {
  let installed = false;
  let unlisten: (() => void) | null = null;
  let inflight: Promise<void> | null = null;

  return async function register(
    onQuickSearch: () => Promise<void> | void,
  ): Promise<() => void> {
    if (installed) {
      // Already installed: return the existing unlisten so callers can
      // detach the same subscription that previous mounts registered.
      return async () => {
        if (unlisten) {
          unlisten();
          unlisten = null;
        }
        installed = false;
      };
    }
    installed = true;
    const activate = (): void => {
      if (inflight) return;
      const activation = openQuickPaste(bridge)
        .then(() => onQuickSearch())
        .catch((error) => {
          console.error("failed to activate quick-paste", error);
          onActivationError?.(error);
        })
        .finally(() => {
          inflight = null;
        });
      inflight = activation;
      void activation;
    };
    try {
      const handle = await listenQuickSearch(activate);
      unlisten = handle;
      return handle;
    } catch (error) {
      installed = false;
      inflight = null;
      throw error;
    }
  };
}
