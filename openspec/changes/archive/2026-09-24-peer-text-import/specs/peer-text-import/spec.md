## ADDED Requirements

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
