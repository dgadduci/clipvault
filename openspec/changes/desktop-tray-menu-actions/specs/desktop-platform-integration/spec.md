## MODIFIED Requirements

### Requirement: System tray or menu bar access

ClipVault SHALL provide a background tray/menu-bar entry with actions to open
the main window, open quick search, clear eligible unorganized history with
confirmation, open General Settings, open About, toggle clipboard capture,
and quit. The tray/menu-bar menu SHALL NOT include a Favorites action. Removing
that menu item SHALL NOT remove the application's favorite feature.

#### Scenario: Background operation

- **WHEN** the user closes the main window while capture is active
- **THEN** ClipVault remains available from the tray or menu bar and continues
  according to the configured capture policy

#### Scenario: Open the existing main window

- **GIVEN** the main window already exists and is hidden, minimized or behind
  another window
- **WHEN** the user selects `Open ClipVault` from the tray/menu-bar menu
- **THEN** ClipVault restores it if minimized, makes it visible and requests
  focus for that same window
- **AND** it does not create a second main window

#### Scenario: Open ClipVault repeatedly

- **WHEN** the user selects `Open ClipVault` repeatedly while the main window
  exists
- **THEN** every activation targets the same main window
- **AND** no additional main windows are created

#### Scenario: Recreate a missing main window

- **GIVEN** the application is running and no main window is registered
- **WHEN** the user selects `Open ClipVault` from the tray/menu-bar menu
- **THEN** ClipVault recreates the main window from its configured window
  definition
- **AND** makes it visible and requests focus
- **AND** subsequent activations reuse that same window

#### Scenario: Favorites is absent from the tray menu

- **WHEN** the native tray/menu-bar menu is displayed
- **THEN** it has no `Favorites` item
- **AND** favorite entries and their existing controls remain available in
  ClipVault

#### Scenario: Preview Clear History

- **WHEN** the user selects `Clear History` from the tray/menu-bar menu
- **THEN** ClipVault shows the existing metadata-only confirmation with the
  count of eligible entries
- **AND** no history entry is changed before confirmation

#### Scenario: Confirm Clear History

- **GIVEN** the user confirmed `Clear History`
- **WHEN** ClipVault completes the existing clear-unorganized-history action
- **THEN** it removes non-favorite entries with no user-collection membership
  (the required `Historial` membership does not count as a user collection)
- **AND** favorites and entries in any user collection remain
- **AND** peer devices and their remote data remain unchanged

#### Scenario: Cancel Clear History

- **WHEN** the user cancels the tray-launched Clear History confirmation
- **THEN** all entries, memberships, favorites, provenance and assets remain
  unchanged

#### Scenario: Open General Settings from the tray

- **WHEN** the user selects `Settings` while the main window is hidden or
  minimized
- **THEN** ClipVault shows, restores and focuses the existing main window
- **AND** opens the existing General Settings modal in that window
- **AND** does not create another window or a second Settings modal

#### Scenario: Open About from the tray

- **WHEN** the user selects `About` from the tray/menu-bar menu
- **THEN** ClipVault shows, restores and focuses the existing main window
- **AND** opens the existing About modal in that window
- **AND** displays the canonical product name and version supplied by
  `clipvault_diagnostics`

#### Scenario: Quit from tray

- **WHEN** the user selects Quit from the tray or menu bar
- **THEN** ClipVault stops capture cleanly and exits without losing already
  committed history

### Requirement: Tray actions

The tray adapter SHALL dispatch `Open ClipVault`, `Clear History`, `Settings`
and `About` to the application flows specified by `System tray or menu bar
access`. UI and history business logic SHALL remain in the existing frontend,
Tauri command and Rust service layers; the native menu callback SHALL remain a
thin adapter. Actions implemented by this change SHALL NOT be reported as
unavailable capabilities.

#### Scenario: Tray action reaches its UI flow

- **WHEN** the user selects `Clear History`, `Settings` or `About`
- **THEN** the tray adapter delivers the typed action to the existing main
  window and the corresponding UI flow opens
- **AND** the tray adapter does not query or mutate SQLite directly

#### Scenario: Quit from tray

- **WHEN** the user chooses Quit from the tray or menu bar
- **THEN** ClipVault stops capture and watchers, releases hotkeys, removes the
  tray icon and exits without losing already committed history

#### Scenario: Future capability action

- **WHEN** the user activates a tray action whose underlying capability is
  not yet implemented
- **THEN** ClipVault emits a thin `capability_unavailable` event with the
  capability name and keeps the rest of the application usable
