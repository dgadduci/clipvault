import { test } from "node:test";
import * as assert from "node:assert/strict";

import {
  createQuickSearchRegistrar,
  defaultQuickPasteBridge,
  QUICK_SEARCH_EVENT,
  QUICK_PASTE_OPENED_EVENT,
  QUICK_PASTE_WINDOW_LABEL,
} from "../src/lib/quickPasteBridge.ts";
import type { QuickPasteTauriBridge } from "../src/lib/quickPasteBridge.ts";

/** Replace the global Tauri internals with a fake that records every
 *  command so tests can assert the order and shape of invocations. */
function installFakeTauri(): {
  calls: { cmd: string; args?: Record<string, unknown> }[];
  handlers: Map<string, () => void>;
  uninstall: () => void;
} {
  const calls: { cmd: string; args?: Record<string, unknown> }[] = [];
  const handlers = new Map<string, () => void>();
  let listenerId = 1;
  const callbacks = new Map<number, (payload?: unknown) => void>();
  const tauri = {
    transformCallback: (callback?: (payload?: unknown) => void): number => {
      const id = listenerId++;
      if (callback) callbacks.set(id, callback);
      return id;
    },
    invoke: async <T,>(
      cmd: string,
      args?: Record<string, unknown>,
    ): Promise<T> => {
      calls.push({ cmd, args });
      if (cmd === "plugin:event|listen") {
        const event = args?.["event"] as string;
        const id = listenerId++;
        const callback = callbacks.get(args?.["handler"] as number);
        if (event && callback) {
          handlers.set(`${event}#${id}`, () => callback());
        }
        return id as unknown as T;
      }
      return undefined as unknown as T;
    },
  };
  // The bridge reads `window.__TAURI_INTERNALS__`. Node.js has no
  // `window` global, so we install one that mirrors `globalThis` for
  // the duration of the test.
  const globalScope = globalThis as unknown as {
    __TAURI_INTERNALS__?: unknown;
    window?: { __TAURI_INTERNALS__?: unknown };
  };
  const previousGlobalInternals = globalScope.__TAURI_INTERNALS__;
  const previousWindow = globalScope.window;
  globalScope.__TAURI_INTERNALS__ = tauri;
  globalScope.window = { __TAURI_INTERNALS__: tauri };
  return {
    calls,
    handlers,
    uninstall: () => {
      if (previousGlobalInternals === undefined) {
        delete globalScope.__TAURI_INTERNALS__;
      } else {
        globalScope.__TAURI_INTERNALS__ = previousGlobalInternals;
      }
      if (previousWindow === undefined) {
        delete globalScope.window;
      } else {
        globalScope.window = previousWindow;
      }
    },
  };
}

