## Purpose

Definir los adaptadores de plataforma y el soporte inicial de macOS, Linux, X11, Wayland y tray/menu bar.

## Requirements

### Requirement: Platform adapter boundary

ClipVault SHALL abstract clipboard access, active-application detection, global hotkeys, paste simulation and tray/menu-bar integration behind platform adapters consumed by the Rust core.

#### Scenario: Core runs with a test platform

- **WHEN** core tests provide a fake platform adapter
- **THEN** they can validate capture and paste orchestration without a real desktop session

#### Scenario: Platform operation fails

- **WHEN** a platform API returns an error
- **THEN** the adapter returns a typed or actionable error to the core without terminating the application

### Requirement: macOS support

The MVP SHALL support clipboard capture, global hotkey, quick paste and tray/menu-bar operation on macOS using the platform-specific implementation.

#### Scenario: macOS daily workflow

- **WHEN** a user copies text, invokes the default hotkey and selects an entry
- **THEN** the entry can be searched and pasted back into the active macOS application

### Requirement: Linux display support

ClipVault SHALL detectar si Linux ejecuta X11 o Wayland y SHALL seleccionar
un adapter compatible sin asumir que una operación exclusiva de X11 funciona
en Wayland. Cuando una sesión Wayland exponga una ventana X11 mediante
XWayland y `DISPLAY` sea utilizable, ClipVault MAY usar el adapter EWMH para
esa ventana concreta; SHALL mantener una respuesta tipada de indisponibilidad
para aplicaciones Wayland nativas que no sean visibles por X11.

#### Scenario: Linux X11 session

- **WHEN** ClipVault starts in a Linux X11 session with a usable display
- **THEN** it uses the X11 active-application adapter when available
- **AND** the capability and diagnostic surfaces identify the X11 backend

#### Scenario: GNOME Wayland with an XWayland application

- **WHEN** ClipVault starts in a Wayland session with `DISPLAY` and the
  focused application exposes a real X11 window
- **THEN** it may resolve the active window through the existing EWMH probe
- **AND** it identifies the result as XWayland/X11 rather than native Wayland

#### Scenario: Native Wayland application

- **WHEN** the focused application is native Wayland and no compatible
  compositor protocol is implemented by ClipVault
- **THEN** active-application metadata is reported as unavailable
- **AND** ClipVault does not fabricate a name, icon or blacklist identifier
- **AND** local history and other supported capabilities remain usable

#### Scenario: XWayland is present but unusable

- **WHEN** `DISPLAY` is present but the X11 connection, active window or
  identifier cannot be obtained
- **THEN** ClipVault falls back to the existing unknown-source behavior
- **AND** it does not report a successful XWayland identification

### Requirement: Linux capability reporting

ClipVault SHALL report the active-application capability according to the
adapter that can actually answer on the current session. It SHALL distinguish
X11/XWayland best-effort support from native Wayland unavailability without
turning a structural limitation into a false permission recommendation.

#### Scenario: X11 metadata backend available

- **WHEN** the Linux X11 adapter connects and can query the active window
- **THEN** the diagnostics identify the X11 metadata path
- **AND** source-application metadata resolution may run

#### Scenario: Wayland metadata backend unavailable

- **WHEN** no XWayland window is available for the active application
- **THEN** the diagnostics identify the unavailable Wayland metadata path
- **AND** the frontend keeps the existing generic fallback behavior

#### Scenario: Metadata lookup fails

- **WHEN** a local metadata or icon lookup fails
- **THEN** the capture remains valid and the failure is non-fatal
- **AND** no clipboard content, absolute path or asset bytes appear in the
  error, logs or events

### Requirement: Linux application metadata provider

ClipVault SHALL resolve Linux source-application metadata behind the existing
`ApplicationMetadataProvider` boundary. Given the stable X11 identifier, the
provider SHALL search local freedesktop `.desktop` entries without executing
their commands, return a non-empty display name when one is available, and
optionally persist a controlled icon reference under the existing
`application-icons/` asset namespace.

#### Scenario: Resolve a known X11 application

- **WHEN** a permitted capture has a `WM_CLASS` identifier matching a valid
  local `.desktop` entry
- **THEN** the provider returns the application display name
- **AND** it returns a relative icon reference when a safe icon is available

#### Scenario: Match by startup class

