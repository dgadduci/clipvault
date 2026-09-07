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

ClipVault SHALL persist optional source-application name and icon metadata for
allowed text captures, using a controlled local asset reference and preserving
source_app as the privacy and matching identifier. An entry SHALL be
considered fully enriched only when both the source-application display name
and the controlled icon reference are populated, so a previous icon failure
can be retried without losing the previously stored name.

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
