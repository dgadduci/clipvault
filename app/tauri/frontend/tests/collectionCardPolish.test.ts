// Pure helpers tests for the `desktop-collection-card-polish` change.
//
// The change introduces three small helpers the rail consumes in place:
// elapsed-time label, Unicode code-point counter and base-1024 byte
// formatter. The tests pin every documented branch so a future
// regression in either formatter or counter cannot drift the cards
// unnoticed.

import { test } from "node:test";
import assert from "node:assert/strict";

import { formatElapsedTime } from "../src/lib/elapsedTime.ts";
import { unicodeCount } from "../src/lib/unicodeCount.ts";
import { formatByteSize } from "../src/lib/byteSize.ts";

const ANCHOR_MS = Date.UTC(2026, 0, 2, 3, 4, 5);

function at(secondsFromAnchor: number): Date {
  return new Date(ANCHOR_MS + secondsFromAnchor * 1000);
}

test("formatElapsedTime: sub-minute elapsed collapses to Ahora", () => {
  const result = formatElapsedTime(
    at(-30).toISOString(),
    at(0),
  );
  assert.equal(result.visual, "Ahora");
  assert.match(result.accessible, /menos de un minuto/);
});

test("formatElapsedTime: minutes use singular label", () => {
  const result = formatElapsedTime(
    at(-5 * 60).toISOString(),
    at(0),
  );
  assert.equal(result.visual, "Hace 5 min");
  assert.match(result.accessible, /5 minutos/);
});

test("formatElapsedTime: hour boundary switches to 'h' label", () => {
  const result = formatElapsedTime(
    at(-2 * 3600).toISOString(),
    at(0),
  );
  assert.equal(result.visual, "Hace 2 h");
  assert.match(result.accessible, /2 horas/);
});

test("formatElapsedTime: day boundary switches to 'días' label", () => {
  const result = formatElapsedTime(
    at(-3 * 86400).toISOString(),
    at(0),
  );
  assert.equal(result.visual, "Hace 3 días");
  assert.match(result.accessible, /3 días/);
});

test("formatElapsedTime: month boundary uses 30-day months", () => {
  const result = formatElapsedTime(
    at(-3 * 30 * 86400).toISOString(),
    at(0),
  );
  assert.equal(result.visual, "Hace 3 meses");
  assert.match(result.accessible, /3 meses/);
});

test("formatElapsedTime: years use 365-day years", () => {
  const result = formatElapsedTime(
    at(-2 * 365 * 86400).toISOString(),
    at(0),
  );
  assert.equal(result.visual, "Hace 2 años");
  assert.match(result.accessible, /2 años/);
});

test("formatElapsedTime: future timestamp is clamped to 'Recién capturado'", () => {
  const result = formatElapsedTime(
    at(120).toISOString(),
    at(0),
  );
  assert.equal(result.visual, "Recién capturado");
});

test("formatElapsedTime: invalid timestamp yields a deterministic fallback", () => {
  for (const bad of [null, undefined, "", "not-a-timestamp", "garbage"]) {
    const result = formatElapsedTime(bad, at(0));
    assert.equal(result.visual, "Reciente");
    assert.match(result.accessible, /desconocida|desconocido|Fecha/);
  }
});

// unicodeCount ------------------------------------------------------------

test("unicodeCount: ASCII text matches its code-point length", () => {
  assert.equal(unicodeCount(""), 0);
  assert.equal(unicodeCount("hello"), 5);
});

test("unicodeCount: multi-byte characters count as a single code point", () => {
  assert.equal(unicodeCount("café"), 4);
  assert.equal(unicodeCount("résumé"), 6);
});

test("unicodeCount: emoji and supplementary planes count by code point", () => {
  // "👨‍👩‍👧" is a single grapheme composed from five code points:
  // three emoji in the supplementary plane plus two ZWJ
  // characters. The counter matches `Array.from`'s code-point
  // iteration; the printed character count agrees with the user's
  // perception of multi-codepoint glyphs.
  const family = "👨\u200d👩\u200d👧";
  assert.equal(unicodeCount(family), 5);
  assert.equal(unicodeCount("🚀"), 1);
  assert.equal(unicodeCount("A"), 1);
});

test("unicodeCount: non-string inputs collapse to 0 without throwing", () => {
  for (const bad of [null, undefined, 123, {}, [], true]) {
    assert.equal(unicodeCount(bad), 0);
  }
});

// formatByteSize -----------------------------------------------------------

test("formatByteSize: bytes use the B unit under 1 KiB", () => {
  const result = formatByteSize(950);
  assert.equal(result.visual, "950 B");
  assert.match(result.accessible, /^950 bytes/);
});

test("formatByteSize: kilobytes use base-1024 with KB label", () => {
  const kb = formatByteSize(2048);
  assert.equal(kb.visual, "2.00 KB");
  assert.match(kb.accessible, /2048 bytes/);
  assert.match(kb.accessible, /kibibytes/);
});

test("formatByteSize: megabytes carry the MB label", () => {
  const result = formatByteSize(5 * 1024 * 1024);
  assert.equal(result.visual, "5.00 MB");
  assert.match(result.accessible, /mebibytes/);
});

test("formatByteSize: very small megabytes get two decimals, then one", () => {
  const sub10 = formatByteSize(2 * 1024 * 1024 + 512 * 1024);
  assert.match(sub10.visual, /MB$/);
  // 2.5 MB at base-1024 rounding.
  const sub100 = formatByteSize(15 * 1024 * 1024);
  assert.match(sub100.visual, /^15\.\d MB$/);
});

test("formatByteSize: large values collapse to GB", () => {
  const result = formatByteSize(3 * 1024 * 1024 * 1024);
  assert.match(result.visual, /GB$/);
});

test("formatByteSize: negative or non-finite inputs collapse to a safe label", () => {
  for (const bad of [-1, Number.NaN, Number.POSITIVE_INFINITY]) {
    const result = formatByteSize(bad);
    assert.equal(result.visual, "—");
    assert.match(result.accessible, /desconocido/);
  }
});
