## ADDED Requirements

### Requirement: The repository landing page explains ClipVault and directs visitors to a release

The repository SHALL provide a root README in English and a complete Spanish
equivalent. The two pages SHALL accurately describe the current published
application, link to official releases and installation guidance, and avoid
claims based only on roadmap plans.

#### Scenario: An English-speaking visitor opens the repository

- **WHEN** the visitor opens the root repository page
- **THEN** the opening section states what ClipVault does and its current
  desktop targets
- **AND** the page links to official releases and installation instructions
- **AND** it provides a visible link to the Spanish version

#### Scenario: A Spanish-speaking visitor chooses the Spanish page

- **WHEN** the visitor opens the Spanish README
- **THEN** it conveys the same product capabilities, platform status, download
  and privacy information as the English README
- **AND** it provides a visible link back to English

#### Scenario: A visitor reviews product capabilities

- **WHEN** the README lists a feature, package or supported configuration
- **THEN** the claim is present in the published version or verified release
  process it describes
- **AND** planned capabilities and unverified configurations are identified
  as such or omitted

#### Scenario: A visitor scans the feature overview
- **WHEN** the README summarizes the current application
- **THEN** it covers clipboard capture, manual text entries and editing,
  search and organization, Quick Paste, and opt-in local-network sharing
- **AND** it describes peer captures as items the user explicitly imports,
  without implying automatic history synchronization

### Requirement: Public project identity uses accurate, privacy-safe assets and metadata

The repository SHALL provide a current social preview and reviewable GitHub
About metadata. Public product imagery SHALL use synthetic data or be
explicitly approved by the user for publication, and SHALL use the current
ClipVault identity. Images SHALL NOT expose credentials or secrets.

#### Scenario: GitHub displays the project social preview

- **WHEN** a maintainer uploads the repository's prepared social preview
- **THEN** the image has the required GitHub preview dimensions and identifies
  ClipVault with the current logo
- **AND** it contains no personal clipboard data or unsupported product claims

#### Scenario: A maintainer updates repository About information

- **WHEN** a maintainer applies the documented metadata values
- **THEN** the description, topics and website link to the actual project,
  release and installation pages
- **AND** the repository change does not silently mutate remote GitHub settings
- **AND** any remote metadata update is made only after an explicit user request
- **AND** topics prioritize product capabilities and platforms over implementation-stack labels

#### Scenario: README privacy description mentions update checks

- **WHEN** the README explains local storage and privacy
- **THEN** it states that normal clipboard history remains local
- **AND** it links to the current updater network behavior
- **AND** it does not claim that the application never contacts the Internet
