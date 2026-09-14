## MODIFIED Requirements

### Requirement: Compact main desktop

ClipVault SHALL organize the main desktop around one bounded workspace with a
collection panel and a right content column containing the search surface and
the horizontal history-card rail, while preserving the existing commands and
states.

#### Scenario: Main desktop shows the primary workflow

- **WHEN** the main window is open
- **THEN** it shows the search surface, recent-history rail and compact global
  controls without displaying the full diagnostic, privacy, retention or
  quick-paste configuration sections inline

#### Scenario: Existing cards remain the visual focus

- **WHEN** the main window contains text, rich-text or image captures
- **THEN** the existing horizontal rail and square card layout remain
  available with their current actions and metadata

#### Scenario: Empty history remains usable

- **WHEN** there are no captures
- **THEN** the compact desktop shows the existing empty state and the toolbar
  controls remain usable

#### Scenario: Search is above the cards

- **WHEN** the main desktop renders the active collection and its entries
- **THEN** the search bar appears in the right content column immediately
  above the card rail
- **AND** the search continues to filter the active collection rather than
  creating a separate history result list

#### Scenario: Collection panel and content column share height

- **WHEN** the main desktop renders the collection panel and the search/card
  column
- **THEN** both columns occupy the same visible workspace height
- **AND** the collection panel does not use an unrelated fixed height that
  diverges from the search/card column

#### Scenario: Collection list remains bounded

- **WHEN** the number of user collections exceeds the available workspace
  height
- **THEN** only the collection list scrolls vertically
- **AND** the desktop body does not grow to display every collection

#### Scenario: Existing card rail remains bounded

- **WHEN** the active collection contains more cards than fit horizontally
- **THEN** the rail keeps its fixed square cards and scrolls horizontally
- **AND** the page does not gain horizontal overflow

#### Scenario: Empty state remains usable

- **WHEN** the active collection has no entries
- **THEN** the search surface, global actions and collection panel remain
  usable while the existing empty state is shown

### Requirement: Consistent local visual system

ClipVault SHALL apply the existing local visual tokens to the reorganized
toolbar, overflow menu, collection panel and card rail without changing card
dimensions or requesting external resources.

#### Scenario: Typography hierarchy

- **WHEN** the main desktop or one of its modals renders text
- **THEN** body text uses approximately 14–15 px, secondary text 12–13 px,
  main title 24–28 px, section/modal titles 16–18 px and compact controls
  12–14 px

#### Scenario: Responsive toolbar

- **WHEN** the application window becomes narrow
- **THEN** toolbar controls wrap or reflow without clipping the trash action,
  modal triggers or search field

#### Scenario: Cards keep their dimensions

- **WHEN** the visual styles are applied
- **THEN** the existing horizontal rail and fixed-size square cards do not
  become a vertical list or change size based on diagnostic/modal content

#### Scenario: No external visual resources

- **WHEN** the desktop renders icons, fonts and controls
- **THEN** it uses local assets/styles and makes no network request for visual
  resources

#### Scenario: Responsive workspace

- **WHEN** the main window becomes narrow
- **THEN** the workspace reflows using the existing responsive strategy
- **AND** search, menu, trash and the card rail remain accessible without
  clipping or page-level horizontal overflow

#### Scenario: Image visual contract survives layout

- **WHEN** the workspace is loaded with previously stored image entries,
  including after a restart
- **THEN** each image card keeps its existing asset metadata and thumbnail
  loading lifecycle and remains visible

## ADDED Requirements

### Requirement: Overflow menu for secondary desktop actions

ClipVault SHALL expose Development, Privacidad, Retención and Atajo de pegado
rápido through one accessible ellipsis menu located to the right of the
search bar.

#### Scenario: Open global actions menu

- **WHEN** the user activates the ellipsis button
- **THEN** one menu opens with exactly the four existing secondary actions
- **AND** no standalone Development, Privacidad, Retención or Atajo button is
  rendered beside the search input

#### Scenario: Select a global action

- **WHEN** the user selects one of the four menu items
- **THEN** the menu closes and the corresponding existing modal opens
- **AND** the existing callback, modal state and command behavior are reused

#### Scenario: Close menu safely

- **WHEN** the user presses Escape, clicks outside, selects an item or the
  component is destroyed
- **THEN** the menu closes and its listeners are removed exactly once
- **AND** focus returns to the ellipsis button when appropriate

#### Scenario: Menu lifecycle is idempotent

- **WHEN** the toolbar is remounted or hot-reloaded
- **THEN** only one menu instance and one set of outside/keyboard handlers
  remain active

### Requirement: Independent clear-history action

ClipVault SHALL render the existing destructive trash action immediately to
the right of the ellipsis menu and outside that menu.

#### Scenario: Trash remains visible

- **WHEN** the toolbar is rendered
- **THEN** the trash button is a sibling of the ellipsis button, visible
  without opening the menu and styled with the existing danger treatment

#### Scenario: Existing clear flow is preserved

- **WHEN** the user activates the trash button
- **THEN** the existing confirmation flow opens before any mutation
- **AND** cancel, confirmation, error and refresh behavior remain unchanged

### Requirement: Preserve existing toolbar and desktop contracts

The toolbar reorganization SHALL preserve search shortcut behavior, active
collection filtering, modal focus management, image rendering, favorites,
organization operations and drag-and-drop.

#### Scenario: Search shortcut remains visible and functional

- **WHEN** the application runs on macOS or Linux
- **THEN** the search field displays the existing platform-specific Cmd-F or
  Ctrl-F hint
- **AND** the existing global shortcut focuses the relocated search input

#### Scenario: Organization and drag operations remain intact

- **WHEN** the user switches collection, tags a card, pins a card or drops a
  card onto a collection
- **THEN** the existing persistence and refresh flows run without losing
  image metadata, tags, memberships or favorite state

#### Scenario: No sensitive data is introduced

- **WHEN** the toolbar or its menu opens, closes or triggers an existing
  callback
- **THEN** no clipboard content, snippets, hashes, asset bytes or absolute
  paths are added to logs, events or DOM attributes

### Requirement: One bounded panel contains the desktop workspace zones

The main desktop SHALL render the collection list, the search/actions row and
the history-card row inside one bounded workspace div/panel. The collection
zone SHALL NOT remain a separate outer panel from the content zones, and the
workspace SHALL own the shared visible height and outer visual surface.

#### Scenario: Collection list and content rows share one panel

- **WHEN** the main desktop renders the three zones represented in the
  reference layout
- **THEN** the collection list, the search/actions row and the history-card
  row are descendants of the same bounded workspace container
- **AND** no zone is rendered as a sibling outside that container
- **AND** the outer background, border, radius and padding do not split zone 1
  away from zones 2 and 3

#### Scenario: Collection viewport stays fixed while rows grow

- **WHEN** the number of user collections exceeds the available workspace
  height
- **THEN** the workspace keeps its visible height
- **AND** only the collection-list viewport scrolls vertically
- **AND** the collection header and new-collection control remain usable
- **AND** the desktop body does not gain a second vertical scrollbar

#### Scenario: Existing organization and card interactions survive

- **WHEN** the user selects, creates, renames or deletes a collection, or
  drags a card over a scrolled collection row
- **THEN** the existing organization and drop-zone behavior remains intact
- **AND** the drag payload continues to contain only the opaque entry id
