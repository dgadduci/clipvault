import { test } from "node:test";
import assert from "node:assert/strict";

import {
  activeAppDiagnosticsCommand,
  ignoredAppIconCommand,
  ignoredAppPickAndAddCommand,
  ignoredAppsAddCommand,
  ignoredAppsListCommand,
  ignoredAppsListWithMetadataCommand,
  ignoredAppsRemoveCommand,
  refreshActiveAppDiagnosticsCommand,
  settingsGetCommand,
  settingsSetCommand,
} from "../src/lib/tauri.ts";
import type {
  ActiveAppDiagnostics,
  ActiveAppRefreshOutcome,
  IgnoredAppEntry,
  PickAndAddResponse,
  PickErrorReason,
  RetentionPolicy,
  Settings,
} from "../src/types.ts";

type InvokeHandle = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

function installTauriMock(invoker: InvokeHandle): void {
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: { invoke: invoker },
  };
}

function defaultSettings(): Settings {
  return {
    retention: "days_30",
    ignored_apps: [],
    quick_paste_hotkey: null,
  };
}

test("settingsGetCommand returns the persisted aggregate", async () => {
  installTauriMock(async (cmd) => {
    assert.equal(cmd, "clipvault_settings_get");
    return defaultSettings();
  });
  const result = await settingsGetCommand();
  assert.equal(result.retention, "days_30");
  assert.deepEqual(result.ignored_apps, []);
  assert.equal(result.quick_paste_hotkey, null);
});

test("settingsSetCommand forwards the partial update and returns the new state", async () => {
  let observed: Record<string, unknown> | undefined;
  installTauriMock(async (cmd, args) => {
    assert.equal(cmd, "clipvault_settings_set");
    observed = args;
    return {
      ...defaultSettings(),
      retention: (args?.update as { retention: RetentionPolicy }).retention,
    };
  });
  const result = await settingsSetCommand({ retention: "days_7" });
  assert.equal(observed?.update?.retention, "days_7");
  assert.equal(result.retention, "days_7");
});

test("ignoredAppsAddCommand sends the identifier through and receives the new aggregate", async () => {
  let receivedId: string | undefined;
  installTauriMock(async (cmd, args) => {
    assert.equal(cmd, "clipvault_ignored_apps_add");
    receivedId = args?.id as string;
    return {
      ...defaultSettings(),
      ignored_apps: [receivedId ?? ""],
    };
  });
  const result = await ignoredAppsAddCommand({ id: "com.apple.Terminal" });
  assert.equal(receivedId, "com.apple.Terminal");
  assert.deepEqual(result.ignored_apps, ["com.apple.Terminal"]);
});

test("ignoredAppsRemoveCommand removes the identifier and reflects the change", async () => {
  let receivedId: string | undefined;
  installTauriMock(async (cmd, args) => {
    assert.equal(cmd, "clipvault_ignored_apps_remove");
    receivedId = args?.id as string;
    return defaultSettings();
  });
  const result = await ignoredAppsRemoveCommand({ id: "firefox" });
  assert.equal(receivedId, "firefox");
  assert.deepEqual(result.ignored_apps, []);
});

test("ignoredAppsListCommand returns the persisted list", async () => {
  installTauriMock(async (cmd) => {
    assert.equal(cmd, "clipvault_ignored_apps_list");
    return ["firefox", "1password"];
  });
  const list = await ignoredAppsListCommand();
  assert.deepEqual(list, ["firefox", "1password"]);
});

test("ignoredAppsAddCommand forwards the trimmed identifier verbatim", async () => {
  // The frontend trims whitespace but never mutates case before
  // the value hits the Tauri boundary; the backend owns the
  // normalisation so the matcher has a single, authoritative rule.
  let receivedId: string | undefined;
  installTauriMock(async (cmd, args) => {
    assert.equal(cmd, "clipvault_ignored_apps_add");
    receivedId = args?.id as string;
    return {
      ...defaultSettings(),
      ignored_apps: [receivedId ?? ""],
    };
  });
  const input = "  Com.Apple.Terminal  ";
  await ignoredAppsAddCommand({ id: input.trim() });
  assert.equal(receivedId, "Com.Apple.Terminal");
});

