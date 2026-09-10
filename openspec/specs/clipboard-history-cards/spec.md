## Purpose

Definir la presentación visual de las capturas recientes del historial: la rail horizontal de cards cuadradas, la jerarquía de metadata visible, los iconos locales por tipo de contenido y la metadata de la aplicación fuente asociada a cada captura.

## Requirements

### Requirement: Horizontal recent-history card rail

ClipVault SHALL render the recent text-capture history in the main window as a
horizontal scrollable rail of fixed-size square cards.

#### Scenario: Recent entries use a horizontal rail

- **WHEN** the main window displays one or more recent text captures
- **THEN** the entries appear in a horizontally scrollable rail and each card keeps the same fixed square dimensions without shrinking into a vertical row

#### Scenario: Card layout remains stable for long text

- **WHEN** a capture contains more text than fits inside its card
- **THEN** the card keeps its fixed size, uses smaller preview typography and truncates or clips the preview without changing the persisted content

#### Scenario: Empty history remains usable

- **WHEN** there are no recent captures
- **THEN** the rail area shows an explicit empty state without rendering broken cards or preventing the rest of the main window from being used

### Requirement: Card metadata hierarchy

Each recent-history card SHALL show a compact metadata header with a
representative content-type icon and label, a title centered between metadata
areas, and the source application's icon when available. The source
application's user-visible name, bundle identifier, source identifier and any
other source-application metadata SHALL NOT appear as visible text on the
card and SHALL only be exposed through the `aria-label` and `title`
attributes that screen readers and keyboard navigation consume.

#### Scenario: Text capture shows type metadata

- **WHEN** a card represents a text capture classified as text, JSON, email, HTML or another supported textual type
- **THEN** the card shows the matching local type icon and label in its upper area

#### Scenario: Card shows source application icon only

- **WHEN** the capture has source-application icon metadata and a user-visible display name
- **THEN** the card renders the source icon and exposes the display name through `aria-label` and `title` only — the visual surface never carries the application name, the bundle identifier or the raw source identifier

#### Scenario: Metadata is unavailable

- **WHEN** the type is unknown or the source application metadata is missing or cannot be loaded
- **THEN** the card uses deterministic generic type and application fallbacks (graphical only, no visible text) and remains fully rendered

### Requirement: Generated local content-type icons

ClipVault SHALL provide a local deterministic SVG icon set for every currently
supported textual content type and a safe fallback for unknown values.

#### Scenario: Supported type has an icon

- **WHEN** the frontend renders a card whose content_type is one of the documented textual variants
- **THEN** it renders the corresponding local icon and accessible label without requesting an external resource

#### Scenario: Unknown type falls back safely

- **WHEN** the backend returns an unknown or future content_type
- **THEN** the card renders the generic text icon and label instead of becoming empty or failing

### Requirement: Editable card title

Each card SHALL display a title that defaults to the localized content-type
label and SHALL allow the user to change or restore that title from the card
menu.

#### Scenario: Default title

- **WHEN** an entry has no custom title
- **THEN** the card displays the localized label for its content type as the title

#### Scenario: Edit title from the menu

- **WHEN** the user chooses Editar título, enters a valid title and confirms
- **THEN** ClipVault persists the custom title for that entry and updates the card without changing its clipboard content

#### Scenario: Restore default title

- **WHEN** the user clears a custom title or chooses the restore-default action
- **THEN** ClipVault stores no custom title and the card returns to the content-type label

#### Scenario: Invalid title

- **WHEN** the user submits an empty-invalid or overlong title according to the documented validation rules
- **THEN** ClipVault rejects it with a safe validation message and leaves the previous title unchanged

#### Scenario: Title survives restart

- **WHEN** a custom title was saved and ClipVault is restarted
- **THEN** the card displays the same custom title while the underlying clipboard content and type remain unchanged

### Requirement: Card actions and overflow menu

Each card SHALL expose its actions through an accessible lower action area,
keeping pin/unpin immediately to the left of the ellipsis menu button.

#### Scenario: Open card menu

- **WHEN** the user activates the ellipsis button in the lower-left area of a card
- **THEN** a single accessible menu opens without changing the card content or opening a duplicate menu elsewhere

#### Scenario: Close card menu

- **WHEN** the user presses Escape, clicks outside the menu or moves focus away
- **THEN** the menu closes and the user remains in the history view

#### Scenario: Existing pin behavior is preserved

