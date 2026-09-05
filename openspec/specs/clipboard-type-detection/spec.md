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
variants as snake_case strings, and SHALL NOT introduce a destructive
migration for the `clipboard_entries` table.

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
list, and SHALL treat unknown values as `Text`.

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
