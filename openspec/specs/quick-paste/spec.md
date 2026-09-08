## Purpose

Definir la interacción rápida de ClipVault para abrir, buscar, seleccionar y pegar un elemento mediante teclado.

## Requirements

### Requirement: Open the quick-paste window with a global hotkey

ClipVault SHALL expose the existing configurable global hotkey with the MVP
defaults `Cmd + Shift + V` on macOS and `Ctrl + Shift + V` on Linux, and SHALL
open the existing dedicated transient `quick-paste` window when it fires. The
window SHALL start hidden, have a fixed size of `720 × 520` logical pixels,
skip decorations, be non-resizable, stay on top and skip the host taskbar or
dock when the platform permits. On activation it SHALL be centered on the
current monitor when supported, with a safe fallback to the primary monitor.
The visible window shell SHALL use a rounded border consistent with the result
cards. The rounded styling SHALL be applied without changing the native
window's fixed size, centering, always-on-top behavior or platform capability
handling.

#### Scenario: macOS hotkey

- **WHEN** the user presses `Cmd + Shift + V` on macOS while ClipVault is running
- **THEN** ClipVault opens or focuses its dedicated quick-paste window

#### Scenario: Linux hotkey

- **WHEN** the user presses `Ctrl + Shift + V` on Linux while ClipVault is running
- **THEN** ClipVault opens or focuses its dedicated quick-paste window

#### Scenario: Hotkey conflict

- **WHEN** the operating system rejects the configured hotkey because another application owns it
- **THEN** ClipVault reports the conflict and keeps the rest of the application usable

#### Scenario: Fixed compact window

- **WHEN** the user opens Quick Paste
- **THEN** the existing transient window is shown at `720 × 520`, cannot be resized and is centered without opening the main desktop

#### Scenario: Window opens centered on the active display

- **WHEN** the host exposes the current display to the Tauri window API
- **THEN** Quick Paste is centered on that display before becoming visible

#### Scenario: Centering fallback

- **WHEN** the host cannot determine the current display
- **THEN** Quick Paste is centered on the primary display and remains usable

#### Scenario: Rounded fixed window

- **WHEN** the user opens Quick Paste
- **THEN** the window remains fixed at `720 × 520`, centered according to the
  existing contract, and its visible shell has rounded card-like corners

#### Scenario: Menus and preview preserve window geometry

- **WHEN** the user opens an item menu or the preview overlay
- **THEN** the window does not resize, widen or introduce horizontal overflow

### Requirement: Compact fixed-height quick-paste result list

The quick-paste window SHALL render results in one vertical, internally
scrollable list. Each result item SHALL use the same fixed height, fixed
internal columns and two-line layout for every entry type. The list SHALL NOT
introduce horizontal scrolling or grow the window according to content. The
window shell SHALL use a rounded border that matches the visual language of
the result cards. Item typography SHALL reuse the family, weights, sizes and
color tokens of the main desktop cards while remaining compact and preventing
horizontal overflow.

#### Scenario: Homogeneous text items

- **WHEN** the result list contains text entries with previews of different lengths
- **THEN** every item keeps the documented fixed height and long content is truncated with an ellipsis

#### Scenario: Homogeneous image items

- **WHEN** the result list contains image entries with different dimensions
- **THEN** every item keeps the same height and each thumbnail is contained in the reserved fixed image area

#### Scenario: Long list scrolls internally

- **WHEN** more results exist than fit in the `720 × 520` window
- **THEN** only the result list scrolls vertically and the search surface remains visible

#### Scenario: No horizontal overflow

- **WHEN** a title, preview or source-app label is long
- **THEN** the item truncates or clips safely without widening the window or creating horizontal scroll

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

### Requirement: Display compact two-line result metadata with real icons

