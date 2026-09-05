## Purpose

Corregir el drag-and-drop de cards a colecciones y mejorar su feedback visual,
el icono de favorito, el tamaño del icono de aplicación fuente y el espacio
inferior del desktop sin alterar los datos ni las capacidades existentes.

## Requirements

## ADDED Requirements

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

### Requirement: Preserve image and organization invariants

This change SHALL preserve previously stored images, organization associations
and all existing history workflows.

#### Scenario: Image survives restart

- **WHEN** the application restarts with stored image entries
- **THEN** SQLite returns complete image metadata, the asset bridge returns valid bytes and the thumbnail becomes visible

#### Scenario: Organization operations preserve images

- **WHEN** an image is tagged, pinned, unpinned, moved through drag-and-drop or refreshed after a collection change
- **THEN** asset_ref, MIME, dimensions, content_size and thumbnail state remain unchanged

#### Scenario: Existing workflows remain intact

- **WHEN** the user uses capture, search, tags, collections, paste, retention, privacy or quick-paste
- **THEN** current behavior, accessibility and privacy guarantees remain unchanged

### Requirement: Thin local implementation and privacy

The change SHALL reuse the existing organization bridge and local visual
assets, without network calls, telemetry, unnecessary dependencies or
sensitive payloads in logs and events.

#### Scenario: Existing bridge is reused

- **WHEN** a drop is accepted
- **THEN** the operation delegates to the existing collection command and does not write SQLite from Svelte

#### Scenario: No sensitive drag or diagnostic data

- **WHEN** drag, highlight, icon or footer logic runs
- **THEN** no clipboard content, snippets, hashes, absolute paths or image bytes are included in DataTransfer, events, logs or errors
