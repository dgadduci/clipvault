## ADDED Requirements

### Requirement: Active toolbar filters have distinct visual states

Desktop SHALL show a semi-transparent, category-specific background and white
text when a toolbar filter has a selected value. Source-application, tag,
collection and capture-type filters SHALL use blue, violet, teal and amber
surfaces `rgba(37, 99, 235, alpha)`, `rgba(109, 40, 217, alpha)`,
`rgba(15, 118, 110, alpha)` and `rgba(180, 83, 9, alpha)`, respectively.
Each surface SHALL use the same alpha as collection-row tints
(`COLLECTION_ROW_TINT_OPACITY`, currently `0.18`). The corresponding active
filter icon SHALL also be white where it follows the text color. Unselected
filters and the `Todas` option SHALL retain the neutral background.

#### Scenario: Selected filters use distinct colors

- **GIVEN** one or more toolbar filters have selected values
- **WHEN** the toolbar is displayed
- **THEN** each selected source-application, tag, collection or capture-type
  control uses its defined color surface and white text
- **AND** simultaneous active filters remain distinguishable by hue

#### Scenario: Clearing a filter restores its neutral appearance

- **WHEN** the user selects `Todas` or clears an active filter
- **THEN** that control returns to the neutral background and default text
  color
- **AND** other selected filters keep their category-specific active colors
