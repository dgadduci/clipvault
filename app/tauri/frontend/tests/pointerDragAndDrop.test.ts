import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

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
  KeyboardEventImpl,
  MouseEventImpl,
  PointerEventImpl,
} from "./_domPolyfill.ts";
import type { Collection } from "../src/types.ts";

const FRONTEND_ROOT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

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

test("pointercancel drops the ghost, ends the session and never dispatches a card-drop", { concurrency: false }, () => {
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

    let dropped: { entryId: number; collectionId: number } | null = null;
    const handlers = createCollectionDropZoneHandlers({
      collections: [makeCollection()],
      getViewport: () => viewport,
      getDragOverCollectionId: () => null,
      setDragOverCollectionId: () => {},
      onCardDrop: (entryId, collectionId) => {
        dropped = { entryId, collectionId };
      },
    });
    viewport.addEventListener(POINTER_DROP_EVENT, (event) => {
      handlers.onPointerDrop(event as CustomEvent);
    });
    document.hitTestElement = row;

    const card = document.createElement("article");
    card.setAttribute("data-testid", "history-card");
    card.setAttribute("data-entry-id", "44");
    document.body.appendChild(card);
    const cleanup = installPointerDragController(
      document as unknown as Document,
    );

    card.dispatchEvent(
      new PointerEventImpl("pointerdown", {
        bubbles: true,
        cancelable: true,
        pointerId: 21,
        clientX: 5,
        clientY: 5,
      }),
    );
    card.dispatchEvent(
      new PointerEventImpl("pointermove", {
        bubbles: true,
        pointerId: 21,
        clientX: 100,
        clientY: 100,
      }),
    );
    assert.equal(isPointerDragActive(), true);
    assert.ok(document.body.querySelector(".cv-pointer-drag-ghost"));

    document.dispatchEvent(
      new PointerEventImpl("pointercancel", {
        bubbles: true,
        pointerId: 21,
      }),
    );

    assert.equal(isPointerDragActive(), false);
    assert.equal(hasActiveDragSession(), false);
    assert.equal(document.body.querySelector(".cv-pointer-drag-ghost"), null);
    assert.equal(
      document.body.classList.contains("cv-pointer-dragging"),
      false,
    );
    assert.equal(dropped, null, "pointercancel must never dispatch a card-drop");
    cleanup();
  } finally {
    restore();
  }
});

