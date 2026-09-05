import { test } from "node:test";
import assert from "node:assert/strict";

import {
  __resetDragSessionForTests,
  hasActiveDragSession,
} from "../src/lib/dragAndDrop.ts";
import {
  __resetPointerDragForTests,
  installPointerDragController,
  isPointerDragActive,
  POINTER_DRAG_OVER_EVENT,
  POINTER_DROP_EVENT,
} from "../src/lib/pointerDragAndDrop.ts";
import {
  COLLECTION_DROP_TARGET_VALUE,
  createCollectionDropZoneHandlers,
} from "../src/lib/collectionDropZone.ts";
import { createCardDropTextHandlers } from "../src/lib/cardDropTextHandlers.ts";
import {
  installDomPolyfill,
  PointerEventImpl,
} from "./_domPolyfill.ts";
import type { Collection } from "../src/types.ts";

function makeCollection(): Collection {
  return {
    id: 9,
    stable_key: null,
    name: "Trabajo",
    kind: "user",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
  };
}

test("pointer drag uses hit-testing for a scrolled collection row and persists the drop callback", { concurrency: false }, () => {
  const { document, restore } = installDomPolyfill();
  try {
    __resetDragSessionForTests();
    __resetPointerDragForTests();
    const viewport = document.createElement("ul");
    const row = document.createElement("li");
    row.setAttribute("data-drop-target", COLLECTION_DROP_TARGET_VALUE);
    row.setAttribute("data-collection-id", "9");
    viewport.appendChild(row);
    document.body.appendChild(viewport);

    let highlighted: number | null = null;
    let dropped: { entryId: number; collectionId: number } | null = null;
    const handlers = createCollectionDropZoneHandlers({
      collections: [makeCollection()],
      getViewport: () => viewport,
      getDragOverCollectionId: () => highlighted,
      setDragOverCollectionId: (id) => {
        highlighted = id;
      },
      onCardDrop: (entryId, collectionId) => {
        dropped = { entryId, collectionId };
      },
    });
    viewport.addEventListener(POINTER_DRAG_OVER_EVENT, (event) => {
      handlers.onPointerDragOver(event as CustomEvent);
    });
    viewport.addEventListener(POINTER_DROP_EVENT, (event) => {
      handlers.onPointerDrop(event as CustomEvent);
    });

    const card = document.createElement("article");
    card.setAttribute("data-testid", "history-card");
    card.setAttribute("data-entry-id", "42");
    document.body.appendChild(card);
    document.hitTestElement = row;
    const cleanup = installPointerDragController(
      document as unknown as Document,
    );

    card.dispatchEvent(
      new PointerEventImpl("pointerdown", {
        bubbles: true,
        cancelable: true,
        pointerId: 7,
        clientX: 10,
        clientY: 10,
      }),
    );
    card.dispatchEvent(
      new PointerEventImpl("pointermove", {
        bubbles: true,
        cancelable: true,
        pointerId: 7,
        clientX: 100,
        clientY: 100,
      }),
    );

    assert.equal(isPointerDragActive(), true);
    assert.equal(hasActiveDragSession(), true);
    assert.equal(highlighted, 9);
    assert.equal(document.body.classList.contains("cv-pointer-dragging"), true);
    const ghost = document.body.querySelector(".cv-pointer-drag-ghost");
    assert.ok(ghost);
    assert.equal(ghost?.getAttribute("aria-hidden"), "true");
    assert.equal(
      ghost?.querySelector(".cv-pointer-drag-ghost-header")?.textContent,
      "",
    );
    assert.equal(
      ghost?.querySelector(".cv-pointer-drag-ghost-body")?.textContent,
      "Arrastrando…",
    );
    assert.equal(
      ghost?.querySelector(".cv-pointer-drag-ghost-footer")?.textContent,
      "Suelta en una colección",
    );

    card.dispatchEvent(
      new PointerEventImpl("pointerup", {
        bubbles: true,
        cancelable: true,
        pointerId: 7,
        clientX: 100,
        clientY: 100,
      }),
    );

    assert.deepEqual(dropped, { entryId: 42, collectionId: 9 });
    assert.equal(isPointerDragActive(), false);
    assert.equal(hasActiveDragSession(), false);
    assert.equal(document.body.classList.contains("cv-pointer-dragging"), false);
    assert.equal(document.body.querySelector(".cv-pointer-drag-ghost"), null);
    cleanup();
  } finally {
    restore();
  }
});

