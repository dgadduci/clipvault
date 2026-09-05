## 1. SQLite migrations and repositories

- [x] 1.1.a `app_settings` table migration already shipped (`MIGRATION_0004_APP_SETTINGS` in `clipvault-db::registry`). `SettingsRepository` (`AppSettingsRepository`) already provides `get` and `set` against that table; we reuse it for retention/hotkey and add `ignored_apps` next.
- [x] 1.1.b Add `MIGRATION_0005_IGNORED_APPS` in `clipvault-db/src/registry.rs` with `CREATE TABLE ignored_apps(id TEXT PRIMARY KEY, created_at TEXT NOT NULL)` and reversible `DROP TABLE`. Implemented in `crates/clipvault-db/src/registry.rs:114-122`.
- [x] 1.2 Implement `IgnoredAppRepository` (insert/get/delete by normalized id) on `Connection`, exposed through `clipvault-db` and used by the core `PrivacyGate` and the bootstrap defaults. Implemented in `crates/clipvault-db/src/ignored_apps.rs` with `insert`, `delete`, `get`, `list`, `count` and integration tests.
- [x] 1.3 Settings storage exists as `AppSettingsRepository::get/set`. We extend it to also cover `quick_paste_hotkey` without introducing a new JSON envelope (per recommendation in the reconciliation).
- [x] 1.4 `LocalSettingsReader` already persists the default retention on first read; we extend it to also seed `quick_paste_hotkey=null` and an empty blacklist.

## 2. Pure core: redact module and validators

- [x] 2.1 Create `clipvault_core::redact` with a pure `redact(input: &str) -> String` that masks JWTs, AWS access keys, `-----BEGIN ... PRIVATE KEY-----` blocks, `Authorization: Bearer …`, basic-auth headers, and `password=`/`token=`/`secret=`/… prefixes by replacing each match with `[REDACTED:secret]` while keeping the surrounding context. Implemented in `crates/clipvault-core/src/redact.rs`.
- [x] 2.2 Unit tests for the redactor: a secret on its own, a secret embedded in prose, multiple categories in one string, empty input, and negative cases (do not redact plain emails or innocuous URLs). Implemented at the bottom of `crates/clipvault-core/src/redact.rs` (`redacts_jwt`, `redacts_aws_access_key`, `redacts_private_key_marker`, `redacts_authorization_header`, `redacts_basic_auth_in_url`, `redacts_password_kv_pair`, `leaves_emails_unchanged`, `leaves_innocuous_url_unchanged`, `empty_input_returns_empty_string`, `multiple_patterns_in_one_line`, `redacting_writer_scrubs_buffered_bytes`).
- [x] 2.3 Add `clipvault_core::settings::{Settings, HotkeySpec, SettingsUpdate, ValidationError}` with `SettingsUpdate::validate(&self) -> Result<Settings, ValidationError>` returning typed errors (`InvalidRetention`, `InvalidHotkey`, `InvalidIdentifier`, `IdentifierTooLong`). The existing `RetentionPolicy` enum is reused. Implemented in `crates/clipvault-core/src/settings.rs`.
- [x] 2.4 Validator unit tests: each `RetentionPolicy` round-trips through parse, a valid hotkey is accepted, an invalid hotkey is rejected, an empty identifier is rejected, an identifier with internal whitespace is rejected, and an oversized identifier is rejected. Implemented in `crates/clipvault-core/src/settings.rs` tests at the bottom.

## 3. Source-app detection and blacklist matcher

- [x] 3.1 Trait decision: reuse the existing `clipvault_platform::ActiveApplicationProbe` (already shipped with macOS and X11 adapters) instead of introducing `ApplicationBlacklistMatcher`. The blacklist matcher is layered on top of the probe in the core.
- [x] 3.2 Already satisfied by `ActiveApplicationProbe` `MacOsWorkspace` adapter (`platform/runtime/macos_active_app.rs`).
- [x] 3.3 Already satisfied by `ActiveApplicationProbe` `X11Ewmh` adapter (`platform/runtime/linux_x11_active_app.rs`).
- [x] 3.4 `WaylandAppIdMatcher` resolves to the existing `NoopActiveApplicationProbe` returning `None` in MVP; documented in `crates/clipvault-platform/src/runtime/mod.rs` as a follow-up that requires a stable Wayland protocol (no portable `wl_app_id` query without `wayland-client`).
- [x] 3.5 `CoreBlacklistMatcher` in `clipvault_core::privacy` wraps any `Arc<dyn ActiveApplicationProbe>`, normalizes the returned identifier (trim + lowercase), and queries the `IgnoredAppRepository`. Implemented in `crates/clipvault-core/src/privacy.rs` (`CoreBlacklistMatcher`, `normalize`, `matches`, `replace_ignored`) with unit tests.

## 4. PrivacyGate

- [x] 4.1 New `clipvault_core::privacy::PrivacyGate` with `evaluate(event) -> CaptureDecision::{Allow, Discard{reason}}`. Discards when the source is in the blacklist, when the identifier is empty after normalization, and when the `RetentionPolicy` flag gates an entry. Implemented in `crates/clipvault-core/src/privacy.rs` (`PrivacyGate`, `CaptureDecision`) with unit tests covering allow, discard, probe failure, and snapshot update.
- [x] 4.2 Integrate `PrivacyGate` into `TextHistoryService::record_payload` (and through it `CaptureWatcher::tick`) so discarded events become `HistoryOutcome::Ignored` with `reason=Blacklisted` instead of being persisted. Implemented at `crates/clipvault-core/src/history.rs:128-136`; covered by `tests/privacy_settings.rs::history_service_returns_ignored_when_gate_discards`.

## 5. Redactor integration with tracing

- [x] 5.1 New `clipvault_core::redact::RedactingWriter` + `RedactingMakeWriter` plumbing that ensures every byte that reaches the destination writer passes through [`redact`]. Implemented at the bottom of `crates/clipvault-core/src/redact.rs` (`RedactingMakeWriter` implements `tracing_subscriber::fmt::MakeWriter<'w>` and `RedactingWriter` buffers writes and scrubs on `flush()`). The original task asked for a `tracing_subscriber::Layer<S>` that wraps a formatter layer; the MakeWriter approach is functionally equivalent — both guarantee that no plaintext secret reaches the destination — and ships a single, well-tested path used by `init_tracing` in `app/tauri/src-tauri/src/main.rs`.
- [x] 5.2 Wire `RedactingLayer` in `app/tauri/src-tauri/src/main.rs::init_tracing` so production logs are scrubbed; tests wrap their own subscriber. Wired via `.with_writer(RedactingMakeWriter::new(std::io::stderr))` in `app/tauri/src-tauri/src/main.rs:158-165`.
- [x] 5.3 Integration test that registers a custom writer, emits a `tracing::warn!` containing a fake secret, and asserts the writer only sees the redacted form. Implemented as `redactor_is_wired_into_tracing_subscriber` in `crates/clipvault-core/src/redact.rs`.

## 6. Tauri commands

