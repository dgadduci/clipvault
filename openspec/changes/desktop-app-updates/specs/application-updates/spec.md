## ADDED Requirements

### Requirement: Signed updates are available to desktop clients

ClipVault SHALL publish versioned desktop releases for macOS and Linux through
GitHub Releases. Each release SHALL include a platform-, architecture- and
installer-compatible update manifest and Tauri-signed updater artifacts. The
client SHALL reject an artifact whose signature cannot be verified with the
configured public key. Release tags and the canonical Cargo, Tauri and
frontend application versions SHALL identify the same SemVer version.

#### Scenario: Release pipeline publishes compatible signed artifacts

- **WHEN** a trusted `vMAJOR.MINOR.PATCH` tag is built by the release workflow
- **THEN** the workflow creates a draft GitHub Release with the supported
  macOS and Linux installers and updater artifacts
- **AND** the generated manifest identifies the correct artifact for each
  supported platform, architecture and installer type
- **AND** every updater artifact has a signature that verifies with the
  public key embedded in ClipVault
- **AND** the release cannot be published if the tag and application versions
  disagree

#### Scenario: Client refuses an invalidly signed update

- **WHEN** the updater receives an artifact with an invalid or missing
  signature
- **THEN** ClipVault does not install or relaunch into that artifact
- **AND** the currently installed application and local user data remain
  available

### Requirement: Update checks do not interrupt normal use

Production builds SHALL check for updates asynchronously without blocking
startup or ordinary ClipVault use. Locally packaged production builds SHALL
use the same configured endpoint and verification key as release builds. The
user SHALL be able to initiate another check from “Acerca de”. Development
builds SHALL NOT contact the release endpoint. Update checks and downloads
SHALL NOT transmit clipboard content, history records, titles, local paths or
local entry identifiers.

#### Scenario: Update check finds no release

- **WHEN** a production client checks GitHub and there is no compatible newer
  version
- **THEN** ClipVault continues normal use without opening an installation
  prompt
- **AND** “Acerca de” can report that the installed version is current

#### Scenario: Update service is offline or unavailable

- **WHEN** a check cannot reach GitHub or the release manifest is unavailable
- **OR** the update check exceeds its configured request timeout
- **THEN** startup and all existing application features remain usable
- **AND** “Acerca de” presents a recoverable status and permits a manual retry

#### Scenario: Locally packaged production build checks for updates

- **WHEN** the user opens “Acerca de” in a locally packaged Tauri production
  build on macOS or Linux
- **THEN** the update action is enabled and checks the configured GitHub release
  endpoint
- **AND** a compatible signed release can be installed after explicit user
  approval
- **AND** the local build does not require the updater private signing key

#### Scenario: Local macOS production bundle has a compatible application icon

- **WHEN** a contributor packages the macOS application from a clean checkout
- **THEN** the configured icon set includes the `.icns` asset required by the
  `.app` bundle
- **AND** the local build uses the same versioned icon assets as release CI

#### Scenario: Development build starts

- **WHEN** ClipVault runs from a development build
- **THEN** it does not query GitHub for production updates

#### Scenario: Update action in a build without updater support

- **WHEN** the user opens “Acerca de” in a development build where updater
  checks are disabled
- **THEN** the update action remains visible but disabled
- **AND** a localized message explains that updates are available in installed
  ClipVault releases
- **AND** the build does not contact the release endpoint

### Requirement: Users approve update installation

ClipVault SHALL display an available compatible version and SHALL require an
explicit user action before downloading and installing it. The interface SHALL
show available progress and recoverable errors. After successful installation,
ClipVault SHALL relaunch into the new version only through the updater's
supported relaunch flow.

#### Scenario: User approves an available update

- **WHEN** an update is available and the user selects the install action
- **THEN** ClipVault downloads the compatible signed artifact and displays
  progress when available
- **AND** it installs only after signature verification succeeds
- **AND** it requests/reports relaunch after installation completes

#### Scenario: User does not approve an update

- **WHEN** an update is available and the user closes “Acerca de” or declines
  to install
- **THEN** ClipVault keeps running the current version without downloading or
  installing the update

#### Scenario: Installation fails

- **WHEN** the download, signature check or installation fails
- **THEN** ClipVault retains the current usable version and local data
- **AND** “Acerca de” displays a recoverable error and permits retry

### Requirement: Linux package updates respect installer permissions

Linux clients SHALL receive an update artifact compatible with the installer
type from which ClipVault is running. An AppImage update SHALL not require
system package-manager privileges. A `.deb` update SHALL use the supported
system authorization flow before replacing the installed package. Declining
authorization SHALL cancel the update without breaking the installed client.

#### Scenario: Installed Linux AppImage is updated

- **WHEN** an AppImage installation accepts a valid signed update
- **THEN** the update targets the AppImage bundle compatible with its
  architecture
- **AND** the update does not invoke the `.deb` package installation flow

#### Scenario: Installed Linux Debian package is updated

- **WHEN** a `.deb` installation accepts a valid signed update
- **THEN** the update uses the `.deb` artifact compatible with its
  architecture
- **AND** the operating system requests the authorization required to replace
  the package
- **AND** cancelling authorization leaves the installed version usable

### Requirement: macOS releases support ad-hoc signing without Apple credentials

The macOS release workflow SHALL build Apple Silicon and Intel bundles using
the ad-hoc signing identity when no Apple Developer credentials are configured.
The workflow SHALL NOT require Apple Developer ID or notarization secrets for
this mode. Release documentation SHALL explain that ad-hoc signing does not
identify the publisher to Gatekeeper and that users may need to approve
ClipVault manually in macOS Privacy & Security settings. Tauri updater
signatures SHALL remain required independently of macOS code signing.

#### Scenario: User opens an ad-hoc signed macOS release

- **WHEN** a user downloads and opens ClipVault from a GitHub release
- **THEN** the app bundle has an ad-hoc code signature and a valid Tauri updater
  signature
- **AND** the user is told how to approve the app in Privacy & Security if
  Gatekeeper blocks the first launch
- **AND** the documentation does not instruct users to disable Gatekeeper
  globally

#### Scenario: Release build runs without Apple Developer secrets

- **WHEN** the release workflow builds a trusted version tag without Apple
  Developer ID or notarization credentials
- **THEN** macOS Apple Silicon and Intel artifacts are still produced using
  ad-hoc signing
- **AND** Linux artifacts retain their normal updater signing and packaging

### Requirement: Update behavior is localized and preserves local data

All update-related product and accessible text SHALL be consumed through
translation keys present in the `en`, `es`, `pt`, `de` and `fr` catalogs. An
application update SHALL preserve the existing local database and persisted
assets.

#### Scenario: User views update status in any supported language

- **WHEN** an update status, action, progress message or error is shown
- **THEN** it is rendered using the selected language's translation catalog
- **AND** all five catalogs define the same update keys and interpolation
  placeholders

#### Scenario: Application updates successfully

- **WHEN** a compatible update is installed and ClipVault relaunches
- **THEN** the existing local database and persisted assets remain available
- **AND** no clipboard history is included in the update request or package