Each quick-paste result SHALL display a compact title and a preview or image
thumbnail, together with a non-empty content-type icon, a source-application
icon area and elapsed capture time. The content-type and source-application
areas SHALL have stable dimensions through loading, success and failure. A
valid persisted icon reference SHALL resolve to the corresponding icon; when
it is unavailable, an accessible generic fallback SHALL be shown without
displaying a raw application identifier as ordinary content. The large
desktop-style title of the window SHALL NOT consume the result area.

#### Scenario: Text result metadata

- **WHEN** a text result is rendered
- **THEN** its first line contains the type indicator, title and source-app icon, and its second line contains a bounded text preview and elapsed time

#### Scenario: Image result metadata

- **WHEN** an image result is rendered
- **THEN** its first line contains the type indicator, title and source-app icon, and its second line contains a fixed-size thumbnail and elapsed time

#### Scenario: Missing source-app icon

- **WHEN** the source-app icon is unavailable
- **THEN** the reserved icon area remains stable and an accessible fallback is shown without displaying a raw identifier as normal content

#### Scenario: Accessible compact controls

- **WHEN** a user navigates results with the keyboard
- **THEN** the selected item has visible focus/selection styling and compact icon-only elements retain accessible names

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

### Requirement: Focus the search surface on activation

When the existing quick-paste window receives its opened signal, the search
input SHALL receive focus without requiring a mouse click. Focusing the search
input SHALL reset or preserve only the existing query behavior defined by the
current quick-paste activation flow and SHALL NOT change the previously active
application target. When Quick Paste is active, `Cmd+K` on macOS and `Ctrl+K`
on Linux SHALL focus the existing search input and select its current query.
The search surface SHALL visibly display the platform-appropriate shortcut.
The shortcut SHALL not create a duplicate global listener or alter the active
application target.

#### Scenario: Type immediately after opening

- **WHEN** the user presses the global hotkey and types before clicking
- **THEN** the characters enter the Quick Paste search field

#### Scenario: Focus remains inside the fixed window

- **WHEN** the window opens with results available
- **THEN** keyboard focus is in the search field while the result selection remains deterministic

#### Scenario: macOS focuses search

- **WHEN** the user presses `Cmd+K` while Quick Paste is active
- **THEN** the search input receives focus, its current query is selected and
  the displayed hint identifies `⌘K`

#### Scenario: Linux focuses search

- **WHEN** the user presses `Ctrl+K` while Quick Paste is active
- **THEN** the search input receives focus, its current query is selected and
  the displayed hint identifies `Ctrl K`

### Requirement: Keyboard-first quick-paste interaction

The quick-paste window SHALL support query, result navigation, selection and dismissal without requiring a mouse.

#### Scenario: Search and select

- **WHEN** the user opens the window, types a query, navigates results and presses Enter
- **THEN** ClipVault selects the highlighted history entry for pasting

#### Scenario: Dismiss with Escape

- **WHEN** the quick-paste window is open and the user presses Escape
- **THEN** the window closes or hides without changing the clipboard history

#### Scenario: Empty history

- **WHEN** the user opens quick paste before any text has been captured
- **THEN** the window shows a clear empty state and can be dismissed normally

### Requirement: Paste the selected entry into the active application

ClipVault SHALL write the selected text to the operating system clipboard and trigger the platform-appropriate paste action into the previously active application when supported.

#### Scenario: Successful paste

- **WHEN** the user selects a text entry and confirms it
- **THEN** the text is pasted into the previously active application and the entry's usage metadata is updated

#### Scenario: Active application is unavailable

- **WHEN** the target application or paste action cannot be identified
- **THEN** ClipVault reports the failure without deleting or corrupting the selected history entry

### Requirement: Quick-paste window is transient

The quick-paste interaction SHALL remain visually small and SHALL return focus to the active application after a successful paste or dismissal.

#### Scenario: Paste completes

- **WHEN** a selected entry is pasted successfully
- **THEN** the quick-paste window hides and the prior application regains focus when the platform permits it

### Requirement: Quick-paste is driven by a single frontend listener

The shell SHALL emit `clipvault://quick-search` exactly once when the
configured global hotkey fires, and the main frontend window SHALL be
the only subscriber to that event. The subscriber MUST register
through an idempotent registrar so repeated mounts do not multiply
listeners.

