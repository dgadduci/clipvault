// Pure orchestration of the quick-paste window lifecycle.
//
// `App.svelte` uses `createQuickSearchRegistrar` (which calls into
// `quickPasteBridge`) to wire the global hotkey, but every decision
// about ordering and ordering-only side effects lives here so it can
// be exercised by `node:test` without a DOM or a real Tauri shell.

import type {
  ActiveApplicationResponse,
  CopyResponse,
  PasteResponse,
} from "../types.ts";
import type { QuickPasteTauriBridge } from "./quickPasteBridge.ts";

/**
 * Step the controller runs in order when the global hotkey fires.
 *
 * Captured so unit tests can assert that the live code follows the
 * documented order (`captureActiveApp → center → show → focus →
 * emitOpened`). The `center` step is part of the optional
 * `bridge.center` adapter the production bridge wires to
 * `centerQuickPasteWindow`; a test bridge that does not implement
 * `center` simply skips the step while still preserving the rest of
 * the ordering.
 */
export type QuickPasteStep =
  | "captureActiveApp"
  | "center"
  | "show"
  | "focus"
  | "emitOpened";

/**
 * Order of steps executed for a fresh quick-paste activation. The
 * `center` step sits between the active-app probe and the visibility
 * transition so the documented `show → focus → emitOpened` tail stays
 * intact and the palette lands on the current monitor before it
 * becomes visible.
 */
export const QUICK_PASTE_STEP_ORDER: readonly QuickPasteStep[] = [
  "captureActiveApp",
  "center",
  "show",
  "focus",
  "emitOpened",
] as const;

/**
 * The visible activation tail the spec protects. The three steps
 * must run in this exact order and only after `center` resolves so
 * the window lands on its final rectangle before becoming visible.
 */
export const QUICK_PASTE_VISIBLE_TAIL: readonly QuickPasteStep[] = [
  "show",
  "focus",
  "emitOpened",
] as const;

/** Maximum time the active-app probe may delay opening the transient UI. */
export const QUICK_PASTE_ACTIVE_APP_TIMEOUT_MS = 500;

function withTimeout<T>(operation: Promise<T>, timeoutMs: number): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(new Error("active application probe timed out"));
    }, timeoutMs);
    operation.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (error: unknown) => {
        clearTimeout(timer);
        reject(error);
      },
    );
  });
}

/**
 * Run the show-focus-signal sequence on `bridge`. Each step awaits the
 * previous one so the order is observable from tests and from
 * production logs.
 */
export async function openQuickPaste(
  bridge: QuickPasteTauriBridge,
  activeAppTimeoutMs = QUICK_PASTE_ACTIVE_APP_TIMEOUT_MS,
): Promise<ActiveApplicationResponse | null> {
  let active: ActiveApplicationResponse | null = null;
  try {
    // Invoke the probe before showing the window, as required by the
    // focus contract, but do not let an unavailable native API leave
    // the transient UI permanently invisible.
    active = await withTimeout(
      Promise.resolve().then(() => bridge.captureActiveApp()),
      activeAppTimeoutMs,
    );
  } catch (error) {
    // The active-app probe is best-effort: a missing capability or
    // error must not block the user from opening quick-paste. The
    // synthetic paste step will surface a guidance modal later if
    // needed.
    void error;
  }
  if (bridge.center) {
    try {
      await bridge.center();
    } catch (error) {
      // Centring is best-effort: a failure must not block the user
      // from opening the palette. The window keeps the conf-defined
      // defaults declared in `tauri.conf.json` instead of throwing.
      void error;
    }
  }
  await bridge.show();
  await bridge.focus();
  await bridge.emitOpened();
  return active;
}

/** Outcome of a paste attempt expressed in the language the UI cares
 *  about. Centralised so the modal can react without duplicating the
 *  decision tree. */
