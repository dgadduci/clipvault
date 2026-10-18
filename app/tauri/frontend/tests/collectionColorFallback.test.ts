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
import {
  COLLECTION_COLOR_FALLBACK,
  collectionColor,
  collectionColorTint,
} from "../src/lib/collectionColor.ts";

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
    assert.equal(collectionColor(bad), COLLECTION_COLOR_FALLBACK, `bad: ${bad}`);
  }
});

test("collectionColor handles non-string payloads without throwing", () => {
  assert.equal(collectionColor(null), COLLECTION_COLOR_FALLBACK);
  assert.equal(collectionColor(undefined), COLLECTION_COLOR_FALLBACK);
  assert.equal(collectionColor(0xdeadbeef), COLLECTION_COLOR_FALLBACK);
});

test("collectionColorTint converts a valid collection color at 18% opacity", () => {
  assert.equal(collectionColorTint("#AABBCC"), "rgba(170, 187, 204, 0.18)");
});

test("collectionColorTint uses the safe fallback for invalid colors", () => {
  assert.equal(collectionColorTint("url(javascript:alert(1))"), "rgba(148, 163, 184, 0.18)");
});

test("collectionColorTint clamps opacity to the CSS alpha range", () => {
  assert.equal(collectionColorTint("#000000", 2), "rgba(0, 0, 0, 1)");
  assert.equal(collectionColorTint("#000000", -1), "rgba(0, 0, 0, 0)");
});
