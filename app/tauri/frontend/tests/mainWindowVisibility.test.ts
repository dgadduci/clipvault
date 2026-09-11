import test from "node:test";
import assert from "node:assert/strict";
import {
  MAIN_WINDOW_LABEL,
  presentMountedMainWindow,
  type MainWindowSurface,
} from "../src/lib/mainWindowVisibility.ts";

test("mounted main webview requests visibility exactly once", async () => {
  let calls = 0;
  const surface: MainWindowSurface = {
    label: MAIN_WINDOW_LABEL,
    async show() {
      calls += 1;
    },
  };

  assert.equal(await presentMountedMainWindow(surface), true);
  assert.equal(calls, 1);
});

test("mounted quick paste surface cannot present the desktop window", async () => {
  let calls = 0;
  const surface: MainWindowSurface = {
    label: "quick-paste",
    async show() {
      calls += 1;
    },
  };

  assert.equal(await presentMountedMainWindow(surface), false);
  assert.equal(calls, 0);
});
