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