test("window blur cancels an active drag and never dispatches a card-drop", { concurrency: false }, () => {
  const { document, window, restore } = installDomPolyfill();
  try {
    __resetDragSessionForTests();
    __resetPointerDragForTests();
    // The polyfill does not expose `document.defaultView`, so the
    // blur listener registration in `installPointerDragController`
    // would be a silent no-op. Patch the polyfill document to
    // surface a `defaultView` whose `addEventListener` /
    // `removeEventListener` / `dispatchEvent` mirror the production
    // window shape so the blur handler still runs when the window
    // fires a `blur` event.
    const blurListeners = new Set<(event: Event) => void>();
    const fakeWindow = {
      addEventListener(type: string, listener: (event: Event) => void): void {
        if (type === "blur") blurListeners.add(listener);
      },
      removeEventListener(type: string, listener: (event: Event) => void): void {
        if (type === "blur") blurListeners.delete(listener);
      },
      dispatchEvent(event: Event): boolean {
        for (const listener of Array.from(blurListeners)) {
          listener(event);
        }
        return true;
      },
    };
    (document as unknown as { defaultView: unknown }).defaultView = fakeWindow;
    // Mirror the patched defaultView on the polyfill's window
    // shim so any later code that reads it sees the same shape.
    Object.assign(window as unknown as Record<string, unknown>, fakeWindow);

    const viewport = document.createElement("ul");
    const row = document.createElement("li");
    row.setAttribute("data-drop-target", COLLECTION_DROP_TARGET_VALUE);
    row.setAttribute("data-collection-id", "9");
    viewport.appendChild(row);
    document.body.appendChild(viewport);

    let dropped: { entryId: number; collectionId: number } | null = null;
    const handlers = createCollectionDropZoneHandlers({
      collections: [makeCollection()],
      getViewport: () => viewport,
      getDragOverCollectionId: () => null,
      setDragOverCollectionId: () => {},
      onCardDrop: (entryId, collectionId) => {
        dropped = { entryId, collectionId };
      },
    });
    viewport.addEventListener(POINTER_DROP_EVENT, (event) => {
      handlers.onPointerDrop(event as CustomEvent);
    });
    document.hitTestElement = row;

    const card = document.createElement("article");
    card.setAttribute("data-testid", "history-card");
    card.setAttribute("data-entry-id", "45");
    document.body.appendChild(card);
    const cleanup = installPointerDragController(
      document as unknown as Document,
    );

    card.dispatchEvent(
      new PointerEventImpl("pointerdown", {
        bubbles: true,
        cancelable: true,
        pointerId: 22,
        clientX: 5,
        clientY: 5,
      }),
    );
    card.dispatchEvent(
      new PointerEventImpl("pointermove", {
        bubbles: true,
        pointerId: 22,
        clientX: 100,
        clientY: 100,
      }),
    );
    assert.equal(isPointerDragActive(), true);

    fakeWindow.dispatchEvent(
      new (globalThis as { Event: new (type: string) => Event }).Event("blur"),
    );

    assert.equal(isPointerDragActive(), false);
    assert.equal(hasActiveDragSession(), false);
    assert.equal(document.body.querySelector(".cv-pointer-drag-ghost"), null);
    assert.equal(dropped, null, "blur must never dispatch a card-drop");
    cleanup();
  } finally {
    restore();
  }
});

test("ghost element is rendered through a CSS class whose source rule declares pointer-events:none", { concurrency: false }, () => {
  const { document, restore } = installDomPolyfill();
  try {
    __resetDragSessionForTests();
    __resetPointerDragForTests();
    const card = document.createElement("article");
    card.setAttribute("data-testid", "history-card");
    card.setAttribute("data-entry-id", "46");
    document.body.appendChild(card);
    const cleanup = installPointerDragController(
      document as unknown as Document,
    );

    card.dispatchEvent(
      new PointerEventImpl("pointerdown", {
        bubbles: true,
        cancelable: true,
        pointerId: 23,
        clientX: 10,
        clientY: 10,
      }),
    );
    card.dispatchEvent(
      new PointerEventImpl("pointermove", {
        bubbles: true,
        pointerId: 23,
        clientX: 100,
        clientY: 100,
      }),
    );

    const ghost = document.body.querySelector(".cv-pointer-drag-ghost");
    assert.ok(ghost, "the ghost element must be appended to body during a drag");
    // The runtime ghost is a plain DOM node, so `pointer-events: none`
    // is enforced through the production CSS rule (not through an
    // inline style). Read the source of App.svelte so a regression
    // that dropped the rule surfaces as a failed assertion here.
    const appSource = loadSource("src/App.svelte");
    const ghostRule = appSource.match(
      /:global\(\.cv-pointer-drag-ghost\)\s*\{[\s\S]*?\}/,
    );
    assert.ok(
      ghostRule,
      "the production CSS must define a rule for the ghost element",
    );
    assert.match(
      ghostRule?.[0] ?? "",
      /pointer-events:\s*none/,
      "the ghost rule must declare pointer-events: none so the row under the cursor stays the drop target",
    );
    cleanup();
  } finally {
    restore();
  }
});