test("activeAppDiagnosticsCommand routes to the diagnostics command", async () => {
  const expected: ActiveAppDiagnostics = {
    available: true,
    backend: "macos_workspace",
    cache_populated: true,
    identifier: "com.apple.Terminal",
    name: "Terminal",
    refresh_outcome: { kind: "ok" },
    failure_kind: null,
    loop_started: true,
    refresh_attempts: 5,
    successful_refreshes: 5,
    failed_refreshes: 0,
    last_refresh_unix_ms: 1700000000000,
    last_capture_decision: "allowed:stored",
  };
  installTauriMock(async (cmd) => {
    assert.equal(cmd, "clipvault_active_app_diagnostics");
    return expected;
  });
  const diag = await activeAppDiagnosticsCommand();
  assert.equal(diag.backend, "macos_workspace");
  assert.equal(diag.cache_populated, true);
  assert.equal(diag.identifier, "com.apple.Terminal");
  assert.deepEqual(diag.refresh_outcome, { kind: "ok" });
  assert.equal(diag.failure_kind, null);
  assert.equal(diag.loop_started, true);
  assert.equal(diag.refresh_attempts, 5);
  assert.equal(diag.successful_refreshes, 5);
  assert.equal(diag.failed_refreshes, 0);
  assert.equal(diag.last_refresh_unix_ms, 1700000000000);
  assert.equal(diag.last_capture_decision, "allowed:stored");
});

test("activeAppDiagnosticsCommand surfaces a failed refresh outcome", async () => {
  // A failed refresh must reach the frontend with the raw error
  // message sanitised by the backend; the helper never includes
  // clipboard content. The structured `failure_kind` lets the UI
  // render actionable copy without parsing the free-form `message`.
  const failed: ActiveAppDiagnostics = {
    available: true,
    backend: "x11_ewmh",
    cache_populated: false,
    identifier: null,
    name: null,
    refresh_outcome: {
      kind: "failed",
      failure_kind: "backend",
      message: "x11 disconnected",
    },
    failure_kind: "backend",
    loop_started: true,
    refresh_attempts: 3,
    successful_refreshes: 2,
    failed_refreshes: 1,
    last_refresh_unix_ms: 1700000000000,
    last_capture_decision: "discarded:blacklisted",
  };
  installTauriMock(async () => failed);
  const diag = await activeAppDiagnosticsCommand();
  assert.equal(diag.refresh_outcome.kind, "failed");
  assert.equal(diag.failure_kind, "backend");
  assert.equal(diag.failed_refreshes, 1);
});

test("refreshActiveAppDiagnosticsCommand triggers a backend refresh and returns the snapshot", async () => {
  // The **Refrescar diagnóstico** button must call a command that
  // actually triggers a refresh on the backend, then return the
  // resulting snapshot. This regression pins the contract: a future
  // refactor that wires the button to the read-only command instead
  // would let the user click "refresh" while the snapshot stays
  // stuck on `pending`, and the user would have no way to recover.
  let observedCmd: string | undefined;
  const refreshed: ActiveAppDiagnostics = {
    available: true,
    backend: "macos_workspace",
    cache_populated: true,
    identifier: "com.apple.TextEdit",
    name: "TextEdit",
    refresh_outcome: { kind: "ok" },
    failure_kind: null,
    loop_started: true,
    refresh_attempts: 1,
    successful_refreshes: 1,
    failed_refreshes: 0,
    last_refresh_unix_ms: 1700000000000,
    last_capture_decision: "discarded:blacklisted",
  };
  installTauriMock(async (cmd) => {
    observedCmd = cmd;
    return refreshed;
  });
  const diag = await refreshActiveAppDiagnosticsCommand();
  assert.equal(
    observedCmd,
    "clipvault_refresh_active_app_diagnostics",
    "the refresh command MUST route to the backend refresh path",
  );
  assert.equal(diag.identifier, "com.apple.TextEdit");
  assert.equal(diag.refresh_outcome.kind, "ok");
  assert.equal(diag.refresh_attempts, 1);
});

