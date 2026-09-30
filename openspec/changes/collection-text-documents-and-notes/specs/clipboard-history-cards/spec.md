## ADDED Requirements

### Requirement: Show and open capture notes from history cards

Each history card SHALL show an accessible note icon when its text, rich-text
or image entry has a saved note. Activating the icon SHALL open the shared
multiline note editor for that entry. A card without a note SHALL offer an
Agregar nota action in its existing action menu. The note control SHALL remain
independent from pin, title, collection, delete, paste and drag actions and
SHALL NOT change the card's fixed dimensions, image rendering or drag payload.

#### Scenario: Card with a saved note

- **WHEN** a history card represents an entry with a saved note
- **THEN** the card shows a note icon with an accessible name
- **AND** activating it opens the note editor without starting paste or drag

#### Scenario: Add a note from the card menu

- **GIVEN** a card represents an entry without a note
- **WHEN** the user chooses Agregar nota from the card action menu and saves
  a note
- **THEN** the card displays the note icon after a successful refresh

#### Scenario: Edit or remove a note from the card

- **GIVEN** a card represents an entry with a note
- **WHEN** the user opens its note editor, changes the note or saves it empty
- **THEN** the card indicator reflects the persisted note state
- **AND** its captured content and other card metadata remain unchanged

#### Scenario: Note icon preserves card interactions

- **WHEN** the note icon is activated or focused by keyboard
- **THEN** the note action does not become a drag source and the existing
  pointer/mouse drag controller, pin, title, menu and collection actions
  continue to work
