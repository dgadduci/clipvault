## ADDED Requirements

### Requirement: Active-app cache diagnostics

ClipVault SHALL expose, through a metadata-only Tauri command, the state of the cached active-application probe and the outcome of the most recent refresh attempt, so the user can verify the blacklist is reading the same identifier the platform reports.

#### Scenario: Metadata-only diagnostics report

- **WHEN** the GUI requests the active-app diagnostics
- **THEN** ClipVault returns the cached identifier (when populated), the cached display name, the adapter kind (`macos_workspace`, `x11_ewmh`, `unavailable`), the cache-population flag and the outcome of the most recent refresh (`pending`, `ok`, `failed`) — never the clipboard content, the content hash or any prior source identifier

#### Scenario: Pending outcome before first refresh

- **WHEN** the application has booted but the capture loop has not performed a refresh yet
- **THEN** the diagnostics endpoint reports `refresh_outcome = "pending"` so the user knows the cache is empty and the matcher is operating under the "unknown source" contract

#### Scenario: Failed refresh surfaces the actionable reason

- **WHEN** the platform refuses to enqueue the refresh on the main thread, the main thread does not execute the closure inside the configured timeout, or the inner probe returns a backend error
- **THEN** the diagnostics endpoint reports `refresh_outcome.kind = "failed"` and a sanitised reason the user can act on; the previous cached value stays visible so the matcher can still match against it; the diagnostics counters (`refresh_attempts`, `successful_refreshes`, `failed_refreshes`) increment by exactly one per attempt

#### Scenario: Lifecycle counters and metadata

- **WHEN** the diagnostics endpoint is serialised as JSON
- **THEN** the payload includes `loop_started` (sticky flag flipped when the capture loop spawns), `refresh_attempts` (total attempts), `successful_refreshes` (count of `Ok` outcomes), `failed_refreshes` (count of `Failed` outcomes from the inner probe, the shell sync helper or the Tauri scheduler), `last_refresh_unix_ms` (epoch milliseconds of the most recent attempt or `null` if no attempt has happened yet) and `last_capture_decision` (metadata-only label such as `allowed:stored`, `discarded:blacklisted`, `unchanged` or `failed:backend`); none of these fields carry clipboard content, content hashes, snippets or source-app identifiers

#### Scenario: Blacklist match preview in the settings panel

- **WHEN** the settings panel renders the diagnostics card
- **THEN** it shows whether the currently observed identifier matches an entry in the persisted blacklist (true/false) and never suggests that the user "blacklist everything" when the cache is empty

#### Scenario: On-demand refresh from the settings panel

- **WHEN** the user clicks the **Refrescar diagnóstico** button in the settings panel
- **THEN** the frontend invokes the dedicated `clipvault_refresh_active_app_diagnostics` command which schedules a synchronous main-thread refresh, records the outcome on the diagnostics state and returns the resulting snapshot — the panel MUST NOT offer a "refresh" button that only reads the cached snapshot; a separate **Consultar diagnóstico** button is provided for the read-only path so the labels stay truthful

### Requirement: Configurable ignored-application blacklist

ClipVault SHALL provide a local setting that allows the user to add, remove and view application identifiers that must not be captured, and SHALL compare each captured clipboard event against this blacklist before persisting content.

#### Scenario: Add an ignored application

- **WHEN** the user adds an application identifier to the blacklist through the local settings UI or command
- **THEN** the setting is persisted locally in SQLite and applies to subsequent clipboard events from that application

#### Scenario: Remove an ignored application

- **WHEN** the user removes an application identifier from the blacklist
- **THEN** future clipboard events from that application may be captured according to the other active policies, and previously ignored content remains absent from history

#### Scenario: List ignored applications

- **WHEN** the GUI or CLI requests the current blacklist
- **THEN** ClipVault returns the locally stored identifiers ordered by insertion or alphabetical order without contacting any external service

#### Scenario: Clipboard event from ignored application

- **WHEN** the active source application matches a blacklisted identifier
- **THEN** ClipVault does not persist the clipboard content, does not write the value to local diagnostics and does not include it in recent or search responses

#### Scenario: Identify source application across platforms

