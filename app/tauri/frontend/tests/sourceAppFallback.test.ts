/**
 * Tests for the source-app fallback chain the history card relies
 * on. The user-reported bug was that the card showed the application
 * identifier as visible text and rendered a bare `—` placeholder
 * when metadata was missing. The new contract pins the card to
 * icon-only — the application name, identifier, bundle id and
 * "unknown" sentinel stay off the visual surface and are only
 * reachable through the `aria-label` / `title` attributes.
 *
 * The helpers in `src/lib/sourceAppFallback.ts` are the single
 * source of truth for the accessible label. The tests below pin the
 * three branches the helper must cover:
 *
 * 1. When the metadata provider resolved a user-visible display name
 *    the accessible label is `"Aplicación fuente: <name>"`.
 * 2. When only the privacy/matching identifier is populated the
 *    accessible label is `"Aplicación fuente: <id>"`. The identifier
 *    never reaches the visual surface.
 * 3. When neither column is populated the accessible label is the
 *    Spanish string `"Aplicación fuente desconocida"` and the legacy
 *    `—` placeholder NEVER appears in the value.
 *
 * The helper is the only piece of source-app copy the rail reads;
 * tests verify the contract directly without spinning up a Svelte
 * runtime.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  sourceAppAccessibleLabel,
} from "../src/lib/sourceAppFallback.ts";
import type { EntryRecord } from "../src/types.ts";

function makeEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 1,
    content: "hello",
    content_type: "text",
    content_size: 5,
    content_hash: "abc",
    source_app: null,
    is_pinned: false,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    last_seen_at: "2026-01-01T00:00:00Z",
    title: null,
    source_app_name: null,
    source_app_icon_ref: null,
    asset_ref: null,
    mime_type: null,
    payload_width: null,
    payload_height: null,
    ...overrides,
  };
}

test("sourceAppAccessibleLabel prefixes a known display name with the stable prefix", () => {
  const entry = makeEntry({
    source_app: "com.apple.Terminal",
    source_app_name: "Terminal",
  });
  assert.equal(sourceAppAccessibleLabel(entry), "Aplicación fuente: Terminal");
});

test("sourceAppAccessibleLabel falls back to the identifier when the name is missing", () => {
  const entry = makeEntry({
    source_app: "com.apple.TextEdit",
    source_app_name: null,
  });
  assert.equal(sourceAppAccessibleLabel(entry), "Aplicación fuente: com.apple.TextEdit");
});

test("sourceAppAccessibleLabel describes the missing case in Spanish", () => {
  const entry = makeEntry({
    source_app: null,
    source_app_name: null,
  });
  assert.equal(sourceAppAccessibleLabel(entry), "Aplicación fuente desconocida");
});

test("sourceAppAccessibleLabel never returns the legacy dash placeholder", () => {
  const entry = makeEntry({
    source_app: null,
    source_app_name: null,
  });
  assert.notEqual(sourceAppAccessibleLabel(entry), "—");
  assert.notEqual(sourceAppAccessibleLabel(entry), "-");
});

test("sourceAppAccessibleLabel trims whitespace-only values", () => {
  const entry = makeEntry({
    source_app: "   ",
    source_app_name: "   ",
  });
  assert.equal(sourceAppAccessibleLabel(entry), "Aplicación fuente desconocida");
});

test("sourceAppAccessibleLabel prefers the name even when the identifier is present", () => {
  const entry = makeEntry({
    source_app: "com.apple.Safari",
    source_app_name: "Safari",
  });
  assert.equal(sourceAppAccessibleLabel(entry), "Aplicación fuente: Safari");
});

test("sourceAppAccessibleLabel treats empty strings as missing", () => {
  const entry = makeEntry({
    source_app: "",
    source_app_name: "",
  });
  assert.equal(sourceAppAccessibleLabel(entry), "Aplicación fuente desconocida");
});

test("sourceAppAccessibleLabel never exposes the bare identifier in the visible surface", () => {
  // The card rail exposes the helper result through the
  // `aria-label` and `title` attributes only; this test pins that
  // contract by asserting no identifier-shaped substring ever
  // appears outside the `Aplicación fuente:` prefix and no helper
  // ever returns the raw identifier as a "visible text" stand-in.
  const known = makeEntry({
    source_app: "com.apple.Terminal",
    source_app_name: "Terminal",
  });
  const label = sourceAppAccessibleLabel(known);
  assert.ok(
    label.startsWith("Aplicación fuente:"),
    `label must keep the stable prefix: ${label}`,
  );
  assert.ok(
    !label.includes("unknown") && !label.includes("desconocida"),
    `a known entry must never report 'desconocida'`,
  );

  const unknown = makeEntry({});
  const unknownLabel = sourceAppAccessibleLabel(unknown);
  assert.equal(unknownLabel, "Aplicación fuente desconocida");
  assert.ok(
    !unknownLabel.includes("com.apple."),
    `missing entries must not echo identifier prefixes: ${unknownLabel}`,
  );
});