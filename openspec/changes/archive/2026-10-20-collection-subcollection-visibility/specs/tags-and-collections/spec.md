## ADDED Requirements

### Requirement: Filter shared captures in a principal collection view

The desktop collection list SHALL provide an accessible visibility toggle on
each user principal collection that has direct subcollections. The toggle
SHALL control whether captures directly associated with the principal remain
visible when they are also associated with one or more of its direct
subcollections. It SHALL default to showing those captures and SHALL retain
its state per principal collection for the current application session.
Collection memberships SHALL remain independent: a capture assigned only to
a subcollection SHALL NOT become a member of its principal collection.

#### Scenario: Show captures also assigned to a subcollection

- **GIVEN** a capture belongs directly to a principal collection and to one
  of its subcollections
- **WHEN** the principal collection's visibility toggle is active
- **THEN** the capture remains visible in that principal collection
- **AND** the principal's other directly associated captures remain visible

#### Scenario: Hide captures also assigned to a subcollection

- **GIVEN** a capture belongs directly to a principal collection and to one
  of its subcollections
- **WHEN** the principal collection's visibility toggle is inactive
- **THEN** the capture is hidden from the principal collection view
- **AND** a capture belonging directly to the principal but to no child
  remains visible

#### Scenario: Consider every direct subcollection

- **GIVEN** a capture belongs directly to the principal and to any one of its
  direct subcollections
- **WHEN** the visibility toggle is inactive
- **THEN** that capture is hidden even if it is not in the principal's other
  subcollections
- **AND** memberships in unrelated collections do not affect the result

#### Scenario: Keep subcollection-only captures independent

- **GIVEN** a capture belongs to a subcollection but not its principal
- **WHEN** the user selects the principal, with either toggle state
- **THEN** the capture is not added to the principal's view by this toggle
- **AND** all persisted memberships remain unchanged

#### Scenario: Apply visibility together with search and filters

- **GIVEN** the user has disabled shared captures for the selected principal
- **WHEN** recent entries or text-search results are displayed with tag or
  source-application filters
- **THEN** captures also assigned to any direct subcollection remain hidden
- **AND** the existing search, tag, source-application and ordering behavior
  is otherwise preserved

#### Scenario: Expose the toggle accessibly

- **WHEN** a principal collection with direct subcollections is rendered
- **THEN** its eye control appears immediately after the collection name and
  before the row's badges
- **AND** the note control and actions-menu control appear after the badges
- **AND** it has localized action text and an `aria-pressed` state
- **AND** the state and action text describe whether shared captures are
  currently shown or will be hidden
- **AND** Historial, child collections and principals without children do
  not show the control

#### Scenario: Widen the collection and linked-team panel

- **WHEN** the desktop organization panel is rendered
- **THEN** the grid column containing collections and linked teams has a
  minimum width of `198px` and a maximum width of `242px`
- **AND** its mobile single-column layout remains unchanged

#### Scenario: Keep the choice local to the session

- **WHEN** the user switches away from a principal and selects it again
- **THEN** its visibility choice remains in effect for that session
- **WHEN** ClipVault starts a new session
- **THEN** shared captures are visible by default