- **WHEN** the identifier matches `StartupWMClass` or `X-GNOME-WMClass`
- **THEN** that entry wins over a lower-priority filename match
- **AND** matching is deterministic and case-insensitive

#### Scenario: Localized display name

- **WHEN** a matching entry has a locale-compatible `Name[locale]`
- **THEN** the provider prefers that value over the generic `Name`
- **AND** it never persists the complete `.desktop` file contents

#### Scenario: Name resolves but icon does not

- **WHEN** a matching `.desktop` entry has a valid name but its icon is absent,
  unsupported or unreadable
- **THEN** the provider returns the name without an icon reference
- **AND** the capture is stored successfully with the existing generic icon
  fallback

#### Scenario: Unknown application

- **WHEN** no unambiguous `.desktop` entry matches the active identifier
- **THEN** the provider returns no metadata or the existing generic fallback
- **AND** it does not use the window title as a stable identifier

### Requirement: Safe Linux application icon assets

Linux application icons SHALL use the existing controlled asset bridge. Any
persisted icon SHALL be written atomically under
`<data_dir>/assets/application-icons/`, validated before publication, and
exposed only through a relative reference accepted by the existing icon
validator.

#### Scenario: Persist a resolved icon

- **WHEN** the provider resolves a local supported icon for an application
- **THEN** it writes a complete validated asset before returning its reference
- **AND** the reference contains no absolute path or traversal component

#### Scenario: Icon write fails

- **WHEN** an icon cannot be read, converted, validated or atomically written
- **THEN** the provider keeps the display name when available
- **AND** the capture remains stored without a partial asset or partial row

#### Scenario: Existing icon survives re-enrichment

- **WHEN** a later metadata lookup cannot obtain an icon for an entry that
  already has a valid icon reference
- **THEN** the existing reference is not replaced with an empty value
- **AND** the existing icon remains loadable after restart

### Requirement: Linux source metadata persistence and backfill

Allowed Linux captures SHALL preserve the existing `source_app` identifier
for privacy matching and SHALL enrich `source_app_name` and
`source_app_icon_ref` through the existing best-effort enrichment path. A
bounded, idempotent backfill MAY process legacy rows that have an identifier
but incomplete metadata without changing their content or organization.

#### Scenario: New allowed capture

- **WHEN** an allowed X11 or XWayland capture has a resolvable source
- **THEN** the existing history row stores the identifier and available name
  and icon metadata
- **AND** the frontend can render it through the existing DTO and bridge

#### Scenario: Blacklisted source

- **WHEN** the active X11/XWayland identifier is blacklisted
- **THEN** the privacy gate rejects the capture before metadata or icon I/O
- **AND** no row, icon file or metadata is created for that capture

#### Scenario: Bounded legacy backfill

- **WHEN** the application starts with legacy rows containing `source_app` but
  incomplete source metadata
- **THEN** it processes at most the documented batch limit per run
- **AND** it is idempotent and preserves content, hashes, timestamps, assets,
  tags, collections and favorite state

#### Scenario: Native Wayland capture without an identifier

- **WHEN** a capture is permitted but native Wayland cannot provide a source
  identifier
- **THEN** it follows the existing unknown-source behavior
- **AND** no fabricated application metadata is persisted

### Requirement: Linux metadata privacy

Linux source-application resolution SHALL be metadata-only. It SHALL never
execute a desktop entry, inspect window contents, persist clipboard content in
diagnostics, or expose absolute filesystem paths through logs, events, DTOs or
frontend attributes.

#### Scenario: Safe metadata diagnostic

- **WHEN** a Linux metadata lookup succeeds or fails
- **THEN** diagnostics may expose only stable backend/category fields
- **AND** they contain no clipboard content, snippets, hashes, asset bytes,
  `.desktop` contents or absolute paths

#### Scenario: Platform guidance remains honest

- **WHEN** native Wayland metadata is unavailable
- **THEN** the UI describes it as an unsupported/ unavailable capability
- **AND** it does not present the condition as a user permission problem

### Requirement: Linux XDG data root traversal

