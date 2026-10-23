## ADDED Requirements

### Requirement: Progressive preview loading during collection selection

When the user selects a local collection, the desktop SHALL show the new
collection's text previews as soon as the entry snapshot is available and SHALL
render image previews as their local assets finish loading. The existing
collection loading indicator SHALL remain visible until collection metadata
hydration and all visible image preview requests reach a terminal state. The
workspace SHALL remain non-interactive until that load completes. A failed
image request SHALL render its accessible fallback and SHALL NOT leave the
workspace locked indefinitely.

#### Scenario: Loading feedback starts when the selection starts

- **WHEN** the user selects a different collection
- **THEN** the existing spinner is visible while the new entry snapshot is
  being fetched
- **AND** the workspace is non-interactive from that point until the active
  collection load completes

#### Scenario: New collection cards appear while image previews load

- **WHEN** the entry snapshot for a selected collection is available while
  one or more image assets are still loading
- **THEN** the new text previews and image-card loading placeholders are
  visible in the rail
- **AND** each image is replaced by its preview independently when its asset
  request completes
- **AND** the collection loading spinner remains visible until all visible
  previews and collection metadata have completed

#### Scenario: The first rail frame appears before membership hydration

- **GIVEN** shared captures from subcollections are enabled for the selected
  collection
- **WHEN** the entry snapshot arrives and membership lookups are still pending
- **THEN** the new text cards and image loading placeholders are painted in the
  rail before those lookups start
- **AND** each membership result can reveal its eligible card while other
  entries are still being hydrated
- **AND** the spinner remains visible and the workspace remains locked

#### Scenario: User input stays blocked during collection loading

- **WHEN** the selected collection is still loading
- **THEN** pointer and keyboard interactions with the desktop workspace do not
  invoke card, toolbar, sidebar or global shortcut actions
- **AND** the loading indicator remains perceivable as a status

#### Scenario: Failed image preview settles the loading state

- **WHEN** a visible image preview request fails or its asset is invalid
- **THEN** the card renders its existing accessible image fallback
- **AND** the failed preview counts as complete so the loading indicator can
  finish after the remaining previews and metadata settle

#### Scenario: Superseded collection load cannot overwrite the active rail

- **WHEN** a collection load is superseded by a newer scope request
- **THEN** results and preview completions from the older request do not
  replace entries or settle the newer request's loading state

#### Scenario: Hidden subcollection previews do not block completion

- **GIVEN** the selected parent collection hides captures assigned to its
  subcollections
- **WHEN** membership hydration determines which entries remain visible
- **THEN** only image previews for entries that remain in the rail are awaited
- **AND** hidden entries do not keep the loading indicator active
