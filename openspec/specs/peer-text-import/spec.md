## Purpose

Definir cómo ClipVault importa texto desde un par vinculado y activo sólo cuando el usuario activa Importar sobre una fila remota: la transferencia es autenticada, validada y limitada a 1 MiB UTF-8; cada importación es idempotente, queda asociada a una colección vinculada por peer_id y preserva los datos locales, sobreviviendo a revocaciones, bloqueos o eliminaciones de colección.

## Requirements

### Requirement: Complete remote text is fetched only for explicit import

ClipVault SHALL request complete remote text only when the user activates
Importar for a row of a trusted active peer. The host SHALL revalidate
authorization, entry eligibility and a maximum body size of 1 MiB UTF-8 before
returning it. Import SHALL NOT write to clipboard or synthesize paste.

#### Scenario: Explicit import request

- **WHEN** the user activates Importar for a listed remote row
- **THEN** ClipVault fetches only that entry over trusted TLS and validates it
  locally before any SQLite mutation

#### Scenario: Body is unavailable or too large

- **WHEN** the entry disappeared, became non-transferable or exceeds 1 MiB
- **THEN** the client receives a typed safe outcome, creates no local entry and
  does not log or retain the rejected body

### Requirement: Import creates an idempotent local snapshot

ClipVault SHALL hash and type a received text through local canonical helpers.
It SHALL create a local entry when absent or reuse an identical local entry
without overwriting existing local metadata. A valid remote title SHALL be
preserved only on a new local entry.

#### Scenario: Same snapshot is imported twice

- **WHEN** the same peer, remote entry and canonical content are imported again
- **THEN** ClipVault creates no duplicate entry, membership or provenance row

#### Scenario: Identical local content already exists

- **WHEN** local history already contains the canonical text
- **THEN** ClipVault reuses it and preserves its title, timestamps, favorites,
  tags, collections, source metadata and assets

#### Scenario: Remote content changed

- **WHEN** a remote entry is later edited and its canonical text differs
- **THEN** a later explicit import creates or reuses that new local snapshot
  according to its local hash

### Requirement: Each peer has an independent bound local collection

ClipVault SHALL bind imported entries to a user collection by peer_id, not
mutable name. The first collection name SHALL use the valid visible peer name,
with (equipo) only for a collision. Later remote renames SHALL NOT rename that
collection, and N peers SHALL retain independent bindings.

#### Scenario: First import for a peer

- **WHEN** the user imports a new snapshot from a peer without a binding
- **THEN** ClipVault creates and stores one bound collection and adds the entry
  to it atomically with provenance

#### Scenario: Peer collection was deleted

- **WHEN** a user deleted the bound collection and later imports again
- **THEN** existing entries and provenance remain and ClipVault creates a new
  binding without selecting a collection solely by matching name

### Requirement: Revoking peer access preserves local imports

Revoking, blocking or losing a peer SHALL stop future remote access but SHALL
NOT remove imported entries, their collections, membership, provenance or
assets. Import commit SHALL refresh local history and organization with
metadata-only events.

#### Scenario: Peer is blocked after import

- **WHEN** a user blocks a peer after importing text
- **THEN** the text remains in local history and its peer collection while
  future fetch/import requests are denied

### Requirement: Trusted peers expose only a bounded pageable text history

An active trusted peer SHALL expose a newest-first, cursor-paginated list only
of transferable textual entries. A transferable entry SHALL have textual
content and no image asset, asset MIME/size metadata or rich-text reference.
The listing SHALL contain no complete content payload, tags, collections,
favorites, source application metadata or content hash.

#### Scenario: Browse first recent page

- **WHEN** the user selects an active trusted peer and opens its history
- **THEN** ClipVault shows the most recent page of transferable text entries
  with title when present, content type, date and an escaped two-line preview
  capped at 300 characters

#### Scenario: Browse next page

- **WHEN** the user requests the next or previous remote page
- **THEN** ClipVault uses only the opaque cursor supplied by the host, limits a
  page to at most 50 rows, and does not issue a remote full-text search

#### Scenario: Image or rich-text entry exists on host

- **WHEN** the host history includes an image or an entry carrying rich-text
  payload metadata
- **THEN** that entry is not returned by the peer-history API and no disabled
  remote row is rendered

