/**
 * Tests for the keyboard shortcuts the Quick Paste window installs
 * for the `Cmd/Ctrl+K` (focus search) and `Cmd/Ctrl+Enter` (open
 * preview overlay) affordances.
 *
 * The matchers live inline inside `QuickPaste.svelte` (the search
 * matcher is local to the component) but the preview matcher is a
 * thin wrapper around `matchesPreviewShortcut` from
 * `lib/clipboardPreview.ts` so the Desktop rail and Quick Paste
 * share one platform-aware matcher. The shared helper owns the
 * actual `Alt` / `Shift` / `Enter` / `metaKey` / `ctrlKey` checks
 * the keyboard layer relies on:
 *
 *   - macOS accepts `Cmd+K` / `Cmd+Enter` and rejects `Ctrl+K` /
 *     `Ctrl+Enter`;
 *   - Linux accepts `Ctrl+K` / `Ctrl+Enter` and rejects `Cmd+K` /
 *     `Cmd+Enter`;
 *   - every host rejects `Alt+` and `Shift+` combinations so an
 *     accidental chord does not steal focus;
 *   - the search shortcut is `k` / `K` only — a different letter is
 *     never a trigger;
 *   - the preview shortcut is `Enter` only — `Shift+Enter` keeps the
 *     legacy copy-only flow.
 *
 * The matchers are read straight from the QuickPaste component
 * source so a regression that drops the matcher or replaces it with
 * a global listener surfaces here.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

const quickPasteSource = readFileSync(
  resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
  "utf8",
);

const previewHelperSource = readFileSync(
  resolvePath(process.cwd(), "src", "lib", "clipboardPreview.ts"),
  "utf8",
);

function hasMatcher(name: string): boolean {
  return quickPasteSource.includes(`function ${name}`);
}

function extractPreviewHelperBody(name: string): string {
  const start = previewHelperSource.indexOf(`export function ${name}`);
  assert.notEqual(start, -1, `${name} must be declared in clipboardPreview.ts`);
  const openBrace = previewHelperSource.indexOf("{", start);
  let depth = 1;
  let cursor = openBrace + 1;
  while (depth > 0 && cursor < previewHelperSource.length) {
    const ch = previewHelperSource[cursor];
    if (ch === "{") depth += 1;
    else if (ch === "}") depth -= 1;
    cursor += 1;
  }
  return previewHelperSource.slice(openBrace, cursor);
}

test("Quick Paste exposes inline shortcut matchers for search and preview", () => {
  // The matchers live inline so a refactor that drops them is caught
  // before the keyboard surface stops responding. The preview
  // matcher is a wrapper around the shared helper consumed by
  // Desktop and Quick Paste.
  assert.equal(
    hasMatcher("matchesQuickPasteSearchShortcut"),
    true,
    "Quick Paste must expose matchesQuickPasteSearchShortcut",
  );
  assert.equal(
    hasMatcher("matchesQuickPastePreviewShortcut"),
    true,
    "Quick Paste must expose matchesQuickPastePreviewShortcut",
  );
});

test("matchesQuickPasteSearchShortcut accepts the platform-specific key only", () => {
  const body = extractFunctionBody("matchesQuickPasteSearchShortcut");
  assert.ok(
    body.includes("altKey") && body.includes("shiftKey"),
    "search matcher must reject Alt+ and Shift+ combinations",
  );
  assert.ok(
    body.includes('key !== "k"'),
    "search matcher must pin the trigger to the k / K letter",
  );
  assert.ok(
    body.includes("metaKey") && body.includes("ctrlKey"),
    "search matcher must consult metaKey / ctrlKey per platform",
  );
});

test("matchesQuickPastePreviewShortcut delegates to the shared preview matcher", () => {
  // The inline matcher is now a thin wrapper around
  // `matchesPreviewShortcut` from `lib/clipboardPreview.ts` so the
  // Desktop rail and Quick Paste share one implementation. The
  // wrapper must be present (so the keyboard handler can call it)
  // and the wrapper must call the shared matcher.
  const body = extractFunctionBody("matchesQuickPastePreviewShortcut");
  assert.ok(
    body.includes("matchesPreviewShortcut"),
    "the inline matcher must delegate to matchesPreviewShortcut",
  );
});

test("matchesPreviewShortcut accepts Cmd/Ctrl+Enter only", () => {
  // The platform-aware matcher is shared between Desktop and Quick
  // Paste so the keyboard contract is pinned once in
  // `lib/clipboardPreview.ts`. The matcher must reject Alt+ and
  // Shift+ combinations and consult the documented modifier table.
  const body = extractPreviewHelperBody("matchesPreviewShortcut");
  assert.ok(
    body.includes("altKey") && body.includes("shiftKey"),
    "preview matcher must reject Alt+ and Shift+ combinations",
  );
  assert.ok(
    body.includes('key !== "Enter"'),
    "preview matcher must pin the trigger to Enter",
  );
  assert.ok(
    body.includes("metaKey") && body.includes("ctrlKey"),
    "preview matcher must consult metaKey / ctrlKey per platform",
  );
});

test("the search shortcut drives focusSearchInput with the select-all branch", () => {
  const block = extractWindowKeydownBlock();
  // The search shortcut must select the existing query so the user
  // can overwrite it immediately. A regression that drops the
  // `true` argument falls back to caret-at-end, breaking the
  // documented contract.
  assert.ok(
    block.includes("focusSearchInput(true)"),
    "the search shortcut must select the existing query",
  );
  assert.ok(
    block.includes("matchesQuickPasteSearchShortcut"),
    "the search shortcut must consult the inline matcher",
  );
});

test("the preview shortcut is bound to the openPreviewFor controller", () => {
  const block = extractWindowKeydownBlock();
  assert.ok(
    block.includes("matchesQuickPastePreviewShortcut"),
    "the preview shortcut must consult the inline matcher",
  );
  assert.ok(
    block.includes("openPreviewFor("),
    "the preview shortcut must route through openPreviewFor",
  );
});

test("the Escape handler layers the preview over the Quick Paste window", () => {
  const block = extractWindowKeydownBlock();
  // The preview MUST close first so the user can dismiss it
  // without losing the window; a second Escape falls through to
  // the legacy handler.
  const previewEscapeIndex = block.indexOf("previewEntryId !== null");
  const legacyEscapeIndex = block.indexOf("handleEscape(");
  assert.notEqual(previewEscapeIndex, -1, "preview Escape branch must exist");
  assert.notEqual(legacyEscapeIndex, -1, "legacy Escape handler must exist");
  assert.ok(
    previewEscapeIndex < legacyEscapeIndex,
    "the preview Escape branch must run before the legacy handler",
  );
});

function extractFunctionBody(name: string): string {
  const start = quickPasteSource.indexOf(`function ${name}`);
  assert.notEqual(start, -1, `${name} must be declared in QuickPaste.svelte`);
  // Find the matching closing brace by counting opens and closes.
  const openBrace = quickPasteSource.indexOf("{", start);
  let depth = 1;
  let cursor = openBrace + 1;
  while (depth > 0 && cursor < quickPasteSource.length) {
    const ch = quickPasteSource[cursor];
    if (ch === "{") depth += 1;
    else if (ch === "}") depth -= 1;
    cursor += 1;
  }
  return quickPasteSource.slice(openBrace, cursor);
}

function extractWindowKeydownBlock(): string {
  const start = quickPasteSource.indexOf("function onWindowKeydown");
  assert.notEqual(start, -1, "onWindowKeydown must be declared");
  const openBrace = quickPasteSource.indexOf("{", start);
  let depth = 1;
  let cursor = openBrace + 1;
  while (depth > 0 && cursor < quickPasteSource.length) {
    const ch = quickPasteSource[cursor];
    if (ch === "{") depth += 1;
    else if (ch === "}") depth -= 1;
    cursor += 1;
  }
  return quickPasteSource.slice(openBrace, cursor);
}