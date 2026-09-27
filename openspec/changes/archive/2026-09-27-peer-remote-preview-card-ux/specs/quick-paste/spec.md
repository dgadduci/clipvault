## ADDED Requirements

### Requirement: QuickVault shows imported source-app attribution

QuickVault recent and search rows SHALL show the locally stored source-app
icon associated with an imported entry's latest `remote_imports` provenance.
The source-app name SHALL be exposed only through the icon's `title` and
accessible label, not as visible text. QuickVault SHALL obtain metadata for
the visible entry IDs through the existing bounded local projection, SHALL NOT
request source-app metadata from a peer, and SHALL NOT wait for that projection
before rendering the result list. Missing or invalid imported icons SHALL use
the static imported-origin marker; missing names SHALL use an honest unknown
source label. A stale projection or icon response SHALL NOT replace metadata
for a newer result set. This presentation SHALL NOT mutate clipboard data,
paste behavior, or the entry's local source-app metadata.

#### Scenario: Imported entry appears in recent results

- **WHEN** a text or image entry with source-app metadata from a peer import
  appears in QuickVault recent results
- **THEN** QuickVault displays the locally resolved imported app icon and
  exposes its name through that icon's tooltip and accessible label
- **AND** it does not display the name as visible text or expose a peer ID

#### Scenario: Imported entry appears in search results

- **WHEN** an imported entry appears in QuickVault search results
- **THEN** QuickVault uses the same latest-provenance icon and name as recent
  results without a per-entry peer request

#### Scenario: Imported icon or name is unavailable

- **WHEN** the latest provenance has no valid local icon reference or name
- **THEN** QuickVault displays the static imported-origin icon and an honest
  unknown-source label for whichever field is missing

#### Scenario: Projection is pending or stale

- **WHEN** source-app projection is pending or a previous recent/search
  response completes after a newer result set
- **THEN** the result rows render without waiting, and stale metadata or icon
  responses are discarded
