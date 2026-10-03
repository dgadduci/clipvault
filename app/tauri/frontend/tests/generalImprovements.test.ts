import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const frontendRoot = process.cwd();

function source(...segments: string[]): string {
  return readFileSync(path.join(frontendRoot, ...segments), "utf8");
}

test("remote preview rail shows a centered, non-blocking loading status and keeps stale-load guards", () => {
  const rail = source("src/RemoteHistoryRail.svelte");
  assert.match(rail, /class="remote-history-rail-loading-overlay"/);
  assert.match(rail, /role="status"/);
  assert.match(rail, /remote\.history\.loading_previews/);
  assert.match(rail, /pointer-events:\s*none/);
  assert.match(rail, /generation !== loadGeneration/);
  assert.match(
    rail,
    /Promise\.all\([\s\S]*publishFirstPageRows\(textPromise\)[\s\S]*publishFirstPageRows\(imagePromise\)/,
  );
  assert.match(rail, /if \(cached\.loading\)[\s\S]*?loadInitialPage/);
});

test("remote import feedback names the local collection, truncates visually, and expires only success states", () => {
  const card = source("src/RemotePreviewCard.svelte");
  const locales = ["en", "es", "pt", "de", "fr"].map((locale) =>
    JSON.parse(source("src", "locales", `${locale}.json`)),
  );
  assert.match(card, /remote\.import\.duplicate_collection/);
  assert.match(card, /outcome\.collection_name/);
  assert.match(card, /function describeOutcome\(/);
  assert.match(card, /function describeImageOutcome\(/);
  assert.equal((card.match(/outcome\.kind === "imported" && outcome\.deduplicated/g) ?? []).length, 2);
  assert.match(card, /aria-label=\{\$t\(lastResult\.summaryKey, lastResult\.values/);
  assert.match(card, /text-overflow:\s*ellipsis/);
  assert.match(card, /setTimeout\([\s\S]*?3000\)/);
  assert.match(card, /clearResultDismissTimer\(\);[\s\S]*?onDestroy/);
  for (const catalog of locales) {
    assert.match(catalog["remote.import.duplicate_collection"], /\{collection\}/);
  }
});

test("collection clearing uses a metadata preview and keeps the collection action contextual", () => {
  const app = source("src/App.svelte");
  const toolbar = source("src/DesktopToolbar.svelte");
  const bridge = source("src/lib/tauri.ts");
  const commands = readFileSync(
    path.resolve(frontendRoot, "../src-tauri/src/commands.rs"),
    "utf8",
  );
  assert.match(app, /collectionsClearPreviewCommand/);
  assert.match(app, /collectionsClearCommand/);
  assert.match(app, /collection-clear-keep-history/);
  assert.match(app, /collection-clear-delete-history/);
  assert.match(app, /showClearCollection=\{activeCollection\?\.kind === "user"\}/);
  assert.match(toolbar, /data-testid=\{showClearCollection \? "trash-clear-collection"/);
  assert.match(bridge, /clipvault_collections_clear_preview/);
  assert.match(bridge, /clipvault_collections_clear/);
  assert.match(commands, /pub fn clipvault_collections_clear\(/);
  assert.match(commands, /pub fn clipvault_collections_clear_preview\(/);
});

test("keyboard shortcut list action has a main-window binding and localized row", () => {
  const app = source("src/App.svelte");
  const modal = source("src/KeyboardShortcutsModal.svelte");
  const shortcuts = source("src/lib/keyboardShortcuts.ts");
  const core = readFileSync(
    path.resolve(frontendRoot, "../../../crates/clipvault-core/src/keyboard_shortcuts.rs"),
    "utf8",
  );
  assert.match(shortcuts, /"open_keyboard_shortcuts"/);
  assert.match(app, /matchesConfiguredShortcut\(event, "open_keyboard_shortcuts"/);
  assert.match(modal, /id: "open_keyboard_shortcuts"/);
  assert.match(core, /OpenKeyboardShortcuts/);
  assert.match(core, /make\(Id::OpenKeyboardShortcuts, "k", true, false\)/);
  for (const locale of ["en", "es", "pt", "de", "fr"]) {
    const catalog = JSON.parse(source("src", "locales", `${locale}.json`));
    assert.ok(catalog["keyboard_shortcuts.action.open_keyboard_shortcuts"]);
  }
});
