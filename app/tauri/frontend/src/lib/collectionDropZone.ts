// Single delegated drop zone for the collection list.
//
// The previous implementation attached `dragenter` / `dragover` /
// `dragleave` / `drop` listeners to every `<li>` row inside
// `OrganizationSidebar.svelte`. That wiring is fragile: the row
// only fires the listeners when the DOM event target is the row
// itself or one of its descendants, but in WebKit/Tauri a stale
// `box-shadow`, an inherited `draggable` attribute, a child
// `<button>` element or the `overflow-y: auto` ancestor's
// hit-testing can all swallow the event before it bubbles to the
// row. The user-visible symptom is the row that never highlights
// and the card that never lands in the target collection.
//
// The fix is structural: install ONE delegated drop zone on the
// scrollable container itself and resolve the row under the
// pointer from the live event. The container is identified by a
// `data-collections-drop-viewport` attribute so the helper can be
// reused by any caller (the test harness exercises the same code
// path the production component does) and so a regression that
// drops the attribute surfaces as a failed assertion.
//
// Contract being pinned:
//
//   - The viewport must carry `data-collections-drop-viewport`.
//   - Each drop target row must carry
//     `data-drop-target="collection"` AND a numeric
//     `data-collection-id` attribute. Rows that do not satisfy
//     both attributes are ignored — the helper never falls back
//     to attribute-only inference that could be tricked by a
//     foreign drag.
//   - The helper resolves the row in this order:
//       1. Walk up from `event.target` to the viewport, stopping
//          at the first element with `data-drop-target="collection"`
//          whose `data-collection-id` resolves to a `user`
//          collection that lives inside the viewport.
//       2. Fall back to `document.elementFromPoint(clientX, clientY)`
//          when the walk-up fails — this is the WebKit path where
//          the event target is the viewport itself (the listener
//          fires once on the container, not on any descendant).
//       3. Return `null` when no candidate row resolves. The
//          caller treats that as a safe no-op.
//   - `dragover` and `drop` always `preventDefault` once a row
//          resolves so the browser fires the matching `drop`. The
//          helper falls back to the in-memory drag session the
//          controller opened after pointer/mouse activation so a WebKit/Tauri drag that
//          drops the DataTransfer still opts the row in.
//   - `dragleave` only clears the highlight when the related
//          target is outside the viewport; moving the pointer
//          between children of the same viewport keeps the
//          highlight alive.
//   - The helper never inspects clipboard content, snippets,
//          hashes, asset references, paths or image bytes. The
//          only payload field it reads is the opaque entry id the
//          card encodes through `parseDragPayloadFromTransfer`.

import type { Collection } from "../types.ts";
import {
  CLIPVAULT_ENTRY_MIME,
  acceptsDragOver,
  endDragSession,
  getActiveDragSessionEntryId,
  parseDragPayloadFromTransfer,
} from "./dragAndDrop.ts";
import {
  activePointerDragEntryId,
  type PointerDragDetail,
} from "./pointerDragAndDrop.ts";

/**
 * Attribute the scrollable viewport must carry so the delegated
 * drop zone can identify it without depending on class names or
 * DOM position.
 */
export const COLLECTIONS_DROP_VIEWPORT_ATTR = "data-collections-drop-viewport";

/**
 * Attribute each row must carry so the helper can resolve the
 * collection id without inspecting the Svelte render output. The
 * value MUST be the literal string `"collection"` for any row
 * that should accept drops; the helper ignores every other value
 * so a future render that adds a second drop-target role (for
 * example, a tag list) cannot accidentally wire the row to the
 * collection flow.
 */
export const COLLECTION_DROP_TARGET_ATTR = "data-drop-target";
export const COLLECTION_DROP_TARGET_VALUE = "collection";

/**
 * Attribute that exposes the numeric collection id of the row.
 * The helper refuses any non-integer id so a regression that wired
 * a stable_key or a name by mistake surfaces as a hard failure
 * rather than a silent drop.
 */
export const COLLECTION_DROP_ID_ATTR = "data-collection-id";

export interface ResolvedDropRow {
  row: Element;
  collection: Collection;
}

/**
 * Walk up from the live event target to the viewport, stopping at
 * the first element that carries `data-drop-target="collection"`
 * AND a numeric `data-collection-id` matching a `user`
 * collection. The row MUST live inside the viewport so a row
 * mounted by another sidebar cannot be impersonated.
 */
export function resolveDropRowFromTarget(
  target: EventTarget | null,
  viewport: Element,
  collections: readonly Collection[],
): ResolvedDropRow | null {
  let node: Node | null = target as Node | null;
  while (node && node !== viewport) {
    if (
      node instanceof Element &&
      node.getAttribute(COLLECTION_DROP_TARGET_ATTR) ===
        COLLECTION_DROP_TARGET_VALUE
    ) {
      return resolveRow(node, viewport, collections);
    }
    node = node.parentNode;
  }
  return null;
}

