## MODIFIED Requirements

### Requirement: Open the quick-paste window with a global hotkey

ClipVault SHALL preserve the existing dedicated transient Quick Paste window,
its fixed `720 × 520` geometry and its current activation behavior. The visible
window shell SHALL use a rounded border consistent with the result cards. The
rounded styling SHALL be applied without changing the native window's fixed
size, centering, always-on-top behavior or platform capability handling.

#### Scenario: Rounded fixed window

- **WHEN** the user opens Quick Paste
- **THEN** the window remains fixed at `720 × 520`, centered according to the
  existing contract, and its visible shell has rounded card-like corners

#### Scenario: Menus and preview preserve window geometry

- **WHEN** the user opens an item menu or the preview overlay
- **THEN** the window does not resize, widen or introduce horizontal overflow

### Requirement: Display compact two-line result metadata with real icons

Each quick-paste result SHALL display a compact title and a preview or image
thumbnail, together with a non-empty content-type icon, a source-application
icon area and elapsed capture time. The content-type and source-application
areas SHALL have stable dimensions through loading, success and failure. A
valid persisted icon reference SHALL resolve to the corresponding icon; when
it is unavailable, an accessible generic fallback SHALL be shown without
displaying a raw application identifier as ordinary content.

#### Scenario: Text result shows both icons

- **WHEN** a text result has a known content type and source-application icon
- **THEN** the first line shows the type icon, title and resolved source-app
  icon without an empty placeholder square

#### Scenario: Image result shows both icons

- **WHEN** an image result has a valid persisted thumbnail and source-app icon
- **THEN** the item shows the image thumbnail in its reserved area and shows
  both the content-type icon and source-app icon in the metadata line

#### Scenario: Quick Paste reuses desktop icon contracts

- **WHEN** the main desktop already resolves a content-type icon or a
  source-application icon for the same entry
- **THEN** Quick Paste reuses the same registry, metadata contract and resolver
  behavior instead of maintaining a second icon mapping or duplicate asset
  loading implementation

#### Scenario: Icon loading or failure is isolated

- **WHEN** one type or source-app icon is loading, missing or rejected
- **THEN** that row shows a stable visual fallback and other rows continue
  resolving their icons independently

#### Scenario: Stale icon response cannot overwrite another row

- **WHEN** the result set or the icon reference changes before an icon request
  settles
- **THEN** the obsolete response is ignored and no icon from another entry is
  rendered

### Requirement: Compact fixed-height quick-paste result list

Quick Paste SHALL preserve its fixed `720 × 520` window, vertical internal
scroll and fixed-height rows. The window shell SHALL use a rounded border that
matches the visual language of the result cards. Item typography SHALL reuse
the family, weights, sizes and color tokens of the main desktop cards while
remaining compact and preventing horizontal overflow.

#### Scenario: Rounded fixed window

- **WHEN** Quick Paste is displayed
- **THEN** its shell has the configured rounded border, remains fixed at
  `720 × 520` and does not grow when content, menus or previews open

#### Scenario: Typography matches the card system

- **WHEN** text and image results are rendered
- **THEN** their type, title, preview and metadata use the existing card
  typography tokens and preserve the fixed two-line geometry

#### Scenario: Long content remains bounded

- **WHEN** a title, preview or application name is long
- **THEN** it truncates or wraps within the item without creating horizontal
  scrolling or widening the window

### Requirement: Focus the search surface on activation

When Quick Paste is active, `Cmd+K` on macOS and `Ctrl+K` on Linux SHALL focus
the existing search input and select its current query. The search surface
SHALL visibly display the platform-appropriate shortcut. The shortcut SHALL
not create a duplicate global listener or alter the active application target.

#### Scenario: macOS focuses search

- **WHEN** the user presses `Cmd+K` while Quick Paste is active
- **THEN** the search input receives focus, its current query is selected and
  the displayed hint identifies `⌘K`

#### Scenario: Linux focuses search