export type PasteFlowOutcome =
  | { kind: "pasted"; windowStaysHidden: true }
  | {
      kind: "failed";
      windowStaysHidden: false;
      response: PasteResponse | { error: string };
    };

/**
 * Hide the quick-paste window before invoking the paste command. The
 * caller passes the result of the paste call so the controller can
 * decide whether to re-show the window.
 *
 * The hide step happens **before** the `pasteFn` invocation so the
 * previously focused host application can receive the synthetic paste.
 */
export async function performPasteFlow(args: {
  bridge: QuickPasteTauriBridge;
  pasteFn: () => Promise<PasteResponse>;
  isCapabilityUnavailable?: (response: PasteResponse) => boolean;
  isFailed?: (response: PasteResponse) => boolean;
  onReShow?: () => void;
}): Promise<PasteFlowOutcome> {
  await args.bridge.hide();
  let response: PasteResponse;
  try {
    response = await args.pasteFn();
  } catch (error) {
    await args.bridge.show();
    args.onReShow?.();
    return {
      kind: "failed",
      windowStaysHidden: false,
      response: {
        error: error instanceof Error ? error.message : String(error),
      },
    };
  }
  const failed = args.isFailed?.(response) ?? response.kind === "failed";
  const unavailable =
    args.isCapabilityUnavailable?.(response) ??
    response.kind === "capability_unavailable";
  if (response.kind === "pasted") {
    return { kind: "pasted", windowStaysHidden: true };
  }
  if (failed || unavailable) {
    await args.bridge.show();
    args.onReShow?.();
    return { kind: "failed", windowStaysHidden: false, response };
  }
  return { kind: "pasted", windowStaysHidden: true };
}

/** Outcome of a copy attempt expressed in the language the UI cares
 *  about. Centralised so the modal can react without duplicating the
 *  decision tree. The arms mirror [`PasteFlowOutcome`] minus the
 *  `pasted` branch: a copy flow can never report `pasted` because
 *  the synthetic paste was not triggered. */
export type CopyFlowOutcome =
  | { kind: "copied"; windowStaysHidden: true }
  | {
      kind: "failed";
      windowStaysHidden: false;
      response: CopyResponse | { error: string };
    };

/**
 * Hide the quick-paste window, invoke the copy-only command, and
 * hide the window if the copy succeeded.
 *
 * The function NEVER calls a synthetic paste controller: it routes
 * through `clipvault_copy_entry` which only writes to the clipboard
 * and arms the suppression token. The previously focused
 * application regains focus the moment the window hides, ready for
 * the user to decide when to run `Cmd/Ctrl+V`.
 *
 * On a `failed` / `capability_unavailable` outcome the window is
 * re-shown and the caller can render the typed guidance the
 * backend returned. The history row is never touched.
 */
export async function performCopyFlow(args: {
  bridge: QuickPasteTauriBridge;
  copyFn: () => Promise<CopyResponse>;
  isCapabilityUnavailable?: (response: CopyResponse) => boolean;
  isFailed?: (response: CopyResponse) => boolean;
  onReShow?: () => void;
}): Promise<CopyFlowOutcome> {
  await args.bridge.hide();
  let response: CopyResponse;
  try {
    response = await args.copyFn();
  } catch (error) {
    await args.bridge.show();
    args.onReShow?.();
    return {
      kind: "failed",
      windowStaysHidden: false,
      response: {
        error: error instanceof Error ? error.message : String(error),
      },
    };
  }
  const failed = args.isFailed?.(response) ?? response.kind === "failed";
  const unavailable =
    args.isCapabilityUnavailable?.(response) ??
    response.kind === "capability_unavailable";
  if (response.kind === "copied" || response.kind === "copied_plain_fallback") {
    return { kind: "copied", windowStaysHidden: true };
  }
  if (failed || unavailable) {
    await args.bridge.show();
    args.onReShow?.();
    return { kind: "failed", windowStaysHidden: false, response };
  }
  return { kind: "copied", windowStaysHidden: true };
}
