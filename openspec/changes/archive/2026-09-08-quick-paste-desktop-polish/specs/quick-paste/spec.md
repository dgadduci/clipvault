## MODIFIED Requirements

### Requirement: Handle Escape according to the active Quick Paste surface

Quick Paste SHALL distinguish its list and preview surfaces. Escape from the
preview MUST return to the capture list without hiding the Quick Paste window.
Escape from the list MAY hide the window using the existing close behavior.
The transition MUST preserve query, selected entry, selected index, scroll and
focus whenever the selected entry remains visible.

#### Scenario: Escape from preview returns to the list

- **WHEN** Quick Paste is showing an entry preview and the user presses Escape
- **THEN** the preview closes, the capture list becomes visible, and the Quick
  Paste window remains open

#### Scenario: Escape from the list closes Quick Paste

- **WHEN** Quick Paste is showing the capture list and the user presses Escape
- **THEN** the existing hide/close behavior runs and the window is no longer
  visible

#### Scenario: Returning from preview preserves list context

- **WHEN** the user opens a preview after searching, selecting an entry and
  scrolling the list, then presses Escape
- **THEN** the same query, selected entry and list context are restored without
  selecting the first entry unexpectedly

#### Scenario: Controls keep Escape precedence

- **WHEN** focus is inside an input, menu or other control that owns an Escape
  interaction
- **THEN** that control handles Escape first and Quick Paste does not hide or
  change surface unexpectedly

### Requirement: Render Quick Paste rows with fixed two-line geometry

Every Quick Paste capture row SHALL use the same fixed height and SHALL reserve
two visual lines even when its content fits on one line. Content, thumbnails,
tags and loading/error states MUST be clipped or ellipsized inside the fixed
geometry and MUST NOT increase row height.

#### Scenario: Short content keeps the same height

- **WHEN** one capture has a short title and another has long content
- **THEN** both rows have identical height and both reserve the two-line layout

#### Scenario: Long content is bounded

- **WHEN** a title, preview or tag list exceeds the available row width
- **THEN** it is truncated or summarized within the row without changing row
  height or widening the Quick Paste window

#### Scenario: Thumbnail and loading states keep geometry

- **WHEN** an image thumbnail is loading, loaded or failed
- **THEN** its placeholder and final thumbnail occupy the same fixed footprint
  and the row height remains unchanged

### Requirement: Show entry tags in Quick Paste rows

Quick Paste SHALL show the tags associated with each visible entry in the title
line, aligned toward the right, using the existing organization hydration and
tag presentation semantics. Missing, loading or failed tag metadata MUST keep
the row geometry stable.

#### Scenario: Tags appear for a tagged entry

- **WHEN** a visible entry has one or more persisted tags
- **THEN** the row shows compact escaped tag chips after the title without
  changing the fixed row height

#### Scenario: Untagged entry remains stable

- **WHEN** a visible entry has no tags
- **THEN** the title line remains aligned and no error text is shown as a tag

#### Scenario: Many tags are bounded

- **WHEN** an entry has more tags than fit in the title line
- **THEN** the row shows a bounded set of chips and an accessible remaining-count
  indicator without wrapping to a third line

#### Scenario: Stale tag hydration cannot overwrite another row

- **WHEN** Quick Paste changes scope or refreshes before tag hydration completes
- **THEN** a stale response does not attach tags to an entry from the new scope
