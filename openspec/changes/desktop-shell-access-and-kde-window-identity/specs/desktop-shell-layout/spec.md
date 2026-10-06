## MODIFIED Requirements

### Requirement: Modal entry points

The main desktop SHALL expose its ordinary configuration actions through the
accessible overflow menu and SHALL open the Development diagnostics modal
only through the reserved, platform-specific main-window keyboard shortcut.
The Development shortcut MUST NOT be registered as a system-global hotkey.
ClipVault SHALL allow at most one modal to be open at a time, and modal
switching and cleanup SHALL preserve the existing lifecycle.

#### Scenario: Open a configuration modal

- **WHEN** the user activates an ordinary configuration control in the
  overflow menu
- **THEN** the corresponding modal opens with its title, current state and
  existing controls

#### Scenario: Switching modal entry points

- **WHEN** a modal is open and the user activates another available modal
  entry point
- **THEN** the current modal closes and only the newly requested modal
  remains open

#### Scenario: Modal state does not duplicate

- **WHEN** the desktop is remounted, hot-reloaded or a modal is opened
  repeatedly
- **THEN** only one modal instance and one set of related
  listeners/handlers remain active

#### Scenario: Development is absent from visible menus

- **WHEN** the user opens the desktop overflow menu or another application
  menu
- **THEN** no `Development` menu item is present
- **AND** the other existing menu actions retain their current behavior

#### Scenario: Open Development with the reserved shortcut

- **WHEN** the main window has focus, no modal is open, and the user presses
  `⌃⌥⌘⇧D` on macOS or `Ctrl+Alt+Shift+D` on Linux
- **THEN** the existing Development diagnostics modal opens
- **AND** no new window or duplicate modal is created

#### Scenario: Ignore the shortcut while editing or in another modal

- **WHEN** the reserved shortcut is pressed while a modal is open or focus is
  in a text-editing control
- **THEN** it does not open or replace a modal and does not interfere with
  text entry

#### Scenario: Shortcut remains application-local

- **WHEN** the main window is not focused
- **THEN** the Development shortcut does not activate ClipVault globally

## ADDED Requirements

### Requirement: Collection content loading feedback

The desktop SHALL show the existing circular loading indicator while content
for any selected local or remote collection is loading or refreshing. The
indicator SHALL expose an accessible, localized busy status, SHALL disappear
when the operation succeeds or fails, and SHALL NOT present the previous
collection's content as belonging to the newly selected collection.

#### Scenario: Loading a local collection

- **GIVEN** the user selects Historial, Favoritos or a user-defined local
  collection
- **WHEN** ClipVault loads or refreshes that collection's content
- **THEN** the circular loading indicator appears in the collection content
  area with an accessible busy status

#### Scenario: Loading a remote collection

- **GIVEN** the user selects a linked remote computer
- **WHEN** ClipVault loads or refreshes its collection content
- **THEN** the same circular loading indicator and accessible busy status
  appear

#### Scenario: Collection loading finishes

- **WHEN** a collection load or refresh succeeds or returns an error
- **THEN** its loading indicator disappears and the resulting content or
  existing recoverable error state is shown

#### Scenario: Collection is idle

- **WHEN** the selected collection has no load or refresh in progress
- **THEN** no loading indicator is shown
