## ADDED Requirements

### Requirement: Preserve original characters in shared text previews

The shared Desktop and Quick Paste preview SHALL display the canonical captured
text with the same visible characters and whitespace as the stored capture.
Plain text MUST be rendered as inert text without pre-encoding it into visible
HTML entity spellings or interpreting the capture as markup.

#### Scenario: Quotes and entity-like text remain literal

- **WHEN** a text capture contains double quotes, apostrophes, ampersands, or
  literal text such as `&quot;`
- **THEN** the preview displays those exact captured characters, without
  replacing quotes with visible entity spellings or decoding literal entity-like
  text

#### Scenario: Markup-like capture remains inert text

- **WHEN** a text capture contains markup-like strings such as `<script>` or
  text with event-handler syntax
- **THEN** the preview displays the captured characters as text and creates no
  active element, handler, script execution, or navigation

#### Scenario: Desktop and Quick Paste use the same literal rendering

- **WHEN** the same textual entry is previewed from a Desktop card and from
  Quick Paste
- **THEN** both previews show the same complete canonical text and preserve its
  whitespace and visible characters
