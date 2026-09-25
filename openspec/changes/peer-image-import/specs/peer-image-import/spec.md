## ADDED Requirements

### Requirement: Remote image metadata is exposed only by eligible active peers

An active trusted peer SHALL expose transferable image rows only when the
image entry and its local PNG asset pass the host's existing asset, dimension
and size validation. The row SHALL contain only an opaque remote reference,
validated optional title, type, date, byte size and dimensions. It SHALL NOT
contain image bytes, thumbnails, asset references, filesystem paths, hashes,
tags, collections, favorites or source-application metadata.

#### Scenario: Valid image is listed

- **WHEN** a trusted active peer has a persisted image whose asset is valid
- **THEN** the remote page may include one metadata-only image row and the
  client renders the common static image placeholder

#### Scenario: Image asset is missing or invalid

- **WHEN** a remote image row has a missing, malformed, oversized or
  out-of-namespace asset
- **THEN** the host omits it from the transferable image set and returns no
  asset bytes or local path

#### Scenario: Peer is not eligible

- **WHEN** the caller is not trusted and active, is revoked or blocked, or
  does not advertise `image_import`
- **THEN** the image listing is denied with a typed safe outcome and no rows

### Requirement: Missing image capability does not block text history

The remote history rail SHALL treat text and image browse results as
independent streams. If the image endpoint reports `peer_unavailable` because
the peer does not advertise `image_import`, the rail SHALL mark only the image
stream exhausted and SHALL still apply a successful text response from the
same request. The expected absence of an optional image capability SHALL NOT
be shown as a global history-load failure.

#### Scenario: Text-only peer keeps its text previews

- **WHEN** a text-compatible peer returns a valid text page while the image
  endpoint returns `peer_unavailable { reason: not_available }`
- **THEN** the rail renders the text rows, records the image stream as
  exhausted and shows no global load error

### Requirement: Complete image bytes are fetched only for explicit import

ClipVault SHALL request complete remote image bytes only when the user
activates `Importar` for a listed image row. The host SHALL revalidate caller
authorization, entry eligibility and the existing image size and dimension
limits before returning the persisted PNG. Browsing SHALL NOT fetch image
bytes, and importing SHALL NOT write to the clipboard or synthesize paste.

#### Scenario: User explicitly imports an image

- **WHEN** the user activates `Importar` for a listed image row
- **THEN** ClipVault fetches that image over the authenticated peer connection,
  validates it locally and only then begins local persistence

#### Scenario: Remote image changes before import

- **WHEN** the remote entry no longer exists, is no longer transferable or its
  asset fails host validation
- **THEN** the client receives a typed safe outcome and creates no local image
  entry, provenance or membership

#### Scenario: Image exceeds a configured limit

- **WHEN** the remote PNG exceeds the existing asset size, dimension or
  transport response limit
- **THEN** the request is rejected without retaining the rejected body or
  mutating local history

### Requirement: Imported images use the local canonical asset pipeline

ClipVault SHALL validate and normalize a received PNG through the existing
image pipeline, persist it under the local clipboard asset namespace, and
store only a validated relative local `asset_ref` in the image entry. It SHALL
never persist or resolve the remote asset reference or filesystem path.

#### Scenario: New image is imported

- **WHEN** a valid remote PNG has no identical local persisted image
- **THEN** ClipVault writes one local canonical asset and one image history
  entry with local hash, size, MIME type and dimensions

#### Scenario: Identical local image already exists

- **WHEN** the canonical persisted bytes match an existing local image
- **THEN** ClipVault reuses the local entry and asset without overwriting its
  title, timestamps, favorites, tags, collections or source metadata

#### Scenario: Remote title is supplied

- **WHEN** a new local image entry is created and the remote title passes the
  existing title validation
- **THEN** the title is preserved on creation only; later imports do not
  overwrite local title edits

### Requirement: Image import is idempotent and preserves peer organization

