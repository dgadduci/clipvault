/**
 * Frontend regression coverage for the `history-unorganized-delete-ux`
 * change.
 *
 * The contract being pinned:
 *
 *   - The global clear-history trash icon is rendered by
 *     `DesktopToolbar` ONLY when the parent passes a `showClearHistory`
 *     prop. When the prop is `false`, the button is absent from the
 *     DOM, the keyboard tab order and the accessibility tree — not
 *     merely hidden through CSS, not merely `disabled` and not
 *     replaced with an inert placeholder.
 *   - The accessible label and the `title` tooltip are pinned to
 *     `Eliminar capturas no organizadas`. The same wording drives
 *     the `aria-label` and the visible title so screen-reader,
 *     keyboard and mouse users see the same intent.
 *   - The `data-testid="trash-clear-history"` selector survives so
 *     the rest of the regression suite can still target it.
 *   - The toolbar never invokes `clearUnorganizedHistoryCommand`
 *     directly: the icon is the entry point, the parent owns the
 *     modal.
 *   - The `activeCollectionIsHistory` helper in `App.svelte` matches
 *     the documented contract (`selectedCollectionId === null` OR a
 *     system-kind collection); a future regression that broke the
 *     default branch would re-introduce the bug this change closes.
 *   - The confirmation dialog reuses the documented wording
 *     (`Eliminar capturas no organizadas`) and explains that only
 *     non-favorite entries without user collections are removed,
 *     while favorites and organised entries are preserved.
 *   - The destructive path never logs clipboard content, snippets,
 *     hashes, app names or file paths through the toolbar, the
 *     confirmation or the events the desktop dispatches.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const FRONTEND_ROOT = process.cwd();

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/\/.*$/gm, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
}

// ---------------------------------------------------------------------------
// Toolbar visibility: the trash button is gated by `showClearHistory`.
// ---------------------------------------------------------------------------

test("DesktopToolbar gates the trash button behind the showClearHistory prop", () => {
  const source = stripComments(loadSource("src/DesktopToolbar.svelte"));
  // The prop MUST exist with a documented default so a regression
  // that drops it from the toolbar would surface as a missing
  // `export let showClearHistory`.
  assert.match(
    source,
    /export let showClearHistory:\s*boolean\s*=\s*true/,
    "showClearHistory must be a public boolean prop defaulting to true",
  );
  // The trash button MUST live inside a `{#if showClearHistory}` block
  // so a parent passing `false` removes it from the DOM entirely.
  const trashOpenIdx = source.indexOf("{#if showClearHistory}");
  assert.notEqual(trashOpenIdx, -1, "the trash button must be wrapped in {#if showClearHistory}");
  // The closing `{/if}` must come AFTER the trash button so the
  // guard actually wraps the button.
  const trashButtonIdx = source.indexOf('data-testid="trash-clear-history"', trashOpenIdx);
  assert.notEqual(trashButtonIdx, -1, "the trash button must live after the guard");
  const trashCloseIdx = source.indexOf("{/if}", trashButtonIdx);
  assert.notEqual(trashCloseIdx, -1, "the guard must close after the trash button");
});

test("DesktopToolbar trash button keeps the documented testid, danger hook and aria-busy hook", () => {
  const source = stripComments(loadSource("src/DesktopToolbar.svelte"));
  const guardOpen = source.indexOf("{#if showClearHistory}");
  assert.notEqual(guardOpen, -1);
  const guardClose = source.indexOf("{/if}", guardOpen);
  assert.notEqual(guardClose, -1);
  const guardBlock = source.slice(guardOpen, guardClose);
  assert.match(
    guardBlock,
    /data-testid="trash-clear-history"/,
    "the trash button must keep its stable test id",
  );
  assert.match(
    guardBlock,
    /data-cv-danger="clear-history"/,
    "the trash button must keep its documented danger hook",
  );
  assert.match(
    guardBlock,
    /aria-busy=\{trashConfirming\}/,
    "the trash button must keep its busy hook",
  );
});

test("DesktopToolbar trash button uses 'Eliminar capturas no organizadas' as both aria-label and title", () => {
  const source = stripComments(loadSource("src/DesktopToolbar.svelte"));
  // The default value of `trashLabel` MUST be the new wording so the
  // button keeps the documented accessible name even when the parent
  // does not forward a custom string.
  assert.match(
    source,
    /export let trashLabel:\s*string\s*=\s*"Eliminar capturas no organizadas"/,
    "trashLabel default must be 'Eliminar capturas no organizadas'",
  );
  // The legacy wording MUST be gone from the default value to avoid
  // a regression that mixed the two wordings.
  assert.equal(
    /export let trashLabel:\s*string\s*=\s*"Limpiar historial no favorito"/.test(source),
    false,
    "the legacy 'Limpiar historial no favorito' default must be gone",
  );
  // Both `aria-label` and `title` MUST keep wiring to `trashLabel` so
  // the visible tooltip and the accessible name stay in lock-step.
  assert.match(source, /aria-label=\{trashLabel\}/);
  assert.match(source, /title=\{trashLabel\}/);
});

test("DesktopToolbar never invokes clearUnorganizedHistoryCommand directly", () => {
  const source = stripComments(loadSource("src/DesktopToolbar.svelte"));
  assert.doesNotMatch(
    source,
    /clearUnorganizedHistoryCommand/,
    "the toolbar must never invoke the destructive command directly",
  );
  // The button keeps forwarding through the documented intent.
  const buttonMatch = source.match(
    /<button[\s\S]*?data-testid="trash-clear-history"[\s\S]*?<\/button>/,
  );
  assert.ok(buttonMatch, "the trash button must exist");
  assert.match(
    buttonMatch[0],
    /on:click=\{onRequestClearHistory\}/,
    "the trash button must keep its parent-owned intent",
  );
});

// ---------------------------------------------------------------------------
// App.svelte: showClearHistory is derived from the documented contract.
// ---------------------------------------------------------------------------

test("App.svelte forwards showClearHistory from activeCollectionIsHistory", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  // The parent must pass the computed flag through the documented
  // prop so the toolbar can react to collection switches.
  assert.match(
    source,
    /showClearHistory=\{activeCollectionIsHistory\}/,
    "App.svelte must forward showClearHistory from activeCollectionIsHistory",
  );
});

test("App.svelte activeCollectionIsHistory matches the documented contract", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  // The helper MUST keep the contract: `selectedCollectionId === null`
  // (default view) OR a system-kind collection. The previous
  // implementation only matched the second branch and silently
  // returned `false` for the default state, hiding the trash button
  // in `Historial`. The regression is pinned here so a future
  // contributor cannot reintroduce the asymmetry.
  assert.match(
    source,
    /selectedCollectionId\s*===\s*null\s*\|\|\s*\(\s*activeCollection\s*!==\s*null\s*&&\s*activeCollection\.kind\s*===\s*"system"\s*\)/,
    "activeCollectionIsHistory must match the documented contract",
  );
  // The previous helper MUST be gone.
  assert.equal(
    /activeCollectionIsHistory\s*=\s*activeCollection\s*!==\s*null\s*&&\s*activeCollection\.kind\s*===\s*"system"\s*;/.test(source),
    false,
    "the legacy single-branch helper must be gone",
  );
});

// ---------------------------------------------------------------------------
// Confirmation: the dialog explains the unorganized-history predicate.
// ---------------------------------------------------------------------------

test("App.svelte confirmation dialog uses 'Eliminar capturas no organizadas'", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  // The dialog title and the destructive action MUST reuse the same
  // wording so the user gets a consistent message regardless of the
  // surface they read.
  const titleMatch = source.match(/<h2 id="confirm-title">([^<]+)<\/h2>/g) ?? [];
  assert.ok(
    titleMatch.some((line) => line.includes("Eliminar capturas no organizadas")),
    "the dialog must use the documented title",
  );
  const confirmMatch =
    source.match(/data-testid="confirm-clear"[\s\S]*?<\/button>/) ?? [];
  assert.ok(confirmMatch[0], "the confirm button must exist");
  assert.match(
    confirmMatch[0],
    /Eliminar capturas no organizadas/,
    "the confirm button must reuse the documented wording",
  );
  // The legacy ambiguous title MUST be gone so a regression that
  // re-introduced "Limpiar historial" would surface here.
  assert.equal(
    /<h2 id="confirm-title">Limpiar historial sin colección<\/h2>/.test(source),
    false,
    "the legacy 'Limpiar historial sin colección' dialog title must be gone",
  );
  assert.equal(
    /Limpiar historial sin colección/.test(source),
    false,
    "no surface may use the legacy ambiguous wording",
  );
});

test("App.svelte confirmation dialog distinguishes unorganized from favorite/organised entries", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  // The dialog must explain the predicate so the user understands
  // which captures will be removed and which will survive.
  const summaryMatch =
    source.match(/data-testid="confirm-clear-summary"[\s\S]*?<\/p>/) ?? [];
  assert.ok(summaryMatch[0], "the confirm-clear summary must exist");
  assert.match(
    summaryMatch[0],
    /no favorita/,
    "the summary must mention non-favorite entries",
  );
  assert.match(
    summaryMatch[0],
    /sin colecciones de usuario/,
    "the summary must mention entries without user collections",
  );
  // The secondary line must explain what survives the deletion.
  assert.match(
    source,
    /Las capturas favoritas y las asociadas a una o más colecciones de usuario se conservan\./,
    "the dialog must state which entries are preserved",
  );
});

// ---------------------------------------------------------------------------
// Privacy: no surface logs clipboard content, snippets, hashes or paths.
// ---------------------------------------------------------------------------

test("DesktopToolbar trash affordance never leaks clipboard content, snippets, hashes or paths", () => {
  const source = stripComments(loadSource("src/DesktopToolbar.svelte"));
  for (const forbidden of [
    "content_hash",
    "asset_ref",
    "mime_type",
    "source_app",
    "snippet",
    "DataTransfer",
    "base64",
  ]) {
    assert.equal(
      source.includes(forbidden),
      false,
      `DesktopToolbar must not reference "${forbidden}"`,
    );
  }
});

test("App.svelte confirmation copy never echoes clipboard content, snippets, hashes or paths", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  // The change must not introduce clipboard, snippet, hash or path
  // tokens into the confirmation copy the user reads.
  const confirmTitleMatch = source.match(
    /<h2 id="confirm-title">[\s\S]*?<\/h2>/g,
  ) ?? [];
  for (const line of confirmTitleMatch) {
    for (const forbidden of [
      "content_hash",
      "asset_ref",
      "snippet",
      "DataTransfer",
      "base64",
      "/Users/",
    ]) {
      assert.equal(
        line.includes(forbidden),
        false,
        `the confirmation title must not reference "${forbidden}"`,
      );
    }
  }
  const summaryMatch = source.match(
    /data-testid="confirm-clear-summary"[\s\S]*?<\/p>/,
  );
  assert.ok(summaryMatch, "the clear summary must exist");
  for (const forbidden of [
    "content_hash",
    "asset_ref",
    "snippet",
    "DataTransfer",
    "base64",
    "/Users/",
  ]) {
    assert.equal(
      summaryMatch[0].includes(forbidden),
      false,
      `the summary must not reference "${forbidden}"`,
    );
  }
});