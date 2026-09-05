## Purpose

Mejorar el panel de colecciones, la edición directa de nombres y títulos, el
atajo de búsqueda y la metadata temporal y de tamaño de las cards, conservando
la persistencia y las capacidades existentes de ClipVault.

## Requirements

### Requirement: Collection panel matches the card rail height

ClipVault SHALL render the collection panel with the same stable visual height
as the visible square-card rail and SHALL constrain scrolling to the internal
collection list.

#### Scenario: Many collections stay inside the panel

- **WHEN** the user has more collections than fit in the panel
- **THEN** the panel keeps the rail-aligned height and only the collection list scrolls vertically

#### Scenario: Collections do not expand the desktop

- **WHEN** the collection list is long or a collection name is long
- **THEN** the main desktop does not grow indefinitely in height or width and no horizontal overflow is introduced

#### Scenario: Empty or narrow layout remains usable

- **WHEN** there are no user collections or the viewport is narrow
- **THEN** the panel remains usable, keeps its controls visible and uses a bounded responsive layout

### Requirement: Inline collection rename and delete actions

ClipVault SHALL remove the textual rename button from user collection rows and
SHALL start inline renaming with a double click on the collection name. The
protected Historial collection SHALL not be renamed.

#### Scenario: Rename by double click

- **WHEN** the user double-clicks the name of a user collection
- **THEN** an inline editor replaces the name without opening a separate modal or application

#### Scenario: Keyboard-accessible rename

- **WHEN** the focused user collection name receives the documented keyboard rename command, such as F2
- **THEN** the same inline editor opens without requiring a mouse double click

#### Scenario: Confirm or cancel collection rename

- **WHEN** the inline editor is active and the user confirms with its icon or Enter
- **THEN** the existing rename command persists the validated name and the row returns to normal mode

- **WHEN** the user cancels with its icon or Escape
- **THEN** no rename command is sent and the previous name remains unchanged

#### Scenario: Delete icon remains on the same row

- **WHEN** a user collection is rendered
- **THEN** a red accessible delete icon appears on the same line as its name and the existing confirmation flow runs before deletion

#### Scenario: Historial remains protected

- **WHEN** Historial is rendered or activated
- **THEN** it has no rename or delete action and its protected behavior is unchanged

### Requirement: Compact inline collection creation

ClipVault SHALL replace the textual + Nueva control with a local accessible
icon and SHALL provide a wide inline creation input with icon-based confirm
and cancel controls.

#### Scenario: Open collection creation

- **WHEN** the user activates the new-collection icon
- **THEN** a single inline input receives focus, uses the available row width and does not open another application

#### Scenario: Create with keyboard or icon

- **WHEN** the user enters a valid name and presses Enter or the confirm icon
- **THEN** the existing create command persists the collection and the input closes

#### Scenario: Cancel creation

- **WHEN** the user presses Escape or activates the cancel icon
- **THEN** no collection is created and the input closes

#### Scenario: Invalid creation is safe

- **WHEN** the name is empty, duplicated or invalid according to the existing collection rules
- **THEN** the UI preserves the validation error, does not mutate the collection list and keeps the form usable

### Requirement: Direct card title editing

ClipVault SHALL remove card title editing from the overflow menu and SHALL
provide direct inline title editing from the card title by double click, with a
keyboard-accessible equivalent.

#### Scenario: Edit title by double click

- **WHEN** the user double-clicks a card title
- **THEN** an inline title editor opens in the title area without changing the square card dimensions

#### Scenario: Edit title from keyboard

- **WHEN** the focused title receives the documented keyboard edit command, such as F2 or Enter when the title is focused
- **THEN** the same inline editor opens with the current title selected or available for editing

#### Scenario: Confirm or cancel title edit

- **WHEN** the user confirms a valid title with the icon or Enter
- **THEN** the existing title command persists it and the card updates without changing its payload

- **WHEN** the user cancels with the icon or Escape
- **THEN** the previous title remains unchanged and no title command is sent

#### Scenario: Restore default title

