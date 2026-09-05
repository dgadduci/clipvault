## ADDED Requirements

### Requirement: Bootstrap diagnostics command

ClipVault SHALL expose a `clipvault_diagnostics` Tauri command that returns, in a single local call, the application version, the resolved SQLite database path, the number of applied migrations and the bootstrap timestamp.

#### Scenario: Frontend requests diagnostics
- **WHEN** the frontend invokes `clipvault_diagnostics` after startup
- **THEN** the core returns the version, database path, applied migration count and a UTC bootstrap timestamp without performing any network call

#### Scenario: Diagnostics reported even with no captured data
- **WHEN** ClipVault has never captured any clipboard content
- **THEN** `clipvault_diagnostics` still reports the database path, migration count and bootstrap timestamp successfully