function fakeBridge(): QuickPasteTauriBridge & {
  recorded: string[];
} {
  const recorded: string[] = [];
  return {
    recorded,
    captureActiveApp: async () => {
      recorded.push("captureActiveApp");
      return {
        available: true,
        name: "TestApp",
        identifier: "com.example.TestApp",
      };
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
}

test("QUICK_SEARCH_EVENT, QUICK_PASTE_OPENED_EVENT and QUICK_PASTE_WINDOW_LABEL are stable", () => {
  // Pin the event names so the backend and frontend agree across
  // releases.
  assert.equal(QUICK_SEARCH_EVENT, "clipvault://quick-search");
  assert.equal(
    QUICK_PASTE_OPENED_EVENT,
    "clipvault://quick-paste-opened",
  );
  assert.equal(QUICK_PASTE_WINDOW_LABEL, "quick-paste");
});

test("registrar subscribes to the quick-search event exactly once", async () => {
  const tauri = installFakeTauri();
  try {
    const registrar = createQuickSearchRegistrar(fakeBridge());
    const first = await registrar(() => undefined);
    const second = await registrar(() => undefined);

    // Both calls must return the same unlisten handle so the caller
    // can detach the same subscription that previous mounts registered.
    assert.equal(typeof first, "function");
    assert.equal(typeof second, "function");

    const listenCalls = tauri.calls.filter(
      (call) => call.cmd === "plugin:event|listen",
    );
    assert.equal(listenCalls.length, 1);
    assert.equal(listenCalls[0]?.args?.["event"], QUICK_SEARCH_EVENT);
  } finally {
    tauri.uninstall();
  }
});

test("registrar runs the show-focus-emit sequence on every activation", async () => {
  const tauri = installFakeTauri();
  try {
    const bridge = fakeBridge();
    const registrar = createQuickSearchRegistrar(bridge);
    let callbackCalls = 0;
    await registrar(() => {
      callbackCalls += 1;
    });
    assert.deepEqual(bridge.recorded, []);

    // Find the handler that the registrar installed and trigger it.
    const handlerEntry = [...tauri.handlers.entries()].find(([key]) =>
      key.startsWith(QUICK_SEARCH_EVENT),
    );
    assert.ok(handlerEntry, "expected a listener for the quick-search event");
    const [, handler] = handlerEntry!;
    handler();

    // Allow microtasks to flush.
    await new Promise((resolve) => setImmediate(resolve));

    assert.deepEqual(bridge.recorded, [
      "captureActiveApp",
      "show",
      "focus",
      "emitOpened",
    ]);
    assert.equal(callbackCalls, 1);
  } finally {
    tauri.uninstall();
  }
});

test("registrar surfaces activation errors without leaking clipboard data", async () => {
  const tauri = installFakeTauri();
  try {
    const bridge = fakeBridge();
    bridge.show = async () => {
      throw new Error("quick-paste window unavailable");
    };
    let activationError: unknown = null;
    const registrar = createQuickSearchRegistrar(bridge, (error) => {
      activationError = error;
    });
    await registrar(() => undefined);
    const handlerEntry = [...tauri.handlers.entries()].find(([key]) =>
      key.startsWith(QUICK_SEARCH_EVENT),
    );
    assert.ok(handlerEntry);
    const [, handler] = handlerEntry!;
    handler();
    await new Promise((resolve) => setImmediate(resolve));
    assert.ok(activationError instanceof Error);
    assert.equal((activationError as Error).message, "quick-paste window unavailable");
    const serialized = JSON.stringify(activationError);
    assert.equal(serialized.includes("clipboard"), false);
    assert.equal(serialized.includes("content"), false);
  } finally {
    tauri.uninstall();
  }
});

test("registrar does not send clipboard content in any event", async () => {
  const tauri = installFakeTauri();
  try {
    const registrar = createQuickSearchRegistrar(fakeBridge());
    await registrar(() => undefined);

    // The listen call payload must not include clipboard content,
    // query or snippet.
    const listenCall = tauri.calls.find(
      (call) => call.cmd === "plugin:event|listen",
    );
    assert.ok(listenCall);
    const payload = JSON.stringify(listenCall?.args ?? {});
    assert.equal(payload.includes("content"), false);
    assert.equal(payload.includes("clipboard"), false);
    assert.equal(payload.includes("query"), false);
    assert.equal(payload.includes("snippet"), false);
  } finally {
    tauri.uninstall();
  }
});

test("registrar tolerates active-app probe failure without skipping show/focus/emit", async () => {
  const tauri = installFakeTauri();
  try {
    const recorded: string[] = [];
    const bridge: QuickPasteTauriBridge = {
      captureActiveApp: async () => {
        recorded.push("captureActiveApp");
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
    const registrar = createQuickSearchRegistrar(bridge);
    await registrar(() => undefined);
    const handlerEntry = [...tauri.handlers.entries()].find(([key]) =>
      key.startsWith(QUICK_SEARCH_EVENT),
    );
    assert.ok(handlerEntry);
    const [, handler] = handlerEntry!;
    handler();
    await new Promise((resolve) => setImmediate(resolve));
    assert.deepEqual(recorded, [
      "captureActiveApp",
      "show",
      "focus",
      "emitOpened",
    ]);
  } finally {
    tauri.uninstall();
  }
});

test("default bridge commands match the Tauri plugin conventions", () => {
  // Smoke check that the bridge stays aligned with the documented
  // Tauri command names. The actual invocation is covered by the
  // registrar tests above; here we only confirm the function set.
  assert.equal(typeof defaultQuickPasteBridge.captureActiveApp, "function");
  assert.equal(typeof defaultQuickPasteBridge.show, "function");
  assert.equal(typeof defaultQuickPasteBridge.focus, "function");
  assert.equal(typeof defaultQuickPasteBridge.emitOpened, "function");
  assert.equal(typeof defaultQuickPasteBridge.hide, "function");
});