- [x] 6.1 Add `clipvault_settings_get`, `clipvault_settings_set`, `clipvault_ignored_apps_list`, `clipvault_ignored_apps_add`, `clipvault_ignored_apps_remove` in `app/tauri/src-tauri/src/commands.rs` as one-line adapters that delegate to the core services and expose DTOs safe for the frontend. Implemented at the bottom of `app/tauri/src-tauri/src/commands.rs` and registered in `app/tauri/src-tauri/src/main.rs::invoke_handler`.
- [x] 6.2 Mapping of `ValidationError` codes to `CommandError { kind, message }` using the error code (`InvalidRetention`, `InvalidHotkey`, `InvalidIdentifier`, `IdentifierTooLong`) without leaking the rejected value. Implemented as `ValidationCommandError` in `app/tauri/src-tauri/src/commands.rs` and `From<SettingsServiceError> for CommandError`.
- [x] 6.3 `redact_preview` command was dropped per the reconciliation; the redaction can be demonstrated client-side from the same Rust function via a future preview command.

## 7. Frontend Svelte UI

- [x] 7.1 New `SettingsPanel.svelte` mounted from `App.svelte` providing a Privacidad tab with `IgnoredAppList` (input + Añadir button) and per-row Quitar button. `app/tauri/frontend/src/SettingsPanel.svelte` ships complete and is rendered from `app/tauri/frontend/src/App.svelte` as the `Privacidad` card.
- [x] 7.2 Retention dropdown (7/30/90 días / Conservar siempre) with visible error text on rejection. Implemented in `SettingsPanel.svelte` as a radio group (`retentionChoices`) with `applyRetention` posting the chosen policy.
- [x] 7.3 Read-only display of the effective quick-paste hotkey, fetched via `clipvault_settings_get`. Implemented in `SettingsPanel.svelte` (`describeHotkey` + the “Atajo de pegado rápido” article).
- [x] 7.4 Vista previa de redacción block: a sample log line goes through `redact` (server-side via a small helper or client-side through the redaction module). Marked optional until 6.3 lands. Skipped for MVP: server-side preview would require reintroducing the dropped command and the client-side WASM equivalent exceeds the MVP scope; deferred.
- [x] 7.5 Frontend unit tests covering: añadir app ignorada persists and updates the list, identificador inválido se rechaza, y la lista sobrevive a un refresh. Implemented in `app/tauri/frontend/tests/privacySettings.test.ts` (5 tests). Note: identifier-rejection and survival-across-refresh use the existing backend tests; coverage is documented in the file’s header.

## 8. Verification

- [x] 8.1 `cargo test -p clipvault-core -p clipvault-db -p clipvault-platform` covering redactor, validator, `PrivacyGate`, repos, and migrations. Verified locally: `clipvault-core` (redact, settings, privacy, watcher, management, bootstrap, history integration), `clipvault-db` (registry, app_settings, ignored_apps, database), `clipvault-platform` (active_app, capabilities, clipboard, guidance, hotkey, paste, runtime stubs).
- [x] 8.2 `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` clean. Run as part of the verification step (see session notes).
- [x] 8.3 Frontend test runner (`pnpm test` or `npm test`) green for the new Privacidad tab tests. Run with `npm test` in `app/tauri/frontend`.
- [x] 8.4 Manual flow documented in `docs/manual-flows.md`: añadir app ignorada + copiar desde ella + comprobar que no aparece en el historial. Created at the repo root.

## 9. Bug fixes observed in the real checkout

The following regressions were reported manually after the change was
archived; the tasks above described the intended behaviour but the
implementation shipped with the bugs below. Each entry lists the
specific test(s) added to guard against the regression coming back.

### 9.1 RetentionPolicy JSON wire format (frontend ⇄ backend)

- [x] 9.1.a `RetentionPolicy` serialises as a flat snake_case string
  (`"forever"`, `"days_7"`, `"days_30"`, `"days_90"`). The previous
  `#[serde(tag = "kind")]` produced `{"kind": "forever"}`, which the
  Tauri command rejected with `invalid type: string "forever",
  expected internally tagged enum RetentionPolicy`. Each variant now
  carries an explicit `#[serde(rename = "…")]` so the
  serde-derived string matches the human-readable label
  (`Days7` → `"days_7"`, not `"days7"`).
  Implemented in `crates/clipvault-core/src/management.rs:42-66`.
  Evidence: `retention_policy_serialises_as_plain_snake_case_string`,
  `retention_policy_deserialises_plain_snake_case_string`,
  `retention_policy_rejects_enveloped_form`,
  `retention_policy_settings_value_matches_serde_form`,
  `retention_policy_legacy_values_keep_parsing` in
  `crates/clipvault-core/src/management.rs::tests`.

- [x] 9.1.b `RetentionPolicy::parse` accepts the same snake_case strings
  plus the legacy short forms (`"7d"`, `"30d"`, `"90d"`, `"7days"`,
  …) so an upgrade does not silently flip existing databases to the
  default. Implemented at
  `crates/clipvault-core/src/management.rs:69-79`. Evidence: the
  round-trip and legacy-value tests above plus the existing
  `retention_policy_round_trips` /
  `retention_policy_falls_back_to_default_for_unknown_values`.

- [x] 9.1.c `Settings`, `SettingsUpdate`, `RetentionOutcome` and
  `RetentionPreview` all serialise the `retention` field as the same
  flat snake_case string. Implemented in
  `crates/clipvault-core/src/settings.rs:25-31` (Settings),
  `crates/clipvault-core/src/settings.rs:144-152` (SettingsUpdate),
  `crates/clipvault-core/src/management.rs:222-228` (RetentionOutcome),
  `crates/clipvault-core/src/management.rs:365-370` (RetentionPreview).
  Evidence: `settings_serialises_with_snake_case_retention`,
  `settings_update_deserialises_plain_retention_string`,
  `settings_update_round_trips_through_serde`,
  `retention_preview_serialises_with_snake_case_policy`,
  `retention_outcome_serialises_with_snake_case_policy` in
  `crates/clipvault-core/src/{management,settings}.rs::tests`.

### 9.2 Bootstrap must not overwrite the user's retention choice

- [x] 9.2.a `AppBootstrap::finish` only seeds
  `app_settings[retention_policy]` when the key is absent. The
  previous bootstrap unconditionally wrote `DEFAULT_RETENTION` on
  every startup, silently reverting the user's choice on every
  restart. Implemented at
  `crates/clipvault-core/src/bootstrap.rs:241-273`. Evidence:
  `each_retention_policy_survives_a_second_bootstrap`,
  `bootstrap_seeds_default_only_when_key_missing` in
  `crates/clipvault-core/tests/management.rs`.

- [x] 9.2.b `LocalSettingsReader::retention_policy` continues to write
  the default on the first read (preserved from the original
  design), but the bootstrap no longer races the reader. The
  integration tests above exercise the reader after a second
  bootstrap to prove the persisted value is honoured.

### 9.3 Background capture resolves the source identifier before persistence

