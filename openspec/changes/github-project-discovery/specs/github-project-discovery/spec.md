## ADDED Requirements

### Requirement: GitHub project information guides a new user from discovery to first use

The public project information SHALL provide a consistent path from a concise
description of ClipVault to an official release, the appropriate installation
guide, and a support route. The path SHALL be available in English and Spanish.

#### Scenario: A visitor evaluates ClipVault

- **WHEN** a visitor opens the repository landing page
- **THEN** the page explains what ClipVault does using capabilities present in
  the referenced published version
- **AND** it links to current official releases and language-appropriate setup
  guidance
- **AND** it does not present planned or unverified capabilities as available

#### Scenario: A visitor chooses a desktop configuration

- **WHEN** a visitor follows an installation guide for macOS or Linux
- **THEN** the guide identifies the official artifact and tested desktop
  configuration it covers
- **AND** it distinguishes X11 from Wayland where the behavior differs
- **AND** it labels unverified configurations instead of implying universal
  compatibility

#### Scenario: Public examples show the application

- **WHEN** repository documentation or social preview includes product imagery
- **THEN** the imagery represents the current application and logo
- **AND** user-provided imagery has explicit user approval for public release
- **AND** the image contains no credentials or secrets

#### Scenario: A user looks for help

- **WHEN** a user needs installation help or wants to report a problem
- **THEN** the landing page links to a maintained route with instructions in
  English and Spanish
- **AND** the route does not ask the user to disclose clipboard contents,
  credentials or other secrets
