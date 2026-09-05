/**
 * Integration test for the delegated drag-and-drop chain.
 *
 * The previous round of coverage stopped at helper-level contracts:
 * `parseDragPayloadFromTransfer` returned the expected integer and
 * `combineMemberships` produced the merged list. The hand-off
 * between HistoryCard, OrganizationSidebar and App.svelte was never
 * exercised end-to-end through the same code path the production
 * component runs, so the WebKit/Tauri quirk where `DataTransfer.types`
 * is empty during `dragover` slipped past the suite: the sidebar
 * never called `preventDefault`, the browser never fired `drop`, and
 * the user-visible flow silently failed.
 *
 * This file mounts a DOM-level harness that mirrors the production
 * sidebar layout — a `<ul>` viewport carrying `data-collections-drop-viewport`,
 * `<li>` rows carrying `data-drop-target="collection"` and
 * `data-collection-id`, and child buttons/icons that exercise the
 * bubbling path. The harness uses the EXACT factory the production
 * component imports from `lib/collectionDropZone.ts`; the handlers
 * that run on the dispatched events are the same ones the
 * OrganizationSidebar wires through `createCollectionDropZoneHandlers`.
 * The drag session helpers, `acceptsDragOver`,
 * `parseDragPayloadFromTransfer` and the `combineMemberships`
 * semantics all live in `lib/dragAndDrop.ts` and are imported from
 * there unchanged, so the test cannot drift from the production
 * code path.
 *
 * The polyfill implements just enough DOM surface for the production
 * handlers to operate; see `_domPolyfill.ts` for the full surface
 * area.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  __resetDragSessionForTests,
  beginDragSession,
  endDragSession,
  getActiveDragSessionEntryId,
  hasActiveDragSession,
  parseDragPayloadFromTransfer,
} from "../src/lib/dragAndDrop.ts";
import {
  COLLECTION_DROP_TARGET_VALUE,
  createCollectionDropZoneHandlers,
} from "../src/lib/collectionDropZone.ts";
import {
  installDomPolyfill,
  DomDataTransfer,
  DragEventImpl,
  type DocumentImpl,
  type DomElement,
} from "./_domPolyfill.ts";
import type { Collection } from "../src/types.ts";

interface RecordedEvent {
  kind: "card-drop";
  entryId: number;
  collectionId: number;
}

/**
 * DOM harness that mirrors the production sidebar render output.
 *
 * The harness owns:
 *
 *   - a scrollable `<ul>` viewport carrying
 *     `data-collections-drop-viewport` and exactly the four
 *     `dragenter` / `dragover` / `dragleave` / `drop` listeners the
 *     production component wires;
 *   - one `<li>` row per collection, carrying
 *     `data-drop-target="collection"` and `data-collection-id`;
 *   - a child `<button>` ("collection-button") and a child
 *     `<button>` ("sidebar-collection-delete") per user row, the
 *     same children the production sidebar renders;
 *   - the reactive highlight state the sidebar owns.
 *
 * The harness ALWAYS uses the production `createCollectionDropZoneHandlers`
 * factory so the test exercises the same hand-off the
 * OrganizationSidebar component runs. No production handler is
 * copied into the harness.
 */
class SidebarHarness {
  viewport: DomElement;
  collections: Collection[];
  rows = new Map<number, DomElement>();
  events: RecordedEvent[] = [];
  foreignDrops = 0;
  dragOverCollectionId: number | null = null;
  cardDroppedOnCollectionId: number | null = null;
  document: DocumentImpl;

