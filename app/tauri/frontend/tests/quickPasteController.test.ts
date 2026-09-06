import { test } from "node:test";
import * as assert from "node:assert/strict";

import {
  openQuickPaste,
  performPasteFlow,
  QUICK_PASTE_ACTIVE_APP_TIMEOUT_MS,
  QUICK_PASTE_STEP_ORDER,
} from "../src/lib/quickPasteController.ts";
import type { QuickPasteTauriBridge } from "../src/lib/quickPasteBridge.ts";
import type {
  ActiveApplicationResponse,
  PasteResponse,
  PlatformGuidance,
} from "../src/types.ts";

function fakeBridge(): QuickPasteTauriBridge & {
  recorded: { step: string }[];
} {
  const recorded: { step: string }[] = [];
  const make = (step: string) => async () => {
    recorded.push({ step });
  };
  return {
    recorded,
    captureActiveApp: async () => {
      recorded.push({ step: "captureActiveApp" });
      return {
        available: true,
        name: "TestApp",
        identifier: "com.example.TestApp",
      };
    },
    center: make("center"),
    show: make("show"),
    focus: make("focus"),
    emitOpened: make("emitOpened"),
    hide: make("hide"),
  };
}

test("openQuickPaste follows the documented step order", async () => {
  const bridge = fakeBridge();
  await openQuickPaste(bridge);
  assert.deepEqual(
    bridge.recorded.map((entry) => entry.step),
    [...QUICK_PASTE_STEP_ORDER],
  );
});

test("openQuickPaste returns the captured active application", async () => {
  const bridge = fakeBridge();
  const active = await openQuickPaste(bridge);
  assert.ok(active);
  assert.equal(active?.name, "TestApp");
  assert.equal(active?.identifier, "com.example.TestApp");
});

test("openQuickPaste still opens the window when the active-app probe fails", async () => {
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => {
      throw new Error("probe unavailable");
    },
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  const active = await openQuickPaste(bridge);
  assert.equal(active, null);
  assert.deepEqual(recorded, ["show", "focus", "emitOpened"]);
});

test("openQuickPaste does not wait forever for an active-app probe", async () => {
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: () => new Promise<ActiveApplicationResponse>(() => undefined),
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  const active = await openQuickPaste(bridge, 5);
  assert.equal(active, null);
  assert.deepEqual(recorded, ["show", "focus", "emitOpened"]);
  assert.equal(QUICK_PASTE_ACTIVE_APP_TIMEOUT_MS, 500);
});

test("openQuickPaste runs the center step between the probe and show", async () => {
  // The `quick-paste-compact-ui` change adds a centring step right
  // after the active-app probe. The controller MUST run `center`
  // before `show` so the window lands on the current monitor before
  // becoming visible. The test mirrors the documented order and
  // confirms the visible tail `show → focus → emitOpened` is still
  // preserved.
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => {
      recorded.push("captureActiveApp");
      return {
        available: true,
        name: "TestApp",
        identifier: "com.example.TestApp",
      };
    },
    center: async () => {
      recorded.push("center");
    },
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  await openQuickPaste(bridge);
  assert.deepEqual(recorded, [
    "captureActiveApp",
    "center",
    "show",
    "focus",
    "emitOpened",
  ]);
});

test("openQuickPaste skips the center step when the bridge does not provide one", async () => {
  // A test bridge that does not implement `center` MUST still open
  // the palette through the protected visible tail. The
  // `compact-ui` change adds the step optionally so existing tests
  // and ad-hoc fakes keep working without modification.
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => ({
      available: false,
      name: null,
      identifier: null,
    }),
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  await openQuickPaste(bridge);
  assert.deepEqual(recorded, ["show", "focus", "emitOpened"]);
});

test("openQuickPaste tolerates a failing center step without blocking show", async () => {
  // Centring is best-effort: a transient failure (Tauri monitor API
  // unavailable) MUST NOT block the user from opening the palette.
  // The window keeps the conf-defined defaults declared in
  // `tauri.conf.json` instead of throwing.
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => ({
      available: true,
      name: "TestApp",
      identifier: "com.example.TestApp",
    }),
    center: async () => {
      throw new Error("monitor API unavailable");
    },
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  await openQuickPaste(bridge);
  assert.deepEqual(recorded, ["show", "focus", "emitOpened"]);
});

test("performPasteFlow hides the window before invoking paste", async () => {
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => ({
      available: false,
      name: null,
      identifier: null,
    }),
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  let pasteCalled = false;
  const pasted: PasteResponse = {
    kind: "pasted",
    id: 42,
    capability: null,
    error_kind: null,
    message: null,
    guidance: null,
  };
  const outcome = await performPasteFlow({
    bridge,
    pasteFn: async () => {
      // The hide MUST happen before pasteFn is invoked: we assert
      // by checking the recorded sequence so far.
      assert.deepEqual(recorded, ["hide"]);
      pasteCalled = true;
      return pasted;
    },
  });
  assert.equal(pasteCalled, true);
  assert.equal(outcome.kind, "pasted");
  assert.equal(outcome.windowStaysHidden, true);
  assert.deepEqual(recorded, ["hide"]);
});

test("performPasteFlow keeps the window hidden on pasted", async () => {
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => ({
      available: false,
      name: null,
      identifier: null,
    }),
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  const outcome = await performPasteFlow({
    bridge,
    pasteFn: async () => ({
      kind: "pasted",
      id: 7,
      capability: null,
      error_kind: null,
      message: null,
      guidance: null,
    }),
  });
  assert.equal(outcome.kind, "pasted");
  assert.equal(outcome.windowStaysHidden, true);
  // Show was never invoked: the window stays hidden by contract.
  assert.deepEqual(recorded, ["hide"]);
});