The Linux metadata provider SHALL resolve icons and `.desktop` files from a
single deduped collection of XDG data roots built from `XDG_DATA_HOME` (or
`$HOME/.local/share` when unset), every entry in `XDG_DATA_DIRS`, the canonical
`/usr/local/share` and `/usr/share` fallbacks and — only when present on disk —
the optional Flatpak, Snap and NixOS export namespaces. The provider SHALL
never build `<root>/icons/icons/...` paths, SHALL canonicalise candidate
paths and refuse anything that escapes an allowed root, and SHALL honour the
canonical Ubuntu / Debian / Fedora / Arch / openSUSE / GNOME / KDE layout
(`<root>/icons/<theme>/<size>x<size>/apps/<name>.{png,svg}` and
`<root>/icons/<theme>/scalable/apps/<name>.{png,svg}`) alongside the legacy
`<root>/icons/<size>x<size>/apps/` and `<root>/pixmaps/` layouts.

#### Scenario: XDG_DATA_HOME explicit

- **WHEN** `XDG_DATA_HOME` is set to a custom path
- **THEN** the provider uses that path as the primary data root
- **AND** every entry in `XDG_DATA_DIRS` and the optional fallback namespaces
  are appended in deterministic order

#### Scenario: XDG_DATA_HOME unset

- **WHEN** `XDG_DATA_HOME` is unset
- **THEN** the provider falls back to `$HOME/.local/share`
- **AND** `/usr/local/share` and `/usr/share` are appended when
  `XDG_DATA_DIRS` is empty

#### Scenario: Optional fallback present

- **WHEN** an optional export namespace (Flatpak, Snap, NixOS) exists on disk
- **THEN** it is included after the standard XDG roots
- **AND** it never displaces any entry of `XDG_DATA_DIRS`

#### Scenario: No duplicated icons segment

- **WHEN** the provider walks the icon directories
- **THEN** no candidate path contains the `icons` segment more than once
- **AND** the resolver reaches the canonical
  `/usr/share/icons/hicolor/48x48/apps/<name>.png` layout

### Requirement: Linux icon source formats and ordering

The Linux metadata provider SHALL resolve `Icon=<name>` deterministically by
walking, in order, the PNG candidate, then the SVG candidate of every theme
directory, then the PNG and SVG candidate of every `pixmaps/` directory. PNG
SHOULD win over SVG when both exist for the same icon name. The size list
SHALL be deterministic and SHALL include `16, 22, 24, 32, 48, 64, 96, 128`
and `256`. Absolute `Icon=/path` values SHALL be canonicalised, validated
as regular files, required to live under an allowed root and rejected when
they escape (including symlinks that point outside the allowed roots).

#### Scenario: PNG is preferred over SVG

- **WHEN** both `<theme>/<size>x<size>/apps/<name>.png` and
  `<theme>/scalable/apps/<name>.svg` exist
- **THEN** the PNG wins
- **AND** the diagnostic records `icon_kind = "png"` and
  `rasterization_attempted = false`

#### Scenario: Absolute path outside allowed roots

- **WHEN** `Icon=/etc/passwd` or any path that escapes the allowed roots
- **THEN** the resolver rejects the path
- **AND** the diagnostic records `icon_failure_kind = "not_found"`

#### Scenario: Multiple icon sizes

- **WHEN** the same icon name lives under `16x16`, `22x22`, `24x24`,
  `32x32`, `48x48`, `64x64`, `96x96`, `128x128` and `256x256`
- **THEN** the resolver walks every size in the documented order
- **AND** the first PNG it finds is persisted

### Requirement: Safe Linux SVG rasterization

When only an SVG candidate exists for a Linux application icon, the Linux
metadata provider MAY rasterize it through a pure-Rust, default-features-off
`resvg` / `tiny-skia` pipeline. The rasterizer SHALL disable every external
resource resolver (no file paths, no network, no embedded raster images),
SHALL cap the byte length, source dimensions and target dimensions, SHALL
preserve aspect ratio and transparency, SHALL never invoke `convert`,
`ImageMagick`, `magick`, `gio` or any other helper process and SHALL
validate the resulting PNG before persistence. The output SHALL be persisted
under the existing `application-icons/` namespace as PNG; the SVG bytes
themselves SHALL NEVER be persisted.

#### Scenario: SVG-only icon resolves

- **WHEN** an `.svg` icon is the only candidate under `<theme>/scalable/apps/`
- **THEN** the provider rasterizes it to PNG and persists it under
  `application-icons/<safe-id>.png`
- **AND** the diagnostic records `icon_kind = "svg"`,
  `rasterization_attempted = true` and `rasterization_succeeded = true`

#### Scenario: Malformed SVG

