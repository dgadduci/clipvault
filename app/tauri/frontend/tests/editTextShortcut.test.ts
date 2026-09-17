/**
 * Tests for the platform-aware `Cmd/Ctrl+E` shortcut helper the
 * Desktop installs to open the in-window text editor on an eligible
 * card.
 *
 * The contract being pinned:
 *
 *   - `editTextShortcutPlatform` routes the same diagnostics
 *     strings the search-shortcut helper already accepts, so a tweak
 *     to `platformFromDiagnostics` only has to land in one place;
 *   - the matcher recognises the platform-specific binding while
 *     rejecting accidental combinations (`Alt`, `Shift`, the wrong
 *     modifier) and case variants of the `E` letter;
 *   - the visible / accessible label pair and the
 *     `aria-keyshortcuts` attribute stay in lockstep with the
 *     matcher so the menu hint the user reads matches the binding
 *     the listener accepts.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  EDIT_TEXT_SHORTCUT_LETTER,
  editTextShortcutAccessibleLabel,
  editTextShortcutKeyAttribute,
  editTextShortcutLabel,
  editTextShortcutPlatform,
  matchesEditTextShortcut,
  resolveEditTextShortcutEntryId,
} from "../src/lib/editTextShortcut.ts";

test("editTextShortcutPlatform: routes macOS family to macos", () => {
  assert.equal(editTextShortcutPlatform("macos"), "macos");
  assert.equal(editTextShortcutPlatform("darwin"), "macos");
  assert.equal(editTextShortcutPlatform("OSX"), "macos");
  assert.equal(editTextShortcutPlatform("MacOS 14.5"), "macos");
});

test("editTextShortcutPlatform: routes linux and other hosts to other", () => {
  assert.equal(editTextShortcutPlatform("linux"), "other");
  assert.equal(editTextShortcutPlatform("Linux Ubuntu 22.04"), "other");
  assert.equal(editTextShortcutPlatform("windows"), "other");
  assert.equal(editTextShortcutPlatform("freebsd"), "other");
  assert.equal(editTextShortcutPlatform(""), "other");
  assert.equal(editTextShortcutPlatform(null), "other");
  assert.equal(editTextShortcutPlatform(undefined), "other");
});

test("EDIT_TEXT_SHORTCUT_LETTER is the lowercase letter the matcher accepts", () => {
  assert.equal(EDIT_TEXT_SHORTCUT_LETTER, "e");
});

test("editTextShortcutLabel: macos uses the dedicated glyph", () => {
  assert.equal(editTextShortcutLabel("macos"), "⌘E");
  assert.equal(editTextShortcutLabel("other"), "Ctrl+E");
});

test("editTextShortcutAccessibleLabel: matches the visible hint", () => {
  assert.match(editTextShortcutAccessibleLabel("macos"), /Comando E/);
  assert.match(editTextShortcutAccessibleLabel("other"), /Control E/);
});

test("editTextShortcutKeyAttribute: mirrors the WAI-ARIA modifier table", () => {
  assert.equal(editTextShortcutKeyAttribute("macos"), "Meta+E");
  assert.equal(editTextShortcutKeyAttribute("other"), "Control+E");
});

test("resolveEditTextShortcutEntryId: current selection wins over stale focus", () => {
  assert.equal(resolveEditTextShortcutEntryId(42, 7), 42);
  assert.equal(
    resolveEditTextShortcutEntryId(null, 7),
    7,
    "focused card remains the fallback when there is no rail selection",
  );
  assert.equal(resolveEditTextShortcutEntryId(null, null), null);
});

test("matchesEditTextShortcut: macOS accepts Cmd+E and rejects Ctrl+E", () => {
  const platform = "macos" as const;
  assert.equal(
    matchesEditTextShortcut(
      {
        key: "e",
        metaKey: true,
        ctrlKey: false,
        altKey: false,
        shiftKey: false,
      },
      platform,
    ),
    true,
  );
  assert.equal(
    matchesEditTextShortcut(
      {
        key: "E",
        metaKey: true,
        ctrlKey: false,
        altKey: false,
        shiftKey: false,
      },
      platform,
    ),
    true,
    "case-insensitive key match",
  );
  assert.equal(
    matchesEditTextShortcut(
      {
        key: "e",
        metaKey: false,
        ctrlKey: true,
        altKey: false,
        shiftKey: false,
      },
      platform,
    ),
    false,
    "macOS must not steal Ctrl+E",
  );
  assert.equal(
    matchesEditTextShortcut(
      {
        key: "e",
        metaKey: true,
        ctrlKey: true,
        altKey: false,
        shiftKey: false,
      },
      platform,
    ),
    false,
    "macOS must not accept the mixed Ctrl+Cmd combination",
  );
});

test("matchesEditTextShortcut: linux accepts Ctrl+E and rejects Cmd+E", () => {
  const platform = "other" as const;
  assert.equal(
    matchesEditTextShortcut(
      {
        key: "e",
        metaKey: false,
        ctrlKey: true,
        altKey: false,
        shiftKey: false,
      },
      platform,
    ),
    true,
  );
  assert.equal(
    matchesEditTextShortcut(
      {
        key: "E",
        metaKey: false,
        ctrlKey: true,
        altKey: false,
        shiftKey: false,
      },
      platform,
    ),
    true,
    "case-insensitive key match",
  );
  assert.equal(
    matchesEditTextShortcut(
      {
        key: "e",
        metaKey: true,
        ctrlKey: false,
        altKey: false,
        shiftKey: false,
      },
      platform,
    ),
    false,
    "Linux must not steal Cmd+E",
  );
  assert.equal(
    matchesEditTextShortcut(
      {
        key: "e",
        metaKey: true,
        ctrlKey: true,
        altKey: false,
        shiftKey: false,
      },
      platform,
    ),
    false,
    "Linux must not accept the mixed Ctrl+Cmd combination",
  );
});

test("matchesEditTextShortcut: rejects Alt+ or Shift+ combinations", () => {
  const cases = ["macos" as const, "other" as const];
  for (const platform of cases) {
    assert.equal(
      matchesEditTextShortcut(
        {
          key: "e",
          metaKey: platform === "macos",
          ctrlKey: platform !== "macos",
          altKey: true,
          shiftKey: false,
        },
        platform,
      ),
      false,
      `${platform} rejects Alt+ shortcut`,
    );
    assert.equal(
      matchesEditTextShortcut(
        {
          key: "e",
          metaKey: platform === "macos",
          ctrlKey: platform !== "macos",
          altKey: false,
          shiftKey: true,
        },
        platform,
      ),
      false,
      `${platform} rejects Shift+ shortcut`,
    );
  }
});

test("matchesEditTextShortcut: a different letter is not the shortcut", () => {
  for (const platform of ["macos", "other"] as const) {
    assert.equal(
      matchesEditTextShortcut(
        {
          key: "f",
          metaKey: platform === "macos",
          ctrlKey: platform !== "macos",
          altKey: false,
          shiftKey: false,
        },
        platform,
      ),
      false,
    );
    assert.equal(
      matchesEditTextShortcut(
        {
          key: "",
          metaKey: platform === "macos",
          ctrlKey: platform !== "macos",
          altKey: false,
          shiftKey: false,
        },
        platform,
      ),
      false,
    );
  }
});