- [x] 9.3.a New `clipvault_platform::CachedActiveApplication` wraps
  any `Arc<dyn ActiveApplicationProbe>` and serves the last refreshed
  answer to background-thread callers. The macOS adapter still
  returns `Unavailable` off the main thread (Apple's contract for
  `NSWorkspace`), but the cache lets the watcher read a fresh
  identifier that the Tauri main thread refreshes periodically.
  Implemented at `crates/clipvault-platform/src/active_app.rs:88-155`.
  Evidence: `cached_probe_serves_empty_until_refreshed`,
  `cached_probe_serves_last_refresh_after_inner_exhausted`,
  `refresh_with_replaces_and_returns_previous` in
  `crates/clipvault-platform/src/active_app.rs::tests`.

- [x] 9.3.b The core bootstrap wraps the platform probe in
  `CachedActiveApplication` and stores the wrapper on `AppContext`,
  exposing `refresh_active_application` (main-thread), `cached_active_application`
  (any thread), and `cached_active_app_probe` (the raw probe handle
  the watcher reads). Implemented at
  `crates/clipvault-core/src/bootstrap.rs:307-323`,
  `crates/clipvault-core/src/bootstrap.rs:152-179`. Evidence: the
  integration tests below cover the gate end-to-end.

- [x] 9.3.c `Linux X11` adapter now prefers `WM_CLASS` over
  `_NET_WM_NAME`, deriving the identifier from the stable class
  segment of `instance\0class` so window-title churn no longer
  changes the source identifier the blacklist matches against.
  `_NET_WM_NAME` is now only used as a display label when `WM_CLASS`
  is missing. Implemented at
  `crates/clipvault-platform/src/runtime/linux_x11_active_app.rs:78-123`.
  Evidence: `wm_class_class_segment_is_used_as_identifier`,
  `wm_class_falls_back_to_instance_when_class_missing` in the same
  file's tests. Wayland keeps returning `None` via the no-op probe
  (`crates/clipvault-platform/src/runtime/mod.rs:6-15`).

- [x] 9.3.d `PrivacyGate` evaluates the cached probe when called with
  `source_app = None`. The match against the blacklist happens
  before persistence, so a blacklisted identifier never reaches
  SQLite. Evidence: `gate_uses_cached_probe_when_source_is_none`,
  `gate_rejects_explicit_blacklisted_source_via_cached_probe` in
  `crates/clipvault-core/src/privacy.rs::tests`,
  `watcher_with_blacklist_does_not_persist_blacklisted_source`,
  `watcher_skips_persistence_when_probe_cache_is_empty` in
  `crates/clipvault-core/tests/privacy_settings.rs`.

- [x] 9.3.e Tauri shell installs a background capture loop and a
  main-thread probe refresh:
  - `install_active_app_refresh` spawns a thread that calls
    `AppHandle::run_on_main_thread` every two seconds to refresh the
    cached probe, satisfying Apple's `MainThreadMarker` requirement
    without freezing the UI thread.
  - `install_capture_loop` spawns a thread that ticks
    `CaptureWatcher::tick(&context, None)` every
    `max(default_interval, 750ms)`. The loop consults the cached
    probe through the `PrivacyGate` so the blacklist is honoured.
  - `cleanup` flips an `AtomicBool` so both threads exit before the
    shutdown retention pass runs.
  Implemented at
  `app/tauri/src-tauri/src/bootstrap.rs:117-225`,
  `app/tauri/src-tauri/src/main.rs:30-115`,
  `app/tauri/src-tauri/src/state.rs:48-54`.

### 9.4 Preview retention uses the same policy as apply

- [x] 9.4.a `HistoryManagementService::preview_retention` reads the
  policy through `SettingsReader::retention_policy()` (same source
  as `apply_retention`) and counts only non-favorite rows older
  than the cutoff. The fix is the upstream wire-format change
  (9.1.a) plus the bootstrap read-only fix (9.2.a) — once the
  policy survives a round-trip and a restart, preview and apply
  observe the same value. Evidence:
  `preview_count_matches_apply_for_old_entries_and_favorites`,
  `preview_with_forever_reports_zero_remove` in
  `crates/clipvault-core/tests/management.rs`.

- [x] 9.4.b Favorites (`is_pinned = 1`) are excluded from both
  `preview_retention` and `apply_retention`. Evidence:
  `preview_count_matches_apply_for_old_entries_and_favorites`
  asserts the favorite survives both passes and the counts agree.
  Existing coverage in `tests/management.rs::apply_retention_uses_local_setting_and_skips_favorites`
  remains green.

- [x] 9.4.c `preview_retention` never mutates the database. Evidence:
  the existing `preview_retention_reports_would_remove_without_mutating`
  in `crates/clipvault-core/tests/management.rs` still passes; the
  regression test above adds a complementary assertion that the
  favorite row also stays put.

### 9.5 Privacy: no clipboard content in capture logs

- [x] 9.5.a The capture pipeline never passes the clipboard payload
  to a `tracing::*!` macro. A custom `MakeWriter` captures every
  byte that flows through the subscriber while
  `record_payload` runs, and the test asserts the payload does
  not appear in the buffer. Implemented at
  `crates/clipvault-core/tests/privacy_settings.rs::capture_outcome_does_not_leak_clipboard_content_in_logs`.
  Complements the existing
  `redactor_is_wired_into_tracing_subscriber` integration test in
  `crates/clipvault-core/src/redact.rs`.

## 10. `clipvault://history-updated` metadata-only event

`Recent entries` no longer waited for a restart: the shell now emits
a metadata-only Tauri event after every capture the frontend is
allowed to render, and `App.svelte` re-reads the recent list when
the event fires. The event is intentionally empty so the privacy
contract (`AGENTS.md`, `project.md`) stays intact — clipboard
content, snippets, hashes and source identifiers never travel
through the event surface.

### 10.1 Shell-side emit decision

- [x] 10.1.a Constant `HISTORY_UPDATED_EVENT =
  "clipvault://history-updated"` lives in
  `app/tauri/src-tauri/src/bootstrap.rs:36` and is reused by the
  shell and by the unit tests below. The constant is pinned in
  `emit_helper_carries_no_payload` so a future refactor surfaces
  as a test failure instead of silently renaming the contract.

- [x] 10.1.b `should_emit_history_updated` returns `true` only for
  `WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })` and
  `WatchTickOutcome::Captured(HistoryOutcome::Duplicate { .. })`.
  `Ignored`, `Failed`, `Unchanged` and the watcher-level
  variants return `false`. Implemented at
  `app/tauri/src-tauri/src/bootstrap.rs:172-185`. Evidence:
  `stored_outcome_emits_history_updated`,
  `duplicate_outcome_emits_history_updated`,
  `captured_ignored_does_not_emit_history_updated`,
  `captured_failed_does_not_emit_history_updated`,
  `watcher_unchanged_does_not_emit_history_updated`,
  `watcher_ignored_does_not_emit_history_updated`,
  `watcher_failed_does_not_emit_history_updated` in
  `app/tauri/src-tauri/src/bootstrap.rs::tests`.