- **WHEN** the SVG bytes fail to parse
- **THEN** the provider keeps the display name and skips the icon
- **AND** the diagnostic records `icon_failure_kind = "invalid_svg"`

#### Scenario: SVG above the size cap

- **WHEN** the SVG payload exceeds `MAX_SVG_BYTES` (4 MiB) or its declared
  dimensions exceed `MAX_SVG_SOURCE_DIM` (1024 × 1024)
- **THEN** the provider rejects the file without rasterizing it
- **AND** the diagnostic records `icon_failure_kind = "svg_rejected"`

#### Scenario: External resources are dropped

- **WHEN** an SVG references a local file via `xlink:href` or embeds a
  network resource
- **THEN** the rasterizer silently drops the reference
- **AND** the resulting PNG is still persisted without leaking the
  referenced payload

### Requirement: Linux icon diagnostics surface

The Linux metadata provider SHALL publish an `IconDiagnostics` snapshot that
distinguishes, without exposing paths or asset bytes: whether the `.desktop`
declared an `Icon=` key, the source format the resolver identified
(`png`, `svg`, `pixmap`, `unknown`), whether a path was resolved, whether a
rasterization was attempted and whether it succeeded, whether the PNG was
validated, whether the asset was persisted, the byte length, the dimensions
and a typed failure category (`not_declared`, `not_found`, `out_of_roots`,
`invalid_png`, `invalid_svg`, `svg_rejected`, `rasterization_failed`,
`write_error`, `none`).

#### Scenario: Successful PNG icon

- **WHEN** a PNG icon resolves and persists
- **THEN** the snapshot reports `kind = "png"`, `declared = true`,
  `resolved = true`, `rasterization_attempted = false`,
  `rasterization_succeeded = false`, `png_validated = true`,
  `persisted = true`, `bytes > 0`, `dimensions = (w, h)` and
  `failure_kind = "none"`

#### Scenario: Successful SVG rasterization

- **WHEN** an SVG icon rasterizes and persists
- **THEN** the snapshot reports `kind = "svg"`,
  `rasterization_attempted = true`, `rasterization_succeeded = true`,
  `png_validated = true`, `persisted = true` and
  `failure_kind = "none"`

#### Scenario: Icon not found

- **WHEN** the `.desktop` declares an icon name with no candidate under any
  allowed root
- **THEN** the snapshot reports `declared = true`, `resolved = false`,
  `persisted = false`, `bytes = None`, `dimensions = None` and
  `failure_kind = "not_found"`

#### Scenario: No Icon declared

- **WHEN** the `.desktop` does not declare any `Icon=` key
- **THEN** the snapshot reports `declared = false` and `failure_kind = "none"`

#### Scenario: Path outside allowed roots

- **WHEN** an absolute `Icon=` value escapes the allowed XDG roots
- **THEN** the snapshot records `failure_kind = "not_found"`
- **AND** no path or filename is logged

### Requirement: Capture debug instrumentation

The core SHALL expose an opt-in, metadata-only capture-debug
instrumentation the operator can enable with the
`CLIPVAULT_DEBUG_CAPTURE` environment variable. The default value
SHALL be `0` (disabled). The instrumentation SHALL be inert when
disabled — no allocation, no event emission, no observable change
in the capture pipeline. The instrumentation SHALL be
strictly metadata-only: clipboard content, snippets, hashes,
`asset_ref` values, full window titles, environment-variable values,
absolute filesystem paths and any other user payload SHALL NOT
appear in any captured event. A diagnostic event MUST NOT convert
a valid capture into a `Failed` outcome. Each capture attempt
SHALL carry a monotonic, process-scoped numeric `correlation_id`
that ties every emitted event of the same attempt together. The
debug surface SHALL be gated on the production paths
(`PrivacyGate`, `TextHistoryService::record_clipboard_payload`,
`ApplicationMetadataProvider::lookup`, persistence write, outcome)
so a debug sink records the documented chain
`attempt_start → clipboard_read → cache_state → privacy_gate →
metadata_provider → persistence → outcome`. The pre-existing
redaction layer SHALL remain active on every emitted error message.

#### Scenario: Disabled by default

- **WHEN** `CLIPVAULT_DEBUG_CAPTURE` is unset or set to anything
  other than the literal string `"1"`
- **THEN** the capture pipeline does not allocate a debug sink
- **AND** no debug event is emitted
- **AND** the capture outcome matches the pre-instrumentation
  behaviour bit-for-bit

