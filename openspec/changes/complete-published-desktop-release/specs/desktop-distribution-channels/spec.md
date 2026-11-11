## ADDED Requirements

### Requirement: A published desktop release can be completed for a platform

The release workflow SHALL allow a trusted stable release that is already
published to receive Linux artifacts when Linux was not built in the first
publication. A completion run SHALL validate the release tag and SHALL build
only Linux x86_64, preserving assets already attached to the release.

#### Scenario: Linux completes a published macOS release

- **WHEN** an authorized maintainer manually runs the release workflow for an
  existing published stable tag whose source commit is reachable from `main`
- **THEN** CI validates the tag and all canonical application versions
- **AND** CI builds only Linux x86_64 AppImage, DEB and RPM packages
- **AND** the packages are attached to the existing published release without
  removing its macOS assets
- **AND** the release description reflects the available macOS and Linux
  installers

#### Scenario: Completion updates the updater manifest

- **WHEN** a platform completion run uploads signed updater artifacts
- **THEN** the published updater manifest retains existing platform entries
- **AND** it includes the newly built platform artifacts and signatures

#### Scenario: Manual completion targets an unavailable release

- **WHEN** the selected tag is not a stable version reachable from `main`, or
  its GitHub release is a draft or prerelease
- **THEN** validation fails before a build starts