- **WHEN** the user confirms an empty title through the inline editor
- **THEN** the card uses the localized content-type title and preserves content, timestamps, tags, collections and assets

#### Scenario: Menu no longer edits titles

- **WHEN** the user opens a card overflow menu
- **THEN** Editar título is absent and all other type-appropriate actions remain available

### Requirement: Visible platform shortcut for search

ClipVault SHALL focus the existing search input when the user presses Cmd+F on
macOS or Ctrl+F on Linux and SHALL display the effective shortcut in the search
surface.

#### Scenario: macOS search shortcut

- **WHEN** the desktop runs on macOS and the user presses Cmd+F outside a blocking modal operation
- **THEN** the native webview action is prevented and the existing search input receives focus

#### Scenario: Linux search shortcut

- **WHEN** the desktop runs on Linux and the user presses Ctrl+F outside a blocking modal operation
- **THEN** the native webview action is prevented and the existing search input receives focus

#### Scenario: Shortcut is visible and accessible

- **WHEN** the search surface renders
- **THEN** it shows ⌘F on macOS or Ctrl F on Linux in a local visual hint and exposes equivalent accessible text

#### Scenario: No duplicate keyboard listeners

- **WHEN** the desktop is remounted, hot-reloaded or reopened
- **THEN** only one search-shortcut listener is active and it is removed exactly once on teardown

### Requirement: Capture chronology and elapsed time

ClipVault SHALL persist the original capture datetime in created_at, order the
card rail from newest to oldest by that datetime and show elapsed time from
that immutable value.

#### Scenario: Capture datetime is persisted

- **WHEN** an allowed capture is accepted
- **THEN** its original created_at is stored in the same transaction as the entry and survives a restart

#### Scenario: Newest captures appear first

- **WHEN** the active collection or Historial contains entries with different capture datetimes
- **THEN** cards appear in descending created_at order, with descending id as a deterministic tie-breaker

#### Scenario: Pin and metadata do not reorder chronology

- **WHEN** the user pins, unpins, tags, retitles or assigns a collection to an entry
- **THEN** its chronological position is not changed by those metadata mutations

#### Scenario: Elapsed time uses the capture datetime

- **WHEN** a card renders an entry captured minutes, hours, days, months or years ago
- **THEN** it shows the corresponding localized elapsed-time label derived from created_at, not from updated_at or last_seen_at

#### Scenario: Invalid or future datetime is safe

- **WHEN** a legacy or malformed datetime cannot be parsed or is in the future
- **THEN** the card remains usable and displays a deterministic safe fallback without throwing

### Requirement: Card payload-size metadata

Text cards SHALL show the total character count of their canonical content and
image cards SHALL show the persisted payload size in bytes, KB or MB.

#### Scenario: Text card shows complete character count

- **WHEN** a text or rich-text card renders
- **THEN** it shows the count for the complete canonical plain-text content, not the truncated preview

#### Scenario: Unicode count is deterministic

- **WHEN** text contains multibyte or non-ASCII characters
- **THEN** the displayed count follows the documented Unicode counting rule and does not depend on UTF-16 byte length

#### Scenario: Image card shows persisted bytes

- **WHEN** an image card renders with a valid payload
- **THEN** it shows the persisted payload size from content_size, formatted with base-1024 bytes, KB and MB, and does not use thumbnail or Blob URL size

#### Scenario: Metadata does not alter payload rendering

- **WHEN** the character or size metadata is computed
- **THEN** the preview, image thumbnail, paste actions, source-app icon, tags and collections remain unchanged

### Requirement: Red and accessible card deletion control

ClipVault SHALL render the card deletion action with a local red danger icon
while preserving the existing confirmation, error and accessibility behavior.

#### Scenario: Delete icon is visually dangerous

- **WHEN** a user opens a card menu
- **THEN** the delete action is represented by a red icon with an accessible name and visible keyboard focus

#### Scenario: Delete still requires confirmation

- **WHEN** the user activates the red delete icon
- **THEN** the existing confirmation flow runs before any entry or asset is removed

### Requirement: Preserve image persistence and organization state

