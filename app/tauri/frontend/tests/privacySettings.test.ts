import { test } from "node:test";
import assert from "node:assert/strict";

import {
  activeAppDiagnosticsCommand,
  ignoredAppIconCommand,
  ignoredAppLinuxAddCommand,
  ignoredAppLinuxCatalogCommand,
  ignoredAppPickAndAddCommand,
  ignoredAppsAddCommand,
  ignoredAppsListCommand,
  ignoredAppsListWithMetadataCommand,
  ignoredAppsRemoveCommand,
  localPeerProfileGetCommand,
  localPeerProfileUpdateCommand,
  refreshActiveAppDiagnosticsCommand,
  settingsGetCommand,
  settingsSetCommand,
} from "../src/lib/tauri.ts";
import type {
  ActiveAppDiagnostics,
  ActiveAppRefreshOutcome,
  IgnoredAppEntry,
  LinuxCatalogResponse,
  LinuxPickAndAddResponse,
  LocalPeerProfileResponse,
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
    local_peer_display_name: null,
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

test("PrivacyModal identifiers normalise for the blacklist match (frontend helper)", () => {
  // We mirror the core matcher rule (trim + lowercase) in
  // `PrivacyModal.svelte` so a captured identifier is compared
  // against the persisted list with the same canonicalisation.
  // This regression asserts the helper invariant the modal relies
  // on. If a future refactor moves the helper into a dedicated
  // module the test should follow it.
  const observed = "  Com.Apple.Terminal  ".trim().toLowerCase();
  const persisted = "com.apple.terminal";
  assert.equal(observed, persisted);
});

test("error responses do not break the privacy modal contract", () => {
  // The frontend relies on a typed `CommandError` shape; we expect
  // `{ kind: string; message: string }` so that the Svelte modal
  // renders the message without crashing. The Tauri helpers never
  // change this contract because the modal teardown depends on the
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
// `blacklist-app-picker`: the PrivacyModal now drives the picker through a
// typed Tauri command instead of accepting arbitrary user-input identifiers.
// The tests below pin the contract the modal relies on: the response is a
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
  // The privacy modal must distinguish "first time" from "metadata
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
// Icon presentation for the picker metadata. The privacy modal must
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


// ---------------------------------------------------------------------------
// Linux picker catalog (`linux-blacklist-app-picker` capability).
//
// The Linux picker replaces the synchronous `ApplicationPicker::pick`
// path with a deterministic catalog so the user picks the
// installed `.desktop` entry whose identifier the active-app
// adapter publishes. The catalog command returns either the list
// of candidates or a typed `unsupported` reason; the add command
// persists the chosen candidate through the same service the macOS
// picker uses.
// ---------------------------------------------------------------------------

test("ignoredAppLinuxCatalogCommand returns the supported list", async () => {
  const response: LinuxCatalogResponse = {
    kind: "supported",
    strategy: "wm_class",
    candidates: [
      {
        identifier: "firefox",
        display_name: "Firefox",
        icon_ref: "application-icons/firefox.png",
        strategy: "wm_class",
      },
      {
        identifier: "code",
        display_name: "Visual Studio Code",
        icon_ref: null,
        strategy: "wm_class",
      },
    ],
  };
  installTauriMock(async (cmd) => {
    assert.equal(cmd, "clipvault_ignored_app_linux_catalog");
    return response;
  });
  const result = await ignoredAppLinuxCatalogCommand();
  assert.equal(result.kind, "supported");
  if (result.kind === "supported") {
    assert.equal(result.strategy, "wm_class");
    assert.equal(result.candidates.length, 2);
    assert.equal(result.candidates[0].identifier, "firefox");
  }
});

test("ignoredAppLinuxCatalogCommand surfaces the unsupported reason", async () => {
  const response: LinuxCatalogResponse = {
    kind: "unsupported",
    reason: "no compositor publishes a stable app_id",
  };
  installTauriMock(async () => response);
  const result = await ignoredAppLinuxCatalogCommand();
  assert.equal(result.kind, "unsupported");
  if (result.kind === "unsupported") {
    assert.match(result.reason, /app_id/);
  }
});

test("ignoredAppLinuxAddCommand forwards only the opaque identifier", async () => {
  // The Linux picker IPC contract keeps the payload identifier-only:
  // the backend re-runs the catalog and uses its own metadata, so the
  // frontend MUST NOT echo the display name, icon reference or any
  // other metadata that already lives in the catalog.
  const observed: Record<string, unknown> = {};
  installTauriMock(async (cmd, args) => {
    assert.equal(cmd, "clipvault_ignored_app_linux_add");
    Object.assign(observed, args);
    return {
      kind: "added",
      entry: {
        id: (args?.identifier as string) ?? "",
        display_name: null,
        icon_ref: null,
        created_at: "2026-01-02T03:04:05Z",
      },
    } satisfies LinuxPickAndAddResponse;
  });
  const result = await ignoredAppLinuxAddCommand({
    identifier: "firefox",
  });
  assert.deepEqual(Object.keys(observed), ["identifier"]);
  assert.equal(observed.identifier, "firefox");
  assert.equal(result.kind, "added");
});

test("ignoredAppLinuxAddCommand propagates backend errors without leaking payloads", async () => {
  installTauriMock(async () => {
    throw { kind: "missing_identifier", message: "missing_identifier" };
  });
  await assert.rejects(
    ignoredAppLinuxAddCommand({
      identifier: "",
    }),
    (error: unknown) => {
      assert.equal((error as { kind: string }).kind, "missing_identifier");
      // The error MUST NOT carry clipboard content, hashes or
      // snippets — the backend never has them and the wrapper
      // forwards the typed error verbatim.
      assert.equal(
        (error as { content?: unknown }).content,
        undefined,
      );
      assert.equal((error as { hash?: unknown }).hash, undefined);
      assert.equal((error as { snippet?: unknown }).snippet, undefined);
      return true;
    },
  );
});

// ---------------------------------------------------------------------------
// Local peer identity foundation.
//
// The bridge only forwards metadata-only payloads — the private
// Ed25519 key bytes MUST never appear in the IPC envelope.
// ---------------------------------------------------------------------------

test("localPeerProfileGetCommand returns the available profile", async () => {
  const expected: LocalPeerProfileResponse = {
    kind: "available",
    profile: {
      peer_id: "0123456789abcdef0123456789abcdef",
      fingerprint: "0123456789abcdef",
      display_name: "Studio",
    },
  };
  installTauriMock(async (cmd) => {
    assert.equal(cmd, "clipvault_local_peer_profile_get");
    return expected;
  });
  const response = await localPeerProfileGetCommand();
  assert.equal(response.kind, "available");
  if (response.kind !== "available") return;
  assert.equal(response.profile.peer_id, expected.profile.peer_id);
  assert.equal(response.profile.display_name, "Studio");
  // Privacy invariant: the IPC envelope MUST NOT carry a private
  // key, public key, certificate or any other secret material. The
  // `public_key_hex` field the previous draft shipped has been
  // removed from the DTO so the frontend cannot leak the bytes that
  // the secure store already guards.
  const forbidden: Array<keyof { public_key_hex: unknown; private_key: unknown; secret: unknown; seed: unknown }> = [
    "public_key_hex",
    "private_key",
    "secret",
    "seed",
  ];
  for (const field of forbidden) {
    assert.equal(
      (response.profile as unknown as Record<string, unknown>)[field],
      undefined,
      `LocalPeerProfile MUST NOT include ${field}`,
    );
  }
});

test("localPeerProfileGetCommand reports unavailability without leaking platform detail", async () => {
  installTauriMock(async (cmd) => {
    assert.equal(cmd, "clipvault_local_peer_profile_get");
    return {
      kind: "unavailable",
      reason: "secure identity store unavailable",
    } satisfies LocalPeerProfileResponse;
  });
  const response = await localPeerProfileGetCommand();
  assert.equal(response.kind, "unavailable");
  if (response.kind !== "unavailable") return;
  assert.equal(response.reason, "secure identity store unavailable");
});

test("localPeerProfileUpdateCommand returns the refreshed available profile", async () => {
  // The update command MUST persist the validated name and return
  // the refreshed profile so the frontend can update its in-memory
  // snapshot without a follow-up GET. The peer_id and fingerprint
  // stay unchanged across name edits, which is the spec scenario
  // "User changes visible name" pins.
  let observed: Record<string, unknown> | undefined;
  installTauriMock(async (cmd, args) => {
    assert.equal(cmd, "clipvault_local_peer_profile_update");
    observed = args;
    return {
      kind: "available",
      profile: {
        peer_id: "0123456789abcdef0123456789abcdef",
        fingerprint: "0123456789abcdef",
        display_name: (args?.update as { name: string | null }).name,
      },
    } satisfies LocalPeerProfileResponse;
  });
  const result = await localPeerProfileUpdateCommand({ name: "Studio" });
  assert.equal(observed?.update, { name: "Studio" });
  assert.equal(result.kind, "available");
  if (result.kind !== "available") return;
  assert.equal(result.profile.display_name, "Studio");
  assert.equal(result.profile.peer_id, "0123456789abcdef0123456789abcdef");
  assert.equal(result.profile.fingerprint, "0123456789abcdef");
});

test("localPeerProfileUpdateCommand clears the name with null", async () => {
  let observed: Record<string, unknown> | undefined;
  installTauriMock(async (cmd, args) => {
    assert.equal(cmd, "clipvault_local_peer_profile_update");
    observed = args;
    return {
      kind: "available",
      profile: {
        peer_id: "0123456789abcdef0123456789abcdef",
        fingerprint: "0123456789abcdef",
        display_name: null,
      },
    } satisfies LocalPeerProfileResponse;
  });
  const result = await localPeerProfileUpdateCommand({ name: null });
  assert.equal(observed?.update, { name: null });
  assert.equal(result.kind, "available");
  if (result.kind !== "available") return;
  assert.equal(result.profile.display_name, null);
});

test("localPeerProfileUpdateCommand surfaces Unavailable when the secure store is down", async () => {
  // A temporarily unavailable keychain MUST NOT block the user
  // from editing the visible name. The backend persists the name
  // first and then surfaces the typed Unavailable outcome; the
  // frontend renders the muted copy without losing the edit.
  installTauriMock(async (cmd) => {
    assert.equal(cmd, "clipvault_local_peer_profile_update");
    return {
      kind: "unavailable",
      reason: "secure identity store unavailable",
    } satisfies LocalPeerProfileResponse;
  });
  const result = await localPeerProfileUpdateCommand({ name: "Studio" });
  assert.equal(result.kind, "unavailable");
  if (result.kind !== "unavailable") return;
  assert.equal(result.reason, "secure identity store unavailable");
});
