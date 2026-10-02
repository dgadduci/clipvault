import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  applyKeyboardShortcutsSnapshot,
  ariaShortcut,
  currentKeyboardShortcut,
  matchesConfiguredShortcut,
  shortcutLabel,
  shortcutStatusTranslationKey,
} from "../src/lib/keyboardShortcuts.ts";
import { matchesSearchShortcut, searchShortcutLabel } from "../src/lib/searchShortcut.ts";

test("configured bindings update matching, visible labels, and accessibility values immediately", () => {
  const focusSearch = {
    id: "focus_main_search",
    key: "f",
    cmd_or_ctrl: true,
    shift: true,
    alt: false,
    meta: false,
  };
  applyKeyboardShortcutsSnapshot({
    bindings: [focusSearch],
    status: { focus_main_search: "ready" },
  });

  assert.equal(currentKeyboardShortcut("focus_main_search")?.key, "f");

  assert.equal(searchShortcutLabel("other"), "Ctrl+Shift+F");
  assert.equal(searchShortcutLabel("macos"), "⌘⇧F");
  assert.equal(
    matchesSearchShortcut(
      { key: "F", ctrlKey: true, metaKey: false, shiftKey: true, altKey: false },
      "other",
    ),
    true,
  );
  assert.equal(
    matchesConfiguredShortcut(
      { key: "f", ctrlKey: true, metaKey: false, shiftKey: false, altKey: false },
      "focus_main_search",
      false,
    ),
    false,
  );
  assert.equal(ariaShortcut(focusSearch, false), "Control+Shift+F");
  assert.equal(ariaShortcut(focusSearch, true), "Meta+Shift+F");
  assert.equal(shortcutLabel(focusSearch, false), "Ctrl+Shift+F");
  assert.equal(shortcutStatusTranslationKey("registered"), "keyboard_shortcuts.status.registered");

  applyKeyboardShortcutsSnapshot({ bindings: [], status: {} });
});

test("shortcut modal binds each displayed key directly to the reactive catalog", () => {
  const source = readFileSync(
    resolve(process.cwd(), "src/KeyboardShortcutsModal.svelte"),
    "utf8",
  );
  assert.match(source, /\{@const binding = \$keyboardShortcuts\[row\.id\]\}/);
  assert.match(source, /\{binding \? shortcutLabel\(binding, macos\) : "—"\}/);
  assert.doesNotMatch(source, /id: "save_text"/);
  assert.match(source, /id: "open_entry_note"/);
  assert.match(source, /id: "open_history"/);
  assert.match(source, /id: "create_text_capture"/);
});

test("main-window shortcuts route to the selected note, History, and text dialog", () => {
  const app = readFileSync(resolve(process.cwd(), "src/App.svelte"), "utf8");
  const rail = readFileSync(resolve(process.cwd(), "src/HistoryCardRail.svelte"), "utf8");
  const card = readFileSync(resolve(process.cwd(), "src/HistoryCard.svelte"), "utf8");
  const sidebar = readFileSync(resolve(process.cwd(), "src/OrganizationSidebar.svelte"), "utf8");
  const toolbar = readFileSync(resolve(process.cwd(), "src/DesktopToolbar.svelte"), "utf8");
  assert.match(app, /matchesConfiguredShortcut\(event, "open_entry_note"/);
  assert.match(app, /railSelectedEntryId === null/);
  assert.match(app, /clipvault:entry-note-shortcut/);
  assert.match(rail, /handleEntryNoteShortcutRequest/);
  assert.match(card, /card-entry-note-shortcut/);
  assert.match(card, /aria-keyshortcuts=\{entryNoteShortcutKeyAttributeText\}/);
  assert.match(card, /data-testid="history-card-note-shortcut"/);
  assert.match(app, /matchesConfiguredShortcut\(event, "open_history"/);
  assert.match(app, /selectCollectionById\(historyCollectionId\)/);
  assert.doesNotMatch(sidebar, /sidebar-history-shortcut|historyShortcut\s*=/);
  assert.match(sidebar, /aria-keyshortcuts=\{isHistory\(collection\) \? historyShortcutAccessible \|\| undefined : undefined\}/);
  assert.match(app, /historyShortcutAccessible=\{historyShortcutAccessibleText\}/);
  assert.match(app, /matchesConfiguredShortcut\(event, "create_text_capture"/);
  assert.match(app, /openCreateTextEntryFromShortcut\(\)/);
  assert.match(app, /onCreateManualText=\{openCreateTextEntry\}/);
  assert.match(toolbar, /data-testid="create-text-capture-shortcut"/);
  assert.match(toolbar, /aria-keyshortcuts=\{createTextShortcutAccessible \|\| undefined\}/);
});