test("pointer drag starts a pending session only after the activation distance is crossed", { concurrency: false }, () => {
  const { document, restore } = installDomPolyfill();
  try {
    __resetDragSessionForTests();
    __resetPointerDragForTests();
    const card = document.createElement("article");
    card.setAttribute("data-testid", "history-card");
    card.setAttribute("data-entry-id", "47");
    document.body.appendChild(card);
    const cleanup = installPointerDragController(
      document as unknown as Document,
    );

    card.dispatchEvent(
      new PointerEventImpl("pointerdown", {
        bubbles: true,
        cancelable: true,
        pointerId: 24,
        clientX: 100,
        clientY: 100,
      }),
    );
    // 3px move: still under the activation threshold.
    card.dispatchEvent(
      new PointerEventImpl("pointermove", {
        bubbles: true,
        pointerId: 24,
        clientX: 103,
        clientY: 100,
      }),
    );
    assert.equal(
      isPointerDragActive(),
      false,
      "a small move below the activation threshold must not start a session",
    );
    assert.equal(hasActiveDragSession(), false);
    assert.equal(document.body.querySelector(".cv-pointer-drag-ghost"), null);
    // 6.01px move: the activation threshold is exceeded and the
    // session starts.
    card.dispatchEvent(
      new PointerEventImpl("pointermove", {
        bubbles: true,
        pointerId: 24,
        clientX: 106.5,
        clientY: 100.5,
      }),
    );
    assert.equal(isPointerDragActive(), true);
    assert.equal(hasActiveDragSession(), true);
    assert.ok(document.body.querySelector(".cv-pointer-drag-ghost"));
    cleanup();
  } finally {
    restore();
  }
});

test("pointer drag fires a single drop callback for an image card exactly once", { concurrency: false }, () => {
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

    let dropCount = 0;
    let lastDrop: { entryId: number; collectionId: number } | null = null;
    const handlers = createCollectionDropZoneHandlers({
      collections: [makeCollection()],
      getViewport: () => viewport,
      getDragOverCollectionId: () => null,
      setDragOverCollectionId: () => {},
      onCardDrop: (entryId, collectionId) => {
        dropCount += 1;
        lastDrop = { entryId, collectionId };
      },
    });
    viewport.addEventListener(POINTER_DROP_EVENT, (event) => {
      handlers.onPointerDrop(event as CustomEvent);
    });
    document.hitTestElement = row;

    const imageCard = document.createElement("article");
    imageCard.setAttribute("data-testid", "history-card");
    imageCard.setAttribute("data-entry-id", "48");
    imageCard.setAttribute("data-content-type", "image");
    document.body.appendChild(imageCard);
    const cleanup = installPointerDragController(
      document as unknown as Document,
    );

    imageCard.dispatchEvent(
      new PointerEventImpl("pointerdown", {
        bubbles: true,
        cancelable: true,
        pointerId: 25,
        clientX: 5,
        clientY: 5,
      }),
    );
    imageCard.dispatchEvent(
      new PointerEventImpl("pointermove", {
        bubbles: true,
        pointerId: 25,
        clientX: 60,
        clientY: 60,
      }),
    );
    imageCard.dispatchEvent(
      new PointerEventImpl("pointerup", {
        bubbles: true,
        cancelable: true,
        pointerId: 25,
        clientX: 60,
        clientY: 60,
      }),
    );

    assert.equal(dropCount, 1, "image card drop must fire onCardDrop exactly once");
    assert.deepEqual(lastDrop, { entryId: 48, collectionId: 9 });
    cleanup();
  } finally {
    restore();
  }
});

