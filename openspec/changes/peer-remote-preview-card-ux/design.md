## Context

See `proposal.md` for the user-visible problems. `RemotePreviewCard` is a
read-only component separate from `HistoryCard`; today it has content-driven
height and no rail-owned selection. The local rail already defines fixed
square geometry and tested horizontal-selection semantics. Explicit text/image
fetch paths and `remote_imports` provenance are separate from browsing.

## Goals / Non-Goals

**Goals:**

- Bring remote cards into geometric and keyboard parity with the local card
  rail without sharing local edit, pin, paste, clipboard, or drag behavior.
- Preserve the app's privacy boundary: a trusted peer that advertises the
  capability may receive only the bounded source-app name inline in browse
  rows; source-app icons travel only as part of a user-initiated import and
  never in browse or thumbnail responses.
- Keep source attribution tied to the `(peer, imported snapshot)` provenance
  so deduplication cannot overwrite local metadata or misattribute another
  peer's import. In general history, choose the most recently imported
  provenance deterministically while never exposing which peer supplied it.
- Display the original application's name (not its icon) in remote previews,
  and expose imported names as the icon's accessible label/tooltip while
  rendering the locally stored icon in general history and the matching
  peer-bound collection, without exposing the host's icon reference or
  filesystem layout.

**Non-Goals:**

- Returning source-app icon bytes/references in browse rows or any source-app
  metadata in image-thumbnail responses.
- Showing a peer identifier or arbitrary peer provenance in general history;
  that view may show only the deterministic latest imported app presentation.
- Showing an imported app name as visible text beside its icon in cards.
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

### Additive capability and low-latency preview source names

Advertise `source_app_presentation` through a new additive `caps_extra_v2`
mDNS TXT field. Keep `capability=pairing` and the existing `caps_extra`
tokens unchanged: deployed clients reject unknown tokens in `caps_extra`, so
putting a new token there would make them reject the peer rather than safely
ignore the optional feature. New clients combine recognized tokens from both
fields; old clients ignore the previously unknown TXT key and continue using
the existing pairing/image capabilities. New parsers retain the current
unsupported-unknown-token policy within fields they understand.

Text and image browse rows carry an optional normalized source-app display
name (trimmed, at most 128 Unicode scalar values, no control characters).
This is a small string in the page response already needed to render the
visible rail; the client makes no second TLS request per card. New fields are
optional/defaulted so older peers continue to browse, and the client displays
the name only when the peer advertises `source_app_presentation`. The host
never includes icon bytes, references/paths, bundle IDs, raw source IDs,
content hashes, clipboard payloads or unrelated app metadata in browse rows.
Image-thumbnail responses remain unchanged and contain no source-app data.

The host resolves the name only from source-app metadata attached to the
listed entry itself. It must not borrow a name from a `remote_imports` row,
because that attribution belongs to another peer's import provenance and
must not be relayed as though it were a local capture. Missing/invalid names
remain `None`; the client renders a truthful fallback without delaying the
rest of the card.

The text and image browse endpoints remain parallel and independently
paginated, but the initial/retry page does not wait for the slower endpoint
before showing useful content. The first successful non-empty response paints
provisional rows immediately; cursors and buffers remain uncommitted until
both endpoints settle, at which point the existing merge helper replaces the
provisional rows with the authoritative newest-first, deduplicated page. The
pager remains disabled while that merge is pending. If one endpoint fails,
rows from the successful endpoint remain visible alongside the error and retry
action. This removes the cross-stream `Promise.all` first-paint barrier without
mixing cursor state or allowing a late response from a stale peer/page to
overwrite the current rail.

No preview-only icon bytes are requested, transferred, decoded or persisted.
The icon transfer is retained only in the explicit import response, where
the user has chosen to import the capture. This removes the per-card network
round trip and up-to-512-KiB icon payload from preview loading.

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
for that provenance. A scope-aware projection resolves the name and local icon
reference for the exact peer when a peer-bound collection is viewed. For
general history, it selects the most recently imported provenance for each
visible entry, using `imported_at DESC` and a stable tie-breaker, without
returning a peer ID. A different peer-bound collection never borrows another
peer's attribution.

### Application presentation in remote and imported cards

In remote preview cards, render only the bounded, ellipsized source-app name
when available. Do not show or fetch a source-app icon; retain the generic
static application/import marker. Keep the name within fixed card geometry
and use an honest unknown-application label when it is absent.

In a peer-bound imported collection, resolve the icon/name from that
collection's peer provenance. In general history, resolve the most recent
import provenance for each entry, without revealing the peer. In either view,
render the locally persisted source-app icon and expose the bounded name only
as the icon's accessible label/tooltip, matching ordinary history cards. Reuse
the existing safe local application-icon resolver with the locally generated
reference. If no valid icon was imported, use a deterministic local static
import marker; if no valid name exists, use an unknown-application label. A
different peer-bound collection never borrows the attribution.

## Risks / Trade-offs

- **A peer can send an excessively long or control-character source name** →
  Validate and bound the value at the host, client and persistence boundary;
  render it as escaped text and never place it in logs.
- **A peer can send malformed or oversized icon bytes** → Enforce the wire
  envelope before decoding on explicit import; validate PNG signature,
  decode success, dimensions and byte size on host and client. Icon validity
  does not affect a valid imported name or image.
- **Browse rows disclose source-app usage** → Send only the validated app
  display name to an already trusted/active peer that can browse the history;
  never include it in thumbnails or for entries without their own metadata.
  Legacy clients ignore the optional field; new clients display it only when
  the additive capability is present.
- **A deduplicated entry has provenance from several peers** → Store name and
  icon reference per provenance row. Peer-bound collections select their own
  row; general history selects the latest row deterministically without
  exposing its peer ID.
- **Older peers do not support the new optional data** → Keep browse fields
  optional/defaulted and explicit-import fields optional; render generic
  app/import markers plus honest unknown-source fallbacks.
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
