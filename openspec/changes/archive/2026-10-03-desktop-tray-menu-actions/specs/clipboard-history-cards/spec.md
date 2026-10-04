## MODIFIED Requirements

### Requirement: Desktop "Acerca de" surface

The desktop SHALL provide `About` access from the global toolbar menu and the
tray/menu-bar menu, and both SHALL open the same existing About modal. The
modal SHALL display the canonical product name (`ClipVault`) and version
(`vX.Y.Z`) read from the existing `clipvault_diagnostics` Tauri command. The
version SHALL NOT be hard-coded in `Svelte`. The About modal SHALL NOT be
duplicated in card menus, Quick Paste or a separate window. Closing the modal
SHALL honour the shared `Modal` shell contract (Escape, backdrop click and the
close button).

#### Scenario: Open the About modal from the global menu

- **WHEN** the user picks `About` inside the global toolbar menu
- **THEN** the menu closes and the existing About modal opens
- **AND** the version label reads `v` plus `diagnostics.version` from the
  backend

#### Scenario: Open About from the tray/menu-bar menu

- **WHEN** the user picks `About` from the tray/menu-bar menu
- **THEN** the existing main window is shown and focused before the modal
  opens
- **AND** the same About modal and canonical version are used

#### Scenario: Close the About modal with Escape

- **WHEN** the About modal is open and the user presses Escape
- **THEN** the modal closes
- **AND** focus returns to the toolbar trigger when opened from the toolbar, or
  remains in the main application window when opened from the tray/menu bar

#### Scenario: Version matches the canonical manifest

- **WHEN** the canonical version in `Cargo.toml` is `0.0.1`
- **THEN** the modal renders `v0.0.1`
- **AND** a future bump that updates `Cargo.toml`, `tauri.conf.json` and
  `package.json` shows the new value without any Svelte code change

#### Scenario: About modal is single-sourced

- **WHEN** the desktop renders the HistoryCard rail or Quick Paste
- **THEN** no card-level menu, Quick Paste surface or separate window exposes
  a second About modal
- **AND** the toolbar and tray/menu-bar entries open the same modal