ClipVault SHALL record image imports using the existing peer provenance and
collection-binding rules. The idempotency key SHALL include `peer_id`, the
opaque remote entry identifier and the hash of the locally persisted canonical
PNG. A repeated identical snapshot SHALL not create duplicate entries, assets,
memberships or provenance rows.

#### Scenario: Same image snapshot is imported twice

- **WHEN** the same peer, remote entry and canonical image snapshot are
  imported again
- **THEN** ClipVault returns a deduplicated outcome and leaves one local entry,
  one collection membership and one provenance record

#### Scenario: Remote image is edited

- **WHEN** a later explicit import produces a different canonical image hash
- **THEN** ClipVault creates or reuses the corresponding local snapshot without
  changing the prior snapshot's asset or metadata

#### Scenario: First image import from a peer

- **WHEN** the peer has no collection binding and a valid image is committed
- **THEN** ClipVault creates the same peer-bound collection used by text import
  and adds the image atomically with membership and provenance

#### Scenario: Multiple peers import identical bytes

- **WHEN** two independent trusted peers import the same canonical image
- **THEN** ClipVault may reuse one local image entry while preserving an
  independent provenance and collection membership for each peer

### Requirement: Image import is atomic and leaves local assets intact on failure

ClipVault SHALL commit the local image entry, collection membership and remote
provenance only after the validated asset is safely persisted. A failed import
SHALL clean up only its temporary output and SHALL NOT delete or rename an
existing referenced asset.

#### Scenario: Database commit fails after asset staging

- **WHEN** SQLite rejects the image entry or organization transaction
- **THEN** ClipVault removes the temporary staged asset, leaves no partial
  entry or provenance and preserves all pre-existing assets

#### Scenario: Asset write fails before database commit

- **WHEN** the local asset store cannot validate or atomically write the PNG
- **THEN** ClipVault returns a typed persistence outcome and creates no image
  row, membership or provenance

### Requirement: Concurrent image imports coordinate staging leases

ClipVault SHALL protect the production SQLite staging cycle from racing with
another in-flight import on the same `asset_ref`. A rollback that drops a
freshly-`Written` asset SHALL wait until every other in-flight stage of the
same `asset_ref` either commits or rolls back, and SHALL refuse to unlink the
file as long as the staging lease counter is greater than one. The
`SqliteImageImportPersistence` adapter MUST carry the same staging lease
invariant the in-memory adapter exposes, so a regression that only fixes the
test double does not leave production unprotected. The final reference check
and unlink SHALL be serialized with both a new peer stage of that asset and a
local image capture's asset-write-through-SQLite-commit operation. A new peer
stage that encounters a cleanup reservation SHALL wait until cleanup finishes
before it stores or reuses the file.

#### Scenario: A writes and rolls back while B reuses the same asset

- **WHEN** import A stages a fresh PNG (`Written`) and import B stages the
  same PNG before A commits (`Reused`), then A's commit fails and the
  rollback path executes
- **THEN** the file stays on disk, A's rollback does not unlink it and B's
  subsequent commit still resolves the canonical `asset_ref` to a readable
  PNG

#### Scenario: Only one in-flight stage with no references

- **WHEN** exactly one import stages an asset as `Written`, no other import
  holds the lease, and no `clipboard_entries` row references the asset
- **THEN** the rollback path unlinks the staged PNG so the asset store does
  not accumulate orphans

#### Scenario: Staged `Reused` asset during rollback

- **WHEN** an import stages an asset as `Reused` and its commit fails
- **THEN** the rollback path MUST keep the file even when no row references
  it, so future imports keep hitting the dedupe contract

#### Scenario: A new stage starts after rollback reserves cleanup

- **WHEN** import A releases the last lease for a freshly-written asset and
  reserves its reference-check/unlink operation, then import B starts staging
  the same PNG before A's unlink finishes
- **THEN** B waits for A's cleanup, writes the PNG after the reservation is
  cleared, and can commit an entry whose `asset_ref` remains readable

#### Scenario: Local capture and peer rollback race on the same PNG