/**
 * Fallback resolver that uses `document.elementFromPoint` to
 * locate the element under the pointer. Used when the live event
 * target is the viewport itself (WebKit/Tauri quirk: the
 * listener fires on the container with `target === viewport`).
 * The helper reuses `resolveRow` for the validation step so the
 * two strategies share the same contract.
 */
export function resolveDropRowFromPoint(
  clientX: number,
  clientY: number,
  viewport: Element,
  collections: readonly Collection[],
): ResolvedDropRow | null {
  if (typeof document === "undefined") return null;
  const element = document.elementFromPoint(clientX, clientY);
  if (!element || !(element instanceof Element)) return null;
  if (!viewport.contains(element)) return null;
  let node: Element | null = element;
  while (node && node !== viewport) {
    if (
      node.getAttribute(COLLECTION_DROP_TARGET_ATTR) ===
      COLLECTION_DROP_TARGET_VALUE
    ) {
      return resolveRow(node, viewport, collections);
    }
    node = node.parentElement;
  }
  return null;
}

function resolveRow(
  row: Element,
  viewport: Element,
  collections: readonly Collection[],
): ResolvedDropRow | null {
  if (!viewport.contains(row)) return null;
  const idAttr = row.getAttribute(COLLECTION_DROP_ID_ATTR);
  if (!idAttr) return null;
  const id = Number(idAttr);
  if (!Number.isInteger(id) || !Number.isFinite(id) || id < 0) return null;
  const collection = collections.find((c) => c.id === id);
  if (!collection || collection.kind !== "user") return null;
  return { row, collection };
}

export interface CollectionDropZoneOptions {
  /** Snapshot of the collections rendered by the sidebar. */
  collections: readonly Collection[];
  /** Read the live viewport element the helper resolves rows against. */
  getViewport: () => Element | null;
  /** Read the current highlight (so the helper avoids redundant writes). */
  getDragOverCollectionId: () => number | null;
  /** Mutate the highlight reactively. */
  setDragOverCollectionId: (id: number | null) => void;
  /** Called when the drop resolves to a valid (entryId, collectionId) pair. */
  onCardDrop: (entryId: number, collectionId: number) => void;
  /** Called when the drop resolves to NO valid payload (foreign drag). */
  onForeignDrop?: () => void;
}

export interface CollectionDropZoneHandlers {
  onDragEnter: (event: DragEvent) => void;
  onDragOver: (event: DragEvent) => void;
  onDragLeave: (event: DragEvent) => void;
  onDrop: (event: DragEvent) => void;
  onPointerDragOver: (event: CustomEvent<PointerDragDetail>) => void;
  onPointerDrop: (event: CustomEvent<PointerDragDetail>) => void;
}

/**
 * Build the four delegated handlers the sidebar binds on the
 * `data-collections-drop-viewport` element. The handlers always
 * read the live viewport through `getViewport` so a sidebar that
 * re-mounts its row list keeps working without re-creating the
 * closures. The handlers are pure: they read the latest
 * collections snapshot through the bound `options.collections`
 * reference and forward the drop to the parent through
 * `onCardDrop`.
 */