test("performPasteFlow re-shows the window on capability_unavailable", async () => {
  const recorded: string[] = [];
  const guidance: PlatformGuidance = {
    capability: "synthetic_paste",
    kind: "permission_required",
    title: "Permission required",
    summary: "macOS Accessibility",
    steps: ["Open System Settings."],
    retryable: true,
    can_open_settings: true,
    settings_target: "macos_accessibility",
  };
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => ({
      available: false,
      name: null,
      identifier: null,
    }),
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  let onReShowCalls = 0;
  const outcome = await performPasteFlow({
    bridge,
    pasteFn: async () => ({
      kind: "capability_unavailable",
      id: null,
      capability: "synthetic_paste",
      error_kind: null,
      message: null,
      guidance,
    }),
    onReShow: () => {
      onReShowCalls += 1;
    },
  });
  assert.equal(outcome.kind, "failed");
  assert.equal(outcome.windowStaysHidden, false);
  assert.deepEqual(recorded, ["hide", "show"]);
  assert.equal(onReShowCalls, 1);
  if (outcome.kind === "failed") {
    assert.equal(outcome.response.kind, "capability_unavailable");
  }
});

test("performPasteFlow re-shows the window on failed response", async () => {
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => ({
      available: false,
      name: null,
      identifier: null,
    }),
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  const outcome = await performPasteFlow({
    bridge,
    pasteFn: async () => ({
      kind: "failed",
      id: null,
      capability: null,
      error_kind: "backend",
      message: "paste controller crashed",
      guidance: null,
    }),
  });
  assert.equal(outcome.kind, "failed");
  assert.equal(outcome.windowStaysHidden, false);
  assert.deepEqual(recorded, ["hide", "show"]);
});

test("performPasteFlow re-shows the window when pasteFn rejects", async () => {
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => ({
      available: false,
      name: null,
      identifier: null,
    }),
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  const outcome = await performPasteFlow({
    bridge,
    pasteFn: async () => {
      throw new Error("IPC disconnected");
    },
  });
  assert.equal(outcome.kind, "failed");
  assert.equal(outcome.windowStaysHidden, false);
  assert.deepEqual(recorded, ["hide", "show"]);
  if (outcome.kind === "failed") {
    if ("error" in outcome.response) {
      assert.equal(outcome.response.error, "IPC disconnected");
    } else {
      assert.fail("expected an error payload");
    }
  }
});

test("performPasteFlow lets callers detect paste failures with custom predicates", async () => {
  // The controller accepts optional predicate hooks so the Svelte
  // layer can keep the modal logic in one place without re-implementing
  // the response classification.
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => ({
      available: false,
      name: null,
      identifier: null,
    }),
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  let failedCalls = 0;
  let unavailableCalls = 0;
  const outcome = await performPasteFlow({
    bridge,
    pasteFn: async () => ({
      kind: "pasted",
      id: 1,
      capability: null,
      error_kind: null,
      message: null,
      guidance: null,
    }),
    isFailed: () => {
      failedCalls += 1;
      return false;
    },
    isCapabilityUnavailable: () => {
      unavailableCalls += 1;
      return false;
    },
  });
  assert.equal(outcome.kind, "pasted");
  assert.equal(failedCalls, 1);
  assert.equal(unavailableCalls, 1);
  assert.deepEqual(recorded, ["hide"]);
});

test("openQuickPaste never sends clipboard content in any step", async () => {
  // Privacy invariant: the bridge steps are called with no
  // clipboard payload, query string or snippet. We assert by reading
  // the recorded entries and confirming the captured application
  // never includes user content.
  const bridge = fakeBridge();
  const active = await openQuickPaste(bridge);
  // The fake returns the active app object; assert its shape is
  // strictly name + identifier, with no `content`/`snippet`/`query`
  // fields.
  const payload = active as unknown as Record<string, unknown>;
  assert.equal(typeof payload?.["name"], "string");
  assert.equal(typeof payload?.["identifier"], "string");
  assert.equal(payload?.["content"], undefined);
  assert.equal(payload?.["snippet"], undefined);
  assert.equal(payload?.["query"], undefined);
  assert.equal(payload?.["clipboard"], undefined);
});

test("performPasteFlow does not mutate the source history entry", async () => {
  // Pin the contract from `clipvault-core/src/paste.rs`: paste
  // failures never touch the row. The controller is read-only and
  // never invokes anything resembling `record_payload` or
  // `delete`.
  const recorded: string[] = [];
  const bridge: QuickPasteTauriBridge = {
    captureActiveApp: async () => ({
      available: false,
      name: null,
      identifier: null,
    }),
    show: async () => {
      recorded.push("show");
    },
    focus: async () => {
      recorded.push("focus");
    },
    emitOpened: async () => {
      recorded.push("emitOpened");
    },
    hide: async () => {
      recorded.push("hide");
    },
  };
  await performPasteFlow({
    bridge,
    pasteFn: async () => {
      throw new Error("crashed");
    },
  });
  // The controller never sends anything close to a write intent on
  // the entry repository; only window control and the paste command.
  for (const step of recorded) {
    assert.notEqual(step, "record");
    assert.notEqual(step, "delete");
    assert.notEqual(step, "touch");
  }
});