test("pointer drag fires a single drop callback for a text card exactly once", { concurrency: false }, () => {
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

    let dropCount = 0;
    let lastDrop: { entryId: number; collectionId: number } | null = null;
    const handlers = createCollectionDropZoneHandlers({
      collections: [makeCollection()],
      getViewport: () => viewport,
      getDragOverCollectionId: () => null,
      setDragOverCollectionId: () => {},
      onCardDrop: (entryId, collectionId) => {
        dropCount += 1;
        lastDrop = { entryId, collectionId };
      },
    });
    viewport.addEventListener(POINTER_DROP_EVENT, (event) => {
      handlers.onPointerDrop(event as CustomEvent);
    });
    document.hitTestElement = row;

    const textCard = document.createElement("article");
    textCard.setAttribute("data-testid", "history-card");
    textCard.setAttribute("data-entry-id", "49");
    textCard.setAttribute("data-content-type", "text");
    document.body.appendChild(textCard);
    const cleanup = installPointerDragController(
      document as unknown as Document,
    );

    textCard.dispatchEvent(
      new PointerEventImpl("pointerdown", {
        bubbles: true,
        cancelable: true,
        pointerId: 26,
        clientX: 5,
        clientY: 5,
      }),
    );
    textCard.dispatchEvent(
      new PointerEventImpl("pointermove", {
        bubbles: true,
        pointerId: 26,
        clientX: 60,
        clientY: 60,
      }),
    );
    textCard.dispatchEvent(
      new PointerEventImpl("pointerup", {
        bubbles: true,
        cancelable: true,
        pointerId: 26,
        clientX: 60,
        clientY: 60,
      }),
    );

    assert.equal(dropCount, 1, "text card drop must fire onCardDrop exactly once");
    assert.deepEqual(lastDrop, { entryId: 49, collectionId: 9 });
    cleanup();
  } finally {
    restore();
  }
});

