# desktop-shell-layout Specification

## ADDED Requirements

### Requirement: Image import action placement

The Desktop toolbar SHALL provide an accessible photo-icon button to import
local image files. The button SHALL be positioned between the PDF export
button and the delete button.

#### Scenario: Open image import

- **WHEN** the user activates the photo button
- **THEN** ClipVault opens the native multi-file picker for supported local
  image formats
- **AND** the button is available independently of the current capture
  selection

## MODIFIED Requirements

### Requirement: Responsive toolbar controls

The desktop toolbar SHALL keep search, filters, capture creation, the overflow
menu and the clear action in one horizontal row. When the controls exceed the
available toolbar width, the toolbar viewport SHALL scroll horizontally and
keep every control reachable without creating page-level horizontal overflow.

The tag and collection filters SHALL each use a 110 px width and 5.5rem
minimum. The source-application filter SHALL use a 168.3 px width and 8.415rem
minimum, a 10% reduction from its current 187 px and 9.35rem dimensions. The
search field SHALL use a 180 px base width (75% of its current 240 px base) and
a 9rem minimum. It SHALL not grow to absorb free space.

The create-text action SHALL display only its document icon. Its accessible
name, tooltip, keyboard shortcut metadata and activation behavior SHALL remain
available. The search placeholder SHALL be the localized equivalent of
“Search”, while its accessible name remains descriptive.

#### Scenario: Toolbar scrolls when controls exceed available width

- **WHEN** the combined toolbar controls are wider than their viewport
- **THEN** the toolbar remains a single row and scrolls horizontally
- **AND** search, filters, capture creation, menu and clear action remain
  reachable
- **AND** the document does not gain horizontal overflow

#### Scenario: Toolbar remains usable with keyboard and pointer scrolling

- **WHEN** the toolbar content overflows
- **THEN** the toolbar viewport can receive keyboard focus and scroll using
  the browser's horizontal scrolling keys
- **AND** pointer and touch scrolling remain available

#### Scenario: Compact filter widths do not affect the application filter

- **WHEN** the toolbar shows tag, collection and source-application filters
- **THEN** tag and collection controls use 110 px width and 5.5rem minimum
- **AND** the source-application control uses 168.3 px width and 8.415rem
  minimum
- **AND** the tag options popup matches the tag control width

#### Scenario: Search uses the reduced width and concise placeholder

- **WHEN** the toolbar renders in any supported locale
- **THEN** the search field uses a 180 px base width and 9rem minimum
- **AND** its placeholder is a concise localized equivalent of “Search”
- **AND** its accessible name continues to describe the search scope

#### Scenario: Create-text action displays only its icon

- **WHEN** the create-text button is available in the toolbar
- **THEN** its visible content is only the document icon, without a text label
  or shortcut badge
- **AND** its accessible name, tooltip, keyboard shortcut metadata and
  activation behavior remain available

#### Scenario: Menu remains visible from the scrollable toolbar

- **WHEN** the user opens the overflow menu while the toolbar is scrolled
- **THEN** the menu remains anchored to its trigger and is not clipped by the
  horizontal scroll viewport

#### Scenario: Tag filter option reveals its complete name on hover

- **WHEN** the user hovers a tag option whose label is longer than the filter
  width
- **THEN** a tooltip displays the complete text of that tag

#### Scenario: Collection filter option reveals its complete name on hover

- **WHEN** the user hovers a collection option whose label is longer than the
  filter width
- **THEN** a tooltip displays the complete text of that collection

#### Scenario: Cards keep their existing characteristics

- **WHEN** the toolbar is resized or scrolled
- **THEN** history cards retain their existing size, contents, actions,
  horizontal rail and drag-and-drop behavior

### Requirement: Create text captures from the filter toolbar

The desktop SHALL expose the existing manual text-capture flow as a compact
icon-only action in the toolbar. The action SHALL appear after the filter
controls, including the capture-type filter, and before the ellipsis menu,
preserve the existing scope rules, and return focus to its trigger when the
modal closes.

#### Scenario: Text capture action sits after the type filter

- **WHEN** the desktop toolbar is rendered in any local collection scope
- **THEN** the capture-type filter and other filter controls appear before the
  text-capture action
- **AND** the text-capture action appears before the ellipsis menu
- **AND** the action has an accessible name «Nueva captura de texto»
- **AND** its visible content is only the document icon

#### Scenario: Text capture action preserves existing scope rules

- **WHEN** the action is available in the active scope and the user activates
  it
- **THEN** the existing text-capture modal opens for that scope
- **AND** closing the modal returns focus to the action

#### Scenario: Text action is unavailable in unsupported scopes

- **WHEN** manual text creation is disallowed for the active scope
- **THEN** the text-capture action is absent from the toolbar
