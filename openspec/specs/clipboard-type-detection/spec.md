## Purpose

Definir la detección determinística del tipo de contenido textual del portapapeles en el MVP v0.1, su persistencia compatible hacia atrás, su exposición como metadato en la UI y las garantías de privacidad asociadas.

## Requirements

### Requirement: Deterministic content-type detection for textual payloads

ClipVault SHALL classify every textual clipboard payload into exactly one
`ContentType` variant before persisting it. The detector SHALL be a pure
function in `clipvault-core` that does not depend on Tauri, SQLite, the
clipboard adapter, the filesystem, the network or any logger.

#### Scenario: A URL-shaped payload is detected as Url

- **WHEN** the clipboard returns a string that parses as an absolute URL
  with an allowed scheme (`http`, `https`, `ftp`, `sftp`, `ssh`, `file`,
  `mailto`, `tel`)
- **THEN** the captured entry stores `content_type = "url"` and the
  payload is persisted unchanged

#### Scenario: An email address is detected as Email

- **WHEN** the clipboard returns a single value that matches a strict
  `local@domain.tld` shape (no whitespace, valid local and domain parts)
- **THEN** the captured entry stores `content_type = "email"`

#### Scenario: A structured JSON document is detected as Json

- **WHEN** the clipboard returns a value that parses as a non-empty JSON
  object or array via `serde_json`
- **THEN** the captured entry stores `content_type = "json"`

#### Scenario: A three-segment JWT is detected as Jwt

- **WHEN** the clipboard returns a value that starts with `eyJ` and
  contains exactly three base64url segments separated by `.`
- **THEN** the captured entry stores `content_type = "jwt"`

#### Scenario: A UUID-shaped value is detected as Uuid

- **WHEN** the clipboard returns a value that matches a UUID in any
  of the documented forms (standard hyphenated, braced,
  parenthesized, `urn:uuid:`, no-hyphen) with valid hexadecimal
  digits at the expected positions
- **THEN** the captured entry stores `content_type = "uuid"`

#### Scenario: An IPv4 address is detected as Ipv4

- **WHEN** the clipboard returns a value that parses via
  `std::net::Ipv4Addr::from_str`
- **THEN** the captured entry stores `content_type = "ipv4"`

#### Scenario: An IPv6 address is detected as Ipv6

- **WHEN** the clipboard returns a value that parses via
  `std::net::Ipv6Addr::from_str`
- **THEN** the captured entry stores `content_type = "ipv6"`

#### Scenario: A hex color is detected as HexColor

- **WHEN** the clipboard returns a value of the form `#RGB`, `#RGBA`,
  `#RRGGBB` or `#RRGGBBAA` with valid hexadecimal digits
- **THEN** the captured entry stores `content_type = "hex_color"`

#### Scenario: An HTML snippet is detected as Html

- **WHEN** the clipboard returns a value containing at least one
  recognizable paired HTML tag (`<html>…</html>`, `<div>…</div>`,
  `<p>…</p>`, `<a …>…</a>`, `<script …>…</script>`,
  `<style …>…</style>`)
- **THEN** the captured entry stores `content_type = "html"`

#### Scenario: A file path is detected as FilePath

- **WHEN** the clipboard returns a value that looks like a macOS or
  Linux path (starts with `/Users/`, `/home/`, `/etc/`, `/usr/`,
  `/var/`, `/tmp/`, `/Volumes/`, `/private/`, `~/`, `./` or `../`)
  and is NOT a URL with a scheme
- **THEN** the captured entry stores `content_type = "file_path"`

#### Scenario: A shell command is detected as ShellCommand

- **WHEN** the clipboard returns a value with a shebang
  (`#!…`) or a multi-line command containing at least one of the
  shell operators (`|`, `&&`, `||`, `>`, `<`, `2>&1`) plus a known
  shell keyword (`ls`, `cd`, `grep`, `cat`, `echo`, `mkdir`, `rm`,
  `cp`, `mv`, `chmod`, `chown`, `awk`, `sed`, `curl`, `wget`, `git`,
  `npm`, `cargo`, `psql`, `ssh`, `docker`, `kubectl`, `make`, `yarn`,
  `pnpm`, `brew`, `apt`, `sudo`)
