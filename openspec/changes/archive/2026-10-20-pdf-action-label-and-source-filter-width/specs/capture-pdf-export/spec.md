# capture-pdf-export Specification Delta

## ADDED Requirements

### Requirement: Show a red PDF text label for export

The PDF export action on Desktop and QuickPaste MUST show the visible text
`PDF` in red instead of a document icon. It MUST retain its existing
semi-transparent red background, localized accessible action name, placement,
selection gating and export behavior.

#### Scenario: Desktop export action uses a red text label

- **WHEN** the Desktop toolbar shows the PDF export action
- **THEN** its visible content is the text `PDF` in red
- **AND** it retains the full localized accessible name and red-tinted
  semi-transparent background

#### Scenario: QuickPaste export action uses a red text label

- **WHEN** QuickPaste shows the PDF export action for a selection
- **THEN** its visible content is the text `PDF` in red
- **AND** activating it still exports the selection without confirming a paste
