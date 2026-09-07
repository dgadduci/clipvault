## MODIFIED Requirements

### Requirement: Persist image metadata and a local asset reference

Every persisted image entry SHALL use `content_type = image`,
retain its dimensions, MIME type, byte size and deterministic
hash, and store only a validated relative `asset_ref` instead of
an absolute filesystem path or raw image bytes in the textual
column.

#### Scenario: Valid image is committed

- **WHEN** a supported image passes validation and the local
  database commit succeeds
- **THEN** SQLite contains an image entry with `asset_ref`,
  `mime_type`, width, height, size and hash metadata, and the
  asset exists locally

#### Scenario: Existing textual rows are reopened

- **WHEN** ClipVault opens a database created before image
  support
- **THEN** existing textual rows remain readable, searchable
  and pasteable without requiring a destructive migration

#### Scenario: Asset persistence fails

- **WHEN** the image cannot be validated, encoded or written
  to the local asset store
- **THEN** ClipVault returns a typed non-fatal failure, does
  not create a partially valid image row and leaves existing
  history usable

### Requirement: Use a controlled local image asset store

Image assets SHALL be stored below
`<data_dir>/assets/clipboard/` with a reference of the form
`clipboard/<lowercase-sha256>.png`, written atomically, and
served only after namespace, traversal, symlink, PNG,
dimension and size validation.

#### Scenario: Asset reference is safe

- **WHEN** the frontend requests a persisted image using its
  `asset_ref`
- **THEN** the backend accepts only a relative reference
  inside the clipboard asset namespace and never returns an
  absolute path

#### Scenario: Malicious reference is supplied

- **WHEN** the asset command receives an absolute path,
  traversal component, wrong namespace, escaped symlink,
  invalid PNG or oversized image
- **THEN** it rejects the request with a stable typed error
  and returns no file bytes

#### Scenario: Duplicate asset already exists

- **WHEN** the normalized image hash maps to an existing
  valid asset
- **THEN** ClipVault reuses that asset and does not create a
  second file

### Requirement: Deduplicate image history by persisted bytes

ClipVault SHALL hash the bytes it actually persists on disk for
dedupe, shall not create duplicate rows for identical persisted
images, and shall preserve the existing hash and timestamp
semantics for text.

#### Scenario: Same image is copied again

- **WHEN** an identical persisted image is captured again
- **THEN** no second history row or asset is created and the
  existing entry is refreshed according to the current
  duplicate policy

#### Scenario: Different image is copied

- **WHEN** the persisted bytes differ
- **THEN** ClipVault creates a distinct image entry without
  modifying the prior entry's asset or metadata

### Requirement: Capture original PNG bytes when the clipboard exposes them

On hosts whose native clipboard exposes a `public.png`
representation (macOS via `NSPasteboard`), ClipVault SHALL
prefer the original PNG bytes over the AppKit-derived RGBA
bitmap the legacy `arboard::get_image` path produces. The
capture pipeline MUST preserve the original PNG bytes
verbatim when:

- the clipboard exposes a non-empty `public.png` flavour;
- the bytes parse as a valid PNG (correct signature, decoder
  succeeds);
- the decoded dimensions match the captured bitmap's
  dimensions and stay within `MAX_CLIPBOARD_IMAGE_DIM`;
- the byte length stays within `MAX_CLIPBOARD_ASSET_BYTES`;
- the RGBA frame derived from the PNG matches the bitmap the
  core uses for dedupe and paste suppression.

When no usable native PNG is exposed, ClipVault MAY fall back to the
legacy `arboard::get_image` + `normalize_image` path, because there is
no native PNG metadata to preserve. When the native macOS bridge is
temporarily unavailable, ClipVault MUST return a typed soft
unavailable result and retry on a later watcher tick; it MUST NOT
fall back to `arboard::get_image`, because that would silently erase
the resolution/profile metadata that may still be available from the
native pasteboard. When `public.png` is present but fails a structural
or pixel-coherence validation, ClipVault MUST return a typed
non-fatal failure and MUST NOT silently replace that representation
with a degraded legacy re-encode. In either case the watcher keeps
running and no partial image row is created.

When macOS provides a paired `public.tiff` representation,
ClipVault MUST treat the TIFF/AppKit X/Y resolution as
authoritative over a PNG `pHYs` value when the values differ.
This includes the common case where `public.png` contains a
default 72 ppi chunk while Preview reports 144 ppi for the
same source image. The persisted PNG MUST contain the
authoritative resolution and the same decoded pixels; it MAY
be rebuilt rather than preserved byte-for-byte because the
original PNG did not contain the correct resolution.

#### Scenario: macOS clipboard publishes PNG with pHYs and ICC profile

- **WHEN** the macOS clipboard exposes a PNG of 1104×396 px,
  144 ppi and a Display P3 / sRGB / ICC profile
- **AND** the platform bridge successfully hops to the Cocoa
  main thread and reads `public.png`
- **THEN** ClipVault persists the original PNG bytes verbatim
- **AND** `payload_width` and `payload_height` reflect the
  IHDR dimensions (1104×396), not any AppKit-derived
  representation
- **AND** the persisted file keeps its `pHYs`, `iCCP` /
  `sRGB`, `gAMA`, `cHRM` and any other safe metadata chunks

#### Scenario: Clipboard publishes a PNG without metadata chunks

- **WHEN** the clipboard exposes a PNG with only the IHDR,
  IDAT and IEND chunks
- **THEN** ClipVault persists the original PNG bytes verbatim
- **AND** the file remains decodable by the existing
  `decode_png` pipeline

#### Scenario: TIFF resolution overrides a default PNG resolution

