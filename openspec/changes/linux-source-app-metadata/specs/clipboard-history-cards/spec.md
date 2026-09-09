## MODIFIED Requirements

### Requirement: Source application metadata on history entries

Clipboard history entries SHALL preserve the existing stable `source_app`
identifier and SHALL best-effort enrich it with `source_app_name` and
`source_app_icon_ref` on platforms that can resolve local application
metadata. Linux X11 and XWayland SHALL use the same persisted fields and icon
bridge as macOS; unavailable native Wayland metadata SHALL remain an honest
fallback rather than a fabricated value.

#### Scenario: Linux X11 card metadata

- **WHEN** a text, rich-text or image capture is obtained from a Linux X11
  application with a matching `.desktop` entry
- **THEN** the card receives the resolved application name and icon when
  available
- **AND** the existing source-application visual treatment is reused

#### Scenario: Linux XWayland card metadata

- **WHEN** a capture is obtained from an X11 application inside a Wayland
  session and XWayland exposes its window
- **THEN** the card uses the same metadata path as Linux X11
- **AND** it does not label the application as native Wayland

#### Scenario: Native Wayland card fallback

- **WHEN** the source application is native Wayland and no supported active-app
  protocol is available
- **THEN** the card renders the existing accessible unknown-source fallback
- **AND** it does not display a misleading name or icon

#### Scenario: Metadata does not alter the capture

- **WHEN** source metadata lookup succeeds, fails or is unavailable
- **THEN** the captured content, content type, timestamps, hashes, image
  dimensions, tags, collections and favorite state remain unchanged

#### Scenario: Metadata survives restart

- **WHEN** a Linux entry with source metadata is loaded after restarting
  ClipVault
- **THEN** its persisted name and icon reference are hydrated through the
  existing recent-entry and icon-asset paths
- **AND** the card does not require a new clipboard capture to render them

## ADDED Requirements

### Requirement: Desktop "Acerca de" surface

The desktop SHALL expose a single-source "Acerca de" entry inside the
global ellipsis menu of the toolbar. The modal SHALL display the
canonical product name (`ClipVault`) and version (`vX.Y.Z`) read from
the existing `clipvault_diagnostics` Tauri command. The version SHALL
NOT be hard-coded in `Svelte`. The entry SHALL NOT be duplicated on
per-card menus or on Quick Paste. Closing the modal SHALL honour the
shared `Modal` shell contract (Escape, backdrop click and the close
button).

#### Scenario: Open the About modal from the global menu

- **WHEN** the user picks the "Acerca de" item inside the toolbar
  ellipsis menu
- **THEN** the menu closes and the modal opens
- **AND** the version label reads `v` + the value of
  `diagnostics.version` from the backend

#### Scenario: Close the About modal with Escape

- **WHEN** the About modal is open and the focus is inside it
- **THEN** pressing Escape closes the modal
- **AND** focus returns to the ellipsis menu trigger that opened it

#### Scenario: Version matches the canonical manifest

- **WHEN** the canonical version in `Cargo.toml` is `0.0.1`
- **THEN** the modal renders `v0.0.1`
- **AND** a future bump that updates `Cargo.toml`,
  `tauri.conf.json` and `package.json` shows the new value without
  any Svelte code change

#### Scenario: About modal is single-sourced

- **WHEN** the desktop renders the HistoryCard rail or Quick Paste
- **THEN** no card-level or window-level menu exposes a parallel
  "Acerca de" entry
- **AND** the global ellipsis menu is the only affordance that opens
  the modal
