## ADDED Requirements

### Requirement: Sidebar list height allocation

The desktop sidebar SHALL place the collection-list viewport and the linked-
peers section inside a shared vertical container. Relative to that container's
available height, the collection-list viewport SHALL occupy 70% and the
linked-peers section SHALL occupy 30%. The collection heading and any
collection-creation controls SHALL remain outside the shared container. Both
lists SHALL retain their own bounded vertical scrolling, and this allocation
SHALL NOT increase the workspace or window height.

#### Scenario: Sidebar divides the shared list container in a 70/30 ratio

- **WHEN** the desktop sidebar renders the collection and linked-peer lists
- **THEN** the collection-list viewport receives 70% of the shared container's
  available height
- **AND** the linked-peers section receives the remaining 30%
- **AND** the collection heading and creation controls do not count toward that
  ratio

#### Scenario: Both lists scroll independently within their allocated areas

- **WHEN** either list contains more rows than fit in its allocated area
- **THEN** that list scrolls vertically without changing the 70/30 allocation
- **AND** the other list and the workspace height remain unchanged
- **AND** collection selection and drag-and-drop over visible collection rows
  continue to work