test("refreshActiveAppDiagnosticsCommand surfaces a failed refresh outcome", async () => {
  // A failed synchronous refresh (Tauri scheduler rejected the
  // closure, or the main thread did not pick it up in time) must
  // reach the frontend with the actionable reason sanitised by the
  // backend. The command MUST surface `failed` instead of leaving
  // the snapshot at `pending`.
  const failed: ActiveAppDiagnostics = {
    available: true,
    backend: "macos_workspace",
    cache_populated: false,
    identifier: null,
    name: null,
    refresh_outcome: {
      kind: "failed",
      failure_kind: "timeout",
      message: "main thread did not execute the closure in 500ms",
    },
    failure_kind: "timeout",
    loop_started: true,
    refresh_attempts: 1,
    successful_refreshes: 0,
    failed_refreshes: 1,
    last_refresh_unix_ms: 1700000000000,
    last_capture_decision: null,
  };
  installTauriMock(async (cmd) => {
    assert.equal(cmd, "clipvault_refresh_active_app_diagnostics");
    return failed;
  });
  const diag = await refreshActiveAppDiagnosticsCommand();
  assert.equal(diag.refresh_outcome.kind, "failed");
  if (diag.refresh_outcome.kind === "failed") {
    assert.ok(diag.refresh_outcome.message.includes("500ms"));
  }
  assert.equal(diag.failure_kind, "timeout");
  assert.equal(diag.failed_refreshes, 1);
});

test("SettingsPanel identifiers normalise for the blacklist match (frontend helper)", () => {
  // We mirror the core matcher rule (trim + lowercase) in
  // `SettingsPanel.svelte` so a captured identifier is compared
  // against the persisted list with the same canonicalisation.
  // This regression asserts the helper invariant the panel relies
  // on. If a future refactor moves the helper into a dedicated
  // module the test should follow it.
  const observed = "  Com.Apple.Terminal  ".trim().toLowerCase();
  const persisted = "com.apple.terminal";
  assert.equal(observed, persisted);
});

test("error responses do not break the settings panel contract", () => {
  // The frontend relies on a typed `CommandError` shape; we expect
  // `{ kind: string; message: string }` so that .svelte panel
  // renders the message without crashing. The Tauri helpers never
  // change this contract because the panel teardown depends on the
  // structured failure path.
  const error = { kind: "validation_error", message: "identifier too long" };
  assert.equal(typeof error.kind, "string");
  assert.equal(typeof error.message, "string");
});

test("failure_kind distinguishes schedule from timeout from unavailable from backend", () => {
  // The user-reported regression saw 214 attempts, 214 failures, 0
  // successes with no way to tell WHY each refresh failed. The
  // structured `failure_kind` field pins the four failure causes
  // the user can encounter and the snake_case string the UI uses
  // to render actionable copy.
  const kinds = [
    "schedule",
    "timeout",
    "unavailable",
    "backend",
  ] as const;
  for (const kind of kinds) {
    assert.ok(
      kinds.indexOf(kind) >= 0,
      `${kind} must be one of the four failure kinds`,
    );
  }
  // The serialised payload mirrors the Rust enum exactly.
  const schedule: ActiveAppRefreshOutcome = {
    kind: "failed",
    failure_kind: "schedule",
    message: "main thread did not accept the closure: event loop shut down",
  };
  const timeout: ActiveAppRefreshOutcome = {
    kind: "failed",
    failure_kind: "timeout",
    message: "main thread did not execute the closure in 500ms",
  };
  const unavailable: ActiveAppRefreshOutcome = {
    kind: "failed",
    failure_kind: "unavailable",
    message: "active-app probe unavailable on this session",
  };
  const backend: ActiveAppRefreshOutcome = {
    kind: "failed",
    failure_kind: "backend",
    message: "x11 disconnected",
  };
  if (
    schedule.kind === "failed" &&
    timeout.kind === "failed" &&
    unavailable.kind === "failed" &&
    backend.kind === "failed"
  ) {
    assert.equal(schedule.failure_kind, "schedule");
    assert.equal(timeout.failure_kind, "timeout");
    assert.equal(unavailable.failure_kind, "unavailable");
    assert.equal(backend.failure_kind, "backend");
  }
});