- **WHEN** the platform exposes a source application identifier (macOS bundle id, X11 WM_CLASS, Wayland app id, etc.)
- **THEN** ClipVault compares the normalized identifier against the blacklist using a deterministic matcher that can be mocked in core tests

#### Scenario: Background loop and manual tick share a watcher

- **WHEN** the background capture loop and any subsequent watcher tick (a Tauri command or a future CLI invocation) coexist
- **THEN** they operate on the same shared `CaptureWatcher` instance so dedupe, `last_hash` and the configured interval are identical for every caller, and neither path can re-evaluate a payload the other has already processed

#### Scenario: Exact identifier echoed from UI to backend

- **WHEN** the user types an identifier in the privacy panel and presses `Añadir`
- **THEN** the value forwarded to the Tauri command is the trimmed string (the panel never lowercases the value); the matcher is the only layer that normalises the identifier and the rejected identifier surfaces through the structured validation error path

### Requirement: Local minimal settings

ClipVault SHALL expose local settings for retention policy, ignored applications and the configured quick-paste hotkey without requiring an account, network call or remote configuration.

#### Scenario: Change a setting

- **WHEN** the user changes one of the supported settings (retention policy, ignored applications, quick-paste hotkey) and saves it through the GUI, CLI or Tauri command
- **THEN** the setting persists across application restarts in the local SQLite database and affects subsequent behavior

#### Scenario: Read effective settings

- **WHEN** the GUI or CLI requests the effective settings
- **THEN** ClipVault returns the persisted values for retention policy, ignored applications and quick-paste hotkey, including defaults when nothing is stored yet

#### Scenario: Invalid setting

- **WHEN** a setting value fails validation (unknown retention policy, malformed hotkey, empty or oversized identifier, unsupported platform value)
- **THEN** ClipVault rejects it with a useful local error that names the field and reason, preserves the last valid value and does not modify the persisted setting

#### Scenario: Retention policy is enforced

- **WHEN** the retention policy is set to a bounded value (e.g. 7 days)
- **THEN** expired entries are removed (or marked expired) on startup and on a periodic sweep, while favorites are kept regardless of retention

#### Scenario: Settings update propagates to PrivacyGate

- **WHEN** the settings service persists a change to the ignored-apps table
- **THEN** the `PrivacyGate`'s in-memory snapshot is updated before the next call returns so the gate's next `evaluate` reflects the new blacklist without requiring a restart

### Requirement: Privacy-preserving diagnostics

ClipVault SHALL keep diagnostics local and SHALL redact or omit clipboard content and sensitive values from logs using a deterministic redactor shared across modules.

#### Scenario: Clipboard error is logged

- **WHEN** a clipboard or database error is written to local diagnostics
- **THEN** the log contains the error category and useful context without the full clipboard content, password, token, private key or other detected sensitive value

#### Scenario: Redactor covers secrets

- **WHEN** a log message contains content the redactor classifies as a likely secret (password, API token, private key, JWT, cookie, session id)
- **THEN** the matched value is replaced with a stable placeholder (e.g. `[REDACTED:secret]`) and the surrounding context is preserved for debugging

#### Scenario: Redactor is testable

- **WHEN** tests exercise the redactor
- **THEN** the redactor is implemented as a pure function in `clipvault-core` that does not require the clipboard, GUI or filesystem and produces the same output for the same input

#### Scenario: No telemetry

- **WHEN** the application is used normally
- **THEN** it does not send usage data, clipboard content, settings, diagnostics or any other information to an external service

#### Scenario: Capture pipeline writes no sensitive payload to logs or events

- **WHEN** the watcher ticks — whether from the background loop or a manual `Tick capture` — and produces an `Allowed`, `Discard`, `Unchanged`, `Duplicate`, `Stored` or `Failed` outcome
- **THEN** neither `tracing` output, the metadata-only `clipvault://history-updated` event, nor any structured log carries the clipboard content, the content hash or the source-application identifier; only outcome categories and non-sensitive context are emitted

#### Scenario: Blacklist diagnostic state never leaks clipboard content

- **WHEN** the diagnostics endpoint is serialised as JSON
- **THEN** the payload contains the cached identifier, the refresh outcome and the cache flag — but never the clipboard content the matcher would have discarded, the content hash derived from such content or a previous blacklist entry beyond what the user already entered
