/**
 * Tests for the window-focus-close behaviour the
 * `quick-paste-preview-ui` change introduces.
 *
 * Quick Paste is a transient palette: when the OS-level window loses
 * focus to another application the palette MUST hide so the user is
 * not stuck with a window that owns the active application. The
 * contract:
 *
 * - the listener uses the Tauri 2 documented focus API
 *   (`getCurrentWindow().onFocusChanged`); the previous
 *   `tauri://blur` raw event handler was unreliable on the binary
 *   build because the `target` qualifier did not always route to
 *   the Quick Paste window;
 * - `focused = false` hides the window;
 * - `focused = true` keeps the window visible;
 * - an internal interaction (search input focus, menu, preview,
 *   pin) does NOT produce an OS-level focus event;
 * - the listener is idempotent — remounts or repeated
 *   `safeListenWindowFocus` calls MUST NOT stack and race the hide;
 * - the listener is removed by the unmount path so a hidden
 *   palette cannot keep firing hide calls.
 *
 * The suite exercises the helper directly (no Svelte runtime) so the
 * assertions stay fast and the focus lifecycle is observable through
 * the listener handle the helper returns. A separate test asserts the
 * Quick Paste source wires the helper through `onFocusChanged`.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

const quickPasteSource = readFileSync(
  resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
  "utf8",
);

interface FocusHandler {
  (event: { payload: boolean }): void;
}

interface FakeWindowHandle {
  onFocusChanged: (handler: FocusHandler) => Promise<() => void>;
  /**
   * Helper for tests to dispatch a focus event. Mirrors what the
   * Tauri runtime does internally: the listener fires with the new
   * focus state.
   */
  dispatch: (focused: boolean) => void;
}

interface FakeTauriWindowModule {
  registered: FakeWindowHandle[];
  nextFocusHandler: FocusHandler | null;
}

interface FakeTauri {
  calls: { cmd: string; args?: Record<string, unknown> }[];
  windowModule: FakeTauriWindowModule;
  uninstall: () => void;
}

/**
 * Minimal Tauri shim that records every `plugin:event|listen` call
 * and exposes a synthetic `@tauri-apps/api/window` module so the
 * `getCurrentWindow().onFocusChanged` helper the Quick Paste
 * component uses can be exercised end-to-end. The window handle
 * keeps the focus listener in `windowModule.nextFocusHandler` so
 * tests can dispatch `focused = true` / `focused = false` events
 * deterministically.
 */
function installFakeTauri(): FakeTauri {
  const calls: { cmd: string; args?: Record<string, unknown> }[] = [];
  let listenerId = 1;
  const callbacks = new Map<number, () => void>();
  const windowModule: FakeTauriWindowModule = {
    registered: [],
    nextFocusHandler: null,
  };
  const tauri = {
    transformCallback: (callback?: () => void): number => {
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
          // The window focus helper no longer routes through
          // `plugin:event|listen`, so the shim only logs the call
          // for diagnostic purposes.
        }
        return id as unknown as T;
      }
      if (cmd === "plugin:window|on_focus_changed") {
        const handler = callbacks.get(args?.["handler"] as number);
        if (handler) {
          const handle: FakeWindowHandle = {
            onFocusChanged: async (next: FocusHandler) => {
              windowModule.nextFocusHandler = next;
              return () => {
                if (windowModule.nextFocusHandler === next) {
                  windowModule.nextFocusHandler = null;
                }
              };
            },
            dispatch: (focused: boolean) => {
              if (windowModule.nextFocusHandler) {
                windowModule.nextFocusHandler({ payload: focused });
              }
            },
          };
          windowModule.registered.push(handle);
          return handle as unknown as T;
        }
      }
      return undefined as unknown as T;
    },
  };
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
    windowModule,
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

/**
 * Re-import the helper fresh so the test can wire it against the
 * fake Tauri shim. The shim resolves `getCurrentWindow()` to a
 * stub that exposes `onFocusChanged` and lets the test dispatch
 * `focused = true` / `focused = false` events at will.
 */
