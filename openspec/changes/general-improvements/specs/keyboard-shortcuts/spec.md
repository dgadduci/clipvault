## ADDED Requirements

### Requirement: Open the keyboard shortcuts dialog with a configurable shortcut

ClipVault SHALL provide a configurable main-window shortcut that opens the
existing keyboard shortcuts dialog. The action SHALL use the shared shortcut
registry and SHALL appear as a row in that dialog with its current binding and
main-window context. Its default SHALL be `⌘⇧K` on macOS and
`Ctrl+Shift+K` on Linux. The existing menu and General Settings entry points
SHALL remain available.

#### Scenario: Open the dialog from the main window

- **WHEN** the user presses the active `open_keyboard_shortcuts` binding in
  the main window while no modal or text-editing field is active
- **THEN** the existing keyboard shortcuts dialog opens
- **AND** it does not create a second dialog or a new window

#### Scenario: View and change the dialog shortcut

- **WHEN** the user opens the keyboard shortcuts dialog from General Settings
- **THEN** the list includes an `open_keyboard_shortcuts` row with the
  platform-formatted active binding and the main-window context
- **AND** the user can change that binding through the existing recording
  flow
- **AND** an accepted change takes effect immediately and persists locally

#### Scenario: Shortcut does not reopen an active modal

- **WHEN** the configured shortcut is pressed while a modal is already open
  or focus is in a text-editing field
- **THEN** it does not replace, close or open a modal

#### Scenario: Shortcut does not collide with an active action

- **WHEN** the user records a binding that conflicts with another action in
  the main-window context
- **THEN** the existing conflict validation rejects it and retains the prior
  active and saved binding