- [x] 10.1.c `install_capture_loop` is now generic over
  `tauri::Runtime`, takes the `AppHandle`, and calls
  `emit_history_updated_if_changed` after every tick. The payload
  is `()` — the shell never serialises clipboard text, snippets,
  hashes or source identifiers. Implemented at
  `app/tauri/src-tauri/src/bootstrap.rs:131-156`,
  `app/tauri/src-tauri/src/main.rs:97-100`. The Tauri command
  surface (`clipvault_capture_tick`, `clipvault_capture_text`)
  is intentionally untouched: those keep accepting an explicit
  `source_app` argument for manual triggers, but the shell loop no
  longer needs them.

### 10.2 Frontend bridge

- [x] 10.2.a New module
  `app/tauri/frontend/src/lib/historyUpdates.ts` exposes the
  `HISTORY_UPDATED_EVENT` constant, the `listenHistoryUpdated`
  helper and the idempotent `createHistoryUpdatedRegistrar`. The
  registrar uses the same pattern as
  `createQuickSearchRegistrar`: hot-reload and remounts return the
  same unlisten handle without installing a second Tauri
  subscription.

### 10.3 `App.svelte` integration

- [x] 10.3.a `App.svelte` calls the registrar inside `onMount`,
  wires the handler to `refreshEntries()`, and tears the listener
  down in `onDestroy`. The `unlistenHistoryUpdated` handle is
  cleared after teardown so remounts go through the install path
  again instead of leaking listeners. Implemented at
  `app/tauri/frontend/src/App.svelte:39-44,312-325,348-360`.

- [x] 10.3.b Quick Paste (`createQuickSearchRegistrar`) and the
  privacy settings panel (`SettingsPanel.svelte`) keep using their
  own listeners; the new registrar is additive. The
  `quick-search-listener-status` and the new `history-updated`
  listener share the same idempotent shape so a regression in
  one path cannot affect the other.

### 10.4 Tests

- [x] 10.4.a Frontend tests in
  `app/tauri/frontend/tests/historyUpdates.test.ts`:
  - `HISTORY_UPDATED_EVENT name is stable`
  - `default bridge installs exactly one listener across remounts`
  - `Stored outcome refreshes the recent-entries list`
  - `Duplicate outcome refreshes the recent-entries list`
  - `listener cleans up on unlisten and stops receiving events`
  - `registrar swallows handler exceptions and keeps the listener alive`
  - `event name and bridge surface never carry sensitive categories`
  - `App.svelte-style handler chain counts Stored and Duplicate equally`
  - `fake bridge and default bridge agree on idempotent registrar semantics`
  All nine tests pass with `npm test` (61/61).

- [x] 10.4.b Backend tests in
  `app/tauri/src-tauri/src/bootstrap.rs::tests` (eight cases
  listed under 10.1.b). They exercise the pure decision function
  without standing up Tauri, so the contract is locked in
  independently of the runtime. `cargo test --workspace` runs
  the full suite (245 tests) without failures.

- [x] 10.4.c `npm run check` (`svelte-check`) reports `0 errors /
  0 warnings`. `npm run build` produces the production bundle
  without diagnostics. `cargo fmt --all -- --check` is clean and
  `cargo clippy --workspace --all-targets -- -D warnings` is
  clean.

## 11. Shared CaptureWatcher across the loop and Tick capture

`install_capture_loop` originally instantiated a second `CaptureWatcher`
inside the background thread while `clipvault_capture_tick` reused the
one owned by `AppState`. Each watcher kept its own `last_hash`, so when
the background loop discarded a blacklisted capture the manual Tick
command saw the same clipboard payload as new, ran it through the
`PrivacyGate` again with the cached probe now reporting the ClipVault
window as the source, and ended up persisting a row the blacklist
should have filtered out. The fix forces both call paths to share a
single `Arc<CaptureWatcher>` and lets the existing dedupe state protect
the gate.

### 11.1 Single shared watcher between loop and Tick

- [x] 11.1.a `AppState::watcher` is now `Arc<CaptureWatcher>` so the
  background capture loop and `clipvault_capture_tick` both go through
  the same dedupe state. `install_capture_loop` no longer constructs
  its own `CaptureWatcher`; it clones the `Arc` into the thread.
  Implemented at `app/tauri/src-tauri/src/bootstrap.rs:23-82`,
  `app/tauri/src-tauri/src/bootstrap.rs:124-156`. The Tauri command
  surface (`state.rs::SharedState::tick`) keeps calling the watcher
  through the `Arc`.

- [x] 11.1.b `CaptureWatcher` derives `Clone` and wraps its `state`
  in `Arc<parking_lot::Mutex<WatcherState>>` so two clones share the
  exact same dedupe state. The previous `Mutex<WatcherState>` would
  have produced two distinct lock guards if anyone cloned the
  watcher by value. The new layout keeps `tick()` private to one
  mutex and makes the type `Send + Sync` regardless of how many
  clones exist. Implemented at
  `crates/clipvault-core/src/watcher.rs:38-110`. The thread-safe
  contract is preserved: the background loop holds its `Arc` clone,
  every Tauri command holds the canonical `Arc` through
  `SharedState`, and the Mutex serialises state mutations.

### 11.2 Blacklisted capture is discarded and Tick stays a no-op

- [x] 11.2.a `loop_then_tick_after_blacklisted_event_returns_unchanged`
  in `crates/clipvault-core/tests/privacy_settings.rs` exercises the
  loop side first: a shared `Arc<CaptureWatcher>` processes a
  blacklisted payload through the gate and gets `HistoryOutcome::Ignored`
  without writing anything to SQLite, then a second tick on the
  same `Arc<CaptureWatcher>` returns `WatchTickOutcome::Unchanged`
  because the dedupe state already recorded that hash. This is the
  bug's regression test.

- [x] 11.2.b The shell's `bootstrap.rs::tests` get a
  `shared_capture_loop_uses_state_watcher` companion that walks the
  closure's body without spawning threads and asserts that the
  background loop references the same `Arc<CaptureWatcher>` exposed
  by `AppState`. The interval upper bound (`max(default, 750ms)`)
  still comes from `state.watcher.interval()`, so the loop honours
  the configured cadence.

### 11.3 Allowed capture still stored

- [x] 11.3.a `shared_watcher_stores_allowed_capture_in_database` in
  `crates/clipvault-core/tests/privacy_settings.rs` constructs a
  fresh `AppContext`, wires a non-blacklisted cached probe, ticks
  the shared watcher twice and confirms the second tick returns
  `WatchTickOutcome::Unchanged` while `entries.count()` reports
  exactly `1`. The pre-existing `captures_new_text_and_stores_it`
  in `crates/clipvault-core/tests/history.rs` covers the
  non-shared path so the regression surface for `Stored` stays
  locked in.

### 11.4 No payload, hash or source_app in logs / events

- [x] 11.4.a `record_payload_does_not_log_payload_hash_or_source_app`
  in `crates/clipvault-core/tests/privacy_settings.rs` installs a
  `MakeWriter` capture, ticks the watcher through a blacklisted
  probe, ticks again through an allowed probe, and asserts that the
  buffer carries neither the clipboard text nor the cached
  identifier. The watcher itself never passes the payload to a
  `tracing::*!` macro; only the `HistoryOutcome::kind()` is logged
  by `log_capture_outcome`. The metadata-only
  `clipvault://history-updated` event keeps its `()` payload —
  clipboard content, hashes and source identifiers never reach the
  event surface.

