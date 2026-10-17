## ADDED Requirements

### Requirement: Delete tags from the card assignment selector

The tag selector opened from a history card SHALL provide a per-tag delete
action. Before deleting a tag, it SHALL explain that the tag and its
associations will be removed from every capture and require confirmation.
After confirmation, ClipVault SHALL persistently remove the tag definition
and all of its associations without deleting captures. The organization
snapshot and every loaded capture's tag metadata SHALL refresh after success.
On failure, the tag and its associations SHALL remain unchanged and the
selector SHALL remain open with a localized error.

#### Scenario: Cancel tag deletion

- **GIVEN** the user requested deletion of a tag from the card selector
- **WHEN** the user cancels the confirmation
- **THEN** the tag definition, its associations and the selection remain
  unchanged

#### Scenario: Delete a tag used by several captures

- **GIVEN** the tag is associated with the current capture and other captures
- **WHEN** the user confirms deletion from the card selector
- **THEN** the tag definition and all its associations are removed in
  persistent storage
- **AND** no capture is deleted or has its clipboard content changed
- **AND** every loaded card refreshes and no longer displays the tag
- **AND** the tag disappears from the toolbar filter options

#### Scenario: Delete the currently filtered tag

- **GIVEN** the desktop toolbar filters history by a tag
- **WHEN** that tag is deleted from a card selector
- **THEN** the toolbar filter resets to `Todas`
- **AND** the history reloads without applying the deleted tag ID

#### Scenario: Failed tag deletion

- **WHEN** the delete command fails
- **THEN** the modal displays a localized error and stays open
- **AND** the tag definition, associations and current selection remain intact
