## MODIFIED Requirements

### Requirement: Linux releases include an x86_64 RPM package

Each supported stable Linux release SHALL include an x86_64 RPM bundle built
with Tauri's RPM bundler and attached to the matching GitHub Release. The
package SHALL contain the ClipVault desktop application, accurate package
metadata and only verified runtime dependencies. Documentation SHALL name
only RPM-based distributions and versions that have passed installation
verification.

#### Scenario: RPM release artifact is built

- **WHEN** a trusted stable release tag is built
- **THEN** CI produces an x86_64 RPM whose version matches the release tag and
  the Cargo, Tauri and frontend application versions
- **AND** the RPM is attached to the matching GitHub Release, whether the
  release is still a draft or has already been published
- **AND** existing AppImage, DEB and desktop platform artifacts remain
  available
