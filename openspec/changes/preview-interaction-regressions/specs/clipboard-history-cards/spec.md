## ADDED Requirements

### Requirement: Select a Desktop history card with click

The Desktop history rail SHALL allow one visible card to be selected through a
click on its non-interactive surface. Selection SHALL be local UI state only,
shall be visually and accessibly exposed, and SHALL not mutate the entry or
invoke a backend command. Clicking another card moves the selection; clicking
the selected card again or pressing Escape while the rail is active clears it.
Interactive controls SHALL keep their existing actions and SHALL not trigger
selection accidentally.

#### Scenario: Click selects a card

- **WHEN** the user clicks the non-interactive surface of a visible Desktop
  card
- **THEN** that card becomes the only selected card, receives visible selected
  styling and exposes the selected state accessibly

#### Scenario: Click another card moves selection

- **WHEN** a card is selected and the user clicks the non-interactive surface
  of another card
- **THEN** the first card is deselected and the second card becomes selected

#### Scenario: Click selected card deselects

- **WHEN** the user clicks the non-interactive surface of the already selected
  card
- **THEN** no card remains selected and no backend mutation occurs

#### Scenario: Escape clears selection

- **WHEN** a Desktop card is selected and the rail has focus
- **THEN** pressing Escape clears the selection without opening or closing a
  preview unexpectedly

#### Scenario: Controls remain independent

- **WHEN** the user clicks pin, menu, title editor, tag, collection, paste,
  delete or drag-and-drop controls
- **THEN** the existing control action runs and the click does not accidentally
  select, deselect or start a different card action

#### Scenario: Selection is not persisted

- **WHEN** the selected card is refreshed, removed from the visible scope or
  the application restarts
- **THEN** no selection value is written to SQLite, events or clipboard
  payloads, and a missing card cannot remain selected

### Requirement: Show the preview shortcut for the selected card

Only the selected Desktop card SHALL show the existing platform-aware preview
shortcut: `⌘↵` on macOS or `Ctrl↵` on Linux. The hint SHALL fit within the
existing fixed card geometry, SHALL be accessible, and SHALL disappear as soon
as the card is deselected or leaves the visible scope. The existing
`Cmd/Ctrl+Enter` matcher and shared `ClipboardPreview` implementation SHALL be
reused.

#### Scenario: Selected card shows a macOS hint

- **WHEN** a Desktop card is selected on macOS
- **THEN** that card visibly shows the preview action and `⌘↵`, while other
  cards show no preview shortcut hint

#### Scenario: Selected card shows a Linux hint

- **WHEN** a Desktop card is selected on Linux
- **THEN** that card visibly shows the preview action and `Ctrl↵`, while other
  cards show no preview shortcut hint

#### Scenario: Preview shortcut targets the selected card

- **WHEN** the user presses the platform preview shortcut while a card is
  selected
- **THEN** the shared preview opens for that exact card and no copy, paste,
  title, pin, delete or drag action runs

#### Scenario: Shortcut context is isolated

- **WHEN** the shortcut is pressed while focus is inside an input, title editor,
  menu or another interactive control
- **THEN** the existing control keeps focus and no card preview opens

### Requirement: Keep Desktop card menus fully visible and disclose shortcuts

The existing single-open card menu SHALL render outside the clipping context of
the fixed card and shall position itself within the available viewport. Every
menu action SHALL remain visible and keyboard reachable; if the viewport is
too short, only the menu popover SHALL scroll. An action that has a real
platform shortcut SHALL show its shortcut text and expose the same value via
`aria-keyshortcuts`. Actions without a shortcut SHALL not display an invented
hint.

#### Scenario: Menu opens near the bottom edge

- **WHEN** the user opens a card menu for a card near the bottom edge of the
  Desktop
- **THEN** the menu flips or repositions within the viewport and all actions
  remain visible instead of being clipped by the card

#### Scenario: Menu opens near the top or side edge

- **WHEN** the user opens a card menu near any viewport edge
- **THEN** the menu chooses a safe position and does not widen the Desktop,
  move the rail or clip its actions

#### Scenario: Menu uses internal scrolling only when necessary

- **WHEN** the available viewport height is smaller than the complete menu
- **THEN** the popover gets a bounded internal scroll area while the card and
  rail keep their fixed dimensions

#### Scenario: Preview menu item shows its shortcut

- **WHEN** the card menu exposes `Previsualizar`
- **THEN** the item shows `⌘↵` on macOS or `Ctrl↵` on Linux and exposes the
  same shortcut through its accessible metadata

#### Scenario: Menu controls remain isolated

- **WHEN** the menu opens, closes, repositions or scrolls
- **THEN** only one menu remains active, outside-click and Escape cleanup stay
  idempotent, and drag-and-drop selection/ghost behavior remains unchanged