#### Scenario: Entry changes between list and import

- **WHEN** a host entry becomes non-transferable or disappears after listing
  but before a fetch
- **THEN** the host returns a typed safe outcome, the client creates no local
  entry, and the remaining remote list stays usable

### Requirement: Full remote text is transferred only by explicit import

ClipVault SHALL request the complete text of a remote entry only after the
user activates Importar for that row. The authenticated host SHALL recheck
trust, eligibility and a 1 MiB UTF-8 payload limit immediately before returning
it. Importing SHALL never modify a system clipboard or synthesize a paste.

#### Scenario: Explicit text import

- **WHEN** the user chooses Importar for a listed entry
- **THEN** the client fetches that entry over the trusted local transport,
  validates it locally and persists its local snapshot only after all checks
  succeed

#### Scenario: Preview does not fetch full content

- **WHEN** the user only opens or navigates a remote history page
- **THEN** ClipVault receives and renders only the bounded preview, not the
  complete remote text

#### Scenario: Text body exceeds the limit

- **WHEN** a host attempts to return more than 1 MiB UTF-8 for an import
- **THEN** the client rejects it before SQLite mutation, shows a typed error,
  and does not log or retain the rejected body

### Requirement: Import creates an idempotent local snapshot and peer collection membership

For a valid imported text snapshot, ClipVault SHALL calculate its local hash
and type using the normal deterministic local helpers. It SHALL create a local
entry if no matching content exists or reuse the matching entry without
overwriting existing local metadata. It SHALL associate the entry with exactly
the user collection bound to the remote peer_id and record provenance for
idempotence.

#### Scenario: First import from a peer

- **WHEN** the user imports a new text snapshot from a trusted peer for the
  first time
- **THEN** ClipVault creates a normal local history entry, preserves a valid
  remote title when supplied, creates a peer-bound user collection named for
  the visible peer (using (equipo) only to resolve a name collision), adds the
  entry to it, and records remote provenance

#### Scenario: Same snapshot is imported again

- **WHEN** the user imports the same peer, remote entry and canonical content
  snapshot again
- **THEN** ClipVault reuses the recorded/local entry and membership without
  creating a duplicate entry, collection or provenance row

#### Scenario: Local content already existed

- **WHEN** an identical text already exists in local history before import
- **THEN** ClipVault reuses that entry, preserves its title, timestamps,
  favorites, tags, collections, assets and source metadata, adds only the
  peer-collection membership/provenance, and reports a deduplicated import

#### Scenario: Remote title is invalid locally

- **WHEN** a remote title is absent or fails local title validation
- **THEN** the text may still be imported with the normal local title fallback,
  without persisting the invalid title or exposing it in an error payload

### Requirement: Imported text remains local after peer revocation

An imported entry, its collection membership and provenance SHALL be local
data. Revoking, blocking, losing connectivity with or deleting a peer
collection binding SHALL not remove local entries or their assets. A later
import MAY create a new bound collection only when the previous bound
collection was intentionally deleted.

#### Scenario: Peer is blocked after import

- **WHEN** the user blocks a peer after importing its text
- **THEN** the imported entry remains visible in local history and in its
  peer-named collection, while no new remote read or import is permitted

#### Scenario: User deletes the peer collection

- **WHEN** the user deletes the user collection bound to a peer
- **THEN** ClipVault retains the local entries and provenance; a future allowed
  import recreates a separate binding rather than reusing a collection by name

### Requirement: Remote history UI is read-only and isolated from local cards

The peer-history view SHALL render read-only remote rows with bounded previews,
pagination, loading/error states and explicit import controls. It SHALL not
reuse local HistoryCard drag/drop, editing, pin, menu or clipboard actions.
Successful imports SHALL refresh local history and organization through
metadata-only events after persistence commits.

#### Scenario: User views an active peer

- **WHEN** the user opens a trusted active peer history
- **THEN** remote rows cannot be dragged, edited, pinned, copied or pasted by
  local-card controls, and only their explicit import action can mutate local
  state

#### Scenario: Import finishes successfully

- **WHEN** an import transaction commits
- **THEN** ClipVault refreshes the local rail and collection snapshot without
  putting remote text in event payloads, diagnostics or drag data
