## MODIFIED Requirements

### Requirement: Configurable ignored-application blacklist

ClipVault SHALL provide a local setting that allows the user to add, remove
and view ignored applications using platform metadata selected through a
supported application picker, and SHALL compare each captured clipboard event
against the normalized application identifier before persisting content.

#### Scenario: Select an application from macOS Applications

- **WHEN** the user presses Seleccionar aplicación and selects a valid .app bundle from the native macOS picker
- **THEN** ClipVault extracts its identifier, visible name and optional icon, persists the normalized identifier and displays the application by icon and name

#### Scenario: The picker starts in Applications

- **WHEN** the macOS application picker opens
- **THEN** it starts in /Applications and restricts the selectable target to application bundles without executing or modifying the selected application

#### Scenario: Cancel application selection

- **WHEN** the user closes or cancels the picker without selecting an application
- **THEN** ClipVault leaves settings and the ignored-applications list unchanged and does not show a mutation error

#### Scenario: Invalid selection

- **WHEN** the user selects a non-application file, an invalid bundle or a bundle without a usable identifier
- **THEN** ClipVault rejects the selection with a typed local error, leaves the blacklist unchanged and does not execute the selected path

#### Scenario: Application metadata is persisted

- **WHEN** a valid application is added to the blacklist
- **THEN** the identifier remains the matching key, the display name and icon metadata are persisted locally when available, and the same icon/name can be rendered after restart

#### Scenario: Missing icon does not block selection

- **WHEN** a valid application identifier and name are available but its icon cannot be extracted or stored
- **THEN** ClipVault adds the application with a deterministic generic icon and keeps the blacklist rule active

#### Scenario: Selecting the same application twice

- **WHEN** the user selects an application already present in the blacklist
- **THEN** ClipVault performs an idempotent update without creating a duplicate row and may fill missing metadata

#### Scenario: Existing identifier-only rows

- **WHEN** the database contains an ignored-application row created before picker metadata was introduced
- **THEN** ClipVault continues matching its identifier and displays a safe fallback name/icon without requiring a manual migration by the user

#### Scenario: Ignored application still blocks capture

- **WHEN** the active source identifier matches an application selected through the picker
- **THEN** the existing PrivacyGate discards the clipboard event before persistence and the content does not appear in recent or search responses

#### Scenario: Unsupported platform picker

- **WHEN** the current platform/session cannot map a selected application to a stable identifier used by its active-app adapter
- **THEN** ClipVault reports unsupported_session or backend_unavailable, keeps the settings panel usable and does not invent an application identifier

#### Scenario: Ignored-application list is metadata-only

- **WHEN** the GUI requests or renders ignored applications
- **THEN** the response contains only application metadata required by the UI, never clipboard content, clipboard hashes, snippets or capture payloads

### Requirement: Local minimal settings

ClipVault SHALL expose ignored-application records through local settings and
preserve their behavior across restarts without requiring an account, network
call or remote configuration.

#### Scenario: Selected application survives restart

- **WHEN** the user selects an application, exits ClipVault and launches it again
- **THEN** the ignored-applications list contains the same normalized identifier and available presentation metadata, and PrivacyGate uses the persisted identifier without another selection

## ADDED Requirements

### Requirement: Native application metadata picker

ClipVault SHALL expose a platform adapter for selecting an installed
application and extracting safe presentation metadata without putting platform
or filesystem logic in the frontend.

#### Scenario: macOS native picker returns metadata

- **WHEN** the native macOS picker selects a valid application bundle
- **THEN** the adapter returns a non-empty identifier, a display name when available and an optional icon representation, without launching the application

#### Scenario: Picker adapter is testable without GUI

- **WHEN** core or shell tests use a fake application picker
- **THEN** they can exercise valid, cancelled, invalid, unsupported and metadata-failure outcomes without opening a real system dialog

### Requirement: Safe application icon handling

ClipVault SHALL store or reference selected application icons only through a
local representation controlled by ClipVault and SHALL provide a generic
fallback when icon extraction is unavailable.

#### Scenario: Icon reference is safe to render

- **WHEN** the frontend renders an ignored-application row
- **THEN** it uses a validated local icon representation or a generic fallback and never loads an arbitrary user-supplied filesystem path or remote URL

#### Scenario: Icon metadata fails independently

- **WHEN** icon conversion or persistence fails after identifier validation
- **THEN** ClipVault keeps the valid blacklist identifier and display name, reports non-fatal metadata status and does not roll back the privacy rule

### Requirement: Application-picker privacy boundary

The picker flow SHALL remain local and SHALL not couple application metadata
selection with clipboard payloads or capture diagnostics.

#### Scenario: Selection produces no clipboard side effects

- **WHEN** the user selects, cancels or rejects an application
- **THEN** ClipVault does not read, write, log or emit clipboard content, hash, snippet or source-capture payload as part of the picker flow

#### Scenario: Unsupported Linux session is explicit

- **WHEN** Linux cannot provide a stable mapping from an installed application entry to the active-app identifier used by the blacklist
- **THEN** ClipVault reports the unsupported capability without inventing an identifier and leaves existing local settings and capture pipeline unchanged
