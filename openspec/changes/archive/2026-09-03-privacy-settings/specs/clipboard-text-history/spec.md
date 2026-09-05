## MODIFIED Requirements

### Requirement: Capture text clipboard changes

ClipVault SHALL capture new text content when the operating system clipboard changes, SHALL ignore non-text content in MVP v0.1 without terminating the application and SHALL ignore text content whose source application matches the configured ignored-application blacklist.

#### Scenario: New text is copied

- **WHEN** a user copies text from an application that is not ignored
- **THEN** ClipVault stores the text in the local history with its capture timestamp

#### Scenario: Non-text content is copied

- **WHEN** a user copies an image, file or unsupported clipboard representation
- **THEN** ClipVault does not create a text history entry and continues running

#### Scenario: Clipboard event from a blacklisted application

- **WHEN** the source application identifier matches an entry in the ignored-application blacklist (whether or not the platform reported the source)
- **THEN** ClipVault does not persist the text content, does not record the full value in local diagnostics, and does not surface the content in recent or search responses

#### Scenario: Source application is unavailable

- **WHEN** the operating system does not provide a source application identifier
- **THEN** ClipVault treats the event as unclassified and the matcher's "unknown source" contract keeps the capture accessible to the persistence path; the failure surface is recorded through the metadata-only diagnostics endpoint instead of forcing a fall-back to Discard

#### Scenario: Clipboard read fails

- **WHEN** the platform clipboard cannot be read or returns malformed data
- **THEN** ClipVault records a local diagnostic event without crashing or corrupting existing history

#### Scenario: Background loop and manual tick share the same dedupe state

- **WHEN** the background capture loop has already evaluated a clipboard change (including events the `PrivacyGate` discards) and the user later forces a watcher tick
- **THEN** the manual tick observes the loop's last seen hash, returns the unchanged outcome and never re-evaluates the discarded payload against the blacklist with a stale source-application snapshot taken after the user clicked the button

#### Scenario: Blacklisted capture must not leak via Tick capture

- **WHEN** a blacklisted application copies text to the clipboard while it is focused
- **THEN** the background loop alone decides whether to persist the event; even if the user switches to ClipVault and forces a tick, the watcher keeps the discard decision, never invokes the persistence path, and the SQLite history remains untouched for that payload

#### Scenario: Source-app snapshot must come from the original capture

- **WHEN** the `PrivacyGate` evaluates an event
- **THEN** it relies on the source-application snapshot/cached probe that was associated with the capture (refreshed by the platform thread) and never falls back to inventing a source identifier simply because the user interface regained focus later in the flow

#### Scenario: Capture loop refreshes the active-app cache on the main thread

- **WHEN** the background capture loop iterates over the watcher
- **THEN** it schedules a synchronous refresh of the cached active-application probe on the Tauri main thread before reading the clipboard, so the matcher observes an identifier that is at most one tick stale; the previous fire-and-forget background refresh thread is removed and the only background refresh path is this synchronous one

#### Scenario: Refresh failure is surfaced, not swallowed

- **WHEN** the synchronous refresh fails (Tauri refuses to schedule, the main thread does not pick it up inside the timeout, or the inner probe returns a backend error)
- **THEN** the diagnostics endpoint reports `refresh_outcome = "failed"` with the actionable reason, the capture loop continues running, and the matcher keeps evaluating with the last known identifier (or the "unknown source" allow rule when the cache is still empty)

### Requirement: Maintain history across restarts

Captured text SHALL remain available after the application restarts until it is explicitly deleted, removed by the configured expiration policy, or pruned by an enforced retention sweep.

#### Scenario: History is reopened

- **WHEN** the application starts after previously capturing text
- **THEN** the persisted entries are available to the search and quick-paste workflows

#### Scenario: Retention policy expires entries

- **WHEN** the retention policy is set to a bounded value (e.g. 7, 30 or 90 days) and the application starts
- **THEN** ClipVault removes or marks as expired any entry whose capture timestamp is older than the configured horizon, except entries that the user marked as favorite

#### Scenario: Retention sweep runs while running

- **WHEN** the retention sweep executes while the application is running
- **THEN** ClipVault deletes (or marks expired) the same set of entries it would on startup, keeps favorites regardless of age and records a local diagnostic event describing how many entries were affected
