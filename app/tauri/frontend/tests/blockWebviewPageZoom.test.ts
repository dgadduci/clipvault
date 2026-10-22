import { test } from "node:test";
import assert from "node:assert/strict";
import { blockWebviewPageZoom } from "../src/lib/blockWebviewPageZoom.ts";

test("browser-level pinch and modified-wheel zoom are canceled and listeners are removed", () => {
  const listeners = new Map<string, EventListener>();
  const removed: string[] = [];
  const fakeDocument = {
    addEventListener(type: string, listener: EventListener) {
      listeners.set(type, listener);
    },
    removeEventListener(type: string) {
      removed.push(type);
    },
  } as unknown as Document;

  const release = blockWebviewPageZoom(fakeDocument);
  let ctrlWheelPrevented = false;
  let ordinaryWheelPrevented = false;
  let pinchPrevented = false;

  listeners.get("wheel")?.({
    ctrlKey: true,
    metaKey: false,
    preventDefault: () => { ctrlWheelPrevented = true; },
  } as unknown as WheelEvent);
  listeners.get("wheel")?.({
    ctrlKey: false,
    metaKey: false,
    preventDefault: () => { ordinaryWheelPrevented = true; },
  } as unknown as WheelEvent);
  listeners.get("gesturestart")?.({
    preventDefault: () => { pinchPrevented = true; },
  } as unknown as Event);

  assert.equal(ctrlWheelPrevented, true);
  assert.equal(ordinaryWheelPrevented, false);
  assert.equal(pinchPrevented, true);

  release();
  assert.deepEqual(removed, ["wheel", "gesturestart", "gesturechange", "gestureend"]);
});
