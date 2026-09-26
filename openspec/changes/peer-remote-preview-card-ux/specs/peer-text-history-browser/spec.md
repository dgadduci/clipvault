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

### Requirement: Remote browse rows carry only the bounded source-app name

Text and image history browse rows SHALL include an optional normalized
source-app display name from that entry's own metadata. They SHALL NOT include
icon bytes, icon references/paths, bundle IDs, raw source identifiers, content
hashes or clipboard content. New clients SHALL display the name only when the
selected peer advertises `source_app_presentation`; peers predating the field
remain compatible. Image-thumbnail responses SHALL remain free of source-app
metadata. Preview loading SHALL NOT issue a separate per-card source-app
request or transfer a source-app icon.

#### Scenario: Name is available with the initial browse page

- **WHEN** a peer advertising `source_app_presentation` returns text or image
  history rows whose entries have valid source-app names
- **THEN** each browse row carries its validated, bounded source-app name
- **AND** the corresponding preview displays that name without an additional
  network request
- **AND** no source-app icon bytes are fetched or displayed

#### Scenario: Legacy or invalid source name

- **WHEN** the host is an older peer, or an entry has no valid source-app name
- **THEN** the new client receives `null`/no name and displays the honest
  unknown-name fallback without delaying the card
- **AND** a client without the capability does not display an unsolicited
  source-app name

#### Scenario: Browse and thumbnail payload boundaries

- **WHEN** history rows or image thumbnails are requested
- **THEN** browse rows may contain only the bounded display name in addition
  to their existing fields, and thumbnails contain no source-app metadata
- **AND** neither response includes icon bytes/references/paths, application
  identifiers, content hashes or clipboard content

### Requirement: Remote browse paints the first successful stream promptly

The remote-history rail SHALL keep text and image browse requests parallel,
but SHALL render rows from the first successful non-empty stream response
without waiting for the other stream. The provisional rows SHALL NOT commit or
advance either stream's cursor or buffer state. Once both requests settle, the
rail SHALL replace the provisional view with the normal newest-first,
deduplicated combined page. Pagination controls SHALL remain disabled during
this merge. A stale response SHALL NOT paint into a different peer/page, and
rows from a successful stream SHALL remain visible if the other stream fails.

#### Scenario: One stream responds before the other

- **WHEN** the text or image endpoint returns a non-empty successful first
  page while the other endpoint is still pending
- **THEN** the returned rows appear immediately with their source-app names
- **AND** the pending stream can later contribute rows to the authoritative
  combined page without duplicates or crossed cursors

#### Scenario: One stream fails after the other succeeds

- **WHEN** one browse endpoint succeeds with rows and the other endpoint fails
- **THEN** the successful rows remain visible with the retryable error
- **AND** the failed stream's cursor is not advanced
