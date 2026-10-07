## ADDED Requirements

### Requirement: Explain platform failures with typed guidance

ClipVault SHALL classify actionable platform failures as
permission_required, unsupported_session, backend_unavailable or unknown, and
SHALL expose structured guidance without including clipboard content.

#### Scenario: macOS paste permission is missing

- **WHEN** the user attempts synthetic paste on macOS without the required
  Accessibility permission
- **THEN** ClipVault returns CapabilityUnavailable with cause
  permission_required, explains that Accessibility is required to send Cmd+V,
  and provides ordered remediation steps

#### Scenario: Linux X11 integration is restricted

- **WHEN** ClipVault cannot access the X11 display, XTest or a declared
  application sandbox permission required by the paste backend
- **THEN** ClipVault classifies the issue as backend_unavailable or
  permission_required only when the restriction is known, and provides
  Linux-specific remediation steps without mentioning macOS permissions

#### Scenario: Linux Wayland lacks synthetic paste support

- **WHEN** ClipVault runs on a Wayland session without a supported paste
  mechanism
- **THEN** ClipVault reports unsupported_session, explains that this is not a
  missing macOS permission, and keeps the remaining local capabilities usable

#### Scenario: Platform backend fails for another reason

- **WHEN** the clipboard or paste backend fails for a reason that cannot be
  classified more precisely
- **THEN** ClipVault returns backend_unavailable or unknown with a safe,
  actionable explanation and keeps the source history entry unchanged

### Requirement: Show an actionable platform window

ClipVault SHALL show an accessible modal when a user-triggered platform
operation returns actionable guidance.

#### Scenario: Paste returns macOS permission guidance

- **WHEN** Paste latest receives permission_required on macOS
- **THEN** the modal shows a clear title, a short explanation, numbered steps,
  and buttons for Abrir configuración, Reintentar and Cerrar

#### Scenario: Paste returns Linux guidance

- **WHEN** Paste latest receives permission_required, unsupported_session or
  backend_unavailable on Linux
- **THEN** the modal shows Linux-specific cause and remediation text and only
  displays Abrir configuración when the backend provides a safe target

#### Scenario: User dismisses guidance

- **WHEN** the user closes the guidance modal
- **THEN** the modal closes without changing, deleting or duplicating the
  history entry

### Requirement: Open system settings safely

ClipVault SHALL provide a best-effort action to open a known system settings
destination when platform guidance identifies one.

#### Scenario: Open macOS Accessibility settings

- **WHEN** the user selects Abrir configuración for MacosAccessibility
- **THEN** ClipVault opens the corresponding macOS System Settings pane or
  falls back to opening System Settings and reports whether the user must
  navigate manually

#### Scenario: Open Linux desktop integration settings

- **WHEN** the user selects Abrir configuración for a known Linux desktop
  integration target
- **THEN** ClipVault opens the supported desktop settings entry or reports
  fallback-required with manual steps when the desktop environment has no
  portable settings target

#### Scenario: Settings opening fails

- **WHEN** the operating system rejects or cannot open the settings destination
- **THEN** ClipVault keeps the modal open, shows the manual navigation steps,
  and does not terminate or alter history

#### Scenario: Untrusted settings target

- **WHEN** a settings command receives a target not represented by the known
  platform enum
- **THEN** ClipVault rejects it locally without executing an arbitrary URL or
  command

### Requirement: Re-check permissions after remediation

ClipVault SHALL re-evaluate relevant platform capabilities when the user
selects Reintentar after returning from system settings.

#### Scenario: macOS permission was granted

- **WHEN** the user grants Accessibility and selects Reintentar
- **THEN** ClipVault refreshes the capability state, enables supported paste
  controls and does not automatically paste until the user explicitly selects
  a paste action

#### Scenario: Linux configuration was changed

- **WHEN** the user changes a Linux desktop, sandbox or session configuration
  and selects Reintentar
- **THEN** ClipVault refreshes the capability state and reports the resulting
  support without requiring a duplicate paste

#### Scenario: Permission or capability is still unavailable

- **WHEN** the user selects Reintentar without resolving the issue
- **THEN** ClipVault keeps the guidance available, reports that the issue is
  still present and leaves the history intact

### Requirement: Preserve privacy in guidance

Platform guidance SHALL remain local and SHALL not expose clipboard payloads.

#### Scenario: Error is shown to the user

- **WHEN** a platform error is rendered in the modal or written to local
  diagnostics
- **THEN** it contains only capability, cause and remediation context, never
  the copied text, password, token, private key or full clipboard value
