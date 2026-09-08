## MODIFIED Requirements

### Requirement: Render a safe rich card preview with preserved whitespace

The shared rich-text preview SHALL render the existing sanitized rich
representation without collapsing meaningful whitespace. It MUST preserve
line breaks, tabs, consecutive blank lines and indentation from the captured
plain/rich representation while retaining the supported formatting and the
existing fixed preview bounds. If the rich preview is unavailable, the plain
fallback SHALL preserve the same whitespace characters.

#### Scenario: Rich preview preserves line breaks

- **WHEN** a rich capture contains multiple paragraphs, explicit line breaks
  or consecutive blank lines
- **THEN** the preview displays the same line structure and does not collapse
  the lines into one paragraph

#### Scenario: Rich preview preserves tabs and indentation

- **WHEN** a rich capture contains tab characters or leading indentation
- **THEN** the preview preserves their visual separation with a stable tab
  width and does not replace the content with a collapsed single space

#### Scenario: Rich formatting remains safe

- **WHEN** the preview contains permitted font, color, weight, italic,
  underline, list or paragraph formatting
- **THEN** that formatting remains visible while scripts, event handlers,
  dangerous URLs, remote resources and unsupported active objects remain
  blocked

#### Scenario: Plain fallback preserves whitespace

- **WHEN** a rich preview reference is missing, invalid or still loading
- **THEN** the safe plain-text fallback preserves line breaks, tabs and blank
  lines and does not use the truncated card summary

#### Scenario: Shared preview keeps one implementation

- **WHEN** the same rich entry is previewed from Desktop and Quick Paste
- **THEN** both surfaces use the shared preview component/helper and produce
  the same whitespace and sanitization behavior