- **WHEN** the user activates pin or unpin in the lower action area
- **THEN** the existing favorite command and semantics are used, and the card updates its pinned state without changing its title or payload

#### Scenario: Existing delete behavior is preserved

- **WHEN** the user chooses the existing delete action from the card menu
- **THEN** the existing confirmation flow runs before deletion and the card rail refreshes only after the backend mutation succeeds

### Requirement: Source application metadata in cards

Clipboard history entries SHALL preserve the existing stable `source_app`
identifier and SHALL best-effort enrich it with `source_app_name` and
`source_app_icon_ref` on platforms that can resolve local application
metadata. Linux X11 and XWayland SHALL use the same persisted fields and icon
bridge as macOS; unavailable native Wayland metadata SHALL remain an honest
fallback rather than a fabricated value. An entry SHALL be considered fully
enriched only when both the source-application display name and the controlled
icon reference are populated, so a previous icon failure can be retried
without losing the previously stored name.

#### Scenario: Linux X11 card metadata

- **WHEN** a text, rich-text or image capture is obtained from a Linux X11
  application with a matching `.desktop` entry
- **THEN** the card receives the resolved application name and icon when
  available
- **AND** the existing source-application visual treatment is reused

#### Scenario: Linux XWayland card metadata

- **WHEN** a capture is obtained from an X11 application inside a Wayland
  session and XWayland exposes its window
- **THEN** the card uses the same metadata path as Linux X11
- **AND** it does not label the application as native Wayland

#### Scenario: Native Wayland card fallback

- **WHEN** the source application is native Wayland and no supported active-app
  protocol is available
- **THEN** the card renders the existing accessible unknown-source fallback
- **AND** it does not display a misleading name or icon

#### Scenario: Metadata does not alter the capture

- **WHEN** source metadata lookup succeeds, fails or is unavailable
- **THEN** the captured content, content type, timestamps, hashes, image
  dimensions, tags, collections and favorite state remain unchanged

#### Scenario: Metadata survives restart

- **WHEN** a Linux entry with source metadata is loaded after restarting
  ClipVault
- **THEN** its persisted name and icon reference are hydrated through the
  existing recent-entry and icon-asset paths
- **AND** the card does not require a new clipboard capture to render them

#### Scenario: First capture from an application

- **WHEN** a permitted text capture has a stable source application identifier and no stored icon metadata exists for that application
- **THEN** ClipVault obtains the application name and icon on a supported platform, stores a controlled local icon reference when available and renders the metadata in the card

#### Scenario: Repeated capture from the same application

- **WHEN** another permitted capture comes from an application whose icon asset is already stored
- **THEN** ClipVault reuses the existing local asset instead of creating an unbounded duplicate asset

#### Scenario: Icon retry on partial enrichment

- **WHEN** an entry already carries a source-application display name but no controlled icon reference because a previous extraction failed
- **THEN** the capture pipeline retries the icon extraction on the next main-thread pass without overwriting the stored name

#### Scenario: Metadata lookup fails

- **WHEN** application metadata or icon extraction is unavailable
- **THEN** ClipVault still stores the valid text capture and renders a generic application fallback

#### Scenario: Blacklisted capture has no metadata side effect

- **WHEN** the source application is rejected by PrivacyGate
- **THEN** ClipVault does not persist the clipboard payload, does not create a new application icon asset and does not add a card

#### Scenario: Backfill of legacy rows with source_app but no icon

- **WHEN** a row already in the history carries a non-null `source_app` but is missing either the display name or the icon reference
- **THEN** a bounded startup backfill schedules a main-thread re-run of the metadata enrichment for at most the documented batch size; rows whose `source_app` is `NULL` are never candidates

### Requirement: Desktop "Acerca de" surface

The desktop SHALL expose a single-source "Acerca de" entry inside the
global ellipsis menu of the toolbar. The modal SHALL display the
canonical product name (`ClipVault`) and version (`vX.Y.Z`) read from
the existing `clipvault_diagnostics` Tauri command. The version SHALL
NOT be hard-coded in `Svelte`. The entry SHALL NOT be duplicated on
per-card menus or on Quick Paste. Closing the modal SHALL honour the
shared `Modal` shell contract (Escape, backdrop click and the close
button).

#### Scenario: Open the About modal from the global menu

- **WHEN** the user picks the "Acerca de" item inside the toolbar
  ellipsis menu
- **THEN** the menu closes and the modal opens
- **AND** the version label reads `v` + the value of
  `diagnostics.version` from the backend

