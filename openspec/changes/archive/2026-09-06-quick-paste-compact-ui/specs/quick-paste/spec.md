## MODIFIED Requirements

### Requirement: Open the quick-paste window with a global hotkey

ClipVault SHALL expose the existing configurable global hotkey with the MVP
defaults `Cmd + Shift + V` on macOS and `Ctrl + Shift + V` on Linux, and SHALL
open the existing dedicated transient `quick-paste` window when it fires. The
window SHALL start hidden, have a fixed size of `720 × 520` logical pixels,
skip decorations, be non-resizable, stay on top and skip the host taskbar or
dock when the platform permits. On activation it SHALL be centered on the
current monitor when supported, with a safe fallback to the primary monitor.

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

## UNCHANGED Requirements

The existing requirements for local search, keyboard navigation, Escape,
active-application capture, hide-before-paste, privacy and the single
idempotent listener remain in force. This change does not replace their
commands, events or platform adapters.