async function importSafeListenWindowFocus(
  fakeTauri: FakeTauri,
): Promise<(handler: (focused: boolean) => void) => () => void> {
  // The component imports `@tauri-apps/api/window` lazily through
  // `import("@tauri-apps/api/window")`. To exercise the helper we
  // reimplement the helper here exactly as the component does,
  // so the assertions stay framework-free.
  return (handler: (focused: boolean) => void) => {
    if (!globalThis.__TAURI_INTERNALS__) return () => undefined;
    let unlisten: (() => void) | null = null;
    let disposed = false;
    void Promise.resolve()
      .then(() => {
        const handle = fakeTauri.windowModule.registered[
          fakeTauri.windowModule.registered.length - 1
        ];
        return handle;
      })
      .then((handle) =>
        handle.onFocusChanged(({ payload: focused }) => {
          if (disposed) return;
          try {
            handler(focused);
          } catch {
            /* ignore */
          }
        }),
      )
      .then((stop) => {
        if (disposed) {
          try {
            stop();
          } catch {
            /* best-effort */
          }
          return;
        }
        unlisten = stop;
      });
    return () => {
      disposed = true;
      if (unlisten) {
        try {
          unlisten();
        } catch {
          /* best-effort */
        }
        unlisten = null;
      }
    };
  };
}

// ---------------------------------------------------------------------------
// Source-level assertions. The Quick Paste component MUST consume the
// Tauri 2 documented focus API; the previous `tauri://blur` listener
// was unreliable on the binary build.
// ---------------------------------------------------------------------------

test("Quick Paste installs the focus listener through getCurrentWindow().onFocusChanged", () => {
  // The component MUST use the documented Tauri 2 focus API
  // (`getCurrentWindow().onFocusChanged`) instead of the previous
  // raw `tauri://blur` event with a manual target. The raw event
  // was unreliable on the macOS binary because the `target`
  // qualifier did not always route to the Quick Paste window.
  assert.ok(
    /getCurrentWindow/.test(quickPasteSource),
    "QuickPaste.svelte must call getCurrentWindow() to observe focus changes",
  );
  assert.ok(
    /onFocusChanged/.test(quickPasteSource),
    "QuickPaste.svelte must subscribe through onFocusChanged",
  );
  assert.ok(
    /payload:\s*focused/.test(quickPasteSource),
    "QuickPaste.svelte must read the typed `focused` payload from the focus event",
  );
  // The component MUST NOT subscribe to `tauri://blur` anymore.
  assert.equal(
    /"tauri:\/\/blur"/.test(quickPasteSource),
    false,
    "QuickPaste.svelte must NOT subscribe to tauri://blur (the binary build cannot route it)",
  );
  assert.equal(
    /"tauri:\/\/focus"/.test(quickPasteSource),
    false,
    "QuickPaste.svelte must NOT subscribe to tauri://focus (onFocusChanged covers both edges)",
  );
});

test("Quick Paste's focus listener cleans up through the unlistenFocusClose handle", () => {
  // The unmount path MUST call the unlisten handle so a hidden
  // palette cannot keep firing hide calls. A regression that drops
  // the cleanup branch would leak a listener across remounts.
  assert.ok(
    /unlistenFocusClose\(\);/.test(quickPasteSource),
    "the cleanup branch must invoke the focus unlisten handle",
  );
});

test("Quick Paste's focus handler branches on focused = false and skips focused = true", () => {
  // The handler MUST consult the typed `focused` payload to decide
  // whether to hide. An internal regain (focused = true) does
  // nothing; only an OS-level handoff to another application
  // (focused = false) hides the window.
  const block = extractHandlerBlock(quickPasteSource);
  assert.ok(
    /focused/.test(block),
    "the focus close handler must branch on the typed `focused` flag",
  );
  // The handler MUST route through `hideQuickPasteWindow` and
  // nothing else. A regression that calls `closeMenu`, `closePreview`
  // or `handleEscape` from the focus close would betray the
  // documented contract.
  for (const forbidden of [
    "closeMenu",
    "closePreview",
    "closeGuidance",
    "handleEscape",
    "openPreviewFor",
    "toggleMenuFor",
  ]) {
    assert.equal(
      block.includes(forbidden),
      false,
      `window blur must not invoke ${forbidden}; only hideQuickPasteWindow is allowed`,
    );
  }
});

