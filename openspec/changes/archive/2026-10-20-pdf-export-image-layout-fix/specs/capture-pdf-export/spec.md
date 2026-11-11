# capture-pdf-export Specification Delta

## ADDED Requirements

### Requirement: Preserve image proportions and selection identity

PDF export MUST preserve the original aspect ratio of each image and fit it
inside the printable page area. When captures start on separate pages, each
page MUST show the image belonging to that capture's position in the user's
selection order.

#### Scenario: Fit landscape image to the printable width

- **WHEN** a landscape image is exported
- **THEN** it uses the available printable width when that is the limiting
  dimension
- **AND** its height is scaled by the same factor as its width

#### Scenario: Fit portrait image to the printable height

- **WHEN** a portrait image is exported
- **THEN** it uses the available printable height when that is the limiting
  dimension
- **AND** its width is scaled by the same factor as its height
- **AND** the image remains within the printable width

#### Scenario: Export multiple images on separate pages

- **WHEN** the one-capture-per-page setting is enabled for multiple selected
  images
- **THEN** every page shows the image for its corresponding selected capture
- **AND** the pages follow the user's selection order
