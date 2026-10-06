## ADDED Requirements

### Requirement: KDE task switching identifies the main application window

On KDE Plasma, ClipVault SHALL expose a main-window application identity and
icon that KWin can associate with the installed ClipVault desktop entry. The
window SHALL appear with the ClipVault icon in both the panel Task Manager
and the Alt+Tab application switcher. This identity SHALL remain distinct
from the existing Status and Notifications tray icon.

#### Scenario: Main window appears in the Task Manager

- **GIVEN** ClipVault is running on KDE Plasma with its main window visible
- **WHEN** the user views the panel Task Manager
- **THEN** the ClipVault window appears with the existing application icon
- **AND** it is associated with the installed ClipVault desktop entry

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
- **AND** showing or hiding the main window does not replace that tray icon