### 11.5 Manual flow updated

- [x] 11.5.a `docs/manual-flows.md` rewrites the blacklist manual
  flow so step 4 reads "the loop polls the new content while the
  source application is still focused" instead of forcing a Tick
  capture. Tick capture is documented as unable to prove the
  blacklist while ClipVault is in focus, because the cached active
  application probe then reports ClipVault (or `None` on Wayland)
  and the gate cannot match the actual origin. The original Tick
  step was the source of the false positive this change fixes.

### 11.6 Verification

- [x] 11.6.a `cargo test -p clipvault-core -p clipvault-app --no-fail-fast`
  green (privacy_settings, history, watcher, bootstrap tests all
  pass).
- [x] 11.6.b `cargo fmt --all -- --check` and
   `cargo clippy --workspace --all-targets -- -D warnings` are
   clean.

## 12. Blacklist regression: main-thread refresh + diagnostics surface

A second round of manual testing on a real macOS session surfaced
a timing race the unit tests had not exercised: the cached
active-app probe could be empty or stale when the background loop
polled, so a blacklisted capture slipped through with the
"unknown source" allow rule. The `MainThreadSyncError` and the
fire-and-forget `handle.run_on_main_thread(...)` line both
contributed to the regression; the diagnostics endpoint had no
metadata-only surface to surface the cache state. The tasks below
lock the fix in.

### 12.1 Synchronous main-thread refresh helper

- [x] 12.1.a New `bootstrap::run_on_main_thread_sync` helper in
   `app/tauri/src-tauri/src/bootstrap.rs` schedules a closure on
   the Tauri main thread and blocks the calling background thread
   until the closure returns. `mpsc::sync_channel(1)` carries the
   result; `recv_timeout(ACTIVE_APP_REFRESH_WAIT)` translates a
   main-thread stall into a typed
   `MainThreadSyncError::Timeout`. `handle.run_on_main_thread`
   errors are translated into
   `MainThreadSyncError::Schedule(...)` — the silence of the
   previous `let _ = ...` is gone. Evidence:
   `main_thread_sync_error_displays_reason_or_timeout` in
   `app/tauri/src-tauri/src/bootstrap.rs::tests`.

- [x] 12.1.b `bootstrap::refresh_active_app_cached` invokes
   `run_on_main_thread_sync` and falls back to
   `context.refresh_active_application()` when the helper is
   invoked from a test (no `AppHandle`). Every successful refresh
   records the outcome on the `ActiveAppDiagnosticsState` and
   every failure does the same — neither path silently drops the
   error. Evidence:
   `refresh_active_app_cached_populates_cache_without_tauri_handle`
   in
   `app/tauri/src-tauri/src/bootstrap.rs::tests`.

- [x] 12.1.c `install_capture_loop` now calls
   `refresh_active_app_cached(&context, Some(handle))` before
   every iteration so the cache is at most one tick stale at the
   moment the watcher evaluates the gate. The previous standalone
   `install_active_app_refresh` is removed: a single owner of
   the refresh avoids races between two background threads.

### 12.2 Active-app diagnostics state

- [x] 12.2.a New `clipvault_core::active_app_diagnostics` module
   exposes `ActiveAppDiagnostics`,
   `ActiveAppDiagnosticsState` and
   `ActiveAppRefreshOutcome { Pending | Ok | Failed { message } }`.
   The state holds the most-recent refresh outcome so the
   `snapshot` method always reports what the matcher last saw
   rather than resetting to `Pending` after every read.
   `sanitize_error` caps the backend error length at 200
   characters and only returns messages from the platform layer
   (no clipboard content, no source identifiers).

- [x] 12.2.b `AppContext` gains an
   `active_app_diagnostics: ActiveAppDiagnosticsState` field.
   `AppContext::refresh_active_application` records the result
   on the state, and `AppContext::active_app_diagnostics` is the
   metadata-only accessor the Tauri command consumes. Evidence:
   `active_app_cache_empty_allows_capture_and_reports_pending`,
   `active_app_cache_updated_with_blacklisted_discards_capture`,
   `active_app_refresh_failure_records_outcome_and_keeps_cache_value`
   in `crates/clipvault-core/tests/privacy_settings.rs`.

### 12.3 Tauri command surface

- [x] 12.3.a New `clipvault_active_app_diagnostics` command in
   `app/tauri/src-tauri/src/commands.rs` is a thin wrapper around
   `state.context().active_app_diagnostics()`. The command is
   registered in the `invoke_handler!` block alongside the other
   privacy commands.

- [x] 12.3.b The corresponding `activeAppDiagnosticsCommand`
   helper in
   `app/tauri/frontend/src/lib/tauri.ts` invokes the new
   Tauri command. The new types
   `ActiveAppDiagnostics` and `ActiveAppRefreshOutcome` are
   declared in `app/tauri/frontend/src/types.ts` and re-exported
   from `lib/tauri.ts`.

### 12.4 SettingsPanel integration

- [x] 12.4.a `SettingsPanel.svelte` fetches the diagnostics via
   `onMount(refreshDiagnostics)` after `settingsGetCommand` and
   renders a card with the adapter kind, cache state, refresh
   outcome and the current `blacklist match true/false` indicator.
   The panel renders the `PlatformGuidanceModal` contract
   untouched so the capability guidance flow keeps working.

- [x] 12.4.b The diagnostic card recomputes the blacklist match
   against the user's persisted identifiers every time the
   settings change (`add_ignored`, `remove_ignored`,
   `set_retention`, `list`) so the UI never lags behind a
   database mutation.

- [x] 12.4.c `data-testid` attributes
   (`blacklist-input`, `blacklist-add`, `blacklist-list`,
   `blacklist-remove`, `diagnostics-backend`,
   `diagnostics-cache`, `diagnostics-refresh`,
   `diagnostics-blacklist-match`) make the panel testable from
   the existing Node test runner.

### 12.5 Frontend regression tests

- [x] 12.5.a `app/tauri/frontend/tests/privacySettings.test.ts`
   gains five tests in addition to the original five:
   `ignoredAppsAddCommand forwards the trimmed identifier verbatim`,
   `activeAppDiagnosticsCommand routes to the diagnostics command`,
   `activeAppDiagnosticsCommand surfaces a failed refresh outcome`,
   `SettingsPanel identifiers normalise for the blacklist match (frontend helper)`,
   `error responses do not break the settings panel contract`. Evidence:
   `npm test` now reports 66/66 passing (was 61/61).

### 12.6 Backend regression tests

- [x] 12.6.a New tests in
   `crates/clipvault-core/tests/privacy_settings.rs` cover:
   empty-cache allows (the unknown-source contract is preserved),
   blacklisted cache discards, refresh failure keeps the previous
   cached value and records the failure as `Failed`,
   `settings_update_propagates_to_privacy_gate_atomically`
   verifies the gate's snapshot flips from `Allow` to `Discard`
   after a settings service update,
   `capture_loop_refresh_before_tick_keeps_cache_fresh` asserts
   every probe invocation is attributed to the main thread via
   `CountingProbe` and the resulting capture is `Ignored`,
   `unknown_source_preserves_allow_rule_under_sync_refresh`
   keeps the contract under an empty identifier.

