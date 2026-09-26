## Context

See `proposal.md` for the user-visible problems. `RemotePreviewCard` is a
read-only component separate from `HistoryCard`; today it has content-driven
height and no rail-owned selection. The local rail already defines fixed
square geometry and tested horizontal-selection semantics. Peer browse DTOs
deliberately omit source-app metadata, while the explicit text/image fetch
paths and `remote_imports` provenance are separate from browsing.

## Goals / Non-Goals

**Goals:**

- Bring remote cards into geometric and keyboard parity with the local card
  rail without sharing local edit, pin, paste, clipboard, or drag behavior.
- Preserve the app's privacy boundary: source-app display names and icons are
  disclosed only for a card the user is viewing through a separate bounded
  request, or as part of a user-initiated import; never in bulk browse pages
  or thumbnail responses.
- Keep source attribution tied to the `(peer, imported snapshot)` provenance
  so deduplication cannot overwrite local metadata or misattribute another
  peer's import.
- Display the original application's name and icon in visible remote previews
  and in the matching peer-bound imported collection without exposing the
  host's local icon reference or filesystem layout.

**Non-Goals:**

- Returning source-app names or icon bytes in history-list or image-thumbnail
  responses.
- Showing peer-specific application attribution in general history or in a
  different peer's collection.
- Changing clipboard contents, paste behavior, import deduplication, thumbnail
  behavior, or drag payloads.

## Decisions

### Fixed geometry and footer

Keep `RemotePreviewCard` as a dedicated read-only renderer and size it to the
same fixed square card token/footprint used by local history cards (currently
240 × 240 px in the desktop rail). Constrain preview content within that
footprint; anchor the elapsed-time/menu footer to the bottom independently of
row type and preview state. Reusing `HistoryCard` itself was rejected because
it would couple remote rows to local edit, organization and drag contracts.

### Rail-owned selected state and keyboard handling

`RemoteHistoryRail` owns one selected opaque `remote_entry_id`, updates it on a
non-interactive card click, and renders the same blue selected-border cue and
accessible selected state as local cards. Extend the pure horizontal
selection helper to support both numeric local IDs and opaque remote string
IDs while preserving existing number behavior. The remote listener is scoped
to the rail: it prevents native horizontal scrolling only when it handles
ArrowLeft/ArrowRight, preserves typing and focused-control behavior, does not
wrap at list boundaries, and scrolls the newly selected card into view.

### Additive capability and on-demand preview presentation

Advertise `source_app_presentation` through a new additive `caps_extra_v2`
mDNS TXT field. Keep `capability=pairing` and the existing `caps_extra`
tokens unchanged: deployed clients reject unknown tokens in `caps_extra`, so
putting a new token there would make them reject the peer rather than safely
ignore the optional feature. New clients combine recognized tokens from both
fields; old clients ignore the previously unknown TXT key and continue using
the existing pairing/image capabilities. New parsers retain the current
unsupported-unknown-token policy within fields they understand.

A client calls a new authenticated source-presentation operation only for a
card visible in the selected peer's rail. The request contains the peer-scoped
opaque remote entry ID, never an app identifier or asset reference. The host
revalidates the mTLS pin, current trusted/active state, advertised
capability, row eligibility and local icon namespace on every request.

The response carries an optional normalized display name (trimmed, at most
128 Unicode scalar values, no control characters) and optional PNG bytes. It
never carries the host's icon reference/path, bundle ID, raw source ID,
content hash, clipboard payload or unrelated app metadata. Validate PNG bytes
against the existing application-icon limits (512 KiB and 256 × 256 px);
encode them as base64 in a response capped at 720 KiB. Invalid or missing
icons are omitted without failing the name lookup. A per-peer gate allows at
most two concurrent source-presentation requests on each client and host.
Older peers that do not advertise the capability receive no request and use
the generic application fallback.

The host resolves this route only from source-app metadata attached to the
listed entry itself. It must not borrow a name or icon from a `remote_imports`
row, because that attribution belongs to another peer's import provenance and
must not be relayed as though it were a local capture.

The lookup is independent of history-page browsing and image-thumbnail
fetching. It does not create local entries or persist preview-only icon bytes.
Keep only bounded in-memory peer/entry presentation state while the remote
rail is open; ignore stale peer/card responses and release object URLs on
replacement, peer switch and component destruction. This deliberately
discloses the source application only for visible rows in the selected,
trusted peer's rail, avoiding bulk disclosure of every app in remote history.
Keep at most the current combined remote page's 50 presentation results per
peer; evict results when rows leave that window or the peer is closed.

