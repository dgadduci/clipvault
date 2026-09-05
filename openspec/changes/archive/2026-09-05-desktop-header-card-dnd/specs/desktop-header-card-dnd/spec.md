## Purpose

Simplificar el desktop de ClipVault, permitir agregar cards a colecciones por
drag-and-drop, reducir el espacio vacío inicial y usar un pin para favoritos,
sin alterar la persistencia ni las capacidades existentes.

## Requirements

## ADDED Requirements

### Requirement: Compact desktop without redundant application header

ClipVault SHALL remove the desktop line containing the application title,
subtitle and its redundant texts, while preserving useful status messages and
the native window identity.

#### Scenario: Main desktop starts with the workflow surface

- **WHEN** the main desktop is rendered
- **THEN** the toolbar, collection panel and card rail are available without the redundant application-title line occupying vertical space

#### Scenario: Status remains available

- **WHEN** startup, connection, search or organization errors occur
- **THEN** the relevant status remains visible and accessible without restoring the removed title line

### Requirement: Drag cards into user collections

ClipVault SHALL allow a card to be dragged onto a user collection to add that
collection membership using the existing organization command and SHALL
transport only an opaque entry identifier.

The drag source SHALL use the local pointer controller with a mouse fallback
for WebKit/Tauri sessions where HTML5 DataTransfer events are incomplete. It
SHALL activate only after the pointer crosses the configured movement
threshold, prevent accidental text selection while active, and clean up the
ghost, session, pointer capture and visual feedback on every completion or
cancellation path.

#### Scenario: Drag a card to a user collection

- **WHEN** the user drags a valid text or image card onto a user collection
- **THEN** the target collection is added to the entry and the card remains in Historial and all previously assigned collections

#### Scenario: Drag begins on the visible card title

- **WHEN** the user begins the drag on the visible title of a text or image card and moves beyond the activation threshold
- **THEN** the same pointer/mouse drag flow starts and the title is accepted as a valid card surface

#### Scenario: Title editing remains available

- **WHEN** the user clicks, double-clicks, presses Enter or presses F2 on the title without crossing the drag threshold
- **THEN** the existing title focus and editing behavior remains available, while title editor inputs and confirmation/cancellation buttons never start a drag

#### Scenario: WebKit/Tauri mouse fallback completes the drop

- **WHEN** a WebKit/Tauri session emits `mousedown`, `mousemove` and `mouseup` without a complete Pointer Events sequence
- **THEN** ClipVault creates the same metadata-only drag session and applies the drop exactly once

#### Scenario: Active drag prevents text selection

- **WHEN** a card is moved beyond the activation threshold while its preview or neighboring cards contain text
- **THEN** the drag ghost is visible, text selection remains disabled, and the pointer can reach a scrolled collection target

#### Scenario: Drop is idempotent

- **WHEN** the user drops the same card on the same collection more than once
- **THEN** only one membership exists and no duplicate association or duplicate card is created

#### Scenario: Drop feedback is visible

- **WHEN** a valid draggable card is over a valid collection target
- **THEN** the target shows local visual feedback and clears it after drop, cancel or drag leave

#### Scenario: Invalid or unavailable drop is safe

- **WHEN** the payload is invalid, the target is Historial, the collection no longer exists or current associations cannot be hydrated
- **THEN** no destructive or replacement mutation is sent and the UI shows a safe no-op or error state

#### Scenario: Drag payload protects privacy

- **WHEN** a card starts a drag
- **THEN** the DataTransfer payload contains no clipboard content, snippet, hash, asset reference, file path or image bytes

#### Scenario: Keyboard alternative remains available

- **WHEN** a user cannot use drag-and-drop
- **THEN** the existing accessible collection-assignment action from the card remains available and produces the same association semantics

### Requirement: Bounded desktop minimum height

ClipVault SHALL use the minimum height required by the toolbar, collection
panel, card rail, status space and margins, without a large empty lower band or
unbounded growth from collection or card content.

