## ADDED Requirements

### Requirement: Persistent local clipboard capture control

ClipVault SHALL persist the user's local clipboard-capture enabled state in
local settings, default it to enabled when unset, and expose it in the General
Settings section. The setting SHALL be changed through the shared Rust
settings service; the frontend SHALL NOT access the database directly.

#### Scenario: General Settings shows capture state and shortcut

- **WHEN** the user opens General Settings
- **THEN** a `Captura del portapapeles` item shows whether capture is active or
  paused, provides a control to change that state, and displays the platform
  shortcut (`⌘⌥⇧B` on macOS, `Ctrl+Alt+Shift+B` on Linux)

#### Scenario: General Settings changes capture state

- **WHEN** the user changes the capture control in General Settings
- **THEN** the local setting is persisted and the shared capture state changes
- **AND** the tray/menu-bar action reflects the new state

#### Scenario: General Settings cannot save the change

- **WHEN** persisting the capture preference fails
- **THEN** the displayed state remains the last successfully applied value and
  the interface reports the error
