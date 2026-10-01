## ADDED Requirements

### Requirement: Include imported source applications in the active filter

The source-application filter SHALL include validated source attributions from
imported text and image captures in every active collection scope, including
`Historial`, peer-bound collections and other user collections. Its options
SHALL use the same effective attribution and deterministic precedence as the
corresponding cards, show the available display name and local icon, and use
the existing generic icon fallback when no local icon is available. Selecting
an imported application SHALL filter entries by that same effective
attribution. Options SHALL be computed from all entries in scope rather than
the currently loaded card page. Internal option values SHALL be opaque and
SHALL NOT expose peer identifiers, remote paths, capture hashes or content.

#### Scenario: History lists imported applications

- **GIVEN** `Historial` contains imported text or image entries with valid
  source attribution in peer provenance
- **WHEN** the user opens the source-application filter
- **THEN** it offers each effective imported application with its display name
  and locally stored icon or generic fallback
- **AND** selecting an option shows the matching imported entries in Historial

#### Scenario: Peer-bound collection uses its own provenance

- **GIVEN** the active collection is bound to peer A and an entry has
  different source attribution from peers A and B
- **WHEN** the user opens or applies the source-application filter
- **THEN** the option and matching result use only peer A's provenance
- **AND** the option value does not expose peer A's identifier

#### Scenario: User collection includes imported entries

- **GIVEN** a user collection contains imported entries with effective source
  attribution
- **WHEN** the user opens the filter
- **THEN** its options include those entries even when they are outside the
  currently loaded card page
  - **AND** selecting a source filters within that collection and composes with
    the existing tag and text-search filters

#### Scenario: Application options follow the destination after leaving History

- **GIVEN** Historial contains applications that are absent from the selected
  user collection
- **AND** a previous application-options request for Historial is still in
  flight when the user changes collections
- **WHEN** the user switches from Historial to that collection
- **THEN** the filter options are loaded for the destination collection id
- **AND** the in-flight Historial response cannot replace those options
- **AND** only applications represented in the destination collection are
  offered

#### Scenario: Entry has both local and imported attribution

- **GIVEN** a history entry has valid local source metadata and imported
  provenance
- **WHEN** source options and matching cards are resolved
- **THEN** both use the existing effective attribution precedence for that
  active collection
- **AND** selecting the shown application returns that entry

#### Scenario: Missing imported attribution uses the unknown option

- **GIVEN** an imported entry has no valid effective source name
- **WHEN** the active scope contains that entry
- **THEN** it remains represented by the existing unknown-source behavior
- **AND** the source-app filter does not invent an application name or icon

#### Scenario: Imported icon cannot be loaded

- **WHEN** an imported application has a valid display name but its local icon
  is absent or cannot be loaded
- **THEN** the option remains selectable and shows the existing generic icon
- **AND** the filter and cards continue to work
