# Delta: etiquetas de colecciones en cards

## ADDED Requirements

### Requirement: Show assigned collections below card tags

Each history card SHALL render assigned collections immediately below the
existing tag line using compact chips with the same visual treatment as tags.
Only user collections SHALL appear as inline chips; the protected `Historial`
membership SHALL NOT appear as an inline chip. Each chip SHALL use the
collection's persisted color as its text color. If the user-collection chips do
not fit in the card, the card SHALL expose an accessible `+N` collection chip
that opens a read-only modal containing every assigned collection, including
`Historial`.

#### Scenario: Card shows collection memberships when they fit

- **WHEN** an entry belongs to `Historial`, `Trabajo` and `Clientes`
- **THEN** the card shows `Trabajo` and `Clientes` as collection chips below
  the tags when they fit
- **AND** the `Historial` label is not rendered inline
- **AND** each chip has a border, background, rounded shape and compact
  spacing consistent with the existing tag chips

#### Scenario: Collection labels use persisted colors

- **WHEN** assigned collections have different `color_hex` values
- **THEN** each chip's text uses the color of its own collection
- **AND** changing one collection color refreshes all visible chips for that
  collection after the organization update succeeds

#### Scenario: Collection overflow opens the complete membership modal

- **WHEN** the combined collection labels exceed the available card width
- **THEN** the card shows the chips that fit and one accessible `+N` chip,
  where `N` is the number of hidden user-collection chips
- **AND** activating the `+N` chip opens a read-only modal listing every assigned
  collection, including `Historial`, with its name and persisted color
- **AND** the card keeps its fixed dimensions, preview, tags and actions
- **AND** overflow detection measures the intrinsic width of all user
  collection chips before deriving the visible subset, so a clipped visible
  row cannot make a real overflow appear to fit
- **AND** the initial hydrated render and subsequent card resize recalculate
  the `+N` state from the actual DOM geometry

#### Scenario: History-only membership has no collection row

- **WHEN** an entry belongs only to the protected `Historial` collection
- **THEN** the card does not render an inline collection chip or `+N` overflow
  chip

#### Scenario: Collection membership modal closes safely

- **WHEN** the collection membership modal is open
- **THEN** Escape, the close button or the backdrop closes it
- **AND** focus returns to the `+N` overflow chip
- **AND** no collection association or clipboard value changes

#### Scenario: Collection chips and overflow do not change card actions

- **WHEN** the user hovers, focuses, selects or drags a card containing
  collection labels
- **THEN** the existing pin, menu, title, paste and drag-and-drop behavior is
  preserved
- **AND** the `+N` overflow chip is excluded from card selection and drag
  payload
- **AND** collection chips and modal data do not become drag payload or
  clipboard data

#### Scenario: Missing collection metadata is non-fatal

- **WHEN** an association cannot be hydrated or a legacy color is invalid
- **THEN** the card remains rendered with a safe visual fallback
- **AND** the card does not expose raw IDs, paths, hashes or clipboard content
