## MODIFIED Requirements

### Requirement: Render a bounded card preview using the shared preview projection

Desktop history cards SHALL reuse the existing safe preview projection used by
the preview surface. When a text entry has a detected code language or other
meaningful whitespace, the card MUST preserve supported syntax colors, tabs,
indentation and line breaks within its fixed bounds. The implementation MUST
not duplicate language detection, sanitization or highlighting rules.

#### Scenario: Code card shows the shared highlighted presentation

- **WHEN** a text entry has a supported detected code language
- **THEN** its Desktop card shows the same safe syntax highlighting projection
  used by the preview, bounded to the card geometry

#### Scenario: Whitespace remains visible in a code card

- **WHEN** the captured code contains tabs, indentation or multiple lines
- **THEN** the card preserves their visual separation and line structure rather
  than collapsing the content into a single plain-text paragraph

#### Scenario: Unsafe content remains sanitized

- **WHEN** a card preview contains scripts, event handlers, dangerous URLs or
  unsupported active content
- **THEN** the shared safe fallback/rendering blocks those features while
  keeping the visible text useful

#### Scenario: Non-code and image cards keep their contracts

- **WHEN** a card contains ordinary text, rich text or an image
- **THEN** its existing preview, thumbnail, asset lifecycle and fixed geometry
  remain intact and no code grammar is invented

### Requirement: Explain the capture type through an icon tooltip

The Desktop capture-type icon SHALL expose the canonical human-readable type
through an accessible tooltip or equivalent hover/focus affordance without
changing the card geometry.

#### Scenario: Type icon shows its label on hover

- **WHEN** the user hovers the capture-type icon
- **THEN** a tooltip shows the corresponding type label such as Texto, Imagen,
  HTML, JSON or Código · Python

#### Scenario: Type icon is accessible on focus

- **WHEN** keyboard focus reaches the capture-type icon or its wrapper
- **THEN** an accessible name exposes the same type meaning without relying on
  color alone

#### Scenario: Tooltip does not interfere with card controls

- **WHEN** the tooltip opens or closes
- **THEN** it does not select the card, open the card menu, start drag-and-drop
  or alter the card's fixed dimensions
