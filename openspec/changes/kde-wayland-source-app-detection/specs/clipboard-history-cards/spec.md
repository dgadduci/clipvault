## ADDED Requirements

### Requirement: KDE Wayland source metadata uses the Linux application provider

For a permitted capture with a valid KWin application identifier, ClipVault
SHALL reuse `LinuxApplicationMetadataProvider` and the existing
`source_app_name` and `source_app_icon_ref` fields. It MUST NOT add a parallel
`.desktop` parser or persist an absolute KWin-reported path. Missing local
application metadata MUST NOT block clipboard capture.

#### Scenario: KDE application name and icon are resolved locally

- **GIVEN** a permitted KDE Plasma Wayland capture has a valid KWin
  application identifier matching a local `.desktop` entry
- **AND** the entry has a resolvable display name and icon
- **WHEN** the existing metadata-enrichment step runs
- **THEN** the history entry receives the local application name and
  controlled `application-icons/` reference
- **AND** the existing card presentation renders that name and icon

#### Scenario: KDE application metadata cannot be resolved

- **GIVEN** a permitted KDE Plasma Wayland capture has no matching local
  `.desktop` entry or usable icon
- **WHEN** metadata enrichment completes
- **THEN** the valid clipboard payload remains in history
- **AND** the current unknown-source or missing-icon fallback is shown

#### Scenario: Blacklisted KDE application has no metadata side effect

- **GIVEN** KWin reports an application identifier matched by the ignored-app
  list
- **WHEN** a clipboard payload is captured
- **THEN** `PrivacyGate` rejects the payload before metadata lookup
- **AND** no new application icon asset is written
