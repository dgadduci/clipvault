# ClipVault project metadata

This document anchors the canonical product metadata for ClipVault so
every package manifest, build configuration and surface that needs to
expose a version agrees on the same string. The rules below must
remain the single source of truth for the canonical product version.

## Versioning policy

The canonical product version lives in three places:

- `Cargo.toml` (`[workspace.package].version`) — drives every Rust
  crate and the Tauri shell binary;
- `app/tauri/src-tauri/tauri.conf.json` (`version`) — drives the
  bundled installer / desktop bundle identifier;
- `app/tauri/frontend/package.json` (`version`) — drives the
  frontend package and the about dialog copy.

All three MUST agree on the same string before a release is cut; the
build scripts and the modal surface that the user sees both read the
canonical value and never fall back to a hard-coded constant.

### Rules

- The first visible product version is **v0.0.1**.
- Each **functional** implementation completes bumps **only the
  patch** component. The expected cadence is therefore
  **v0.0.1 → v0.0.2 → v0.0.3 → …** until the team explicitly
  promotes the minor component.
- Resuming an interrupted implementation does **not** re-bump the
  version. If a change was already versioned before the interruption
  the patch MUST stay put — the canonical version is the one this
  file documents and the manifests agree on, not a local guess from
  the developer machine.
- Pure tooling work — running tests, regenerating the OpenSpec
  artefacts, archiving a completed change, refreshing documentation,
  rebasing a branch — MUST NOT bump the version. Only the closure of
  a functional change does.
- The "Acerca de" modal in the desktop surface MUST read its
  version from the canonical source (`Cargo.toml` via the
  `clipvault_diagnostics` Tauri command) instead of a hard-coded
  string in `Svelte`. A drift between the modal copy and the
  manifests is a regression that the change must surface as a test
  failure.
### Current canonical version

| Surface | Version |
| --- | --- |
| Cargo workspace (`Cargo.toml`) | 0.0.13 |
| Tauri config (`app/tauri/src-tauri/tauri.conf.json`) | 0.0.13 |
| Frontend package (`app/tauri/frontend/package.json`) | 0.0.13 |

