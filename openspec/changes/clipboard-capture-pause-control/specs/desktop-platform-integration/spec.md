## ADDED Requirements

### Requirement: Global shortcut for pausing clipboard capture

ClipVault SHALL register a global shortcut that toggles local clipboard
capture: `Cmd+Option+Shift+B` on macOS and `Ctrl+Alt+Shift+B` on Linux. The
shortcut SHALL remain registered while capture is paused so the user can resume
without opening the main window. Registration SHALL use the existing platform
hotkey adapter on macOS and X11, and the existing GNOME Shell or KDE KWin
integration on matching Linux Wayland sessions. General Settings SHALL explain
when a Wayland desktop integration must be enabled for the shortcut to work.

#### Scenario: Shortcut pauses active capture

- **GIVEN** local clipboard capture is enabled
- **WHEN** the user presses `Cmd+Option+Shift+B` on macOS or
  `Ctrl+Alt+Shift+B` on Linux
- **THEN** local clipboard capture is paused and the persisted state is
  updated

#### Scenario: Shortcut resumes paused capture

- **GIVEN** local clipboard capture is paused
- **WHEN** the user presses the platform shortcut
- **THEN** local clipboard capture is resumed and the persisted state is
  updated

#### Scenario: Shortcut conflicts with another application

- **WHEN** the operating system cannot register the capture shortcut
- **THEN** ClipVault reports the existing typed hotkey conflict or unavailable
  outcome, continues to allow changes from General Settings and the tray, and
  remains usable

#### Scenario: Wayland shortcut uses the desktop integration

- **GIVEN** a supported GNOME or KDE Wayland integration is installed and
  enabled
- **WHEN** the user presses `Ctrl+Alt+Shift+B`
- **THEN** ClipVault toggles local clipboard capture through the shared
  persisted state transition

### Requirement: Tray action for pausing and resuming capture

The tray/menu-bar menu SHALL expose one action that pauses or resumes local
clipboard capture according to its current state and SHALL show the matching
platform shortcut label (`⌘⌥⇧B` on macOS, `Ctrl+Alt+Shift+B` on Linux). Activating
the action SHALL use the same persisted state transition as the global
shortcut and General Settings.

#### Scenario: Tray pauses active capture

- **GIVEN** local clipboard capture is enabled
- **WHEN** the user selects `Pausar capturas` from the tray/menu bar
- **THEN** local clipboard capture is paused, its preference is saved, and the
  menu action changes to `Reanudar capturas`

#### Scenario: Tray resumes paused capture

- **GIVEN** local clipboard capture is paused
- **WHEN** the user selects `Reanudar capturas` from the tray/menu bar
- **THEN** local clipboard capture is resumed, its preference is saved, and
  the menu action changes to `Pausar capturas`

#### Scenario: Tray reflects a change from another surface

- **WHEN** capture state changes from the shortcut or General Settings
- **THEN** the tray/menu-bar action and shortcut label reflect the persisted
  state without requiring an application restart
