import type { Capabilities, PlatformGuidance } from "../types.ts";

export interface RetryCallbacks {
  /** Called when refresh succeeded and the capability is now available.
   *  Receives the full refreshed matrix so the caller can update all
   *  capability fields, not just `synthetic_paste`. */
  onResolved: (capabilities: Capabilities) => void;
  /** Called when refresh succeeded but the capability is still unavailable.
   *  Receives the full refreshed matrix so the caller can update all
   *  capability fields. */
  onStillUnavailable: (capabilities: Capabilities) => void;
  /** Called when refresh itself failed (network error, backend error, ...).
   *  The message comes from the thrown error and must never contain the
   *  clipboard payload. */
  onRefreshError: (message: string) => void;
}

export interface RetryContext extends RetryCallbacks {
  /** Refresh the capability matrix from the backend. */
  refresh: () => Promise<Capabilities>;
}

/**
 * Re-evaluate the capability matrix after the user returns from system
 * settings. The retry **must never** trigger another paste attempt;
 * the user has to press "Paste latest" again to actually paste.
 *
 * The helper never mutates caller state directly: it surfaces the
 * refreshed matrix through the resolved / still-unavailable callbacks
 * so the parent component stays the single source of truth.
 *
 * The function takes its dependencies through callbacks so the
 * decision tree is testable without wiring the full Tauri IPC stack.
 */
export async function retryGuidance(ctx: RetryContext): Promise<void> {
  try {
    const caps = await ctx.refresh();
    if (caps.synthetic_paste) {
      ctx.onResolved(caps);
      return;
    }
    ctx.onStillUnavailable(caps);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.onRefreshError(message);
  }
}

/**
 * Decide whether the paste button should be enabled. The button is
 * always enabled when there is at least one entry: if the capability
 * is unavailable the explicit click triggers the guidance modal so the
 * user can see what to do instead of being silently blocked.
 */
export function shouldEnablePasteButton(entryCount: number): boolean {
  return entryCount > 0;
}

/**
 * Result of a paste operation expressed in the language the UI cares
 * about. Centralised here so the modal can adapt without duplicating
 * the logic in the Svelte component.
 */
export type PastePresentation =
  | { kind: "pasted" }
  | { kind: "guidance"; guidance: PlatformGuidance | null };

export function presentPaste(
  paste: { kind: string; guidance: PlatformGuidance | null },
): PastePresentation {
  if (paste.kind === "pasted") {
    return { kind: "pasted" };
  }
  return { kind: "guidance", guidance: paste.guidance };
}
