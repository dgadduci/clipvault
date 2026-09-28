## ADDED Requirements

### Requirement: KDE Plasma Wayland reports the active source application

In a supported KDE Plasma Wayland session, ClipVault SHALL obtain the active
application identifier from the consented KWin integration when it is
connected. The integration MUST use a stable application identifier exposed
by KWin, MUST clear its snapshot when the active application becomes
unavailable, and MUST NOT infer identity from a window title, process ID,
process enumeration, clipboard contents or window ordering. It SHALL update
the existing shared active-application probe used by clipboard capture.

#### Scenario: KWin reports a supported active application

- **GIVEN** the user has enabled the ClipVault KWin integration in a supported
  KDE Plasma Wayland session
- **AND** KWin exposes a valid desktop-file identifier for its active window
- **WHEN** ClipVault captures a supported clipboard payload
- **THEN** the existing capture pipeline receives that identifier as
  `source_app` before `PrivacyGate`
- **AND** the KWin path does not depend on an X11 selection or active window

#### Scenario: ClipVault reconnects after the KWin script is already active

- **GIVEN** the user enabled the KWin integration before starting ClipVault
- **AND** the KWin script already has an active-window snapshot
- **WHEN** ClipVault registers its D-Bus receiver or reconnects to it
- **THEN** the script publishes the current active-application snapshot without
  waiting for another focus change
- **AND** the next capture does not use a missing or stale source identifier

#### Scenario: KWin has no valid active application

- **GIVEN** the KWin integration is connected
- **AND** the active window or its desktop-file identifier is absent or
  invalid
- **WHEN** the active-application snapshot is refreshed
- **THEN** the snapshot is cleared
- **AND** a previous KWin or XWayland identifier is not reused for the next
  capture
- **AND** clipboard capture continues with unknown source metadata

#### Scenario: KWin integration is unavailable or declined

- **GIVEN** the user declined, disabled or cannot initialize the KWin
  integration
- **WHEN** ClipVault captures a clipboard payload in KDE Plasma Wayland
- **THEN** it keeps the existing Wayland/XWayland fallback behavior
- **AND** capture continues if no source identifier is available

#### Scenario: Existing platform source detection is preserved

- **WHEN** ClipVault runs on macOS, Linux GNOME X11 or Linux GNOME Wayland
- **THEN** each session keeps its existing active-application probe and
  precedence
- **AND** the KDE integration is not loaded or selected for those sessions

#### Scenario: KWin diagnostics are metadata-only

- **WHEN** ClipVault reports KWin integration status or an IPC error
- **THEN** diagnostics expose only stable backend, state and error categories
- **AND** contain no window title, process ID, absolute path, clipboard
  content, hash or raw environment value