#### Scenario: Listener is registered once across mounts

- **WHEN** the main frontend mounts while `clipvault://quick-search`
  is later emitted by the shell
- **THEN** the open-quick-paste flow runs exactly once per emission

#### Scenario: Duplicate registrations are coalesced

- **WHEN** the registrar is invoked twice during the same session
- **THEN** only one active listener subscription remains

### Requirement: Capture the active application before showing quick-paste

The main frontend SHALL invoke the active-application command before
the quick-paste window becomes visible, so the previously focused
application can be inferred and the synthetic paste targets the right
window. The probe failure SHALL NOT block opening quick-paste.

#### Scenario: Active app probe succeeds

- **WHEN** the user triggers the hotkey while a host application is
  focused
- **THEN** the quick-paste window becomes visible and focused after
  the probe resolves

#### Scenario: Active app probe unavailable

- **WHEN** the user triggers the hotkey on a host that does not
  expose the active application
- **THEN** ClipVault still opens the quick-paste window and reports
  the failure through the existing guidance flow when the user
  confirms a paste

### Requirement: Show, focus and signal quick-paste in order

When the hotkey fires, ClipVault SHALL show and focus the
`quick-paste` window in that order, then emit a `quick-paste-opened`
signal to that window. The signal payload SHALL NOT include query
text, snippet text, entry content, the active application identifier
or any other clipboard payload.

#### Scenario: Show then focus then signal

- **WHEN** the user triggers the quick-paste hotkey
- **THEN** ClipVault hides the previous quick-paste visibility state,
  brings the window to the front, focuses the input and finally emits
  the opened signal to the quick-paste window

#### Scenario: Opened signal has no sensitive payload

- **WHEN** the quick-paste window receives the opened signal
- **THEN** the event payload contains no clipboard content, no query
  text, no snippet text and no active-app identifier

### Requirement: Hide quick-paste before invoking the paste command

The quick-paste window SHALL be hidden before `clipvault_paste_entry`
is invoked, so the previously focused application regains focus and
receives the synthetic paste. The hide step SHALL NOT depend on the
outcome of the paste.

#### Scenario: Successful paste

- **WHEN** the user confirms a selection and `clipvault_paste_entry`
  returns `pasted`
- **THEN** the quick-paste window stays hidden and the previously
  focused application receives the pasted text

#### Scenario: Failed or unavailable paste

- **WHEN** the user confirms a selection and the paste command
  returns `failed` or `capability_unavailable`
- **THEN** ClipVault shows the quick-paste window again and opens
  the existing guidance modal with the returned `PlatformGuidance`

#### Scenario: Source history entry stays untouched

- **WHEN** the paste command fails or returns an unavailable outcome
- **THEN** the source history row is not deleted, modified or
  re-touched

### Requirement: Keyboard-first quick-paste selection

The quick-paste window SHALL preserve its keyboard-first navigation, empty
state, selection, Home/End and Escape behavior. When an entry is selected,
Enter SHALL copy the type-appropriate representation to the system clipboard
and hide the window without triggering synthetic paste. Shift+Enter SHALL copy
plain text when a valid plain-text representation exists. Neither key SHALL
invoke any synthetic paste controller in the copy-only flow.

#### Scenario: Empty history

- **WHEN** the user opens quick-paste before any text has been
  captured
- **THEN** the window renders a clear empty state and remains
  dismissable with Escape

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

#### Scenario: Circular ArrowUp and ArrowDown navigation

- **WHEN** the user presses ArrowUp on the first entry or ArrowDown
  on the last entry
- **THEN** the selection wraps around to the opposite end of the list

#### Scenario: Home and End jump to extremes

- **WHEN** the user presses Home or End inside the quick-paste window
- **THEN** the selection jumps to the first or last entry
  respectively

#### Scenario: Escape hides the window without changing history

- **WHEN** the quick-paste window is open and the user presses
  Escape
