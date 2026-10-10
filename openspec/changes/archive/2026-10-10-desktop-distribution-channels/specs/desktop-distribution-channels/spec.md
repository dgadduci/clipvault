## ADDED Requirements

### Requirement: Desktop release channels share a clear source and version

ClipVault SHALL publish the Linux RPM and macOS Homebrew cask from the same
versioned GitHub Releases channel as its existing desktop installers. The
release documentation SHALL distinguish each installation method, supported
architecture, update route and platform security behavior.

#### Scenario: A stable release is prepared

- **WHEN** a trusted stable version tag is built
- **THEN** the GitHub draft contains the supported RPM artifact and the
  existing platform installers for that version
- **AND** the Homebrew cask metadata identifies that same version and its
  official macOS DMG assets
- **AND** the draft remains subject to the existing release review before
  publication

#### Scenario: User chooses an installation channel

- **WHEN** a user reads the installation guide
- **THEN** the guide states the relevant architecture and supported system
  requirements for RPM or Homebrew
- **AND** it does not imply that an RPM release asset is a configured system
  package repository
- **AND** it does not imply that Homebrew removes macOS Gatekeeper checks

### Requirement: New distribution channels preserve local-first behavior

The RPM and Homebrew distribution channels SHALL use official ClipVault release
artifacts and SHALL NOT transmit clipboard contents, history, account data,
telemetry or usage analytics. Updating either installation SHALL preserve the
existing local database and persisted assets.

#### Scenario: A user installs or updates through a new channel

- **WHEN** the user installs or updates ClipVault using RPM or Homebrew
- **THEN** only public release metadata and application artifacts are fetched
- **AND** existing local captures and assets remain available
