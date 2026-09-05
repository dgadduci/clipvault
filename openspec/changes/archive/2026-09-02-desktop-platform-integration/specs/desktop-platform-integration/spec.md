## ADDED Requirements

### Requirement: Capability matrix

ClipVault SHALL expose a typed capability matrix that reports whether `clipboard_read`, `clipboard_write`, `global_hotkey`, `synthetic_paste`, `active_application` and `tray` are available on the current host.

#### Scenario: Linux Wayland detected

- **WHEN** ClipVault starts on a Wayland session
- **THEN** `clipboard_read`, `clipboard_write`, `global_hotkey` and `tray` are reported available while `synthetic_paste` and `active_application` are reported unavailable with an actionable reason

#### Scenario: Unknown display server

- **WHEN** ClipVault starts on a host where no platform adapter is available
- **THEN** the capability matrix reports every operation as unavailable and the application continues running with history and capture disabled but the local database intact

### Requirement: Hotkey lifecycle

ClipVault SHALL register a default global hotkey (`Cmd+Shift+V` on macOS, `Ctrl+Shift+V` on Linux), explicitly handle registration success, conflict with another application and unsupported sessions, and release every registered hotkey when the application shuts down.

#### Scenario: Hotkey registration succeeds

- **WHEN** ClipVault starts on a platform that supports global hotkeys
- **THEN** the default hotkey is registered and the frontend can subscribe to its activation

#### Scenario: Hotkey conflict

- **WHEN** another application already owns the configured hotkey
- **THEN** ClipVault reports the conflict, leaves the rest of the application usable and does not terminate the process

#### Scenario: Hotkey released on shutdown

- **WHEN** ClipVault is asked to quit
- **THEN** every registered hotkey is released before the process exits

### Requirement: Paste pipeline

ClipVault SHALL paste a selected text entry by writing it to the system clipboard, triggering the platform-appropriate paste action into the previously active application and reporting typed failures without modifying the source history entry.

#### Scenario: Paste succeeds

- **WHEN** the user selects a history entry and confirms the paste
- **THEN** ClipVault writes the text to the clipboard and triggers the platform paste action and returns a typed success outcome

#### Scenario: Paste fails

- **WHEN** the platform paste action cannot be executed
- **THEN** ClipVault returns a typed failure outcome and leaves the source entry untouched in the history

#### Scenario: Paste unsupported under Wayland

- **WHEN** ClipVault runs on a Wayland session without a paste portal
- **THEN** `paste_entry` returns `CapabilityUnavailable` and the rest of the application remains usable

### Requirement: Clipboard watcher

ClipVault SHALL observe operating-system clipboard changes through a watcher that delegates to `TextHistoryService::record_text` and SHALL continue running when the backend returns errors or unsupported formats.

#### Scenario: Clipboard backend unavailable

- **WHEN** the real clipboard backend is missing or returns an error
- **THEN** the watcher reports the failure through a typed outcome and continues polling without terminating the application

#### Scenario: Non-text content

- **WHEN** the clipboard exposes a non-text representation
- **THEN** the watcher reports `Ignored` and no entry is created

### Requirement: Tray actions

ClipVault SHALL expose a tray or menu bar entry with the actions Open main window, Open quick search, Open favorites, Clear history (with confirmation), Open settings and Quit, and SHALL route actions that depend on unimplemented capabilities through thin events while reporting explicit unavailability.

#### Scenario: Quit from tray

- **WHEN** the user chooses Quit from the tray or menu bar
- **THEN** ClipVault stops capture and watchers, releases hotkeys, removes the tray icon and exits without losing already committed history

#### Scenario: Future capability action

- **WHEN** the user activates a tray action whose underlying capability is not yet implemented
- **THEN** ClipVault emits a thin `capability_unavailable` event with the capability name and keeps the rest of the application usable
