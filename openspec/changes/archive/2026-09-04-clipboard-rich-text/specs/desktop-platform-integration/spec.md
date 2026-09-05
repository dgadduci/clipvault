## ADDED Requirements

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