- **THEN** the window hides and no entry is created, modified or
  removed

### Requirement: Platform-correct global hotkey binding

The shell SHALL map the `cmd_or_ctrl` modifier of the default quick-
paste binding to `SUPER` (Command) on macOS and `CONTROL` (Ctrl) on
every other supported host. The default binding SHALL remain
`Cmd+Shift+V` on macOS and `Ctrl+Shift+V` on Linux.

#### Scenario: macOS default binding

- **WHEN** the shell registers the default hotkey on macOS
- **THEN** the runtime binding uses Command + Shift + V

#### Scenario: Linux default binding

- **WHEN** the shell registers the default hotkey on Linux
- **THEN** the runtime binding uses Control + Shift + V

### Requirement: Robust local search helper

The frontend's search helper SHALL resolve or reject every controller
exactly once, discard stale results, and surface backend errors
instead of leaving promises pending.

#### Scenario: Stale result discarded

- **WHEN** the helper is invoked twice in quick succession
- **THEN** only the most recent invocation's response reaches the
  caller

#### Scenario: Backend error reaches the caller

- **WHEN** the backend invocation rejects
- **THEN** the most recent controller's promise rejects with the
  original error

#### Scenario: Cancelled controller does not surface a value

- **WHEN** the caller cancels a controller
- **THEN** the controller's promise neither resolves with a stale
  response nor rejects with a fabricated error

### Requirement: Select rows with pointer and keep the selection visible

Every result row SHALL be selectable and confirmable by clicking its
non-interactive surface. A normal row click SHALL have the same effect as
pressing Enter for that entry: it SHALL copy the default type-appropriate
representation, keep Quick Paste open and leave the representation available
for a later `Cmd/Ctrl+V`, without synthetic paste. Keyboard navigation SHALL
keep the selected row visible inside the Quick Paste scroll container without
scrolling the desktop, the page or an unrelated container. Interactive
controls inside a row SHALL retain their own action and SHALL NOT accidentally
confirm the row.

#### Scenario: Click confirms a result like Enter

- **WHEN** the user clicks the non-interactive area of a visible result row
- **THEN** Quick Paste performs the same copy-only action as Enter for that
  entry, keeps the window open, does not send synthetic paste and leaves the
  copied representation available for a later `Cmd/Ctrl+V`

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
previous click behavior, which hid the window after copying.

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

### Requirement: Order recent Quick Paste results chronologically

Quick Paste SHALL render its recent-mode result list in deterministic
chronological order. The existing favorites-first behavior SHALL remain: the
favorite group comes first, and both the favorite and non-favorite groups SHALL
be ordered by capture time descending (`created_at DESC`) with `id DESC` as the
tie-breaker. Search-mode hits SHALL retain the ranking returned by
`SearchService` instead of being reordered by date.

#### Scenario: Recent favorites are newest first

- **WHEN** recent Quick Paste results contain multiple favorite entries
- **THEN** the favorite entries appear from the most recently captured to the
  oldest, using the entry id only when capture timestamps tie

#### Scenario: Recent non-favorites are newest first

- **WHEN** recent Quick Paste results contain multiple non-favorite entries
- **THEN** the non-favorite entries appear from the most recently captured to
  the oldest, using the entry id only when capture timestamps tie

#### Scenario: Search ranking is preserved

- **WHEN** the user has an active Quick Paste search query
- **THEN** the list preserves the ranking and tie-breaks returned by
  `SearchService` rather than replacing relevance with chronological order

#### Scenario: Refresh and hydration do not scramble order

- **WHEN** Quick Paste refreshes after `history-updated`, completes metadata
  hydration, toggles a favorite or clears the search query
- **THEN** the visible list remains deterministic and follows the applicable
  recent or search ordering contract

#### Scenario: Copy does not change capture chronology

- **WHEN** the user copies an entry from Quick Paste without creating a new
  capture
- **THEN** the entry remains in its chronological position and its
  `created_at` value is not changed

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
