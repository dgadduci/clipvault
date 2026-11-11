## ADDED Requirements

### Requirement: Guide the user to configure a visible name before sharing

ClipVault SHALL require a saved, non-blank local visible device name before it
shows the local-network sharing form. When the name is missing, the
`Share on local network` modal SHALL show only concise instructions to save it
under **Settings → Privacy → Display name**, then return to the sharing modal.
It SHALL hide the sharing toggle, runtime controls and peer list, and SHALL
NOT request sharing status or the peer snapshot. When the name is configured,
ClipVault SHALL show the existing sharing form and peer content without
automatically enabling network discovery.

#### Scenario: Name has not been configured

- **WHEN** the user opens `Share on local network` and the saved display name
  is absent, blank or whitespace-only
- **THEN** the modal shows only instructions to open **Settings → Privacy**,
  enter and save a **Display name**, then return to `Share on local network`
- **AND** the modal does not render the sharing toggle, runtime status,
  identity-retry control or peer list
- **AND** ClipVault does not request sharing status or a peer snapshot and
  does not start local-network discovery

#### Scenario: Display name is configured

- **WHEN** the saved display name is non-blank and valid
- **THEN** the modal shows the existing sharing toggle, runtime status,
  identity-retry control and peer list
- **AND** opening the modal alone does not enable sharing or start discovery

#### Scenario: User saves a display name after seeing the guidance

- **WHEN** the user saves a valid display name in **Settings → Privacy** and
  opens `Share on local network` again
- **THEN** the modal re-reads the saved configuration and shows the full
  sharing form

#### Scenario: macOS first local-network access

- **GIVEN** a display name is configured and sharing is still disabled
- **WHEN** the user explicitly enables sharing for the first time on macOS
- **THEN** ClipVault starts its existing local discovery flow and macOS may
  request Local Network permission
- **AND** the guidance explains that the user can then manage ClipVault under
  **System Settings → Privacy & Security → Local Network**
- **AND** the modal does not imply that installation alone adds ClipVault to
  the system permission list

#### Scenario: Configuration cannot be read

- **WHEN** the modal cannot determine whether a display name is configured
- **THEN** it shows a safe loading/error state and a retry action without
  exposing the sharing form or starting network discovery
