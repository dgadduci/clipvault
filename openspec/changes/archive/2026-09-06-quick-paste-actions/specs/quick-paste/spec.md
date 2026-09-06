## MODIFIED Requirements

### Requirement: Keyboard-first quick-paste selection

The quick-paste window SHALL preserve its keyboard-first navigation, empty
state, selection, Home/End and Escape behavior. When an entry is selected,
Enter SHALL copy the type-appropriate representation to the system clipboard
and hide the window without triggering synthetic paste. Shift+Enter SHALL copy
plain text when a valid plain-text representation exists. Neither key SHALL
invoke any synthetic paste controller in the copy-only flow.

#### Scenario: Enter copies rich text without synthetic paste

- **WHEN** a selected entry has valid rich and plain representations and the user presses Enter
- **THEN** Quick Paste writes the rich representation to the system clipboard, hides and does not send `Cmd/Ctrl+V`

#### Scenario: Shift+Enter copies plain text

- **WHEN** a selected entry has a valid plain-text representation and the user presses Shift+Enter
- **THEN** Quick Paste writes plain text to the system clipboard, hides and does not send synthetic paste

#### Scenario: Enter copies plain text

- **WHEN** a selected entry has only a valid plain-text representation and the user presses Enter
- **THEN** Quick Paste writes plain text to the system clipboard and does not invoke a paste controller

#### Scenario: Enter copies an image

- **WHEN** a selected entry is an image and the user presses Enter
- **THEN** Quick Paste writes the image to the system clipboard, hides and does not send synthetic paste

#### Scenario: Shift+Enter on image-only entry

- **WHEN** a selected entry has no valid textual representation and is an image
- **THEN** Quick Paste performs no copy or paste command and remains usable

#### Scenario: Enter without selection

- **WHEN** the result list is empty or no entry is highlighted
- **THEN** pressing Enter or Shift+Enter invokes neither copy nor paste

### Requirement: Keep the copied representation available

A successful keyboard copy SHALL leave the selected representation in the
system clipboard after Quick Paste hides. ClipVault SHALL NOT restore, clear or
replace it, and the copy SHALL NOT create or update a history entry.

#### Scenario: Later manual paste

- **WHEN** a keyboard copy succeeds and the user focuses a compatible target and presses `Cmd/Ctrl+V`
- **THEN** the selected representation is pasted unless an external action changed the clipboard

#### Scenario: Copy does not create a history card

- **WHEN** the user confirms an entry with Enter or Shift+Enter
- **THEN** no new history card is created as a side effect of writing the clipboard

### Requirement: Provide per-entry direct paste actions

Each visible result SHALL expose an accessible `...` menu with only the
type-appropriate direct paste actions. These explicit menu actions SHALL reuse
the existing direct paste lifecycle and MAY trigger synthetic paste to the
previously active application. Image entries SHALL expose only `Pegar` and
SHALL NOT expose text paste actions.

#### Scenario: Rich text direct menu

- **WHEN** the user opens the menu for an entry with rich and plain representations
- **THEN** the menu offers direct `Pegar texto enriquecido` and `Pegar texto plano` actions

#### Scenario: Image direct menu

- **WHEN** the user opens the menu for an image entry
- **THEN** the menu offers only direct `Pegar`

#### Scenario: Direct menu action uses existing paste flow

- **WHEN** the user activates a valid direct menu action
- **THEN** the existing hide-before-paste, active-target, guidance and successful-close behavior is preserved

### Requirement: Mark and prioritize favorites in quick-paste

Every visible result SHALL display an accessible pin control that reuses the
existing favorite mutation. Favorited entries SHALL appear before
non-favorited entries while preserving the existing order within each group.
Toggling a pin SHALL NOT modify payload, timestamps, tags, collections,
source-app metadata or assets.

#### Scenario: Pin an entry

- **WHEN** the user activates the unpinned control
- **THEN** the entry becomes favorite, moves to the favorite group and focus/selection remain deterministic

#### Scenario: Unpin an entry

- **WHEN** the user activates the pinned control
- **THEN** the entry becomes non-favorite, moves after favorites and the list remains usable