#### Scenario: Enabled through the environment variable

- **WHEN** `CLIPVAULT_DEBUG_CAPTURE=1` is set at startup
- **THEN** the bootstrap installs a debug sink that emits the
  documented events for every capture attempt
- **AND** every event is metadata-only (no content, no paths,
  no secrets, no full window titles)

#### Scenario: Per-attempt correlation id

- **WHEN** the sink observes two consecutive capture attempts
- **THEN** the first attempt's events all share `correlation_id = 1`
- **AND** the second attempt's events all share `correlation_id = 2`
- **AND** no event is emitted without a correlation id when the
  sink is enabled

#### Scenario: Environment snapshot fires once per process

- **WHEN** the first capture tick runs
- **THEN** the environment snapshot event fires exactly once
- **AND** subsequent ticks do not re-emit it

#### Scenario: Blacklisted source

- **WHEN** the privacy gate evaluates a blacklisted identifier
- **THEN** the persistence event fires with `outcome = "ignored"`
  and no row is created
- **AND** the gate decision label reflects the `blacklisted`
  reason

#### Scenario: Cache unavailable

- **WHEN** the active-app probe returns `Err(Unavailable)`
- **THEN** the cache snapshot reports `available = false`
- **AND** the outcome records `cache_populated = false`
- **AND** the capture still succeeds with `source_app = NULL`

#### Scenario: Metadata provider failure

- **WHEN** the application-metadata provider returns an error
- **THEN** the metadata snapshot fires with `error_kind` set
- **AND** the persisted row carries `source_app` and
  `source_app_name = NULL`
- **AND** the capture outcome stays `Stored` (not `Failed`)

#### Scenario: Persistence failure

- **WHEN** the SQLite write fails
- **THEN** the persistence snapshot fires with `outcome = "failed"`
  and a typed `error_kind`
- **AND** the outcome event reports `row_persisted = false`

#### Scenario: Debug sink serialises only metadata

- **WHEN** the rendering helper turns a recorded event stream
  into JSON
- **THEN** no substring matching `/Users/`, `/home/`,
  `/tmp/.clipvault`, `asset_ref`, `secret-text`,
  `password=hunter2`, `Bearer eyJ`, `.desktop`,
  `secret-window-title`, `WAYLAND_DISPLAY=` or `DISPLAY=` appears
  in the serialised payload

#### Scenario: Production paths are observable without race

- **WHEN** concurrent tests exercise the capture flow
- **THEN** each test constructs a deterministic debug sink through
  `CaptureDebugSinkHandle::from_predicate` rather than the global
  environment
- **AND** no test relies on `std::env::var("CLIPVAULT_DEBUG_CAPTURE")`

### Requirement: Shell wiring of Linux X11 / XWayland adapters

The Tauri shell MUST compile the Linux arms of
`build_active_application` and `build_paste_controller` on every
Linux build, regardless of whether the `clipvault-app` crate
itself enables the `linux-x11` feature. The Linux‑X11 adapters
live in `clipvault-platform` and the workspace `Cargo.toml`
target‑specific dependency declaration already enables the
`linux-x11` feature on `clipvault-platform` for every Linux
build; the shell MUST rely on that target‑specific dependency
instead of duplicating the `feature = "linux-x11"` gate at the
shell's own `#[cfg(...)]` attributes. Gating the shell-side
wiring on the `clipvault-app` `linux-x11` feature caused the
`build_active_application` Linux arm to compile out at build
time, the bootstrap to silently return `NoopActiveApplicationProbe`,
`capabilities.active_application` to stay `true`, and every
capture to land with `source_app = NULL` on Ubuntu GNOME Wayland
+ XWayland — the regression this requirement addresses.

#### Scenario: Linux shell arms compile without the shell feature

- **WHEN** `cargo tauri dev` runs on a Linux target
- **THEN** the `build_active_application` and
  `build_paste_controller` Linux arms are present in the shell
  binary
- **AND** the shell does not introduce a
  `#[cfg(all(target_os = "linux", feature = "linux-x11"))]`
  attribute on either arm

#### Scenario: Wayland + DISPLAY selects XWayland backend

- **WHEN** the host detects `DisplayServer::Wayland` and
  `$DISPLAY` is set to a reachable X server
- **THEN** the shell builds `X11ActiveApplication::with_kind(None,
  ProbeKind::XWayland)`
