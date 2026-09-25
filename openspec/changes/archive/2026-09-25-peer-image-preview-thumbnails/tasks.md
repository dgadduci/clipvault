# Tasks: peer-image-preview-thumbnails

## 1. Baseline and contracts

- [x] 1.1 Read the archived `peer-image-import` artifacts and current
  `peer-text-history-browser` / `peer-image-import` base specs; preserve the
  metadata-only page and original-image import contracts.
- [x] 1.2 Inspect current discovery capability parsing, image TLS messages,
  asset validation/PNG codecs, Tauri bridge and remote card lifecycle before
  choosing adapters.
- [x] 1.3 Confirm the current branch changes and generated files are
  user-owned; keep implementation limited to this OpenSpec change.

## 2. Capability and transport

- [x] 2.1 Add `image_preview_thumbnail` as an additive `caps_extra` token while
  preserving `capability=pairing` and compatibility with older peers.
- [x] 2.2 Add versioned thumbnail request, success and safe-unavailable
  messages/handler without changing `FetchImage` import semantics.
- [x] 2.3 Bound the PNG body to 384 KiB and serialized envelope to at least
  544 KiB; test base64/framing at the cap and reject oversized messages before
  retaining the body.
- [x] 2.4 Enforce mTLS pin, trusted/active state, both image capabilities and
  current row eligibility on every thumbnail request.

## 3. Core thumbnail generation

- [x] 3.1 Add a core service/adapter path that resolves the opaque remote entry
  ID internally, revalidates the PNG through `ClipboardAssetStore`, and never
  accepts or returns `asset_ref`, path or hash.
- [x] 3.2 Generate a deterministic in-memory PNG thumbnail preserving aspect
  ratio/transparency with a maximum 256 px longest side; reuse existing codec
  dependencies and do not persist the derivative.
- [x] 3.3 Enforce output-byte and per-peer decode/resize concurrency limits;
  map stale, invalid, oversized, unsupported and busy outcomes to typed safe
  results.
- [x] 3.4 Keep logging metadata-only and ensure thumbnail handling does not
  mutate entries, collections, provenance, clipboard, paste or local assets.

## 4. Tauri and frontend

- [x] 4.1 Add a thin Tauri command and frontend bridge/type for fetching a
  thumbnail by peer and opaque remote entry ID.
- [x] 4.2 Request thumbnails only when a supported image card intersects the
  remote rail viewport, with at most two client requests in flight.
- [x] 4.3 Keep the common placeholder for loading, unsupported capability,
  busy, failure and invalid PNG; do not raise a global history error.
- [x] 4.4 Ignore stale results after peer/page/card changes and release
  Object URLs on replacement/unmount; do not use remote URLs or the local
  asset resolver.
- [x] 4.5 Preserve the existing explicit Importar flow so it fetches and
  persists the original PNG, never the thumbnail.

## 5. Regression coverage

- [x] 5.1 Add core tests for downscale dimensions/aspect ratio/transparency,
  missing or invalid source assets, body/envelope limits, capability and
  authorization failures, and bounded work.
- [x] 5.2 Add transport tests for versioned message round-trips, maximum
  envelope sizing and typed failure mapping.
- [x] 5.3 Add frontend tests for viewport-gated requests, placeholder/fallback,
  no requests to legacy peers, concurrency cap, stale response and Object URL
  cleanup, and independence from Importar.
- [x] 5.4 Re-run image import, peer history pagination, peer capability,
  drag-and-drop and persisted-asset regressions.
- [x] 5.5 Audit logs, events, SQLite, filesystem and payloads for absence of
  thumbnail persistence, original-image bytes outside Importar, paths, hashes
  and asset references; tests must use temporary asset roots.

## 6. Verification

- [x] 6.1 Run `cargo fmt --all -- --check` and `git diff --check`.
- [x] 6.2 Run relevant Rust tests for `clipvault-core`, `clipvault-platform`,
  `clipvault-db` (if touched) and `clipvault-app` (if touched); report existing
  baseline failures separately.
- [x] 6.3 Run frontend `npm run check`, `npm run build` and focused thumbnail,
  remote history and drag-and-drop tests.
- [x] 6.4 Run `openspec validate peer-image-preview-thumbnails --strict --type
  change` and review the complete diff.
- [x] 6.5 Do not mark platform/manual checks complete by inference and do not
  sync, archive, commit or push as part of implementation unless separately
  requested.

## 7. Manual verification

- [x] 7.1 On macOS, Linux X11 and Linux Wayland builds, verify lazy thumbnail
  appearance, scroll behavior, and static placeholder fallback for an older
  peer or an unavailable/invalid thumbnail.
- [x] 7.2 Confirm a viewed thumbnail does not alter clipboard, paste, drag
  payload, local history or pre-existing assets, and that Importar still
  obtains the original image.
