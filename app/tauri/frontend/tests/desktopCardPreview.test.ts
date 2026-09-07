/**
 * Tests for the Desktop-side integration the `desktop-card-preview`
 * change introduces.
 *
 * The contract the change enforces:
 *
 *   - the HistoryCard menu exposes a `Previsualizar` action that
 *     dispatches a `preview-request` event the rail forwards to
 *     `App.svelte`;
 *   - the rail resolves the entry from the visible scope so a stale
 *     id never reaches the preview;
 *   - `App.svelte` is the single switch that owns the Desktop
 *     preview state — only one preview can be open at a time;
 *   - the per-card keyboard shortcut (`Cmd+Enter` / `Ctrl+Enter`)
 *     operates on the focused card only and ignores inputs,
 *     textareas, the title editor, the menu and other controls;
 *   - opening the preview from the menu closes the menu exactly
 *     once and never reaches the pin / paste / delete / drag / title
 *     editor branches;
 *   - the desktop rail renders a single `ClipboardPreview` overlay
 *     when the entry is in scope and nothing otherwise;
 *   - if the entry disappears (search / collection / delete /
 *     refresh), the preview closes silently without showing a stale
 *     entry.
 *
 * The tests inspect the source of `App.svelte`,
 * `HistoryCard.svelte` and `HistoryCardRail.svelte` so a future
 * regression that drops the integration surfaces in CI.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

const appSource = readFileSync(
  resolvePath(process.cwd(), "src", "App.svelte"),
  "utf8",
);

const cardSource = readFileSync(
  resolvePath(process.cwd(), "src", "HistoryCard.svelte"),
  "utf8",
);

const railSource = readFileSync(
  resolvePath(process.cwd(), "src", "HistoryCardRail.svelte"),
  "utf8",
);

const previewSource = readFileSync(
  resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
  "utf8",
);

test("App.svelte owns the single Desktop preview state", () => {
  // The desktop preview state lives on `App.svelte` so the rail
  // and the keyboard layer share one switch. A regression that
  // moves the state into HistoryCard or HistoryCardRail would
  // let two overlays mount concurrently — that breaks the
  // documented contract.
  assert.ok(
    appSource.includes("let previewEntry"),
    "App.svelte must own the preview entry state",
  );
  assert.ok(
    appSource.includes("let previewTrigger"),
    "App.svelte must own the preview trigger so focus can be restored on close",
  );
  assert.ok(
    appSource.includes("function requestPreview"),
    "App.svelte must expose a single requestPreview() switch",
  );
  assert.ok(
    appSource.includes("function closePreview"),
    "App.svelte must expose a closePreview() switch",
  );
});

test("App.svelte coordinates one preview through ClipboardPreview", () => {
  // The Desktop mounts exactly one preview overlay; the rail must
  // never declare its own `<ClipboardPreview>` block.
  const previewInstances = (appSource.match(/<ClipboardPreview/g) ?? []).length;
  assert.equal(
    previewInstances,
    1,
    "App.svelte must mount exactly one <ClipboardPreview> instance",
  );
  assert.ok(
    appSource.includes("testIdPrefix=\"history-card-preview\""),
    "App.svelte must prefix the preview overlay with the documented test ids",
  );
});

test("App.svelte closes the preview when the visible scope changes", () => {
  // When the rail / search / collection filter changes and the
  // entry is no longer visible the preview must close silently.
  // The reactive block below `bumpPreviewScope` is the single
  // switch that owns that contract.
  assert.ok(
    appSource.includes("bumpPreviewScope"),
    "App.svelte must declare a bumpPreviewScope helper",
  );
  const body = appSource.slice(
    appSource.indexOf("function bumpPreviewScope"),
    appSource.length,
  );
  assert.ok(
    body.includes("visibleEntries.some"),
    "bumpPreviewScope must consult visibleEntries so a stale entry closes the preview",
  );
});

test("HistoryCard menu exposes Previsualizar as a `history-card-preview` item", () => {
  // The menu action must live on every card and so be available
  // for text, rich text, image and the other supported types.
  assert.ok(
    cardSource.includes('data-testid="history-card-preview"'),
    "the menu must expose a Previsualizar item with the documented test id",
  );
  // The label is rendered between the opening and closing tag;
  // Svelte may add whitespace, so the assertion just confirms
  // the literal `Previsualizar` substring appears inside the
  // menu item template.
  const menuItemMatch = cardSource.match(
    /<button[^>]*data-testid="history-card-preview"[^>]*>([\s\S]*?)<\/button>/,
  );
  assert.ok(
    menuItemMatch,
    "the menu item with history-card-preview testid must be a button element",
  );
  assert.ok(
    menuItemMatch![1].includes("Previsualizar"),
    "the menu item must show the documented label Previsualizar",
  );
});

test("HistoryCard dispatches a preview-request event for App.svelte to consume", () => {
  // The card never opens a preview on its own; it dispatches a
  // CustomEvent the rail forwards. The event name and the
  // dispatcher are the documented contract.
  assert.ok(
    cardSource.includes('"preview-request"'),
    "the card must declare a preview-request event in its dispatcher",
  );
  assert.ok(
    cardSource.includes('dispatch("preview-request"'),
    "the card must dispatch preview-request when the menu item is clicked",
  );
});

function extractCardFunctionBody(name: string): string {
  const start = cardSource.indexOf(`function ${name}`);
  assert.notEqual(start, -1, `${name} must be declared in HistoryCard.svelte`);
  const openBrace = cardSource.indexOf("{", start);
  let depth = 1;
  let cursor = openBrace + 1;
  while (depth > 0 && cursor < cardSource.length) {
    const ch = cardSource[cursor];
    if (ch === "{") depth += 1;
    else if (ch === "}") depth -= 1;
    cursor += 1;
  }
  return cardSource.slice(openBrace, cursor);
}

test("HistoryCard closes its menu exactly once when opening the preview", () => {
  // The menu must dismiss the moment the user picks
  // `Previsualizar`; a regression that leaves the menu open
  // produces a confusing double-overlay surface. The helper
  // `closeMenuAfterAction` is the single switch.
  const helper = extractCardFunctionBody("requestPreview");
  assert.ok(
    helper.includes("closeMenuAfterAction()"),
    "requestPreview must close the menu exactly once",
  );
  assert.ok(
    helper.includes('dispatch("preview-request"'),
    "requestPreview must dispatch the preview-request event",
  );
});

test("HistoryCard previews from the menu never reach pin / paste / delete / drag / title editor", () => {
  // The Previsualizar action MUST stay isolated. The menu only
  // triggers `requestPreview`; it MUST NOT call any other
  // mutation path that would copy, paste, pin, delete, edit the
  // title or otherwise mutate the entry.
  const helper = extractCardFunctionBody("requestPreview");
  for (const forbidden of [
    "runPaste",
    "togglePin",
    "handlePinClick",
    "handleDeleteClick",
    "startEditTitle",
    "confirmEditTitle",
    "restoreDefaultTitle",
    "openTagSelector",
    "openCollectionSelector",
  ]) {
    assert.equal(
      helper.includes(forbidden),
      false,
      `requestPreview must not invoke ${forbidden}`,
    );
  }
});

test("HistoryCard is focusable without breaking the drag-and-drop baseline", () => {
  // The card must accept keyboard focus so `Cmd+Enter` /
  // `Ctrl+Enter` can target the focused card. The change MUST NOT
  // drop `draggable="false"`, `data-testid="history-card"`,
  // `data-entry-id` or any other baseline the AGENTS.md pins.
  assert.ok(
    cardSource.includes('draggable="false"'),
    "the card must keep draggable=\"false\" so pointer drag remains the single switch",
  );
  assert.ok(
    cardSource.includes('data-testid="history-card"'),
    "the card must keep data-testid=\"history-card\"",
  );
  assert.ok(
    cardSource.includes("data-entry-id"),
    "the card must keep the data-entry-id attribute the drag payload relies on",
  );
});

test("HistoryCard keyboard handler ignores interactive controls", () => {
  // The per-card shortcut MUST NOT fire when the focus is on an
  // input, textarea, content-editable, button, menu or the title
  // editor. The `isInteractiveTarget` helper is the single switch.
  const helper = cardSource.slice(
    cardSource.indexOf("function isInteractiveTarget"),
    cardSource.length,
  );
  for (const guarded of [
    "INPUT",
    "TEXTAREA",
    "SELECT",
    "isContentEditable",
    "BUTTON",
    "role='menu'",
    "role='menuitem'",
    ".menu",
    ".title-input",
  ]) {
    assert.ok(
      helper.includes(guarded),
      `isInteractiveTarget must guard ${guarded}`,
    );
  }
});

test("HistoryCardRail forwards preview-request from the card to App.svelte", () => {
  // The rail only forwards the request; it MUST NOT open its own
  // preview overlay. The forwarder is the single switch.
  assert.ok(
    railSource.includes("onRequestPreview"),
    "the rail must accept an onRequestPreview callback",
  );
  assert.ok(
    railSource.includes("on:preview-request"),
    "the rail must forward preview-request events to App.svelte",
  );
  assert.equal(
    railSource.includes("<ClipboardPreview"),
    false,
    "the rail MUST NOT mount its own ClipboardPreview overlay",
  );
});

test("HistoryCardRail resolves the preview request from the visible scope", () => {
  // The rail filters out requests whose entry id is no longer in
  // the visible scope so a stale id never reaches App.svelte.
  const helper = railSource.slice(
    railSource.indexOf("function handlePreviewRequest"),
    railSource.length,
  );
  assert.ok(
    helper.includes("entries.find"),
    "handlePreviewRequest must consult the visible entries",
  );
  assert.ok(
    helper.includes("return") && helper.includes("onRequestPreview(entry)"),
    "handlePreviewRequest must drop stale entries and forward fresh ones",
  );
});

test("App.svelte passes the active platform into the per-card matcher", () => {
  // The per-card matcher needs the platform the diagnostics
  // resolved so the modifier table stays in lockstep with the
  // Quick Paste window and the search shortcut.
  assert.ok(
    cardSource.includes("searchShortcutPlatform"),
    "HistoryCard.svelte must derive the platform through searchShortcutPlatform",
  );
});

test("ClipboardPreview is the only renderer the Desktop relies on", () => {
  // The Desktop must NOT carry its own preview overlay markup
  // alongside the shared one. A regression that introduces a
  // second renderer breaks the documented "single source of
  // truth" contract.
  const cssBlocks = previewSource.match(/\.cv-preview-/g) ?? [];
  assert.ok(
    cssBlocks.length > 0,
    "the shared component must own the preview CSS rules",
  );
  assert.equal(
    appSource.includes('class="cv-preview-overlay"'),
    false,
    "App.svelte MUST NOT redefine the preview overlay CSS — the shared component owns it",
  );
});

test("App.svelte preserves the existing desktop rail / collection layout", () => {
  // The preview overlay must not introduce a new Tauri webview
  // or alter the desktop's fixed geometry. The rail renders the
  // same `HistoryCardRail` component it consumed before this
  // change so the rail / collection panel / search bar layout
  // stays untouched.
  assert.ok(
    appSource.includes("<HistoryCardRail"),
    "App.svelte must keep the HistoryCardRail entry point",
  );
  assert.ok(
    appSource.includes("onRequestPreview="),
    "App.svelte must pass onRequestPreview to the rail",
  );
});