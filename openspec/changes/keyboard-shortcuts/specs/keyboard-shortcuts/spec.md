## ADDED Requirements

### Requirement: Configure every application shortcut with modifiers

The application SHALL expose the modifier-based shortcuts it owns in one
keyboard shortcuts dialog opened from General Settings. The dialog SHALL list
the effective current binding for each action and SHALL include opening
QuickVault, toggling clipboard capture, focusing search in either window,
previewing the selected capture, editing the selected text capture, and
opening or editing the note for the selected capture, navigating to History,
and creating a text capture. Shared actions SHALL use one binding
across all surfaces that perform that action. Standard navigation and
activation keys without modifiers SHALL retain their existing behavior.
QuickVault SHALL expose `Shift+Enter` for copying a selected rich-text capture
as plain text, while plain `Enter` SHALL retain its default action.
The configurable Save text action SHALL not be listed.

#### Scenario: View the current keyboard shortcuts

- **WHEN** the user opens Keyboard shortcuts from General Settings
- **THEN** the dialog lists every application-owned modifier-based shortcut
- **AND** each row displays the active platform-specific key combination
- **AND** standard navigation keys remain outside the configurable list
- **AND** QuickVault lists `Shift+Enter` separately from plain `Enter`
- **AND** the current binding is rendered from the latest active shortcut
  snapshot without closing and reopening the dialog

#### Scenario: Shared action has one binding

- **WHEN** the user views the Edit selected text action
- **THEN** the same binding applies in the main window and QuickVault

#### Scenario: Open or edit the selected capture note

- **WHEN** the user invokes the configured note shortcut with a capture
  selected in the main history rail
- **THEN** the existing note dialog opens for that capture
- **AND** it loads the existing note for editing or an empty note for creation
- **WHEN** no capture is selected
- **THEN** the shortcut does not open a dialog

#### Scenario: Navigate to History

- **WHEN** the user invokes the configured History shortcut
- **THEN** the History system collection is selected in the sidebar
- **AND** the capture list refreshes using the History collection scope
- **AND** the History row does not show the shortcut binding beside its name
- **AND** the History row exposes the active binding through
  `aria-keyshortcuts`

#### Scenario: Create a text capture from the keyboard

- **WHEN** the user invokes the configured create-text shortcut in the main
  window
- **THEN** the existing text-capture dialog opens for the active collection
- **AND** the toolbar button remains available and opens the same dialog

### Requirement: Capture and persist a replacement binding

The application SHALL let the user select a shortcut action and record one
key combination containing at least one modifier and one supported
non-modifier key. A valid binding SHALL be applied and persisted to local
application settings without a database schema migration. The UI SHALL
display the new binding as soon as activation succeeds. Bindings SHALL remain
in effect after the application restarts.

#### Scenario: User records a valid shortcut

- **WHEN** the user records a valid combination for an action
- **THEN** the application validates and activates the replacement
- **AND** it persists the binding locally
- **AND** the dialog row and every affected window display and use the new
  combination immediately without reloading a window

#### Scenario: Application starts with a saved binding

- **WHEN** ClipVault starts with a valid saved shortcut configuration
- **THEN** it registers each global shortcut and installs each local matcher
  using the saved bindings before reporting them as active

#### Scenario: Existing QuickVault binding is loaded

- **WHEN** settings contain the existing `quick_paste_hotkey` value
- **THEN** ClipVault uses it as the `open_quick_paste` binding when valid
- **AND** it uses the platform default when the value is absent or invalid

### Requirement: Apply global shortcuts on supported desktop integrations

ClipVault SHALL register and replace its global QuickVault and clipboard
capture shortcuts on macOS, Linux/X11, GNOME Wayland, and KDE Plasma Wayland.
GNOME and KWin integrations SHALL accept runtime binding updates from the
local ClipVault process. A successful update SHALL not require restarting
ClipVault, refreshing a WebView, or manually editing desktop settings.

#### Scenario: Global shortcuts are changed in GNOME Wayland

- **WHEN** the user changes either global binding while the GNOME integration
  is available
- **THEN** the extension replaces that accelerator and confirms its active
  state to ClipVault
- **AND** the other global binding remains registered

#### Scenario: Global shortcuts are changed in KDE Plasma Wayland

- **WHEN** the user changes either global binding while the KWin integration
  is available
- **THEN** KWin replaces that action without restarting ClipVault
- **AND** the active application bridge and the other global binding remain
  available

#### Scenario: Global shortcut is changed on macOS or X11

- **WHEN** the user changes a global binding on macOS or Linux/X11
- **THEN** the platform hotkey adapter replaces only the selected action
- **AND** the other global binding remains registered

### Requirement: Keep the previous binding on validation, registration, or save failure

The application SHALL reject unsupported or conflicting bindings and SHALL
keep the previously active and persisted binding when validation,
platform registration, or persistence fails. The dialog SHALL report the
failure in the current interface language.

#### Scenario: Operating system reports a global shortcut conflict

- **WHEN** another application or the desktop already owns the requested
  global combination
- **THEN** ClipVault does not persist the new value
- **AND** the previous binding remains active and visible
- **AND** the dialog shows a localized conflict message

#### Scenario: Two active ClipVault actions would collide

- **WHEN** a proposed binding duplicates another ClipVault shortcut in a
  context where both can run
- **THEN** the application rejects the proposal and explains the conflict
- **AND** it preserves the previous bindings

#### Scenario: Saving a registered binding fails

- **WHEN** the OS accepts a new global binding but local persistence fails
- **THEN** ClipVault unregisters the new binding and restores the previous
  binding
- **AND** it leaves the saved value and displayed effective binding unchanged

### Requirement: Synchronize labels, active handlers, and accessibility hints

The application SHALL derive visible shortcut hints, tooltips,
`aria-keyshortcuts` values, and keyboard matchers from the same active
binding. A successful change SHALL update the main window, QuickVault, and
native tray labels during the current session. All new user-facing copy SHALL
be present in every supported local catalog.

#### Scenario: A shortcut appears in more than one surface

- **WHEN** the user changes a binding shown in both the main window and
  QuickVault
- **THEN** both surfaces display the same platform-appropriate combination
- **AND** both handlers react only to that combination

#### Scenario: Capture-toggle shortcut appears in the tray

- **WHEN** the user changes the capture-toggle binding
- **THEN** the tray label updates to the active combination
- **AND** activating that combination still uses the existing capture
  pause/resume transition

#### Scenario: User interface text is translated

- **WHEN** the shortcut dialog or a validation error is shown
- **THEN** all ClipVault-authored copy is loaded from the supported locale
  catalogs and dynamic key names are interpolated as values
