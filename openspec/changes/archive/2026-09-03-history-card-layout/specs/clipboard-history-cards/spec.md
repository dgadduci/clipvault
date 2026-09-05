## ADDED Requirements

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
areas, and the source application's icon and name when available.

#### Scenario: Text capture shows type metadata

- **WHEN** a card represents a text capture classified as text, JSON, email, HTML or another supported textual type
- **THEN** the card shows the matching local type icon and label in its upper area

#### Scenario: Card shows source application metadata

- **WHEN** the capture has source-application name and icon metadata
- **THEN** the card shows the source icon and name in the upper area without exposing the source identifier as the primary presentation

#### Scenario: Metadata is unavailable

- **WHEN** the type is unknown or the source application metadata is missing or cannot be loaded
- **THEN** the card uses deterministic generic type and application fallbacks and remains fully rendered

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
source_app as the privacy and matching identifier.

#### Scenario: First capture from an application

- **WHEN** a permitted text capture has a stable source application identifier and no stored icon metadata exists for that application
- **THEN** ClipVault obtains the application name and icon on a supported platform, stores a controlled local icon reference when available and renders the metadata in the card

#### Scenario: Repeated capture from the same application

- **WHEN** another permitted capture comes from an application whose icon asset is already stored
- **THEN** ClipVault reuses the existing local asset instead of creating an unbounded duplicate asset

#### Scenario: Metadata lookup fails

- **WHEN** application metadata or icon extraction is unavailable
- **THEN** ClipVault still stores the valid text capture and renders a generic application fallback

#### Scenario: Blacklisted capture has no metadata side effect

- **WHEN** the source application is rejected by PrivacyGate
- **THEN** ClipVault does not persist the clipboard payload, does not create a new application icon asset and does not add a card

### Requirement: Safe local icon rendering

Application icons shown in cards SHALL be resolved only through a validated
local representation controlled by ClipVault.

#### Scenario: Valid icon reference

- **WHEN** a card has a valid local application icon reference
- **THEN** the frontend receives renderable local image data through the Tauri bridge and never receives an arbitrary absolute filesystem path

#### Scenario: Invalid or missing icon reference

- **WHEN** an icon reference is invalid, missing, too large, not a PNG or fails to load
- **THEN** the card uses the generic application icon and the history remains usable

### Requirement: Card interaction accessibility

The card rail SHALL remain keyboard and assistive-technology usable while
preserving the existing history and search contracts.

#### Scenario: Keyboard reaches card actions

- **WHEN** the user navigates the history area with the keyboard
- **THEN** pin/unpin, ellipsis, title editing and menu actions have visible focus and accessible names

#### Scenario: Other workflows remain unchanged

- **WHEN** the user searches, opens quick-paste, captures text or applies retention
- **THEN** the card presentation does not alter those commands, ranking rules or privacy behavior
