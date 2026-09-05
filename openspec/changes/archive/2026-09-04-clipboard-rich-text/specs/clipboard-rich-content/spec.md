## MODIFIED Requirements

### Requirement: Capture supported image clipboard payloads

The existing text-first rule is refined: rich textual content MUST be
considered the highest-priority textual representation. Images MUST remain
the fallback only when no non-empty rich or plain text exists.

#### Scenario: Rich text and image are both available

- **WHEN** the clipboard exposes non-empty plain text with HTML or RTF and an
  image for the same operation
- **THEN** ClipVault captures one rich-text entry and does not create an image
  entry for that operation

#### Scenario: Plain text and image are both available

- **WHEN** the clipboard exposes non-empty plain text without a supported rich
  representation and an image
- **THEN** ClipVault captures text using the existing text pipeline and does
  not create an additional image entry

#### Scenario: Image remains the fallback

- **WHEN** the clipboard has no non-empty rich or plain text and contains a
  supported raster image
- **THEN** ClipVault captures the image through the existing image asset
  pipeline

#### Scenario: RTF-only is no longer treated as unsupported

- **WHEN** the clipboard exposes non-empty plain text plus a valid RTF flavor
  and no HTML
- **THEN** ClipVault captures it as `RichText` rather than ignoring it or
  reducing it to a plain-only entry

### Requirement: Paste Mode::Plain publishes only plain text

`PasteMode::Plain` MUST publish only the canonical plain text
flavour. On macOS the adapter MUST call `clearContents()` before
declaring the type set so residual `public.html`, `public.rtf` and
any prior rich flavours are dropped. The composite clipboard MUST
NOT delegate the plain write to a backend that leaves rich
flavours on the pasteboard.

#### Scenario: Plain paste clears residual rich flavours

- **WHEN** the user selects `Paste de texto plano` for any entry
- **THEN** the resulting pasteboard contains only the canonical
  plain text flavour; `public.html`, `public.rtf` and the previous
  capture's rich bytes are absent

#### Scenario: Plain paste does not write an artificial RTF

- **WHEN** the plain path is exercised
- **THEN** the contract reports `pasted` (or a typed failure); no
  artificial RTF wrapper is published to mask the missing colours
  or fonts of the source

#### Scenario: Rich paste still publishes original rich bytes

- **WHEN** the user selects `Paste de texto enriquecido` for a
  rich entry
- **THEN** the adapter publishes the original HTML and RTF
  representations and reports `pasted`; the plain-text fallback
  is only reported when the host cannot publish the rich
  flavours
