## ADDED Requirements

### Requirement: Linux main desktop fills available monitor width

On Linux, ClipVault SHALL size the main desktop window so its visible content
fills the complete horizontal work area available on the monitor where the
window opens. The result SHALL be consistent on X11 and Wayland, including
GNOME and KDE Plasma sessions. The window SHALL account for monitor scale and
SHALL NOT be constrained by a fixed configured or minimum width that leaves
unused horizontal space. This requirement does not require fullscreen or a
change to the existing compact height.

#### Scenario: Main desktop fills the width on Linux

- **WHEN** the main desktop opens on Linux with valid monitor geometry
- **THEN** its visible content spans the available horizontal monitor area
- **AND** it has no unused side band caused by the configured default width
- **AND** the compact vertical layout remains unchanged

#### Scenario: Linux display backends agree

- **WHEN** the main desktop opens in X11 or Wayland, including GNOME or KDE
  Plasma sessions
- **THEN** the main window fills the available horizontal area using the
  geometry reported by that backend
- **AND** logical and physical dimensions account for the monitor scale

#### Scenario: Work-area geometry is unavailable

- **WHEN** Linux provides monitor bounds but no valid work-area dimensions
- **THEN** ClipVault uses the available monitor bounds to size the main
  desktop
- **WHEN** neither work-area nor monitor bounds are available
- **THEN** ClipVault retains the configured fallback size and still starts

#### Scenario: A narrow display constrains the minimum width

- **WHEN** the available monitor width is less than the configured minimum
  width
- **THEN** the native minimum does not force the main window beyond the
  available horizontal area
- **AND** the desktop content remains usable without page-level horizontal
  overflow

#### Scenario: Manual resize remains user-controlled

- **WHEN** the user manually resizes or moves the main desktop after its
  initial layout
- **THEN** later frontend updates do not reset the window to full width or
  move it back to its initial position

#### Scenario: Other windows and macOS retain their behavior

- **WHEN** the main desktop opens on macOS or QuickVault opens on any platform
- **THEN** their existing size and layout behavior remains unchanged
