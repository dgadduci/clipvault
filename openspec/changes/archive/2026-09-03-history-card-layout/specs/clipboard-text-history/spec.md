## MODIFIED Requirements

### Requirement: Capture text clipboard changes

ClipVault SHALL capture new text content when the operating system clipboard changes, SHALL ignore non-text content in MVP v0.1 without terminating the application and SHALL ignore text content whose source application matches the configured ignored-application blacklist. A permitted text capture MAY also enrich the persisted entry with the source application's user-visible display name and a controlled local icon reference for the history card without altering the captured text, hash, source identifier or content-type metadata.

#### Scenario: Allowed capture stores source application presentation metadata

- **WHEN** a permitted text capture has an available source application identifier
- **THEN** the capture pipeline may enrich the entry with the source application's display name and a controlled icon reference for the history card, while preserving the existing text, hash, source identifier and content-type behavior

#### Scenario: Application metadata failure does not block text capture

- **WHEN** the source application's name or icon cannot be resolved or stored
- **THEN** ClipVault persists the valid text capture with nullable metadata and the frontend uses a generic fallback

#### Scenario: Blacklisted capture does not create application metadata

- **WHEN** the PrivacyGate rejects a text capture from an ignored application
- **THEN** ClipVault does not persist the payload or create a new source-application icon asset as a side effect
