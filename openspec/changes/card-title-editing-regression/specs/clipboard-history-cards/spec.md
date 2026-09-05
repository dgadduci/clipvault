## MODIFIED Requirements

### Requirement: Editable card title

Each card SHALL display a title that defaults to the localized content-type
label and SHALL allow the user to edit it from the visible title by double
click or keyboard activation, and from an `Editar título` action in the card
menu. All entry points SHALL open the same inline editor and persist through
the existing title command without changing the clipboard payload.

#### Scenario: Default title

- **WHEN** an entry has no custom title
- **THEN** the card displays the localized label for its content type and the
  title remains editable

#### Scenario: Edit title by double click

- **WHEN** the user double-clicks the visible title without dragging
- **THEN** the inline title editor opens, receives focus and selects the
  current title value

#### Scenario: Edit title from the card menu

- **WHEN** the user opens the card menu and chooses `Editar título`
- **THEN** the same inline editor opens and the menu closes exactly once

#### Scenario: Edit title with keyboard

- **WHEN** the title has focus and the user presses Enter or F2
- **THEN** the same inline editor opens without changing the card payload

#### Scenario: Confirm a valid title

- **WHEN** the user enters a valid title and confirms it
- **THEN** ClipVault invokes the existing title command once, persists the
  title and updates the card after a successful response

#### Scenario: Cancel title editing

- **WHEN** the user presses Escape, activates the cancel icon or leaves the
  editor without confirming
- **THEN** the editor closes and the previous persisted title remains
  unchanged

#### Scenario: Restore default title

- **WHEN** the user clears a custom title or chooses the existing restore-default
  action
- **THEN** ClipVault stores no custom title and the card returns to the
  localized content-type label

#### Scenario: Invalid title

- **WHEN** the user submits an empty-invalid or overlong title according to the
  documented validation rules
- **THEN** the existing validation behavior is used, the previous title is not
  overwritten and a safe error remains actionable

#### Scenario: Title survives restart

- **WHEN** a custom title was saved and ClipVault is restarted
- **THEN** the card displays the same custom title while its content, type,
  image asset metadata, tags, collections and favorite state remain unchanged

#### Scenario: Title editor is available for every card type

- **WHEN** the card contains text, rich text or an image
- **THEN** double click, keyboard activation and `Editar título` expose the
  same title editing behavior
