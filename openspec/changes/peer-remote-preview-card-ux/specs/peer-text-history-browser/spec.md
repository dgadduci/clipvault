## ADDED Requirements

### Requirement: Remote preview cards match the local card geometry

The remote-history rail SHALL render text and image preview cards with the
same fixed square dimensions as local desktop history cards. The preview
content SHALL be clipped or contained within those dimensions and SHALL NOT
resize a card based on whether it contains text, a thumbnail, a placeholder,
or an import result.

#### Scenario: Text and image previews share local card dimensions

- **WHEN** the remote rail displays a text row and an image row
- **THEN** each remote card has the same fixed width and height as a local
  history card in the same desktop layout
- **AND** neither remote card grows beyond that footprint because of its
  content

### Requirement: Remote preview cards expose keyboard selection

The remote-history rail SHALL expose one selected card at a time, visibly
marked with the same blue selection accent used by local history cards and
accessibly identified as selected. `ArrowLeft` and `ArrowRight` pressed while
the rail or a card surface is active SHALL select the previous or next card in
display order and bring it into view; the handled key SHALL NOT perform the
rail's native horizontal scroll. Navigation SHALL not wrap at either end.
Typing surfaces and interactive card controls SHALL retain their own keyboard
behavior.

#### Scenario: Arrow keys move the remote-card selection

- **WHEN** a remote card is selected and the user presses `ArrowRight` or
  `ArrowLeft` while focus is not in an interactive control
- **THEN** selection moves to the adjacent card in that direction
- **AND** the newly selected card is brought into view without a native
  horizontal-scroll step
- **AND** its border displays the blue selection accent

#### Scenario: Clicking a remote card selects it without activating controls

- **WHEN** the user clicks a remote card's non-interactive surface
- **THEN** that card becomes the single selected card and shows the blue
  selection accent
- **AND** clicking its menu or import control does not change selection

#### Scenario: Navigation starts without a selected card and stops at bounds

- **WHEN** no remote card is selected and the user presses a horizontal arrow,
  or the selected card is already at the corresponding end of the list
- **THEN** the key selects the first or last card as appropriate when starting
  without selection, and otherwise stays at the boundary without wrapping

#### Scenario: Text entry and card controls keep their keyboard behavior

- **WHEN** a horizontal arrow is pressed in a text-entry surface or while an
  interactive menu control owns focus
- **THEN** the remote rail does not consume the key or change card selection

### Requirement: Remote card elapsed-time and menu metadata stay at the bottom

Every remote text and image preview card SHALL keep its elapsed-time label and
menu trigger aligned in a footer at the bottom edge of the fixed card. The
footer position SHALL remain the same when a card contains a text preview, a
thumbnail, a static image placeholder, or an import status.

#### Scenario: Footer alignment is independent of preview type

- **WHEN** the remote rail shows text and image cards with different preview
  content heights
- **THEN** each card's elapsed-time label and menu trigger appear on the same
  bottom-aligned footer row
- **AND** the preview content remains within the fixed card dimensions

### Requirement: Visible remote cards show source-app name and icon on demand

For a selected peer that advertises `source_app_presentation`, each visible
text or image preview card SHALL request the source application's validated
display name and optional PNG icon through the separate
`peer-source-app-presentation` route. The request SHALL not add source metadata
to browse rows or image-thumbnail responses. The card SHALL show the returned
name and icon when available, with generic/unknown fallbacks, while preserving
its fixed geometry and stale-response isolation.

#### Scenario: Visible card resolves source-app presentation

- **WHEN** a text or image card becomes visible for the active peer and that
  peer advertises `source_app_presentation`
- **THEN** the rail requests the presentation separately from its browse and
  thumbnail calls
- **AND** the card shows the original application's name and icon when valid
  values are returned

#### Scenario: Browse and thumbnail stay metadata-safe

- **WHEN** the peer history page or image thumbnail is requested
- **THEN** neither payload contains the source-app name, icon bytes, icon
  reference, path or application identifier
- **AND** the source name/icon is returned only by the visible-card route or
  an explicit import response

#### Scenario: Peer does not support presentation or metadata is absent

- **WHEN** the peer lacks the additive capability, the source app is unknown,
  or its icon/name is absent or invalid
- **THEN** the client does not treat this as a global history error
- **AND** the card shows a generic icon and an honest unknown-name fallback
  for missing values without changing size