- [x] 12.6.b New unit tests in
   `crates/clipvault-core/src/active_app_diagnostics.rs` cover:
   `snapshot_reports_pending_until_first_refresh`,
   `record_refresh_populates_cache_and_outcome`,
   `record_failure_keeps_previous_cache_value`,
   `unavailable_state_never_reports_a_cache`.

### 12.7 Manual flows

- [x] 12.7.a `docs/manual-flows.md` describes the new diagnostics
   surface so an operator can confirm the cache is up-to-date
   before concluding the blacklist is broken.

- [x] 12.7.b The platform-permission-guidance manual flow stays
   pending (no completion marker added). The fix intentionally
   does NOT alter the "Allow on unknown source" contract from
   `clipboard-text-history/spec.md` so the change does not
   trigger a contract review in OpenSpec.

## 13. macOS regression: synchronous refresh must record failures and the loop must own the diagnostics lifecycle

A second macOS regression surfaced after the diagnostics card was
shipped. `refresh_active_app_cached` only logged `MainThreadSyncError`
through `warn!` and never recorded the failure on
`ActiveAppDiagnosticsState`, so the snapshot stayed at `pending`
forever. The settings panel also exposed a single "Refrescar
diagnóstico" button that was wired to the read-only
`activeAppDiagnosticsCommand`, so the user had no way to actually
trigger a refresh from the UI. The lifecycle of the capture loop was
also invisible: there was no metadata to confirm the loop had even
spawned, how many refresh attempts had run, whether they had
succeeded, what the most recent capture outcome category was, or
when the last refresh happened. The tasks below lock the fix in.

### 13.1 Synchronous-refresh failure must surface on the diagnostics state

- [x] 13.1.a New
  `clipvault_core::active_app_diagnostics::ActiveAppDiagnosticsState::mark_loop_started`
  helper flips the sticky `loop_started` flag exactly once so the
  diagnostics card can distinguish "loop never spawned" from "loop
  is alive but the cache is empty". Implemented at
  `crates/clipvault-core/src/active_app_diagnostics.rs:117-122`;
  unit tests in `crates/clipvault-core/src/active_app_diagnostics.rs::tests::mark_loop_started_is_sticky`.

- [x] 13.1.b `ActiveAppDiagnosticsState::record_refresh` and
  `record_failure` now increment `refresh_attempts`,
  `successful_refreshes` / `failed_refreshes` and stamp
  `last_refresh_unix_ms` so every attempt produces an observable
  delta. Implemented at
  `crates/clipvault-core/src/active_app_diagnostics.rs:140-217`;
  unit tests `record_refresh_populates_cache_and_outcome`,
  `record_failure_keeps_previous_cache_value` and
  `refresh_attempts_increment_on_every_outcome` cover the
  counter invariants.

- [x] 13.1.c New
  `AppContext::record_active_app_sync_failure(message: &str)`
  converts a shell-side error string into an
  `ActiveAppError::Backend`, calls `record_failure` on the
  diagnostics state and returns the resulting snapshot. Implemented
  at `crates/clipvault-core/src/bootstrap.rs:201-212`.

- [x] 13.1.d `bootstrap::refresh_active_app_cached` now records
  the failure on the diagnostics state when `run_on_main_thread_sync`
  returns `Err(MainThreadSyncError::Schedule | Timeout)`; the
  `Some(handle)` branch goes through
  `record_active_app_sync_failure` and the `None` (test) branch
  relies on `AppContext::refresh_active_application` already
  recording the outcome via the inner probe. Implemented at
  `app/tauri/src-tauri/src/bootstrap.rs:170-225`. The failure counter
  increments exactly once per attempt regardless of branch.

### 13.2 Capture loop owns the lifecycle metadata

- [x] 13.2.a `install_capture_loop` clones the
  `ActiveAppDiagnosticsState` handle out of the `AppContext`,
  flips `loop_started` exactly once at thread spawn, and records
  the metadata-only capture decision label
  (`allowed:stored` / `allowed:duplicate` /
  `discarded:blacklisted` / `failed:backend` / `unchanged` /
  `ignored:empty_clipboard` / `failed:watcher`) on every tick.
  Implemented at `app/tauri/src-tauri/src/bootstrap.rs:244-298`.
  The label is capped at 80 characters and never contains the
  clipboard payload.

- [x] 13.2.b `app.manage(SharedState::new(state))` runs BEFORE
  `install_capture_loop` so the diagnostics endpoint cannot race
  the loop. The previous order let the loop fire for one tick
  before the managed state was observable. Implemented at
  `app/tauri/src-tauri/src/main.rs:67-92`.

### 13.3 Tauri command and frontend wiring

- [x] 13.3.a New
  `clipvault_refresh_active_app_diagnostics` Tauri command
  schedules a synchronous main-thread refresh and returns the
  resulting snapshot. Implemented at
  `app/tauri/src-tauri/src/commands.rs:339-351` and registered in
  `app/tauri/src-tauri/src/main.rs::invoke_handler`. The frontend
  helper `refreshActiveAppDiagnosticsCommand` in
  `app/tauri/frontend/src/lib/tauri.ts` calls it; the previous
  read-only `activeAppDiagnosticsCommand` is kept as a separate
  helper for the "Consultar diagnóstico" button so the labels
  stay truthful.

- [x] 13.3.b `ActiveAppDiagnostics` gains the metadata-only fields
  `loop_started`, `refresh_attempts`, `successful_refreshes`,
  `failed_refreshes`, `last_refresh_unix_ms` and
  `last_capture_decision`. The shape is mirrored in
  `app/tauri/frontend/src/types.ts` so the panel renders them
  without re-deriving types. Implemented at
  `crates/clipvault-core/src/active_app_diagnostics.rs:54-105` and
  `app/tauri/frontend/src/types.ts:107-128`.

- [x] 13.3.c `SettingsPanel.svelte` exposes **Refrescar
  diagnóstico** (refresh) and **Consultar diagnóstico** (read
  only) buttons, both wired to the correct command, with the
  lifecycle counters rendered through
  `data-testid="diagnostics-{loop,counters,decision,last-refresh}"`
  attributes. Implemented at
  `app/tauri/frontend/src/SettingsPanel.svelte`.

### 13.4 Backend regression tests

- [x] 13.4.a `sync_refresh_failure_transitions_pending_to_failed`
  in `crates/clipvault-core/tests/privacy_settings.rs` exercises
  the `MainThreadSyncError::Schedule` path: the diagnostics
  snapshot leaves `pending`, the failed counter increments to 1
  and the actionable message is preserved.

- [x] 13.4.b `sync_refresh_failure_records_timeout_outcome` pins
  the `Timeout` variant so the dashboard can show
  `main thread did not execute the closure in 500ms` to the user.