- **WHEN** the macOS clipboard exposes pixel-identical
  `public.png` and `public.tiff` representations
- **AND** `public.png` contains a `pHYs` value of 72 ppi
- **AND** the TIFF/AppKit representation reports 144 ppi
- **THEN** ClipVault persists a PNG whose `pHYs` value is
  144 ppi on both axes
- **AND** the persisted PNG decodes to exactly the same width,
  height and RGBA pixels as `public.png`
- **AND** the capture does not silently fall back to the
  legacy RGBA re-encoder

#### Scenario: AppKit resolution fallback is used for incomplete TIFF IFDs

- **WHEN** `public.tiff` carries a decodable image whose
  logical AppKit size represents 144 ppi
- **AND** the narrow TIFF IFD parser cannot resolve a complete
  X/Y resolution pair
- **THEN** the main-thread bridge derives the bounded DPI pair
  from `NSBitmapImageRep` without returning image bytes or a
  path through diagnostics
- **AND** the persisted PNG receives a 144 ppi `pHYs` chunk
- **AND** its decoded pixels remain unchanged

#### Scenario: macOS screenshot Copy exposes only TIFF

- **WHEN** `⌘⇧4` is completed through the macOS screenshot UI's
  Copy action
- **AND** the pasteboard exposes a decodable `public.tiff` raster
  but no usable `public.png` representation
- **THEN** the native bridge decodes the TIFF on the Cocoa main
  thread at its original pixel dimensions
- **AND** ClipVault persists a PNG containing the TIFF/AppKit
  X/Y resolution (for example 144 ppi) and available colour
  profile metadata
- **AND** the persisted PNG contains the complete decoded raster,
  without routing this capture through `arboard::get_image`
- **AND** diagnostics identify the route as `tiff_metadata`, not
  `arboard_fallback`, without exposing TIFF bytes or paths

#### Scenario: TIFF metadata is stored in a secondary IFD

- **WHEN** the macOS clipboard exposes a valid `public.tiff`
  representation whose X/Y resolution is reachable through a
  `SubIFDs`, `ExifIFD` or `InteroperabilityIFD` pointer
- **AND** `public.png` contains a default or missing resolution
- **THEN** the bounded TIFF walker follows the referenced IFD and
  extracts the same authoritative X/Y resolution as a first-IFD
  representation
- **AND** ClipVault persists the PNG with that resolution while
  preserving the decoded pixels and without exposing TIFF bytes in
  diagnostics

#### Scenario: Clipboard publishes neither a usable PNG nor TIFF

- **WHEN** the clipboard exposes a raster image but neither a
  usable `public.png` nor a decodable `public.tiff`
  representation
- **AND** the legacy `arboard::get_image` path is the only
  available transport
- **THEN** the capture falls back to the existing
  `arboard::get_image` + `normalize_image` flow
- **AND** the persisted PNG is the legacy canonical RGBA
  8-bit PNG with no metadata chunks

#### Scenario: Original PNG validation fails

- **WHEN** the PNG bytes returned by `public.png` fail
  signature, dimension, size or RGBA consistency validation
- **THEN** the capture returns a typed non-fatal failure when a
  `public.png` representation was present but invalid, without
  falling back to a degraded re-encoding
- **AND** no partial image row is created
- **AND** the watcher keeps polling instead of failing

#### Scenario: macOS bridge cannot reach the main thread

- **WHEN** the `NSPasteboard` bridge cannot hop to the Cocoa
  main thread (no event loop, unit test outside Tauri)
- **THEN** the native adapter surfaces a typed `Unavailable`
  for the image read
- **AND** the composite clipboard does not invoke the legacy
  `arboard::get_image` path for that tick
- **AND** the watcher treats the result as a soft miss and retries
  on a later tick without creating a degraded image row

### Requirement: Paste selected image entries from persisted bytes

`paste_entry` SHALL write the exact bytes ClipVault persisted
for an image entry to the platform clipboard and then reuse
the existing paste controller, while leaving the history
entry unchanged when image writing or synthetic paste fails.
The bytes the paste publishes MUST be byte-identical to the
bytes the asset store wrote; the paste pipeline MUST NOT
re-decode, re-encode, resize or otherwise transform the PNG.

#### Scenario: Image paste succeeds

- **WHEN** the user selects an image entry in quick-paste or
  the main history and the platform supports image writing
  and synthetic paste
- **THEN** ClipVault writes the persisted PNG bytes verbatim,
  triggers paste into the previously active application and
  returns a typed success outcome
- **AND** the receiving application observes the same
  dimensions, resolution and color profile as the original
  clipboard capture

#### Scenario: Image paste capability is unavailable

- **WHEN** the current session cannot write image data or
  cannot perform the required synthetic paste
- **THEN** ClipVault returns a typed capability outcome with
  platform guidance when actionable, without converting the
  image to text or mutating history

### Requirement: Preserve privacy for image capture

The PrivacyGate SHALL be evaluated before permanent image
metadata or asset creation, and no image bytes, original PNG
bytes, thumbnails, hashes, paths or clipboard contents may be
placed in logs, Tauri events or user-facing error messages.

#### Scenario: Blacklisted application copies an image

- **WHEN** the source application matches the ignored
  application blacklist
- **THEN** ClipVault creates no image row, no image asset and
  no history-updated event for that capture

#### Scenario: Image capture is allowed

- **WHEN** PrivacyGate allows the source application
- **THEN** ClipVault may persist the image and source-app
  presentation metadata locally, but diagnostics and events
  remain metadata-only
- **AND** the original PNG bytes (when preserved) are never
  surfaced through `Debug`, `Display` or log lines
