## ADDED Requirements

### Requirement: Arbitrate title editing and card drag gestures

ClipVault SHALL allow the visible card title to serve both as a title-editing
surface and as a drag source. A click or double click that does not cross the
configured drag threshold SHALL reach the title editing interaction. A gesture
that crosses the threshold SHALL activate the existing pointer/mouse drag
controller and SHALL preserve its ghost, pointer capture, selection lock,
collection hit-testing and cleanup behavior.

#### Scenario: Title double click is delivered

- **WHEN** the user double-clicks a card title without moving beyond the drag
  threshold
- **THEN** the title receives the double-click, the editor opens and no drag
  session or ghost is created

#### Scenario: Title drag remains available

- **WHEN** the user presses on the visible title and moves beyond the drag
  threshold
- **THEN** the existing drag controller starts a drag for that card and the
  card can be dropped on a user collection

#### Scenario: Native title click is not cancelled prematurely

- **WHEN** the user performs a title click or double click below the threshold
- **THEN** the controller does not prematurely prevent default, capture the
  pointer or claim the gesture as a drag

#### Scenario: Editor controls are not drag sources

- **WHEN** the user presses the title input, confirm icon or cancel icon while
  editing
- **THEN** the control keeps its normal editing behavior and no card drag is
  started

#### Scenario: Drag cleanup is preserved

- **WHEN** a title drag completes, is cancelled with Escape/blur/pointercancel,
  or ends with pointerup/mouseup
- **THEN** pointer capture, ghost, selection lock and the in-memory drag session
  are cleaned up exactly as in the protected baseline

#### Scenario: Drag payload remains private

- **WHEN** a title drag starts
- **THEN** the payload contains only the opaque entry identifier and never the
  title, clipboard content, snippet, hash, asset reference, path or image bytes
