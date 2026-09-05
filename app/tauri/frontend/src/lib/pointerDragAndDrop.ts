// Reliable in-app drag channel for Tauri/WebKit.
//
// WebKit's HTML5 DataTransfer drag lifecycle is not reliable inside an
// embedded Tauri window: a visible mouse drag can arrive without
// dragover/drop, or with an empty DataTransfer. This module gives the
// desktop a deterministic pointer path that never crosses the clipboard.
//
// Only an integer entry id is kept in memory. No clipboard content, asset
// reference, snippet, hash, path or image bytes are involved.

import {
  beginDragSession,
  endDragSession,
  getActiveDragSessionEntryId,
} from "./dragAndDrop.ts";

export const POINTER_DRAG_OVER_EVENT = "clipvault-pointer-drag-over";
export const POINTER_DROP_EVENT = "clipvault-pointer-drop";
export const POINTER_DRAG_END_EVENT = "clipvault-pointer-drag-end";

const CARD_SELECTOR = '[data-testid="history-card"]';
const INTERACTIVE_SELECTOR =
  "[data-testid='history-card-menu'], [data-testid='history-card-menu-trigger'], [data-testid='history-card-pin'], [data-testid='history-card-title'], [role='button'], [role='menuitem'], input, button, textarea, [contenteditable='true']";
const ACTIVATION_DISTANCE = 6;
const DRAG_SELECTION_BLOCK_CLASS = "cv-pointer-dragging";

export interface PointerDragDetail {
  entryId: number;
  clientX: number;
  clientY: number;
}

interface PendingPointerDrag {
  entryId: number;
  pointerId: number;
  startX: number;
  startY: number;
  active: boolean;
  sessionToken: number | null;
  ghost: HTMLElement | null;
}

let pendingDrag: PendingPointerDrag | null = null;
let installedDocument: Document | null = null;
let installedWindow: Window | null = null;

function isValidEntryId(value: number): boolean {
  return Number.isFinite(value) && Number.isInteger(value) && value >= 0;
}

function dispatchAtPoint(
  doc: Document,
  eventName: string,
  event: PointerEvent,
  entryId: number,
): void {
  const target = doc.elementFromPoint(event.clientX, event.clientY);
  if (!target) return;
  const detail: PointerDragDetail = {
    entryId,
    clientX: event.clientX,
    clientY: event.clientY,
  };
  target.dispatchEvent(
    new CustomEvent<PointerDragDetail>(eventName, {
      bubbles: true,
      cancelable: true,
      composed: true,
      detail,
    }),
  );
}

function dispatchDragEnd(doc: Document, entryId: number): void {
  doc.dispatchEvent(
    new CustomEvent<{ entryId: number }>(POINTER_DRAG_END_EVENT, {
      bubbles: false,
      cancelable: false,
      detail: { entryId },
    }),
  );
}

function createDragGhost(doc: Document): HTMLElement {
  // The preview is intentionally generic. It confirms that the card is
  // being dragged without duplicating clipboard text, a user title, source
  // application details or image pixels into another DOM surface.
  const ghost = doc.createElement("div");
  ghost.className = "cv-pointer-drag-ghost";
  ghost.setAttribute("aria-hidden", "true");

  const header = doc.createElement("div");
  header.className = "cv-pointer-drag-ghost-header";
  const marker = doc.createElement("span");
  marker.className = "cv-pointer-drag-ghost-marker";
  const label = doc.createElement("span");
  label.textContent = "Captura";
  header.appendChild(marker);
  header.appendChild(label);

  const body = doc.createElement("div");
  body.className = "cv-pointer-drag-ghost-body";
  body.textContent = "Arrastrando…";

  const footer = doc.createElement("div");
  footer.className = "cv-pointer-drag-ghost-footer";
  footer.textContent = "Suelta en una colección";

  ghost.appendChild(header);
  ghost.appendChild(body);
  ghost.appendChild(footer);
  doc.body.appendChild(ghost);
  return ghost;
}

function positionDragGhost(ghost: HTMLElement, event: PointerEvent): void {
  ghost.style.transform = `translate3d(${Math.round(event.clientX + 14)}px, ${Math.round(event.clientY + 14)}px, 0)`;
}

function removeDragGhost(ghost: HTMLElement | null): void {
  if (ghost?.parentNode) {
    ghost.parentNode.removeChild(ghost);
  }
}

function setSelectionBlocked(doc: Document | null, blocked: boolean): void {
  doc?.body?.classList.toggle(DRAG_SELECTION_BLOCK_CLASS, blocked);
}

