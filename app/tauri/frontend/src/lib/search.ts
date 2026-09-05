// Pure helpers for the search input. Kept separate from the Svelte
// component so they can be exercised by `node:test` without a DOM.

import type { SearchResponse } from "../types.ts";

export interface RunSearchOptions {
  query: string;
  /** Function that actually calls the backend. */
  invoke: (query: string) => Promise<SearchResponse>;
  /** Optional debounce window in milliseconds. */
  debounceMs?: number;
  /**
   * Optional clock used by tests to control time. Defaults to
   * `setTimeout`/`clearTimeout`.
   */
  setTimer?: (cb: () => void, ms: number) => unknown;
  clearTimer?: (handle: unknown) => void;
}

export interface RunSearchController {
  /** Resolves with the response produced by this invocation. */
  result: Promise<SearchResponse>;
  /**
   * Cancel the controller. After calling `cancel`, `result` will
   * never resolve or reject — every pending callback is suppressed
   * and any in-flight response is dropped on the floor.
   */
  cancel: () => void;
}

/**
 * Run a search with an optional debounce. The returned controller
 * settles exactly once with one of three outcomes:
 *
 * 1. Resolves with the response produced by the invocation.
 * 2. Rejects with the error thrown by the invocation.
 * 3. Stays pending forever when the caller cancels before the
 *    invocation completes. We intentionally do not reject with a
 *    fabricated error because that would surface misleading failures
 *    in the Svelte component.
 *
 * The caller is responsible for cancelling a previous controller
 * before starting a new one so that stale responses do not overwrite
 * fresh ones; `cancel` suppresses late callbacks, so two consecutive
 * controllers cannot leak results.
 */
export function runSearch(options: RunSearchOptions): RunSearchController {
  const setTimer = options.setTimer ?? defaultSetTimer;
  const clearTimer = options.clearTimer ?? defaultClearTimer;
  const debounceMs = Math.max(0, options.debounceMs ?? 0);

  let timer: unknown = null;
  let cancelled = false;

  const result: Promise<SearchResponse> = new Promise((resolve, reject) => {
    const fire = () => {
      options
        .invoke(options.query)
        .then((response) => {
          if (cancelled) return;
          resolve(response);
        })
        .catch((err) => {
          if (cancelled) return;
          reject(err);
        });
    };

    if (debounceMs === 0) {
      fire();
    } else {
      timer = setTimer(fire, debounceMs);
    }
  });

  return {
    result,
    cancel: () => {
      cancelled = true;
      if (timer != null) {
        clearTimer(timer);
        timer = null;
      }
    },
  };
}

function defaultSetTimer(cb: () => void, ms: number): unknown {
  return setTimeout(cb, ms);
}

function defaultClearTimer(handle: unknown): void {
  if (handle != null) {
    clearTimeout(handle as ReturnType<typeof setTimeout>);
  }
}
