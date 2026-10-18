import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const ROOT = process.cwd();
const sidebar = readFileSync(
  path.join(ROOT, "src/OrganizationSidebar.svelte"),
  "utf8",
).replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");

test("collection rows use their assigned color as a translucent background", () => {
  assert.match(
    sidebar,
    /class="collection-row"[\s\S]*?collectionColorTint\(\s*collection\.color_hex,\s*\)/,
  );
  assert.match(
    sidebar,
    /\.collection-row\s*\{[^}]*background-color:\s*var\(--collection-row-tint/s,
  );
  assert.match(
    sidebar,
    /\.collection-row\.active\s*\{[^}]*inset 0 0 0 1px var\(--cv-accent/s,
  );
});

test("the collection selection row contains only its name and applicable badges", () => {
  const selectionButton = sidebar.match(
    /<button\s+type="button"\s+class="collection-button"[\s\S]*?<\/button>/,
  );
  assert.ok(selectionButton, "collection selection button must exist");
  assert.match(selectionButton[0], /class="collection-name"/);
  assert.match(selectionButton[0], /class="badge system"/);
  assert.match(selectionButton[0], /class="badge remote"/);
  assert.match(selectionButton[0], /class="badge capture-count"/);
  assert.doesNotMatch(selectionButton[0], /<svg|sidebar-collection-color|sidebar-collection-note|sidebar-collection-delete/);
  const rowActions = sidebar.match(
    /class="collection-button"[\s\S]*?<\/button>\s*([\s\S]*?)<button[\s\S]*?class="icon-only actions-menu-trigger"/,
  );
  assert.ok(rowActions, "collection row controls must follow the selection button");
  assert.match(rowActions[1], /class="icon-only note-icon"[\s\S]*?data-testid="sidebar-collection-note"/);
  assert.doesNotMatch(rowActions[1], /sidebar-collection-color|sidebar-collection-delete/);
  assert.match(sidebar, /noteReturnFocusTarget = event\.currentTarget as HTMLButtonElement/);
});

test("the menu contains color and user-only deletion, while notes stay on the row", () => {
  const menu = sidebar.match(
    /class="collection-actions-menu"[\s\S]*?data-testid="sidebar-collection-actions-menu"[\s\S]*?<\/div>/,
  );
  assert.ok(menu, "collection actions menu must exist");
  assert.match(menu[0], /role="menu"/);
  assert.match(menu[0], /data-testid="sidebar-collection-color"/);
  assert.doesNotMatch(menu[0], /sidebar-collection-note/);
  assert.match(menu[0], /\{#if collection\.kind === "user"\}[\s\S]*?data-testid="sidebar-collection-delete"/);
  assert.match(sidebar, /\.collection-action-menu-item\.danger\.delete-icon\s*\{[^}]*--cv-danger/s);
});

test("collection actions menu supports keyboard navigation and outside dismissal", () => {
  assert.match(sidebar, /function onActionsMenuKeydown\(event: KeyboardEvent\)/);
  assert.match(sidebar, /event\.key === "Escape"[\s\S]*?closeActionsMenu\(true\)/);
  assert.match(sidebar, /event\.key === "Home"[\s\S]*?items\[0\]\.focus\(\)/);
  assert.match(sidebar, /event\.key === "End"[\s\S]*?items\[items\.length - 1\]\.focus\(\)/);
  assert.match(sidebar, /document\.addEventListener\("pointerdown", onWindowPointerDown\)/);
  assert.match(sidebar, /document\.removeEventListener\("pointerdown", onWindowPointerDown\)/);
  assert.match(sidebar, /document\.addEventListener\("focusin", onWindowFocusIn\)/);
  assert.match(sidebar, /document\.removeEventListener\("focusin", onWindowFocusIn\)/);
});
