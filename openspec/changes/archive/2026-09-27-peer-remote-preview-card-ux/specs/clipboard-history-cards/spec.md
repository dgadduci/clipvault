## MODIFIED Requirements

### Requirement: Card metadata hierarchy

Each recent-history card SHALL show a compact metadata header with a
representative content-type icon and a title centered between metadata areas.
For ordinary local captures, it SHALL show the source application's icon when
available; the application's user-visible name, bundle identifier, source
identifier and other source-application metadata SHALL NOT appear as visible
text and SHALL only be exposed through the `aria-label` and `title` attributes.
The exception is an imported entry rendered inside the peer-bound collection
for the peer that supplied that import, or in general history: the card SHALL
show the locally persisted application icon associated with the applicable
import provenance, falling back to a deterministic local import icon if no
valid application icon is available. The bounded source-application name
SHALL be exposed only through the icon's accessible label and `title`, not as
visible text. In general history, the most recently imported provenance wins
with a stable tie-breaker; the peer identifier is never shown. Another peer's
bound collection SHALL use only its own provenance. The card SHALL NOT resolve
an icon from a remote path.

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
- **THEN** the card shows the validated local application icon, or the static
  local import icon when unavailable, and exposes the name only through the
  icon's accessible label and `title`
- **AND** the name is not visible text beside the icon
- **AND** it does not request or resolve an icon from a remote path
- **AND** neither the peer identifier nor a bundle/source identifier is shown

#### Scenario: Imported source name or icon is unavailable

- **WHEN** the matching import provenance has no valid source-application
  display name or local icon reference
- **THEN** the card uses a static import icon when the icon is missing and
  shows an honest unknown application label when the name is missing

#### Scenario: General history shows latest import attribution without peer identity

- **WHEN** an imported local entry is rendered in general history
- **THEN** the card uses the latest import provenance and shows its icon or
  static import marker with the source name as accessible label/tooltip
- **AND** it does not expose the source peer's identifier

#### Scenario: Another peer's collection remains isolated

- **WHEN** the same local entry is rendered in a collection bound to a
  different peer
- **THEN** that card uses only that collection's peer provenance and never
  borrows another peer's source name or icon
