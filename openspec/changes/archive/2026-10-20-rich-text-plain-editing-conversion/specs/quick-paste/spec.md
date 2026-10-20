## MODIFIED Requirements

### Requirement: Provide per-entry copy actions

Every visible result SHALL expose an accessible `...` menu. The menu SHALL
show only valid copy actions for that entry, `Previsualizar`, and `Editar
captura` for eligible plain-text and rich-text entries. Plain-text and
non-rich entries SHALL offer `Copiar`; entries with rich and plain
representations SHALL offer `Copiar texto enriquecido` and `Copiar texto
plano`; image entries SHALL offer only `Copiar`. Copy actions SHALL write the
selected representation to the system clipboard and SHALL NOT perform a
synthetic paste. `Editar captura` SHALL use the existing text editor for
plain-text entries and SHALL require the explicit conversion warning for
rich-text entries. The menu SHALL remain inside the fixed window and SHALL not
change row geometry. Each action with a keyboard shortcut SHALL show its
platform-aware shortcut text and expose the matching `aria-keyshortcuts`
value.

#### Scenario: Plain text menu

- **WHEN** the user opens the menu for an eligible plain-text entry
- **THEN** it offers `Copiar`, `Editar captura` and `Previsualizar`, and no
  rich-text actions
- **AND** the edit item shows `⌘E` on macOS or `Ctrl+E` on Linux

#### Scenario: Rich text menu

- **WHEN** the user opens the menu for an entry with rich and plain
  representations
- **THEN** it offers `Copiar texto enriquecido`, `Copiar texto plano`,
  `Editar captura` and `Previsualizar`
- **AND** the edit item shows `⌘E` on macOS or `Ctrl+E` on Linux
- **AND** selecting `Editar captura` opens the explicit conversion warning

#### Scenario: Menu opens with visible actions

- **WHEN** the user activates the `...` control of a visible result
- **THEN** an accessible popover opens for that row and its expected actions
  are visible, keyboard reachable and not hidden behind the list or window
  bounds

#### Scenario: Image menu

- **WHEN** the user opens the menu for an image entry
- **THEN** it offers only `Copiar` and `Previsualizar`, with no text edit or
  rich-text actions

#### Scenario: Menu action copies without synthetic paste

- **WHEN** the user activates a copy action from the menu
- **THEN** the selected representation is written through the existing
  copy-only command, no synthetic paste is sent, no new history entry is
  created, and Quick Paste remains visible after success

#### Scenario: Menu shortcut semantics are accessible

- **WHEN** the menu renders `Editar captura` or `Previsualizar`
- **THEN** the visible hint matches the platform-aware matcher
- **AND** the edit item exposes `aria-keyshortcuts="Meta+E"` on macOS or
  `aria-keyshortcuts="Control+E"` on Linux
- **AND** the preview item exposes `aria-keyshortcuts="Meta+Enter"` on macOS
  or `aria-keyshortcuts="Control+Enter"` on Linux

## ADDED Requirements

### Requirement: Confirm rich-to-plain editing in Quick Paste

Quick Paste SHALL provide the same rich-to-plain conversion flow as the main
history. The warning SHALL appear before the existing text editor, and
confirming SHALL load the entry's canonical plain text and resolved title
without mutating the entry. A conversion SHALL occur only when the user saves
a non-empty draft whose text differs byte-for-byte from the original.

#### Scenario: Confirm warning and open the editor

- **WHEN** the user chooses `Editar captura` for a rich-text result and
  confirms the warning
- **THEN** the existing plain-text editor opens inside the Quick Paste window
- **AND** its draft is seeded from the canonical plain text and its heading
  uses the resolved capture title
- **AND** the warning and confirmation have not changed the persisted entry

#### Scenario: Cancel warning or editor without saving

- **WHEN** the user cancels the warning, or closes the editor using Cancelar,
  Escape, backdrop or its close button
- **THEN** no conversion command commits and the entry retains its rich
  representations
- **AND** Quick Paste remains visible and restores focus to the triggering
  result or its stable row target

#### Scenario: Save changed text and refresh Quick Paste

- **WHEN** the user saves non-empty text that differs from the original
  canonical plain text
- **THEN** the same entry is converted to plain text and the editor closes
- **AND** Quick Paste refreshes its active recent/search source while
  preserving query, selected entry ID and focus when that ID remains visible
- **AND** the existing metadata-only history update signal contains no
  content, hash, snippet, asset reference or path

#### Scenario: Unchanged rich draft cannot be saved

- **WHEN** the rich conversion editor is open and the draft is empty or
  byte-for-byte equal to its original baseline
- **THEN** Guardar is disabled and no backend mutation is requested
- **AND** the entry remains rich and unchanged

#### Scenario: Rich edit shortcut uses the warning flow

- **WHEN** a rich-text result is selected in Quick Paste and the user presses
  `Cmd+E` on macOS or `Ctrl+E` on Linux
- **THEN** the same conversion warning used by `Editar captura` opens
- **AND** confirming opens the existing plain-text editor for the currently
  selected entry
- **AND** the shortcut never uses a stale entry ID or changes history by
  itself

#### Scenario: Image shortcut remains informational

- **WHEN** an image result is selected in Quick Paste and the user presses
  `Cmd+E` on macOS or `Ctrl+E` on Linux
- **THEN** Quick Paste shows its existing non-editable informational dialog
- **AND** it does not open the conversion warning or editor and does not
  mutate the entry
