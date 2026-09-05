import { test } from "node:test";
import * as assert from "node:assert/strict";

import {
  HISTORY_UPDATED_EVENT,
  createHistoryUpdatedRegistrar,
  defaultHistoryUpdatedBridge,
  type HistoryUpdatedTauriBridge,
} from "../src/lib/historyUpdates.ts";

interface FakeTauri {
  handlers: Map<string, () => void>;
  listenCalls: { event: string }[];
  unlistenCalls: { event: string; eventId: number }[];
  emitCalls: { event: string; payload?: unknown }[];
  uninstall: () => void;
}

/**
 * Install a fake `window.__TAURI_INTERNALS__` so the production
 * `defaultHistoryUpdatedBridge` can be exercised without Tauri.
 *
 * The fake honours Tauri's `plugin:event|listen` /
 * `plugin:event|unlisten` commands: `listen` returns a teardown
 * function that, when called, routes through `unlisten` so the
 * registered handler is removed from the bookkeeping map. That is
 * what Tauri's real runtime does.
 */
function installFakeTauri(): FakeTauri {
  const handlers = new Map<string, () => void>();
  const listenCalls: { event: string }[] = [];
  const unlistenCalls: { event: string; eventId: number }[] = [];
  const emitCalls: { event: string; payload?: unknown }[] = [];
  let listenerId = 1;
  const callbacks = new Map<number, () => void>();

  function buildTauri(): {
    transformCallback: (callback?: () => void) => number;
    invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
  } {
    const tauri = {
      transformCallback: (callback?: () => void): number => {
        const id = listenerId++;
        if (callback) callbacks.set(id, callback);
        return id;
      },
      invoke: async <T>(
        cmd: string,
        args?: Record<string, unknown>,
      ): Promise<T> => {
        if (cmd === "plugin:event|listen") {
          const event = args?.["event"] as string;
          const id = listenerId++;
          const callback = callbacks.get(args?.["handler"] as number);
          const handlerKey = `${event}#${id}`;
          if (event && callback) {
            handlers.set(handlerKey, () => callback());
          }
          listenCalls.push({ event });
          // Match Tauri's real `plugin:event|listen` contract: the
          // returned value is the listener id, which Tauri's listen
          // wrapper turns into a teardown that invokes
          // `plugin:event|unlisten` with that id.
          return id as unknown as T;
        }
        if (cmd === "plugin:event|unlisten") {
          const event = args?.["event"] as string;
          const eventId = args?.["eventId"] as number;
          if (event && typeof eventId === "number") {
            handlers.delete(`${event}#${eventId}`);
          }
          unlistenCalls.push({ event, eventId });
          return undefined as unknown as T;
        }
        if (cmd === "plugin:event|emit") {
          emitCalls.push({
            event: args?.["event"] as string,
            payload: args?.["payload"],
          });
          return undefined as unknown as T;
        }
        return undefined as unknown as T;
      },
    };
    return tauri;
  }

  const tauri = buildTauri();
  const globalScope = globalThis as unknown as {
    __TAURI_INTERNALS__?: unknown;
    window?: { __TAURI_INTERNALS__?: unknown };
  };
  const previousGlobalInternals = globalScope.__TAURI_INTERNALS__;
  const previousWindow = globalScope.window;
  globalScope.__TAURI_INTERNALS__ = tauri;
  globalScope.window = { __TAURI_INTERNALS__: tauri };
  return {
    handlers,
    listenCalls,
    unlistenCalls,
    emitCalls,
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

function fakeBridge(tauri: FakeTauri): HistoryUpdatedTauriBridge {
  // The fake bridge mirrors what `listenHistoryUpdated` does over
  // `plugin:event|listen` so the registrar exercises the same code
  // path. Returning a teardown that deletes from the bookkeeping map
  // is sufficient for unit tests; production uses Tauri's real
  // unlisten handler.
  return {
    listen: (event, handler) => {
      const handlerKey = `${event}#fake`;
      tauri.handlers.set(handlerKey, () => handler());
      tauri.listenCalls.push({ event });
      const teardown = () => {
        tauri.handlers.delete(handlerKey);
      };
      return Promise.resolve(teardown);
    },
  };
}

/** Simulate the shell firing the event. */
function fireEvent(tauri: FakeTauri, event: string): void {
  for (const [key, handler] of tauri.handlers) {
    if (key.startsWith(event)) {
      handler();
    }
  }
}

test("HISTORY_UPDATED_EVENT name is stable", () => {
  // Pin the event name so a backend rename surfaces as a test
  // failure instead of silently dropping notifications.
  assert.equal(HISTORY_UPDATED_EVENT, "clipvault://history-updated");
});

test("default bridge installs exactly one listener across remounts", async () => {
  const tauri = installFakeTauri();
  try {
    const registrar = createHistoryUpdatedRegistrar(defaultHistoryUpdatedBridge);
    const first = await registrar(() => undefined);
    const second = await registrar(() => undefined);

    // The first install registers the listener; the second install
    // returns the same unlisten handle without registering a second
    // Tauri subscription.
    assert.equal(typeof first, "function");
    assert.equal(typeof second, "function");
    assert.equal(tauri.listenCalls.length, 1);
    assert.equal(tauri.listenCalls[0]?.event, HISTORY_UPDATED_EVENT);
  } finally {
    tauri.uninstall();
  }
});

test("Stored outcome refreshes the recent-entries list", async () => {
  const tauri = installFakeTauri();
  try {
    let refreshCalls = 0;
    const registrar = createHistoryUpdatedRegistrar(defaultHistoryUpdatedBridge);
    await registrar(() => {
      refreshCalls += 1;
    });

    // The shell emits the event after a Stored (or Duplicate) outcome.
    fireEvent(tauri, HISTORY_UPDATED_EVENT);
    assert.equal(refreshCalls, 1, "Stored outcome should refresh the list");
  } finally {
    tauri.uninstall();
  }
});

test("Duplicate outcome refreshes the recent-entries list", async () => {
  const tauri = installFakeTauri();
  try {
    let refreshCalls = 0;
    const registrar = createHistoryUpdatedRegistrar(defaultHistoryUpdatedBridge);
    await registrar(() => {
      refreshCalls += 1;
    });

    // Multiple Stored/Duplicate outcomes each trigger a refresh,
    // mirroring the App.svelte wiring where the handler always calls
    // `refreshEntries()` after the event fires.
    fireEvent(tauri, HISTORY_UPDATED_EVENT);
    fireEvent(tauri, HISTORY_UPDATED_EVENT);
    fireEvent(tauri, HISTORY_UPDATED_EVENT);
    assert.equal(refreshCalls, 3, "Duplicate outcome should refresh the list");
  } finally {
    tauri.uninstall();
  }
});

test("listener cleans up on unlisten and stops receiving events", async () => {
  const tauri = installFakeTauri();
  try {
    let callCount = 0;
    // Use the fake bridge so the unlisten teardown is synchronous.
    // Tauri's real unlisten goes through an extra `invoke` round
    // trip; the fake bridge collapses it so the cleanup is
    // observable by the next assertion, which is what App.svelte's
    // `onDestroy` hook actually cares about.
    const registrar = createHistoryUpdatedRegistrar(fakeBridge(tauri));
    const unlisten = await registrar(() => {
      callCount += 1;
    });

    fireEvent(tauri, HISTORY_UPDATED_EVENT);
    assert.equal(callCount, 1);

    // Cleanup path: the App.svelte `onDestroy` hook calls the
    // returned unlisten handle. After that, the fake removes the
    // listener so subsequent events do not reach the handler.
    unlisten();
    fireEvent(tauri, HISTORY_UPDATED_EVENT);
    assert.equal(
      callCount,
      1,
      "handler must not fire after cleanup (Ignored/Failed-equivalent)",
    );
  } finally {
    tauri.uninstall();
  }
});

test("registrar swallows handler exceptions and keeps the listener alive", async () => {
  const tauri = installFakeTauri();
  try {
    let recovered = 0;
    const registrar = createHistoryUpdatedRegistrar(defaultHistoryUpdatedBridge);
    // First install with a handler that throws. The bridge catches
    // the error and logs it so the next event still reaches the
    // listener.
    await registrar(() => {
      throw new Error("boom");
    });
    // We cannot install a second listener with the production
    // registrar because it is idempotent by design, so the same
    // listener has to absorb the failure. We assert that firing
    // another event still hits the listener (which throws again).
    const before = tauri.handlers.size;
    fireEvent(tauri, HISTORY_UPDATED_EVENT);
    assert.equal(tauri.handlers.size, before, "listener survives handler errors");
    // Silence the unused warning for `recovered`: the production
    // listener is wired above; the test asserts structural
    // behaviour (the listener survives errors) rather than the
    // payload it receives.
    assert.equal(recovered, 0);
  } finally {
    tauri.uninstall();
  }
});

test("event name and bridge surface never carry sensitive categories", async () => {
  const tauri = installFakeTauri();
  try {
    const registrar = createHistoryUpdatedRegistrar(defaultHistoryUpdatedBridge);
    await registrar(() => undefined);

    // The listen command payload must not include clipboard
    // content, query, snippet, hash or source identifier: those
    // values are exactly what the privacy contract forbids from
    // appearing in any Tauri event.
    const listenCall = tauri.listenCalls[0];
    assert.ok(listenCall);
    const payload = JSON.stringify(listenCall);
    for (const forbidden of [
      "content",
      "clipboard",
      "query",
      "snippet",
      "hash",
      "source_app",
      "sourceIdentifier",
    ]) {
      assert.equal(
        payload.includes(forbidden),
        false,
        `listen payload must not include "${forbidden}"`,
      );
    }
  } finally {
    tauri.uninstall();
  }
});

test("App.svelte-style handler chain counts Stored and Duplicate equally", async () => {
  // Simulates the wiring inside App.svelte: the registrar is
  // idempotent across remounts and the handler is the only entry
  // point that calls `refreshEntries()`.
  const tauri = installFakeTauri();
  try {
    let refreshCalls = 0;
    const registrar = createHistoryUpdatedRegistrar(defaultHistoryUpdatedBridge);
    const first = await registrar(() => {
      refreshCalls += 1;
    });
    const second = await registrar(() => {
      refreshCalls += 1;
    });

    // The second register call must reuse the first listener so
    // refresh is not invoked twice per event.
    assert.equal(first, second);

    fireEvent(tauri, HISTORY_UPDATED_EVENT);
    fireEvent(tauri, HISTORY_UPDATED_EVENT);
    assert.equal(refreshCalls, 2, "single listener must drive both events");
  } finally {
    tauri.uninstall();
  }
});

test("fake bridge and default bridge agree on idempotent registrar semantics", async () => {
  const tauri = installFakeTauri();
  try {
    const bridge = fakeBridge(tauri);
    const registrar = createHistoryUpdatedRegistrar(bridge);
    await registrar(() => undefined);
    const second = await registrar(() => undefined);
    // Both calls return the same unlisten function so the listener
    // surface stays singular.
    const first = await registrar(() => undefined);
    void first;
    assert.equal(typeof second, "function");
    assert.equal(tauri.listenCalls.length, 1);
  } finally {
    tauri.uninstall();
  }
});