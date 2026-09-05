## ADDED Requirements

### Requirement: Filter cards by source application

The desktop SHALL provide an accessible source-application combobox between
the search field and the configuration menu. It SHALL list `Todas` first and
then the distinct applications represented in the active collection scope,
showing a local icon and display name for each option. Selecting an option
SHALL immediately refresh the cards without an Apply button.

#### Scenario: All applications is the first option

- **WHEN** the desktop renders the source-application filter
- **THEN** the first option is `Todas` with a local generic applications icon,
  and selecting it removes only the source-application restriction

#### Scenario: Known applications show metadata

- **WHEN** the active scope contains captures from one or more known
  applications
- **THEN** the combobox contains one option per stable `source_app` identifier,
  each option showing its persisted local icon when available and its display
  name without exposing the raw identifier

#### Scenario: Unknown application fallback

- **WHEN** the active scope contains a capture without a source application
  identifier
- **THEN** the combobox contains one `Aplicación desconocida` option with a
  generic local icon, and selecting it filters only those entries

#### Scenario: Options belong to the active collection

- **WHEN** the active view is `Historial` or a user collection
- **THEN** the options are computed from all entries in that active scope,
  respectively including all history or only entries associated with the user
  collection, rather than only from the currently visible card limit

#### Scenario: Selecting an application filters immediately

- **WHEN** the user selects a known, unknown or `Todas` option
- **THEN** the cards refresh immediately using the selected application filter
  and no Apply button is required

#### Scenario: Source filter combines with text search

- **WHEN** the user has a text query and selects a source application
- **THEN** cards match both the existing deterministic text search and the
  selected source-application filter within the active collection

#### Scenario: Source filter combines with collection navigation

- **WHEN** the user changes from one collection to another
- **THEN** the source-application filter resets to `Todas`, the options reload
  for the new scope, and the cards show the new collection without stale
  results from the previous scope

#### Scenario: Keyboard-complete combobox

- **WHEN** the user opens the combobox with the keyboard
- **THEN** ArrowUp/ArrowDown, Home/End, Enter, Escape and visible focus allow
  opening, navigating, selecting and closing it without a mouse

#### Scenario: Icon failure is non-blocking

- **WHEN** an application icon is missing, invalid or fails to load
- **THEN** the option uses the generic local icon, remains selectable and does
  not prevent the cards from rendering

#### Scenario: Refresh after a new capture

- **WHEN** a new capture is stored while the desktop is open
- **THEN** the source-application options and cards refresh through the existing
  metadata-only history update without registering duplicate listeners

#### Scenario: Existing card and privacy behavior remains intact

- **WHEN** the user applies, removes or changes the source-application filter
- **THEN** card dimensions, image loading, tags, favorites, collection
  memberships, paste actions and drag-and-drop remain unchanged, and no
  clipboard payload or sensitive metadata is logged or displayed
