## ADDED Requirements

### Requirement: Preserve newly created tag selection in the card selector

When a user creates a tag in a card's tag selector, the new tag SHALL remain
selected in that selector until the user saves or cancels, including while
organization snapshots refresh in the background.

#### Scenario: Newly created tag remains selected during organization refresh

- **GIVEN** the tag selector is open for a capture
- **WHEN** the user creates a tag and the organization snapshot refreshes
- **THEN** the new tag appears in the list with its checkbox selected
- **AND** existing selections remain unchanged

#### Scenario: Save newly created tag assignment

- **GIVEN** the user created a tag and it is selected in the list
- **WHEN** the user saves the selector
- **THEN** the tag association is persisted for that capture and the card
  displays the tag after the organization update

#### Scenario: Cancel newly created tag assignment

- **GIVEN** the user created a tag and it is selected in the list
- **WHEN** the user cancels the selector
- **THEN** the capture's tag associations remain unchanged
