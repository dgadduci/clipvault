## MODIFIED Requirements

### Requirement: Capability matrix

ClipVault SHALL expose a typed capability matrix that reports whether
`clipboard_read`, `clipboard_write`, `clipboard_read_image`,
`clipboard_write_image`, `global_hotkey`, `synthetic_paste`,
`active_application` and `tray` are available on the current host. Image
capabilities SHALL be evaluated independently from text capabilities.

#### Scenario: Text is available but image is not

- **WHEN** a platform can read and write text but cannot verify image support
- **THEN** text capabilities remain available and the image capabilities are
  reported unavailable without disabling text history

#### Scenario: Linux session differs by display server

- **WHEN** ClipVault starts on Linux X11 or Wayland
- **THEN** it reports image capabilities according to the real adapter/session
  result and does not infer X11 image behavior for Wayland

### Requirement: Paste pipeline

ClipVault SHALL paste a selected text or image entry by writing the matching
payload to the system clipboard, triggering the platform-appropriate paste
action into the previously active application and reporting typed failures
without modifying the source history entry.

#### Scenario: Image paste succeeds

- **WHEN** an image entry is selected and image clipboard write plus synthetic
  paste are available
- **THEN** the image is written and pasted using the existing paste controller

#### Scenario: Image paste is unsupported

- **WHEN** the session cannot write image data or perform the required paste
  action
- **THEN** the operation returns a typed capability result with guidance when
  available and leaves the image history entry untouched

### Requirement: Clipboard watcher

ClipVault SHALL observe operating-system clipboard changes through a watcher
that delegates supported text and image payloads to the Rust core and SHALL
continue running when the backend returns errors or unsupported formats.

#### Scenario: Image is available

- **WHEN** the watcher reads a supported image without usable text
- **THEN** it delegates the image to the same capture pipeline used by the
  history cards

#### Scenario: Unsupported format is available

- **WHEN** the watcher cannot normalize the current clipboard representation
- **THEN** it reports a typed ignored/unsupported outcome and keeps polling
