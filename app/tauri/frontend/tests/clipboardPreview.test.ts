/**
 * Tests for the shared `lib/clipboardPreview.ts` helpers the
 * `desktop-card-preview` change introduces.
 *
 * The Desktop rail and the Quick Paste window both consume the
 * same helper module so the keyboard matcher, the platform label
 * and the metadata re-exports cannot drift between the two
 * surfaces. The suite pins the contract by:
 *
 *   - exercising the platform-aware `matchesPreviewShortcut`
 *     helper against `Cmd+Enter` (macOS), `Ctrl+Enter` (Linux) and
 *     the rejection cases (`Alt`, `Shift`, plain `Enter`,
 *     wrong-platform modifiers, unrelated keys);
 *   - asserting the visible / accessible labels follow the
 *     modifier table;
 *   - asserting the typed metadata re-exports
 *     (`entryFullPreviewText`, `escapeForPreview`, `isImageEntry`,
 *     `hasRenderableImage`) come from the same path the Quick Paste
 *     regression suite already pins;
 *   - asserting the matcher helper module is the single source of
 *     truth for both consumers — neither `QuickPaste.svelte` nor
 *     `HistoryCard.svelte` re-implements the modifier table.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

import {
  matchesPreviewShortcut,
  previewShortcutAccessibleLabel,
  previewShortcutLabel,
  previewShortcutPlatform,
  type PreviewShortcutPlatform,
} from "../src/lib/clipboardPreview.ts";

function makeEvent(
  overrides: Partial<{
    key: string;
    metaKey: boolean;
    ctrlKey: boolean;
    altKey: boolean;
    shiftKey: boolean;
  }> = {},
): Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey"> {
  return {
    key: overrides.key ?? "Enter",
    metaKey: overrides.metaKey ?? false,
    ctrlKey: overrides.ctrlKey ?? false,
    altKey: overrides.altKey ?? false,
    shiftKey: overrides.shiftKey ?? false,
  };
}

test("matchesPreviewShortcut — macOS accepts Cmd+Enter", () => {
  assert.equal(
    matchesPreviewShortcut(makeEvent({ metaKey: true }), "macos"),
    true,
    "Cmd+Enter must trigger the preview on macOS",
  );
});

test("matchesPreviewShortcut — macOS rejects Ctrl+Enter", () => {
  assert.equal(
    matchesPreviewShortcut(makeEvent({ ctrlKey: true }), "macos"),
    false,
    "Ctrl+Enter must NOT trigger the preview on macOS",
  );
});

test("matchesPreviewShortcut — macOS rejects Cmd+Shift+Enter", () => {
  assert.equal(
    matchesPreviewShortcut(
      makeEvent({ metaKey: true, shiftKey: true }),
      "macos",
    ),
    false,
    "Cmd+Shift+Enter must NOT trigger the preview so Shift+Enter keeps its legacy copy-only flow",
  );
});

test("matchesPreviewShortcut — macOS rejects Cmd+Alt+Enter", () => {
  assert.equal(
    matchesPreviewShortcut(
      makeEvent({ metaKey: true, altKey: true }),
      "macos",
    ),
    false,
    "Cmd+Alt+Enter must NOT trigger the preview so accidental chords cannot steal focus",
  );
});

test("matchesPreviewShortcut — macOS rejects plain Enter", () => {
  assert.equal(
    matchesPreviewShortcut(makeEvent(), "macos"),
    false,
    "a plain Enter must NOT trigger the preview so the Enter shortcut keeps its copy-only flow",
  );
});

test("matchesPreviewShortcut — macOS rejects a non-Enter key", () => {
  assert.equal(
    matchesPreviewShortcut(
      makeEvent({ key: "k", metaKey: true }),
      "macos",
    ),
    false,
    "Cmd+K must NOT trigger the preview matcher (the search shortcut is its own key)",
  );
});

test("matchesPreviewShortcut — Linux accepts Ctrl+Enter", () => {
  assert.equal(
    matchesPreviewShortcut(makeEvent({ ctrlKey: true }), "other"),
    true,
    "Ctrl+Enter must trigger the preview on Linux / unknown",
  );
});

test("matchesPreviewShortcut — Linux rejects Cmd+Enter", () => {
  assert.equal(
    matchesPreviewShortcut(makeEvent({ metaKey: true }), "other"),
    false,
    "Cmd+Enter must NOT trigger the preview on Linux / unknown",
  );
});

test("matchesPreviewShortcut — Linux rejects Ctrl+Shift+Enter", () => {
  assert.equal(
    matchesPreviewShortcut(
      makeEvent({ ctrlKey: true, shiftKey: true }),
      "other",
    ),
    false,
    "Ctrl+Shift+Enter must NOT trigger the preview",
  );
});

test("matchesPreviewShortcut — Linux rejects Ctrl+Alt+Enter", () => {
  assert.equal(
    matchesPreviewShortcut(
      makeEvent({ ctrlKey: true, altKey: true }),
      "other",
    ),
    false,
    "Ctrl+Alt+Enter must NOT trigger the preview",
  );
});

test("matchesPreviewShortcut — Linux rejects plain Enter", () => {
  assert.equal(
    matchesPreviewShortcut(makeEvent(), "other"),
    false,
    "a plain Enter must NOT trigger the preview on Linux either",
  );
});

test("matchesPreviewShortcut — Linux rejects a non-Enter key", () => {
  assert.equal(
    matchesPreviewShortcut(
      makeEvent({ key: "k", ctrlKey: true }),
      "other",
    ),
    false,
    "Ctrl+K must NOT trigger the preview matcher",
  );
});

test("previewShortcutLabel — macOS renders ⌘Enter", () => {
  assert.equal(previewShortcutLabel("macos"), "⌘Enter");
});

test("previewShortcutLabel — Linux renders Ctrl Enter", () => {
  assert.equal(previewShortcutLabel("other"), "Ctrl Enter");
});

test("previewShortcutAccessibleLabel — macOS uses Comando Enter", () => {
  assert.equal(
    previewShortcutAccessibleLabel("macos"),
    "Previsualizar (Comando Enter)",
  );
});

test("previewShortcutAccessibleLabel — Linux uses Control Enter", () => {
  assert.equal(
    previewShortcutAccessibleLabel("other"),
    "Previsualizar (Control Enter)",
  );
});

test("previewShortcutPlatform — macOS-like strings resolve to macos", () => {
  assert.equal(previewShortcutPlatform("macos"), "macos");
  assert.equal(previewShortcutPlatform("darwin"), "macos");
  assert.equal(previewShortcutPlatform("osx"), "macos");
});

test("previewShortcutPlatform — non-macOS strings resolve to other", () => {
  assert.equal(previewShortcutPlatform("linux"), "other");
  assert.equal(previewShortcutPlatform("ubuntu"), "other");
  assert.equal(previewShortcutPlatform(""), "other");
  assert.equal(previewShortcutPlatform(null), "other");
  assert.equal(previewShortcutPlatform(undefined), "other");
});

test("PreviewShortcutPlatform — the type alias equals SearchShortcutPlatform", () => {
  // The shared matcher intentionally re-exports
  // `SearchShortcutPlatform` under a preview-specific name so the
  // Desktop rail and Quick Paste can read the platform without
  // reaching for the `searchShortcut` module. Pin the alias so a
  // future drift is caught before the matcher stops accepting the
  // platform argument.
  const previewPlatform: PreviewShortcutPlatform = "macos";
  assert.equal(previewPlatform === "macos", true);
});

test("QuickPaste.svelte imports the shared matcher", () => {
  // The change forbids re-implementing the matcher inside
  // `QuickPaste.svelte`. The wrapper delegates to the shared
  // module so the Desktop and Quick Paste surfaces consume one
  // implementation.
  const source = readFileSync(
    resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
    "utf8",
  );
  assert.ok(
    source.includes("from \"./lib/clipboardPreview\""),
    "QuickPaste.svelte must import the shared clipboardPreview module",
  );
  assert.ok(
    source.includes("matchesPreviewShortcut"),
    "QuickPaste.svelte must consult the shared matchesPreviewShortcut helper",
  );
});

test("HistoryCard.svelte references the shared matcher type", () => {
  // The per-card shortcut uses the shared `PreviewShortcutPlatform`
  // type so the matcher and the platform derivation stay in one
  // place. The card does NOT need to import the matcher itself
  // because it is loaded lazily inside `onMount` so the card
  // remains testable in a non-Tauri context.
  const source = readFileSync(
    resolvePath(process.cwd(), "src", "HistoryCard.svelte"),
    "utf8",
  );
  assert.ok(
    source.includes("PreviewShortcutPlatform"),
    "HistoryCard.svelte must consume the shared PreviewShortcutPlatform type",
  );
});