The canonical public version is intentionally established at **0.0.13**.
The earlier `0.1.x` values were development manifest values and did not
represent a public product release. The `0.0.1` baseline pinned the
first visible product version; the `with_kind` → `connect_to_kind`
build fix in `linux-source-app-metadata` was a follow-up patch that
bumped the canonical version to **0.0.2**; the capture-loop cache
refresh that lets the `LinuxApplicationMetadataProvider` resolve
name and icon for X11 / XWayland captures lifted it to
**0.0.3**;
the byte-decoding fix for `_NET_ACTIVE_WINDOW` and the granular
`ProbeStage` diagnostics surface that lets the user tell apart "no
X11 window focused" from "WM_CLASS undeclared" landed at
**0.0.4**. The opt-in `CLIPVAULT_DEBUG_CAPTURE=1` instrumentation
that lets the operator confirm the `_NET_ACTIVE_WINDOW → WM_CLASS →
source identifier` chain on Ubuntu GNOME Wayland + XWayland
landed at **0.0.5**. The follow-up patch that drops the
`#[cfg(all(target_os = "linux", feature = "linux-x11"))]` arms on
`build_active_application` / `build_paste_controller` (the shell
was gating the Linux X11 adapter wiring on a feature the
`clipvault-app` crate never enables, so the bootstrap returned
`NoopActiveApplicationProbe` while `capabilities.active_application`
stayed `true` and every Ubuntu capture landed with `source_app =
NULL`) is a functional change, so the patch is incremented to
**0.0.6**. The follow-up that corrects the XDG icon root
collection, adds the `resvg`-backed SVG → PNG rasterizer and
extends `IconDiagnostics` with the format / rasterization / typed
failure surface so the resolver no longer duplicates `icons` and
no longer misses Ubuntu / Debian / Fedora / Arch / openSUSE /
GNOME / KDE layouts (X11 and XWayland alike) is a functional
change, so the patch is incremented to **0.0.7**. The follow-up
that adds the missing `linux-svg-raster` feature to the Linux
target-specific dependency on `clipvault-platform` in
`app/tauri/src-tauri/Cargo.toml` (the `resvg` rasterizer was
already wired inside `clipvault-platform` but the shell-side
dependency declaration still listed only `clipboard-arboard`,
`hotkey-global` and `linux-x11`, so the Ubuntu binary never
linked the SVG → PNG path, the resolver fell back to
`IconFailureKind::SvgRejected` and the
`application-icons/` directory stayed empty for any application
whose `Icon=` resolved to an SVG) is a functional change, so the
patch is incremented to **0.0.8**. The follow-up that adds the
Linux native Wayland active-app probe (`ext-foreign-toplevel-list-v1`
with the `zwlr_foreign_toplevel_management_unstable_v1` fallback)
behind the `linux-wayland-active-app` feature, wires it as the
authoritative source on Wayland sessions before falling back to the
XWayland EWMH probe, integrates the resolved `app_id` through the
existing `LinuxApplicationMetadataProvider` and `PrivacyGate`, and
extends the diagnostics surface with the new
`wayland_foreign_toplevel` / `wayland_wlr_foreign_toplevel`
backend identifiers plus the granular `Identified` /
`ActiveWindowEmpty` / `Unavailable` / `Backend` stages is a
functional change, so the patch is incremented to **0.0.9**.
The follow-up that re-implements the probe against the canonical
Wayland wire protocol — `wl_display` is id 1, `get_registry`
allocates a fresh `wl_registry` id, `bind` carries `name` /
`new_id` / `interface` / `version` in the documented order, the
ext `toplevel` event delivers a `new_id` (not `app_id`) and the
`app_id`/`closed`/`done`/`identifier` events come from the
per-toplevel handle, the zwlr `state` event packs a `wl_array`
whose length is in bytes (not in elements) and uses
`activated == 2` as the focus signal, `WAYLAND_SOCKET` is no
longer converted to `/proc/self/fd/<fd>`, and the probe only
returns `Operational` once the handshake and the
wlroots-provided bind land — is a functional change, so the
patch is incremented to **0.0.10**. The follow-up that adds
the optional `gnome-wayland-integration`: the bundled GNOME
Shell extension (UUID `clipvault@clipvault.app`), the metadata-only
Unix-socket IPC channel (`$XDG_RUNTIME_DIR/clipvault/clipvault-focus.sock`
with a 16-bit protocol version that rejects incompatible peers
and refuses oversized frames), the atomic installer that writes
the extension into `~/.local/share/gnome-shell/extensions/<uuid>/`
through a staging directory + `rename(2)` commit, the per-session
listener that backs `GnomeShellActiveApplication::active_application`
with the snapshot the extension publishes, the hot-swap path that
promotes the GNOME probe ahead of the native Wayland / XWayland
chain once the user accepts the consent prompt, the
`unknown/accepted/declined/disabled` consent persistence in
`app_settings`, the GNOME metadata-only diagnostics surface
(`gnome_not_detected`, `not_wayland`, `not_installed`, `disabled`,
`incompatible`, `activation_pending`, `identified`,
`no_active_application`, `disconnected`, `communication_error`)
and the dedicated consent modal that records the user's choice
on GNOME Wayland without prompting on macOS, X11 or non-GNOME
Wayland sessions — is a functional change, so the patch is
incremented to **0.0.11**. The follow-up that corrects the GNOME
integration consent lifecycle so a first launch on Ubuntu GNOME
Wayland reports `applicable = true` with `consent = unknown`
instead of collapsing to `not_applicable`, primes the in-memory
consent / technical-state cache at bootstrap so the public payload
answers without a database round-trip, gates `record_consent`
writes so `unknown` / `declined` / `disabled` never trigger an
install or start a listener, hardens the extension handshake with
a strictly serialized write queue so the `hello` envelope is
flushed before any `app_id` write is queued (the focus handler
refuses to publish until the handshake write resolves), drops
`target_dir` and `metadata_json` from the install payload's
serialised form so the public surface never carries absolute paths
or extension internals, and aligns the frontend TypeScript types
with the metadata-only Rust struct is a functional change, so the
patch is incremented to **0.0.12**. The follow-up that guarantees
the main desktop window is explicitly presented on startup and from
the tray even when a Wayland compositor reports no primary monitor,
while selecting `current`, `primary` or another available monitor
only for optional layout, is a functional correction, so the patch
is incremented to **0.0.13**.

## Bumping the version

1. Update `Cargo.toml` (`[workspace.package].version`).
2. Update `app/tauri/src-tauri/tauri.conf.json` (`version`).
3. Update `app/tauri/frontend/package.json` (`version`).
4. Add a one-line note to the change's `tasks.md` listing the
   previous and new version so the audit trail stays intact.
5. Re-run the validation suite (`cargo fmt`, `cargo clippy`,
   `cargo test`, `npm run check`, `npm run build`) to confirm the
   new version compiles everywhere.
