## ADDED Requirements

### Requirement: Preserve desktop history and organization list presentation

The rich-text editing flow SHALL preserve the existing desktop presentation:
capture cards stay in one horizontal rail with fixed card and rail dimensions
that scrolls when its content exceeds the available width. The collection list
keeps a bounded viewport whose height does not grow with its item count and
scrolls vertically when needed. Collection and linked-team item labels use the
light foreground visual token.

#### Scenario: Capture rail overflows horizontally

- **WHEN** the history contains more cards than fit in the visible rail
- **THEN** cards remain in one horizontal row and the rail can scroll
  horizontally without changing card dimensions

#### Scenario: Organization labels retain light contrast

- **WHEN** collection or linked-team items appear in the desktop sidebar
- **THEN** their labels use the light foreground color against the dark
  workspace background

#### Scenario: Collection list remains bounded as it grows

- **WHEN** the number of collections exceeds the allocated sidebar viewport
- **THEN** the list scrolls vertically inside its fixed-height region
- **AND** adding collections does not change the region's height or the
  available height for linked teams
