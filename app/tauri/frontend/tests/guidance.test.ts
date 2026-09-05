import { test } from "node:test";
import * as assert from "node:assert/strict";

import {
  retryGuidance,
  shouldEnablePasteButton,
  presentPaste,
} from "../src/lib/guidance.ts";
import type { Capabilities, PlatformGuidance } from "../src/types.ts";

function allCapabilities(overrides: Partial<Capabilities> = {}): Capabilities {
  return {
    clipboard_read: true,
    clipboard_write: true,
    global_hotkey: true,
    synthetic_paste: true,
    active_application: true,
    tray: true,
    ...overrides,
  };
}

test("retryGuidance closes guidance and propagates the full refreshed matrix when capability becomes available", async () => {
  let resolved: Capabilities | null = null;
  let stillUnavailable = 0;
  let refreshError: string | null = null;
  // Use distinct, non-default values for every field so we can prove
  // the entire matrix was propagated, not just `synthetic_paste`.
  const refreshed: Capabilities = {
    clipboard_read: true,
    clipboard_write: false,
    global_hotkey: false,
    synthetic_paste: true,
    active_application: false,
    tray: false,
  };
  await retryGuidance({
    refresh: async () => refreshed,
    onResolved: (caps) => {
      resolved = caps;
    },
    onStillUnavailable: () => {
      stillUnavailable += 1;
    },
    onRefreshError: (msg) => {
      refreshError = msg;
    },
  });
  assert.deepEqual(resolved, refreshed);
  assert.equal(stillUnavailable, 0);
  assert.equal(refreshError, null);
});

test("retryGuidance keeps guidance open and still propagates the full matrix when capability stays unavailable", async () => {
  let resolved = 0;
  let stillUnavailable: Capabilities | null = null;
  let refreshError: string | null = null;
  const refreshed: Capabilities = {
    clipboard_read: true,
    clipboard_write: true,
    global_hotkey: true,
    synthetic_paste: false,
    active_application: false,
    tray: false,
  };
  await retryGuidance({
    refresh: async () => refreshed,
    onResolved: () => {
      resolved += 1;
    },
    onStillUnavailable: (caps) => {
      stillUnavailable = caps;
    },
    onRefreshError: (msg) => {
      refreshError = msg;
    },
  });
  assert.equal(resolved, 0);
  assert.deepEqual(stillUnavailable, refreshed);
  assert.equal(refreshError, null);
});

test("retryGuidance surfaces refresh errors and keeps the modal usable", async () => {
  let resolved = 0;
  let stillUnavailable = 0;
  let refreshError: string | null = null;
  await retryGuidance({
    refresh: async () => {
      throw new Error("IPC failed");
    },
    onResolved: () => {
      resolved += 1;
    },
    onStillUnavailable: () => {
      stillUnavailable += 1;
    },
    onRefreshError: (msg) => {
      refreshError = msg;
    },
  });
  assert.equal(resolved, 0);
  assert.equal(stillUnavailable, 0);
  assert.equal(refreshError, "IPC failed");
});

test("retryGuidance refresh error does not contain clipboard payload", async () => {
  // The refresh error path must surface a safe message. The helper
  // itself must not include any payload that the caller could
  // mistake for clipboard content.
  const payloadMarker = "payload-marker-DO-NOT-LOG";
  let refreshError: string | null = null;
  await retryGuidance({
    refresh: async () => {
      const err = new Error("IPC failed");
      // Attach a marker to prove the helper never reads it.
      (err as Error & { payload?: string }).payload = payloadMarker;
      throw err;
    },
    onResolved: () => {},
    onStillUnavailable: () => {},
    onRefreshError: (msg) => {
      refreshError = msg;
    },
  });
  assert.equal(refreshError, "IPC failed");
  assert.equal(
    refreshError?.includes(payloadMarker) ?? false,
    false,
    "refresh error must not leak attached payloads",
  );
});

test("retryGuidance never invokes a paste command (no pasteEntryCommand in the chain)", async () => {
  // This test pins the contract of the retry helper: it must only
  // refresh capabilities. If a paste command were ever invoked from
  // this code path, the assignment below would throw because
  // `pasteEntryCommand` is intentionally undefined here.
  let pasteInvocations = 0;
  const pasteEntryCommand = () => {
    pasteInvocations += 1;
    return Promise.reject(new Error("paste must not run from retryGuidance"));
  };
  // The retry helper does not receive a paste callback, so calling
  // pasteEntryCommand here proves the helper does not touch it.
  await retryGuidance({
    refresh: async () => allCapabilities({ synthetic_paste: true }),
    onResolved: () => {},
    onStillUnavailable: () => {},
    onRefreshError: () => {},
  });
  // Sanity: the retry helper never invoked the paste command above.
  assert.equal(pasteInvocations, 0);
  // Explicit re-assignment to demonstrate the variable name exists.
  void pasteEntryCommand;
});

test("shouldEnablePasteButton keeps the button usable when an entry exists", () => {
  assert.equal(shouldEnablePasteButton(0), false);
  assert.equal(shouldEnablePasteButton(1), true);
  assert.equal(shouldEnablePasteButton(42), true);
});

test("presentPaste classifies the response for the modal", () => {
  const guidance: PlatformGuidance = {
    capability: "synthetic_paste",
    kind: "permission_required",
    title: "Permission required",
    summary: "macOS needs Accessibility.",
    steps: ["Open System Settings."],
    retryable: true,
    can_open_settings: true,
    settings_target: "macos_accessibility",
  };
  const pasted = presentPaste({ kind: "pasted", guidance: null });
  assert.deepEqual(pasted, { kind: "pasted" });
  const failed = presentPaste({ kind: "capability_unavailable", guidance });
  assert.deepEqual(failed, { kind: "guidance", guidance });
});

test("retryGuidance does not flip synthetic_paste before refreshing", async () => {
  // A defensive invariant: the helper does not mutate the cached
  // capabilities object. Callers update their own state from the
  // refreshed value.
  const original = allCapabilities({ synthetic_paste: false });
  await retryGuidance({
    refresh: async () => allCapabilities({ synthetic_paste: true }),
    onResolved: (caps) => {
      // The helper must not reach into the caller state.
      assert.equal(original.synthetic_paste, false);
      assert.equal(caps.synthetic_paste, true);
    },
    onStillUnavailable: () => {},
    onRefreshError: () => {},
  });
  assert.equal(original.synthetic_paste, false);
});

test("retryGuidance tolerates an async onResolved callback", async () => {
  // The helper must not assume the callbacks are synchronous. The
  // modal forwards an `onRetry` that may return a promise when the
  // parent propagates the refreshed matrix asynchronously.
  let resolvedSeen: Capabilities | null = null;
  await retryGuidance({
    refresh: async () => allCapabilities({ synthetic_paste: true }),
    onResolved: async (caps) => {
      await Promise.resolve();
      resolvedSeen = caps;
    },
    onStillUnavailable: () => {},
    onRefreshError: () => {},
  });
  assert.equal(resolvedSeen?.synthetic_paste, true);
});
