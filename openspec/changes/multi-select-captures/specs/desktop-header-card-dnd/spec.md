## MODIFIED Requirements

### Requirement: Drag cards into user collections

ClipVault SHALL allow one capture or the selected group of captures to be
dragged onto a local user collection and SHALL add that collection membership
using the existing organization command. Dragging a selected card SHALL carry
the visible selected group; dragging an unselected card SHALL carry only that
card. The DataTransfer payload SHALL continue to carry only the opaque ID of
the card that starts the drag. Additional selected IDs MAY exist only in the
in-memory local drag session.

The drag source SHALL use the local pointer controller with a mouse fallback
for WebKit/Tauri sessions where HTML5 DataTransfer events are incomplete. It
SHALL activate only after the pointer crosses the configured movement
threshold, prevent accidental text selection while active, and clean up the
ghost, session, pointer capture and visual feedback on every completion or
cancellation path.

#### Scenario: Drag a card to a user collection

- **WHEN** the user drags a valid text or image card onto a local user
  collection
- **THEN** the target collection is added to the entry and the card remains in
  Historial and all previously assigned collections

#### Scenario: Drag begins on the visible card title

- **WHEN** the user begins the drag on the visible title of a text or image
  card and moves beyond the activation threshold
- **THEN** the same pointer/mouse drag flow starts and the title is accepted as
  a valid card surface

#### Scenario: Title editing remains available

- **WHEN** the user clicks, double-clicks, presses Enter or presses F2 on the
  title without crossing the drag threshold
- **THEN** the existing title focus and editing behavior remains available,
  while title editor inputs and confirmation/cancellation buttons never start
  a drag

#### Scenario: WebKit/Tauri mouse fallback completes the drop

- **WHEN** a WebKit/Tauri session emits `mousedown`, `mousemove` and `mouseup`
  without a complete Pointer Events sequence
- **THEN** ClipVault creates the same metadata-only drag session and applies
  the drop exactly once

#### Scenario: Active drag prevents text selection

- **WHEN** a card is moved beyond the activation threshold while its preview or
  neighboring cards contain text
- **THEN** the drag ghost is visible, text selection remains disabled, and
  the pointer can reach a scrolled collection target

#### Scenario: Drop is idempotent

- **WHEN** the user drops the same card on the same collection more than once
- **THEN** only one membership exists and no duplicate association or duplicate
  card is created

#### Scenario: Drop feedback is visible

- **WHEN** a valid draggable card is over a valid collection target
- **THEN** the target shows local visual feedback and clears it after drop,
  cancel or drag leave

#### Scenario: Invalid or unavailable drop is safe

- **WHEN** the payload is invalid, the target is Historial, the collection no
  longer exists or current associations cannot be hydrated
- **THEN** no destructive or replacement mutation is sent and the UI shows a
  safe no-op or error state

#### Scenario: Drag payload protects privacy

- **WHEN** a card starts a drag
- **THEN** the DataTransfer payload contains no clipboard content, snippet,
  hash, asset reference, file path or image bytes

#### Scenario: Keyboard alternative remains available

- **WHEN** a user cannot use drag-and-drop
- **THEN** the existing accessible collection-assignment action from the card
  remains available and produces the same association semantics

#### Scenario: Drag one unselected card

- **WHEN** the user drags a visible card that is not part of the selection
- **THEN** only that card is added to the target collection
- **AND** its existing Historial and collection memberships remain intact

#### Scenario: Drag the selected group

- **WHEN** the user drags a card that belongs to a selection containing
  multiple visible captures onto a local user collection
- **THEN** every selected capture receives the target collection membership
- **AND** every capture keeps Historial and all other collection memberships

#### Scenario: Group assignment is idempotent

- **WHEN** one or more selected captures already belong to the target
  collection
- **THEN** the drop creates no duplicate membership and still adds the target
  for the other selected captures

#### Scenario: Group drop on an ineligible target

- **WHEN** the selected group is released on Historial, a peer-bound
  collection, or a non-user target
- **THEN** no selected capture's membership changes and existing rejection
  feedback remains in effect

#### Scenario: Group drag keeps the privacy contract

- **WHEN** a selected group drag starts
- **THEN** the DataTransfer carries only the initiating capture's opaque ID
- **AND** neither clipboard content, snippets, hashes, asset references, paths
  nor image bytes are exposed through the payload or in-memory drag metadata

#### Scenario: Group drag cleanup

- **WHEN** a group drag completes, is cancelled with Escape/blur/pointercancel,
  or ends with pointerup/mouseup
- **THEN** the selected-ID session, ghost, selection lock, pointer capture and
  target feedback are cleaned up while the rail's selection remains usable