- **AND** the probe name reported by the bootstrap diagnostics
  is `xwayland_ewmh`

#### Scenario: XWayland capture persists the X11 identifier

- **WHEN** an X11 application focused under XWayland publishes
  `WM_CLASS = ("dev.warp.Warp", "dev.warp.Warp")` and the user
  copies a payload from it
- **THEN** the new `clipboard_entries` row stores
  `source_app = "dev.warp.Warp"`
- **AND** the `ApplicationMetadataProvider::lookup` receives
  `"dev.warp.Warp"` as the identifier

#### Scenario: Native Wayland keeps the empty-source contract

- **WHEN** the focused application is native Wayland and no X11
  window is exposed by XWayland
- **THEN** the cached probe is empty
- **AND** the persisted row carries `source_app = NULL`,
  `source_app_name = NULL` and `source_app_icon_ref = NULL`
- **AND** the `ApplicationMetadataProvider::lookup` is never
  consulted for the empty identifier

#### Scenario: Active-app diagnostics distinguish every state

- **WHEN** the operator inspects the
  `clipvault_active_app_diagnostics` JSON
- **THEN** the JSON exposes, at minimum, the following boolean /
  string / numeric fields, each capable of differentiating the
  states below:
  - `capabilities.active_application` — the capability the
    platform layer declares.
  - `available: bool` — the adapter the bootstrap actually
    constructed (`true` ⇔ a real adapter, `false` ⇔ the no-op
    fallback).
  - `backend: &'static str` — the active probe name
    (`macos_workspace`, `x11_ewmh`, `xwayland_ewmh`,
    `unavailable`).
  - `cache_populated: bool` — the cached probe has a non-empty
    identifier.
  - `identifier: Option<String>` — the source identifier the
    matcher will use.
  - `last_probe_stage` — `not_applicable`, `started`,
    `active_window_missing`, `active_window_empty`,
    `wm_class_missing`, `identifier_empty`, `identified`,
    `unavailable`, `backend`.
  - `net_active_window_seen: Option<bool>` — `_NET_ACTIVE_WINDOW`
    returned a parseable window id.
  - `wm_class_seen: Option<bool>` — `WM_CLASS` returned a
    parseable non-empty payload.
  - `refresh_attempts`, `successful_refreshes`,
    `failed_refreshes`, `last_refresh_unix_ms`,
    `failure_kind`, `loop_started`, `refresher_installed`,
    `timer_callback_count`, `last_capture_decision` — the
    surrounding context.
- **AND** no clipboard content, snippet, content hash, asset
  reference, full window title, environment-variable value or
  absolute filesystem path appears in any field

#### Scenario: Debug snapshot never logs content, identifiers, paths or secrets

- **WHEN** `clipvault_active_app_diagnostics` runs
- **THEN** the JSON payload never includes the clipboard
  payload, snippets, hashes, full `asset_ref` values,
  full window titles, environment-variable values, or
  absolute filesystem paths
- **AND** the existing redaction layer continues to apply to
  every emitted error message

### Requirement: System tray or menu bar access

ClipVault SHALL provide a background tray/menu-bar entry with actions to open the application, open quick search, access favorites, clear history with confirmation, open settings and quit.

#### Scenario: Background operation

- **WHEN** the user closes the main window while capture is active
- **THEN** ClipVault remains available from the tray or menu bar and continues according to the configured capture policy

#### Scenario: Quit from tray

- **WHEN** the user selects Quit from the tray or menu bar
- **THEN** ClipVault stops capture cleanly and exits without losing already committed history

### Requirement: Capability matrix

ClipVault SHALL expose a typed capability matrix that reports whether
`clipboard_read`, `clipboard_write`, `clipboard_read_image`,
`clipboard_write_image`, `global_hotkey`, `synthetic_paste`,
`active_application` and `tray` are available on the current host. Image
capabilities SHALL be evaluated independently from text capabilities.

#### Scenario: Linux Wayland detected

- **WHEN** ClipVault starts on a Wayland session
- **THEN** `clipboard_read`, `clipboard_write`, `global_hotkey` and `tray` are reported available while `synthetic_paste` is reported unavailable with an actionable reason
- **AND** `active_application` is reported available only when a real XWayland adapter and window can answer, otherwise it is reported unavailable with the structural limitation stated explicitly

#### Scenario: Text is available but image is not

