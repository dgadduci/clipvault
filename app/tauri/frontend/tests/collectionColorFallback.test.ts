/**
 * Pure helper coverage for the colour label rendering the
 * `clipboard-history-cards` change adds. The card renders each
 * assigned collection's `color_hex` value as its text colour;
 * legacy rows that pre-date the migration, malformed payloads
 * from a hand-edit regression or any other unforeseen value must
 * never break the card so the helper falls back to a neutral grey
 * without ever leaking clipboard content.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

function collectionColor(value: unknown): string {
  const fallback = "#94a3b8";
  if (typeof value !== "string") return fallback;
  const body = value.trim().toLowerCase();
  if (/^#[0-9a-f]{6}$/.test(body)) return body;
  return fallback;
}

test("collectionColor accepts canonical #rrggbb values", () => {
  for (const valid of ["#c62828", "#1565c0", "#2e7d32", "#a16207"]) {
    assert.equal(collectionColor(valid), valid);
  }
});

test("collectionColor lowercases accepted input", () => {
  assert.equal(collectionColor("#AABBCC"), "#aabbcc");
});

test("collectionColor trims surrounding whitespace", () => {
  assert.equal(collectionColor("  #c62828  "), "#c62828");
});

test("collectionColor rejects alpha, names and short forms", () => {
  for (const bad of [
    "#aabbccdd",
    "red",
    "currentColor",
    "#abc",
    "#aabbc",
    "#xyzxyz",
    "#gggggg",
    "aabbcc",
    "##aabbcc",
    "",
  ]) {
    assert.equal(collectionColor(bad), "#94a3b8", `bad: ${bad}`);
  }
});

test("collectionColor handles non-string payloads without throwing", () => {
  assert.equal(collectionColor(null), "#94a3b8");
  assert.equal(collectionColor(undefined), "#94a3b8");
  assert.equal(collectionColor(0xdeadbeef), "#94a3b8");
});
