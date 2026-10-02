import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const ROOT = process.cwd();
function source(...parts: string[]): string {
  return readFileSync(path.join(ROOT, ...parts), "utf8");
}

test("manual text modal preserves a multiline draft through the typed bridge", () => {
  const modal = source("src/CreateTextEntryModal.svelte");
  const bridge = source("src/lib/tauri.ts");
  const app = source("src/App.svelte");
  assert.match(modal, /<textarea[\s\S]*data-testid="create-text-entry-textarea"/);
  assert.match(modal, /createManualTextCommand\(\{ collectionId, content: draft \}\)/);
  assert.doesNotMatch(modal, /save_text|on:keydown=\{onKeydown\}/);
  assert.match(bridge, /invoke<CreateManualTextResponse>\("clipvault_create_manual_text"/);
  assert.match(app, /!activeCollection\.is_peer_bound/);
  assert.match(app, /data-testid="create-manual-text-entry"|canCreateManualText/);
});

test("the shared note modal supports entry and collection notes with empty removal", () => {
  const modal = source("src/TextNoteModal.svelte");
  const shell = source("src/Modal.svelte");
  const bridge = source("src/lib/tauri.ts");
  assert.match(modal, /targetKind: "entry" \| "collection"/);
  assert.match(modal, /<textarea[\s\S]*data-testid="text-note-textarea"/);
  assert.match(modal, /setEntryNoteCommand\(\{ entryId: targetId, body: draft \}\)/);
  assert.match(modal, /setCollectionNoteCommand\(\{ collectionId: targetId, body: draft \}\)/);
  assert.match(modal, /await setEntryNoteCommand[\s\S]*await setCollectionNoteCommand/);
  assert.match(modal, /entryNoteCommand\(\{ entryId: targetId \}\)/);
  assert.match(modal, /collectionNoteCommand\(\{ collectionId: targetId \}\)/);
  assert.match(modal, /busy=\{busy\}/);
  assert.match(modal, /\{returnFocusTo\}/);
  assert.match(modal, /role="alert" data-testid="text-note-error"/);
  assert.match(modal, /function cancel\(\): void/);
  assert.match(shell, /document\.addEventListener\("keydown", onKeydown, true\)/);
  assert.match(shell, /on:click=\{onBackdropClick\}/);
  assert.match(shell, /returnFocusTo\.focus\(\)/);
  assert.match(bridge, /clipvault_entry_note_ids/);
});

test("canceling manual text creation closes without calling the persistence bridge", () => {
  const modal = source("src/CreateTextEntryModal.svelte");
  const cancel = modal.slice(modal.indexOf("function cancel()"), modal.indexOf("</script>"));
  assert.match(cancel, /if \(!saving\) dispatch\("close"\)/);
  assert.doesNotMatch(cancel, /createManualTextCommand/);
  assert.match(modal, /<textarea[\s\S]*rows="10"/);
});

test("note presence actions keep the card and delegated collection drop contracts", () => {
  const card = source("src/HistoryCard.svelte");
  const rail = source("src/HistoryCardRail.svelte");
  const sidebar = source("src/OrganizationSidebar.svelte");
  const dnd = source("src/lib/pointerDragAndDrop.ts");
  assert.match(card, /data-testid="history-card-note"/);
  assert.match(card, /data-testid="history-card-note-action"/);
  assert.match(card, /data-testid="history-card"/);
  assert.match(card, /data-entry-id=\{entry\.id\}/);
  assert.match(card, /draggable="false"/);
  assert.match(rail, /hasNote=\{entryNoteIds\.has\(entry\.id\)\}/);
  assert.match(sidebar, /data-testid="sidebar-collection-note"/);
  assert.match(sidebar, /data-drop-target=\{isDropTarget\(collection\)/);
  assert.match(sidebar, /createCollectionDropZoneHandlers/);
  assert.match(dnd, /setPointerCapture/);
});

test("note refresh events carry no note body", () => {
  const commands = source("../src-tauri/src/commands.rs");
  const entrySave = commands.slice(
    commands.indexOf("pub fn clipvault_set_entry_note"),
    commands.indexOf("pub fn clipvault_collection_note"),
  );
  const collectionSave = commands.slice(
    commands.indexOf("pub fn clipvault_set_collection_note"),
    commands.indexOf("pub fn clipvault_source_app_icon"),
  );
  assert.match(entrySave, /emit_history_updated\(&handle\)/);
  assert.match(collectionSave, /emit_organization_updated\(&handle\)/);
  assert.doesNotMatch(entrySave, /handle\.emit\([^;]*body/s);
  assert.doesNotMatch(collectionSave, /handle\.emit\([^;]*body/s);
});
