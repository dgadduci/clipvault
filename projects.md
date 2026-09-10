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
| Cargo workspace (`Cargo.toml`) | 0.0.5 |
| Tauri config (`app/tauri/src-tauri/tauri.conf.json`) | 0.0.5 |
| Frontend package (`app/tauri/frontend/package.json`) | 0.0.5 |

The canonical public version is intentionally established at **0.0.5**.
The earlier `0.1.x` values were development manifest values and did not
represent a public product release. The `0.0.1` baseline pinned the
first visible product version; the `with_kind` → `connect_to_kind`
build fix in `linux-source-app-metadata` was a follow-up patch that
bumped the canonical version to **0.0.2**; the capture-loop cache
refresh that lets the `LinuxApplicationMetadataProvider` resolve
name and icon for X11 / XWayland captures lifted it to **0.0.3**;
the byte-decoding fix for `_NET_ACTIVE_WINDOW` and the granular
`ProbeStage` diagnostics surface that lets the user tell apart "no
X11 window focused" from "WM_CLASS undeclared" landed at
**0.0.4**. The opt-in `CLIPVAULT_DEBUG_CAPTURE=1` instrumentation
this change ships — a metadata-only capture pipeline the user can
toggle on to confirm the `_NET_ACTIVE_WINDOW → WM_CLASS →
source identifier` chain on Ubuntu GNOME Wayland + XWayland — is
a functional change, so the patch is incremented to **0.0.5**.

## Bumping the version

1. Update `Cargo.toml` (`[workspace.package].version`).
2. Update `app/tauri/src-tauri/tauri.conf.json` (`version`).
3. Update `app/tauri/frontend/package.json` (`version`).
4. Add a one-line note to the change's `tasks.md` listing the
   previous and new version so the audit trail stays intact.
5. Re-run the validation suite (`cargo fmt`, `cargo clippy`,
   `cargo test`, `npm run check`, `npm run build`) to confirm the
   new version compiles everywhere.