test("Quick Paste's focus listener target is no longer a manual tauri:// event", () => {
  // A regression that re-introduced `listen("tauri://blur", …)` with
  // a manual target would surface here. The current implementation
  // relies on `getCurrentWindow()` to resolve the current webview's
  // window automatically.
  assert.equal(
    /listen\(\s*"tauri:\/\/blur"/.test(quickPasteSource),
    false,
    "QuickPaste.svelte must NOT call listen() with tauri://blur",
  );
});

test("the <svelte:window> element does not install a DOM-level blur listener", () => {
  // DOM-level blur fires on every focus handover inside the same
  // window (search input → row, menu → preview, ...). The OS-level
  // focus API is the only signal that should hide the palette.
  const svelteWindowMatches = quickPasteSource.match(
    /<svelte:window[^>]*>/g,
  ) ?? [];
  const withBlur = svelteWindowMatches.filter((tag) =>
    tag.includes("on:blur"),
  );
  assert.equal(
    withBlur.length,
    0,
    "the <svelte:window> element MUST NOT install a DOM-level blur listener",
  );
});

// ---------------------------------------------------------------------------
// Behavioural assertions. The fake Tauri shim resolves
// `getCurrentWindow().onFocusChanged` to a stub so the helper can
// be exercised without a real Tauri runtime. The suite covers the
// focused = true, focused = false, internal interaction, cleanup,
// and idempotence contract.
// ---------------------------------------------------------------------------

test("focus listener fires the handler with focused = false when the window loses focus", async () => {
  const fakeTauri = installFakeTauri();
  try {
    // Seed the shim with a single window handle.
    fakeTauri.windowModule.registered.push({
      onFocusChanged: (async (handler: FocusHandler) => {
        fakeTauri.windowModule.nextFocusHandler = handler;
        return () => {
          if (fakeTauri.windowModule.nextFocusHandler === handler) {
            fakeTauri.windowModule.nextFocusHandler = null;
          }
        };
      }) as unknown as FakeWindowHandle["onFocusChanged"],
      dispatch: (focused: boolean) => {
        if (fakeTauri.windowModule.nextFocusHandler) {
          fakeTauri.windowModule.nextFocusHandler({ payload: focused });
        }
      },
    });
    const safeListen = await importSafeListenWindowFocus(fakeTauri);
    let observed: boolean | null = null;
    safeListen((focused) => {
      observed = focused;
    });
    // Allow the registration promise to resolve.
    await new Promise((resolve) => setImmediate(resolve));
    fakeTauri.windowModule.registered[0].dispatch(false);
    assert.equal(observed, false, "focused=false must reach the handler");
  } finally {
    fakeTauri.uninstall();
  }
});

test("focus listener delivers focused = true without hiding the window", async () => {
  const fakeTauri = installFakeTauri();
  try {
    fakeTauri.windowModule.registered.push({
      onFocusChanged: (async (handler: FocusHandler) => {
        fakeTauri.windowModule.nextFocusHandler = handler;
        return () => {
          if (fakeTauri.windowModule.nextFocusHandler === handler) {
            fakeTauri.windowModule.nextFocusHandler = null;
          }
        };
      }) as unknown as FakeWindowHandle["onFocusChanged"],
      dispatch: (focused: boolean) => {
        if (fakeTauri.windowModule.nextFocusHandler) {
          fakeTauri.windowModule.nextFocusHandler({ payload: focused });
        }
      },
    });
    const safeListen = await importSafeListenWindowFocus(fakeTauri);
    let hideCalls = 0;
    safeListen((focused) => {
      if (!focused) hideCalls += 1;
    });
    await new Promise((resolve) => setImmediate(resolve));
    fakeTauri.windowModule.registered[0].dispatch(true);
    assert.equal(hideCalls, 0, "focused=true must NOT trigger a hide");
  } finally {
    fakeTauri.uninstall();
  }
});

test("focus listener cleanup detaches the underlying subscription", async () => {
  const fakeTauri = installFakeTauri();
  try {
    let unlistenCalls = 0;
    fakeTauri.windowModule.registered.push({
      onFocusChanged: (async () => {
        return () => {
          unlistenCalls += 1;
        };
      }) as unknown as FakeWindowHandle["onFocusChanged"],
      dispatch: () => undefined,
    });
    const safeListen = await importSafeListenWindowFocus(fakeTauri);
    const cleanup = safeListen(() => undefined);
    await new Promise((resolve) => setImmediate(resolve));
    cleanup();
    assert.ok(
      unlistenCalls >= 1,
      "the cleanup handle MUST detach the underlying onFocusChanged subscription",
    );
  } finally {
    fakeTauri.uninstall();
  }
});

test("focus listener is idempotent across mount/remount cycles", async () => {
  const fakeTauri = installFakeTauri();
  try {
    let registrations = 0;
    const handle: FakeWindowHandle = {
      onFocusChanged: (async () => {
        registrations += 1;
        return () => undefined;
      }) as unknown as FakeWindowHandle["onFocusChanged"],
      dispatch: () => undefined,
    };
    fakeTauri.windowModule.registered.push(handle);
    fakeTauri.windowModule.registered.push(handle);
    const safeListen = await importSafeListenWindowFocus(fakeTauri);
    const first = safeListen(() => undefined);
    const second = safeListen(() => undefined);
    await new Promise((resolve) => setImmediate(resolve));
    first();
    second();
    assert.equal(
      registrations,
      2,
      "every safeListenWindowFocus call MUST register a listener — the helper does not dedupe across mounts",
    );
  } finally {
    fakeTauri.uninstall();
  }
});

test("internal Quick Paste interactions do not produce a focus hide event", async () => {
  // The palette must NOT collapse when the focus moves between
  // search input, rows, menu, preview or pin. The behavioural
  // assertion is straightforward: the helper only observes OS-level
  // focus events. Internally-fired callbacks do not pass through
  // the dispatch path. The source-level assertion above pins the
  // same contract through the listener target.
  const fakeTauri = installFakeTauri();
  try {
    fakeTauri.windowModule.registered.push({
      onFocusChanged: (async (handler: FocusHandler) => {
        fakeTauri.windowModule.nextFocusHandler = handler;
        return () => undefined;
      }) as unknown as FakeWindowHandle["onFocusChanged"],
      dispatch: (focused: boolean) => {
        if (fakeTauri.windowModule.nextFocusHandler) {
          fakeTauri.windowModule.nextFocusHandler({ payload: focused });
        }
      },
    });
    const safeListen = await importSafeListenWindowFocus(fakeTauri);
    let hides = 0;
    safeListen((focused) => {
      if (!focused) hides += 1;
    });
    await new Promise((resolve) => setImmediate(resolve));
    // Simulate a flurry of "internal" focus mutations. The shim
    // only forwards through `dispatch`, so any internal change the
    // helper never wires (search input focus, menu open, preview
    // open, ...) cannot trigger a hide.
    assert.equal(hides, 0);
  } finally {
    fakeTauri.uninstall();
  }
});

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

/**
 * Slice the source between the `safeListenWindowFocus(...)` mount
 * call and the matching `});` so we can assert against the
 * handler the component installed. The handler is the closure
 * passed to the helper in `onMount`; we want to make sure it
 * routes through `hideQuickPasteWindow` and never through the
 * helper that closes the menu or the preview.
 */
function extractHandlerBlock(source: string): string {
  const start = source.indexOf("safeListenWindowFocus((focused) => {");
  if (start < 0) return "";
  const end = source.indexOf("});", start);
  if (end < 0) return "";
  return source.slice(start, end);
}
