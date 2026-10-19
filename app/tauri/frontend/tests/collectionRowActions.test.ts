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

test("the collection selection button contains only its name", () => {
  const selectionButton = sidebar.match(
    /<button\s+type="button"\s+class="collection-button"[\s\S]*?<\/button>/,
  );
  assert.ok(selectionButton, "collection selection button must exist");
  assert.match(selectionButton[0], /class="collection-name"/);
  assert.doesNotMatch(
    selectionButton[0],
    /<svg|class="badge|sidebar-collection-(?:color|note|subcollection-visibility|count|remote-origin|delete)/,
  );
  const rowActions = sidebar.match(
    /data-testid="sidebar-collection-button"[\s\S]*?data-testid="sidebar-collection-menu-trigger"/,
  );
  assert.ok(rowActions, "collection row controls must include name, visibility, badges, note, and menu actions");
  const rowActionOrder = [
    'class="collection-name"',
    'data-testid="sidebar-collection-subcollection-visibility"',
    'class="badge system"',
    'class="badge remote"',
    'data-testid="sidebar-collection-count"',
    'data-testid="sidebar-collection-note"',
    'data-testid="sidebar-collection-menu-trigger"',
  ].map((marker) => rowActions[0].indexOf(marker));
  assert.ok(
    rowActionOrder.every((index, position) => index >= 0 && (position === 0 || index > rowActionOrder[position - 1])),
    "row actions must follow name, eye, badges, note, and menu order",
  );
  assert.match(
    sidebar,
    /\{#if collection\.parent_id == null && collection\.kind === "user" && collectionChildren\(collections, collection\.id\)\.length > 0\}[\s\S]*?data-testid="sidebar-collection-subcollection-visibility"/,
  );
  assert.match(rowActions[0], /aria-pressed=\{showSubcollectionCaptures\}/);
  assert.doesNotMatch(rowActions[0], /sidebar-collection-color|sidebar-collection-delete/);
  assert.match(
    sidebar,
    /function onCollectionRowClick\([\s\S]*?target\.closest\("button, \[role='menu'\]"\)[\s\S]*?!target\.closest\("\.badge"\)[\s\S]*?selectCollection\(collection\.id\)/,
  );
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
