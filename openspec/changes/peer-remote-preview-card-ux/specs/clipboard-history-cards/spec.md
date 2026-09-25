## MODIFIED Requirements

### Requirement: Card metadata hierarchy

Each recent-history card SHALL show a compact metadata header with a
representative content-type icon and a title centered between metadata areas.
For ordinary local captures, it SHALL show the source application's icon when
available; the application's user-visible name, bundle identifier, source
identifier and other source-application metadata SHALL NOT appear as visible
text and SHALL only be exposed through the `aria-label` and `title` attributes.
The exception is an imported entry rendered inside the peer-bound collection
for the peer that supplied that import: that card SHALL show the validated
source-application display name and the locally persisted application icon
associated with that peer's import provenance, falling back to a deterministic
local import icon if no valid application icon is available. It SHALL NOT
resolve an icon from a remote path, and the exception SHALL NOT expose the
source name or icon in general history or another peer's collection.

#### Scenario: Text capture shows type metadata

- **WHEN** a card represents a text capture classified as text, JSON, email,
  HTML or another supported textual type
- **THEN** the card shows the matching local type icon and label in its upper
  area

#### Scenario: Card shows source application icon only

- **WHEN** the capture has source-application icon metadata and a user-visible
  display name
- **THEN** the card renders the source icon and exposes the display name
  through `aria-label` and `title` only — the visual surface never carries the
  application name, the bundle identifier or the raw source identifier

#### Scenario: Metadata is unavailable

- **WHEN** an ordinary local capture has an unknown type, or its source
  application metadata is missing or cannot be loaded
- **THEN** the card uses deterministic generic type and application fallbacks
  (graphical only, no visible source-name text) and remains fully rendered

#### Scenario: Imported card shows peer-scoped source attribution

- **WHEN** an imported entry is rendered in the bound collection for the peer
  whose provenance contains a validated source-application display name
- **THEN** the card shows that name beside the validated local application
  icon, or the static local import icon when unavailable
- **AND** it does not request or resolve an icon from a remote path
- **AND** neither the peer identifier nor a bundle/source identifier is shown

#### Scenario: Imported source name or icon is unavailable

- **WHEN** the matching import provenance has no valid source-application
  display name or local icon reference
- **THEN** the card uses a static import icon when the icon is missing and
  shows an honest unknown application label when the name is missing

#### Scenario: Imported attribution does not leak across views

- **WHEN** the same local entry is rendered in general history or in a
  collection bound to a different peer
- **THEN** the peer-specific source name and import marker are not attributed
  to that view, and ordinary local source-app presentation remains unchanged
