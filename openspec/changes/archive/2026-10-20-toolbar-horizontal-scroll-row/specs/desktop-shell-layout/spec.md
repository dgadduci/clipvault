## ADDED Requirements

### Requirement: Keep toolbar controls in one horizontally scrollable row

The desktop toolbar SHALL keep its search, filters, text-capture action and
menu actions in one row. The row SHALL NOT wrap controls onto a second line.
When the controls exceed the available width, the row SHALL provide horizontal
scrolling and its height SHALL be bounded by the tallest control in that row.
Open filter and action menus SHALL remain visible and usable outside the
scrolling viewport while staying anchored to their trigger.

#### Scenario: Toolbar controls exceed the available width

- **WHEN** the toolbar controls are wider than the available window space
- **THEN** every control remains on the same row
- **AND** the user can scroll horizontally to reach the controls
- **AND** the row height remains bounded by its tallest control

#### Scenario: A filter or action menu is open while the row scrolls

- **WHEN** the user opens a filter list or the action menu
- **AND** the toolbar row clips overflow horizontally
- **THEN** the open menu remains visible and interactive outside the scroll
  viewport
- **AND** the menu stays positioned relative to its trigger as the viewport or
  row scrolls

#### Scenario: Toolbar row is navigated with a keyboard

- **WHEN** a keyboard user focuses the toolbar scroll row
- **THEN** the row exposes a descriptive accessible name
- **AND** the user can scroll to controls outside the visible width
