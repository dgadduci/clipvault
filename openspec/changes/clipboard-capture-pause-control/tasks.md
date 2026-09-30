# Tasks: clipboard-capture-pause-control

## 1. State and persistence

- [x] 1.1 Extend local settings with a capture-enabled value that defaults to
  `true` when absent and round-trips through the existing settings service.
- [x] 1.2 Add one shared capture-state transition used by the watcher, shortcut,
  General Settings and tray/menu-bar adapters.
- [x] 1.3 Ensure pause takes effect before reporting success, preserves existing
  entries/assets, and resume does not backfill the clipboard value present at
  the time of resumption.
- [x] 1.4 Ensure persistence failures retain the previous effective state and
  return a typed error to the caller.

## 2. User controls

- [x] 2.1 Register `Cmd+Option+Shift+B` on macOS and `Ctrl+Alt+Shift+B` on
  Linux through the native adapters and existing GNOME/KDE Wayland integrations,
  including while capture is paused.
- [x] 2.2 Add a `Captura del portapapeles` row to General Settings with current
  state, an enable/pause control and the platform shortcut label.
- [x] 2.3 Add a dynamic tray/menu-bar pause/resume action with the same shortcut
  hint and keep it synchronized with changes from other surfaces.
- [x] 2.4 Route all surfaces through the shared Rust operation and expose errors
  without showing an unpersisted state as active.

## 3. Regression coverage and verification

- [x] 3.1 Cover default-enabled state, persistence/restart, pause/resume,
  in-flight work, and no backfill with focused core/settings/watcher tests.
- [x] 3.2 Cover shortcut registration/toggle and tray menu updates with focused
  adapter tests, including hotkey conflict behavior.
- [x] 3.3 Cover General Settings state, shortcut labels, error handling and
  synchronization with the tray action.
- [x] 3.4 Run affected Rust and frontend checks/builds, focused tests, OpenSpec
  validation and `git diff --check`; preserve existing tray, hotkey and
  clipboard-capture behavior.
- [ ] 3.5 Perform manual pause/resume checks on macOS, Ubuntu X11, Ubuntu
  Wayland and Arch KDE Wayland, including restart persistence and the
  no-backfill behavior.

Verification note: focused checks pass. The complete Tauri binary suite reports
14 failures in active-app attribution fixtures and the host Wayland probe; the
capture-toggle binding test passes independently.
