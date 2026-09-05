// Reliable in-app drag channel for Tauri/WebKit.
//
// WebKit's HTML5 DataTransfer drag lifecycle is not reliable inside an
// embedded Tauri window: a visible mouse drag can arrive without
// dragover/drop, or with an empty DataTransfer. This module gives the
// desktop a deterministic pointer/mouse path that never crosses the
// clipboard. The mouse listeners are a compatibility fallback for
// WebViews that expose a mouse gesture but do not deliver a complete
// Pointer Events sequence.
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
const INTERACTIVE_SELECTORS = [
  "[data-testid='history-card-menu']",
  "[data-testid='history-card-menu-trigger']",
  "[data-testid='history-card-pin']",
  "[data-testid='history-card-title']",
  "[role='button']",
  "[role='menuitem']",
  "input",
  "button",
  "textarea",
  "[contenteditable='true']",
] as const;
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
  sourceElement: Element | null;
}

let pendingDrag: PendingPointerDrag | null = null;
let installedDocument: Document | null = null;
let installedWindow: Window | null = null;

function isValidEntryId(value: number): boolean {
  return Number.isFinite(value) && Number.isInteger(value) && value >= 0;
}

type DragCoordinates = Pick<
  MouseEvent,
  "clientX" | "clientY" | "preventDefault"
>;

function dispatchAtPoint(
  doc: Document,
  eventName: string,
  event: DragCoordinates,
  entryId: number,
): Element | null {
  const target = doc.elementFromPoint(event.clientX, event.clientY);
  if (!target) return null;
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
  return target;
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

function isInteractiveTarget(target: Element): boolean {
  return INTERACTIVE_SELECTORS.some(
    (selector) => target.closest(selector) !== null,
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

function positionDragGhost(ghost: HTMLElement, event: DragCoordinates): void {
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

function capturePointer(element: Element | null, pointerId: number): void {
  if (!element) return;
  const candidate = element as Element & {
    setPointerCapture?: (id: number) => void;
  };
  try {
    candidate.setPointerCapture?.(pointerId);
  } catch {
    // Some WebKit versions expose the method but reject it when the
    // pointer has already moved. The document-level fallback remains active.
  }
}

function releasePointer(element: Element | null, pointerId: number): void {
  if (!element) return;
  const candidate = element as Element & {
    releasePointerCapture?: (id: number) => void;
  };
  try {
    candidate.releasePointerCapture?.(pointerId);
  } catch {
    // Cleanup must never turn a completed drop into an uncaught exception.
  }
}

function finishPendingDrag(): void {
  const current = pendingDrag;
  pendingDrag = null;
  if (!current) return;
  if (!current.active) {
    // A click that never crossed the activation threshold still may have
    // installed pointer capture. Release it so the next click is not
    // redirected to a stale card.
    releasePointer(current.sourceElement, current.pointerId);
    return;
  }
  setSelectionBlocked(installedDocument, false);
  removeDragGhost(current.ghost);
  releasePointer(current.sourceElement, current.pointerId);
  endDragSession(current.sessionToken ?? undefined);
  if (installedDocument) {
    dispatchDragEnd(installedDocument, current.entryId);
  }
}

function startPendingDrag(
  event: DragCoordinates,
  target: Element,
  pointerId: number,
): Element | null {
  const card = target.closest(CARD_SELECTOR);
  if (!card || isInteractiveTarget(target)) return null;
  const rawId = card.getAttribute("data-entry-id");
  const entryId = rawId === null ? Number.NaN : Number(rawId);
  if (!isValidEntryId(entryId)) return null;

  // A new pointerdown supersedes a stale pending pointer without touching a
  // different active session unless it belonged to this controller.
  finishPendingDrag();
  pendingDrag = {
    entryId,
    pointerId,
    startX: event.clientX,
    startY: event.clientY,
    active: false,
    sessionToken: null,
    ghost: null,
    sourceElement: card,
  };
  return card;
}

function onPointerDown(event: PointerEvent): void {
  if (event.button !== 0 && event.pointerType !== "touch") return;
  const target = event.target;
  if (!(target instanceof Element)) return;
  const card = startPendingDrag(event, target, event.pointerId);
  if (!card) return;
  // Prevent the browser's text-selection gesture from painting text across
  // neighbouring cards while the pointer is waiting for the activation
  // threshold. This is a card drag source, not a text-selection surface.
  event.preventDefault();
  capturePointer(card, event.pointerId);
}

function updatePendingDrag(event: DragCoordinates): void {
  const current = pendingDrag;
  if (!current) return;
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

function onPointerMove(event: PointerEvent): void {
  const current = pendingDrag;
  if (!current || current.pointerId !== event.pointerId) return;
  updatePendingDrag(event);
}

function onMouseDown(event: MouseEvent): void {
  // Pointer-capable browsers emit mousedown after pointerdown. The pending
  // guard prevents the compatibility path from opening a second session.
  if (pendingDrag || event.button !== 0) return;
  const target = event.target;
  if (!(target instanceof Element)) return;
  const card = startPendingDrag(event, target, 1);
  if (!card) return;
  event.preventDefault();
}

function onMouseMove(event: MouseEvent): void {
  // The mouse path is intentionally also allowed to finish a pending pointer
  // gesture. This covers WebViews that emit pointerdown but then expose only
  // mousemove/mouseup while the button is held.
  if (!pendingDrag) return;
  updatePendingDrag(event);
}

function onPointerUp(event: PointerEvent): void {
  const current = pendingDrag;
  if (!current || current.pointerId !== event.pointerId) return;
  if (!current.active) {
    finishPendingDrag();
    return;
  }

  event.preventDefault();
  if (installedDocument) {
    dispatchAtPoint(installedDocument, POINTER_DROP_EVENT, event, current.entryId);
  }
  finishPendingDrag();
}

function onMouseUp(event: MouseEvent): void {
  const current = pendingDrag;
  if (!current) return;
  if (!current.active) {
    finishPendingDrag();
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

function onKeyDown(event: KeyboardEvent): void {
  if (event.key !== "Escape" || !pendingDrag) return;
  event.preventDefault();
  event.stopPropagation();
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
  doc.addEventListener("mousedown", onMouseDown, true);
  doc.addEventListener("mousemove", onMouseMove, true);
  doc.addEventListener("mouseup", onMouseUp, true);
  doc.addEventListener("keydown", onKeyDown, true);
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
  doc.removeEventListener("mousedown", onMouseDown, true);
  doc.removeEventListener("mousemove", onMouseMove, true);
  doc.removeEventListener("mouseup", onMouseUp, true);
  doc.removeEventListener("keydown", onKeyDown, true);
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