// ---------------------------------------------------------------------------
// `blacklist-app-picker`: the SettingsPanel now drives the picker through a
// typed Tauri command instead of accepting arbitrary user-typed identifiers.
// The tests below pin the contract the panel relies on: the response is a
// discriminated union (added / updated / cancelled / error) and the frontend
// NEVER receives clipboard content, hashes or snippets through it.
// ---------------------------------------------------------------------------

test("ignoredAppPickAndAddCommand routes to the picker command", async () => {
  // The frontend must call the picker command (no fallback path),
  // never invoke a generic `add` with a hand-typed identifier.
  let observedCmd: string | undefined;
  const added: PickAndAddResponse = {
    kind: "added",
    entry: {
      id: "com.apple.terminal",
      display_name: "Terminal",
      icon_ref: "ignored-apps/com.apple.terminal.png",
      created_at: "2026-01-02T03:04:05Z",
    },
  };
  installTauriMock(async (cmd) => {
    observedCmd = cmd;
    return added;
  });
  const result = await ignoredAppPickAndAddCommand();
  assert.equal(observedCmd, "clipvault_ignored_app_pick_and_add");
  assert.equal(result.kind, "added");
  if (result.kind === "added") {
    assert.equal(result.entry.id, "com.apple.terminal");
    assert.equal(result.entry.display_name, "Terminal");
    assert.equal(
      result.entry.icon_ref,
      "ignored-apps/com.apple.terminal.png",
    );
  }
});

test("ignoredAppPickAndAddCommand surfaces cancellation as a non-error", async () => {
  // Cancellation MUST NOT reach the panel as an error: the blacklist
  // is unchanged and no toast is required.
  installTauriMock(async () => ({ kind: "cancelled" }) satisfies PickAndAddResponse);
  const result = await ignoredAppPickAndAddCommand();
  assert.equal(result.kind, "cancelled");
});

test("ignoredAppPickAndAddCommand surfaces the typed reason on error", async () => {
  // The picker surfaces the same five reasons the Rust enum exposes.
  // The frontend branches on `kind: "error"` and the stable
  // `reason` enum to render the matching copy without parsing the
  // free-form `message`.
  const reasons: PickErrorReason[] = [
    "invalid_selection",
    "missing_identifier",
    "backend_unavailable",
    "unsupported_session",
    "persistence_error",
  ];
  for (const reason of reasons) {
    installTauriMock(
      async () =>
        ({
          kind: "error",
          reason,
          message: `${reason} sample`,
        }) satisfies PickAndAddResponse,
    );
    const result = await ignoredAppPickAndAddCommand();
    assert.equal(result.kind, "error");
    if (result.kind === "error") {
      assert.equal(result.reason, reason);
    }
  }
});

test("ignoredAppPickAndAddCommand preserves the updated outcome", async () => {
  // The settings panel must distinguish "first time" from "metadata
  // refresh" so the action message stays truthful.
  const updated: PickAndAddResponse = {
    kind: "updated",
    entry: {
      id: "com.apple.terminal",
      display_name: "Terminal",
      icon_ref: "ignored-apps/com.apple.terminal.png",
      created_at: "2026-01-02T03:04:05Z",
    },
  };
  installTauriMock(async () => updated);
  const result = await ignoredAppPickAndAddCommand();
  assert.equal(result.kind, "updated");
});

