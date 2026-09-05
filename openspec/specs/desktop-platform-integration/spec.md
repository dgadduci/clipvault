## Purpose

Definir los adaptadores de plataforma y el soporte inicial de macOS, Linux, X11, Wayland y tray/menu bar.

## Requirements

### Requirement: Platform adapter boundary

ClipVault SHALL abstract clipboard access, active-application detection, global hotkeys, paste simulation and tray/menu-bar integration behind platform adapters consumed by the Rust core.

#### Scenario: Core runs with a test platform

- **WHEN** core tests provide a fake platform adapter
- **THEN** they can validate capture and paste orchestration without a real desktop session

#### Scenario: Platform operation fails

- **WHEN** a platform API returns an error
- **THEN** the adapter returns a typed or actionable error to the core without terminating the application

### Requirement: macOS support

The MVP SHALL support clipboard capture, global hotkey, quick paste and tray/menu-bar operation on macOS using the platform-specific implementation.

#### Scenario: macOS daily workflow

- **WHEN** a user copies text, invokes the default hotkey and selects an entry
- **THEN** the entry can be searched and pasted back into the active macOS application

### Requirement: Linux display support

The MVP SHALL explicitly detect and account for whether Linux is running under X11 or Wayland, and SHALL not assume that an X11-only operation works under Wayland.

#### Scenario: Linux session detected

- **WHEN** ClipVault starts on Linux
- **THEN** it identifies the available display/session capabilities and uses the compatible platform adapter

#### Scenario: Capability is unavailable

- **WHEN** a Linux session does not support a required operation such as global hotkeys or synthetic paste
- **THEN** ClipVault reports the limitation and keeps local history and other supported features usable

### Requirement: System tray or menu bar access

ClipVault SHALL provide a background tray/menu-bar entry with actions to open the application, open quick search, access favorites, clear history with confirmation, open settings and quit.

#### Scenario: Background operation

- **WHEN** the user closes the main window while capture is active
- **THEN** ClipVault remains available from the tray or menu bar and continues according to the configured capture policy

#### Scenario: Quit from tray

- **WHEN** the user selects Quit from the tray or menu bar
- **THEN** ClipVault stops capture cleanly and exits without losing already committed history

### Requirement: Capability matrix

ClipVault SHALL expose a typed capability matrix that reports whether
`clipboard_read`, `clipboard_write`, `clipboard_read_image`,
`clipboard_write_image`, `global_hotkey`, `synthetic_paste`,
`active_application` and `tray` are available on the current host. Image
capabilities SHALL be evaluated independently from text capabilities.

#### Scenario: Linux Wayland detected

- **WHEN** ClipVault starts on a Wayland session
- **THEN** `clipboard_read`, `clipboard_write`, `global_hotkey` and `tray` are reported available while `synthetic_paste` and `active_application` are reported unavailable with an actionable reason

#### Scenario: Text is available but image is not

- **WHEN** a platform can read and write text but cannot verify image support
- **THEN** text capabilities remain available and the image capabilities are
  reported unavailable without disabling text history

#### Scenario: Linux session differs by display server

- **WHEN** ClipVault starts on Linux X11 or Wayland
- **THEN** it reports image capabilities according to the real adapter/session
  result and does not infer X11 image behavior for Wayland

#### Scenario: Unknown display server

- **WHEN** ClipVault starts on a host where no platform adapter is available
- **THEN** the capability matrix reports every operation as unavailable and the application continues running with history and capture disabled but the local database intact

### Requirement: Hotkey lifecycle

ClipVault SHALL register a default global hotkey (`Cmd+Shift+V` on macOS, `Ctrl+Shift+V` on Linux), explicitly handle registration success, conflict with another application and unsupported sessions, and release every registered hotkey when the application shuts down.

#### Scenario: Hotkey registration succeeds

- **WHEN** ClipVault starts on a platform that supports global hotkeys
- **THEN** the default hotkey is registered and the frontend can subscribe to its activation

#### Scenario: Hotkey conflict

- **WHEN** another application already owns the configured hotkey
- **THEN** ClipVault reports the conflict, leaves the rest of the application usable and does not terminate the process

#### Scenario: Hotkey released on shutdown

- **WHEN** ClipVault is asked to quit
- **THEN** every registered hotkey is released before the process exits

### Requirement: Paste pipeline