function finishPendingDrag(): void {
  const current = pendingDrag;
  pendingDrag = null;
  if (!current?.active) return;
  setSelectionBlocked(installedDocument, false);
  removeDragGhost(current.ghost);
  endDragSession(current.sessionToken ?? undefined);
  if (installedDocument) {
    dispatchDragEnd(installedDocument, current.entryId);
  }
}

function onPointerDown(event: PointerEvent): void {
  if (event.button !== 0 && event.pointerType !== "touch") return;
  const target = event.target;
  if (!(target instanceof Element)) return;
  const card = target.closest(CARD_SELECTOR);
  if (!card || target.closest(INTERACTIVE_SELECTOR)) return;
  const rawId = card.getAttribute("data-entry-id");
  const entryId = rawId === null ? Number.NaN : Number(rawId);
  if (!isValidEntryId(entryId)) return;

  // A new pointerdown supersedes a stale pending pointer without touching a
  // different active session unless it belonged to this controller.
  finishPendingDrag();
  // Prevent the browser's text-selection gesture from painting text across
  // neighbouring cards while the pointer is waiting for the activation
  // threshold. This is a card drag source, not a text-selection surface.
  event.preventDefault();
  pendingDrag = {
    entryId,
    pointerId: event.pointerId,
    startX: event.clientX,
    startY: event.clientY,
    active: false,
    sessionToken: null,
    ghost: null,
  };
}

function onPointerMove(event: PointerEvent): void {
  const current = pendingDrag;
  if (!current || current.pointerId !== event.pointerId) return;
  if (!current.active) {
    const dx = event.clientX - current.startX;
    const dy = event.clientY - current.startY;
    if (Math.hypot(dx, dy) < ACTIVATION_DISTANCE) return;
    const token = beginDragSession(current.entryId);
    if (token === null) {
      pendingDrag = null;
      return;
    }
    current.active = true;
    current.sessionToken = token;
    if (installedDocument) {
      setSelectionBlocked(installedDocument, true);
      current.ghost = createDragGhost(installedDocument);
      positionDragGhost(current.ghost, event);
    }
  }

  event.preventDefault();
  if (current.ghost) positionDragGhost(current.ghost, event);
  if (installedDocument) {
    dispatchAtPoint(
      installedDocument,
      POINTER_DRAG_OVER_EVENT,
      event,
      current.entryId,
    );
  }
}

function onPointerUp(event: PointerEvent): void {
  const current = pendingDrag;
  if (!current || current.pointerId !== event.pointerId) return;
  if (!current.active) {
    pendingDrag = null;
    return;
  }

  event.preventDefault();
  if (installedDocument) {
    dispatchAtPoint(installedDocument, POINTER_DROP_EVENT, event, current.entryId);
  }
  finishPendingDrag();
}

function onPointerCancel(): void {
  finishPendingDrag();
}

function onWindowBlur(): void {
  finishPendingDrag();
}

/**
 * Install the singleton pointer bridge. Repeated calls against the same
 * document are idempotent and return a no-op cleanup, so remounts cannot
 * multiply pointer handlers. The returned cleanup owns the first install.
 */
export function installPointerDragController(
  doc: Document | null = typeof document === "undefined" ? null : document,
): () => void {
  if (!doc) return () => {};
  if (installedDocument === doc) return () => {};
  if (installedDocument) uninstallPointerDragController();

  installedDocument = doc;
  installedWindow = doc.defaultView;
  doc.addEventListener("pointerdown", onPointerDown, true);
  doc.addEventListener("pointermove", onPointerMove, true);
  doc.addEventListener("pointerup", onPointerUp, true);
  doc.addEventListener("pointercancel", onPointerCancel, true);
  installedWindow?.addEventListener("blur", onWindowBlur);

  let released = false;
  return () => {
    if (released) return;
    released = true;
    if (installedDocument === doc) uninstallPointerDragController();
  };
}

export function uninstallPointerDragController(): void {
  if (!installedDocument) return;
  const doc = installedDocument;
  const win = installedWindow;
  finishPendingDrag();
  doc.removeEventListener("pointerdown", onPointerDown, true);
  doc.removeEventListener("pointermove", onPointerMove, true);
  doc.removeEventListener("pointerup", onPointerUp, true);
  doc.removeEventListener("pointercancel", onPointerCancel, true);
  win?.removeEventListener("blur", onWindowBlur);
  installedDocument = null;
  installedWindow = null;
}

export function isPointerDragActive(): boolean {
  return pendingDrag?.active === true;
}

export function activePointerDragEntryId(): number | null {
  if (!pendingDrag?.active) return null;
  return getActiveDragSessionEntryId();
}

/** Test-only state reset; production cleanup uses the controller cleanup. */
export function __resetPointerDragForTests(): void {
  finishPendingDrag();
  pendingDrag = null;
}