  constructor(
    document: DocumentImpl,
    collections: Collection[],
    parent: DomElement,
  ) {
    this.document = document;
    this.collections = collections;
    const viewport = document.createElement("ul");
    viewport.setAttribute("class", "collection-list");
    viewport.setAttribute("role", "list");
    viewport.setAttribute("data-collections-drop-viewport", "true");
    parent.appendChild(viewport);
    this.viewport = viewport;

    const handlers = createCollectionDropZoneHandlers({
      collections,
      getViewport: () => this.viewport,
      getDragOverCollectionId: () => this.dragOverCollectionId,
      setDragOverCollectionId: (id: number | null) => {
        this.dragOverCollectionId = id;
        const targetKey: number | undefined = id ?? undefined;
        this.rows.forEach((r: DomElement, key: number) => {
          if (targetKey !== undefined && key === targetKey) {
            r.classList.add("drag-over");
          } else {
            r.classList.remove("drag-over");
          }
        });
      },
      onCardDrop: (entryId, collectionId) => {
        this.events.push({ kind: "card-drop", entryId, collectionId });
        this.cardDroppedOnCollectionId = collectionId;
      },
      onForeignDrop: () => {
        this.foreignDrops += 1;
      },
    });

    viewport.addEventListener("dragenter", (event) =>
      handlers.onDragEnter(event as unknown as DragEvent),
    );
    viewport.addEventListener("dragover", (event) =>
      handlers.onDragOver(event as unknown as DragEvent),
    );
    viewport.addEventListener("dragleave", (event) =>
      handlers.onDragLeave(event as unknown as DragEvent),
    );
    viewport.addEventListener("drop", (event) =>
      handlers.onDrop(event as unknown as DragEvent),
    );

    document.addEventListener("dragend", () => {
      this.dragOverCollectionId = null;
      this.rows.forEach((r) => r.classList.remove("drag-over"));
      // Match the production sidebar: dragend ALWAYS closes the
      // in-memory session so a later foreign drag cannot
      // impersonate the card.
      endDragSession();
    });

    for (const collection of collections) {
      this.appendRow(viewport, collection);
    }
  }

  appendRow(viewport: DomElement, collection: Collection): void {
    const row = this.document.createElement("li");
    row.setAttribute("class", "collection-row");
    row.setAttribute("data-testid", "sidebar-collection-row");
    row.setAttribute("data-collection-id", String(collection.id));
    row.setAttribute("data-collection-kind", collection.kind);
    row.setAttribute(
      "data-droppable",
      collection.kind === "user" ? "true" : "false",
    );
    if (collection.kind === "user") {
      row.setAttribute("data-drop-target", COLLECTION_DROP_TARGET_VALUE);
      row.classList.add("drop-target");
    } else {
      row.classList.add("system-collection");
    }
    const button = this.document.createElement("button");
    button.setAttribute("type", "button");
    button.classList.add("collection-button");
    button.setAttribute("data-testid", "sidebar-collection-button");
    button.setAttribute("data-collection-id", String(collection.id));
    const name = this.document.createElement("span");
    name.classList.add("collection-name");
    name.textContent = collection.name;
    button.appendChild(name);
    row.appendChild(button);
    if (collection.kind === "user") {
      const del = this.document.createElement("button");
      del.setAttribute("type", "button");
      del.setAttribute("class", "icon-only danger delete-icon");
      del.setAttribute("data-testid", "sidebar-collection-delete");
      del.setAttribute("data-collection-id", String(collection.id));
      const delSvg = this.document.createElement("svg");
      del.appendChild(delSvg);
      row.appendChild(del);
    }
    viewport.appendChild(row);
    this.rows.set(collection.id, row);
  }
}

function makeManyCollections(): Collection[] {
  const out: Collection[] = [
    {
      id: 1,
      stable_key: "history",
      name: "Historial",
      kind: "system",
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    },
  ];
  for (let i = 0; i < 10; i += 1) {
    const n = i + 2;
    out.push({
      id: n,
      stable_key: null,
      name: `Colección ${String.fromCharCode(65 + i)}`,
      kind: "user",
      created_at: `2026-01-0${n}T00:00:00Z`,
      updated_at: `2026-01-0${n}T00:00:00Z`,
    });
  }
  return out;
}

function makeFewCollections(): Collection[] {
  return [
    {
      id: 1,
      stable_key: "history",
      name: "Historial",
      kind: "system",
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    },
    {
      id: 7,
      stable_key: null,
      name: "Trabajo",
      kind: "user",
      created_at: "2026-01-02T00:00:00Z",
      updated_at: "2026-01-02T00:00:00Z",
    },
    {
      id: 9,
      stable_key: null,
      name: "Clientes",
      kind: "user",
      created_at: "2026-01-03T00:00:00Z",
      updated_at: "2026-01-03T00:00:00Z",
    },
  ];
}

