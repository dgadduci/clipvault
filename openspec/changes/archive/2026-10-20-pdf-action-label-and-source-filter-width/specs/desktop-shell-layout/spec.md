# desktop-shell-layout Specification Delta

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

The create-text shortcut SHALL appear as a compact badge inside its button.
The search placeholder SHALL be the localized equivalent of “Search”, while
its accessible name remains descriptive.

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

#### Scenario: Create-text shortcut appears as a badge

- **WHEN** the create-text button has a configured shortcut
- **THEN** the shortcut appears as a compact badge within the button
- **AND** the button retains its existing accessible name, tooltip, keyboard
  shortcut metadata and activation behavior

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