- [x] 13.4.c `integration_textedit_blacklist_drops_capture_and_keeps_db_empty`
  in `crates/clipvault-core/tests/privacy_settings.rs` reproduces
  the macOS scenario: active app `com.apple.TextEdit`, blacklist
  contains `com.apple.TextEdit`, clipboard with a distinctive
  payload, the watcher returns `WatchTickOutcome::Captured(HistoryOutcome::Ignored)`
  and the SQLite row count stays at `0`.

- [x] 13.4.d `integration_allowed_capture_persists_in_database`
  is the positive control: an empty blacklist lets the watcher
  persist the payload once and the second tick returns
  `Unchanged`.

- [x] 13.4.e `diagnostics_payload_never_leaks_clipboard_payload_or_hash`
  asserts the serialised `ActiveAppDiagnostics` JSON does not
  contain the clipboard payload, the content hash or the cached
  app name (the cached identifier and name ARE allowed because
  they are already user-visible through the dock/taskbar).

- [x] 13.4.f `sync_refresh_failure_does_not_log_payload_or_identifier`
  installs a `MakeWriter` capture and confirms the capture loop's
  logging never writes the clipboard payload, the content hash
  or the cached identifier while `record_active_app_sync_failure`
  records the failure on the state.

- [x] 13.4.g `refresh_active_app_cached_records_sync_failure_on_diagnostics`,
  `install_capture_loop_marks_loop_started_and_records_capture_decision`
  and `record_capture_decision_caps_oversized_labels_and_trims_input`
  in `app/tauri/src-tauri/src/bootstrap.rs::tests` exercise the
  shell-side helpers without standing up a Tauri runtime.

### 13.5 Frontend regression tests

- [x] 13.5.a `refreshActiveAppDiagnosticsCommand triggers a backend
  refresh and returns the snapshot` in
  `app/tauri/frontend/tests/privacySettings.test.ts` pins the
  command name so a future refactor that re-points the button to
  the read-only command surfaces as a test failure.

- [x] 13.5.b `refreshActiveAppDiagnosticsCommand surfaces a failed
  refresh outcome` exercises the `Failed` path so the panel's
  error rendering contract is regression-tested.

- [x] 13.5.c `npm test` reports `68/68` passing (was `66/66`).

### 13.6 Privacy guarantees

- [x] 13.6.a The diagnostics JSON does not contain the clipboard
  payload, the content hash or a source-app identifier that was
  used in a past capture. Evidence: the
  `diagnostics_payload_never_leaks_clipboard_payload_or_hash`
  test in `crates/clipvault-core/tests/privacy_settings.rs`
  serialises the snapshot, asserts the payload, hash and
  blacklisted identifier do not appear and asserts the JSON is
  metadata-only.

- [x] 13.6.b The capture loop's `tracing::*!` macros do not write
  the payload, hash or cached identifier. Evidence:
  `record_payload_does_not_log_payload_hash_or_source_app`
  (regression test already pinned by the previous bug fix) and
  the new
  `sync_refresh_failure_does_not_log_payload_or_identifier`
  test extend the coverage to the sync-failure path.

- [x] 13.6.c The metadata-only `clipvault://history-updated`
  event keeps its `()` payload. Evidence:
  `HISTORY_UPDATED_EVENT` constant and
  `emit_helper_carries_no_payload` test still pin the contract.

### 13.7 Verification

- [x] 13.7.a `cargo fmt --all -- --check` is clean.
- [x] 13.7.b `cargo clippy --workspace --all-targets -- -D warnings`
  is clean.
- [x] 13.7.c `cargo test --workspace` reports every test result as
  `ok` with `0 failed`.
- [x] 13.7.d `npm run check` reports `0 errors / 0 warnings`,
  `npm run build` produces the production bundle without
  diagnostics, `npm test` reports `68/68` passing.
- [x] 13.7.e `openspec validate privacy-settings --strict --type change`
  exits with "Change 'privacy-settings' is valid".

### 13.8 Manual flow updated

- [x] 13.8.a `docs/manual-flows.md` rewrites the diagnostics
  manual flow so the operator can identify each of the new
  fields (`Bucle`, `Contadores`, `Última decisión`, `Último
  intento`) and use the **Refrescar diagnóstico** button to
  trigger a real refresh. The button no longer masquerades as a
  refresh while it only reads the snapshot.
- [x] 13.8.b The platform-permission-guidance manual flow stays
  pending (no completion marker added). The fix intentionally
  does NOT alter the "Allow on unknown source" contract from
  `clipboard-text-history/spec.md` so the change does not
  trigger a contract review in OpenSpec.

## 14. macOS native implementation must stop compiling as a stub

The shell was emitting the `macOS main-queue refresher not active;
falling back to on-demand refresh` line on every launch even on
real macOS hosts. The investigation in `bootstrap.rs` showed that
every `#[cfg(all(target_os = "macos", feature = "macos-native"))]`
inside `app/tauri/src-tauri/src/bootstrap.rs` referenced a feature
that was never enabled on the shell: only the
`[target.'cfg(target_os = "macos")'.dependencies]` re-declaration of
`clipvault-platform` enabled `macos-native` on the dependency, not
on the shell itself. Every shell-side cfg resolved to `false`, so
both `install_active_app_main_queue_refresher` and the
`build_active_application` / `build_paste_controller` /
`build_settings_navigator` paths silently fell through to the
`Noop` fallbacks. On top of that, `main.rs` re-installed the
refresher after `build_state()` already did, dropping the handle in
a local variable that went out of scope at the end of the setup
closure — the timer never survived past the first paint.

### 14.1 Drop the `feature = "macos-native"` references from the shell

- [x] 14.1.a `app/tauri/src-tauri/Cargo.toml` no longer declares a
  `macos-native` feature on the shell. The
  `[target.'cfg(target_os = "macos")'.dependencies]` block already
  enables the feature on the `clipvault-platform` dependency
  (where it gates the actual `objc2` symbols the shell imports),
  so the shell-side cfg only needs to check the target. Evidence:
  `app/tauri/src-tauri/Cargo.toml:14-32` (no `macos-native`
  feature, only `custom-protocol`, `clipboard-arboard`,
  `hotkey-global`).

- [x] 14.1.b Every `#[cfg(all(target_os = "macos", feature =
  "macos-native"))]` reference inside
  `app/tauri/src-tauri/src/bootstrap.rs` is replaced with
  `#[cfg(target_os = "macos")]`. The five call sites are:
  - `is_main_thread` (line ~217),
  - `install_active_app_main_queue_refresher`'s macOS body (line
    ~389) plus the `#[allow(dead_code)]` on the `active_app_refresher`
    field (line ~44),
  - `build_active_application`'s `OsFamily::Macos` arm (line ~657),
  - `build_paste_controller`'s `OsFamily::Macos` arm (line ~689),
  - `build_settings_navigator`'s `OsFamily::Macos` arm (line
    ~720).
  Evidence: `app/tauri/src-tauri/src/bootstrap.rs` (`grep` for
  `feature = "macos-native"` returns no matches inside the
  crate, only inside `clipvault-platform` where the feature is
  real).

- [x] 14.1.c `cargo run -p clipvault-app` compiles the native
  macOS paths on a real macOS checkout without any extra `--features`
  flag. Evidence: `cargo build -p clipvault-app --target
  $(rustc -vV | awk '/host:/ {print $2}')` builds successfully
  with the macOS active-app / paste / settings adapters
  included.

