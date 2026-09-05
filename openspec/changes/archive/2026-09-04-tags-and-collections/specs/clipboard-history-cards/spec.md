## ADDED Requirements

### Requirement: Organize cards with collections and tags

The existing square card rail SHALL expose organization actions without
changing the established card dimensions or source-app/icon/title layout.

#### Scenario: Card organization menu

- **WHEN** the user opens a card's ellipsis menu
- **THEN** it offers `Agregar tag` and `Agregar a colección` through accessible
  multi-select flows and retains all existing actions

#### Scenario: Collection context action

- **WHEN** a card is shown inside a user collection
- **THEN** its menu offers `Quitar de esta colección` without removing it from
  `Historial`

#### Scenario: Historial context action

- **WHEN** a card is shown inside `Historial`
- **THEN** removing it uses the existing confirmed global delete flow rather
  than a collection-only unlink