This change SHALL preserve image rows, local assets and organization state while
updating layout, ordering or metadata.

#### Scenario: Image survives restart and initial hydration

- **WHEN** ClipVault restarts with previously stored image entries
- **THEN** the entries retain their asset reference and metadata, the asset bridge returns valid bytes and the cards load their thumbnails after hydration

#### Scenario: Tags and pin preserve image fields

- **WHEN** the user hydrates tags, assigns a tag, pins or unpins an image card
- **THEN** asset_ref, mime_type, dimensions and payload size remain unchanged and the thumbnail does not regress to a false missing-image state

#### Scenario: Stale UI responses cannot erase an image

- **WHEN** a collection refresh, organization hydration or thumbnail request completes out of order
- **THEN** stale data is discarded without replacing a newer image EntryRecord or revoking its valid Blob URL

#### Scenario: Existing workflows remain available

- **WHEN** the user uses tags, collections, search, retention, privacy, favorites, quick-paste or type-specific paste after this change
- **THEN** those workflows keep their current contracts and no sensitive clipboard data is logged

#### Scenario: Organization operations preserve images

- **WHEN** an image is tagged, pinned, unpinned, moved through drag-and-drop or refreshed after a collection change
- **THEN** asset_ref, MIME, dimensions, content_size and thumbnail state remain unchanged

### Requirement: Bounded local implementation

The change SHALL keep business logic in the core, Tauri commands thin and UI
formatting local, without network calls, telemetry or unnecessary
dependencies.

#### Scenario: Presentation helpers are pure

- **WHEN** elapsed time, counts, byte sizes or shortcut labels are computed
- **THEN** they can be tested without SQLite, Tauri, a display or a real clipboard

#### Scenario: No sensitive metadata leakage

- **WHEN** the new controls, counters or formatters run
- **THEN** logs, events and errors contain no clipboard content, snippets, hashes, absolute paths or image bytes

#### Scenario: Existing bridge is reused

- **WHEN** a drop is accepted
- **THEN** the operation delegates to the existing collection command and does not write SQLite from Svelte

#### Scenario: No sensitive drag or diagnostic data

- **WHEN** drag, highlight, icon or footer logic runs
- **THEN** no clipboard content, snippets, hashes, absolute paths or image bytes are included in DataTransfer, events, logs or errors

### Requirement: Card drop creates a collection association

ClipVault SHALL persistently add a dragged card to a valid user collection
through the existing collection-assignment flow, preserving Historial and all
previous memberships.

#### Scenario: Drop succeeds in a user collection

- **WHEN** the user drags a valid text or image card onto a user collection
- **THEN** the existing collection-assignment command is invoked and the entry is visible in the target collection after refresh

#### Scenario: Drop works with WebKit fallback data

- **WHEN** dragover and drop expose only the ClipVault text/plain fallback instead of the private MIME
- **THEN** the target accepts the operation, parses the entry id and persists the association

#### Scenario: Tauri pointer fallback completes the drop

- **WHEN** the Tauri/WebKit webview exposes the visible pointer gesture but does not deliver a reliable HTML5 `dragstart`, `dragover` or `drop` sequence
- **THEN** ClipVault uses its singleton pointer channel, hit-tests the currently visible target with the pointer coordinates and persists the same additive association

#### Scenario: Drop preserves memberships

- **WHEN** an entry already belongs to Historial and other collections
- **THEN** dropping it into another user collection adds only the new membership and never replaces the existing set

#### Scenario: Repeated drop is idempotent

- **WHEN** the same card is dropped repeatedly on the same collection
- **THEN** only one association exists and duplicate in-flight writes do not occur

#### Scenario: Drop error is non-destructive

- **WHEN** the entry, collection, hydration or persistence command is unavailable
- **THEN** no empty or partial association list is sent, existing memberships remain unchanged and a safe error is surfaced

#### Scenario: Drop payload contains no clipboard data

- **WHEN** a card starts a drag
- **THEN** every DataTransfer representation contains only a strictly encoded entry id and no content, snippet, hash, asset reference, path or image bytes