#### Scenario: Initial window has no excessive empty space

- **WHEN** the main window opens with normal content
- **THEN** its initial height is sufficient for the desktop workflow and does not contain a large unused area below it

#### Scenario: Long collections do not enlarge the window

- **WHEN** the collection list contains more rows than fit
- **THEN** the collection list scrolls internally and the window height remains bounded

#### Scenario: Narrow viewport remains usable

- **WHEN** the available viewport is narrow
- **THEN** the desktop reflows without horizontal overflow and preserves access to the toolbar, collections and cards

### Requirement: Center main window at top on startup

ClipVault SHALL position the main window once at startup, centered horizontally
within the primary monitor work area and aligned to the top of that work area.

#### Scenario: Window starts centered and top aligned

- **WHEN** the application starts and the primary monitor work area is available
- **THEN** the main window x coordinate centers its width within that work area and its y coordinate equals the work-area top

#### Scenario: Scale factor is respected

- **WHEN** the monitor uses a non-unit scale factor
- **THEN** size and position calculations remain physically correct and the window is not placed off-screen

#### Scenario: Monitor query fails safely

- **WHEN** the monitor or position query cannot be obtained
- **THEN** the application starts with the configured fallback geometry and does not fail setup

#### Scenario: User movement is preserved

- **WHEN** the user later moves or resizes the main window
- **THEN** the startup positioning logic does not run again or overwrite the user's change

#### Scenario: Quick-paste window is untouched

- **WHEN** the main window is positioned at startup
- **THEN** the transient quick-paste window keeps its own size, position, visibility and always-on-top behavior

### Requirement: Pin icon replaces the favorite star

ClipVault SHALL represent the existing favorite action with a local pin icon
and SHALL preserve the current favorite command and state semantics.

#### Scenario: Unpinned card shows an unfilled pin

- **WHEN** a card has is_pinned false
- **THEN** its favorite control shows an unfilled local pin with an accessible Anclar label

#### Scenario: Pinned card shows a filled pin

- **WHEN** a card has is_pinned true
- **THEN** its favorite control shows a filled or highlighted local pin with an accessible Desanclar label

#### Scenario: Pin action preserves organization and payload

- **WHEN** the user pins or unpins a card
- **THEN** the existing favorite command runs and tags, collections, image metadata and asset references remain unchanged

#### Scenario: Star is absent

- **WHEN** the card renders in any state
- **THEN** it does not use a star glyph, emoji or external icon resource for the favorite control

### Requirement: Preserve image, collection and history invariants

This change SHALL preserve image persistence, organization associations and
history behavior while applying layout, drag-and-drop and icon changes.

#### Scenario: Existing image survives restart

- **WHEN** the application restarts with previously captured images
- **THEN** image rows, asset references and metadata remain available and the cards load valid thumbnails

#### Scenario: Drag and hydration do not drop image fields

- **WHEN** a card is dragged, its organization is hydrated or its collection membership is refreshed
- **THEN** asset_ref, mime_type, dimensions, content_size and thumbnail state remain intact

#### Scenario: Existing workflows remain unchanged

- **WHEN** the user captures, searches, tags, pastes, deletes, applies retention, uses privacy controls or opens quick-paste
- **THEN** those workflows retain their current behavior and no sensitive clipboard data is logged

### Requirement: Thin local implementation

The change SHALL keep organization rules in the existing core service,
Tauri window operations thin and presentation helpers deterministic and local.

#### Scenario: Existing association command is reused

- **WHEN** a drop adds a collection
- **THEN** the frontend delegates through the existing organization bridge and does not implement SQLite writes or a duplicate association policy

#### Scenario: Presentation logic is testable without a desktop host

- **WHEN** drag payloads, pin labels or layout calculations are tested
- **THEN** they can be tested without a real clipboard, monitor, network or GUI session
