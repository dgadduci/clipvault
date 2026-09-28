## ADDED Requirements

### Requirement: Linux clipboard backend follows the active session

The Linux clipboard adapter SHALL select a usable clipboard transport for the
active session. In a Wayland session it SHALL use a native Wayland clipboard
protocol when the build and compositor support it, without requiring an
XWayland server to capture native Wayland copies. It MAY use XWayland as an
explicit fallback only when the X11 connection and clipboard are usable. If
neither route works, it SHALL return a typed unavailable/read error and the
watcher SHALL continue polling.

The clipboard payload and its change/revision signal MUST describe the same
clipboard domain. An X11/XFixes revision MUST NOT be treated as authoritative
for a native Wayland read when X11 does not observe the corresponding change.
When no trustworthy revision is available, the adapter SHALL report it as
unknown so the existing watcher can compare payload fingerprints; it MUST NOT
invent a revision or suppress a valid changed payload using a stale counter.
The behavior of supported text, rich text and image payloads SHALL remain
within the existing capture pipeline.

#### Scenario: Native Wayland copy is captured in KDE Plasma

- **GIVEN** ClipVault is running in an Arch Linux KDE Plasma Wayland session
- **AND** a non-empty supported payload is copied by a native Wayland
  application
- **WHEN** the shared capture watcher polls the clipboard
- **THEN** ClipVault captures it through the Wayland clipboard transport
  supported by the tested KDE runtime and sends it through the existing
  privacy, deduplication and persistence pipeline
- **AND** capture from the native Wayland application does not depend on an
  X11 selection change

#### Scenario: Native Wayland read is not suppressed by an X11 revision

- **GIVEN** a Wayland clipboard read returns a payload that differs from the
  last observed payload
- **AND** an X11/XFixes monitor reports the same revision as its previous
  observation
- **WHEN** the capture watcher evaluates the observation
- **THEN** it does not classify the Wayland payload as unchanged solely from
  the X11 revision
- **AND** it evaluates the payload using the native revision, if available,
  or the existing fingerprint fallback

#### Scenario: Native Wayland protocol is unavailable

- **GIVEN** a Linux Wayland session whose compositor or build cannot initialize
  the native clipboard transport
- **WHEN** ClipVault starts or a clipboard read is attempted
- **THEN** diagnostics report the typed native-backend outcome without
  exposing clipboard content or absolute paths
- **AND** a usable XWayland clipboard may be selected as an explicitly
  identified fallback
- **AND** if no clipboard route is usable, the watcher continues polling and
  the rest of ClipVault remains available

#### Scenario: X11 clipboard capture remains unchanged

- **GIVEN** ClipVault is running in a Linux X11 session
- **WHEN** an application copies supported text, rich text or an image
- **THEN** the existing X11 clipboard transport and XFixes change tracking
  continue to deliver the payload to the shared capture pipeline

#### Scenario: Clipboard backend diagnostics remain metadata-only

- **WHEN** the platform reports selected backend, initialization, read or
  change-detection status
- **THEN** diagnostics contain only stable backend/state/error identifiers
- **AND** contain no clipboard content, snippets, hashes, bytes, absolute
  socket paths, window titles, process IDs or raw environment-variable values

#### Scenario: Recopying identical content preserves deduplication semantics

- **GIVEN** a supported clipboard payload has already been captured or its
  history entry was explicitly deleted
- **WHEN** the same payload is copied again on a Linux session
- **THEN** the existing duplicate policy is preserved
- **AND** a trustworthy native change signal, when available, can distinguish
  a new copy from an unchanged clipboard without persisting a duplicate row
