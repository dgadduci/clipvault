## Purpose

Definir el layout del desktop principal de ClipVault: una barra superior compacta con búsqueda y acciones, un rail de cards como foco visual, y modales dedicados para Development, Privacidad, Retención y Atajo de pegado rápido, preservando los contratos existentes de captura, búsqueda, cards, pegado, blacklist, retención y favoritos.

## Requirements

### Requirement: Compact main desktop

ClipVault SHALL keep the main desktop focused on search and the recent-history
card rail, moving diagnostics and configuration sections into dedicated modal
surfaces without changing the existing card, search or capture contracts.

#### Scenario: Main desktop shows the primary workflow

- **WHEN** the main window is open
- **THEN** it shows the search surface, recent-history rail and compact global controls without displaying the full diagnostic, privacy, retention or quick-paste configuration sections inline

#### Scenario: Existing cards remain the visual focus

- **WHEN** the main window contains text, rich-text or image captures
- **THEN** the existing horizontal rail and square card layout remain available with their current actions and metadata

#### Scenario: Empty history remains usable

- **WHEN** there are no captures
- **THEN** the compact desktop shows the existing empty state and the toolbar controls remain usable

### Requirement: Modal entry points

The main desktop SHALL provide distinct accessible controls for `Development`,
`Privacidad`, `Retención` and `Atajo de pegado rápido`, and SHALL allow at most
one of these modals to be open at a time.

#### Scenario: Open a configuration modal

- **WHEN** the user activates one of the four toolbar controls
- **THEN** the corresponding modal opens with its title, current state and existing controls

#### Scenario: Switching modal entry points

- **WHEN** a modal is open and the user activates another modal control
- **THEN** the current modal closes and only the newly requested modal remains open

#### Scenario: Modal state does not duplicate

- **WHEN** the desktop is remounted, hot-reloaded or a modal is opened repeatedly
- **THEN** only one modal instance and one set of related listeners/handlers remain active

### Requirement: Development diagnostics modal

The `Development` modal SHALL contain the existing Frontend ↔ Tauri ↔ Rust ↔
SQLite diagnostics, Capabilities, Active application information and the
working quick-paste diagnostic controls, while keeping the existing commands
and response behavior.

#### Scenario: Development modal shows diagnostics

- **WHEN** the user opens `Development`
- **THEN** the existing architecture diagnostics, capabilities and active-application information are available inside that modal and are absent from the main desktop body

#### Scenario: Functional quick-paste controls are moved

- **WHEN** the existing `Tick capture` or `Paste latest` control still invokes a working command
- **THEN** it is available inside `Development` with its current result, busy and error behavior

#### Scenario: Dead development control is removed

- **WHEN** an existing diagnostic control is confirmed to be a non-functional stub with no user-facing purpose
- **THEN** it is removed instead of being exposed in the main desktop or modal

#### Scenario: Development errors do not break the shell

- **WHEN** a diagnostic command or quick-paste diagnostic control fails
- **THEN** the modal shows the existing safe error state and the main desktop remains usable

### Requirement: Privacy modal

The `Privacidad` modal SHALL contain the existing privacy settings and
blacklist workflow, including application selection, blacklist listing,
capability/error guidance and current privacy state, without duplicating the
retention controls.

#### Scenario: Open privacy settings

- **WHEN** the user activates `Privacidad`
- **THEN** the existing blacklist and privacy controls appear inside the modal and no second copy remains inline on the desktop

#### Scenario: Privacy workflow is preserved

- **WHEN** the user adds, removes or refreshes a blacklisted application
- **THEN** the existing commands, persistence, icon metadata and guidance behavior are used unchanged

#### Scenario: Permission guidance remains compatible

- **WHEN** a privacy or platform operation returns an unavailable capability or permission guidance
- **THEN** the existing guidance modal/state remains usable and moving the panel does not mark the separate manual verification as complete

### Requirement: Retention modal

The `Retención` modal SHALL contain the existing retention-period selector,
`Preview retention` and `Apply retention now` controls, preserving their
current persistence, preview, apply, loading and error semantics.

#### Scenario: Open retention settings

- **WHEN** the user activates `Retención`
- **THEN** the current retention policy and its preview/apply controls appear in the modal and no `History Management` section remains inline

#### Scenario: Preview does not mutate history

- **WHEN** the user activates `Preview retention`
- **THEN** ClipVault displays the existing preview result without deleting, changing or reordering entries

#### Scenario: Apply retention preserves existing semantics

- **WHEN** the user activates `Apply retention now`
- **THEN** the existing retention command applies the selected policy, preserves favorites and refreshes the rail after success

#### Scenario: Retention error remains contained

- **WHEN** preview or apply fails
- **THEN** the modal shows the existing safe error state without closing the desktop or mutating unrelated settings

### Requirement: Quick-paste shortcut modal

The `Atajo de pegado rápido` modal SHALL show the effective existing quick-paste
shortcut and its listener/capability state as read-only information, without
registering a second hotkey or replacing the transient quick-paste window.

#### Scenario: macOS shortcut display

- **WHEN** the application runs on macOS and the user opens the shortcut modal
- **THEN** the modal displays `Cmd + Shift + V` and the current listener/capability state

#### Scenario: Linux shortcut display

- **WHEN** the application runs on Linux and the user opens the shortcut modal
- **THEN** the modal displays `Ctrl + Shift + V` and the current listener/capability state

#### Scenario: Shortcut modal is informational

- **WHEN** the shortcut modal is opened or closed
- **THEN** no new global listener is registered and no quick-paste invocation is triggered

### Requirement: Global clear-history toolbar action

