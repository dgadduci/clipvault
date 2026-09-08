## Purpose

Definir la captura, persistencia, render seguro y pegado explícito de payloads de texto enriquecido del portapapeles (HTML, RTF y vista previa sanitizada), manteniendo coexistencia con el pipeline de texto plano, imagen y las restricciones de privacidad del MVP.

## Requirements

### Requirement: Capture and transport rich textual clipboard payloads

ClipVault SHALL represent a textual clipboard that has non-empty plain text
and at least one supported rich representation as `RichText`, preserving the
available HTML and RTF representations without coupling the core to a desktop
API. The existing `Text` and `Image` payloads SHALL remain supported.

#### Scenario: Clipboard exposes HTML and plain text

- **WHEN** the clipboard exposes non-empty plain text and HTML
- **THEN** ClipVault captures one `RichText` payload containing both
  representations and does not reduce it to `Text`

#### Scenario: Clipboard exposes RTF without HTML

- **WHEN** the clipboard exposes non-empty plain text and RTF only
- **THEN** ClipVault captures one `RichText` payload with the RTF representation
  and keeps the plain text fallback

#### Scenario: Clipboard exposes rich text and image

- **WHEN** a clipboard operation exposes rich text with non-empty plain text
  and also exposes an image
- **THEN** the rich textual payload wins deterministically and no additional
  image row is created

#### Scenario: Rich representation is malformed

- **WHEN** one rich representation cannot be parsed or normalized
- **THEN** a valid alternate representation is retained, or plain text is
  captured, and the watcher continues without logging the payload

### Requirement: Persist original rich representations and a safe preview

Each persisted rich entry SHALL retain the plain text in the existing
`content` field, SHALL store original supported HTML/RTF representations in
validated local assets, and SHALL store a separately generated sanitized
preview reference. Rich bytes SHALL NOT be placed in `EntryRecord` list
responses.

#### Scenario: Rich entry is committed

- **WHEN** a valid rich capture passes PrivacyGate and the SQLite transaction
  succeeds
- **THEN** the row contains the plain text, rich metadata, relative asset
  references and deterministic rich hash, and each referenced asset exists

#### Scenario: Legacy text row is reopened

- **WHEN** ClipVault opens a database created before rich-text support
- **THEN** the text row remains searchable, previewable and pasteable with all
  rich fields null

#### Scenario: Rich asset persistence fails

- **WHEN** an original or preview asset cannot be safely written
- **THEN** ClipVault returns a typed non-fatal failure, creates no partially
  valid rich row and leaves existing history usable

### Requirement: Render a safe rich card preview

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

### Requirement: Paste a rich entry in an explicit mode

ClipVault SHALL expose `Plain` and `Rich` paste modes through the existing
paste service and Tauri command boundary. `Plain` SHALL write only the
canonical plain text. `Rich` SHALL write the original available rich
representations plus the plain fallback, then reuse the existing paste
controller and previously active target.

#### Scenario: User chooses rich paste

- **WHEN** the user selects `Paste de texto enriquecido` for an entry with
  rich metadata
- **THEN** ClipVault writes the available original rich representations,
  performs the existing paste flow and returns a typed success outcome

#### Scenario: User chooses plain paste

- **WHEN** the user selects `Paste de texto plano`
- **THEN** ClipVault writes only `content` and performs the existing paste flow
  without consulting or exposing rich asset bytes

#### Scenario: Rich write is unavailable

- **WHEN** rich clipboard writing is unavailable but plain writing and
  synthetic paste are available
- **THEN** ClipVault writes plain text, pastes it and returns an explicit
  `pasted_plain_fallback` outcome

#### Scenario: No rich representation exists

- **WHEN** a plain-only entry is targeted for rich paste
- **THEN** the UI disables the rich action and the backend rejects the invalid
  mode without writing or pasting anything

#### Scenario: Paste fails

- **WHEN** asset reading, clipboard writing or synthetic paste fails
- **THEN** the source history entry remains unchanged and a typed failure or
  capability outcome is returned without payload details

### Requirement: Expose rich clipboard capabilities independently

The capability matrix SHALL report `clipboard_read_rich_text` and
`clipboard_write_rich_text` independently from plain and image capabilities.
Platform adapters SHALL distinguish macOS, Linux X11 and Linux Wayland and
MUST NOT claim rich support solely because plain text support exists.

#### Scenario: Rich capability is available

- **WHEN** the adapter verifies rich read or write support in the current
  session
- **THEN** only the corresponding rich capability is reported available

#### Scenario: Rich capability is unavailable

- **WHEN** the session cannot read or write rich clipboard data
- **THEN** the rich operation returns a typed capability result, plain text
  capture and paste remain usable, and no misleading permission guidance is
  shown

### Requirement: Protect rich content privacy

PrivacyGate SHALL run before rich originals, previews or derived rich assets
are persisted. Logs, diagnostics, Tauri events and user-facing errors SHALL
not contain plain content, HTML, RTF, rich hashes, snippets, filesystem paths
or raw bytes.

#### Scenario: Rich capture comes from an ignored application

- **WHEN** the source application is blacklisted
- **THEN** ClipVault creates no history row, no rich asset, no temporary
  persistent preview and no history-updated event

#### Scenario: Rich event is emitted after storage

- **WHEN** an allowed rich capture is stored or deduplicated
- **THEN** the existing metadata-only history-updated event is emitted with no
  payload or rich reference

### Requirement: Suppress autocapture of paste-owned payloads

A paste action that writes to the clipboard MUST NOT cause the
capture watcher to create a new history row for the same payload.
The core SHALL arm a metadata-only suppression token before every
clipboard write (plain, rich or image) and the capture watcher
SHALL consume the token before privacy gating, persistence,
metadata enrichment or the `history-updated` event. The token
identifies the payload by canonical hashes (plain text SHA-256,
canonical rich hash, image SHA-256) so OS-side normalisation of
HTML/RTF does not defeat the comparison. The token is bounded: it
expires after a short TTL, is consumed by a single matching
observation and is cleared when the clipboard write or the
synthetic paste fails.

#### Scenario: Plain paste does not create a new card

- **WHEN** the user selects `Paste de texto plano` for an entry
- **THEN** the next watcher tick that observes the same plain text
  reports `Suppressed` and ClipVault creates no new row, does not
  refresh an existing row and does not emit `history-updated`

#### Scenario: Rich paste does not create a new card

- **WHEN** the user selects `Paste de texto enriquecido` for a
  rich entry and the session publishes the original rich
  representations
- **THEN** the next watcher tick that observes the same payload —
  rich or text-only after OS normalisation — reports `Suppressed`
  and ClipVault creates no new row

#### Scenario: Synthetic paste failure keeps the clipboard clean

- **WHEN** the clipboard write succeeds but the synthetic paste
  fails
- **THEN** the suppression token is cleared, the paste surface
  reports a typed `PasteOutcome::Failed` and a subsequent user
  copy is captured normally

#### Scenario: Distinct user copy is still captured

- **WHEN** the user copies a different payload during the
  suppression window
- **THEN** the watcher reports `Captured(...)` for the new
  payload; the suppression only matches the exact fingerprint
  the paste service armed

#### Scenario: Token never carries sensitive content

- **WHEN** the suppression token is armed or consumed
- **THEN** the fingerprint stored on the registry contains only
  canonical SHA-256 digests; plain text, HTML, RTF, image bytes,
  hashes beyond the digest and filesystem paths never appear in
  logs, errors, events or `Debug` output
