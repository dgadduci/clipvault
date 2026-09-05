import { test } from "node:test";
import * as assert from "node:assert/strict";

import { runSearch } from "../src/lib/search.ts";
import type { RunSearchController } from "../src/lib/search.ts";
import type { SearchResponse } from "../src/types.ts";

function response(note: string, ids: number[]): SearchResponse {
  return {
    note,
    hits: ids.map((id) => ({
      entry_id: id,
      snippet: `snippet-${id}`,
      score: 100 - id,
      record: {
        id,
        content: `content-${id}`,
        content_type: "text",
        content_size: 10,
        content_hash: `hash-${id}`,
        source_app: null,
        created_at: "2026-01-02T03:04:05Z",
        updated_at: "2026-01-02T03:04:05Z",
        last_seen_at: "2026-01-02T03:04:05Z",
      },
    })),
  };
}

interface FakeClock {
  now: () => number;
  pending: Array<{ id: number; cb: () => void; at: number }>;
  nextId: number;
}

function makeFakeClock(initial: number): {
  clock: FakeClock;
  setTimer: (cb: () => void, ms: number) => unknown;
  clearTimer: (handle: unknown) => void;
  advance: (ms: number) => void;
} {
  const clock: FakeClock = {
    now: () => initial,
    pending: [],
    nextId: 1,
  };
  const setTimer = (cb: () => void, ms: number) => {
    const id = clock.nextId++;
    clock.pending.push({ id, cb, at: clock.now() + ms });
    return id;
  };
  const clearTimer = (handle: unknown) => {
    const id = handle as number;
    clock.pending = clock.pending.filter((entry) => entry.id !== id);
  };
  const advance = (ms: number) => {
    initial += ms;
    clock.now = () => initial;
    const ready = clock.pending.filter((entry) => entry.at <= initial);
    clock.pending = clock.pending.filter((entry) => entry.at > initial);
    for (const entry of ready) {
      entry.cb();
    }
  };
  return { clock, setTimer, clearTimer, advance };
}

test("runSearch with debounceMs=0 invokes the backend once", async () => {
  let invocations = 0;
  let captured = "";
  const controller = runSearch({
    query: "hello",
    debounceMs: 0,
    invoke: async (q) => {
      invocations += 1;
      captured = q;
      return response("ok", [1]);
    },
    setTimer: () => assert.fail("setTimer must not be called without debounce"),
    clearTimer: () => {},
  });
  const result = await controller.result;
  assert.equal(invocations, 1);
  assert.equal(captured, "hello");
  assert.equal(result.note, "ok");
  assert.equal(result.hits.length, 1);
});

test("runSearch debounce cancels previous timers when caller cancels between calls", async () => {
  const fake = makeFakeClock(0);
  let invocations = 0;
  const invoke = async (q: string) => {
    invocations += 1;
    return response("ok", [q.length]);
  };
  // Simulate the Svelte caller pattern: cancel the previous controller
  // before starting a new one. This is the contract the helper
  // exposes: a `cancel` method that stops both pending and resolved
  // results.
  let active: RunSearchController | null = null;
  function trigger(query: string) {
    if (active) active.cancel();
    const c = runSearch({
      query,
      debounceMs: 50,
      invoke,
      setTimer: fake.setTimer,
      clearTimer: fake.clearTimer,
    });
    active = c;
    return c;
  }

  trigger("h");
  fake.advance(10);
  trigger("he");
  fake.advance(10);
  trigger("hello");
  // No invocation yet: timers have not fired.
  assert.equal(invocations, 0);
  fake.advance(50);
  // Only the last debounce window should have fired because the caller
  // cancelled each previous controller.
  assert.equal(invocations, 1);
  const last = await active!.result;
  assert.equal(last.hits.length, 1);
  assert.equal(last.hits[0].entry_id, "hello".length);
});

test("runSearch cancel stops a pending debounce from invoking the backend", async () => {
  const fake = makeFakeClock(0);
  let invocations = 0;
  const controller = runSearch({
    query: "hello",
    debounceMs: 50,
    invoke: async () => {
      invocations += 1;
      return response("ok", [1]);
    },
    setTimer: fake.setTimer,
    clearTimer: fake.clearTimer,
  });
  controller.cancel();
  fake.advance(200);
  assert.equal(invocations, 0);
});

test("runSearch propagates the note and hits returned by the backend", async () => {
  const controller = runSearch({
    query: "abc",
    debounceMs: 0,
    invoke: async () => response("ok", [7, 8]),
    setTimer: () => {},
    clearTimer: () => {},
  });
  const result = await controller.result;
  assert.equal(result.note, "ok");
  assert.deepEqual(
    result.hits.map((h) => h.entry_id),
    [7, 8],
  );
});

