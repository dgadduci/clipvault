## ADDED Requirements

### Requirement: Show rich text in the existing history card

The existing square `HistoryCard` and horizontal rail SHALL show a bounded,
sanitized rich preview for rich entries while retaining the current card
dimensions, title, type icon, source-app icon, pin/unpin and overflow layout.

#### Scenario: Rich card uses the existing visual foundation

- **WHEN** a rich entry appears in recent history
- **THEN** it is rendered by the same `HistoryCard` and `HistoryCardRail` as
  text and image entries, with no parallel gallery or variable card size

#### Scenario: Rich card falls back to text

- **WHEN** a rich preview is unavailable or invalid
- **THEN** the card displays the existing compact plain-text preview and
  remains keyboard and screen-reader usable

### Requirement: History card always renders the plain-text preview

The `HistoryCard` SHALL render the canonical plain text from
`entry.content` for every entry — including rich entries — using the
existing bounded `<pre class="preview">` element. The card MUST NOT
create a sandboxed iframe, request a `rich_preview_ref`, mint a
`blob:` URL or load sanitized rich HTML for the preview area. The
rich-text metadata remains available internally for the `Paste de
texto enriquecido` action but is never rendered in the card.

#### Scenario: Rich entry shows the canonical plain text

- **WHEN** a rich entry appears in recent history
- **THEN** the card displays the canonical `entry.content` text with
  the same line-clamp, font and background as a plain-text entry, and
  no iframe, blob URL or `@html` is created

#### Scenario: Plain entry shows the same plain preview

- **WHEN** a plain-text entry appears in recent history
- **THEN** the card displays `entry.content` with the same visual
  treatment as a rich entry; the two shapes are visually
  indistinguishable

#### Scenario: No rich preview iframe is created

- **WHEN** the rail mounts a card for a rich entry
- **THEN** the `HistoryCard` does not request the rich preview
  bridge, does not create a sandboxed iframe and does not load a
  sanitized preview blob; only the canonical plain text is rendered

### Requirement: Offer two paste actions in the card menu

Each textual card's accessible ellipsis menu SHALL expose `Paste de texto
enriquecido` and `Paste de texto plano` using the existing paste command and
focus/close contracts. Image cards SHALL expose a single `Paste` action that
delegates to the bitmap write path; they MUST NOT expose the textual paste
actions because the bitmap has no rich flavours and the plain fallback would
publish the empty content sentinel.

#### Scenario: Rich entry menu

- **WHEN** the user opens the menu of a rich entry
- **THEN** both paste actions are visible, accessible and independently
  selectable

#### Scenario: Plain-only entry menu

- **WHEN** the user opens the menu of an entry without rich metadata
- **THEN** the plain action is enabled and the rich action is visibly and
  accessibly disabled without attempting a paste

#### Scenario: Image card menu exposes only Paste

- **WHEN** the user opens the menu of an image entry
- **THEN** the menu renders exactly one `Paste` action that forwards
  `mode = null` to the paste command so the Rust paste service takes the
  image branch
- **AND THEN** neither `Paste de texto enriquecido` nor `Paste de texto
  plano` is rendered
- **AND THEN** the remaining card actions (editar título, restaurar
  título, pin/unpin, eliminar) keep working as for textual entries

#### Scenario: Paste action closes the menu

- **WHEN** the user activates any paste action
- **THEN** the menu closes once, the `pasteBusy` guard prevents duplicate
  invocation and the existing hide-before-paste/focus restoration flow is
  reused

#### Scenario: Paste error in the card

- **WHEN** the selected mode returns a failure or unavailable capability
- **THEN** the card shows safe actionable feedback, leaves the entry intact
  and does not display rich content in the error

## REMOVED Requirements

### Requirement: Render a safe rich card preview (REMOVED 2026-09-04)

The contract that required the card to render a sandboxed rich
preview iframe and load the sanitized rich preview reference has
been removed. The card now renders the canonical plain text for
every entry. The rich preview bridge and the
`richTextPreviewCommand` are preserved internally for future use
but the card no longer calls them.
