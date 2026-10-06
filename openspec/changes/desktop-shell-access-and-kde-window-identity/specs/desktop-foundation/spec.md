## ADDED Requirements

### Requirement: Startup splash gates the first desktop reveal

At application startup, ClipVault SHALL keep the main desktop window hidden
and show a splash window with the local application logo and a short,
imaginative tagline. The splash SHALL remain visible for at least three
seconds from its first visible frame and until initial backend and desktop
loading has settled. After both conditions are met, the splash SHALL close
and the main desktop SHALL be shown and focused. The startup flow MUST NOT
show the main window's intermediate “Connecting to the backend” screen or
briefly reveal the main window before the splash.

#### Scenario: Startup begins with the splash

- **WHEN** the user launches ClipVault
- **THEN** the splash with the local logo and localized tagline is shown
- **AND** the main desktop window remains hidden during initial loading

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