test("runSearch surfaces empty_query note when invoked with whitespace", async () => {
  // The Svelte view short-circuits whitespace-only queries itself; the
  // helper still returns whatever the backend says for transparency.
  const controller = runSearch({
    query: "   ",
    debounceMs: 0,
    invoke: async () => response("empty_query", []),
    setTimer: () => {},
    clearTimer: () => {},
  });
  const result = await controller.result;
  assert.equal(result.note, "empty_query");
  assert.equal(result.hits.length, 0);
});

test("runSearch rejects with the backend error when the invocation fails", async () => {
  // Pin the new contract: errors propagate instead of being swallowed
  // silently (which used to leave the controller pending).
  const controller = runSearch({
    query: "abc",
    debounceMs: 0,
    invoke: async () => {
      throw new Error("backend unreachable");
    },
    setTimer: () => {},
    clearTimer: () => {},
  });
  await assert.rejects(
    controller.result,
    /backend unreachable/,
    "the controller must reject with the backend error",
  );
});

test("runSearch cancel suppresses a late success response", async () => {
  // Cancelling before the response arrives MUST leave the controller
  // pending: the caller is expected to drop the reference. We assert
  // via Promise.race against a microtask pump so we never produce an
  // unhandled rejection.
  type Deferred = {
    resolve: (response: SearchResponse) => void;
    reject: (error: unknown) => void;
  };
  const deferreds: Deferred[] = [];
  const invoke = (_q: string): Promise<SearchResponse> =>
    new Promise<SearchResponse>((resolve, reject) => {
      deferreds.push({ resolve, reject });
    });

  const controller = runSearch({
    query: "abc",
    debounceMs: 0,
    invoke,
    setTimer: () => {},
    clearTimer: () => {},
  });
  controller.cancel();
  // Resolve after cancellation: the response must not settle the
  // controller.
  deferreds[0]!.resolve(response("ok", [99]));
  const settled = await Promise.race([
    controller.result
      .then(() => "resolved")
      .catch(() => "rejected"),
    new Promise((resolve) => setImmediate(() => resolve("pending"))),
  ]);
  assert.equal(settled, "pending");
});

test("runSearch cancel suppresses a late error", async () => {
  type Deferred = {
    resolve: (response: SearchResponse) => void;
    reject: (error: unknown) => void;
  };
  const deferreds: Deferred[] = [];
  const invoke = (_q: string): Promise<SearchResponse> =>
    new Promise<SearchResponse>((resolve, reject) => {
      deferreds.push({ resolve, reject });
    });

  const controller = runSearch({
    query: "abc",
    debounceMs: 0,
    invoke,
    setTimer: () => {},
    clearTimer: () => {},
  });
  controller.cancel();
  deferreds[0]!.reject(new Error("late failure"));
  const settled = await Promise.race([
    controller.result
      .then(() => "resolved")
      .catch(() => "rejected"),
    new Promise((resolve) => setImmediate(() => resolve("pending"))),
  ]);
  assert.equal(settled, "pending");
});

test("cancelled previous controller does not let a stale response overwrite a newer one", async () => {
  // Pins the Svelte usage: cancel the previous controller before
  // starting a new one. The cancelled controller's late response must
  // not pollute the new one's result.
  type Deferred = {
    resolve: (response: SearchResponse) => void;
    reject: (error: unknown) => void;
  };
  const deferreds: Deferred[] = [];
  const invoke = (_q: string): Promise<SearchResponse> =>
    new Promise<SearchResponse>((resolve, reject) => {
      deferreds.push({ resolve, reject });
    });

  const first = runSearch({
    query: "alpha",
    debounceMs: 0,
    invoke,
    setTimer: () => {},
    clearTimer: () => {},
  });
  // Cancel and start the next one immediately.
  first.cancel();
  const second = runSearch({
    query: "beta",
    debounceMs: 0,
    invoke,
    setTimer: () => {},
    clearTimer: () => {},
  });

  // The cancelled invocation's response arrives first: it must NOT
  // resolve or reject the cancelled controller.
  deferreds[0]!.resolve(response("ok", [1]));
  const firstSettled = await Promise.race([
    first.result
      .then(() => "resolved")
      .catch(() => "rejected"),
    new Promise((resolve) => setImmediate(() => resolve("pending"))),
  ]);
  assert.equal(firstSettled, "pending");

  // The newer invocation resolves normally.
  deferreds[1]!.resolve(response("ok", [2]));
  const secondResult = await second.result;
  assert.equal(secondResult.hits[0].entry_id, 2);
});
