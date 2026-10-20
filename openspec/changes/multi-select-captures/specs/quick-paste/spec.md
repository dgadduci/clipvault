## ADDED Requirements

### Requirement: Select multiple Quick Paste captures

Quick Paste SHALL expose a per-result selection control for an in-memory
selection of multiple visible results. Activating the control or
Ctrl/Cmd-click SHALL toggle one result; Shift+Arrow SHALL select the contiguous
range from the selection anchor. Selection controls and selection modifiers
SHALL only change selection and SHALL NOT copy, paste, hide the window or
mutate history. A plain click and Enter SHALL preserve the existing
single-result copy-only behavior when zero or one result is selected. Enter
with multiple selected results SHALL paste each result sequentially through
the existing paste command in visible list order. Quick Paste SHALL hide
before the first paste and stop at the first failure; if any prior result
succeeded, it SHALL report the partial count and show the window again.

#### Scenario: Toggle a result with its selection control

- **WHEN** the user activates a result's selection control
- **THEN** the result is added to or removed from the selection
- **AND** Quick Paste does not copy, paste or hide

#### Scenario: Toggle a result without copying it

- **WHEN** the user Ctrl/Cmd-clicks a visible result
- **THEN** the result is added to or removed from the selection
- **AND** Quick Paste does not write to the clipboard or hide

#### Scenario: Select a result range

- **WHEN** the user navigates with Shift+Arrow
- **THEN** the results between the anchor and active result are selected

#### Scenario: Shift-click keeps its plain-text copy action

- **WHEN** the user Shift-clicks a result
- **THEN** only that result is selected and the existing single-result
  plain-text copy action runs

#### Scenario: Confirm a result while several are selected

- **WHEN** the user presses Enter or plain-clicks a result while multiple
  results are selected
- **THEN** Enter pastes the selected results in visible list order, while the
  plain click keeps the existing copy-only action for the clicked result
- **AND** no selected result is mutated in history

#### Scenario: Paste stack is sequential

- **WHEN** a selected result finishes pasting successfully
- **THEN** Quick Paste waits for that response before starting the next
  selected result and remains hidden until the stack finishes

#### Scenario: Paste stack stops after failure

- **WHEN** a result in a paste stack fails or its paste capability is
  unavailable
- **THEN** later results are not pasted, Quick Paste is shown again, and the
  user receives a localized message with the count already pasted

#### Scenario: Search removes stale selected results

- **WHEN** a search or refresh removes a selected result from the visible list
- **THEN** its ID is removed from the selection and cannot affect later
  Quick Paste actions

#### Scenario: Reopen Quick Paste

- **WHEN** Quick Paste is opened again after being hidden
- **THEN** its previous multi-selection is cleared and the existing default
  result-selection behavior resumes
