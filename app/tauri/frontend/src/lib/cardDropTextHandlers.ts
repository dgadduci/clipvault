// Drag-and-drop handlers for the desktop drop indicator text
// (`CardDropText.svelte`).
//
// The indicator is the simplest possible drop target the desktop
// surfaces below the rail of recent captures: it listens for the
// four drag events on its own DOM node and uses the same helpers
// the sidebar relies on so the contract the card and the rest of
// the desktop agree on (private MIME, `text/plain` fallback and
// the in-memory drag session) stays consistent.
//
// The contract is split into a tiny pure helper (`payloadAccepts`)
// and a small handler factory (`createCardDropTextHandlers`) so
// the integration tests can exercise the EXACT logic the component
// runs without spinning up a Svelte runtime. Every helper
// imported by `CardDropText.svelte` lives in this module so the
// indicator stays a thin wrapper around `createCardDropTextHandlers`.

import { endDragSession, hasActiveDragSession } from "./dragAndDrop.ts";
import {
  activePointerDragEntryId,
  type PointerDragDetail,
} from "./pointerDragAndDrop.ts";

const PRIVATE_MIME = "application/x.clipvault-entry-id";
const TEXT_PLAIN = "text/plain";

/**
 * Pure predicate: returns `true` when the event carries a
 * ClipVault payload (private MIME, `text/plain` fallback or the
 * in-memory drag session the card controller opens after pointer/mouse
 * activation).
 *
 * The function is intentionally tolerant of the WebKit/Tauri
 * quirk that strips `dataTransfer.types`: when the `types`
 * collection is empty but the session is active the helper still
 * returns `true` so a foreign drag that closes the DataTransfer
 * does not silence the indicator. A foreign drag without a
 * session reduces to `false`.
 */
export function payloadAccepts(event: DragEvent): boolean {
  const dt = event.dataTransfer;
  if (dt) {
    const types = dt.types as unknown;
    if (types && typeof (types as { contains?: unknown }).contains === "function") {
      const domList = types as { contains(value: string): boolean };
      if (domList.contains(PRIVATE_MIME) || domList.contains(TEXT_PLAIN)) {
        return true;
      }
    } else if (
      types &&
      typeof (types as { length?: unknown }).length === "number"
    ) {
      const list = types as { [index: number]: string; length: number };
      for (let i = 0; i < list.length; i += 1) {
        const t = list[i];
        if (t === PRIVATE_MIME || t === TEXT_PLAIN) {
          return true;
        }
      }
    }
  }
  return hasActiveDragSession();
}

export type CardDropTextState = "idle" | "hover" | "dropped";

export interface CardDropTextApi {
  setState(state: CardDropTextState): void;
  scheduleReset(): void;
  clearTimer(): void;
}

/**
 * Build the four delegated handlers the indicator binds on its
 * own DOM node. The factory is the single source of truth for the
 * indicator logic so the production component and the integration
 * tests exercise the exact same hand-off.
 */
export function createCardDropTextHandlers(api: CardDropTextApi): {
  onDragEnter: (event: DragEvent) => void;
  onDragOver: (event: DragEvent) => void;
  onDragLeave: (event: DragEvent) => void;
  onDrop: (event: DragEvent) => void;
  onPointerDragOver: (event: CustomEvent<PointerDragDetail>) => void;
  onPointerDrop: (event: CustomEvent<PointerDragDetail>) => void;
} {
  function onDragEnter(event: DragEvent): void {
    const accepted = payloadAccepts(event);
    event.preventDefault();
    if (!accepted) return;
    api.clearTimer();
    api.setState("hover");
  }

  function onDragOver(event: DragEvent): void {
    const accepted = payloadAccepts(event);
    event.preventDefault();
    if (!accepted) return;
    // Calling `preventDefault` here opts the text into the
    // `drop` event. The browser requires the immediate target to
    // opt in on every `dragover` tick; failing to call this is
    // the most common reason a manual drop silently fails to
    // land on the intended target.
    if (event.dataTransfer) {
      try {
        event.dataTransfer.dropEffect = "copy";
      } catch {
        /* readonly in some WebKit builds; ignore. */
      }
    }
    api.setState("hover");
  }

  function onDragLeave(event: DragEvent): void {
    // Only clear the highlight when the related target is outside
    // this text node. Moving the pointer across the text children
    // must NOT clear the highlight.
    const related = event.relatedTarget as Node | null;
    const current = event.currentTarget as Node | null;
    if (related && current && current.contains(related)) {
      return;
    }
    api.setState("idle");
  }

  function onDrop(event: DragEvent): void {
    const accepted = payloadAccepts(event);
    event.preventDefault();
    if (!accepted) return;
    api.setState("dropped");
    // Close the in-memory drag session so the next drag must
    // open a new one. The matching `dragend` from the source
    // card will skip the close because the session token has
    // already advanced.
    endDragSession();
    api.scheduleReset();
  }

  function isCurrentPointerDrag(
    event: CustomEvent<PointerDragDetail>,
  ): boolean {
    const activeEntryId = activePointerDragEntryId();
    return activeEntryId !== null && activeEntryId === event.detail.entryId;
  }

  function onPointerDragOver(
    event: CustomEvent<PointerDragDetail>,
  ): void {
    if (!isCurrentPointerDrag(event)) return;
    event.preventDefault();
    api.clearTimer();
    api.setState("hover");
  }

  function onPointerDrop(event: CustomEvent<PointerDragDetail>): void {
    if (!isCurrentPointerDrag(event)) return;
    event.preventDefault();
    api.setState("dropped");
    api.scheduleReset();
  }

  return {
    onDragEnter,
    onDragOver,
    onDragLeave,
    onDrop,
    onPointerDragOver,
    onPointerDrop,
  };
}
