# Spec Delta

## ADDED Requirements

### Requirement: Card deletion from a user collection asks for its scope

The card `Eliminar` action inside a user collection SHALL present distinct
choices to remove the entry only from the active collection or to delete it
from all local history. The prompt SHALL identify the active collection and
explain that the global choice removes the entry from `Historial` and every
other collection. The `Eliminar` action in `Historial` SHALL keep its existing
confirmed global-delete flow. The prompt SHALL NOT expose clipboard content,
hashes, asset references, paths or peer identifiers.

#### Scenario: Card offers collection-only and global deletion

- **GIVEN** a card is displayed inside a user collection
- **WHEN** the user activates `Eliminar`
- **THEN** the card opens an accessible scope-choice prompt
- **AND** the prompt offers collection-only removal, global history deletion
  and cancellation
- **AND** no entry is changed before the user chooses an action

#### Scenario: Card is displayed in Historial

- **GIVEN** a card is displayed in `Historial`
- **WHEN** the user activates `Eliminar`
- **THEN** the existing confirmed global-delete flow runs without a
  collection-only choice

#### Scenario: Scoped delete prompt is private and accessible

- **WHEN** the scoped delete prompt is rendered or dismissed
- **THEN** it exposes only the safe collection label and action/count metadata
- **AND** cancel, Escape and focus restoration follow the existing modal
  behavior