test("mouse fallback completes a drop when WebKit omits pointer move and up events", { concurrency: false }, () => {
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

    let dropped: { entryId: number; collectionId: number } | null = null;
    const handlers = createCollectionDropZoneHandlers({
      collections: [makeCollection()],
      getViewport: () => viewport,
      getDragOverCollectionId: () => null,
      setDragOverCollectionId: () => {},
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
    document.hitTestElement = row;

    const card = document.createElement("article");
    card.setAttribute("data-testid", "history-card");
    card.setAttribute("data-entry-id", "57");
    document.body.appendChild(card);
    const cleanup = installPointerDragController(
      document as unknown as Document,
    );

    card.dispatchEvent(
      new MouseEventImpl("mousedown", {
        bubbles: true,
        cancelable: true,
        button: 0,
        clientX: 5,
        clientY: 5,
      }),
    );
    card.dispatchEvent(
      new MouseEventImpl("mousemove", {
        bubbles: true,
        cancelable: true,
        button: 0,
        clientX: 60,
        clientY: 60,
      }),
    );
    assert.equal(isPointerDragActive(), true);
    assert.ok(document.body.querySelector(".cv-pointer-drag-ghost"));
    card.dispatchEvent(
      new MouseEventImpl("mouseup", {
        bubbles: true,
        cancelable: true,
        button: 0,
        clientX: 60,
        clientY: 60,
      }),
    );

    assert.deepEqual(dropped, { entryId: 57, collectionId: 9 });
    assert.equal(isPointerDragActive(), false);
    assert.equal(hasActiveDragSession(), false);
    cleanup();
  } finally {
    restore();
  }
});

test("pointer drag captures the source and Escape cancels without dropping", { concurrency: false }, () => {
  const { document, restore } = installDomPolyfill();
  try {
    __resetDragSessionForTests();
    __resetPointerDragForTests();
    const card = document.createElement("article");
    card.setAttribute("data-testid", "history-card");
    card.setAttribute("data-entry-id", "58");
    let captured = 0;
    let released = 0;
    (
      card as unknown as {
        setPointerCapture(pointerId: number): void;
        releasePointerCapture(pointerId: number): void;
      }
    ).setPointerCapture = () => {
      captured += 1;
    };
    (
      card as unknown as {
        setPointerCapture(pointerId: number): void;
        releasePointerCapture(pointerId: number): void;
      }
    ).releasePointerCapture = () => {
      released += 1;
    };
    document.body.appendChild(card);
    const cleanup = installPointerDragController(
      document as unknown as Document,
    );

    card.dispatchEvent(
      new PointerEventImpl("pointerdown", {
        bubbles: true,
        cancelable: true,
        pointerId: 31,
      }),
    );
    card.dispatchEvent(
      new PointerEventImpl("pointermove", {
        bubbles: true,
        pointerId: 31,
        clientX: 20,
        clientY: 20,
      }),
    );
    assert.equal(captured, 1);
    assert.equal(isPointerDragActive(), true);

    document.dispatchEvent(
      new KeyboardEventImpl("keydown", {
        bubbles: true,
        cancelable: true,
        key: "Escape",
      }),
    );

    assert.equal(released, 1);
    assert.equal(isPointerDragActive(), false);
    assert.equal(hasActiveDragSession(), false);
    assert.equal(document.body.querySelector(".cv-pointer-drag-ghost"), null);
    cleanup();
  } finally {
    restore();
  }
});

test("pointer and mouse paths ignore interactive card controls", { concurrency: false }, () => {
  const { document, restore } = installDomPolyfill();
  try {
    __resetDragSessionForTests();
    __resetPointerDragForTests();
    const card = document.createElement("article");
    card.setAttribute("data-testid", "history-card");
    card.setAttribute("data-entry-id", "59");
    const button = document.createElement("button");
    button.setAttribute("data-testid", "history-card-pin");
    card.appendChild(button);
    document.body.appendChild(card);
    const cleanup = installPointerDragController(
      document as unknown as Document,
    );

    const pointerDown = new PointerEventImpl("pointerdown", {
      bubbles: true,
      cancelable: true,
      pointerId: 32,
    });
    button.dispatchEvent(pointerDown);
    const mouseDown = new MouseEventImpl("mousedown", {
      bubbles: true,
      cancelable: true,
      button: 0,
    });
    button.dispatchEvent(mouseDown);

    assert.equal(pointerDown.defaultPrevented, false);
    assert.equal(mouseDown.defaultPrevented, false);
    assert.equal(isPointerDragActive(), false);
    assert.equal(hasActiveDragSession(), false);
    cleanup();
  } finally {
    restore();
  }
});

test("title surface starts a drag without disabling its click and double-click path", { concurrency: false }, () => {
  const { document, restore } = installDomPolyfill();
  try {
    __resetDragSessionForTests();
    __resetPointerDragForTests();
    const card = document.createElement("article");
    card.setAttribute("data-testid", "history-card");
    card.setAttribute("data-entry-id", "60");
    const title = document.createElement("div");
    title.setAttribute("data-testid", "history-card-title");
    title.setAttribute("role", "button");
    card.appendChild(title);
    document.body.appendChild(card);

    let droppedEntryId: number | null = null;
    document.addEventListener(POINTER_DROP_EVENT, (event) => {
      droppedEntryId = (event as CustomEvent<{ entryId: number }>).detail.entryId;
    });
    const dropTarget = document.createElement("div");
    document.body.appendChild(dropTarget);
    document.hitTestElement = dropTarget;
    const cleanup = installPointerDragController(
      document as unknown as Document,
    );

    const mouseDown = new MouseEventImpl("mousedown", {
      bubbles: true,
      cancelable: true,
      button: 0,
      clientX: 5,
      clientY: 5,
    });
    title.dispatchEvent(mouseDown);
    assert.equal(mouseDown.defaultPrevented, false);

    title.dispatchEvent(
      new MouseEventImpl("mousemove", {
        bubbles: true,
        cancelable: true,
        button: 0,
        clientX: 60,
        clientY: 60,
      }),
    );
    assert.equal(isPointerDragActive(), true);
    title.dispatchEvent(
      new MouseEventImpl("mouseup", {
        bubbles: true,
        cancelable: true,
        button: 0,
        clientX: 60,
        clientY: 60,
      }),
    );

    assert.equal(droppedEntryId, 60);
    assert.equal(isPointerDragActive(), false);
    assert.equal(hasActiveDragSession(), false);
    cleanup();
  } finally {
    restore();
  }
});
