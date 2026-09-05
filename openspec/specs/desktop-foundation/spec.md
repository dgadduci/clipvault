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
