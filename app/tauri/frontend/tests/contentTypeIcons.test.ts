/**
 * Tests for the SVG icon map the card rail renders for every
 * documented textual content type. The icons live as a single
 * `<symbol>` sprite so the rail renders the whole grid in one DOM
 * subtree; the helper functions below pin the lookup contract so a
 * future refactor that breaks an id surfaces here.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  contentTypeIconId,
  contentTypeIconLabel,
  CONTENT_TYPE_ICON_SPRITE,
  APP_FALLBACK_ICON_SVG,
} from "../src/lib/contentTypeIcons.ts";

const EXPECTED_IDS = [
  ["text", "cv-icon-text"],
  ["url", "cv-icon-url"],
  ["email", "cv-icon-email"],
  ["json", "cv-icon-json"],
  ["jwt", "cv-icon-jwt"],
  ["uuid", "cv-icon-uuid"],
  ["ipv4", "cv-icon-ipv4"],
  ["ipv6", "cv-icon-ipv6"],
  ["hex_color", "cv-icon-hex-color"],
  ["html", "cv-icon-html"],
  ["file_path", "cv-icon-file-path"],
  ["shell_command", "cv-icon-shell-command"],
  ["sql", "cv-icon-sql"],
  ["code", "cv-icon-code"],
  // `clipboard-rich-content` adds the Image glyph; the card renders it
  // in the header of an image entry and inside the thumbnail fallback.
  ["image", "cv-icon-image"],
];

test("contentTypeIconId returns the documented id for every supported type", () => {
  for (const [value, id] of EXPECTED_IDS) {
    assert.equal(contentTypeIconId(value), id, `value=${value}`);
  }
});

test("contentTypeIconId falls back to the generic text sprite for unknown values", () => {
  assert.equal(contentTypeIconId(null), "cv-icon-fallback");
  assert.equal(contentTypeIconId(undefined), "cv-icon-fallback");
  assert.equal(contentTypeIconId(""), "cv-icon-fallback");
  assert.equal(contentTypeIconId("future-type"), "cv-icon-fallback");
});

test("contentTypeIconLabel returns the documented label for every supported type", () => {
  for (const [value, id] of EXPECTED_IDS) {
    const label = contentTypeIconLabel(value);
    assert.ok(label.length > 0, `value=${value}`);
    // The label mirrors the type name to keep screen readers
    // consistent with the textual badge the rail also renders.
    assert.notEqual(label, id);
  }
});

test("sprite embeds a symbol for every documented type", () => {
  for (const [, id] of EXPECTED_IDS) {
    assert.ok(
      CONTENT_TYPE_ICON_SPRITE.includes(`id="${id}"`),
      `sprite must embed ${id}`,
    );
  }
  assert.ok(
    CONTENT_TYPE_ICON_SPRITE.includes('id="cv-icon-fallback"'),
    "sprite must embed the fallback glyph",
  );
});

test("fallback application icon is a stable local SVG", () => {
  assert.ok(APP_FALLBACK_ICON_SVG.includes("<svg"));
  assert.ok(APP_FALLBACK_ICON_SVG.includes("</svg>"));
  // Must NOT include any remote URL or external reference (the
  // `xmlns="http://..."` declaration is the canonical SVG namespace
  // and is the only `http` substring we allow).
  assert.ok(APP_FALLBACK_ICON_SVG.includes('xmlns="http://www.w3.org/2000/svg"'));
  assert.ok(!APP_FALLBACK_ICON_SVG.includes("https://"));
  assert.ok(!APP_FALLBACK_ICON_SVG.includes("src="));
});