### Requirement: Smooth collection drop feedback

ClipVault SHALL smoothly change the visual color or emphasis of a valid user
collection while a supported card payload is dragged over it.

#### Scenario: Valid target highlights

- **WHEN** a supported card payload enters and remains over a user collection
- **THEN** that collection receives a visible, contrast-safe highlight with a local CSS transition

#### Scenario: Scrolled target highlights through pointer hit-testing

- **WHEN** the user drags a card with the pointer over a user collection row inside the vertically scrolled list
- **THEN** the visible row is highlighted and remains eligible for drop regardless of its logical position in the list

#### Scenario: Pointer drag provides visible feedback without selecting card text

- **GIVEN** a user begins dragging from the non-interactive body of a history card
- **WHEN** the pointer crosses the drag activation threshold
- **THEN** a generic, non-interactive card preview follows the pointer
- **AND** the browser does not select text from the source card or neighboring cards
- **AND** the preview is removed after drop, cancellation, blur, or controller cleanup
- **AND** the preview does not contain clipboard content, title, source metadata, asset references, or image pixels

#### Scenario: Invalid target does not highlight

- **WHEN** an external, malformed or unsupported drag is over a collection, or the target is Historial
- **THEN** the collection does not present itself as an actionable drop target

#### Scenario: Highlight always clears

- **WHEN** the drag leaves, drops, is cancelled, ends, errors or the sidebar is destroyed
- **THEN** no collection remains highlighted and no document listener remains leaked

### Requirement: Keep drop feedback inside the existing collection surface

ClipVault SHALL keep the main desktop free of a post-drop success banner and
the legacy bottom drop-indicator box; collection-row highlighting remains the
only transient drop feedback visible in the desktop.

#### Scenario: Successful drop does not render a green banner

- **WHEN** a card is successfully added to a user collection
- **THEN** the association is persisted and the card/list refreshes as before
- **AND** no green "Captura agregada a la colección" status is rendered

#### Scenario: Main desktop has no bottom drop box

- **WHEN** the main desktop renders the card rail
- **THEN** the legacy "Suelta aquí una tarjeta capturada para confirmar que el drop funciona" box is absent
- **AND** collection drag-and-drop continues to work through the collection list

### Requirement: Replace favorite star with a pushpin icon

ClipVault SHALL render the existing favorite control with a local minimalist
pushpin/chincheta SVG and SHALL never render a star glyph or star-shaped
fallback.

#### Scenario: Unpinned state

- **WHEN** a card has is_pinned false
- **THEN** the card shows an outline pushpin with the existing accessible Anclar semantics

#### Scenario: Pinned state

- **WHEN** a card has is_pinned true
- **THEN** the card shows a filled or highlighted pushpin with the existing accessible Desanclar semantics

#### Scenario: Favorite behavior is unchanged

- **WHEN** the user activates the pushpin
- **THEN** the existing favorite command and persistence run without changing tags, collections, timestamps or image metadata

### Requirement: Enlarge the source application icon

ClipVault SHALL render the source application icon at 150% of its previous
visual size while preserving aspect ratio, fallback, accessibility and card
dimensions.

#### Scenario: Source icon is more visible

- **WHEN** a card has a valid source application icon
- **THEN** the icon is visibly 50% larger than the prior baseline and remains fully contained in the card header

#### Scenario: Missing source icon remains safe

- **WHEN** the source icon is missing or fails to load
- **THEN** the existing generic fallback renders at the same enlarged footprint without showing an identifier as visible text

### Requirement: Remove the technical footer line from the main desktop

ClipVault SHALL stop rendering the technical listener/capability text line
below the card rail in the main desktop.

#### Scenario: Main desktop has no technical footer line

- **WHEN** the main desktop renders below the cards
- **THEN** the obsolete listener-status line is absent and no empty reserved band remains

#### Scenario: Useful diagnostics remain available

- **WHEN** the removed line contains information needed for diagnostics
- **THEN** that information remains available through the existing Development surface without duplicating the line in the main desktop
