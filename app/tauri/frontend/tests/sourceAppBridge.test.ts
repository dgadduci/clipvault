/**
 * Tests for the source-app icon bridge. The command is the only
 * bridge between the relative `source_app_icon_ref` the database
 * stores and the PNG bytes the history card can render.
 *
 * The tests mirror the contract pinned by `iconBridge.test.ts`:
 * - the command targets the right Tauri command name and forwards
 *   the `iconRef` argument unchanged;
 * - the response is a `number[]` (Tauri's JSON encoding of `Vec<u8>`);
 * - failures surface as a rejected promise so the resolver can
 *   collapse them to `null` without leaking the underlying error.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { sourceAppIconCommand, setEntryTitleCommand } from "../src/lib/tauri.ts";
import type { EntryRecord } from "../src/types.ts";

type InvokeHandle = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

function installTauriMock(invoker: InvokeHandle): void {
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: { invoke: invoker },
  };
}

test("sourceAppIconCommand forwards the ref argument to the backend", async () => {
  let observed: { cmd: string; args?: Record<string, unknown> } | undefined;
  installTauriMock(async (cmd, args) => {
    observed = { cmd, args };
    return [0x89, 0x50, 0x4e, 0x47];
  });
  await sourceAppIconCommand({ ref: "application-icons/com.apple.textedit.png" });
  assert.equal(observed?.cmd, "clipvault_source_app_icon");
  assert.equal(
    observed?.args?.iconRef,
    "application-icons/com.apple.textedit.png",
  );
});

test("sourceAppIconCommand returns the raw byte array", async () => {
  installTauriMock(async () => [1, 2, 3, 4, 5]);
  const bytes = await sourceAppIconCommand({
    ref: "application-icons/x.png",
  });
  assert.deepEqual(bytes, [1, 2, 3, 4, 5]);
});

test("sourceAppIconCommand propagates backend rejection as a promise rejection", async () => {
  installTauriMock(async () => {
    throw { kind: "invalid_icon_ref", message: "out_of_scope" };
  });
  await assert.rejects(
    sourceAppIconCommand({ ref: "/etc/passwd" }),
    (error: unknown) => {
      assert.equal((error as { kind: string }).kind, "invalid_icon_ref");
      return true;
    },
  );
});

test("sourceAppIconCommand never forwards an absolute path", async () => {
  let captured: Record<string, unknown> | undefined;
  installTauriMock(async (_cmd, args) => {
    captured = args;
    return [0x89, 0x50, 0x4e, 0x47];
  });
  await sourceAppIconCommand({ ref: "application-icons/com.apple.textedit.png" });
  const ref = (captured as { iconRef: string }).iconRef;
  assert.equal(ref.startsWith("/"), false, "iconRef must remain relative");
});

test("setEntryTitleCommand forwards id and title to the backend", async () => {
  let observed: { cmd: string; args?: Record<string, unknown> } | undefined;
  installTauriMock(async (cmd, args) => {
    observed = { cmd, args };
    const record: EntryRecord = {
      id: 1,
      content: "hello",
      content_type: "text",
      content_size: 5,
      content_hash: "x",
      source_app: null,
      is_pinned: false,
      created_at: "2026-01-02T03:04:05Z",
      updated_at: "2026-01-02T03:04:05Z",
      last_seen_at: "2026-01-02T03:04:05Z",
      title: "Custom",
      source_app_name: null,
      source_app_icon_ref: null,
    asset_ref: null,
    mime_type: null,
    payload_width: null,
    payload_height: null,
    };
    return { kind: "updated", entry: record };
  });
  const response = await setEntryTitleCommand({ id: 1, title: "Custom" });
  assert.equal(observed?.cmd, "clipvault_set_entry_title");
  assert.equal(observed?.args?.entryId, 1);
  assert.equal(observed?.args?.title, "Custom");
  assert.equal(response.kind, "updated");
});

test("setEntryTitleCommand maps null title to the restore-default path", async () => {
  let observed: Record<string, unknown> | undefined;
  installTauriMock(async (_cmd, args) => {
    observed = args;
    const record: EntryRecord = {
      id: 1,
      content: "hello",
      content_type: "text",
      content_size: 5,
      content_hash: "x",
      source_app: null,
      is_pinned: false,
      created_at: "2026-01-02T03:04:05Z",
      updated_at: "2026-01-02T03:04:05Z",
      last_seen_at: "2026-01-02T03:04:05Z",
      title: null,
      source_app_name: null,
      source_app_icon_ref: null,
    asset_ref: null,
    mime_type: null,
    payload_width: null,
    payload_height: null,
    };
    return { kind: "updated", entry: record };
  });
  const response = await setEntryTitleCommand({ id: 1, title: null });
  assert.equal(observed?.title, null);
  assert.equal(response.kind, "updated");
});
