## ADDED Requirements

### Requirement: Create text captures from the filter toolbar

The desktop SHALL expose the existing manual text-capture flow as a compact
icon-and-label action in the toolbar. The action SHALL appear after the
existing filter controls and before the ellipsis menu, preserve the existing
scope rules, and return focus to its trigger when the modal closes.

#### Scenario: Text capture action sits after the type filter

- **WHEN** the desktop toolbar is rendered in any local collection scope
- **THEN** the existing filter controls appear before the «Texto» action
- **AND** the «Texto» action appears before the ellipsis menu
- **AND** the action has an accessible name «Nueva captura de texto»

#### Scenario: Text capture action preserves existing scope rules

- **WHEN** the action is available in the active scope and the user activates
  it
- **THEN** the existing text-capture modal opens for that scope
- **AND** closing the modal returns focus to the action

#### Scenario: Text action is unavailable in unsupported scopes

- **WHEN** manual text creation is disallowed for the active scope
- **THEN** the «Texto» action is absent from the toolbar
