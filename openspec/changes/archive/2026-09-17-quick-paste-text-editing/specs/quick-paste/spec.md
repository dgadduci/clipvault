## MODIFIED Requirements

### Requirement: Provide per-entry copy actions

Every visible result SHALL expose an accessible `...` menu. The menu SHALL
show only valid copy actions for that entry, `Previsualizar`, and
`Editar captura` when the entry is eligible for plain-text editing. Copy
actions SHALL preserve the existing copy-only semantics. `Previsualizar` SHALL
remain read-only. `Editar captura` SHALL open the existing persistent text
editor and SHALL NOT be shown for image or rich-text entries.

The menu SHALL remain inside the fixed window and SHALL not change row
geometry. Each action that has a keyboard shortcut SHALL show its
platform-aware shortcut text and expose the matching `aria-keyshortcuts`
value.

#### Scenario: Plain text menu

- **WHEN** the user opens the menu for an eligible plain-text entry
- **THEN** it offers `Copiar`, `Editar captura` and `Previsualizar`
- **AND** `Editar captura` visibly shows `⌘E` on macOS or `Ctrl+E` on Linux
- **AND** `Previsualizar` visibly shows `⌘Enter` on macOS or `Ctrl Enter` on
  Linux

#### Scenario: Rich text menu

- **WHEN** the user opens the menu for an entry with rich and plain
  representations
- **THEN** it offers the existing rich/plain copy actions and `Previsualizar`
- **AND** it does not offer `Editar captura`

#### Scenario: Menu opens with visible actions

- **WHEN** the user activates the `...` control of a visible result
- **THEN** an accessible popover opens for that row and its expected actions
  are visible, keyboard reachable and not hidden behind the list or window
  bounds

#### Scenario: Image menu

- **WHEN** the user opens the menu for an image entry
- **THEN** it offers the existing image copy action and `Previsualizar`
- **AND** it does not offer `Editar captura`

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

### Requirement: Edit an eligible text entry from Quick Paste

Quick Paste SHALL allow a user to edit an eligible textual history entry from
the entry menu and SHALL persist the replacement through the existing
`updateTextEntryCommand` and Rust text-history service. The operation SHALL
reuse the same eligibility, validation, hash, type detection, duplicate
detection and transactional semantics as the desktop editor.

#### Scenario: Open the editor from Quick Paste

- **WHEN** the user activates `Editar captura` for an eligible text result
- **THEN** the existing text editor modal opens inside the Quick Paste window
- **AND** it loads the current canonical content into its native textarea
- **AND** the modal heading uses the resolved capture title
- **AND** focus moves to the editor
- **AND** the Quick Paste window remains the same fixed window

#### Scenario: Save preserves identity and metadata

- **WHEN** the user saves valid edited text from Quick Paste
- **THEN** ClipVault updates the same SQLite row and preserves the entry ID
- **AND** it recomputes size, hash and textual content type using the existing
  deterministic rules
- **AND** it preserves title, favorites, tags, collections, source-app
  metadata, assets, `created_at` and `last_seen_at`
- **AND** it does not create a second capture or clipboard entry

#### Scenario: Successful edit refreshes Quick Paste and desktop consumers

- **WHEN** the edit commits successfully
- **THEN** the modal closes through its parent-owned state
- **AND** Quick Paste refreshes its active recent/search source
- **AND** the edited content is available to Quick Paste, desktop history,
  search, preview and copy
- **AND** the existing metadata-only history update signal carries no content,
  hash, snippet, asset reference or path
- **AND** Quick Paste remains visible and preserves the query and selected
  entry ID when that ID still exists

#### Scenario: Failed edit remains atomic

- **WHEN** the backend rejects empty content, a duplicate, a missing entry or
  a non-editable entry
- **THEN** the modal shows the existing safe error state
- **AND** the draft remains available for correction or cancellation
- **AND** the persisted row, memberships, metadata and assets remain unchanged

#### Scenario: Cancel and reopen use the current persisted content

- **WHEN** the user cancels, presses Escape or closes the modal and later opens
  `Editar captura` again for the same result