- **WHEN** a platform can read and write text but cannot verify image support
- **THEN** text capabilities remain available and the image capabilities are
  reported unavailable without disabling text history

#### Scenario: Linux session differs by display server

- **WHEN** ClipVault starts on Linux X11 or Wayland
- **THEN** it reports image capabilities according to the real adapter/session
  result and does not infer X11 image behavior for Wayland

#### Scenario: Unknown display server

- **WHEN** ClipVault starts on a host where no platform adapter is available
- **THEN** the capability matrix reports every operation as unavailable and the application continues running with history and capture disabled but the local database intact

### Requirement: Hotkey lifecycle

ClipVault SHALL register a default global hotkey (`Cmd+Shift+V` on macOS, `Ctrl+Shift+V` on Linux), explicitly handle registration success, conflict with another application and unsupported sessions, and release every registered hotkey when the application shuts down.

#### Scenario: Hotkey registration succeeds

- **WHEN** ClipVault starts on a platform that supports global hotkeys
- **THEN** the default hotkey is registered and the frontend can subscribe to its activation

#### Scenario: Hotkey conflict

- **WHEN** another application already owns the configured hotkey
- **THEN** ClipVault reports the conflict, leaves the rest of the application usable and does not terminate the process

#### Scenario: Hotkey released on shutdown

- **WHEN** ClipVault is asked to quit
- **THEN** every registered hotkey is released before the process exits

### Requirement: Paste pipeline

ClipVault SHALL paste a selected text or image entry by writing the matching
payload to the system clipboard, triggering the platform-appropriate paste
action into the previously active application and reporting typed failures
without modifying the source history entry.

#### Scenario: Paste succeeds

- **WHEN** the user selects a history entry and confirms the paste
- **THEN** ClipVault writes the text to the clipboard and triggers the platform paste action and returns a typed success outcome

#### Scenario: Image paste succeeds

- **WHEN** an image entry is selected and image clipboard write plus synthetic
  paste are available
- **THEN** the image is written and pasted using the existing paste controller

#### Scenario: Paste fails

- **WHEN** the platform paste action cannot be executed
- **THEN** ClipVault returns a typed failure outcome and leaves the source entry untouched in the history

#### Scenario: Image paste is unsupported

- **WHEN** the session cannot write image data or perform the required paste
  action
- **THEN** the operation returns a typed capability result with guidance when
  available and leaves the image history entry untouched

#### Scenario: Paste unsupported under Wayland

- **WHEN** ClipVault runs on a Wayland session without a paste portal
- **THEN** `paste_entry` returns `CapabilityUnavailable` and the rest of the application remains usable

### Requirement: Clipboard watcher

ClipVault SHALL observe operating-system clipboard changes through a watcher
that delegates supported text and image payloads to the Rust core and SHALL
continue running when the backend returns errors or unsupported formats.

#### Scenario: Clipboard backend unavailable

- **WHEN** the real clipboard backend is missing or returns an error
- **THEN** the watcher reports the failure through a typed outcome and continues polling without terminating the application

#### Scenario: Image is available

- **WHEN** the watcher reads a supported image without usable text
- **THEN** it delegates the image to the same capture pipeline used by the
  history cards

#### Scenario: Non-text content

- **WHEN** the clipboard exposes a non-text representation
- **THEN** the watcher reports `Ignored` and no entry is created

#### Scenario: Unsupported format is available

- **WHEN** the watcher cannot normalize the current clipboard representation
- **THEN** it reports a typed ignored/unsupported outcome and keeps polling

### Requirement: Tray actions

ClipVault SHALL expose a tray or menu bar entry with the actions Open main window, Open quick search, Open favorites, Clear history (with confirmation), Open settings and Quit, and SHALL route actions that depend on unimplemented capabilities through thin events while reporting explicit unavailability.

#### Scenario: Quit from tray

- **WHEN** the user chooses Quit from the tray or menu bar
- **THEN** ClipVault stops capture and watchers, releases hotkeys, removes the tray icon and exits without losing already committed history

#### Scenario: Future capability action

- **WHEN** the user activates a tray action whose underlying capability is not yet implemented
- **THEN** ClipVault emits a thin `capability_unavailable` event with the capability name and keeps the rest of the application usable

### Requirement: Rich clipboard capability boundary

The platform adapter SHALL expose rich clipboard read and write behind
testable traits and SHALL report those capabilities separately from plain and
image clipboard operations. The core must remain runnable with a fake adapter
that has no desktop session.

