## MODIFIED Requirements

### Requirement: Backward-compatible clipboard image asset bridge

ClipVault SHALL serve every persisted clipboard image asset through
the same Tauri command, the same blob URL lifecycle and the same
typed rejection surface, regardless of the PNG `(ColorType,
BitDepth)` combination the asset was originally written with. The
asset store's decoder MUST accept every combination the PNG
specification considers legal and normalise the frame buffer to an
8-bit RGBA layout the paste pipeline can hand to the clipboard
adapter without inspecting pixels.

#### Scenario: Legacy palette (indexed) PNG loads after a recompile

- **WHEN** a previously saved image was written as a PNG whose
  `ColorType` is `Indexed` (1, 2, 4 or 8 bits per index)
- **AND** the row, the asset file at `<data_dir>/assets/clipboard/`,
  and the SQLite database all remain on disk
- **THEN** opening the desktop and remounting the rail renders the
  thumbnail through the `clipvault_clipboard_asset` bridge without
  the card transitioning to the "Imagen no disponible" fallback

#### Scenario: Legacy grayscale PNG loads after a recompile

- **WHEN** a previously saved image was written as a PNG whose
  `ColorType` is `Grayscale` (1, 2, 4, 8 or 16 bits per sample)
- **THEN** opening the desktop and remounting the rail renders the
  thumbnail without falling back to the error surface

#### Scenario: Legacy grayscale + alpha PNG loads after a recompile

- **WHEN** a previously saved image was written as a PNG whose
  `ColorType` is `GrayscaleAlpha` (8 or 16 bits per sample)
- **THEN** opening the desktop and remounting the rail renders the
  thumbnail with the alpha channel preserved through the bridge

#### Scenario: Legacy RGB / RGBA at 16-bit depth loads after a recompile

- **WHEN** a previously saved image was written as a PNG whose
  `BitDepth` is 16 and `ColorType` is `Rgb` or `Rgba`
- **THEN** opening the desktop and remounting the rail renders the
  thumbnail; the decoder downsamples the 16-bit samples to 8-bit
  RGBA without the bridge requiring the original 16-bit payload

#### Scenario: Oversized PNG is rejected without serving bytes

- **WHEN** the on-disk asset exceeds `MAX_CLIPBOARD_ASSET_BYTES`
- **THEN** the bridge rejects with the stable `too_large` kind
- **AND** the diagnostic helper reports `TooLarge { size }` with
  the metadata-only byte count

#### Scenario: Corrupt PNG is rejected without serving bytes

- **WHEN** the on-disk asset has a valid PNG signature but a
  truncated or otherwise undecodable body
- **THEN** the bridge rejects with the stable `not_png` kind
- **AND** the diagnostic helper reports `InvalidPng { color_type,
  bit_depth }` so the user can confirm the format mismatch

### Requirement: Metadata-only clipboard image asset diagnostic

ClipVault SHALL expose a typed, metadata-only diagnostic that
distinguishes the failure modes the clipboard asset bridge
collapses today. The diagnostic MUST carry enough information for
the frontend to render a stable fallback copy without widening the
privacy surface.

#### Scenario: Diagnostic enumerates every failure mode

- **WHEN** the diagnostic helper runs against an `asset_ref`
- **THEN** it returns exactly one of: `loaded`, `invalid_reference`,
  `not_found`, `wrong_data_dir`, `wrong_namespace`, `too_large`,
  `invalid_png`, `invalid_dimensions`, `io_error`
- **AND** each variant exposes a stable snake_case `kind_str()`
  identifier the frontend can match against documented copy

#### Scenario: Diagnostic distinguishes missing file from wrong data_dir

- **WHEN** the `asset_ref` is well-formed but the namespace
  directory `<data_dir>/assets/clipboard/` does not exist
- **THEN** the diagnostic returns `wrong_data_dir`
- **AND** the surface signals that the running process points at a
  different data directory than the one the rows were persisted
  against

#### Scenario: Diagnostic distinguishes namespace violation from invalid reference

- **WHEN** the `asset_ref` resolves to a symlink that escapes the
  allowed root
- **THEN** the diagnostic returns `wrong_namespace`
- **AND** the frontend can pick a distinct fallback copy without
  parsing the rejection reason

#### Scenario: Diagnostic never carries bytes, hashes or absolute paths

- **WHEN** the diagnostic helper reports a failure
- **THEN** the returned `AssetDiagnostic` does NOT contain the
  payload bytes, the asset reference, the absolute path of the
  data directory or the content hash of the PNG
- **AND** the metadata it carries (the `kind`, the colour type
  label, the bit depth label, the dimensions or the byte count)
  is sufficient to drive the matching fallback copy

### Requirement: Safe asset collector on a failing reference query

