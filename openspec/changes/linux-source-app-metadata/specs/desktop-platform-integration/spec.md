## MODIFIED Requirements

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

## ADDED Requirements

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
