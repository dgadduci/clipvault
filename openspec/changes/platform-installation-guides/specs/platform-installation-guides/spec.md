## ADDED Requirements

### Requirement: New users can install ClipVault using published desktop artifacts

The repository SHALL provide installation and first-run guides in English and
Spanish for macOS, Ubuntu GNOME and Arch KDE Plasma. The guides SHALL use only
official artifacts and SHALL distinguish Linux desktop sessions where
installation or runtime steps differ.

The Linux download selector SHALL describe the distribution families suited
to each published package format. It SHALL distinguish package-format
applicability from manual verification of a specific distribution, desktop or
session.

#### Scenario: macOS user installs ClipVault

- **WHEN** a macOS user follows the guide
- **THEN** it identifies the official `.dmg` for the user's processor
  architecture
- **AND** it explains any current Gatekeeper prompt without asking the user to
  disable Gatekeeper globally

#### Scenario: Ubuntu GNOME user selects a package

- **WHEN** an Ubuntu GNOME user follows the guide
- **THEN** it explains the validated graphical installation path for the
  official `.deb`
- **AND** it identifies AppImage only as a validated alternative
- **AND** it states whether the instructions apply to the user's X11 or
  Wayland session

#### Scenario: Arch KDE Plasma user selects a package

- **WHEN** an Arch KDE Plasma user follows the guide
- **THEN** it recommends an official artifact that was tested with that
  configuration
- **AND** it does not label a Debian package as an Arch package or imply that
  an AUR package exists
- **AND** it distinguishes optional KDE integration from steps required to
  launch the application

#### Scenario: A Linux user selects a package for another distribution

- **WHEN** a user on an x86_64 Debian/Ubuntu-based distribution or another
  mainstream Linux distribution selects a download
- **THEN** the guide identifies the `.deb` for Ubuntu and compatible
  Debian/Ubuntu derivatives, and the AppImage as the portable format for most
  other Linux distributions
- **AND** it notes that `.deb` dependency availability and AppImage runtime
  requirements such as FUSE can vary
- **AND** it keeps untested distributions and desktop sessions marked as
  unverified

#### Scenario: A user follows a first-run instruction

- **WHEN** a user reaches a permission or desktop-integration step
- **THEN** the guide matches the UI and behavior of the referenced published
  version
- **AND** it identifies environment-specific steps and optional integrations
- **AND** it avoids unsupported commands and broad changes to system security

#### Scenario: A configuration has not been verified

- **WHEN** the documentation mentions an operating system, desktop or session
  without an approved manual result
- **THEN** it labels that configuration as unverified or omits the compatibility
  claim
- **AND** it does not infer Wayland behavior from X11 results or vice versa
