## ADDED Requirements

### Requirement: Conservative programming-language metadata

ClipVault SHALL recognize common programming-language snippets locally and
SHALL persist an optional canonical `code_language` without changing the
original clipboard content. The initial canonical values SHALL be
`javascript`, `typescript`, `java`, `c`, `cpp`, `csharp`, `python`, `rust`,
`go`, `kotlin`, `swift`, `php`, `ruby`, `bash` and `shell`.

#### Scenario: A confident JavaScript snippet is classified as code

- **WHEN** a textual capture has sufficient structural evidence and the local
  detector selects `javascript` above the configured confidence threshold
- **THEN** the entry stores `content_type = "code"`,
  `code_language = "javascript"`, and the original content remains unchanged

#### Scenario: A confident Python snippet is classified as code

- **WHEN** a textual capture contains unambiguous Python structure such as a
  function/class or a multi-line Python statement and the detector selects
  `python` above the configured threshold
- **THEN** the entry stores `content_type = "code"` and
  `code_language = "python"`

#### Scenario: A recognized fenced language wins over auto-detection

- **WHEN** a fenced or shebanged capture explicitly declares a supported
  language
- **THEN** the explicit language is normalized and persisted instead of using
  a different automatic candidate

#### Scenario: Ambiguous prose remains text

- **WHEN** a short or ambiguous capture has no explicit language and the best
  automatic candidate is below the threshold, tied, or lacks structural code
  evidence
- **THEN** the entry remains `content_type = "text"` with
  `code_language = null`

#### Scenario: Existing structured types keep their precedence

- **WHEN** a capture is already confidently classified as JSON, HTML, SQL,
  shell, URL, email, JWT, UUID, IP, color or file path
- **THEN** the existing `content_type` is preserved and automatic code-language
  detection does not overwrite it

### Requirement: Stable code-language persistence

ClipVault SHALL store `code_language` as nullable metadata using an additive,
reversible migration. The database, core DTOs, Tauri response and frontend
`EntryRecord` SHALL round-trip the canonical value without exposing clipboard
content.

#### Scenario: Legacy rows remain readable

- **WHEN** an entry predates the code-language migration
- **THEN** it loads successfully with `code_language = null` and retains its
  existing content type and content

#### Scenario: Unknown language values are rejected

- **WHEN** a persistence request or database row contains a language outside
  the canonical allowlist
- **THEN** ClipVault returns a typed validation/conversion error and does not
  silently store or display the unknown value

#### Scenario: Repeating the same classification is idempotent

- **WHEN** Desktop and Quick Paste both request the same language for the same
  entry
- **THEN** the database keeps one canonical value, the second request is
  reported as unchanged, and no duplicate history row is created

#### Scenario: Language classification survives restart

- **WHEN** an entry with `content_type = "code"` and a non-null
  `code_language` is stored and ClipVault restarts
- **THEN** the same metadata is returned by recent entries, search and Quick
  Paste

### Requirement: Shared local syntax highlighting

ClipVault SHALL use one frontend detector/highlighter helper and SHALL reuse it
from Desktop, `HistoryCard`, `ClipboardPreview` and Quick Paste. The generated
highlight markup SHALL remain local, read-only and non-persistent.

#### Scenario: Preview renders persisted language

- **WHEN** a code entry has a supported `code_language` and the user opens its
  preview
- **THEN** the shared preview renders the complete original text with the
  corresponding local syntax grammar

#### Scenario: Missing language falls back safely

- **WHEN** a code entry has no language, an unavailable grammar or an invalid
  stale metadata response
- **THEN** the preview renders escaped plain text and the rest of the card/list
  remains usable

#### Scenario: Highlighting does not mutate history

- **WHEN** a user opens or closes a code preview in Desktop or Quick Paste
- **THEN** no clipboard write, paste, title, favorite, tag, collection or
  capture command is executed

#### Scenario: The same language label appears consistently

- **WHEN** the same code entry is visible in a Desktop card, search result and
  Quick Paste
- **THEN** each surface shows the same canonical language label and code icon

### Requirement: Local and privacy-preserving classification

Code-language detection SHALL execute locally, SHALL not add network or
telemetry behavior, and SHALL never write clipboard content, generated HTML,
bytes, hashes or absolute paths to logs, events or command errors.

#### Scenario: Classification uses metadata-only persistence IPC

- **WHEN** the frontend persists a detected language
- **THEN** the command carries only the entry id and canonical language, and
  its response contains only status metadata

#### Scenario: Large captures do not block indefinitely

- **WHEN** a textual capture exceeds the configured detection limit
- **THEN** the detector skips or bounds the analysis, leaves the entry usable,
  and does not crash or create an unbounded UI task

## MODIFIED Requirements

### Requirement: Backward-compatible persistence

ClipVault SHALL extend the existing textual-entry schema with nullable
`code_language` metadata through an additive and reversible migration. Existing
rows, content types, payloads and asset references SHALL remain unchanged.

#### Scenario: The migration preserves existing entries

- **WHEN** the migration is applied to a database containing text, code, rich
  text and image rows
- **THEN** every row remains readable, the new column is null for legacy rows,
  and no image or rich-text asset is deleted or renamed

#### Scenario: The wire format is stable

- **WHEN** an entry is serialized through SQLite, core DTOs or Tauri
- **THEN** `code_language` is either `null` or the same canonical lowercase
  string, with no enum envelope or alias leaking across the boundary

### Requirement: UI shows content_type as a metadata-only badge

The UI SHALL continue to display the existing content-type badge and SHALL
display the canonical programming language as secondary metadata for code
entries when it is available. The compact card body SHALL remain safe text;
syntax highlighting SHALL be provided by the shared full-preview surface.

#### Scenario: A code language is visible without replacing the type

- **WHEN** an entry has `content_type = "code"` and
  `code_language = "typescript"`
- **THEN** Desktop and Quick Paste show the code icon, the `Código` type label
  and a consistent `TypeScript` language label

#### Scenario: Legacy code remains usable

- **WHEN** an entry has `content_type = "code"` and `code_language = null`
- **THEN** the UI shows the existing generic code label/icon and renders the
  capture as safe plain text without an error