export function createCollectionDropZoneHandlers(
  options: CollectionDropZoneOptions,
): CollectionDropZoneHandlers {
  // WebKit can dispatch the final `drop` on the viewport with a
  // missing/zeroed coordinate pair. Keep the last row hit during the
  // current drag as a geometry-only fallback. It contains no payload
  // or clipboard data and is invalidated when the pointer leaves the
  // viewport or a drop completes.
  let lastResolvedHit: ResolvedDropRow | null = null;

  function resolve(event: DragEvent): ResolvedDropRow | null {
    const viewport = options.getViewport();
    if (!viewport) return null;
    const fromTarget = resolveDropRowFromTarget(
      event.target,
      viewport,
      options.collections,
    );
    if (fromTarget) return fromTarget;
    if (
      typeof event.clientX === "number" &&
      typeof event.clientY === "number" &&
      Number.isFinite(event.clientX) &&
      Number.isFinite(event.clientY)
    ) {
      return resolveDropRowFromPoint(
        event.clientX,
        event.clientY,
        viewport,
        options.collections,
      );
    }
    return null;
  }

  function onDragEnter(event: DragEvent): void {
    const accepted = acceptsDragOver(event);
    const hit = resolve(event);
    if (!hit) return;
    lastResolvedHit = hit;
    event.preventDefault();
    if (!accepted) return;
    options.setDragOverCollectionId(hit.collection.id);
  }

  function onDragOver(event: DragEvent): void {
    const accepted = acceptsDragOver(event);
    const hit = resolve(event);
    if (!hit) return;
    lastResolvedHit = hit;
    // A valid user-collection row opts into the browser drop
    // protocol based on geometry, not on DataTransfer.types. In
    // WebKit/Tauri the types collection can be empty even for an
    // internal card drag. Payload validation remains strict in
    // onDrop, so accepting the browser event here cannot mutate the
    // app for a foreign drag.
    event.preventDefault();
    if (!accepted) return;
    if (event.dataTransfer) {
      try {
        event.dataTransfer.dropEffect = "copy";
      } catch {
        /* readonly in some WebKit builds; ignore. */
      }
    }
    if (options.getDragOverCollectionId() !== hit.collection.id) {
      options.setDragOverCollectionId(hit.collection.id);
    }
  }

  function onDragLeave(event: DragEvent): void {
    const viewport = options.getViewport();
    if (!viewport) return;
    // `dragleave` fires every time the pointer crosses a child
    // boundary inside the viewport. The `relatedTarget` check is
    // the standard escape-hatch browsers document for this case:
    // when the related target is still inside the viewport the
    // highlight must stay. When the related target is outside
    // (or null), the highlight clears.
    const related = event.relatedTarget as Node | null;
    if (related && viewport.contains(related)) {
      return;
    }
    lastResolvedHit = null;
    options.setDragOverCollectionId(null);
  }

  function onDrop(event: DragEvent): void {
    const hit = resolve(event) ?? lastResolvedHit;
    if (!hit) {
      // No target row resolves — the pointer is in the
      // viewport's empty space. Cancel the drop and clear the
      // session so a later foreign drag cannot impersonate a
      // card. The browser also expects `preventDefault` on a
      // valid drop target; we skip that branch because the row
      // never resolved and the default behaviour (no drop) is
      // already what we want.
      endDragSession();
      lastResolvedHit = null;
      options.onForeignDrop?.();
      options.setDragOverCollectionId(null);
      return;
    }
    event.preventDefault();
    // The DataTransfer MAY expose only one of the two
    // representations the card writes. WebKit/Tauri sometimes
    // strips the private MIME during `drop`, so we always try
    // the strict `text/plain` fallback too. When both
    // representations are missing (the WebKit/Tauri path that
    // filters the DataTransfer entirely) the helper consults
    // the in-memory drag session the card opened in
    // pointer/mouse activation.
    const privatePayload =
      event.dataTransfer?.getData(CLIPVAULT_ENTRY_MIME) ?? null;
    const textPayload = event.dataTransfer?.getData("text/plain") ?? null;
    const sessionEntryId = getActiveDragSessionEntryId();
    const entryId = parseDragPayloadFromTransfer({
      privatePayload,
      textPayload,
      sessionEntryId,
    });
    lastResolvedHit = null;
    options.setDragOverCollectionId(null);
    if (entryId == null) {
      endDragSession();
      options.onForeignDrop?.();
      return;
    }
    // Clear the session before dispatching so a parent handler
    // that immediately starts another drag cannot clear the
    // session that the new card just opened. The matching
    // `dragend` from the source card will skip the close
    // because the token no longer matches.
    endDragSession();
    options.onCardDrop(entryId, hit.collection.id);
  }

  /**
   * Pointer fallback for embedded WebKit. The pointer controller dispatches
   * this event on the element currently under the cursor, so it bubbles from
   * a row child to this same viewport. Hit-testing is performed on every
   * move, which also works after the user scrolls the collection list.
   */
  function onPointerDragOver(
    event: CustomEvent<PointerDragDetail>,
  ): void {
    const activeEntryId = activePointerDragEntryId();
    if (activeEntryId === null || activeEntryId !== event.detail.entryId) {
      return;
    }
    const viewport = options.getViewport();
    if (!viewport) return;
    const hit =
      resolveDropRowFromTarget(event.target, viewport, options.collections) ??
      resolveDropRowFromPoint(
        event.detail.clientX,
        event.detail.clientY,
        viewport,
        options.collections,
      );
    if (!hit) {
      if (options.getDragOverCollectionId() !== null) {
        options.setDragOverCollectionId(null);
      }
      return;
    }
    event.preventDefault();
    lastResolvedHit = hit;
    if (options.getDragOverCollectionId() !== hit.collection.id) {
      options.setDragOverCollectionId(hit.collection.id);
    }
  }

  function onPointerDrop(event: CustomEvent<PointerDragDetail>): void {
    const activeEntryId = activePointerDragEntryId();
    if (activeEntryId === null || activeEntryId !== event.detail.entryId) {
      return;
    }
    const viewport = options.getViewport();
    if (!viewport) return;
    const hit =
      resolveDropRowFromTarget(event.target, viewport, options.collections) ??
      resolveDropRowFromPoint(
        event.detail.clientX,
        event.detail.clientY,
        viewport,
        options.collections,
      );
    if (!hit) return;
    event.preventDefault();
    lastResolvedHit = null;
    options.setDragOverCollectionId(null);
    options.onCardDrop(activeEntryId, hit.collection.id);
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