- **WHEN** the user presses `Ctrl+K` while Quick Paste is active
- **THEN** the search input receives focus, its current query is selected and
  the displayed hint identifies `Ctrl K`

### Requirement: Provide per-entry copy actions

Every visible result SHALL expose an accessible `...` menu. The menu SHALL
show only valid copy actions for that entry plus `Previsualizar`. These actions
SHALL write the selected representation to the system clipboard and SHALL NOT
perform a synthetic paste. Plain-text and non-rich entries SHALL offer
`Copiar`; entries with rich and plain representations SHALL offer `Copiar texto
enriquecido` and `Copiar texto plano`; image entries SHALL offer only `Copiar`.
The menu SHALL remain inside the fixed window and SHALL not change row geometry.

#### Scenario: Plain text menu

- **WHEN** the user opens the menu for a plain-text entry
- **THEN** it offers `Copiar` and `Previsualizar`, and no rich-text actions

#### Scenario: Rich text menu

- **WHEN** the user opens the menu for an entry with rich and plain
  representations
- **THEN** it offers `Copiar texto enriquecido`, `Copiar texto plano` and
  `Previsualizar`

#### Scenario: Menu opens with visible actions

- **WHEN** the user activates the `...` control of a visible result
- **THEN** an accessible popover opens for that row and its expected actions are
  visible, keyboard reachable and not hidden behind the list or window bounds

#### Scenario: Image menu

- **WHEN** the user opens the menu for an image entry
- **THEN** it offers only `Copiar` and `Previsualizar`, with no text actions

#### Scenario: Menu action copies without synthetic paste

- **WHEN** the user activates a copy action from the menu
- **THEN** the selected representation is written through the existing
  copy-only command, no synthetic paste is sent, no new history entry is
  created, and Quick Paste remains visible after success

## ADDED Requirements

### Requirement: Preview the selected capture without mutation

Quick Paste SHALL provide `Previsualizar` in the item menu and SHALL open the
same preview with `Cmd+Enter` on macOS or `Ctrl+Enter` on Linux when a valid
entry is selected. The preview SHALL be read-only, remain inside the fixed
window, and SHALL not copy, paste, create history or mutate the selected
entry. Text previews SHALL be safe and bounded; image previews SHALL use the
persisted asset and fit the preview area.

#### Scenario: Preview text

- **WHEN** the user selects a text entry and activates `Previsualizar` or
  presses `Cmd/Ctrl+Enter`
- **THEN** a read-only, scrollable preview of the text opens without changing
  the clipboard, history or active application, and shows the complete
  canonical capture rather than the truncated row preview

#### Scenario: Preview image

- **WHEN** the user selects an image entry and activates `Previsualizar` or
  presses `Cmd/Ctrl+Enter`
- **THEN** the persisted image is shown with contain fitting, or a typed
  unavailable state is shown, without changing `asset_ref` or history

#### Scenario: Escape closes preview first

- **WHEN** the preview overlay is open and the user presses Escape
- **THEN** the overlay closes, Quick Paste remains open and the selected entry
  and list scroll position are preserved

#### Scenario: Preview has no active content

- **WHEN** a preview contains rich or HTML data
- **THEN** only the existing sanitized representation or escaped fallback is
  rendered, with no scripts, event handlers, active navigation or sensitive
  data in logs/events

### Requirement: Click selects and copies without closing Quick Paste

A click on the non-interactive surface of a result row SHALL select that entry
and execute the same type-appropriate copy-only operation as Enter, but SHALL
keep Quick Paste open after a successful copy. The selected entry SHALL remain
visually selected and available for a later `Cmd/Ctrl+V`. Pin and menu controls
remain independent and SHALL NOT trigger this row action. This supersedes the
previous click behavior defined by `quick-paste-actions`, which hid the window
after copying.

#### Scenario: Click copies text and keeps the window open