#### Scenario: macOS rich clipboard

- **WHEN** macOS exposes HTML or RTF through its pasteboard APIs
- **THEN** the adapter returns the supported representations and reports the
  matching rich read/write capabilities

#### Scenario: Linux X11 rich clipboard

- **WHEN** Linux X11 exposes a supported rich target and the adapter can read
  or write it
- **THEN** the operation succeeds through the X11 adapter and does not change
  the plain/image capability flags

#### Scenario: Linux Wayland lacks rich integration

- **WHEN** the current Wayland session has no verified rich clipboard path
- **THEN** rich capabilities are unavailable, plain text remains usable and
  the operation returns typed capability information without a false
  permission recommendation

### Requirement: Rich paste preserves the existing target flow

Rich paste SHALL reuse the existing active-target capture, hide-before-paste
  and synthetic-paste controller. Platform adapters SHALL not receive a Tauri
  window or frontend object.

#### Scenario: Rich paste on supported platform

- **WHEN** a rich entry is selected and rich write plus synthetic paste are
  available
- **THEN** the adapter writes the selected rich representations and the
  controller pastes into the application that was active before ClipVault
  opened

#### Scenario: Rich write unavailable

- **WHEN** rich write is unavailable but plain write and synthetic paste are
  available
- **THEN** the core returns the explicit plain fallback outcome and keeps the
  existing target flow

### Requirement: Resolución de Desktop File ID de GNOME Wayland

Cuando la integración GNOME Wayland entrega un Desktop File ID válido, ClipVault SHALL conservarlo como `source_app` y SHALL usar el provider Linux existente para resolver metadata local. El provider MUST comparar el ID completo, incluido el sufijo `.desktop`, contra el Desktop File ID de una entrada local; MUST NOT tratarlo como un `WM_CLASS` ni mutar el identificador persistido.

#### Scenario: Aplicación GNOME con Desktop File ID

- GIVEN una sesión GNOME Wayland con la integración conectada
- AND GNOME Shell publica `org.mozilla.firefox.desktop`
- AND existe una entrada local correspondiente con `Name` e `Icon`
- WHEN se procesa una captura permitida
- THEN `source_app` conserva `org.mozilla.firefox.desktop`
- AND el provider puede persistir `source_app_name` y una referencia de icono relativa controlada
- AND el blacklist se evalúa antes de leer la entrada o crear el asset

#### Scenario: Prioridad de raíz XDG para Desktop File ID duplicado

- GIVEN dos entradas locales con el mismo Desktop File ID
- AND una pertenece a una raíz XDG de mayor precedencia
- WHEN el provider resuelve ese ID
- THEN usa la entrada de la raíz de mayor precedencia
- AND no deja que el path lexicográficamente menor de otra raíz la reemplace

### Requirement: Identificadores window-backed no atribuibles

La extensión GNOME MUST publicar ausencia cuando `Shell.App` sea window-backed o cuando su id use el formato especial `window:*`. ClipVault MUST NOT inferir una app, nombre o icono desde ese valor.

#### Scenario: Shell publica un id especial de ventana

- GIVEN que el foco GNOME corresponde a una `Shell.App` window-backed
- AND su id es `window:6`
- WHEN la extensión actualiza el foco
- THEN comunica ausencia de `app_id`
- AND el snapshot no queda `identified` con `window:6`
- AND no se transportan título, PID, ruta, icono ni contenido como fallback

#### Scenario: Fila legacy con identificador especial

- GIVEN una fila existente cuyo `source_app` es `window:6`
- WHEN corre el backfill de metadata Linux
- THEN la fila no genera lookup ni I/O de iconos repetido
- AND conserva sus campos persistidos sin renombrar ni borrar assets

### Requirement: Compatibilidad del matcher X11 existente

La incorporación de Desktop File IDs SHALL conservar el matcher de `WM_CLASS` para X11 y XWayland, incluidos sus órdenes de prioridad y sus fallbacks.

#### Scenario: Identificador WM_CLASS sin sufijo desktop

- GIVEN una captura X11 o XWayland con identificador `firefox`
- WHEN el provider resuelve metadata
- THEN conserva la prioridad `StartupWMClass` → `X-GNOME-WMClass` → filename stem
- AND no requiere ni fabrica el sufijo `.desktop`