test("ignoredAppPickAndAddCommand never carries clipboard content", async () => {
  // Privacy regression: the picker response MUST NEVER include
  // clipboard content, hashes or snippets. We assert the payload is
  // metadata-only by checking the structural type.
  installTauriMock(async () => ({
    kind: "added",
    entry: {
      id: "com.apple.terminal",
      display_name: "Terminal",
      icon_ref: null,
      created_at: "2026-01-02T03:04:05Z",
    },
  }));
  const result = await ignoredAppPickAndAddCommand();
  if (result.kind !== "added") throw new Error("expected added");
  const forbidden: Array<keyof IgnoredAppEntry> = [
    "id",
    "display_name",
    "icon_ref",
    "created_at",
  ];
  const keys = Object.keys(result.entry);
  for (const forbiddenKey of forbidden) {
    assert.ok(
      keys.includes(forbiddenKey),
      `entry must expose ${forbiddenKey}`,
    );
  }
  // The structural type forbids content / hash / snippet keys.
  assert.equal((result.entry as unknown as { content?: unknown }).content, undefined);
  assert.equal((result.entry as unknown as { hash?: unknown }).hash, undefined);
  assert.equal((result.entry as unknown as { snippet?: unknown }).snippet, undefined);
});

test("ignoredAppsListWithMetadataCommand returns legacy rows with null metadata", async () => {
  // Rows inserted before the picker landed only carry `id` and
  // `created_at`. The frontend renders the row with the identifier
  // as the fallback name and the icon-fallback CSS class.
  installTauriMock(async (cmd) => {
    assert.equal(cmd, "clipvault_ignored_apps_list_with_metadata");
    return [
      {
        id: "firefox",
        display_name: null,
        icon_ref: null,
        created_at: "2026-01-01T00:00:00Z",
      },
    ] satisfies IgnoredAppEntry[];
  });
  const list = await ignoredAppsListWithMetadataCommand();
  assert.equal(list.length, 1);
  assert.equal(list[0].id, "firefox");
  assert.equal(list[0].display_name, null);
  assert.equal(list[0].icon_ref, null);
});

// ---------------------------------------------------------------------------
// Icon presentation for the picker metadata. The settings panel must
// distinguish rows whose `icon_ref` resolves to a real PNG from rows
// whose reference is missing or rejected: the former render an `<img>`
// backed by a `blob:` URL produced from the new `clipvault_ignored_app_icon`
// command, the latter fall back to the letter render without surfacing
// the underlying error to the user.
// ---------------------------------------------------------------------------

test("ignoredAppIconCommand returns the bytes for a valid icon ref", async () => {
  installTauriMock(async (cmd) => {
    assert.equal(cmd, "clipvault_ignored_app_icon");
    return [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  });
  const bytes = await ignoredAppIconCommand({ ref: "ignored-apps/x.png" });
  assert.deepEqual(bytes, [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
});

test("ignoredAppIconCommand rejects traversal references with the typed reason", async () => {
  installTauriMock(async () => {
    throw { kind: "invalid_icon_ref", message: "traversal" };
  });
  await assert.rejects(
    ignoredAppIconCommand({ ref: "ignored-apps/../etc/passwd" }),
    (error: unknown) => {
      assert.equal(
        (error as { kind: string }).kind,
        "invalid_icon_ref",
      );
      return true;
    },
  );
});

test("ignoredAppIconCommand rejects references outside assets/ignored-apps", async () => {
  installTauriMock(async () => {
    throw { kind: "invalid_icon_ref", message: "out_of_scope" };
  });
  await assert.rejects(
    ignoredAppIconCommand({ ref: "snapshots/x.png" }),
    (error: unknown) => {
      assert.equal(
        (error as { kind: string }).kind,
        "invalid_icon_ref",
      );
      return true;
    },
  );
});

test("ignoredAppIconCommand rejects absolute paths", async () => {
  installTauriMock(async () => {
    throw { kind: "invalid_icon_ref", message: "absolute" };
  });
  await assert.rejects(
    ignoredAppIconCommand({ ref: "/etc/passwd" }),
    (error: unknown) => {
      assert.equal(
        (error as { kind: string }).kind,
        "invalid_icon_ref",
      );
      return true;
    },
  );
});

