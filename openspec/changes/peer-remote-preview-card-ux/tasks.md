## 1. Baseline and contracts

- [x] 1.1 Review the current remote/local card rails, peer discovery and
  transport handlers, app-icon asset reader/writer, import DTOs,
  `remote_imports` repository and source-app presentation; record touched
  files and preserve all pre-existing work in the tree.
- [x] 1.2 Preserve the remote-card read-only contract and protected local
  drag-and-drop controller; identify the existing DnD and peer-preview
  regressions that must pass before delivery.

## 2. Remote preview card layout and navigation

- [x] 2.1 Give remote text and image cards the same fixed square dimensions as
  local history cards; test that text, thumbnail, placeholder, source-app
  presentation and import status do not change card size.
- [x] 2.2 Anchor elapsed-time and menu metadata to the bottom of every remote
  card; test consistent footer placement for text, thumbnail, placeholder,
  source-app loading/error and import-result states.
- [x] 2.3 Add one rail-owned selected remote-entry ID, click selection, an
  accessible selected state and the blue border accent; test single selection
  and ensure menu/import controls do not select a card.
- [x] 2.4 Handle `ArrowLeft` / `ArrowRight` as adjacent-card navigation for
  opaque remote IDs, scroll the selected card into view, prevent native rail
  scrolling when handled, and preserve no-wrap boundary and interactive
  control behavior; keep existing numeric rail tests passing if the shared
  helper is generalized.
- [x] 2.5 Paint the first successful non-empty text/image browse response
  before the other stream settles; keep cursor/buffer state uncommitted and
  pagination disabled until the authoritative merged page is ready. Preserve
  successful rows if the other endpoint fails and reject stale generations.

## 3. Capability and bounded source-app presentation for previews

- [x] 3.1 Advertise `source_app_presentation` through a new additive
  `caps_extra_v2` TXT field while preserving `capability=pairing` and existing
  `caps_extra` tokens; test new-client union/refresh, legacy-client tolerance
  of the unknown TXT key, and unknown-token rejection by new parsers.
- [x] 3.2 Retain versioned `FetchSourceAppPresentation` success/unavailable
  wire messages for compatibility with already-shipped clients; the current
  preview UI no longer calls this route. Test round trips and the full 512 KiB
  icon envelope within the 720 KiB response cap.
- [x] 3.3 Implement client and host core adapters with a maximum of two
  concurrent requests per peer on each side; revalidate mTLS pin, trusted and
  active state, capability, entry eligibility and local icon availability on
  every host request.
- [x] 3.4 Normalize names (trimmed, at most 128 Unicode scalar values, no
  control characters) and validate PNG signature/decode, maximum 512 KiB and
  maximum 256 × 256 px; invalid/missing icon must degrade to a generic icon
  without failing an otherwise valid name response.
- [x] 3.5 Expose the bounded optional source-app name in typed text/image
  browse rows; display it only when the peer advertises the capability.
  Previews do not make a second per-card request and never receive icon bytes.
- [x] 3.6 Keep preview name state scoped to its row; do not allocate object
  URLs or fetch icons. Preserve the fixed layout and an honest unknown-name
  fallback for legacy peers or missing metadata.
- [x] 3.7 Test capability absence, missing/invalid names, legacy-row decoding,
  no preview icon request/bytes, source-name presence in browse rows and
  source metadata redaction from image-thumbnail payloads.

## 4. Explicit import and peer-scoped local icon persistence

- [x] 4.1 Extend explicit text/image fetch responses with optional validated
  source-app name and icon fields only; preserve compatibility with older
  peers and prove the existing text/image body and envelope limits still
  accommodate the maximum icon response.
- [x] 4.2 Add a safe local application-icon writer that validates PNG bytes,
  generates a content-addressed name under `application-icons/`, writes
  atomically, and never accepts a peer-supplied path, filename or reference.
- [x] 4.3 Stage icon writes around the import transaction; on failure release
  only a newly written unreferenced icon, and preserve reused/shared icons.
  Test new, reused, rollback and concurrent/shared-reference behavior.
- [x] 4.4 Add nullable source-app name and local icon-reference columns to
  `remote_imports` in a reversible migration; test existing-row defaults,
  fresh migration and rollback without changing other columns or deleting
  icon files.
- [x] 4.5 Persist name and local icon reference on the matching per-peer
  provenance in text and image import transactions; do not modify
  `clipboard_entries.source_app*`. Test idempotence, deduplicated local entry
  preservation and distinct attribution for two peers sharing one entry.
- [x] 4.6 Extend the attribution projection so a peer-bound collection uses
  only that peer's provenance while general history uses the latest provenance
  without exposing peer identity; render icons through the safe local resolver
  and names as accessible labels/tooltips rather than visible card text.
- [x] 4.7 Test two-peer isolation, latest-provenance selection in general
  history, name/icon label behavior, missing/invalid fallbacks, legacy-peer
  imports, and unchanged clipboard, paste, drag payload and local metadata.

## 5. Cross-layer regression coverage

- [x] 5.1 Add focused Rust tests for discovery, wire limits/round trips,
  authentication and trust gates, PNG/name validation, local icon storage,
  provenance transactions and rollback.
- [x] 5.2 Add frontend tests for card geometry/selection/navigation/footer,
  inline browse-row names, absence of preview icon requests/rendering,
  capability fallback, imported-card attribution and first-successful-stream
  initial paint.
- [x] 5.3 Run existing remote history, image thumbnail, text/image import,
  source-app icon and migration regressions; report baseline failures
  separately and do not weaken existing contracts.

## 6. Verification and manual review

- [x] 6.1 Run `cargo fmt --all -- --check`, `git diff --check`, relevant Rust
  tests for core, database, platform and app crates, plus frontend check/build
  and focused remote/imported-card tests.
- [x] 6.2 Run protected drag-and-drop regressions; confirm remote cards remain
  read-only and payloads, clipboard and paste behavior are unchanged.
- [x] 6.3 Validate `peer-remote-preview-card-ux` with strict OpenSpec change
  validation and review the complete diff; verify no generated assets,
  remote paths, hashes or application identifiers are persisted or logged.
- [ ] 6.4 Manually compare local and remote card size, selection, keyboard
  navigation and footer placement on macOS, Linux X11 and Linux Wayland; check
  source-app names (without icons or extra loading delay) on text and image
  previews and the fallback with a peer lacking the capability. The user has
  approved the left/right navigation portion; recheck source-name rendering
  after this implementation change before closing the full matrix.
- [ ] 6.5 Manually import text and images from two peers; confirm each
  peer-bound collection shows its own icon with the name only as tooltip/tag,
  while general history shows the latest imported app presentation without a
  peer identifier. Confirm local metadata, clipboard, paste and drag payload
  remain unchanged.