ClipVault SHALL paste a selected text or image entry by writing the matching
payload to the system clipboard, triggering the platform-appropriate paste
action into the previously active application and reporting typed failures
without modifying the source history entry.

#### Scenario: Paste succeeds

- **WHEN** the user selects a history entry and confirms the paste
- **THEN** ClipVault writes the text to the clipboard and triggers the platform paste action and returns a typed success outcome

#### Scenario: Image paste succeeds

- **WHEN** an image entry is selected and image clipboard write plus synthetic
  paste are available
- **THEN** the image is written and pasted using the existing paste controller

#### Scenario: Paste fails

- **WHEN** the platform paste action cannot be executed
- **THEN** ClipVault returns a typed failure outcome and leaves the source entry untouched in the history

#### Scenario: Image paste is unsupported

- **WHEN** the session cannot write image data or perform the required paste
  action
- **THEN** the operation returns a typed capability result with guidance when
  available and leaves the image history entry untouched

#### Scenario: Paste unsupported under Wayland

- **WHEN** ClipVault runs on a Wayland session without a paste portal
- **THEN** `paste_entry` returns `CapabilityUnavailable` and the rest of the application remains usable

### Requirement: Clipboard watcher

ClipVault SHALL observe operating-system clipboard changes through a watcher
that delegates supported text and image payloads to the Rust core and SHALL
continue running when the backend returns errors or unsupported formats.

#### Scenario: Clipboard backend unavailable

- **WHEN** the real clipboard backend is missing or returns an error
- **THEN** the watcher reports the failure through a typed outcome and continues polling without terminating the application

#### Scenario: Image is available

- **WHEN** the watcher reads a supported image without usable text
- **THEN** it delegates the image to the same capture pipeline used by the
  history cards

#### Scenario: Non-text content

- **WHEN** the clipboard exposes a non-text representation
- **THEN** the watcher reports `Ignored` and no entry is created

#### Scenario: Unsupported format is available

- **WHEN** the watcher cannot normalize the current clipboard representation
- **THEN** it reports a typed ignored/unsupported outcome and keeps polling

### Requirement: Tray actions

ClipVault SHALL expose a tray or menu bar entry with the actions Open main window, Open quick search, Open favorites, Clear history (with confirmation), Open settings and Quit, and SHALL route actions that depend on unimplemented capabilities through thin events while reporting explicit unavailability.

#### Scenario: Quit from tray

- **WHEN** the user chooses Quit from the tray or menu bar
- **THEN** ClipVault stops capture and watchers, releases hotkeys, removes the tray icon and exits without losing already committed history

#### Scenario: Future capability action

- **WHEN** the user activates a tray action whose underlying capability is not yet implemented
- **THEN** ClipVault emits a thin `capability_unavailable` event with the capability name and keeps the rest of the application usable

### Requirement: Rich clipboard capability boundary

The platform adapter SHALL expose rich clipboard read and write behind
testable traits and SHALL report those capabilities separately from plain and
image clipboard operations. The core must remain runnable with a fake adapter
that has no desktop session.

#### Scenario: macOS rich clipboard

- **WHEN** macOS exposes HTML or RTF through its pasteboard APIs
- **THEN** the adapter returns the supported representations and reports the
  matching rich read/write capabilities

#### Scenario: Linux X11 rich clipboard

- **WHEN** Linux X11 exposes a supported rich target and the adapter can read
  or write it
- **THEN** the operation succeeds through the X11 adapter and does not change
  the plain/image capability flags

#### Scenario: Linux Wayland lacks rich integration

- **WHEN** the current Wayland session has no verified rich clipboard path
- **THEN** rich capabilities are unavailable, plain text remains usable and
  the operation returns typed capability information without a false
  permission recommendation

### Requirement: Rich paste preserves the existing target flow

Rich paste SHALL reuse the existing active-target capture, hide-before-paste
  and synthetic-paste controller. Platform adapters SHALL not receive a Tauri
  window or frontend object.

#### Scenario: Rich paste on supported platform

- **WHEN** a rich entry is selected and rich write plus synthetic paste are
  available
- **THEN** the adapter writes the selected rich representations and the
  controller pastes into the application that was active before ClipVault
  opened

#### Scenario: Rich write unavailable

- **WHEN** rich write is unavailable but plain write and synthetic paste are
  available
- **THEN** the core returns the explicit plain fallback outcome and keeps the
  existing target flow
