# macOS Homebrew tap

## Purpose

Define installation and security behavior for ClipVault's official Homebrew
tap on macOS.

## Requirements

### Requirement: ClipVault is installable from its own Homebrew tap

The project SHALL provide a public Homebrew tap named
dgadduci/homebrew-tap with a ClipVault cask that installs the official DMG
from GitHub Releases. The cask SHALL support Apple Silicon and Intel through
architecture-appropriate download URLs and SHA-256 values. It SHALL NOT use
sha256 :no_check.

#### Scenario: User installs ClipVault from the tap

- **WHEN** a user adds dgadduci/tap, explicitly trusts only the ClipVault cask
  with `brew trust --cask dgadduci/tap/clipvault`, and installs it
- **THEN** Homebrew downloads the official DMG for the user's Mac architecture
- **AND** the cask version and SHA-256 match that release artifact
- **AND** ClipVault is installed in the standard macOS Applications location

#### Scenario: Homebrew requires trust for the external tap

- **WHEN** Homebrew refuses to load ClipVault because dgadduci/tap is untrusted
- **THEN** the installation guide instructs the user to run
  `brew trust --cask dgadduci/tap/clipvault`
- **AND** it does not instruct the user to trust all casks in dgadduci/tap

#### Scenario: Cask metadata is updated for a release

- **WHEN** a new stable macOS release is published
- **THEN** the tap's cask version, architecture URLs and SHA-256 values are
  reviewed and updated to match the new official DMGs
- **AND** the tap does not require a long-lived personal access token in the
  ClipVault release workflow

### Requirement: The Homebrew tap preserves macOS security checks

The Homebrew cask SHALL preserve macOS quarantine and Gatekeeper behavior. The
tap and its documentation SHALL NOT disable or bypass Gatekeeper or instruct
users to remove quarantine attributes. Documentation SHALL explain both the
limited Homebrew cask-trust command and that the current ad-hoc signed DMG may
require the user to approve ClipVault manually in macOS Privacy & Security
before first launch.

#### Scenario: Gatekeeper blocks the first launch

- **WHEN** Gatekeeper blocks ClipVault downloaded through Homebrew
- **THEN** the installation guide explains the supported per-application
  approval flow in macOS Privacy & Security
- **AND** it does not tell the user to disable Gatekeeper globally or remove
  the quarantine flag

### Requirement: Homebrew-installed ClipVault retains its supported updater

The Homebrew cask SHALL identify ClipVault as an application with its own
updater when the existing Tauri updater is available in that release. The
tap's version metadata SHALL remain suitable for fresh installations while
normal application updates continue through the supported in-app updater.

#### Scenario: User checks for an update after installing through Homebrew

- **WHEN** a newer compatible signed macOS release is available
- **THEN** the existing ClipVault updater offers it using the normal explicit
  confirmation flow
- **AND** the cask does not replace the app with a different build or remove
  local user data
