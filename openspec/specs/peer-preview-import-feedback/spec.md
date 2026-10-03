# peer-preview-import-feedback Specification

## Purpose
Specify the feedback ClipVault provides while browsing and importing remote
clipboard previews from paired devices.

## Requirements

### Requirement: Show loading feedback over the remote preview rail

ClipVault SHALL show a floating loading indicator centered over the
horizontal remote-capture rail whenever previews for the selected, available
peer are being fetched. The indicator SHALL not resize the rail, shift its
cards or intercept card interaction. It SHALL expose an accessible loading
status using localized product text. Only completion of requests for the
currently selected peer SHALL update the visible loading state.

#### Scenario: Browse another available peer

- **WHEN** the user selects a different trusted, available peer and its
  preview page is being fetched
- **THEN** a spinner appears at the visual center of the horizontal preview
  rail
- **AND** the rail geometry and card positions remain unchanged
- **AND** the spinner does not block interaction with any preview card

#### Scenario: Preview streams resolve at different times

- **GIVEN** the text and image preview requests for the selected peer are
  outstanding
- **WHEN** only one request resolves and the other remains pending
- **THEN** the spinner remains visible while the rail may show rows from the
  completed request
- **AND** it disappears after all requests for that page have settled,
  including when a request failed

#### Scenario: Preview page is empty or fails

- **WHEN** all preview requests settle with no rows or with an error
- **THEN** the spinner disappears and the existing empty or error state is
  available to the user

#### Scenario: A prior peer response arrives late

- **GIVEN** the user selected peer Y while a preview request for peer X was
  still outstanding
- **WHEN** the request for peer X resolves
- **THEN** it does not replace peer Y's rows or change peer Y's loading state

### Requirement: Identify the local collection for a duplicate import

When an explicit text or image import reuses a capture already stored
locally, ClipVault SHALL report that the capture already exists in the
collection bound to the selected peer. The displayed collection name SHALL
come from local collection data, never from the remote capture payload. The
visible result SHALL be constrained to the card width and truncated with an
ellipsis when needed; its accessible label SHALL retain the complete
localized result and collection name.

#### Scenario: Duplicate text capture

- **WHEN** importing a text preview reuses a local capture
- **THEN** the result identifies the capture as already present and names the
  local peer-bound collection
- **AND** the result fits within the card without horizontal overflow

#### Scenario: Duplicate image capture

- **WHEN** importing an image preview reuses a local capture
- **THEN** the result identifies the capture as already present and names the
  local peer-bound collection
- **AND** the result fits within the card without horizontal overflow

#### Scenario: Collection name is not in the current projection

- **GIVEN** the import result identifies a local `collection_id` whose name
  has not yet been loaded into the frontend projection
- **WHEN** ClipVault displays the duplicate result
- **THEN** it resolves the name from the local database before showing the
  result
- **AND** no collection name is accepted from the peer's capture payload

### Requirement: Dismiss successful import results after a short delay

ClipVault SHALL automatically dismiss the success and duplicate-result
messages shown after importing a remote preview three seconds after the latest
such result appears. A later import result SHALL replace the prior result and
restart the dismissal timer. Error messages SHALL retain their existing
behavior.

#### Scenario: Successful import result expires

- **WHEN** a remote preview import succeeds without reusing an existing local
  capture
- **THEN** the success message remains visible for three seconds and then
  disappears without changing the imported capture

#### Scenario: Duplicate result expires

- **WHEN** a remote preview import reports that the capture already exists
- **THEN** the duplicate message remains visible for three seconds and then
  disappears without changing local collections or capture data

#### Scenario: A new result replaces the previous timer

- **GIVEN** an import result is visible on a remote preview card
- **WHEN** another import result is shown before the first timer expires
- **THEN** the newest result is displayed and receives its own full
  three-second visibility period

#### Scenario: Card is removed while a result timer is active

- **WHEN** the preview card is unmounted before the result expires
- **THEN** its timer is canceled and cannot update a removed card