ClipVault SHALL reclaim clipboard image and rich-text assets
whose name is no longer referenced by any row in
`clipboard_entries`, but SHALL never delete an asset when the live
reference set cannot be derived from SQLite. A SQLite failure
during the reference query (lock contention, WAL read failure,
busy connection, missing or un-prepared table, …) MUST be
distinguished from a successful query that returns the empty set;
the collector MUST skip the namespace entirely on failure and
MUST proceed with the legitimate reclaim path on `Ok(empty_set)`.

#### Scenario: Failing reference query preserves every referenced PNG

- **WHEN** the image-namespace reference query
  (`referenced_asset_refs`) returns an error
- **THEN** the asset collector MUST NOT call
  `ClipboardAssetStore::collect_unreferenced` for that namespace
- **AND** the diagnostic reports `image_reference_query_failed =
  true`, `image_collection_skipped = true` and
  `assets_removed_count = 0`
- **AND** every file under `<data_dir>/assets/clipboard/` remains
  on disk after the pass

#### Scenario: Failing reference query preserves every rich-text asset

- **WHEN** the rich-text-namespace reference query
  (`referenced_rich_asset_refs`) returns an error
- **THEN** the rich-text collector MUST NOT call
  `RichTextAssetStore::collect_unreferenced` for that namespace
- **AND** the diagnostic reports `rich_reference_query_failed =
  true`, `rich_collection_skipped = true` and
  `assets_removed_count = 0`
- **AND** every file under `<data_dir>/assets/rich-text/` remains
  on disk after the pass

#### Scenario: Successful query with references keeps every file

- **WHEN** the reference query succeeds and returns a non-empty
  set that contains the name of a file in the namespace
- **THEN** the collector MUST NOT delete that file
- **AND** the diagnostic reports `*_reference_query_succeeded =
  true`, `*_collection_skipped = false` and the actual number of
  orphans removed

#### Scenario: Successful query with no references removes only orphans

- **WHEN** the reference query succeeds and returns the empty set
- **THEN** the collector MAY delete files in the namespace that no
  row references (for example stale `.tmp` files left over from an
  interrupted capture)
- **AND** the diagnostic reports
  `*_reference_query_succeeded = true`,
  `*_collection_skipped = false` and the actual number of orphans
  removed

#### Scenario: Shared asset survives every pass

- **WHEN** two or more rows in `clipboard_entries` share the same
  `asset_ref` (a re-import or rollback can produce this)
- **THEN** a successful reference query that lists the shared
  reference MUST NOT cause the collector to delete the file
- **AND** a failing reference query MUST also preserve the file
  because the collector cannot prove the other row does not need
  it

#### Scenario: Apply-retention preserves assets when the query fails

- **WHEN** `apply_retention` runs (a path used by both the
  startup and the shutdown retention pass) and the reference
  query fails
- **THEN** the function returns the documented `RetentionOutcome`
  without surfacing the asset-collector failure as an error
- **AND** every asset referenced by `clipboard_entries` remains
  on disk after the pass

#### Scenario: Fresh capture after a failed collection pass

- **WHEN** a collection pass has been skipped because the
  reference query failed
- **THEN** a brand-new image capture that follows MUST succeed
  normally: the asset is written, the row is inserted and the
  card renders through the regular pipeline

#### Scenario: Asset collection diagnostics stay metadata-only

- **WHEN** the collector reports its outcome through the
  `AssetCollectionOutcome` struct
- **THEN** the serialised payload does NOT contain absolute
  paths, `asset_ref` values, byte counts, content hashes,
  clipboard snippets or any other field that could leak the
  payload content
- **AND** the documented snake_case fields
  (`image_reference_query_succeeded`,
  `image_reference_query_failed`, `image_collection_skipped`,
  `rich_reference_query_succeeded`,
  `rich_reference_query_failed`, `rich_collection_skipped`,
  `assets_removed_count`) are sufficient to drive logs and to
  correlate a missing asset with a SQLite failure

## ADDED Requirements

### Requirement: Bootstrap requires explicit PlatformAdapters

`AppBootstrap` MUST refuse to construct an `AppContext` unless the
caller injects explicit `PlatformAdapters` (or opts into host
detection through `with_default_platform_adapters`). The previous
behaviour, which silently fell back to `DefaultPlatform::detect()`
when no adapters were supplied, allowed tests pointing the
SQLite file at a `tempfile::TempDir` to drive destructive
operations against the developer's real `~/.clipvault/assets`.

The two safe paths are:

1. **Tests** MUST inject `PlatformAdapters` whose `PlatformInfo`
   resolves `home_dir` and `data_dir` to a path inside a
   `tempfile::TempDir`. The shared helper
   `clipvault_core::test_support::IsolatedTestHarness` is the
   canonical builder.