- **THEN** no draft is saved by the close operation
- **AND** the next modal instance seeds from the current persisted content
- **AND** focus returns to the menu trigger or stable selected-result target
- **AND** the window does not hide or switch to another result

### Requirement: Use the edit shortcut in the active Quick Paste window

When Quick Paste is active, it SHALL support the same platform-aware text edit
shortcut as the desktop: `Meta+E` on macOS and `Control+E` on Linux. The
shortcut SHALL resolve the currently selected result by entry ID and SHALL
delegate to the same editor-opening path as the menu action.

The shortcut SHALL work on the very first activation, before any other
capture has been edited through Quick Paste. It SHALL NEVER fall back to a
previously edited entry, a closed-over `quickPasteEditorEntry` reference, or
any other stale row id: the single source of truth is the live
`selectedEntryId` (preferred when still in scope) or
`resultIds[selectedIndex]` (fallback when the selection id has not yet been
written). When no entry is selected, the shortcut is a no-op.

#### Scenario: Shortcut edits the selected eligible text result

- **WHEN** an eligible text result is selected in Quick Paste and the user
  presses `Cmd+E` on macOS or `Ctrl+E` on Linux
- **THEN** the text editor modal opens for that selected entry
- **AND** the modal never uses the last edited entry or a stale row ID
- **AND** the default browser action is prevented only after a valid target is
  resolved
- **AND** the activation works on the first press, without requiring a
  previous edit cycle

#### Scenario: Shortcut works with the search input focused

- **WHEN** the Quick Paste search input has the focus and the user presses
  `Cmd+E` on macOS or `Ctrl+E` on Linux
- **THEN** the text editor modal opens for the currently selected entry
- **AND** the search input keeps its normal typing behavior — the shortcut
  does not eat printable characters, the caret position is preserved and the
  typed query is not modified

#### Scenario: Shortcut ignores other interactive targets

- **WHEN** focus is inside a `textarea`, a `select`, a `contenteditable`
  element, a `[role="menu"]` popover or a `[role="dialog"]` wrapper, or
  inside any HTMLInputElement that is not the Quick Paste search input
- **THEN** the edit modal does not open
- **AND** the focused surface keeps its normal keyboard behavior
- **AND** the modifier table and the matcher are exactly the ones the
  desktop rail uses (`matchesEditTextShortcut`)

#### Scenario: Shortcut surfaces an informative dialog for non-editable rows

- **WHEN** the selected entry is an image or a rich-text entry and the user
  presses `Cmd+E` on macOS or `Ctrl+E` on Linux
- **THEN** Quick Paste mounts an informative `Captura no editable` dialog
  built on the shared `Modal.svelte` shell
- **AND** the dialog carries the resolved capture title, an accessible body
  message, and a single `Aceptar` close button
- **AND** the dialog never invokes `updateTextEntryCommand`, never modifies
  the entry, never creates a new history row and never mounts
  `EntryTextEditorModal`
- **AND** Escape, backdrop and the close button dismiss only the dialog and
  keep Quick Paste visible
- **AND** the focus returns to the row whose `data-entry-id` the dialog
  was opened against
- **AND** the menu item `Editar captura` remains hidden for image / rich-text
  rows so the dialog is the keyboard-only affordance

#### Scenario: Escape inside the editor does not hide Quick Paste

- **WHEN** the persistent editor is open and the user presses Escape
- **THEN** only the editor closes and Quick Paste stays visible
- **THEN** the global Escape handler does NOT call `hideQuickPasteWindow`
- **AND** the focus returns to the trigger element (`menuAnchorEls[id]`)
  when the editor was opened from the menu, or to the row
  (`rowRefs.get(id)`) when it was opened through the keyboard shortcut
- **AND** the same Escape contract holds after a successful save, after a
  cancellation, after a backdrop dismissal and after a close-button
  dismissal

#### Scenario: Existing preview shortcut remains unchanged

- **WHEN** the user activates `Previsualizar` or presses the existing
  `Cmd/Ctrl+Enter` shortcut
- **THEN** Quick Paste opens the same read-only preview as before
- **AND** no edit modal, copy action, paste action or history mutation runs
- **AND** the menu shows the matching platform-aware preview shortcut text
