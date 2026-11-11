## ADDED Requirements

### Requirement: Select multiple desktop captures

The desktop history rail SHALL support an in-memory selection of multiple
visible captures. A plain click SHALL preserve the existing single-selection
behavior; Ctrl/Cmd-click SHALL toggle one capture, and Shift-click or
Shift+Arrow navigation SHALL select the contiguous range from the current
anchor. Selection SHALL be cleared when entries leave the visible scope and
SHALL NOT be persisted.

#### Scenario: Toggle a capture in the selection

- **WHEN** the user Ctrl/Cmd-clicks a visible capture
- **THEN** that capture is added to or removed from the current selection
- **AND** the active card and the remaining selected cards keep their state

#### Scenario: Select a contiguous range

- **WHEN** the user Shift-clicks another visible capture or navigates with
  Shift+Arrow
- **THEN** the captures between the selection anchor and active capture are
  selected

#### Scenario: Clear an out-of-scope selection

- **WHEN** a selected capture is deleted, filtered out or leaves the visible
  collection
- **THEN** its ID is removed from the in-memory selection and cannot be dragged
  as part of a later group

#### Scenario: Existing single-selection shortcuts remain scoped

- **WHEN** the user invokes the existing preview shortcut
- **THEN** it applies only to the active capture and does not open previews for
  every selected capture