#### Scenario: Close the About modal with Escape

- **WHEN** the About modal is open and the focus is inside it
- **THEN** pressing Escape closes the modal
- **AND** focus returns to the ellipsis menu trigger that opened it

#### Scenario: Version matches the canonical manifest

- **WHEN** the canonical version in `Cargo.toml` is `0.0.1`
- **THEN** the modal renders `v0.0.1`
- **AND** a future bump that updates `Cargo.toml`,
  `tauri.conf.json` and `package.json` shows the new value without
  any Svelte code change

#### Scenario: About modal is single-sourced

- **WHEN** the desktop renders the HistoryCard rail or Quick Paste
- **THEN** no card-level or window-level menu exposes a parallel
  "Acerca de" entry
- **AND** the global ellipsis menu is the only affordance that opens
  the modal

### Requirement: Safe local icon rendering

Application icons shown in cards SHALL be resolved only through a validated
local representation controlled by ClipVault.

#### Scenario: Valid icon reference

- **WHEN** a card has a valid local application icon reference
- **THEN** the frontend receives renderable local image data through the Tauri bridge and never receives an arbitrary absolute filesystem path

#### Scenario: Invalid or missing icon reference

- **WHEN** an icon reference is invalid, missing, too large, not a PNG or fails to load
- **THEN** the card uses the generic application icon and the history remains usable

#### Scenario: Stale icon refresh does not overwrite a newer card

- **WHEN** an in-flight icon refresh for one entry is pending and the user-visible card switches to a different entry
- **THEN** the late refresh result is discarded so the current card never renders an icon that belongs to a previous capture

### Requirement: Render a bounded card preview using the shared preview projection

Desktop history cards SHALL reuse the existing safe preview projection used by
the preview surface. When a text entry has a detected code language or other
meaningful whitespace, the card MUST preserve supported syntax colors, tabs,
indentation and line breaks within its fixed bounds. The implementation MUST
not duplicate language detection, sanitization or highlighting rules.

#### Scenario: Code card shows the shared highlighted presentation

- **WHEN** a text entry has a supported detected code language
- **THEN** its Desktop card shows the same safe syntax highlighting projection
  used by the preview, bounded to the card geometry

#### Scenario: Whitespace remains visible in a code card

- **WHEN** the captured code contains tabs, indentation or multiple lines
- **THEN** the card preserves their visual separation and line structure rather
  than collapsing the content into a single plain-text paragraph

#### Scenario: Unsafe content remains sanitized

- **WHEN** a card preview contains scripts, event handlers, dangerous URLs or
  unsupported active content
- **THEN** the shared safe fallback/rendering blocks those features while
  keeping the visible text useful

#### Scenario: Non-code and image cards keep their contracts

- **WHEN** a card contains ordinary text, rich text or an image
- **THEN** its existing preview, thumbnail, asset lifecycle and fixed geometry
  remain intact and no code grammar is invented

### Requirement: Explain the capture type through an icon tooltip

The Desktop capture-type icon SHALL expose the canonical human-readable type
through an accessible tooltip or equivalent hover/focus affordance without
changing the card geometry.

#### Scenario: Type icon shows its label on hover

- **WHEN** the user hovers the capture-type icon
- **THEN** a tooltip shows the corresponding type label such as Texto, Imagen,
  HTML, JSON or Código · Python

#### Scenario: Type icon is accessible on focus

- **WHEN** keyboard focus reaches the capture-type icon or its wrapper
- **THEN** an accessible name exposes the same type meaning without relying on
  color alone

#### Scenario: Tooltip does not interfere with card controls

- **WHEN** the tooltip opens or closes
- **THEN** it does not select the card, open the card menu, start drag-and-drop
  or alter the card's fixed dimensions
### Requirement: Card interaction accessibility

The card rail SHALL remain keyboard and assistive-technology usable while
preserving the existing history and search contracts.

#### Scenario: Keyboard reaches card actions

- **WHEN** the user navigates the history area with the keyboard
- **THEN** pin/unpin, ellipsis, title editing and menu actions have visible focus and accessible names

#### Scenario: Other workflows remain unchanged

- **WHEN** the user searches, opens quick-paste, captures text or applies retention
- **THEN** the card presentation does not alter those commands, ranking rules or privacy behavior

### Requirement: Show rich text in the existing history card

The existing square `HistoryCard` and horizontal rail SHALL show a bounded,
sanitized rich preview for rich entries while retaining the current card
dimensions, title, type icon, source-app icon, pin/unpin and overflow layout.

