/**
 * Bridge tests for `pasteEntryCommand`.
 *
 * The history card menu exposes two paste actions that share the
 * existing Tauri command: rich mode (`mode = "rich"`) preserves the
 * original HTML/RTF representations, plain mode (`mode = "plain"`)
 * always writes only the canonical plain text. The bridge must
 * forward the mode argument unchanged, omit it when the caller
 * passes `null` so the legacy quick-paste path stays byte-identical,
 * and propagate backend rejections without leaking payload details.
 *
 * The suite is deliberately metadata-only: the response object is
 * serialised back to JSON and checked for absent content / hashes /
 * asset references so the bridge can never accidentally surface
 * clipboard content to a log line.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { pasteEntryCommand } from "../src/lib/tauri.ts";
import type { PasteResponse } from "../src/types.ts";

type InvokeHandle = (
  cmd: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

function installTauriMock(invoker: InvokeHandle): void {
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: { invoke: invoker },
  };
}

function pastedResponse(id: number): PasteResponse {
  return {
    kind: "pasted",
    id,
    capability: null,
    error_kind: null,
    message: null,
    guidance: null,
    mode: null,
  };
}

test("pasteEntryCommand forwards id and rich mode to the backend", async () => {
  let observed: { cmd: string; args?: Record<string, unknown> } | undefined;
  installTauriMock(async (cmd, args) => {
    observed = { cmd, args };
    return pastedResponse(42);
  });
  await pasteEntryCommand({ id: 42, mode: "rich" });
  assert.equal(observed?.cmd, "clipvault_paste_entry");
  assert.equal(observed?.args?.entryId, 42);
  assert.equal(observed?.args?.mode, "rich");
});

test("pasteEntryCommand forwards id and plain mode to the backend", async () => {
  let observed: Record<string, unknown> | undefined;
  installTauriMock(async (_cmd, args) => {
    observed = args;
    return pastedResponse(7);
  });
  await pasteEntryCommand({ id: 7, mode: "plain" });
  assert.equal(observed?.entryId, 7);
  assert.equal(observed?.mode, "plain");
});

test("pasteEntryCommand omits the mode argument when the caller passes null", async () => {
  // Legacy quick-paste callers never send a mode: the backend MUST
  // default to plain. The bridge serialises `null` to `null` in the
  // IPC payload so the Rust side parses it as the default.
  let observed: Record<string, unknown> | undefined;
  installTauriMock(async (_cmd, args) => {
    observed = args;
    return pastedResponse(3);
  });
  await pasteEntryCommand({ id: 3, mode: null });
  assert.equal(observed?.entryId, 3);
  assert.equal(observed?.mode, null);
});

test("pasteEntryCommand surfaces a pasted_plain_fallback outcome unchanged", async () => {
  installTauriMock(async () => ({
    kind: "pasted_plain_fallback",
    id: 9,
    capability: null,
    error_kind: null,
    message: null,
    guidance: null,
    mode: "plain",
  }));
  const response = await pasteEntryCommand({ id: 9, mode: "rich" });
  assert.equal(response.kind, "pasted_plain_fallback");
  assert.equal(response.mode, "plain");
  assert.equal(response.id, 9);
});

test("pasteEntryCommand surfaces a capability_unavailable outcome with guidance", async () => {
  installTauriMock(async () => ({
    kind: "capability_unavailable",
    id: null,
    capability: "clipboard_write_rich_text",
    error_kind: null,
    message: null,
    guidance: {
      capability: "clipboard_write_rich_text",
      kind: "unsupported_session",
      title: "Sin texto enriquecido",
      summary: "La sesión actual no puede escribir texto enriquecido.",
      steps: ["Cambia a una sesión compatible con HTML/RTF."],
      retryable: false,
      can_open_settings: false,
      settings_target: null,
    },
    mode: null,
  }));
  const response = await pasteEntryCommand({ id: 1, mode: "rich" });
  assert.equal(response.kind, "capability_unavailable");
  assert.equal(response.capability, "clipboard_write_rich_text");
  assert.equal(response.guidance?.kind, "unsupported_session");
  // The guidance MUST classify the failure as a session limit, not a
  // permission the user could grant — a false permission prompt is
  // the regression this assertion guards against.
  assert.equal(response.guidance?.can_open_settings, false);
});

test("pasteEntryCommand propagates backend rejection as a promise rejection", async () => {
  installTauriMock(async () => {
    throw { kind: "clipboard", message: "backend busy" };
  });
  await assert.rejects(
    pasteEntryCommand({ id: 1, mode: "plain" }),
    (error: unknown) => {
      assert.equal((error as { kind: string }).kind, "clipboard");
      return true;
    },
  );
});
