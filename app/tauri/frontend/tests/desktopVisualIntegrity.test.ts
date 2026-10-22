import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const ROOT = process.cwd();

function source(file: string): string {
  return readFileSync(path.join(ROOT, "src", file), "utf8");
}

test("desktop capture cards retain a horizontal, non-wrapping rail", () => {
  const rail = source("HistoryCardRail.svelte");
  const app = source("App.svelte");

  assert.match(rail, /\.rail\s*\{[^}]*flex-flow:\s*row nowrap/s);
  assert.match(rail, /\.rail\s*\{[^}]*overflow-x:\s*auto/s);
  assert.match(rail, /\.rail\s*\{[^}]*overflow-y:\s*hidden/s);
  assert.match(app, /:global\(\.collection-content-rail \.rail\)\s*\{[^}]*flex-flow:\s*row nowrap/s);
});

test("collection badge numbers stay white and desktop typography keeps its baseline tokens", () => {
  const sidebar = source("OrganizationSidebar.svelte");
  const peers = source("LinkedPeers.svelte");
  const sourceFilter = source("SourceAppFilter.svelte");
  const collectionFilter = source("CollectionFilter.svelte");

  assert.match(sidebar, /\.badge\.capture-count\s*\{[^}]*color:\s*#fff/s);
  assert.match(sidebar, /\.collection-button\s*\{[^}]*font-size:\s*var\(--cv-body,\s*0\.9rem\)/s);
  assert.match(sidebar, /\.collection-action-menu-item\s*\{[^}]*font-size:\s*var\(--cv-control,\s*0\.85rem\)/s);
  assert.match(peers, /\.linked-peers-button\s*\{[^}]*font-size:\s*var\(--cv-body,\s*0\.9rem\)/s);
  assert.match(sourceFilter, /\.trigger\s*\{[^}]*font-size:\s*var\(--cv-control,\s*0\.85rem\)/s);
  assert.match(sourceFilter, /\.listbox :global\(li\)\s*\{[^}]*font-size:\s*var\(--cv-control,\s*0\.85rem\)/s);
  assert.match(collectionFilter, /select\s*\{[^}]*font-size:\s*var\(--cv-control,\s*0\.85rem\)/s);
});

test("Desktop and QuickPaste keep the webview zoom at baseline and block browser zoom gestures", () => {
  const app = source("App.svelte");
  const quickPaste = source("QuickPaste.svelte");

  for (const surface of [app, quickPaste]) {
    assert.match(surface, /getCurrentWebview\(\)\.setZoom\(1\)/);
    assert.match(surface, /blockWebviewPageZoom\(document\)/);
    assert.match(surface, /detachWebviewPageZoomGuard\(\)/);
  }
});