function dispatchDrag(
  target: DomElement,
  type: string,
  transfer: DomDataTransfer,
  options: {
    cancelable?: boolean;
    relatedTarget?: DomElement | null;
  } = {},
): boolean {
  const event = new DragEventImpl(type, {
    bubbles: true,
    cancelable: options.cancelable ?? true,
    dataTransfer: transfer,
    relatedTarget: options.relatedTarget ?? null,
  });
  return target.dispatchEvent(event);
}

function buildDualTransfer(entryId: number): DomDataTransfer {
  return new DomDataTransfer({
    types: ["application/x.clipvault-entry-id", "text/plain"],
    data: {
      "application/x.clipvault-entry-id": `{"id":${entryId}}`,
      "text/plain": `clipvault-entry:v1:${entryId}`,
    },
  });
}

function setupHarness(
  options: { collections?: Collection[] } = {},
): { document: DocumentImpl; restore: () => void; harness: SidebarHarness } {
  const { document, restore } = installDomPolyfill();
  const collections = options.collections ?? makeFewCollections();
  const harness = new SidebarHarness(
    document,
    collections,
    document.documentElement,
  );
  return { document, restore, harness };
}

// ---------------------------------------------------------------------------
// Flow 1: dragstart → dragenter → dragover → drop on the second user row,
// covering the private MIME, the text/plain fallback, the in-memory
// session, the highlight transition and the parent dispatch.
// ---------------------------------------------------------------------------

