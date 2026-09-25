## ADDED Requirements

### Requirement: Explicit image import may carry bounded source application attribution

The host SHALL include the captured source application's validated display
name and, when available, bounded PNG icon bytes in the response to an
explicit original-image import. It SHALL NOT include that metadata in image
history or thumbnail responses. The import response SHALL NOT include the
host's icon reference, filesystem path, bundle ID, raw source identifier, or
unrelated application metadata. The client SHALL store the optional name and
a locally generated icon reference with the import's peer provenance and
SHALL NOT overwrite source metadata on an existing deduplicated local entry.

#### Scenario: Explicit image import preserves available source name

- **WHEN** a user explicitly imports an eligible image whose host has a valid
  source-application display name
- **THEN** the authenticated original-image response carries only the bounded
  display name and optional bounded PNG icon bytes in addition to the existing
  image-import payload
- **AND** the committed peer provenance retains the name and any validated
  locally stored icon reference for the matching peer-bound collection
- **AND** the original image continues to be validated and persisted through
  the existing image-import transaction

#### Scenario: Image browsing and thumbnail preview do not expose source metadata

- **WHEN** the client lists remote images or requests a thumbnail without
  activating Importar
- **THEN** those responses contain no source-application name, identifier,
  icon reference or icon bytes

#### Scenario: Existing local entry is deduplicated

- **WHEN** the imported image reuses an identical local entry that already
  has source metadata
- **THEN** its existing local source metadata remains unchanged
- **AND** the remote source name and icon reference are stored only on the
  import provenance

#### Scenario: Source name is missing or invalid

- **WHEN** the host has no source name or its supplied name is empty, exceeds
  128 Unicode scalar values after trimming, or contains a control character
- **THEN** the import succeeds without source-name attribution and does not
  expose invalid metadata in errors or logs

#### Scenario: Source icon is invalid or unavailable

- **WHEN** the host has no source icon or its icon is not a valid PNG within
  the application-icon byte and dimension limits
- **THEN** the image import still succeeds when its image payload is valid
- **AND** no remote path/reference or invalid icon bytes are persisted
- **AND** the matching peer collection uses its generic import-icon fallback
