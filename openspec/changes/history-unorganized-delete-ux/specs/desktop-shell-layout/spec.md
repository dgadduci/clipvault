## MODIFIED Requirements

### Requirement: Global clear-history toolbar action

ClipVault SHALL expose the existing global clear-history action only while the
system `Historial` collection is active. The action SHALL be represented by a
danger-styled trash icon with the accessible name and tooltip
`Eliminar capturas no organizadas`. It SHALL reuse the existing confirmation
and `clearUnorganizedHistoryCommand` flow; the visibility change MUST NOT
change the command's deletion predicate.

#### Scenario: Trash action in Historial

- **WHEN** the active view is `Historial`
- **THEN** the desktop renders exactly one global trash action with
  `data-testid="trash-clear-history"`, `aria-label="Eliminar capturas no organizadas"`
  and the same existing request-clear callback

#### Scenario: Trash action absent in a user collection

- **WHEN** the user activates a user-defined collection
- **THEN** the global trash action is absent from the DOM, keyboard tab order
  and accessibility tree, while the cards' contextual actions remain
  available

#### Scenario: Returning to Historial restores the action

- **WHEN** the user navigates from `Historial` to a user collection and back
- **THEN** the same single trash action becomes visible again without creating
  duplicate handlers or changing the active search/listener state

#### Scenario: Search does not change the action scope

- **WHEN** the user remains in `Historial` and enters a search query
- **THEN** the trash action remains available and its command keeps the
  existing global unorganized-history predicate rather than deleting only
  visible search matches

#### Scenario: Open clear confirmation

- **WHEN** the user activates the trash action in `Historial`
- **THEN** the existing clearable-count request and confirmation dialog open
  before any history mutation occurs, and the wording identifies non-favorite
  entries without user collections

#### Scenario: Cancel clear confirmation

- **WHEN** the user cancels the global clear confirmation
- **THEN** no clear command is invoked and no card, favorite, tag, collection
  membership or image asset is modified

#### Scenario: Confirm unorganized clear

- **WHEN** the user confirms the global clear action in `Historial`
- **THEN** `clearUnorganizedHistoryCommand({ confirm: true })` is invoked once,
  only non-favorite entries without user collections are removed, and the
  cards and clearable count are refreshed after success

#### Scenario: Organized and favorite entries are preserved

- **WHEN** the clear operation encounters a favorite entry or an entry linked
  to one or more user collections
- **THEN** that entry, its organization associations and any referenced image
  or rich assets remain intact

#### Scenario: Individual card actions remain distinct

- **WHEN** the user views a card in `Historial` or in a user collection
- **THEN** the card menu's individual `Delete` action and the contextual
  `Quitar de esta colección` action retain their existing visibility,
  callbacks and semantics, independently of the global trash action

#### Scenario: Clear failure is recoverable

- **WHEN** the count request or clear command fails, or the backend returns
  `confirmation_required`
- **THEN** the existing typed error state is shown without exposing clipboard
  content, and the toolbar remains usable when `Historial` is still active

#### Scenario: Privacy of the affordance

- **WHEN** the toolbar, tooltip or confirmation is rendered
- **THEN** it contains only the action description and metadata count, never
  clipboard content, snippets, hashes, source application identifiers or
  filesystem paths