#### Scenario: Rich card uses the existing visual foundation

- **WHEN** a rich entry appears in recent history
- **THEN** it is rendered by the same `HistoryCard` and `HistoryCardRail` as
  text and image entries, with no parallel gallery or variable card size

#### Scenario: Rich card falls back to text

- **WHEN** a rich preview is unavailable or invalid
- **THEN** the card displays the existing compact plain-text preview and
  remains keyboard and screen-reader usable

### Requirement: History card always renders the plain-text preview

The `HistoryCard` SHALL render the canonical plain text from
`entry.content` for every entry — including rich entries — using the
existing bounded `<pre class="preview">` element. The card MUST NOT
create a sandboxed iframe, request a `rich_preview_ref`, mint a
`blob:` URL or load sanitized rich HTML for the preview area. The
rich-text metadata remains available internally for the `Paste de
texto enriquecido` action but is never rendered in the card.

#### Scenario: Rich entry shows the canonical plain text

- **WHEN** a rich entry appears in recent history
- **THEN** the card displays the canonical `entry.content` text with
  the same line-clamp, font and background as a plain-text entry, and
  no iframe, blob URL or `@html` is created

#### Scenario: Plain entry shows the same plain preview

- **WHEN** a plain-text entry appears in recent history
- **THEN** the card displays `entry.content` with the same visual
  treatment as a rich entry; the two shapes are visually
  indistinguishable

#### Scenario: No rich preview iframe is created

- **WHEN** the rail mounts a card for a rich entry
- **THEN** the `HistoryCard` does not request the rich preview
  bridge, does not create a sandboxed iframe and does not load a
  sanitized preview blob; only the canonical plain text is rendered

### Requirement: Offer two paste actions in the card menu