- **WHEN** a local image capture and a peer-import rollback concurrently
  access the same asset
- **THEN** the shared asset mutation lock ensures either the capture commits
  its row before rollback checks references, or it writes the asset again
  after rollback cleanup; no committed row points to a missing file

### Requirement: Image import refreshes local organization without clipboard mutation

After a successful commit ClipVault SHALL emit the existing metadata-only
history and organization update events. Import SHALL NOT invoke the clipboard
watcher, focus-dependent privacy gate, copy, paste or active-application
mutation. The remote image card SHALL remain separate from local card
drag-and-drop.

#### Scenario: Import succeeds

- **WHEN** the image transaction commits
- **THEN** Historial and the peer collection refresh with the local image and
  its remote-origin metadata, and the import result contains no image bytes

#### Scenario: User only browses remote images

- **WHEN** the user opens or paginates a remote history page without selecting
  Importar
- **THEN** no local entry, asset, clipboard value, paste event, collection or
  provenance row is created

#### Scenario: Local drag-and-drop remains protected

- **WHEN** a user interacts with an imported image card locally
- **THEN** the existing card test IDs, non-draggable behavior, opaque entry-only
  drag payload and listener cancellation rules remain unchanged

### Requirement: Image import reports safe typed outcomes

The transport, core and Tauri bridge SHALL distinguish authorization,
availability, eligibility, invalid-image, limit, transport and persistence
failures without returning image bytes, paths, remote asset references or
sensitive content in errors, logs, events or toasts.

#### Scenario: Peer becomes unavailable during fetch

- **WHEN** the authenticated connection closes or the peer loses active
  presence before the image is committed
- **THEN** ClipVault returns a retryable unavailable outcome and leaves local
  history, collections and assets unchanged

#### Scenario: Malformed image payload is received

- **WHEN** the received bytes fail PNG signature, decoding, dimensions or size
  validation
- **THEN** ClipVault rejects them without creating a local row or exposing the
  bytes in diagnostics

### Requirement: Host page projection distinguishes exhaustion from work-limit continuation

ClipVault SHALL distinguish "the persistence layer returned no more eligible
rows" from "the host hit the `PAGE_AFTER_MAX_BATCHES` budget with more
eligible rows still pending". When the latter happens the host MUST emit an
authenticated, signed continuation cursor whose payload is the
`(peer_id, created_at, id)` triple of the last row the host actually scanned,
so the next page request can resume from that exact position without
skipping eligible entries or surfacing invalid rows. The host MUST never
return an exhausted state because the batch budget was reached when eligible
candidates are still pending in the persisted history.

#### Scenario: More than eight batches of invalid PNGs precede valid PNGs

- **WHEN** the host serves a page through more than eight refill batches of
  invalid PNGs followed by valid PNGs and the requested `limit` is small
  enough that valid PNGs span multiple pages
- **THEN** the host returns a page with at most `limit` valid rows plus a
  signed continuation cursor, the client follows the cursor across multiple
  pages, every valid PNG appears exactly once, and the host eventually
  surfaces exhaustion without skipping an eligible row or surfacing an
  invalid row

#### Scenario: Persistence layer is exhausted

- **WHEN** the host's `fill_page_after` call returns no more candidates at
  all
- **THEN** the page response carries no `next_cursor` and the client treats
  the peer as exhausted for the image stream

#### Scenario: Eligible rows span a partial work-limited scan

- **WHEN** a bounded fill scans some eligible rows, reaches its work limit,
  and a later fill can find more eligible rows than the remaining page slots
- **THEN** the host returns at most `limit` rows, signs the cursor after the
  last row actually returned, and subsequent pages expose every remaining
  eligible row exactly once

#### Scenario: Empty page carries a work-limit continuation

- **WHEN** the bounded host scan reaches its outer work budget without
  finding an eligible image but candidates remain
- **THEN** the host returns an empty page with a signed continuation cursor
  and the client does not mark the image stream exhausted until no cursor
  remains
