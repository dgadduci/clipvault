/**
 * Tests for the search-shortcut helper used by the desktop shell
 * to bind `Cmd+F` (macOS) and `Ctrl+F` (Linux) to the search input.
 *
 * The contract being pinned:
 *
 *   - the platform helper routes macOS-family diagnostics values
 *     to `"macos"` and everything else to `"other"`;
 *   - the matching helper recognises the platform-specific binding
 *     while rejecting accidental combinations (Alt, Shift, mixed
 *     meta/ctrl);
 *   - the visible label and accessible label are stable strings the
 *     toolbar renders and the regression suite asserts.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  matchesSearchShortcut,
  searchShortcutAccessibleLabel,
  searchShortcutLabel,
  searchShortcutPlatform,
} from "../src/lib/searchShortcut.ts";

test("searchShortcutPlatform: routes macOS family to macos", () => {
  assert.equal(searchShortcutPlatform("macos"), "macos");
  assert.equal(searchShortcutPlatform("darwin"), "macos");
  assert.equal(searchShortcutPlatform("OSX"), "macos");
  assert.equal(searchShortcutPlatform("MacOS 14.5"), "macos");
});

test("searchShortcutPlatform: routes linux and other hosts to other", () => {
  assert.equal(searchShortcutPlatform("linux"), "other");
  assert.equal(searchShortcutPlatform("Linux Ubuntu 22.04"), "other");
  assert.equal(searchShortcutPlatform("windows"), "other");
  assert.equal(searchShortcutPlatform("freebsd"), "other");
  assert.equal(searchShortcutPlatform(""), "other");
  assert.equal(searchShortcutPlatform(null), "other");
  assert.equal(searchShortcutPlatform(undefined), "other");
});

test("searchShortcutLabel: macos uses the dedicated glyph", () => {
  assert.equal(searchShortcutLabel("macos"), "⌘F");
  assert.equal(searchShortcutLabel("other"), "Ctrl F");
});

test("searchShortcutAccessibleLabel: matches the visible hint", () => {
  assert.match(searchShortcutAccessibleLabel("macos"), /Comando F/);
  assert.match(searchShortcutAccessibleLabel("other"), /Control F/);
});

test("matchesSearchShortcut: macOS accepts Cmd+F and rejects Ctrl+F", () => {
  const platform = "macos" as const;
  assert.equal(
    matchesSearchShortcut(
      { key: "f", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false },
      platform,
    ),
    true,
  );
  assert.equal(
    matchesSearchShortcut(
      { key: "F", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false },
      platform,
    ),
    true,
    "case-insensitive key match",
  );
  assert.equal(
    matchesSearchShortcut(
      { key: "f", metaKey: true, ctrlKey: true, altKey: false, shiftKey: false },
      platform,
    ),
    false,
    "macOS must not steal Ctrl+F",
  );
});

test("matchesSearchShortcut: linux accepts Ctrl+F and rejects Cmd+F", () => {
  const platform = "other" as const;
  assert.equal(
    matchesSearchShortcut(
      { key: "f", ctrlKey: true, metaKey: false, altKey: false, shiftKey: false },
      platform,
    ),
    true,
  );
  assert.equal(
    matchesSearchShortcut(
      { key: "f", ctrlKey: true, metaKey: true, altKey: false, shiftKey: false },
      platform,
    ),
    false,
    "Linux must not steal Cmd+F",
  );
});

test("matchesSearchShortcut: rejects Alt+ or Shift+ combinations", () => {
  const cases = [
    "macos" as const,
    "other" as const,
  ];
  for (const platform of cases) {
    assert.equal(
      matchesSearchShortcut(
        {
          key: "f",
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
      matchesSearchShortcut(
        {
          key: "f",
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

test("matchesSearchShortcut: a different letter is not the shortcut", () => {
  for (const platform of ["macos", "other"] as const) {
    assert.equal(
      matchesSearchShortcut(
        {
          key: "g",
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