Each textual card's accessible ellipsis menu SHALL expose `Paste de texto
enriquecido` and `Paste de texto plano` using the existing paste command and
focus/close contracts. Image cards SHALL expose a single `Paste` action that
delegates to the bitmap write path; they MUST NOT expose the textual paste
actions because the bitmap has no rich flavours and the plain fallback would
publish the empty content sentinel.

#### Scenario: Rich entry menu

- **WHEN** the user opens the menu of a rich entry
- **THEN** both paste actions are visible, accessible and independently
  selectable

#### Scenario: Plain-only entry menu

- **WHEN** the user opens the menu of an entry without rich metadata
- **THEN** the plain action is enabled and the rich action is visibly and
  accessibly disabled without attempting a paste

#### Scenario: Image card menu exposes only Paste

- **WHEN** the user opens the menu of an image entry
- **THEN** the menu renders exactly one `Paste` action that forwards
  `mode = null` to the paste command so the Rust paste service takes the
  image branch
- **AND THEN** neither `Paste de texto enriquecido` nor `Paste de texto
  plano` is rendered
- **AND THEN** the remaining card actions (editar título, restaurar
  título, pin/unpin, eliminar) keep working as for textual entries

#### Scenario: Paste action closes the menu

- **WHEN** the user activates any paste action
- **THEN** the menu closes once, the `pasteBusy` guard prevents duplicate
  invocation and the existing hide-before-paste/focus restoration flow is
  reused

#### Scenario: Paste error in the card

- **WHEN** the selected mode returns a failure or unavailable capability
- **THEN** the card shows safe actionable feedback, leaves the entry intact
  and does not display rich content in the error

### Requirement: Organize cards with collections and tags

The existing square card rail SHALL expose organization actions without
changing the established card dimensions or source-app/icon/title layout.

#### Scenario: Card organization menu

- **WHEN** the user opens a card's ellipsis menu
- **THEN** it offers `Agregar tag` and `Agregar a colección` through accessible
  multi-select flows and retains all existing actions

#### Scenario: Collection context action

- **WHEN** a card is shown inside a user collection
- **THEN** its menu offers `Quitar de esta colección` without removing it from
  `Historial`

#### Scenario: Historial context action

- **WHEN** a card is shown inside `Historial`
- **THEN** removing it uses the existing confirmed global delete flow rather
  than a collection-only unlink

### Requirement: Preview capture from Desktop cards

Every Desktop history card SHALL expose a read-only `Previsualizar` action in
its existing menu and SHALL support the same platform-aware shortcut used by
Quick Paste: `Cmd+Enter` on macOS and `Ctrl+Enter` on Linux when the card is
focused. Both entry points SHALL open the same shared preview implementation
used by Quick Paste.

#### Scenario: Preview text from the card menu

- **WHEN** the user opens a text card menu and chooses `Previsualizar`
- **THEN** the Desktop opens the shared preview overlay for that exact entry,
  closes the card menu once and leaves the clipboard, history and card
  metadata unchanged

#### Scenario: Preview image from the card menu

- **WHEN** the user opens an image card menu and chooses `Previsualizar`
- **THEN** the Desktop opens the shared image preview and displays the complete
  persisted image inside the bounded preview area without changing the asset,
  its metadata or the clipboard

#### Scenario: Preview with the platform shortcut

- **WHEN** a Desktop card has focus and the user presses `Cmd+Enter` on macOS
  or `Ctrl+Enter` on Linux
- **THEN** the shared preview opens for that card without invoking copy, paste,
  pin, delete, title editing or drag-and-drop

#### Scenario: Invalid shortcut context is ignored

- **WHEN** the shortcut is pressed with no focused card or while focus is in a
  text editor, input, menu, button or other interactive control
- **THEN** no preview opens and the active control keeps its normal behavior

### Requirement: Shared preview behavior and presentation

Desktop and Quick Paste SHALL render text, rich text and image previews through
one shared component/helper path. The shared preview SHALL preserve the
existing Quick Paste behavior, including full safe text, existing rich-preview
policy, validated image assets, loading/error states, bounded scrolling and
consistent type/title/source metadata.

#### Scenario: Full text is not truncated

- **WHEN** a captured text entry is longer than the card or row summary and
  the user opens its preview
- **THEN** the preview shows the complete canonical text with internal scroll
  and never uses the truncated card/row summary as its source

#### Scenario: Text preview is safe

- **WHEN** the captured text contains HTML-active characters or markup-like
  content
- **THEN** the preview displays it as inert text according to the existing
  escaping policy and executes no script, handler or navigation

#### Scenario: Image preview preserves the asset

- **WHEN** the preview loads a persisted image asset
- **THEN** it uses the existing validated asset bridge, shows the full image
  with contain-style layout, preserves intrinsic dimensions and releases its
  Blob URL when the preview closes or changes entry

#### Scenario: Preview asset failure is isolated

- **WHEN** the text or image preview asset is unavailable, invalid or resolves
  after the entry changed
- **THEN** the preview shows the existing safe fallback or loading/error state,
  ignores stale responses and leaves other cards and Quick Paste usable

#### Scenario: Shared implementation is actually reused

- **WHEN** tests inspect or exercise Desktop and Quick Paste preview flows
- **THEN** both surfaces call the same preview component/helpers and there is
  no duplicate renderer, sanitizer, shortcut matcher or asset lifecycle

### Requirement: Accessible preview lifecycle

The Desktop preview SHALL behave as a bounded accessible overlay without
changing the fixed geometry of the application.

#### Scenario: Escape closes the preview first

- **WHEN** the Desktop preview is open and the user presses Escape
- **THEN** the preview closes first, the underlying Desktop remains available,
  and focus returns to the triggering card/control when possible

#### Scenario: Outside click closes the preview

- **WHEN** the user clicks outside the preview surface
- **THEN** the preview closes without mutating the entry; a click inside the
  preview does not close it accidentally

#### Scenario: Repeated open and close is leak-free

- **WHEN** the user opens and closes previews repeatedly or changes the active
  collection while one is open
- **THEN** only one preview/listener set remains active, stale entries are not
  rendered, and all owned Blob URLs/listeners are cleaned up

### Requirement: Desktop card selection

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

### Requirement: Card contracts remain intact

Adding Desktop preview SHALL preserve the existing card, organization,
search, image, paste and drag-and-drop contracts.

#### Scenario: Preview does not mutate the entry

- **WHEN** the user opens, reads and closes a preview
- **THEN** content, title, timestamp, favorite state, tags, collections,
  source-app metadata, asset references and clipboard contents remain unchanged

#### Scenario: Existing card controls remain isolated

- **WHEN** the user activates pin, title editor, paste, delete, tags,
  collections or drag-and-drop controls
- **THEN** those controls perform their existing actions and do not open a
  preview unless the user explicitly chooses `Previsualizar` or the shortcut
  on the card surface

#### Scenario: Search and collection scope remain intact

- **WHEN** a preview is opened from a search result, Historial or a collection
- **THEN** the preview shows that exact visible entry and does not replace,
  broaden or reorder the active card scope
