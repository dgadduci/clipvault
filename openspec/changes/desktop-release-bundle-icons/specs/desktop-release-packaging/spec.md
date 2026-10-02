## ADDED Requirements

### Requirement: Release bundles use platform-compatible application icons

The desktop release workflow SHALL generate its application icons from one
versioned square SVG source before bundling. macOS bundles SHALL include a
valid ICNS icon, and Linux bundles SHALL include square PNG icons generated
from the same source.

#### Scenario: macOS release bundle has a supported icon

- **WHEN** the release workflow builds an Apple Silicon or Intel macOS target
- **THEN** Tauri generates `icon.icns` from the versioned SVG before bundling
- **AND** the app bundle contains the generated ICNS application icon

#### Scenario: Linux release bundles use the same icon source

- **WHEN** the release workflow builds Linux packages
- **THEN** Tauri generates square PNG application icons from the same SVG
- **AND** the release bundle configuration selects a generated PNG icon

#### Scenario: Icon generation does not add generated files to source control

- **WHEN** a release build runs on a hosted runner
- **THEN** all platform icon files are generated in the runner workspace
- **AND** only the SVG source is required in the repository
