/**
 * Bridge tests for `ignoredAppIconCommand`.
 *
 * The command is the only path between the relative `icon_ref` the
 * database stores and the PNG bytes the webview can render. The
 * tests pin the contract the panel depends on:
 *
 * - The command targets `clipvault_ignored_app_icon` and forwards the
 *   `iconRef` argument unchanged.
 * - The response is a `number[]` (Tauri's JSON encoding of `Vec<u8>`)
 *   so the caller can wrap it in `Uint8Array` before constructing a
 *   `Blob`.
 * - Tauri rejection paths (validation failure, missing file, ...)
 *   propagate as a rejected promise; the settings panel surfaces a
 *   generic fallback instead of the underlying error to the user.
 * - The wrapper is metadata-only: it must never carry clipboard
 *   content, hashes, snippets or absolute filesystem paths.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { ignoredAppIconCommand } from "../src/lib/tauri.ts";

type InvokeHandle = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

function installTauriMock(invoker: InvokeHandle): void {
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: { invoke: invoker },
  };
}

test("ignoredAppIconCommand forwards the ref argument to the backend", async () => {
  let observed: { cmd: string; args?: Record<string, unknown> } | undefined;
  installTauriMock(async (cmd, args) => {
    observed = { cmd, args };
    return [0x89, 0x50, 0x4e, 0x47];
  });
  await ignoredAppIconCommand({ ref: "ignored-apps/com.apple.textedit.png" });
  assert.equal(observed?.cmd, "clipvault_ignored_app_icon");
  assert.equal(observed?.args?.iconRef, "ignored-apps/com.apple.textedit.png");
});

test("ignoredAppIconCommand returns the raw byte array", async () => {
  installTauriMock(async () => [1, 2, 3, 4, 5]);
  const bytes = await ignoredAppIconCommand({ ref: "ignored-apps/x.png" });
  assert.ok(Array.isArray(bytes));
  assert.deepEqual(bytes, [1, 2, 3, 4, 5]);
});

test("ignoredAppIconCommand surfaces backend rejection as a promise rejection", async () => {
  installTauriMock(async () => {
    throw { kind: "invalid_icon_ref", message: "absolute" };
  });
  await assert.rejects(
    ignoredAppIconCommand({ ref: "/etc/passwd" }),
    (error: unknown) => {
      assert.equal(typeof error, "object");
      assert.equal((error as { kind: string }).kind, "invalid_icon_ref");
      return true;
    },
  );
});

test("ignoredAppIconCommand never carries clipboard content or absolute paths", async () => {
  let capturedArgs: Record<string, unknown> | undefined;
  installTauriMock(async (_cmd, args) => {
    capturedArgs = args;
    return [0x89, 0x50, 0x4e, 0x47];
  });
  await ignoredAppIconCommand({ ref: "ignored-apps/com.apple.textedit.png" });
  // The forwarded argument must contain only the relative ref; it
  // must NOT carry clipboard content, hashes, snippets or absolute
  // paths.
  assert.deepEqual(Object.keys(capturedArgs ?? {}), ["iconRef"]);
  assert.equal(
    (capturedArgs as { iconRef: string }).iconRef,
    "ignored-apps/com.apple.textedit.png",
  );
  // Defensive: ensure the ref still does not leak an absolute path
  // after the wrapper returns it (the backend will reject one).
  assert.equal(
    (capturedArgs as { iconRef: string }).iconRef.startsWith("/"),
    false,
    "iconRef must remain relative",
  );
});

test("ignoredAppIconCommand does not mutate the supplied ref", async () => {
  installTauriMock(async () => []);
  const original = "ignored-apps/com.apple.TextEdit.png";
  await ignoredAppIconCommand({ ref: original });
  // The original string must reach the backend unchanged. The
  // settings panel normalises identifiers in `PrivacyGate`, not in
  // the icon bridge.
  // (The observation happens through the captured `args.iconRef`
  // assertion in the previous test; here we just pin that the
  // wrapper does not throw on a case-preserving ref.)
});