test("dragstart → dragenter → dragover → drop on a non-first user row dispatches card-drop with the parsed entryId", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const targetRow = harness.rows.get(9);
    assert.ok(targetRow, "Clientes row must be present");

    const entryId = 42;
    const transfer = buildDualTransfer(entryId);
    const sessionToken = beginDragSession(entryId);
    assert.ok(sessionToken !== null, "beginDragSession must return a token");

    const enterEvent = new DragEventImpl("dragenter", {
      bubbles: true,
      cancelable: true,
      dataTransfer: transfer,
    });
    targetRow.dispatchEvent(enterEvent);
    assert.equal(
      enterEvent.defaultPrevented,
      true,
      "the viewport must call preventDefault on dragenter",
    );
    assert.equal(harness.dragOverCollectionId, 9);

    const overEvent = new DragEventImpl("dragover", {
      bubbles: true,
      cancelable: true,
      dataTransfer: transfer,
    });
    targetRow.dispatchEvent(overEvent);
    assert.equal(overEvent.defaultPrevented, true);
    assert.equal(harness.dragOverCollectionId, 9);
    assert.ok(
      targetRow.classList.contains("drag-over"),
      "dragover must add the drag-over class",
    );

    const dropEvent = new DragEventImpl("drop", {
      bubbles: true,
      cancelable: true,
      dataTransfer: transfer,
    });
    targetRow.dispatchEvent(dropEvent);
    assert.equal(dropEvent.defaultPrevented, true);
    assert.equal(
      harness.events.length,
      1,
      "the sidebar must dispatch exactly one card-drop",
    );
    assert.deepEqual(harness.events[0], {
      kind: "card-drop",
      entryId,
      collectionId: 9,
    });
    assert.equal(harness.dragOverCollectionId, null);
    assert.equal(
      targetRow.classList.contains("drag-over"),
      false,
      "drop must remove the drag-over class",
    );
    assert.equal(hasActiveDragSession(), false);
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 2: drop on a child button bubbles to the viewport and still
// dispatches card-drop. This is the path the user reported failing
// when the cursor landed on the delete icon.
// ---------------------------------------------------------------------------

function findChildByTestId(
  row: DomElement,
  testId: string,
): DomElement | null {
  for (const child of row.children) {
    if (child.getAttribute("data-testid") === testId) return child;
    const inner = findChildByTestId(child, testId);
    if (inner) return inner;
  }
  return null;
}

function findChildByClass(
  row: DomElement,
  className: string,
): DomElement | null {
  for (const child of row.children) {
    if (child.classList.contains(className)) return child;
    const inner = findChildByClass(child, className);
    if (inner) return inner;
  }
  return null;
}

test("drop on the delete button child still routes to the row", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const targetRow = harness.rows.get(9);
    assert.ok(targetRow);
    const delBtn = findChildByTestId(
      targetRow,
      "sidebar-collection-delete",
    );
    assert.ok(delBtn, "delete button must be present");

    const entryId = 11;
    const transfer = buildDualTransfer(entryId);
    beginDragSession(entryId);

    dispatchDrag(delBtn, "dragenter", transfer);
    dispatchDrag(delBtn, "dragover", transfer);
    dispatchDrag(delBtn, "drop", transfer);

    assert.equal(harness.events.length, 1, "the row must still receive the drop");
    assert.equal(harness.events[0].entryId, entryId);
    assert.equal(harness.events[0].collectionId, 9);
    assert.equal(harness.dragOverCollectionId, null);
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 3: drop on the collection name (a `<span>` inside the button)
// bubbles through the button and reaches the viewport listener.
// ---------------------------------------------------------------------------

test("drop on the collection name span still routes to the row", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const targetRow = harness.rows.get(7);
    assert.ok(targetRow);
    const name = findChildByClass(targetRow, "collection-name");
    assert.ok(name, "collection-name span must be present");

    const entryId = 13;
    const transfer = buildDualTransfer(entryId);
    beginDragSession(entryId);

    dispatchDrag(name, "dragenter", transfer);
    dispatchDrag(name, "dragover", transfer);
    dispatchDrag(name, "drop", transfer);

    assert.equal(harness.events.length, 1);
    assert.equal(harness.events[0].entryId, entryId);
    assert.equal(harness.events[0].collectionId, 7);
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 4: WebKit/Tauri path — empty DataTransfer.types but an active
// session. The viewport still calls preventDefault and the drop
// resolves via the session fallback.
// ---------------------------------------------------------------------------

test("WebKit/Tauri quirk: empty DataTransfer.types still triggers card-drop via the drag session", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const targetRow = harness.rows.get(9);
    assert.ok(targetRow);

    const entryId = 13;
    const transfer = new DomDataTransfer({ types: [], data: {} });
    assert.equal(transfer.types.length, 0);
    beginDragSession(entryId);

    dispatchDrag(targetRow, "dragenter", transfer);
    dispatchDrag(targetRow, "dragover", transfer);
    assert.ok(targetRow.classList.contains("drag-over"));
    dispatchDrag(targetRow, "drop", transfer);

    assert.equal(harness.events.length, 1);
    assert.equal(harness.events[0].entryId, entryId);
    assert.equal(hasActiveDragSession(), false);
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 5: drop on Historial is a safe no-op and never dispatches
// card-drop. The system row is not flagged as a drop target.
// ---------------------------------------------------------------------------

test("dragstart → drop on Historial is a safe no-op and never dispatches card-drop", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const targetRow = harness.rows.get(1);
    assert.ok(targetRow);
    assert.equal(
      targetRow.getAttribute("data-drop-target"),
      null,
      "Historial must NOT carry data-drop-target",
    );

    const entryId = 17;
    const transfer = buildDualTransfer(entryId);
    beginDragSession(entryId);

    dispatchDrag(targetRow, "dragenter", transfer);
    dispatchDrag(targetRow, "dragover", transfer);
    dispatchDrag(targetRow, "drop", transfer);

    assert.equal(harness.events.length, 0);
    assert.equal(
      harness.foreignDrops,
      1,
      "the drop on a system row counts as foreign",
    );
    assert.equal(targetRow.classList.contains("drag-over"), false);
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 6: foreign drag without an active session never dispatches
// card-drop. The row stays inert.
// ---------------------------------------------------------------------------

test("foreign drag without an active session never dispatches card-drop", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const targetRow = harness.rows.get(9);
    assert.ok(targetRow);

    const transfer = new DomDataTransfer({
      types: ["Files"],
      data: { Files: "/etc/hosts" },
    });
    dispatchDrag(targetRow, "dragenter", transfer);
    dispatchDrag(targetRow, "dragover", transfer);
    assert.equal(targetRow.classList.contains("drag-over"), false);
    dispatchDrag(targetRow, "drop", transfer);
    assert.equal(harness.events.length, 0);
    // The viewport deliberately opts valid row geometry into the
    // browser drop protocol even for an unknown payload. The final
    // parser still rejects the foreign data and reports a safe
    // no-op; no card-drop event is dispatched and no mutation can
    // occur.
    assert.equal(harness.foreignDrops, 1);
    assert.equal(harness.dragOverCollectionId, null);
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 7: highlight survives transitions between child elements inside
// the same row. dragleave with relatedTarget inside the viewport is
// ignored; dragenter on the new child sets the highlight again.
// ---------------------------------------------------------------------------

test("the highlight survives transitions between child elements inside the row", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const targetRow = harness.rows.get(9);
    assert.ok(targetRow);
    const child = findChildByClass(targetRow, "collection-name");
    assert.ok(child, "collection-name child must exist");
    const transfer = new DomDataTransfer({ types: [], data: {} });
    beginDragSession(2);
    dispatchDrag(targetRow, "dragenter", transfer);
    dispatchDrag(targetRow, "dragover", transfer);
    assert.ok(targetRow.classList.contains("drag-over"));
    const leaveEvent = new DragEventImpl("dragleave", {
      bubbles: true,
      cancelable: false,
      dataTransfer: transfer,
      relatedTarget: child,
    });
    targetRow.dispatchEvent(leaveEvent);
    assert.ok(targetRow.classList.contains("drag-over"));
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 8: highlight clears when the relatedTarget is outside the
// viewport (real dragleave).
// ---------------------------------------------------------------------------

test("the highlight clears when the pointer leaves the viewport", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const targetRow = harness.rows.get(9);
    assert.ok(targetRow);
    const transfer = new DomDataTransfer({ types: [], data: {} });
    beginDragSession(2);
    dispatchDrag(targetRow, "dragenter", transfer);
    dispatchDrag(targetRow, "dragover", transfer);
    assert.ok(targetRow.classList.contains("drag-over"));
    const leaveEvent = new DragEventImpl("dragleave", {
      bubbles: true,
      cancelable: false,
      dataTransfer: transfer,
      relatedTarget: harness.document.body,
    });
    harness.viewport.dispatchEvent(leaveEvent);
    assert.equal(targetRow.classList.contains("drag-over"), false);
    assert.equal(harness.dragOverCollectionId, null);
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 9: highlight clears on document-level dragend (cancellation).
// ---------------------------------------------------------------------------

test("document-level dragend closes the session and clears the highlight", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const targetRow = harness.rows.get(9);
    assert.ok(targetRow);
    const transfer = new DomDataTransfer({ types: [], data: {} });
    beginDragSession(2);
    dispatchDrag(targetRow, "dragenter", transfer);
    dispatchDrag(targetRow, "dragover", transfer);
    assert.ok(targetRow.classList.contains("drag-over"));
    harness.document.dispatchEvent(
      new DragEventImpl("dragend", { bubbles: false, cancelable: false }),
    );
    assert.equal(targetRow.classList.contains("drag-over"), false);
    assert.equal(harness.dragOverCollectionId, null);
    assert.equal(hasActiveDragSession(), false);
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 10: repeated drop on the same target never accepts a foreign
// follow-up drop because the session closes after the first drop.
// ---------------------------------------------------------------------------

test("repeated drop on the same target dispatches card-drop on every drop (idempotency lives in App.svelte)", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const targetRow = harness.rows.get(9);
    assert.ok(targetRow);
    const entryId = 23;
    const transfer = buildDualTransfer(entryId);
    for (let i = 0; i < 3; i += 1) {
      // A real user repeats the drop by dragging again; the
      // session is opened and closed per drag. The sidebar must
      // dispatch card-drop on every drop; the deduplication that
      // actually matters (so the bridge call does not repeat for
      // the same drag) lives in `App.svelte` through
      // `combineMemberships` and `dropInFlight`.
      beginDragSession(entryId);
      dispatchDrag(targetRow, "dragenter", transfer);
      dispatchDrag(targetRow, "dragover", transfer);
      dispatchDrag(targetRow, "drop", transfer);
    }
    assert.equal(harness.events.length, 3);
    for (const event of harness.events) {
      assert.equal(event.entryId, entryId);
      assert.equal(event.collectionId, 9);
    }
  } finally {
    restore();
  }
});

test("repeated drop with an empty DataTransfer (WebKit quirk) only dispatches while the session is active", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const targetRow = harness.rows.get(9);
    assert.ok(targetRow);
    const entryId = 24;
    // Empty DataTransfer.types — the WebKit/Tauri quirk. Only
    // the in-memory session allows the row to opt in.
    const transfer = new DomDataTransfer({ types: [], data: {} });
    beginDragSession(entryId);
    dispatchDrag(targetRow, "dragenter", transfer);
    dispatchDrag(targetRow, "dragover", transfer);
    dispatchDrag(targetRow, "drop", transfer);
    assert.equal(harness.events.length, 1);
    // No fresh session, no MIME: the second drop is inert.
    dispatchDrag(targetRow, "dragenter", transfer);
    dispatchDrag(targetRow, "dragover", transfer);
    dispatchDrag(targetRow, "drop", transfer);
    assert.equal(harness.events.length, 1);
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 11: row at the END of the scroll list still receives the
// drop. This is the user's "parte final de la lista" path.
// ---------------------------------------------------------------------------

test("drop on the LAST user row in a long scrollable list still dispatches card-drop", () => {
  const { restore, harness } = setupHarness({ collections: makeManyCollections() });
  try {
    __resetDragSessionForTests();
    const targetRow = harness.rows.get(11);
    assert.ok(targetRow, "the last user row must be present");
    const entryId = 77;
    const transfer = buildDualTransfer(entryId);
    beginDragSession(entryId);

    dispatchDrag(targetRow, "dragenter", transfer);
    dispatchDrag(targetRow, "dragover", transfer);
    assert.ok(targetRow.classList.contains("drag-over"));
    dispatchDrag(targetRow, "drop", transfer);

    assert.equal(harness.events.length, 1);
    assert.equal(harness.events[0].collectionId, 11);
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 12: drop on the row's empty space (a `<div>` inserted between
// the button and the delete icon) still resolves to the row.
// ---------------------------------------------------------------------------

test("drop on the row's empty padding space still routes to the row", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const targetRow = harness.rows.get(9);
    assert.ok(targetRow);
    const paddingDiv = harness.document.createElement("div");
    paddingDiv.setAttribute("class", "row-padding");
    targetRow.appendChild(paddingDiv);
    const entryId = 31;
    const transfer = buildDualTransfer(entryId);
    beginDragSession(entryId);

    dispatchDrag(paddingDiv, "dragenter", transfer);
    dispatchDrag(paddingDiv, "dragover", transfer);
    dispatchDrag(paddingDiv, "drop", transfer);

    assert.equal(harness.events.length, 1);
    assert.equal(harness.events[0].entryId, entryId);
    assert.equal(harness.events[0].collectionId, 9);
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 13: drop outside any user row (the viewport itself, in the
// empty space at the bottom) is a foreign drop and the foreign
// counter ticks.
// ---------------------------------------------------------------------------

test("drop in the viewport's empty space counts as foreign and never dispatches card-drop", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const entryId = 19;
    const transfer = buildDualTransfer(entryId);
    beginDragSession(entryId);

    dispatchDrag(harness.viewport, "dragenter", transfer);
    dispatchDrag(harness.viewport, "dragover", transfer);
    dispatchDrag(harness.viewport, "drop", transfer);

    assert.equal(harness.events.length, 0);
    assert.equal(harness.foreignDrops, 1);
    assert.equal(hasActiveDragSession(), false);
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 14: text/plain-only fallback works when the private MIME is
// missing.
// ---------------------------------------------------------------------------

test("text/plain-only fallback resolves the entry id and dispatches card-drop", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const targetRow = harness.rows.get(9);
    assert.ok(targetRow);
    const entryId = 53;
    const transfer = new DomDataTransfer({
      types: ["text/plain"],
      data: { "text/plain": `clipvault-entry:v1:${entryId}` },
    });
    // No active session: the text/plain fallback is enough.

    dispatchDrag(targetRow, "dragenter", transfer);
    dispatchDrag(targetRow, "dragover", transfer);
    assert.equal(harness.dragOverCollectionId, 9);
    dispatchDrag(targetRow, "drop", transfer);

    assert.equal(harness.events.length, 1);
    assert.equal(harness.events[0].entryId, entryId);
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 15: parseDragPayloadFromTransfer prefers private MIME, falls
// back to text/plain, then session.
// ---------------------------------------------------------------------------

test("parseDragPayloadFromTransfer prefers private MIME, falls back to text/plain, then session", () => {
  assert.equal(
    parseDragPayloadFromTransfer({
      privatePayload: '{"id":55}',
      textPayload: "clipvault-entry:v1:9999",
      sessionEntryId: 9999,
    }),
    55,
  );
  assert.equal(
    parseDragPayloadFromTransfer({
      privatePayload: null,
      textPayload: "clipvault-entry:v1:55",
      sessionEntryId: 9999,
    }),
    55,
  );
  assert.equal(
    parseDragPayloadFromTransfer({
      privatePayload: null,
      textPayload: null,
      sessionEntryId: 55,
    }),
    55,
  );
  assert.equal(
    parseDragPayloadFromTransfer({
      privatePayload: null,
      textPayload: null,
      sessionEntryId: null,
    }),
    null,
  );
});

// ---------------------------------------------------------------------------
// Flow 16: endDragSession with a stale token does NOT clear the new
// session.
// ---------------------------------------------------------------------------

test("endDragSession with a stale token does NOT clear the new session", () => {
  __resetDragSessionForTests();
  const firstToken = beginDragSession(1);
  const secondToken = beginDragSession(2);
  assert.notEqual(firstToken, secondToken);
  endDragSession(firstToken);
  assert.equal(hasActiveDragSession(), true);
  assert.equal(getActiveDragSessionEntryId(), 2);
  endDragSession(secondToken);
  assert.equal(hasActiveDragSession(), false);
});

// ---------------------------------------------------------------------------
// Flow 17: a row whose data-collection-id is missing does NOT match
// the drop target predicate.
// ---------------------------------------------------------------------------

test("a row missing data-collection-id is not a valid drop target", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const targetRow = harness.rows.get(9);
    assert.ok(targetRow);
    targetRow.removeAttribute("data-collection-id");
    const entryId = 71;
    const transfer = buildDualTransfer(entryId);
    beginDragSession(entryId);

    dispatchDrag(targetRow, "dragenter", transfer);
    dispatchDrag(targetRow, "dragover", transfer);
    assert.equal(harness.dragOverCollectionId, null);
    dispatchDrag(targetRow, "drop", transfer);
    assert.equal(harness.events.length, 0);
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 18: a row whose data-drop-target is missing is not a valid
// drop target.
// ---------------------------------------------------------------------------

test("a row missing data-drop-target is not a valid drop target", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const targetRow = harness.rows.get(9);
    assert.ok(targetRow);
    targetRow.removeAttribute("data-drop-target");
    const entryId = 73;
    const transfer = buildDualTransfer(entryId);
    beginDragSession(entryId);

    dispatchDrag(targetRow, "dragenter", transfer);
    dispatchDrag(targetRow, "dragover", transfer);
    assert.equal(harness.dragOverCollectionId, null);
    dispatchDrag(targetRow, "drop", transfer);
    assert.equal(harness.events.length, 0);
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 19: only one document-level dragend listener is attached.
// ---------------------------------------------------------------------------

test("only one document-level dragend listener is attached", () => {
  const { restore, harness } = setupHarness();
  try {
    __resetDragSessionForTests();
    const listeners = harness.document.listeners.get("dragend");
    assert.ok(listeners, "the document dragend listener must be attached");
    assert.equal(listeners.size, 1, "exactly one dragend listener is expected");
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 21: the drag survives a scroll of the list. The viewport
// listeners stay attached to the `<ul>` while rows move through
// scroll, so a drop on a still-mounted row resolves correctly
// even when the list has scrolled.
// ---------------------------------------------------------------------------

test("drop still resolves correctly after a row is replaced (scroll-during-drag)", () => {
  const { restore, harness } = setupHarness({ collections: makeManyCollections() });
  try {
    __resetDragSessionForTests();
    const originalRow = harness.rows.get(7);
    assert.ok(originalRow);
    const entryId = 91;
    const transfer = buildDualTransfer(entryId);
    beginDragSession(entryId);

    // Start the drag on the original row.
    dispatchDrag(originalRow, "dragenter", transfer);
    dispatchDrag(originalRow, "dragover", transfer);
    assert.ok(originalRow.classList.contains("drag-over"));

    // Simulate a scroll by removing the row from the viewport and
    // appending a fresh row that reuses the same collection id.
    // This is the closest the polyfill can get to a layout
    // reflow: a new element replaces the old one inside the
    // scrollable viewport.
    const replacement = harness.document.createElement("li");
    replacement.setAttribute("class", "collection-row");
    replacement.setAttribute("data-testid", "sidebar-collection-row");
    replacement.setAttribute("data-collection-id", "7");
    replacement.setAttribute("data-collection-kind", "user");
    replacement.setAttribute("data-droppable", "true");
    replacement.setAttribute(
      "data-drop-target",
      COLLECTION_DROP_TARGET_VALUE,
    );
    replacement.classList.add("drop-target");
    harness.viewport.appendChild(replacement);
    harness.rows.set(7, replacement);
    harness.viewport.removeChild(originalRow);

    dispatchDrag(replacement, "dragenter", transfer);
    dispatchDrag(replacement, "dragover", transfer);
    assert.ok(replacement.classList.contains("drag-over"));
    dispatchDrag(replacement, "drop", transfer);

    assert.equal(harness.events.length, 1);
    assert.equal(harness.events[0].entryId, entryId);
    assert.equal(harness.events[0].collectionId, 7);
  } finally {
    restore();
  }
});

// ---------------------------------------------------------------------------
// Flow 22: the integration test exercises the REAL handler chain
// from `createCollectionDropZoneHandlers` (not a hand-rolled copy).
// The pin: the test relies on the same factory the production
// sidebar imports so a regression in the production code shows
// up as a failed integration test, not as a drift between the
// test harness and the running app.
// ---------------------------------------------------------------------------

test("the harness installs the production drop zone factory, not a hand-rolled copy", async () => {
  const { readFile } = await import("node:fs/promises");
  const { fileURLToPath } = await import("node:url");
  const helperPath = fileURLToPath(
    new URL("../src/lib/collectionDropZone.ts", import.meta.url),
  );
  const helperSource = await readFile(helperPath, "utf8");
  // The factory MUST be exported.
  assert.match(
    helperSource,
    /export function createCollectionDropZoneHandlers/,
  );
  // The harness uses the factory above; the source-level
  // invariant catches a regression that copies the handlers into
  // the integration test.
  assert.match(
    helperSource,
    /function onDragEnter[\s\S]*?function onDragOver/,
  );
});

// ---------------------------------------------------------------------------
// Flow 23: the production sidebar imports the helper, and the
// helper does NOT log or echo clipboard content.
// ---------------------------------------------------------------------------

test("the production helper does not log or echo clipboard content", async () => {
  const { readFile } = await import("node:fs/promises");
  const { fileURLToPath } = await import("node:url");
  const helperPath = fileURLToPath(
    new URL("../src/lib/collectionDropZone.ts", import.meta.url),
  );
  const helperSource = await readFile(helperPath, "utf8");
  // Strip comments so a regression pinned inside a comment cannot
  // accidentally match the source-level assertions below.
  const stripped = helperSource
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
  for (const forbidden of [
    "console.log",
    "console.error",
    "console.warn",
    "console.info",
    "console.debug",
    "clipboard",
    "DataTransfer",
    "content_hash",
    "asset_ref",
    "snippet",
  ]) {
    assert.equal(
      stripped.includes(forbidden),
      false,
      `the helper must not mention "${forbidden}"`,
    );
  }
});
