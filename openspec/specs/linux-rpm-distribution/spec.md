# Linux RPM distribution

## Purpose

Define the RPM package, compatible update path and local-data guarantees for
ClipVault on supported Linux systems.

## Requirements

### Requirement: Linux releases include an x86_64 RPM package

Each supported stable Linux release SHALL include an x86_64 RPM bundle built
with Tauri's RPM bundler and attached to the matching GitHub draft release.
The package SHALL contain the ClipVault desktop application, accurate package
metadata and only verified runtime dependencies. Documentation SHALL name only
RPM-based distributions and versions that have passed installation
verification.

#### Scenario: RPM release artifact is built

- **WHEN** a trusted stable release tag is built
- **THEN** CI produces an x86_64 RPM whose version matches the release tag and
  the Cargo, Tauri and frontend application versions
- **AND** the RPM is attached to the matching draft GitHub Release
- **AND** existing AppImage and DEB artifacts remain available

#### Scenario: User installs on a supported RPM distribution

- **WHEN** a user installs the published RPM on a declared supported system
- **THEN** the system package manager reports the correct application name,
  version, architecture and dependencies
- **AND** ClipVault can start without missing runtime libraries

#### Scenario: User uses an unverified RPM distribution

- **WHEN** a user consults the installation guide for a distribution that was
  not tested
- **THEN** ClipVault does not claim verified compatibility for that system

### Requirement: RPM installations receive compatible signed updates

An RPM installation SHALL receive only a compatible RPM update artifact
verified with the configured Tauri updater public key. Replacing an installed
RPM SHALL use the operating system's supported authorization flow. The update
flow SHALL NOT substitute an AppImage or DEB for the RPM package.

#### Scenario: User accepts an RPM update

- **WHEN** a newer compatible RPM update is available and the user approves
  installation
- **THEN** ClipVault verifies the updater signature before installation
- **AND** the operating system requests the authorization needed to replace
  the package
- **AND** the application relaunches only after successful installation

#### Scenario: User declines authorization or signature verification fails

- **WHEN** the user cancels system authorization or the RPM updater signature
  is missing or invalid
- **THEN** the update is not installed
- **AND** the existing application, database and persisted assets remain
  available

### Requirement: RPM package removal preserves user data

Removing the RPM package SHALL remove application-owned system files through
the package manager without deleting ClipVault's user database, captures or
persisted image assets.

#### Scenario: User removes the RPM package

- **WHEN** the user uninstalls ClipVault with the system package manager
- **THEN** the application package is removed
- **AND** the user's local ClipVault data remains available for a future
  installation