#### Scenario: Favorite preserves organization

- **WHEN** the user pins or unpins a text or image entry
- **THEN** its content, image asset, tags, collections and timestamp remain unchanged

## ADDED Requirements

### Requirement: Select rows with pointer and keep the selection visible

Every result row SHALL be selectable and confirmable by clicking its
non-interactive surface. A normal row click SHALL have the same effect as
pressing Enter for that entry: it SHALL copy the default type-appropriate
representation, hide Quick Paste and leave the representation available for a
later `Cmd/Ctrl+V`, without synthetic paste. Keyboard navigation SHALL keep the
selected row visible inside the Quick Paste scroll container without scrolling
the desktop, the page or an unrelated container. Interactive controls inside a
row SHALL retain their own action and SHALL NOT accidentally confirm the row.

#### Scenario: Click confirms a result like Enter

- **WHEN** the user clicks the non-interactive area of a visible result row
- **THEN** Quick Paste performs the same copy-only action as Enter for that
  entry, hides the window, does not send synthetic paste and leaves the copied
  representation available for a later `Cmd/Ctrl+V`

#### Scenario: Row controls do not trigger row confirmation

- **WHEN** the user clicks the pin control or the overflow menu/control inside a
  result row
- **THEN** the control performs only its own action and does not trigger an
  accidental row confirmation or copy/paste action

#### Scenario: Arrow navigation scrolls the selected row into view

- **WHEN** the user presses ArrowUp, ArrowDown, Home or End and the resulting
  row is outside the visible portion of the list
- **THEN** Quick Paste scrolls only its list container enough to reveal the
  selected row, preserving the fixed window geometry

#### Scenario: Selection remains usable after result changes

- **WHEN** a search, favorite toggle or refresh changes the visible result set
- **THEN** the selected entry is preserved by stable entry id when possible, or
  replaced by a valid visible entry, and the resulting selection is scrolled
  into view without stale-row actions

### Requirement: Search Quick Paste by title and content

Quick Paste local search SHALL match both the user-defined card title and the
canonical searchable content of each entry within the current Quick Paste
scope. It SHALL reuse the existing local SearchService/search command path,
remain deterministic, preserve the existing ranking semantics for content, and
never log or transmit clipboard payloads.

#### Scenario: Title-only query returns its entry

- **WHEN** the query matches a custom card title but does not occur in the
  entry's content
- **THEN** the entry appears in the Quick Paste results

#### Scenario: Content search remains available

- **WHEN** the query matches the canonical content but not the custom title
- **THEN** the entry appears in the Quick Paste results with the existing local
  ranking behavior

#### Scenario: Title and content matching is deterministic

- **WHEN** multiple entries match through different fields
- **THEN** the result order is deterministic and no entry content, title,
  source-app identifier or asset reference is written to logs or event payloads

### Requirement: Render initial image thumbnails deterministically

Every initially visible image entry SHALL independently request and render its
thumbnail, or settle into an explicit error placeholder. One image failure or
slow response SHALL NOT prevent other image rows from rendering. The loading,
loaded and error states SHALL remain protected against stale responses, and
existing persisted asset references SHALL remain unchanged.

#### Scenario: All visible image rows start loading

- **WHEN** Quick Paste opens with multiple image entries visible
- **THEN** each image row enters its loading state and starts its own asset
  resolution without depending on another row's promise

#### Scenario: One image failure does not block other thumbnails

- **WHEN** one image asset is missing, invalid or rejected while other image
  assets are valid
- **THEN** the failed row shows its error placeholder and the valid rows still
  transition to loaded thumbnails

#### Scenario: Stale image responses cannot overwrite a row

- **WHEN** the visible result set changes or an entry/asset reference changes
  before an image request settles
- **THEN** the obsolete response is ignored, no wrong thumbnail is shown, and
  Blob URLs are released according to the existing lifecycle contract

## UNCHANGED Requirements

The existing global hotkey, activation event, idempotent listener, local search,
privacy rules and fixed compact layout remain in force. The copy-only keyboard
flow is an explicit exception to the direct-paste behavior and does not alter
the existing menu action lifecycle.
