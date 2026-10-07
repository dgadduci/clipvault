## ADDED Requirements

### Requirement: KDE task switching identifies the main application window

On KDE Plasma, ClipVault SHALL expose a main-window application identity and
icon that KWin can associate with the installed ClipVault desktop entry. The
window SHALL appear with the ClipVault icon in both the panel Task Manager
and the Alt+Tab application switcher. This identity SHALL remain distinct
from the Status and Notifications tray icon. On Wayland, the app ID MUST
match the basename of an installed, visible `.desktop` entry, and that entry
MUST reference the installed square ClipVault icon. Any generated launcher
whose basename does not match the app ID SHALL be hidden from application
menus to prevent duplicates. `StartupWMClass` remains available as the X11
matching fallback. The tray icon SHALL use the distinctive ClipVault mark on a
transparent background, separate from the square window icon. The mark SHALL
combine a clip and a vault motif and SHALL NOT resemble a generic document or
writing application icon. On Linux, ClipVault SHALL explicitly apply the
bundled square icon to the main native window at startup and whenever that
window is created or restored through the tray action. The installed desktop
entry and the native window SHALL use the same ClipVault mark.

#### Scenario: Wayland app ID matches an installed desktop entry

- **GIVEN** ClipVault announces `com.clipvault.desktop` as its Wayland app ID
- **WHEN** the Linux package is installed
- **THEN** an installed desktop entry named
  `com.clipvault.desktop.desktop` is available to the desktop shell
- **AND** its `Icon` value resolves to the packaged ClipVault icon
- **AND** it is not marked `NoDisplay=true`
- **AND** the generated `ClipVault.desktop` launcher is hidden from menus

#### Scenario: Main window appears in the Task Manager

- **GIVEN** ClipVault is running on KDE Plasma with its main window visible
- **WHEN** the user views the panel Task Manager
- **THEN** the ClipVault window appears with the existing application icon
- **AND** it is associated with the installed ClipVault desktop entry
- **AND** its native window icon is the bundled square ClipVault mark

#### Scenario: Tray and main window use the same ClipVault mark

- **GIVEN** the tray displays the ClipVault mark correctly
- **WHEN** the main window is opened from the tray
- **THEN** the taskbar and Alt+Tab show the same ClipVault mark
- **AND** neither surface falls back to a generic document icon

#### Scenario: Main window appears in Alt+Tab

- **GIVEN** ClipVault's main window is open on KDE Plasma
- **WHEN** the user switches applications with Alt+Tab
- **THEN** the ClipVault entry shows the same application icon and name

#### Scenario: Quick Paste does not create an extra task-switcher entry

- **GIVEN** the main window and transient Quick Paste window are both used
- **WHEN** the user views the panel Task Manager or Alt+Tab switcher
- **THEN** the main window is represented by ClipVault
- **AND** Quick Paste does not add a separate entry

#### Scenario: Tray icon remains independent

- **WHEN** ClipVault runs on KDE Plasma
- **THEN** its existing Status and Notifications icon remains available
- **AND** the tray logo has a transparent background
- **AND** showing or hiding the main window does not replace that tray icon
