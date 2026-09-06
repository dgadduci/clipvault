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

### Requirement: Compact fixed-height quick-paste result list

The quick-paste window SHALL render results in one vertical, internally
scrollable list. Each result item SHALL use the same fixed height, fixed
internal columns and two-line layout for every entry type. The list SHALL NOT
introduce horizontal scrolling or grow the window according to content.

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

### Requirement: Display compact two-line result metadata

Each quick-paste result SHALL display a compact title and a preview or image
thumbnail, together with its content-type icon, source-application icon and
elapsed capture time. The content-type icon, source-application icon and
reserved controls SHALL use stable dimensions. The large desktop-style title
of the window SHALL NOT consume the result area.

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

### Requirement: Focus the search surface on activation

When the existing quick-paste window receives its opened signal, the search
input SHALL receive focus without requiring a mouse click. Focusing the search
input SHALL reset or preserve only the existing query behavior defined by the
current quick-paste activation flow and SHALL NOT change the previously active
application target.

#### Scenario: Type immediately after opening

- **WHEN** the user presses the global hotkey and types before clicking
- **THEN** the characters enter the Quick Paste search field

#### Scenario: Focus remains inside the fixed window

- **WHEN** the window opens with results available
- **THEN** keyboard focus is in the search field while the result selection remains deterministic

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
representation, hide Quick Paste and leave the representation available for a
later `Cmd/Ctrl+V`, without synthetic paste. Keyboard navigation SHALL keep
the selected row visible inside the Quick Paste scroll container without
scrolling the desktop, the page or an unrelated container. Interactive controls
inside a row SHALL retain their own action and SHALL NOT accidentally confirm
the row.

#### Scenario: Click confirms a result like Enter

- **WHEN** the user clicks the non-interactive area of a visible result row
- **THEN** Quick Paste performs the same copy-only action as Enter for that
  entry, hides the window, does not send synthetic paste and leaves the copied
  representation available for a later `Cmd/Ctrl+V`

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
