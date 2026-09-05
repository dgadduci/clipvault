/**
 * Coverage for the desktop drop indicator text (`CardDropText.svelte`).
 *
 * The indicator is the user-visible drop cue the desktop renders
 * directly below the rail of cards. The contract being pinned:
 *
 *   - It accepts `dragenter` / `dragover` from a card payload (or
 *     a drag session opened by the card) and switches to a
 *     highlight tone.
 *   - It rejects foreign payloads (no MIME, no session) so the
 *     text stays inert.
 *   - On `drop`, it switches to a success tone.
 *   - On `dragleave` outside the text, on a `dragend` from the
 *     source or on the timer reset, it goes back to the idle tone
 *     so the text never stays highlighted after a cancelled drag.
 *   - It does NOT mutate the entry or the bridge; the indicator is
 *     a visual cue only.
 *
 * The harness uses the EXACT factory exported by
 * `lib/cardDropTextHandlers.ts` so the production component and
 * the test exercise the same handlers. The DOM polyfill provides
 * a real `dragenter`/`dragover`/`dragleave`/`drop` chain so the
 * `preventDefault()` call that opts the text into `drop` runs
 * against a faithful event surface.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  __resetDragSessionForTests,
  beginDragSession,
  endDragSession,
  hasActiveDragSession,
} from "../src/lib/dragAndDrop.ts";
import {
  createCardDropTextHandlers,
  payloadAccepts,
  type CardDropTextState,
} from "../src/lib/cardDropTextHandlers.ts";
import {
  installDomPolyfill,
  DomDataTransfer,
  DragEventImpl,
  type DocumentImpl,
  type DomElement,
} from "./_domPolyfill.ts";

interface IndicatorHarness {
  root: DomElement;
  states: CardDropTextState[];
  drop: ReturnType<typeof createCardDropTextHandlers>;
  document: DocumentImpl;
}

function dispatchDrag(
  target: DomElement,
  type: string,
  transfer: DomDataTransfer,
  options: { relatedTarget?: DomElement | null; currentTarget?: DomElement | null } = {},
): boolean {
  const event = new DragEventImpl(type, {
    bubbles: true,
    cancelable: true,
    dataTransfer: transfer,
    relatedTarget: options.relatedTarget ?? null,
  });
  // The polyfill sets `currentTarget = target` on dispatch but the
  // dragleave handler reads it from the event, so let the caller
  // override.
  if (options.currentTarget) {
    Object.defineProperty(event, "currentTarget", {
      value: options.currentTarget,
    });
  }
  return target.dispatchEvent(event);
}

function setupHarness(): {
  document: DocumentImpl;
  restore: () => void;
  harness: IndicatorHarness;
} {
  const { document, restore } = installDomPolyfill();
  const root = document.createElement("div");
  root.setAttribute("class", "card-drop-text");
  root.setAttribute("data-testid", "card-drop-text");
  root.setAttribute("data-drop-state", "idle");
  root.setAttribute("role", "note");
  document.body.appendChild(root);

  const states: CardDropTextState[] = ["idle"];
  let resetTimer: ReturnType<typeof setTimeout> | null = null;

  const drop = createCardDropTextHandlers({
    setState(state) {
      states.push(state);
      root.setAttribute("data-drop-state", state);
      root.classList.toggle("hover", state === "hover");
      root.classList.toggle("dropped", state === "dropped");
    },
    scheduleReset() {
      if (resetTimer !== null) clearTimeout(resetTimer);
      resetTimer = setTimeout(() => {
        states.push("idle");
        root.setAttribute("data-drop-state", "idle");
        root.classList.remove("dropped");
        root.classList.remove("hover");
        resetTimer = null;
      }, 1800);
    },
    clearTimer() {
      if (resetTimer !== null) {
        clearTimeout(resetTimer);
        resetTimer = null;
      }
    },
  });

  root.addEventListener("dragenter", (event) =>
    drop.onDragEnter(event as unknown as DragEvent),
  );
  root.addEventListener("dragover", (event) =>
    drop.onDragOver(event as unknown as DragEvent),
  );
  root.addEventListener("dragleave", (event) =>
    drop.onDragLeave(event as unknown as DragEvent),
  );
  root.addEventListener("drop", (event) =>
    drop.onDrop(event as unknown as DragEvent),
  );

  return {
    document,
    restore,
    harness: { root, states, drop, document },
  };
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

// ---------------------------------------------------------------------------
// `payloadAccepts` (the pure predicate every handler consults).
// ---------------------------------------------------------------------------

test("payloadAccepts: dual MIME + text/plain returns true", () => {
  const transfer = buildDualTransfer(11);
  const event = {
    dataTransfer: transfer,
  } as unknown as DragEvent;
  assert.equal(payloadAccepts(event), true);
});

test("payloadAccepts: empty types + active session returns true", () => {
  __resetDragSessionForTests();
  beginDragSession(7);
  const transfer = new DomDataTransfer({ types: [], data: {} });
  const event = {
    dataTransfer: transfer,
  } as unknown as DragEvent;
  assert.equal(payloadAccepts(event), true);
});

test("payloadAccepts: foreign types + no session returns false", () => {
  __resetDragSessionForTests();
  const transfer = new DomDataTransfer({
    types: ["Files"],
    data: { Files: "/etc/hosts" },
  });
  const event = {
    dataTransfer: transfer,
  } as unknown as DragEvent;
  assert.equal(payloadAccepts(event), false);
});

test("payloadAccepts: missing dataTransfer + no session returns false", () => {
  __resetDragSessionForTests();
  const event = { dataTransfer: null } as unknown as DragEvent;
  assert.equal(payloadAccepts(event), false);
});

// ---------------------------------------------------------------------------
// Integration: the full drag chain on a real `<div>`.
// ---------------------------------------------------------------------------

test("CardDropText turns to the hover tone when a card payload enters", () => {
  __resetDragSessionForTests();
  const { restore, harness } = setupHarness();
  try {
    const transfer = buildDualTransfer(11);
    dispatchDrag(harness.root, "dragenter", transfer);
    assert.equal(
      harness.root.getAttribute("data-drop-state"),
      "hover",
    );
    assert.ok(harness.root.classList.contains("hover"));
    assert.ok(!harness.root.classList.contains("dropped"));
    assert.deepEqual(harness.states, ["idle", "hover"]);
  } finally {
    restore();
  }
});

test("CardDropText turns to the dropped tone when a card is dropped and ends the session", () => {
  __resetDragSessionForTests();
  const { restore, harness } = setupHarness();
  try {
    const transfer = buildDualTransfer(11);
    dispatchDrag(harness.root, "dragenter", transfer);
    dispatchDrag(harness.root, "dragover", transfer);
    const dropPrevented = dispatchDrag(harness.root, "drop", transfer);
    assert.equal(dropPrevented, false,
      "the drop handler MUST call preventDefault so the browser fires drop");
    assert.equal(
      harness.root.getAttribute("data-drop-state"),
      "dropped",
    );
    assert.ok(harness.root.classList.contains("dropped"));
    assert.equal(hasActiveDragSession(), false,
      "the drop handler must close the in-memory session");
  } finally {
    restore();
  }
});

test("CardDropText stays inert on a foreign drag without a session", () => {
  __resetDragSessionForTests();
  const { restore, harness } = setupHarness();
  try {
    const transfer = new DomDataTransfer({
      types: ["Files"],
      data: { Files: "/etc/hosts" },
    });
    dispatchDrag(harness.root, "dragenter", transfer);
    assert.equal(
      harness.root.getAttribute("data-drop-state"),
      "idle",
    );
    dispatchDrag(harness.root, "drop", transfer);
    assert.equal(
      harness.root.getAttribute("data-drop-state"),
      "idle",
    );
  } finally {
    restore();
  }
});

test("CardDropText activates on the WebKit/Tauri session fallback (empty types)", () => {
  __resetDragSessionForTests();
  beginDragSession(99);
  const { restore, harness } = setupHarness();
  try {
    const transfer = new DomDataTransfer({ types: [], data: {} });
    dispatchDrag(harness.root, "dragenter", transfer);
    assert.equal(
      harness.root.getAttribute("data-drop-state"),
      "hover",
    );
    dispatchDrag(harness.root, "drop", transfer);
    assert.equal(
      harness.root.getAttribute("data-drop-state"),
      "dropped",
    );
    assert.equal(hasActiveDragSession(), false);
  } finally {
    restore();
  }
});

test("CardDropText keeps the highlight when the pointer moves between child nodes", () => {
  __resetDragSessionForTests();
  const { restore, harness } = setupHarness();
  try {
    const child = harness.document.createElement("span");
    child.setAttribute("class", "card-drop-text__inner");
    harness.root.appendChild(child);
    const transfer = buildDualTransfer(7);
    dispatchDrag(harness.root, "dragenter", transfer);
    dispatchDrag(harness.root, "dragover", transfer);
    assert.equal(
      harness.root.getAttribute("data-drop-state"),
      "hover",
    );
    // relatedTarget is the inner span which IS contained inside
    // the root; the highlight must NOT clear.
    dispatchDrag(harness.root, "dragleave", transfer, {
      relatedTarget: child,
      currentTarget: harness.root,
    });
    assert.equal(
      harness.root.getAttribute("data-drop-state"),
      "hover",
    );
  } finally {
    restore();
  }
});

test("CardDropText clears the highlight when the pointer leaves the indicator entirely", () => {
  __resetDragSessionForTests();
  const { restore, harness } = setupHarness();
  try {
    const transfer = buildDualTransfer(7);
    dispatchDrag(harness.root, "dragenter", transfer);
    dispatchDrag(harness.root, "dragover", transfer);
    assert.equal(
      harness.root.getAttribute("data-drop-state"),
      "hover",
    );
    dispatchDrag(harness.root, "dragleave", transfer, {
      relatedTarget: harness.document.body,
      currentTarget: harness.root,
    });
    assert.equal(
      harness.root.getAttribute("data-drop-state"),
      "idle",
    );
  } finally {
    restore();
  }
});

test("CardDropText closes the session only after a successful drop", () => {
  __resetDragSessionForTests();
  beginDragSession(15);
  const { restore, harness } = setupHarness();
  try {
    const transfer = buildDualTransfer(15);
    // dragenter must NOT close the session — only drop closes it.
    dispatchDrag(harness.root, "dragenter", transfer);
    assert.equal(hasActiveDragSession(), true,
      "dragenter must keep the session active");
    dispatchDrag(harness.root, "drop", transfer);
    assert.equal(hasActiveDragSession(), false,
      "drop must close the session");
  } finally {
    restore();
  }
});