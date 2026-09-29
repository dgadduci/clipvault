## ADDED Requirements

### Requirement: Wayland clipboard fallback initialization is reused

The Linux clipboard adapter SHALL reuse a successfully initialized `arboard`
client for repeated operations in the same application session. When the
compositor does not support the optional Wayland data-control protocol and
`arboard` successfully selects its existing X11 fallback, ClipVault MUST NOT
repeat the same unsupported-protocol warning on every clipboard poll. It MUST
preserve the existing metadata-only backend diagnostic fields and MUST keep
unexpected backend errors visible.

#### Scenario: Compositor does not publish data-control

- **GIVEN** ClipVault runs in a Linux Wayland session
- **AND** the compositor does not support `ext-data-control` or
  `wlr-data-control`
- **AND** the existing X11 fallback initializes successfully
- **WHEN** the watcher polls the clipboard repeatedly
- **THEN** clipboard capture and the existing fallback behavior continue
- **AND** the unsupported-protocol warning is not repeated for every poll
- **AND** existing backend diagnostic fields retain their current values

#### Scenario: Compositor supports data-control

- **GIVEN** ClipVault runs in a Linux Wayland session whose compositor
  supports a data-control protocol
- **WHEN** the clipboard client is initialized and reused for later polls
- **THEN** the adapter continues to use native Wayland clipboard access
- **AND** it does not select the X11 fallback

#### Scenario: Clipboard connection must be recreated

- **GIVEN** a previously initialized clipboard connection becomes unusable
- **WHEN** the adapter attempts the documented recovery path
- **THEN** it may recreate the client and recover clipboard access
- **AND** after successful recreation it reuses that client for subsequent
  operations
- **AND** unrelated initialization and operation errors remain observable

#### Scenario: Existing platform behavior is preserved

- **WHEN** ClipVault runs on Linux X11, macOS, or a compatible KDE Plasma
  Wayland session
- **THEN** it preserves the current clipboard backend, capture pipeline,
  deduplication, and persisted data behavior