### Explicit-import metadata and per-provenance storage

Successful explicit text/image fetch responses add optional normalized source
application name and optional base64 PNG icon fields. Older peers or entries
without metadata remain importable. Import occurs only after the user
activates Importar; browsing and thumbnail messages do not contain either
field. Use the same name and PNG validation limits as the on-demand route.
Never transfer an icon reference/path, bundle ID or raw source identifier.

Store the name on the corresponding `remote_imports` provenance row through
an additive nullable SQLite migration, and store the icon as a locally
generated `application-icons/<content-digest>.png` asset with a separate
nullable local reference on that provenance row. Validate and stage icon
bytes before committing provenance. Commit/rollback handling must preserve
reused/shared icons and remove only a newly written, unreferenced staged icon
on failure. Never derive a local path from a remote name; never accept a
remote path or reference. Do not write either field into
`clipboard_entries.source_app*`: hash deduplication can reuse a locally
captured entry, and one entry can have different provenance from multiple
peers. Re-importing the same provenance may fill source-app fields that are
still `NULL` on an older row, but must not overwrite fields already recorded
for that provenance. A collection-aware projection resolves the name and
local icon reference only for the peer bound to the collection currently
being viewed.

### Application presentation in remote and imported cards

In remote preview cards, render the returned source-app icon and bounded,
ellipsized application name when available. Keep both within the fixed card
geometry; use a generic local app icon and an honest unknown-application
label when either value is absent. The remote icon is displayed from
validated in-memory bytes and is never sent to the local-path resolver.

In the peer-bound imported collection only, render the locally persisted
source-app icon and bounded name from that peer's provenance. Reuse the
existing safe local application-icon resolver with the locally generated
reference. If no valid icon was imported, use a deterministic local static
import marker; if no valid name exists, use an unknown-application label.
Other history views keep their existing source-app icon/accessible-label
behavior and never use peer-specific provenance.

## Risks / Trade-offs

- **A peer can send an excessively long or control-character source name** →
  Validate and bound the value at the host, client and persistence boundary;
  render it as escaped text and never place it in logs.
- **A peer can send malformed or oversized icon bytes** → Enforce the wire
  envelope before decoding; validate PNG signature, decode success,
  dimensions and byte size on host and client. Drop only the icon when
  invalid; do not fail an otherwise valid preview or content import.
- **Visible previews disclose source-app usage** → Gate the endpoint behind
  the additive `caps_extra_v2` capability plus mTLS trusted/active checks,
  request it only for visible cards, keep it out of bulk list/thumbnail
  responses and do not persist preview-only metadata.
- **A peer changes state or a card while a request is in flight** → Revalidate
  authorization and entry eligibility on each host request; scope frontend
  responses to peer and remote entry IDs and discard stale results.
- **A deduplicated entry has provenance from several peers** → Store name and
  icon reference per provenance row and project them with the active
  peer-bound collection, never by choosing arbitrary metadata from the shared
  entry.
- **Older peers do not support the new optional data** → Keep fields optional
  and existing import behavior intact; do not call the new preview route
  unless the peer advertises its capability, and render generic app/import
  markers plus honest unknown-source fallbacks.
- **A failed import leaves an unreferenced icon asset** → Stage writes,
  content-address them for safe dedupe, and release only newly written assets
  that have no committed provenance reference; never delete a reused/shared
  icon.
- **Fixed cards truncate unusually large previews or metadata labels** → Keep
  the dimensions stable, clamp/ellipsis card content, and expose full safe
  labels to assistive technology without changing persisted data.

## Migration Plan

Add nullable source-application display-name and local icon-reference columns
to `remote_imports` in a new reversible migration. Existing rows remain
`NULL`; the migration down removes only these columns. No icon bytes are stored
in SQLite. Ship the additive capability, optional fetch fields and fallback
together. Older peers remain usable for history/import but show generic app
presentation; old parsers ignore `caps_extra_v2` and retain existing known
capabilities. Rollback leaves existing import provenance, clipboard entries
and local capture metadata intact after reverting the migration; migration
rollback does not destructively clean up content-addressed icon files.
