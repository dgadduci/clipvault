## ADDED Requirements

### Requirement: Filter Desktop entries by tag in the active scope

Desktop SHALL provide an accessible tag combobox between the source-application
filter and the overflow menu. Its first option SHALL be `Todas`. The options
and filtering scope MUST match the active History or collection view. Selecting
a tag MUST filter immediately without an Apply button.

#### Scenario: Tag filter lists History tags

- **WHEN** the active scope is History and the tag filter is opened
- **THEN** it lists the tags present in visible History entries and starts with
  `Todas`

#### Scenario: Tag filter lists collection tags

- **WHEN** a user collection is active and the tag filter is opened
- **THEN** it lists only tags relevant to that collection's entries and does not
  include tags from unrelated scopes

#### Scenario: Selecting a tag filters immediately

- **WHEN** the user selects a tag
- **THEN** the Desktop cards update immediately to entries in the active scope
  associated with that tag, without a separate confirmation button

#### Scenario: Source-app and tag filters combine

- **WHEN** both a source application and a tag are selected
- **THEN** only entries matching both filters remain visible

#### Scenario: All tags clears only the tag filter

- **WHEN** the user selects `Todas`
- **THEN** the tag constraint is removed while the active scope, search query
  and source-application filter retain their existing values

#### Scenario: Scope changes reset invalid tag selection

- **WHEN** the user changes collection and the selected tag is not present in the
  new scope
- **THEN** the tag filter resets safely to `Todas` and the new scope is shown

#### Scenario: Refresh does not install duplicate listeners

- **WHEN** the history-updated event refreshes entries or tag options
- **THEN** the tag filter stays synchronized with the active scope without
  duplicate event listeners or stale responses