test("pointer drag prevents native text selection while waiting for activation", { concurrency: false }, () => {
  const { document, restore } = installDomPolyfill();
  try {
    __resetDragSessionForTests();
    __resetPointerDragForTests();
    const card = document.createElement("article");
    card.setAttribute("data-testid", "history-card");
    card.setAttribute("data-entry-id", "52");
    document.body.appendChild(card);
    const cleanup = installPointerDragController(
      document as unknown as Document,
    );

    const down = new PointerEventImpl("pointerdown", {
      bubbles: true,
      cancelable: true,
      pointerId: 11,
    });
    card.dispatchEvent(down);

    assert.equal(down.defaultPrevented, true);
    assert.equal(document.body.querySelector(".cv-pointer-drag-ghost"), null);
    cleanup();
  } finally {
    restore();
  }
});

test("pointer drag keeps an external pointer outside ClipVault inert", { concurrency: false }, () => {
  const { document, restore } = installDomPolyfill();
  try {
    __resetDragSessionForTests();
    __resetPointerDragForTests();
    const outside = document.createElement("div");
    document.body.appendChild(outside);
    document.hitTestElement = outside;
    const card = document.createElement("article");
    card.setAttribute("data-testid", "history-card");
    card.setAttribute("data-entry-id", "42");
    document.body.appendChild(card);

    card.dispatchEvent(
      new PointerEventImpl("pointerdown", {
        bubbles: true,
        cancelable: true,
        pointerId: 8,
        clientX: 10,
        clientY: 10,
      }),
    );
    card.dispatchEvent(
      new PointerEventImpl("pointermove", {
        bubbles: true,
        cancelable: true,
        pointerId: 8,
        clientX: 100,
        clientY: 100,
      }),
    );
    card.dispatchEvent(
      new PointerEventImpl("pointerup", {
        bubbles: true,
        cancelable: true,
        pointerId: 8,
        clientX: 100,
        clientY: 100,
      }),
    );

    assert.equal(isPointerDragActive(), false);
    assert.equal(hasActiveDragSession(), false);
  } finally {
    restore();
  }
});

test("pointer drag reaches the bottom drop indicator through the same hit-test channel", { concurrency: false }, () => {
  const { document, restore } = installDomPolyfill();
  try {
    __resetDragSessionForTests();
    __resetPointerDragForTests();
    const indicator = document.createElement("div");
    document.body.appendChild(indicator);
    document.hitTestElement = indicator;
    let state = "idle";
    const handlers = createCardDropTextHandlers({
      setState: (next) => {
        state = next;
      },
      scheduleReset: () => {},
      clearTimer: () => {},
    });
    indicator.addEventListener(POINTER_DRAG_OVER_EVENT, (event) => {
      handlers.onPointerDragOver(event as CustomEvent);
    });
    indicator.addEventListener(POINTER_DROP_EVENT, (event) => {
      handlers.onPointerDrop(event as CustomEvent);
    });

    const card = document.createElement("article");
    card.setAttribute("data-testid", "history-card");
    card.setAttribute("data-entry-id", "51");
    document.body.appendChild(card);
    const cleanup = installPointerDragController(
      document as unknown as Document,
    );
    card.dispatchEvent(
      new PointerEventImpl("pointerdown", {
        bubbles: true,
        pointerId: 10,
      }),
    );
    card.dispatchEvent(
      new PointerEventImpl("pointermove", {
        bubbles: true,
        pointerId: 10,
        clientX: 20,
        clientY: 20,
      }),
    );
    assert.equal(state, "hover");
    card.dispatchEvent(
      new PointerEventImpl("pointerup", {
        bubbles: true,
        pointerId: 10,
        clientX: 20,
        clientY: 20,
      }),
    );
    assert.equal(state, "dropped");
    cleanup();
  } finally {
    restore();
  }
});

test("pointer controller installation is idempotent and cleanup removes its listeners", { concurrency: false }, () => {
  const { document, restore } = installDomPolyfill();
  try {
    __resetPointerDragForTests();
    const firstCleanup = installPointerDragController(
      document as unknown as Document,
    );
    const secondCleanup = installPointerDragController(
      document as unknown as Document,
    );
    const card = document.createElement("article");
    card.setAttribute("data-testid", "history-card");
    card.setAttribute("data-entry-id", "42");
    document.body.appendChild(card);
    card.dispatchEvent(
      new PointerEventImpl("pointerdown", {
        bubbles: true,
        pointerId: 9,
      }),
    );
    card.dispatchEvent(
      new PointerEventImpl("pointermove", {
        bubbles: true,
        pointerId: 9,
        clientX: 10,
        clientY: 10,
      }),
    );
    assert.equal(isPointerDragActive(), true);
    firstCleanup();
    secondCleanup();
    assert.equal(isPointerDragActive(), false);
    assert.equal(hasActiveDragSession(), false);
  } finally {
    restore();
  }
});