- **WHEN** the user clicks the non-interactive surface of a text result
- **THEN** the row becomes selected, the same representation as Enter is copied,
  Quick Paste remains visible and the user can later paste with `Cmd/Ctrl+V`

#### Scenario: Click copies an image and keeps the window open

- **WHEN** the user clicks the non-interactive surface of an image result
- **THEN** the image is copied through the copy-only path, Quick Paste remains
  visible and the result remains selected

#### Scenario: Click does not synthesize paste or create history

- **WHEN** a row click copies successfully
- **THEN** no synthetic paste is sent, no new history card is created and the
  source entry's payload, timestamp, tags, collections and assets remain
  unchanged

#### Scenario: Click failure preserves the usable window

- **WHEN** the copy operation initiated by a row click fails or is unavailable
- **THEN** Quick Paste remains open, shows the existing typed error/guidance and
  does not mutate the source entry

### Requirement: Copying an image preserves the complete bitmap

When Quick Paste copies an image entry, the copy-only operation SHALL publish
the complete persisted bitmap represented by `asset_ref`. It SHALL preserve
the canonical width, height and every pixel; it SHALL NOT use the row
thumbnail, preview dimensions, a crop, a downscaled buffer or a truncated
stride. A successful Core test against a fake backend is not sufficient: the
platform adapter boundary and a real macOS manual round-trip SHALL be covered.

#### Scenario: Non-square image round-trip

- **WHEN** the user selects a non-square image and later presses `Cmd/Ctrl+V`
  in another application
- **THEN** the receiving application gets the complete image with the original
  dimensions and full pixel content

#### Scenario: Image copy does not fall back to a partial or textual payload

- **WHEN** the image asset is readable but the platform cannot publish images
- **THEN** Quick Paste reports the existing typed capability/error state and
  never publishes a cropped bitmap or silently substitutes text

### Requirement: Close Quick Paste when its window loses focus

When the Quick Paste window loses OS/window focus to another application, it
SHALL close automatically. The focus-loss handler SHALL be registered once,
cleaned up with the component/window lifecycle and SHALL not create another
global hotkey or clipboard listener.

#### Scenario: Window focus is lost to another application

- **WHEN** Quick Paste is visible and the user activates another application
- **THEN** the Quick Paste window hides/closes and the pending selection,
  preview and menu do not perform additional clipboard or history mutations

#### Scenario: Internal Quick Paste interaction does not close the window

- **WHEN** focus moves between the search input, result rows, menu, preview or
  another control inside Quick Paste
- **THEN** the window remains open and the current interaction completes

#### Scenario: Focus listener is idempotent

- **WHEN** Quick Paste mounts, remounts or is activated repeatedly
- **THEN** exactly one focus-loss listener is active and it is removed during
  cleanup

### Requirement: Use one shared UI typography system

The Quick Paste window and the main desktop SHALL use the same UI typography
system: font family, size scale, weight, line height and semantic colors for
titles, metadata, controls and previews. The source of truth SHALL be shared
code or shared CSS tokens, not duplicated literals in separate webviews. A
semantic monospace style MAY remain only for captured code/content previews
when the main desktop already uses it; it SHALL not be used for Quick Paste UI
chrome or row titles when the desktop uses the primary UI font.

#### Scenario: Quick Paste matches desktop card typography

- **WHEN** the same title, metadata and captured content are rendered in the
  desktop card and Quick Paste
- **THEN** their UI font family, semantic size, weight and line-height match,
  subject only to the compact row geometry

#### Scenario: Typography tokens do not drift between webviews

- **WHEN** the shared visual system changes
- **THEN** both the main desktop and Quick Paste consume the updated token
  values without requiring two unrelated hardcoded scales

## UNCHANGED Requirements

The existing global Quick Paste hotkey, transient-window activation order,
fixed list behavior, copy-only `Enter`/`Shift+Enter` flow, copy-only menu
lifecycle, favorite pinning, title/content search, privacy rules, image asset
lifecycle, focus restoration and idempotent listeners remain in force unless
explicitly modified above.
