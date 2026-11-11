## MODIFIED Requirements

### Requirement: Card actions and overflow menu

Each card SHALL expose its actions through an accessible lower action area,
keeping pin/unpin immediately to the left of the ellipsis menu button. The
ellipsis trigger and the open menu's normal option labels SHALL use an explicit
light foreground color that remains readable against their dark surfaces.
Existing disabled, shortcut-hint, and destructive-action colors SHALL remain
distinct.

#### Scenario: Card menu has readable light text

- **WHEN** the user views a capture card or opens its ellipsis menu
- **THEN** the ellipsis glyph and enabled menu option labels are light colored
  against their dark backgrounds
- **AND** disabled items, shortcut hints, and the destructive action retain
  their existing visual states

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