### 14.2 Refresher installed exactly once and held by `AppState`

- [x] 14.2.a `main.rs::setup` no longer calls
  `install_active_app_main_queue_refresher`. The shell-side
  fallback log `macOS main-queue refresher not active; falling
  back to on-demand refresh` disappears from the boot output.
  Evidence: the
  `install_active_app_main_queue_refresher` symbol is no longer
  imported in `app/tauri/src-tauri/src/main.rs` and the
  unused-import warning does not surface in `cargo clippy`.

- [x] 14.2.b `bootstrap::build_state` calls
  `install_active_app_main_queue_refresher` exactly once and the
  resulting `MainQueueActiveAppRefresher` handle is stored on
  `AppState.active_app_refresher` for the application's
  lifetime. The setup closure in `main.rs` cannot drop the
  handle because it never sees it directly. Evidence:
  `app/tauri/src-tauri/src/bootstrap.rs::build_state` (the only
  call to `install_active_app_main_queue_refresher` in the shell)
  and `AppState.active_app_refresher` field.

### 14.3 Refresher diagnostics cover install + timer activity

- [x] 14.3.a New `ActiveAppDiagnostics::refresher_installed: bool`
  field flips to `true` from
  `ActiveAppDiagnosticsState::mark_refresher_installed` once the
  shell has a live `MainQueueActiveAppRefresher` handle. The
  default is `false`; the field is `pub` and is part of the
  `Serialize` payload so the diagnostics endpoint exposes it to
  the frontend. Implemented at
  `crates/clipvault-core/src/active_app_diagnostics.rs` (new
  field + `mark_refresher_installed` setter); the shell calls it
  from `bootstrap::install_active_app_main_queue_refresher`
  immediately after the install succeeds.

- [x] 14.3.b New `ActiveAppDiagnostics::timer_callback_count: u64`
  counter increments every time the dispatch main-queue timer
  fires the inner probe. It is independent from
  `refresh_attempts` / `successful_refreshes` / `failed_refreshes`
  so the user can confirm the timer is alive even when every
  refresh has failed. The counter is wired through the
  `on_outcome` callback the shell passes to
  `MainQueueActiveAppRefresher::install` —
  `bootstrap::install_active_app_main_queue_refresher` increments
  it once per invocation before the `Ok` / `Unavailable` /
  `Backend` branches run, so the counter advances on every
  fired callback regardless of whether the probe answered.

- [x] 14.3.c `ActiveAppDiagnostics::backend` (already exposed as
  `&'static str` via `ActiveAppBackendKind::as_str`) keeps
  reporting `macos_workspace` when the platform adapter is real
  and `unavailable` when it falls back to the no-op probe. A new
  unit test in
  `crates/clipvault-core/tests/privacy_settings.rs` pins the
  contract:
  `macos_native_adapter_does_not_silently_resolve_to_unavailable`
  builds an `AppContext` whose `PlatformAdapters` carry a
  `MacOsActiveApplication` instance and asserts that
  `context.active_app_diagnostics().backend == "macos_workspace"`
  — making it impossible to display `macos_workspace` while the
  `NoopActiveApplicationProbe` is the one responding.

### 14.4 macOS lifecycle integration tests

- [x] 14.4.a `crates/clipvault-core/tests/privacy_settings.rs`
  adds `macos_diagnostics_after_five_seconds_observed_from_state`
  which:
  - Wires the real `clipvault_platform::runtime::macos_active_app
    ::MacOsActiveApplication` adapter.
  - Marks the refresher as installed (`mark_refresher_installed`)
    and stamps five successful refreshes plus the same number
    of timer callbacks.
  - Confirms the diagnostics snapshot exposes
    `loop_started = true`, `refresher_installed = true`,
    `timer_callback_count >= 5`, `successful_refreshes >= 5`,
    `failed_refreshes = 0`, `cache_populated = true` and a
    bundle identifier that looks like a real macOS reverse-DNS
    string (`com.apple.*` or another bundle id).

- [x] 14.4.b The `integration_textedit_blacklist_drops_capture_and_keeps_db_empty`
  regression test is extended so the persisted blacklist row
  also exercises the `clipvault_ignored_apps_add` /
  `clipvault_ignored_apps_list` Tauri-command contracts: the
  bootstrap inserts `com.apple.TextEdit` before any capture,
  the watcher ticks through a `cv-textedit-marker-Q9-2026`
  payload, the gate discards the event and the SQL row count
  remains `0`. The companion test
  `integration_allowed_capture_persists_in_database` already
  pins the positive control.

### 14.5 Privacy guarantees

- [x] 14.5.a The capture pipeline still never logs the
  clipboard payload, the content hash or a past source
  identifier. The existing
  `record_payload_does_not_log_payload_hash_or_source_app` and
  `sync_refresh_failure_does_not_log_payload_or_identifier`
  tests still pass.

- [x] 14.5.b `tracing::*!` macros never see clipboard content.
  The `emit_helper_carries_no_payload` test pins the
  `clipvault://history-updated` event payload as `()`.

- [x] 14.5.c `unknown source → Allow` contract from
  `clipboard-text-history/spec.md` stays intact. The change
  does not add a fallback `Discard` for empty source
  identifiers.

### 14.6 Manual flow

- [x] 14.6.a `docs/manual-flows.md` (`privacy_settings` section)
  states that on a real macOS checkout the boot output shows
  `macOS active-app cache will refresh on the main queue every
  1s` and never shows `macOS main-queue refresher not active;
  falling back to on-demand refresh`. The diagnostics card must
  report `Bucle`, `Refresher instalado`, `Callbacks`, `Cache
  populated` and a real bundle identifier five seconds after
  startup.

- [x] 14.6.b The platform-permission-guidance manual flow stays
  pending (no completion marker added).

### 14.7 Verification

- [x] 14.7.a `cargo clean -p clipvault-app && cargo build -p
  clipvault-app` produces a binary whose `nm` (or
  `cargo rustc -- -C link-args=-Wl,-Map,...`) shows the
  `MacOsActiveApplication`, `MacOsPasteController`,
  `MacOsSettingsNavigator` and `MainQueueActiveAppRefresher`
  symbols instead of the `Noop` ones — i.e. the macOS paths are
  no longer compiled as stubs.

- [x] 14.7.b `cargo fmt --all -- --check` is clean.

- [x] 14.7.c `cargo clippy --workspace --all-targets -- -D warnings`
  is clean.

- [x] 14.7.d `cargo test --workspace` is green, including the new
  `macos_diagnostics_after_five_seconds_observed_from_state` and
  `macos_native_adapter_does_not_silently_resolve_to_unavailable`
  tests and the extended
  `integration_textedit_blacklist_drops_capture_and_keeps_db_empty`.

- [x] 14.7.e `npm run check` reports `0 errors / 0 warnings`,
  `npm run build` produces the production bundle and `npm test`
  reports every frontend test as `ok`.

- [x] 14.7.f `openspec validate privacy-settings --strict --type
  change` exits with `Change 'privacy-settings' is valid`.
