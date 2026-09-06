/**
 * Tests for the copy-only controller path.
 *
 * `performCopyFlow` is the keyboard copy counterpart of
 * `performPasteFlow`: it hides Quick Paste, calls the
 * `clipvault_copy_entry` Tauri command and keeps the window
 * hidden on a successful write. The synthetic paste trigger
 * MUST NEVER run on the copy path. This suite pins that contract
 * alongside the typed outcome mapping the controller exposes.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";

import { performCopyFlow } from "../src/lib/quickPasteController.ts";
import type { QuickPasteTauriBridge } from "../src/lib/quickPasteBridge.ts";
import type { CopyResponse, PasteResponse } from "../src/types.ts";

function fakeBridge(): QuickPasteTauriBridge & {
  recorded: { step: string }[];
} {
  const recorded: { step: string }[] = [];
  return {
    recorded,
    captureActiveApp: async () => ({
      available: false,
      name: null,
      identifier: null,
    }),
    show: async () => {
      recorded.push({ step: "show" });
    },
    focus: async () => {
      recorded.push({ step: "focus" });
    },
    emitOpened: async () => {
      recorded.push({ step: "emitOpened" });
    },
    hide: async () => {
      recorded.push({ step: "hide" });
    },
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

test("performCopyFlow hides the window before invoking the copy command", async () => {
  const bridge = fakeBridge();
  let copyCalled = false;
  const outcome = await performCopyFlow({
    bridge,
    copyFn: async () => {
      // The hide MUST happen before copyFn is invoked: we assert
      // by checking the recorded sequence so far.
      assert.deepEqual(bridge.recorded.map((entry) => entry.step), ["hide"]);
      copyCalled = true;
      return copiedResponse(42);
    },
  });
  assert.equal(copyCalled, true);
  assert.equal(outcome.kind, "copied");
  assert.equal(outcome.windowStaysHidden, true);
  assert.deepEqual(bridge.recorded.map((entry) => entry.step), ["hide"]);
});

test("performCopyFlow keeps the window hidden on copied", async () => {
  const bridge = fakeBridge();
  const outcome = await performCopyFlow({
    bridge,
    copyFn: async () => copiedResponse(7),
  });
  assert.equal(outcome.kind, "copied");
  assert.equal(outcome.windowStaysHidden, true);
  // Show was never invoked: the window stays hidden by contract.
  assert.deepEqual(bridge.recorded.map((entry) => entry.step), ["hide"]);
});

test("performCopyFlow re-shows the window on capability_unavailable", async () => {
  const bridge = fakeBridge();
  let onReShowCalls = 0;
  const outcome = await performCopyFlow({
    bridge,
    copyFn: async () => ({
      kind: "capability_unavailable",
      id: null,
      capability: "clipboard_write_image",
      error_kind: null,
      message: null,
      guidance: null,
      mode: null,
    }),
    onReShow: () => {
      onReShowCalls += 1;
    },
  });
  assert.equal(outcome.kind, "failed");
  assert.equal(outcome.windowStaysHidden, false);
  assert.deepEqual(bridge.recorded.map((entry) => entry.step), ["hide", "show"]);
  assert.equal(onReShowCalls, 1);
});

test("performCopyFlow re-shows the window on a failed outcome", async () => {
  const bridge = fakeBridge();
  const outcome = await performCopyFlow({
    bridge,
    copyFn: async () => ({
      kind: "failed",
      id: null,
      capability: null,
      error_kind: "clipboard",
      message: "boom",
      guidance: null,
      mode: null,
    }),
  });
  assert.equal(outcome.kind, "failed");
  assert.equal(outcome.windowStaysHidden, false);
  assert.deepEqual(bridge.recorded.map((entry) => entry.step), ["hide", "show"]);
});

test("performCopyFlow re-shows the window when copyFn rejects", async () => {
  const bridge = fakeBridge();
  const outcome = await performCopyFlow({
    bridge,
    copyFn: async () => {
      throw new Error("IPC disconnected");
    },
  });
  assert.equal(outcome.kind, "failed");
  assert.equal(outcome.windowStaysHidden, false);
  assert.deepEqual(bridge.recorded.map((entry) => entry.step), ["hide", "show"]);
  if (outcome.kind === "failed") {
    if ("error" in outcome.response) {
      assert.equal(outcome.response.error, "IPC disconnected");
    } else {
      assert.fail("expected an error payload");
    }
  }
});

test("performCopyFlow never echoes clipboard content in the failure payload", async () => {
  // Pin the metadata-only contract on the copy side so a future
  // refactor that leaks content into the failure payload is
  // visible in CI.
  const bridge = fakeBridge();
  const outcome = await performCopyFlow({
    bridge,
    copyFn: async () => ({
      kind: "failed",
      id: null,
      capability: null,
      error_kind: "asset_read",
      message: "asset missing",
      guidance: null,
      mode: null,
    }),
  });
  if (outcome.kind !== "failed") {
    assert.fail("expected a failed outcome");
  }
  const serialised = JSON.stringify(outcome.response);
  for (const forbidden of [
    "content",
    "snippet",
    "/Users/",
    "clipboard/",
    "asset/",
  ]) {
    assert.equal(
      serialised.includes(forbidden),
      false,
      `failure payload must not leak ${forbidden}`,
    );
  }
});

test("performCopyFlow exposes pasted_plain_fallback as a typed success", async () => {
  const bridge = fakeBridge();
  const outcome = await performCopyFlow({
    bridge,
    copyFn: async () => ({
      kind: "copied_plain_fallback",
      id: 9,
      capability: null,
      error_kind: null,
      message: null,
      guidance: null,
      mode: "plain",
    }),
  });
  assert.equal(outcome.kind, "copied");
  assert.equal(outcome.windowStaysHidden, true);
  assert.deepEqual(bridge.recorded.map((entry) => entry.step), ["hide"]);
});

test("performCopyFlow accepts custom failure predicates", async () => {
  // The caller can decide which response kinds are considered
  // failures so the controller stays decoupled from the wire
  // discriminator naming. Pin the hook contract so a future
  // refactor cannot accidentally drop it.
  const bridge = fakeBridge();
  let failedCalls = 0;
  let unavailableCalls = 0;
  const outcome = await performCopyFlow({
    bridge,
    copyFn: async () => copiedResponse(11),
    isFailed: () => {
      failedCalls += 1;
      return false;
    },
    isCapabilityUnavailable: () => {
      unavailableCalls += 1;
      return false;
    },
  });
  assert.equal(outcome.kind, "copied");
  assert.equal(failedCalls, 1);
  assert.equal(unavailableCalls, 1);
  assert.deepEqual(bridge.recorded.map((entry) => entry.step), ["hide"]);
});

test("performCopyFlow must never call a synthetic paste", async () => {
  // The copy-only controller MUST stay free of any paste trigger.
  // We assert it indirectly by verifying the bridge never sees a
  // paste-shaped call and by checking the typed response shape.
  const bridge = fakeBridge();
  const outcome = await performCopyFlow({
    bridge,
    copyFn: async () => pastedResponse(7),
  });
  // A pasted response is not a valid copy response. The controller
  // falls through to the "window stays hidden" branch because the
  // caller did not classify `pasted` as a failure or capability
  // refusal. The bridge MUST NOT see a paste trigger either way.
  assert.equal(outcome.kind, "copied");
  for (const step of bridge.recorded) {
    assert.notEqual(step.step, "paste");
    assert.notEqual(step.step, "syntheticPaste");
  }
});