2. **Production shells** MUST call
   `AppBootstrap::with_default_platform_adapters()` to opt into
   the host-detected behaviour explicitly. The shell's existing
   call to `with_platform_adapters(...)` with a hand-built
   `PlatformAdapters` continues to work because the requirement
   only forbids the silent fallback.

#### Scenario: bootstrap_at without PlatformAdapters fails fast

- **WHEN** `AppBootstrap::bootstrap_at` (or `bootstrap_default`,
  or `bootstrap_with_database`) is called without a prior call to
  `with_platform_adapters` or `with_default_platform_adapters`
- **THEN** the call returns `BootstrapError::MissingPlatformAdapters`
- **AND** no `AppContext` is constructed, no migrations run
  against the SQLite file and the asset collector cannot be
  reached

#### Scenario: production shell still builds against the host data directory

- **WHEN** `with_default_platform_adapters()` is called before
  `bootstrap_default`
- **THEN** the bootstrap detects the host platform through
  `DefaultPlatform::detect()` and the asset stores resolve to
  `<home_dir>/.clipvault`
- **AND** the existing `run_retention` startup / shutdown passes
  keep working unchanged

### Requirement: Isolated test harness for asset operations

Tests that exercise any code path which can reach the asset
collector (`delete_entry`, `clear_non_favorites`,
`clear_unorganized_history`, `apply_retention`,
`collect_unreferenced_assets`, the capture pipeline when an
image lands, …) MUST build their `AppContext` through
`clipvault_core::test_support::IsolatedTestHarness` (or a
helper that wraps `build_isolated_adapters`). The harness
guarantees, by construction, that the asset stores resolve to a
fresh `tempfile::TempDir` that no other process owns.

#### Scenario: harness data_dir lives inside the tempdir

- **WHEN** a test creates an `IsolatedTestHarness`
- **THEN** `harness.adapters.info().home_dir`,
  `harness.adapters.info().data_dir`,
  `harness.asset_store.root()` and
  `harness.rich_asset_store.root()` all live inside
  `harness.dir.path()`
- **AND** none of them resolves to the host's `~/.clipvault`

#### Scenario: delete_entry does not touch external directories

- **WHEN** a sentinel PNG is written to a sibling tempdir
  standing in for `~/.clipvault/assets/clipboard/`
- **AND** a test inside an `IsolatedTestHarness` calls
  `delete_entry` (or any other destructive op that triggers the
  asset collector)
- **THEN** the sentinel PNG remains on disk after the call
- **AND** the harness's own asset root never contains files
  outside the harness namespace

#### Scenario: clear_non_favorites does not touch external directories

- **WHEN** a sentinel PNG is written to a sibling tempdir
- **AND** a test inside an `IsolatedTestHarness` calls
  `clear_non_favorites(confirm: true)`
- **THEN** the sentinel PNG remains on disk after the call

#### Scenario: apply_retention does not touch external directories

- **WHEN** a sentinel PNG is written to a sibling tempdir
- **AND** a test inside an `IsolatedTestHarness` calls
  `apply_retention` with a finite cutoff
- **THEN** the sentinel PNG remains on disk after the call

#### Scenario: collector can reap a sentinel inside the tempdir

- **WHEN** a sentinel PNG is written inside the harness's own
  asset root
- **AND** SQLite has zero live references
- **THEN** `collect_unreferenced_assets` reaps the orphan
- **AND** the `AssetCollectionOutcome.assets_removed_count` is
  non-zero

#### Scenario: sentinel outside the context is never modified

- **WHEN** a sentinel PNG is written in a directory that shares a
  parent with the harness's data_dir but is not inside it
- **AND** `collect_unreferenced_assets` runs against the harness
- **THEN** the sentinel PNG remains on disk
- **AND** the collector's `assets_removed_count` is `0`

#### Scenario: temp database with zero references never deletes real assets

- **WHEN** a test inside an `IsolatedTestHarness` exercises
  `delete_entry`, `clear_non_favorites`, `apply_retention`,
  `clear_unorganized_history` and `collect_unreferenced_assets`
  against a database that has no rows
- **AND** a sentinel PNG stands in for `~/.clipvault/assets/clipboard/`
  in a sibling tempdir
- **THEN** the sentinel PNG remains on disk after every pass
- **AND** the harness's asset root is empty (or only contains
  its own scratch files)

#### Scenario: audit asserts no test root points at the real ~/.clipvault

- **WHEN** a fresh `IsolatedTestHarness` is built
- **THEN** `harness.adapters.info().data_dir` MUST NOT equal
  `${HOME}/.clipvault`
- **AND** `harness.adapters.info().home_dir` MUST NOT live under
  `${HOME}` (except by coincidence, when `${HOME}` is itself a
  tempdir, which the test explicitly excludes)
- **AND** `harness.adapters.info().data_dir` MUST live inside
  `harness.dir.path()`