ClipVault SHALL expose the existing global clear-history action only while the
system `Historial` collection is active. The action SHALL be represented by a
danger-styled trash icon with the accessible name and tooltip
`Eliminar capturas no organizadas`. It SHALL reuse the existing confirmation
and `clearUnorganizedHistoryCommand` flow; the visibility change MUST NOT
change the command's deletion predicate.

#### Scenario: Trash action in Historial

- **WHEN** the active view is `Historial`
- **THEN** the desktop renders exactly one global trash action with
  `data-testid="trash-clear-history"`, `aria-label="Eliminar capturas no organizadas"`
  and the same existing request-clear callback

#### Scenario: Trash action absent in a user collection

- **WHEN** the user activates a user-defined collection
- **THEN** the global trash action is absent from the DOM, keyboard tab order
  and accessibility tree, while the cards' contextual actions remain
  available

#### Scenario: Returning to Historial restores the action

- **WHEN** the user navigates from `Historial` to a user collection and back
- **THEN** the same single trash action becomes visible again without creating
  duplicate handlers or changing the active search/listener state

#### Scenario: Search does not change the action scope

- **WHEN** the user remains in `Historial` and enters a search query
- **THEN** the trash action remains available and its command keeps the
  existing global unorganized-history predicate rather than deleting only
  visible search matches

#### Scenario: Open clear confirmation

- **WHEN** the user activates the trash action in `Historial`
- **THEN** the existing clearable-count request and confirmation dialog open
  before any history mutation occurs, and the wording identifies non-favorite
  entries without user collections

#### Scenario: Cancel clear confirmation

- **WHEN** the user cancels the global clear confirmation
- **THEN** no clear command is invoked and no card, favorite, tag, collection
  membership or image asset is modified

#### Scenario: Confirm unorganized clear

- **WHEN** the user confirms the global clear action in `Historial`
- **THEN** `clearUnorganizedHistoryCommand({ confirm: true })` is invoked once,
  only non-favorite entries without user collections are removed, and the
  cards and clearable count are refreshed after success

#### Scenario: Organized and favorite entries are preserved

- **WHEN** the clear operation encounters a favorite entry or an entry linked
  to one or more user collections
- **THEN** that entry, its organization associations and any referenced image
  or rich assets remain intact

#### Scenario: Individual card actions remain distinct

- **WHEN** the user views a card in `Historial` or in a user collection
- **THEN** the card menu's individual `Delete` action and the contextual
  `Quitar de esta colección` action retain their existing visibility,
  callbacks and semantics, independently of the global trash action

#### Scenario: Clear failure is recoverable

- **WHEN** the count request or clear command fails, or the backend returns
  `confirmation_required`
- **THEN** the existing typed error state is shown without exposing clipboard
  content, and the toolbar remains usable when `Historial` is still active

#### Scenario: Privacy of the affordance

- **WHEN** the toolbar, tooltip or confirmation is rendered
- **THEN** it contains only the action description and metadata count, never
  clipboard content, snippets, hashes, source application identifiers or
  filesystem paths

### Requirement: Consistent local visual system

The reorganized desktop SHALL use a consistent local dark-theme visual system
with compact typography and spacing while preserving the square card rail.

#### Scenario: Typography hierarchy

- **WHEN** the main desktop or one of its modals renders text
- **THEN** body text uses approximately 14–15 px, secondary text 12–13 px, main title 24–28 px, section/modal titles 16–18 px and compact controls 12–14 px

#### Scenario: Responsive toolbar

- **WHEN** the application window becomes narrow
- **THEN** toolbar controls wrap or reflow without clipping the trash action, modal triggers or search field

#### Scenario: Cards keep their dimensions

- **WHEN** the visual styles are applied
- **THEN** the existing horizontal rail and fixed-size square cards do not become a vertical list or change size based on diagnostic/modal content

#### Scenario: No external visual resources

- **WHEN** the desktop renders icons, fonts and controls
- **THEN** it uses local assets/styles and makes no network request for visual resources

### Requirement: Accessible modal lifecycle

Every configuration modal SHALL expose an accessible dialog lifecycle with
focus management and deterministic cleanup.

#### Scenario: Modal receives initial focus

- **WHEN** a modal opens
- **THEN** focus moves to its first useful control or close control and the dialog has an associated accessible title

#### Scenario: Escape closes modal

- **WHEN** the user presses Escape while a modal is open and no blocking operation requires completion
- **THEN** the modal closes and focus returns to the control that opened it

#### Scenario: Outside click closes modal

- **WHEN** the user clicks the modal backdrop and no blocking operation requires completion
- **THEN** the modal closes without mutating state

#### Scenario: Keyboard navigation stays inside modal

- **WHEN** the user navigates with Tab or Shift+Tab
- **THEN** focus remains within the active dialog until it closes

#### Scenario: Cleanup after close

- **WHEN** a modal closes or the main component is destroyed
- **THEN** its document listeners, event handlers and transient resources are removed exactly once

### Requirement: Preserve existing capabilities during layout change

The desktop reorganization SHALL preserve existing behavior for capture,
search, cards, tags, collections, images, rich text, paste, quick-paste,
blacklist, permissions, retention and favorites.

#### Scenario: Card workflows remain available

- **WHEN** the user interacts with a text, rich-text or image card after the
  layout change
- **THEN** title, tag, collection, pin, delete and appropriate paste actions
  retain their existing behavior

#### Scenario: Image card actions remain type-specific

- **WHEN** the user opens the menu of an image card
- **THEN** the card continues to show its image Paste action and does not show
  text plain/rich paste actions

#### Scenario: Existing events remain safe

- **WHEN** the reorganized UI subscribes to existing history, organization or
  quick-paste events
- **THEN** listener registration remains idempotent and event payloads contain
  no clipboard content, snippets, hashes, paths or image bytes

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