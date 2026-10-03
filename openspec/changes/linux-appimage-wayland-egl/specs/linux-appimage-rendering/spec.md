## ADDED Requirements

### Requirement: Linux AppImages render with the host Wayland and EGL stack

The desktop release workflow SHALL use a Tauri AppImage bundler version that
contains the upstream fix for `EGL_BAD_PARAMETER` on current Mesa/Wayland
systems. The generated AppImage SHALL allow WebKit to use a compatible host
Wayland/EGL stack instead of shadowing it with incompatible bundled libraries.

#### Scenario: AppImage opens on Arch KDE Wayland

- **WHEN** a user starts a release AppImage on Arch Linux with KDE Plasma
  Wayland and a current Mesa stack
- **THEN** the ClipVault desktop WebView renders its interface
- **AND** the WebKit process does not abort with
  `Could not create default EGL display: EGL_BAD_PARAMETER`
- **AND** the normal shell bootstrap and tray remain available

#### Scenario: Release AppImage preserves updater integrity

- **WHEN** the release workflow packages an AppImage with the corrected
  bundler
- **THEN** it still creates the expected signed updater artifact and
  platform target in `latest.json`
- **AND** the `.deb` and macOS release targets remain buildable

#### Scenario: Existing local data survives AppImage replacement

- **WHEN** a user replaces the affected AppImage with a corrected release
- **THEN** the existing local history and persisted assets remain available
- **AND** the update procedure does not delete or reset `~/.clipvault`