- **THEN** the captured entry stores `content_type = "shell_command"`

#### Scenario: A SQL statement is detected as Sql

- **WHEN** the clipboard returns a value that starts with a DML/DDL
  keyword (`SELECT`, `INSERT`, `UPDATE`, `DELETE`, `CREATE`, `DROP`,
  `ALTER`, `WITH`) AND the same or the following line contains a
  structural keyword (`FROM`, `INTO`, `TABLE`, `VALUES`, `SET`,
  `WHERE`)
- **THEN** the captured entry stores `content_type = "sql"`

#### Scenario: A fenced code block is detected as Code

- **WHEN** the clipboard returns a fenced code block (` ``` ` or
  `~~~`) with a recognized language tag, or a shebang
  (`#!/usr/bin/env …`, `#!/bin/…`)
- **THEN** the captured entry stores `content_type = "code"`

### Requirement: Stable precedence and Text fallback

ClipVault SHALL evaluate types in a fixed precedence order so the same
input always produces the same classification. Ambiguous, malformed or
unrecognized payloads SHALL be stored as `content_type = "text"`.

#### Scenario: JWT is preferred over URL when both could match

- **WHEN** the clipboard returns a value that is simultaneously a
  well-formed URL and a three-segment base64url string starting with
  `eyJ`
- **THEN** the captured entry stores `content_type = "jwt"`

#### Scenario: UUID is preferred over arbitrary text

- **WHEN** the clipboard returns a value that parses as a UUID AND
  could also be read as natural language
- **THEN** the captured entry stores `content_type = "uuid"`

#### Scenario: JSON is preferred over HTML when the shape is unambiguous

- **WHEN** the clipboard returns a value that parses as a non-empty
  JSON object or array
- **THEN** the captured entry stores `content_type = "json"`

#### Scenario: Empty or whitespace-only payloads persist as text

- **WHEN** the clipboard returns an empty string or a string that
  contains only whitespace
- **THEN** the capture pipeline returns `Ignored` and never persists
  a row; no `detect_content_type` result is needed

#### Scenario: Ambiguous prose persists as Text

- **WHEN** the clipboard returns a value that does not match any of
  the structured categories above (multi-line natural text, prose
  containing keywords but no syntax, etc.)
- **THEN** the captured entry stores `content_type = "text"`

### Requirement: Backward-compatible persistence

ClipVault SHALL extend `ContentType` with the new variants while
keeping `Text` as a stable value for legacy data, SHALL serialize all
variants as snake_case strings, SHALL extend the existing textual-entry
schema with nullable `code_language` metadata through an additive and
reversible migration, and SHALL NOT introduce a destructive migration
for the `clipboard_entries` table. Existing rows, content types,
payloads and asset references SHALL remain unchanged.

#### Scenario: Existing rows keep content_type = text

- **WHEN** the database was created before this change and contains a
  row with `content_type = 'text'`
- **THEN** ClipVault reads it back as `ContentType::Text` and the
  application keeps treating it as the historical default

#### Scenario: Unknown content_type values are rejected safely

- **WHEN** the database contains a row whose `content_type` value is
  not part of the documented enum
- **THEN** the entry repository returns a typed conversion error so
  the caller can decide how to handle it without panicking or
  silently mapping the value to an arbitrary variant

#### Scenario: Serialization is stable snake_case

- **WHEN** ClipVault serializes any `ContentType` variant to JSON or
  stores it in SQLite
- **THEN** the on-the-wire value is the same snake_case string used
  by `as_str()` (e.g. `ContentType::ShellCommand` → `"shell_command"`,
  `ContentType::Ipv4` → `"ipv4"`)

#### Scenario: The migration preserves existing entries

- **WHEN** the code-language migration is applied to a database
  containing text, code, rich text and image rows
- **THEN** every row remains readable, the new `code_language` column
  is null for legacy rows, and no image or rich-text asset is deleted
  or renamed

#### Scenario: The wire format is stable

- **WHEN** an entry is serialized through SQLite, core DTOs or Tauri
- **THEN** `code_language` is either `null` or the same canonical
  lowercase string, with no enum envelope or alias leaking across the
  boundary

### Requirement: Local search covers classified textual entries

ClipVault SHALL include every textual `ContentType` variant in the
local search query so classified entries remain findable alongside
the legacy `Text` rows.

#### Scenario: A classified URL is found by local search

- **WHEN** the user runs a local search query that matches the
  payload of an entry stored as `content_type = "url"`
- **THEN** ClipVault returns the entry as a search hit

#### Scenario: Search includes every textual variant

- **WHEN** the history contains one entry per textual variant
- **THEN** `EntryRepository::text_entries` returns all of them and
  the local search surfaces them for matching queries

#### Scenario: Management operations work on classified entries

- **WHEN** the user pins, deletes, clears or applies retention to an
  entry whose `content_type` is any textual variant
- **THEN** the management service operates on the row id exactly as
  it does for `text` entries, without changing the `content_type`
  value of the surviving rows

### Requirement: UI shows content_type as a metadata-only badge

ClipVault SHALL display the `content_type` returned by the backend as
an accessible badge in the history, search results and quick-paste
list, SHALL treat unknown values as `Text`, and SHALL display the
canonical programming language as secondary metadata for code entries
when it is available. The compact card body SHALL remain safe text;
syntax highlighting SHALL be provided by the shared full-preview
surface.

#### Scenario: The frontend renders the badge

- **WHEN** the backend returns an `EntryRecord` whose `content_type`
  is any supported variant
- **THEN** the UI renders a visible label for the variant (URL,
  Email, JSON, JWT, UUID, IPv4, IPv6, Hex color, HTML, Ruta, Shell,
  SQL, Código) next to the snippet

#### Scenario: Unknown values fall back to Text

- **WHEN** the backend returns an `EntryRecord` whose `content_type`
  is empty or not in the documented set
- **THEN** the UI renders the label `Texto` and the row remains
  visible / pastable / pinnable

#### Scenario: Quick-paste continues to work for classified entries

- **WHEN** the user opens the quick-paste window while the recent
  list contains classified entries
- **THEN** every entry is still selectable and pastable; the badge
  is decorative metadata only

#### Scenario: A code language is visible without replacing the type

- **WHEN** an entry has `content_type = "code"` and
  `code_language = "typescript"`
- **THEN** Desktop and Quick Paste show the code icon, the `Código`
  type label and a consistent `TypeScript` language label

#### Scenario: Legacy code remains usable

- **WHEN** an entry has `content_type = "code"` and
  `code_language = null`
- **THEN** the UI shows the existing generic code label/icon and
  renders the capture as safe plain text without an error

### Requirement: Privacy preserved during type detection

ClipVault SHALL classify payloads without logging the payload, the
hash, the snippet or any source-app identifier.

#### Scenario: Detection does not log the payload

- **WHEN** `record_payload` runs through the detector
- **THEN** no `tracing::*!` macro receives the payload, the hash or
  the classified `content_type` label, and the buffered log writer
  does not contain any of those substrings

#### Scenario: Blacklisted payloads never reach the detector

- **WHEN** the privacy gate returns `Discard` for the active source
- **THEN** the detector is not invoked and no payload from a
  blacklisted source reaches the storage layer

### Requirement: Capture pipeline persists the detected content_type

The capture pipeline SHALL classify every textual clipboard payload
before constructing the `NewEntry` and SHALL use the detected
`ContentType` when persisting the row. The dedupe-by-hash contract
SHRinks unchanged.

#### Scenario: A new capture persists the detected type

- **WHEN** the clipboard returns a payload that the detector
  classifies as `ContentType::Json`
- **THEN** the new row is inserted with `content_type = "json"`

#### Scenario: A duplicate capture keeps the original content_type

- **WHEN** the clipboard returns a payload that hashes to an existing
  row whose `content_type` is `ContentType::Url`
- **THEN** the existing row's `updated_at` / `last_seen_at` are
  refreshed and the `content_type` column stays `"url"`; no second
  row is created

### Requirement: Conservative programming-language metadata

ClipVault SHALL recognize common programming-language snippets locally
and SHALL persist an optional canonical `code_language` without
changing the original clipboard content. The initial canonical values
SHALL be `javascript`, `typescript`, `java`, `c`, `cpp`, `csharp`,
`python`, `rust`, `go`, `kotlin`, `swift`, `php`, `ruby`, `bash` and
`shell`.

#### Scenario: A confident JavaScript snippet is classified as code

- **WHEN** a textual capture has sufficient structural evidence and
  the local detector selects `javascript` above the configured
  confidence threshold
- **THEN** the entry stores `content_type = "code"`,
  `code_language = "javascript"`, and the original content remains
  unchanged

#### Scenario: A confident Python snippet is classified as code

- **WHEN** a textual capture contains unambiguous Python structure
  such as a function/class or a multi-line Python statement and the
  detector selects `python` above the configured threshold
- **THEN** the entry stores `content_type = "code"` and
  `code_language = "python"`

#### Scenario: A recognized fenced language wins over auto-detection

- **WHEN** a fenced or shebanged capture explicitly declares a
  supported language
- **THEN** the explicit language is normalized and persisted instead
  of using a different automatic candidate

#### Scenario: Ambiguous prose remains text

- **WHEN** a short or ambiguous capture has no explicit language and
  the best automatic candidate is below the threshold, tied, or
  lacks structural code evidence
- **THEN** the entry remains `content_type = "text"` with
  `code_language = null`

#### Scenario: Existing structured types keep their precedence

- **WHEN** a capture is already confidently classified as JSON, HTML,
  SQL, shell, URL, email, JWT, UUID, IP, color or file path
- **THEN** the existing `content_type` is preserved and automatic
  code-language detection does not overwrite it

### Requirement: Stable code-language persistence

ClipVault SHALL store `code_language` as nullable metadata using an
additive, reversible migration. The database, core DTOs, Tauri
response and frontend `EntryRecord` SHALL round-trip the canonical
value without exposing clipboard content.

#### Scenario: Legacy rows remain readable

- **WHEN** an entry predates the code-language migration
- **THEN** it loads successfully with `code_language = null` and
  retains its existing content type and content

#### Scenario: Unknown language values are rejected

- **WHEN** a persistence request or database row contains a language
  outside the canonical allowlist
- **THEN** ClipVault returns a typed validation/conversion error and
  does not silently store or display the unknown value

#### Scenario: Repeating the same classification is idempotent

- **WHEN** Desktop and Quick Paste both request the same language for
  the same entry
- **THEN** the database keeps one canonical value, the second request
  is reported as unchanged, and no duplicate history row is created

#### Scenario: Language classification survives restart

- **WHEN** an entry with `content_type = "code"` and a non-null
  `code_language` is stored and ClipVault restarts
- **THEN** the same metadata is returned by recent entries, search
  and Quick Paste

### Requirement: Shared local syntax highlighting

ClipVault SHALL use one frontend detector/highlighter helper and SHALL
reuse it from Desktop, `HistoryCard`, `ClipboardPreview` and Quick
Paste. The generated highlight markup SHALL remain local, read-only
and non-persistent.

#### Scenario: Preview renders persisted language

- **WHEN** a code entry has a supported `code_language` and the user
  opens its preview
- **THEN** the shared preview renders the complete original text with
  the corresponding local syntax grammar

#### Scenario: Missing language falls back safely

- **WHEN** a code entry has no language, an unavailable grammar or an
  invalid stale metadata response
- **THEN** the preview renders escaped plain text and the rest of the
  card/list remains usable

#### Scenario: Highlighting does not mutate history

- **WHEN** a user opens or closes a code preview in Desktop or Quick
  Paste
- **THEN** no clipboard write, paste, title, favorite, tag, collection
  or capture command is executed

#### Scenario: The same language label appears consistently

- **WHEN** the same code entry is visible in a Desktop card, search
  result and Quick Paste
- **THEN** each surface shows the same canonical language label and
  code icon

### Requirement: Local and privacy-preserving classification

Code-language detection SHALL execute locally, SHALL not add network
or telemetry behavior, and SHALL never write clipboard content,
generated HTML, bytes, hashes or absolute paths to logs, events or
command errors.

#### Scenario: Classification uses metadata-only persistence IPC

- **WHEN** the frontend persists a detected language
- **THEN** the command carries only the entry id and canonical
  language, and its response contains only status metadata

#### Scenario: Large captures do not block indefinitely

- **WHEN** a textual capture exceeds the configured detection limit
- **THEN** the detector skips or bounds the analysis, leaves the
  entry usable, and does not crash or create an unbounded UI task
