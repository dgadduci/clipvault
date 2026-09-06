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
import {
  currentMonitor,
  primaryMonitor,
  type Monitor,
} from "@tauri-apps/api/window";
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
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
  /**
   * Optional centring step. The controller invokes it after the
   * active-app probe resolves and before the window becomes visible
   * so the palette lands on the current monitor with a safe
   * primary-display fallback. Bridges that cannot position the
   * window (non-Tauri callers, tests) can simply no-op; the
   * activation order documented in
   * `quick-paste/spec.md` only requires the window to land on
   * `show`, not on a specific rectangle.
   */
  center?: () => Promise<void>;
  show: () => Promise<void>;
  focus: () => Promise<void>;
  emitOpened: () => Promise<void>;
  hide: () => Promise<void>;
}

/**
 * Default centring step wired to the real Tauri shell. The helper
 * resolves the current monitor's work area (with a primary-display
 * fallback) and hands the result to the pure math helper from
 * `quick_paste_window_layout` through Tauri's monitor API. A
 * missing monitor or a window API failure collapses to a no-op so
 * the activation order stays robust on every supported host.
 */
export async function centerQuickPasteWindow(): Promise<void> {
  try {
    const current = await currentMonitor().catch(() => null);
    const primary = !current ? await primaryMonitor().catch(() => null) : null;
    const target = await resolveCenterWorkArea(current, primary);
    if (!target) {
      return;
    }
    const layout = computeQuickPasteLayout(target);
    const window = await getQuickPasteWindow();
    await applyQuickPasteLayout(window, layout);
  } catch {
    // Centring is best-effort: a missing capability or a transient
    // Tauri error must not block the user from opening the
    // quick-paste palette. The window keeps the conf-defined
    // defaults declared in `tauri.conf.json`.
  }
}

interface QuickPasteCenterTarget {
  workX: number;
  workY: number;
  workWidth: number;
  workHeight: number;
  scaleFactor: number;
}

interface QuickPasteCenterLayout {
  logicalSize: { width: number; height: number };
  logicalPosition: { x: number; y: number };
}

async function resolveCenterWorkArea(
  current: Monitor | null,
  primary: Monitor | null,
): Promise<QuickPasteCenterTarget | null> {
  if (current?.workArea) {
    return toCenterTarget(current);
  }
  if (primary?.workArea) {
    return toCenterTarget(primary);
  }
  return null;
}

function toCenterTarget(monitor: Monitor): QuickPasteCenterTarget {
  return {
    workX: monitor.workArea.position.x,
    workY: monitor.workArea.position.y,
    workWidth: Math.max(1, monitor.workArea.size.width),
    workHeight: Math.max(1, monitor.workArea.size.height),
    scaleFactor: safeScale(monitor.scaleFactor),
  };
}

function safeScale(value: number): number {
  return typeof value === "number" && value > 0 ? value : 1;
}

function computeQuickPasteLayout(target: QuickPasteCenterTarget): QuickPasteCenterLayout {
  const scale = target.scaleFactor;
  const logicalWidth = 720;
  const logicalHeight = 520;
  const workLogicalWidth = target.workWidth / scale;
  const workLogicalHeight = target.workHeight / scale;
  const workLogicalX = target.workX / scale;
  const workLogicalY = target.workY / scale;
  const logicalX = workLogicalX + (workLogicalWidth - logicalWidth) / 2;
  const logicalY = workLogicalY + (workLogicalHeight - logicalHeight) / 2;
  return {
    logicalSize: { width: logicalWidth, height: logicalHeight },
    logicalPosition: { x: logicalX, y: logicalY },
  };
}

async function applyQuickPasteLayout(
  window: WebviewWindow,
  layout: QuickPasteCenterLayout,
): Promise<void> {
  try {
    await window.setSize(new LogicalSize(layout.logicalSize.width, layout.logicalSize.height));
  } catch {
    // Tauri's `setSize` is best-effort on the quick-paste window:
    // the documented contract is "fixed 720×520", and the conf
    // already pins the value. A transient failure leaves the conf
    // defaults in place instead of surfacing an error.
  }
  try {
    await window.setPosition(
      new LogicalPosition(layout.logicalPosition.x, layout.logicalPosition.y),
    );
  } catch {
    // Same rationale as `setSize`: positioning is best-effort, the
    // conf defaults keep the window usable.
  }
}

/** Default bridge wired to the real Tauri shell. */
export const defaultQuickPasteBridge: QuickPasteTauriBridge = {
  captureActiveApp: activeApplicationCommand,
  center: centerQuickPasteWindow,
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
