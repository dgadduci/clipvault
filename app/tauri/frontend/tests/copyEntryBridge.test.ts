/**
 * Bridge tests for `copyEntryCommand`.
 *
 * The keyboard copy-only flow routes through
 * `clipvault_copy_entry` instead of `clipvault_paste_entry`. The
 * bridge MUST forward the id and the optional mode argument, omit
 * the mode argument when the caller passes `null` so the legacy
 * plain-text path stays byte-identical, and propagate backend
 * rejections without leaking payload details.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";

import { copyEntryCommand } from "../src/lib/tauri.ts";
import type { CopyResponse } from "../src/types.ts";

type InvokeHandle = (
  cmd: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

function installTauriMock(invoker: InvokeHandle): void {
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: { invoke: invoker },
  };
}

function copiedResponse(id: number): CopyResponse {
  return {
    kind: "copied",
    id,
    capability: null,
    error_kind: null,
    message: null,
    guidance: null,
    mode: null,
  };
}

test("copyEntryCommand forwards id and rich mode to the backend", async () => {
  let observed: { cmd: string; args?: Record<string, unknown> } | undefined;
  installTauriMock(async (cmd, args) => {
    observed = { cmd, args };
    return copiedResponse(42);
  });
  await copyEntryCommand({ id: 42, mode: "rich" });
  assert.equal(observed?.cmd, "clipvault_copy_entry");
  assert.equal(observed?.args?.entryId, 42);
  assert.equal(observed?.args?.mode, "rich");
});

test("copyEntryCommand forwards id and plain mode to the backend", async () => {
  let observed: Record<string, unknown> | undefined;
  installTauriMock(async (_cmd, args) => {
    observed = args;
    return copiedResponse(7);
  });
  await copyEntryCommand({ id: 7, mode: "plain" });
  assert.equal(observed?.entryId, 7);
  assert.equal(observed?.mode, "plain");
});

test("copyEntryCommand omits the mode argument when the caller passes null", async () => {
  // The frontend passes `mode: null` for image entries so the
  // backend runs the legacy bitmap path. The bridge serialises
  // `null` to `null` in the IPC payload and the Rust side maps it
  // to [`PasteMode::Plain`] — the mode is ignored for image rows
  // anyway, so the value only affects textual entries.
  let observed: Record<string, unknown> | undefined;
  installTauriMock(async (_cmd, args) => {
    observed = args;
    return copiedResponse(11);
  });
  await copyEntryCommand({ id: 11, mode: null });
  assert.equal(observed?.entryId, 11);
  assert.equal(observed?.mode, null);
});

test("copyEntryCommand surfaces a copied_plain_fallback outcome unchanged", async () => {
  installTauriMock(async () => ({
    kind: "copied_plain_fallback",
    id: 9,
    capability: null,
    error_kind: null,
    message: null,
    guidance: null,
    mode: "plain",
  }));
  const response = await copyEntryCommand({ id: 9, mode: "rich" });
  assert.equal(response.kind, "copied_plain_fallback");
  assert.equal(response.mode, "plain");
  assert.equal(response.id, 9);
});

test("copyEntryCommand surfaces a capability_unavailable outcome with guidance", async () => {
  installTauriMock(async () => ({
    kind: "capability_unavailable",
    id: null,
    capability: "clipboard_write_image",
    error_kind: null,
    message: null,
    guidance: {
      capability: "clipboard_write_image",
      kind: "unsupported_session",
      title: "Sin imagen",
      summary: "La sesión actual no puede escribir imágenes.",
      steps: ["Cambia a una sesión compatible con imágenes."],
      retryable: false,
      can_open_settings: false,
      settings_target: null,
    },
    mode: null,
  }));
  const response = await copyEntryCommand({ id: 1, mode: null });
  assert.equal(response.kind, "capability_unavailable");
  assert.equal(response.capability, "clipboard_write_image");
  assert.equal(response.guidance?.kind, "unsupported_session");
});

test("copyEntryCommand propagates backend rejection as a promise rejection", async () => {
  installTauriMock(async () => {
    throw { kind: "clipboard", message: "backend busy" };
  });
  await assert.rejects(
    copyEntryCommand({ id: 1, mode: "plain" }),
    (error: unknown) => {
      assert.equal((error as { kind: string }).kind, "clipboard");
      return true;
    },
  );
});

test("copyEntryCommand never targets the paste Tauri command", async () => {
  // Pin the contract that the copy bridge never falls back to the
  // paste Tauri command. A regression that flips the backend name
  // would re-introduce synthetic paste behaviour.
  let observedCmd: string | undefined;
  installTauriMock(async (cmd) => {
    observedCmd = cmd;
    return copiedResponse(1);
  });
  await copyEntryCommand({ id: 1, mode: "plain" });
  assert.notEqual(observedCmd, "clipvault_paste_entry");
  assert.equal(observedCmd, "clipvault_copy_entry");
});
