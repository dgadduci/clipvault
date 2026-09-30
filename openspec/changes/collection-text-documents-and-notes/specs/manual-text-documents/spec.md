## ADDED Requirements

### Requirement: Create and edit manual text entries in eligible collections

ClipVault SHALL allow a user to create and edit a plain-text history entry
from the active Historial collection or a local user collection that is not
bound to peer imports. The editor SHALL support line breaks and SHALL persist
the text through a modal containing a multiline textarea, the shared Rust
core and SQLite history model. Every created entry SHALL belong to Historial
and, when created from another eligible collection, SHALL also belong to that
collection. The operation SHALL use the existing deterministic text
classification, content-hash and duplicate handling rules. A duplicate SHALL
resolve to the existing entry and ensure its membership in the requested
collection rather than create a duplicate history row. A collection bound to
peer imports SHALL be rejected as a creation target.

#### Scenario: Create a text entry in a local collection

- **GIVEN** the active collection is a local user collection not bound to a
  peer
- **WHEN** the user opens the text-entry modal, enters multiline text and
  confirms creation
- **THEN** ClipVault stores or resolves the text through the normal history
  rules
- **AND** the entry belongs to Historial and the active collection
- **AND** its card uses the icon for its deterministic content type

#### Scenario: Create a text entry from Historial

- **GIVEN** Historial is the active collection
- **WHEN** the user enters multiline text and confirms creation
- **THEN** the entry belongs to Historial and is visible in the history rail

#### Scenario: Preserve text line breaks

- **WHEN** the user creates or edits text containing multiple lines
- **THEN** ClipVault persists and displays the line breaks without converting
  the document into rich text or an external file

#### Scenario: Resolve duplicate manual text by existing capture rules

- **GIVEN** an entry with the submitted text already exists
- **WHEN** the user creates the text from an eligible local collection
- **THEN** ClipVault does not create a duplicate history row
- **AND** the existing entry is associated with the active collection

#### Scenario: Reject manual creation in a peer-import collection

- **GIVEN** the active collection is bound to a peer import
- **WHEN** a create request targets that collection
- **THEN** ClipVault returns a typed invalid-target result and creates no
  entry or membership

#### Scenario: Edit a manual text entry

- **GIVEN** a manual text entry is visible in Historial or another collection
- **WHEN** the user chooses the existing text-edit action, edits valid
  multiline text in its modal and saves
- **THEN** ClipVault updates the same entry using the existing text-edit
  contract and preserves its memberships and unrelated metadata

#### Scenario: Manual text creation is independent of clipboard capture

- **WHEN** automatic clipboard capture is paused or the clipboard source
  application is ignored
- **AND** the user explicitly creates a text entry in an eligible collection
- **THEN** ClipVault persists the manual entry without reading or writing the
  operating-system clipboard

#### Scenario: Manual text survives restart

- **WHEN** a manually created entry has been saved
- **AND** ClipVault restarts
- **THEN** the entry remains available under its saved Historial and local
  collection memberships
