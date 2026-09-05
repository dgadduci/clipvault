## Purpose

Definir la interacción rápida de ClipVault para abrir, buscar, seleccionar y pegar un elemento mediante teclado.

## Requirements

### Requirement: Open the quick-paste window with a global hotkey

ClipVault SHALL expose a configurable global hotkey with these MVP defaults: `Cmd + Shift + V` on macOS and `Ctrl + Shift + V` on Linux, and SHALL open a dedicated transient `quick-paste` window (not the main application window) when the hotkey fires. The window SHALL start hidden, be compact (≈ 640×420), skip decorations, be non-resizable, stay on top of other windows and skip the host taskbar/dock when the platform permits.

#### Scenario: macOS hotkey

- **WHEN** the user presses `Cmd + Shift + V` on macOS while ClipVault is running
- **THEN** ClipVault opens or focuses its dedicated quick-paste window

#### Scenario: Linux hotkey

- **WHEN** the user presses `Ctrl + Shift + V` on Linux while ClipVault is running
- **THEN** ClipVault opens or focuses its dedicated quick-paste window

#### Scenario: Hotkey conflict

- **WHEN** the operating system rejects the configured hotkey because another application owns it
- **THEN** ClipVault reports the conflict and keeps the rest of the application usable

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

The quick-paste window SHALL support empty-state rendering, keyboard
navigation and Enter/Escape handling. Enter SHALL only call the paste
command when at least one result is selected.

#### Scenario: Empty history

- **WHEN** the user opens quick-paste before any text has been
  captured
- **THEN** the window renders a clear empty state and remains
  dismissable with Escape

#### Scenario: Enter without selection

- **WHEN** the result list is empty or no entry is highlighted
- **THEN** pressing Enter SHALL NOT invoke `clipvault_paste_entry`

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