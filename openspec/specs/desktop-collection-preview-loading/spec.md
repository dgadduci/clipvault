# desktop-collection-preview-loading Specification

## Purpose
Define how the desktop progressively displays collection captures and image
previews while keeping collection switching responsive and safe.

## Requirements

### Requirement: Progressive collection preview rendering

On local collection selection, the desktop SHALL show a loading indicator and
lock workspace interaction until metadata and visible image previews finish.
It SHALL display text cards when the entry snapshot arrives and image previews
as their local assets load. A failed image SHALL show its accessible fallback
and SHALL count as complete.

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

### Requirement: Preserve collection visibility during progressive loading

The desktop SHALL apply the selected collection's subcollection visibility
rule as each entry's membership loads. Hidden entries SHALL NOT block visible
previews, and results from a superseded collection load SHALL NOT update the
active rail or complete its loading state.

#### Scenario: The first rail frame appears before membership hydration

- **GIVEN** shared captures from subcollections are enabled for the selected
  collection
- **WHEN** the entry snapshot arrives and membership lookups are still pending
- **THEN** the new text cards and image loading placeholders are painted in the
  rail before those lookups start
- **AND** each membership result can reveal its eligible card while other
  entries are still being hydrated
- **AND** the spinner remains visible and the workspace remains locked

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
