import { test } from "node:test";
import assert from "node:assert/strict";

import {
  contentTypeLabel,
  defaultCardTitle,
  isKnownContentType,
  validateTitle,
  MAX_TITLE_LENGTH,
} from "../src/lib/contentType.ts";

test("contentTypeLabel maps every documented variant", () => {
  const cases: Array<[string, string]> = [
    ["text", "Texto"],
    ["url", "URL"],
    ["email", "Email"],
    ["json", "JSON"],
    ["jwt", "JWT"],
    ["uuid", "UUID"],
    ["ipv4", "IPv4"],
    ["ipv6", "IPv6"],
    ["hex_color", "Hex color"],
    ["html", "HTML"],
    ["file_path", "Ruta"],
    ["shell_command", "Shell"],
    ["sql", "SQL"],
    ["code", "Código"],
    // `clipboard-rich-content` adds the first non-textual variant.
    ["image", "Imagen"],
  ];
  for (const [value, expected] of cases) {
    assert.equal(contentTypeLabel(value), expected, `value = ${value}`);
  }
});

test("contentTypeLabel falls back to Texto for unknown values", () => {
  assert.equal(contentTypeLabel(""), "Texto");
  assert.equal(contentTypeLabel("foo"), "Texto");
  assert.equal(contentTypeLabel("TEXT"), "Texto");
  assert.equal(contentTypeLabel("URL"), "Texto", "case must be snake_case");
  assert.equal(contentTypeLabel("unknown"), "Texto");
});

test("contentTypeLabel falls back to Texto for null/undefined", () => {
  assert.equal(contentTypeLabel(null), "Texto");
  assert.equal(contentTypeLabel(undefined), "Texto");
});

test("isKnownContentType returns true only for documented variants", () => {
  assert.equal(isKnownContentType("text"), true);
  assert.equal(isKnownContentType("url"), true);
  assert.equal(isKnownContentType("hex_color"), true);
  assert.equal(isKnownContentType("shell_command"), true);
  // `image` became a documented variant with `clipboard-rich-content`.
  assert.equal(isKnownContentType("image"), true);
  assert.equal(isKnownContentType(""), false);
  assert.equal(isKnownContentType(null), false);
  assert.equal(isKnownContentType(undefined), false);
});

test("contentTypeLabel is deterministic on repeated calls", () => {
  const inputs = [
    "text",
    "url",
    "json",
    "jwt",
    "uuid",
    "shell_command",
    "unknown-value",
    "",
  ];
  for (const value of inputs) {
    const first = contentTypeLabel(value);
    for (let i = 0; i < 5; i += 1) {
      assert.equal(contentTypeLabel(value), first, `value = ${value}`);
    }
  }
});

test("contentTypeLabel never echoes payload or snippets", () => {
  // The label table is metadata-only: it never includes the value
  // itself or anything that looks like clipboard content.
  const cases = [
    "text",
    "url",
    "json",
    "jwt",
    "uuid",
    "ipv4",
    "ipv6",
    "hex_color",
    "html",
    "file_path",
    "shell_command",
    "sql",
    "code",
  ];
  for (const value of cases) {
    const label = contentTypeLabel(value);
    assert.equal(
      label.includes(value),
      false,
      `label must not echo the value (${value} -> ${label})`,
    );
  }
});

test("defaultCardTitle mirrors contentTypeLabel for every documented variant", () => {
  const variants = [
    "text",
    "url",
    "email",
    "json",
    "jwt",
    "uuid",
    "ipv4",
    "ipv6",
    "hex_color",
    "html",
    "file_path",
    "shell_command",
    "sql",
    "code",
  ];
  for (const value of variants) {
    assert.equal(defaultCardTitle(value), contentTypeLabel(value));
  }
  assert.equal(defaultCardTitle(null), "Texto");
  assert.equal(defaultCardTitle(undefined), "Texto");
  assert.equal(defaultCardTitle("future"), "Texto");
});

test("validateTitle trims whitespace and clears on empty input", () => {
  const trimmed = validateTitle("  hello world  ");
  assert.equal(trimmed.ok, true);
  if (trimmed.ok) assert.equal(trimmed.title, "hello world");

  const cleared = validateTitle("   ");
  assert.equal(cleared.ok, true);
  if (cleared.ok) assert.equal(cleared.title, null);

  const empty = validateTitle("");
  assert.equal(empty.ok, true);
  if (empty.ok) assert.equal(empty.title, null);
});

test("validateTitle rejects input that exceeds the documented maximum length", () => {
  const tooLong = "x".repeat(MAX_TITLE_LENGTH + 1);
  const result = validateTitle(tooLong);
  assert.equal(result.ok, false);
  if (!result.ok) assert.equal(result.reason, "too_long");
});

test("validateTitle accepts input at the boundary length", () => {
  const exact = "x".repeat(MAX_TITLE_LENGTH);
  const result = validateTitle(exact);
  assert.equal(result.ok, true);
  if (result.ok) assert.equal(result.title, exact);
});

test("MAX_TITLE_LENGTH matches the Rust service constant", () => {
  // The Rust service enforces the same limit; the frontend
  // validation MUST match so the user gets a consistent copy.
  assert.equal(MAX_TITLE_LENGTH, 80);
});
