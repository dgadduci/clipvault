import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  dispatchTrayMenuAction,
  TRAY_MENU_ACTION_EVENT,
} from "../src/lib/trayMenuActions.ts";

test("native tray action payloads route to the existing desktop flows", () => {
  const calls: string[] = [];
  const handlers = {
    clearHistory: () => calls.push("clear_history"),
    openSettings: () => calls.push("open_settings"),
    openAbout: () => calls.push("open_about"),
  };

  assert.equal(dispatchTrayMenuAction("clear_history", handlers), true);
  assert.equal(dispatchTrayMenuAction("open_settings", handlers), true);
  assert.equal(dispatchTrayMenuAction("open_about", handlers), true);
  assert.equal(dispatchTrayMenuAction("open_favorites", handlers), false);
  assert.equal(dispatchTrayMenuAction({ action: "open_about" }, handlers), false);
  assert.deepEqual(calls, ["clear_history", "open_settings", "open_about"]);
  assert.equal(TRAY_MENU_ACTION_EVENT, "clipvault://tray-menu-action");
});

test("App listens for native actions on main and removes the listener on teardown", () => {
  const app = readFileSync(resolve(process.cwd(), "src/App.svelte"), "utf8");

  assert.match(app, /listen<unknown>\(\s*TRAY_MENU_ACTION_EVENT/);
  assert.match(app, /\{\s*target:\s*"main"\s*\}/);
  assert.match(app, /dispatchTrayMenuAction\(action,\s*\{/);
  assert.match(app, /clearHistory:\s*\(\)\s*=>\s*\{[\s\S]*?requestClearHistory\(\)/);
  assert.match(app, /openSettings:\s*\(\)\s*=>\s*openModalWith\("general_settings", null\)/);
  assert.match(app, /openAbout:\s*\(\)\s*=>\s*openModalWith\("about", null\)/);
  assert.match(app, /if \(unlistenTrayMenuAction\)\s*\{\s*unlistenTrayMenuAction\(\)/);
});
