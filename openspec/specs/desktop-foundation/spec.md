## Purpose

Definir la base desktop local, la separación arquitectónica y la persistencia SQLite sobre la que se construye ClipVault.

## Requirements

### Requirement: Desktop application foundation

ClipVault SHALL provide a Tauri 2 desktop application with a Svelte and TypeScript frontend and a Rust backend that can start in development mode on macOS and Linux.

#### Scenario: Application starts in development mode

- **WHEN** a developer runs the documented development command
- **THEN** the Tauri application opens a functional desktop window without requiring cloud services, PostgreSQL, Docker or a remote server

#### Scenario: Application restarts

- **WHEN** the application is closed and started again
- **THEN** it loads its local configuration and clipboard history without losing previously persisted data

### Requirement: Business logic boundary

The application SHALL keep clipboard, persistence, search and lifecycle logic in Rust modules independent from Tauri commands and the frontend.

#### Scenario: Core logic is tested without GUI

- **WHEN** core unit tests run without a desktop window or real clipboard
- **THEN** they can exercise the relevant business behavior through platform abstractions or test doubles

#### Scenario: Frontend requests a business operation

- **WHEN** the frontend needs to read, search or modify clipboard history
- **THEN** it invokes a thin Tauri adapter that delegates to the Rust core instead of accessing SQLite directly

### Requirement: Embedded local database

ClipVault SHALL use an embedded SQLite database under `~/.clipvault/clipvault.db` with explicit migrations, indexes and transactions for persisted application data.

#### Scenario: First application start

- **WHEN** ClipVault starts without an existing database
- **THEN** it creates the `~/.clipvault/` directory, initializes the SQLite database and applies all migrations successfully

#### Scenario: Database migration

- **WHEN** a newer application version starts with an older database schema
- **THEN** it applies the pending migrations transactionally before exposing the database to the rest of the application

#### Scenario: Database failure

- **WHEN** SQLite cannot be opened or a migration fails
- **THEN** the application reports a useful local error and does not silently discard existing data

### Requirement: Local-only runtime

The MVP SHALL operate without mandatory network access, user accounts, remote APIs or telemetry.

#### Scenario: Offline startup

- **WHEN** the computer has no network connection
- **THEN** ClipVault starts and provides its local history, search and paste workflows

#### Scenario: Runtime instrumentation

- **WHEN** the application writes diagnostic logs
- **THEN** logs remain local and contain no clipboard content, secrets, tokens or passwords

### Requirement: Bootstrap diagnostics command

ClipVault SHALL expose a `clipvault_diagnostics` Tauri command that returns, in a single local call, the application version, the resolved SQLite database path, the number of applied migrations and the bootstrap timestamp.

#### Scenario: Frontend requests diagnostics

- **WHEN** the frontend invokes `clipvault_diagnostics` after startup
- **THEN** the core returns the version, database path, applied migration count and a UTC bootstrap timestamp without performing any network call

#### Scenario: Diagnostics reported even with no captured data

- **WHEN** ClipVault has never captured any clipboard content
- **THEN** `clipvault_diagnostics` still reports the database path, migration count and bootstrap timestamp successfully

### Requirement: Startup splash gates the first desktop reveal

At application startup, ClipVault SHALL keep the main desktop window hidden
and show a splash window with the local application logo and a short,
imaginative tagline. The splash SHALL remain visible for at least three
seconds from its first visible frame and until initial backend and desktop
loading has settled. After both conditions are met, the splash SHALL close
and the main desktop SHALL be shown and focused. The startup flow MUST NOT
show the main window's intermediate “Connecting to the backend” screen or
briefly reveal the main window before the splash.
The splash background SHALL use the same dark color at the native window and
document levels from its first frame. The splash entry point SHALL load its
small frontend module without eagerly evaluating the main desktop module.

#### Scenario: Startup begins with the splash

- **WHEN** the user launches ClipVault
- **THEN** the splash with the local logo and localized tagline is shown
- **AND** the main desktop window remains hidden during initial loading
- **AND** the splash does not flash a white or blank webview background before
  its content renders

#### Scenario: Initial loading finishes before the minimum duration

- **GIVEN** initial loading finishes in less than three seconds
- **WHEN** three seconds have elapsed since the splash became visible
- **THEN** the splash closes and the main desktop is shown and focused

#### Scenario: Initial loading takes longer than the minimum duration

- **GIVEN** initial loading is still in progress after three seconds
- **WHEN** initial loading settles
- **THEN** the splash closes and the main desktop is shown and focused

#### Scenario: Initial loading settles with a recoverable error

- **WHEN** initial loading finishes with a recoverable error
- **THEN** the splash observes the same three-second minimum
- **AND** the main desktop opens with its existing localized error and retry
  control

#### Scenario: Splash tagline follows the saved locale

- **WHEN** the splash is shown
- **THEN** its tagline uses the saved interface locale from the local
  en/es/pt/de/fr catalogs
- **AND** English is used when no supported locale has been saved

### Requirement: Application surfaces use the distinctive ClipVault mark

ClipVault SHALL use a distinctive ClipVault mark as the source for its
application icon assets on supported desktop operating systems. The mark
SHALL combine a clip and a vault motif and SHALL NOT resemble a generic
document, word processor, or writing tool icon. The icon formats bundled for
macOS and Linux SHALL be regenerated from one square application icon
composition using that mark. The startup splash SHALL show the transparent
mark directly over its existing dark background.

#### Scenario: Application bundles use the distinctive ClipVault mark

- **WHEN** ClipVault is installed or launched from a supported desktop build
- **THEN** the app icon and tray icon use the distinctive ClipVault mark
- **AND** the app icon remains legible on light and dark desktop surfaces
- **AND** it does not appear as a generic document or word processor icon

#### Scenario: Startup splash uses the distinctive mark

- **WHEN** the startup splash is displayed
- **THEN** it shows the transparent ClipVault mark on the splash background
