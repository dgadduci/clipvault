## Purpose

Definir los controles locales de privacidad, blacklist de aplicaciones y configuración mínima del MVP.

## Requirements

### Requirement: Active-app cache diagnostics

ClipVault SHALL expose, through a metadata-only Tauri command, the state of the cached active-application probe and the outcome of the most recent refresh attempt, so the user can verify the blacklist is reading the same identifier the platform reports.

#### Scenario: Metadata-only diagnostics report

- **WHEN** the GUI requests the active-app diagnostics
- **THEN** ClipVault returns the cached identifier (when populated), the cached display name, the adapter kind (`macos_workspace`, `x11_ewmh`, `unavailable`), the cache-population flag and the outcome of the most recent refresh (`pending`, `ok`, `failed`) — never the clipboard content, the content hash or any prior source identifier

#### Scenario: Pending outcome before first refresh

- **WHEN** the application has booted but the capture loop has not performed a refresh yet
- **THEN** the diagnostics endpoint reports `refresh_outcome = "pending"` so the user knows the cache is empty and the matcher is operating under the "unknown source" contract

#### Scenario: Failed refresh surfaces the actionable reason

- **WHEN** the platform refuses to enqueue the refresh on the main thread, the main thread does not execute the closure inside the configured timeout, or the inner probe returns a backend error
- **THEN** the diagnostics endpoint reports `refresh_outcome.kind = "failed"` and a sanitised reason the user can act on; the previous cached value stays visible so the matcher can still match against it; the diagnostics counters (`refresh_attempts`, `successful_refreshes`, `failed_refreshes`) increment by exactly one per attempt

#### Scenario: Lifecycle counters and metadata

- **WHEN** the diagnostics endpoint is serialised as JSON
- **THEN** the payload includes `loop_started` (sticky flag flipped when the capture loop spawns), `refresh_attempts` (total attempts), `successful_refreshes` (count of `Ok` outcomes), `failed_refreshes` (count of `Failed` outcomes from the inner probe, the shell sync helper or the Tauri scheduler), `last_refresh_unix_ms` (epoch milliseconds of the most recent attempt or `null` if no attempt has happened yet) and `last_capture_decision` (metadata-only label such as `allowed:stored`, `discarded:blacklisted`, `unchanged` or `failed:backend`); none of these fields carry clipboard content, content hashes, snippets or source-app identifiers

#### Scenario: Blacklist match preview in the settings panel

- **WHEN** the settings panel renders the diagnostics card
- **THEN** it shows whether the currently observed identifier matches an entry in the persisted blacklist (true/false) and never suggests that the user "blacklist everything" when the cache is empty

#### Scenario: On-demand refresh from the settings panel

- **WHEN** the user clicks the **Refrescar diagnóstico** button in the settings panel
- **THEN** the frontend invokes the dedicated `clipvault_refresh_active_app_diagnostics` command which schedules a synchronous main-thread refresh, records the outcome on the diagnostics state and returns the resulting snapshot — the panel MUST NOT offer a "refresh" button that only reads the cached snapshot; a separate **Consultar diagnóstico** button is provided for the read-only path so the labels stay truthful

### Requirement: Configurable ignored-application blacklist

ClipVault SHALL provide a local setting that allows the user to add, remove
and view ignored applications using platform metadata selected through a
supported application picker, and SHALL compare each captured clipboard event
against the normalized application identifier before persisting content.

#### Scenario: Add an ignored application

- **WHEN** the user adds an application identifier to the blacklist through the local settings UI or command
- **THEN** the setting is persisted locally in SQLite and applies to subsequent clipboard events from that application

#### Scenario: Remove an ignored application

- **WHEN** the user removes an application identifier from the blacklist
- **THEN** future clipboard events from that application may be captured according to the other active policies, and previously ignored content remains absent from history

#### Scenario: List ignored applications

- **WHEN** the GUI or CLI requests the current blacklist
- **THEN** ClipVault returns the locally stored identifiers ordered by insertion or alphabetical order without contacting any external service

#### Scenario: Clipboard event from ignored application

- **WHEN** the active source application matches a blacklisted identifier
- **THEN** ClipVault does not persist the clipboard content, does not write the value to local diagnostics and does not include it in recent or search responses

#### Scenario: Identify source application across platforms

- **WHEN** the platform exposes a source application identifier (macOS bundle id, X11 WM_CLASS, Wayland app id, etc.)
- **THEN** ClipVault compares the normalized identifier against the blacklist using a deterministic matcher that can be mocked in core tests

#### Scenario: Background loop and manual tick share a watcher

- **WHEN** the background capture loop and any subsequent watcher tick (a Tauri command or a future CLI invocation) coexist
- **THEN** they operate on the same shared `CaptureWatcher` instance so dedupe, `last_hash` and the configured interval are identical for every caller, and neither path can re-evaluate a payload the other has already processed

#### Scenario: Exact identifier echoed from UI to backend

- **WHEN** the user types an identifier in the privacy panel and presses `Añadir`
- **THEN** the value forwarded to the Tauri command is the trimmed string (the panel never lowercases the value); the matcher is the only layer that normalises the identifier and the rejected identifier surfaces through the structured validation error path

#### Scenario: Select an application from macOS Applications

- **WHEN** the user presses Seleccionar aplicación and selects a valid .app bundle from the native macOS picker
- **THEN** ClipVault extracts its identifier, visible name and optional icon, persists the normalized identifier and displays the application by icon and name

#### Scenario: The picker starts in Applications

- **WHEN** the macOS application picker opens
- **THEN** it starts in /Applications and restricts the selectable target to application bundles without executing or modifying the selected application

#### Scenario: Cancel application selection

- **WHEN** the user closes or cancels the picker without selecting an application
- **THEN** ClipVault leaves settings and the ignored-applications list unchanged and does not show a mutation error

#### Scenario: Invalid selection

- **WHEN** the user selects a non-application file, an invalid bundle or a bundle without a usable identifier
- **THEN** ClipVault rejects the selection with a typed local error, leaves the blacklist unchanged and does not execute the selected path

#### Scenario: Application metadata is persisted

- **WHEN** a valid application is added to the blacklist
- **THEN** the identifier remains the matching key, the display name and icon metadata are persisted locally when available, and the same icon/name can be rendered after restart

#### Scenario: Missing icon does not block selection

- **WHEN** a valid application identifier and name are available but its icon cannot be extracted or stored
- **THEN** ClipVault adds the application with a deterministic generic icon and keeps the blacklist rule active

#### Scenario: Selecting the same application twice

- **WHEN** the user selects an application already present in the blacklist
- **THEN** ClipVault performs an idempotent update without creating a duplicate row and may fill missing metadata

#### Scenario: Existing identifier-only rows

- **WHEN** the database contains an ignored-application row created before picker metadata was introduced
- **THEN** ClipVault continues matching its identifier and displays a safe fallback name/icon without requiring a manual migration by the user

#### Scenario: Ignored application still blocks capture

- **WHEN** the active source identifier matches an application selected through the picker
- **THEN** the existing PrivacyGate discards the clipboard event before persistence and the content does not appear in recent or search responses

#### Scenario: Unsupported platform picker

- **WHEN** the current platform/session cannot map a selected application to a stable identifier used by its active-app adapter
- **THEN** ClipVault reports unsupported_session or backend_unavailable, keeps the settings panel usable and does not invent an application identifier

#### Scenario: Ignored-application list is metadata-only

- **WHEN** the GUI requests or renders ignored applications
- **THEN** the response contains only application metadata required by the UI, never clipboard content, clipboard hashes, snippets or capture payloads

### Requirement: Local minimal settings

ClipVault SHALL expose local settings for retention policy, ignored applications and the configured quick-paste hotkey without requiring an account, network call or remote configuration, and SHALL preserve ignored-application records across restarts without requiring an account, network call or remote configuration.

#### Scenario: Change a setting

- **WHEN** the user changes one of the supported settings (retention policy, ignored applications, quick-paste hotkey) and saves it through the GUI, CLI or Tauri command
- **THEN** the setting persists across application restarts in the local SQLite database and affects subsequent behavior

#### Scenario: Read effective settings

- **WHEN** the GUI or CLI requests the effective settings
- **THEN** ClipVault returns the persisted values for retention policy, ignored applications and quick-paste hotkey, including defaults when nothing is stored yet

#### Scenario: Invalid setting

- **WHEN** a setting value fails validation (unknown retention policy, malformed hotkey, empty or oversized identifier, unsupported platform value)
- **THEN** ClipVault rejects it with a useful local error that names the field and reason, preserves the last valid value and does not modify the persisted setting

#### Scenario: Retention policy is enforced

- **WHEN** the retention policy is set to a bounded value (e.g. 7 days)
- **THEN** expired entries are removed (or marked expired) on startup and on a periodic sweep, while favorites are kept regardless of retention

#### Scenario: Settings update propagates to PrivacyGate

- **WHEN** the settings service persists a change to the ignored-apps table
- **THEN** the `PrivacyGate`'s in-memory snapshot is updated before the next call returns so the gate's next `evaluate` reflects the new blacklist without requiring a restart

#### Scenario: Selected application survives restart

- **WHEN** the user selects an application, exits ClipVault and launches it again
- **THEN** the ignored-applications list contains the same normalized identifier and available presentation metadata, and PrivacyGate uses the persisted identifier without another selection

### Requirement: Privacy-preserving diagnostics

ClipVault SHALL keep diagnostics local and SHALL redact or omit clipboard content and sensitive values from logs using a deterministic redactor shared across modules.

#### Scenario: Clipboard error is logged

- **WHEN** a clipboard or database error is written to local diagnostics
- **THEN** the log contains the error category and useful context without the full clipboard content, password, token, private key or other detected sensitive value

#### Scenario: Redactor covers secrets

- **WHEN** a log message contains content the redactor classifies as a likely secret (password, API token, private key, JWT, cookie, session id)
- **THEN** the matched value is replaced with a stable placeholder (e.g. `[REDACTED:secret]`) and the surrounding context is preserved for debugging

#### Scenario: Redactor is testable

- **WHEN** tests exercise the redactor
- **THEN** the redactor is implemented as a pure function in `clipvault-core` that does not require the clipboard, GUI or filesystem and produces the same output for the same input

#### Scenario: No telemetry

- **WHEN** the application is used normally
- **THEN** it does not send usage data, clipboard content, settings, diagnostics or any other information to an external service

#### Scenario: Capture pipeline writes no sensitive payload to logs or events

- **WHEN** the watcher ticks — whether from the background loop or a manual `Tick capture` — and produces an `Allowed`, `Discard`, `Unchanged`, `Duplicate`, `Stored` or `Failed` outcome
- **THEN** neither `tracing` output, the metadata-only `clipvault://history-updated` event, nor any structured log carries the clipboard content, the content hash or the source-application identifier; only outcome categories and non-sensitive context are emitted

#### Scenario: Blacklist diagnostic state never leaks clipboard content

- **WHEN** the diagnostics endpoint is serialised as JSON
- **THEN** the payload contains the cached identifier, the refresh outcome and the cache flag — but never the clipboard content the matcher would have discarded, the content hash derived from such content or a previous blacklist entry beyond what the user already entered

### Requirement: Native application metadata picker

ClipVault SHALL expose a platform adapter for selecting an installed
application and extracting safe presentation metadata without putting platform
or filesystem logic in the frontend.

#### Scenario: macOS native picker returns metadata

- **WHEN** the native macOS picker selects a valid application bundle
- **THEN** the adapter returns a non-empty identifier, a display name when available and an optional icon representation, without launching the application

#### Scenario: Picker adapter is testable without GUI

- **WHEN** core or shell tests use a fake application picker
- **THEN** they can exercise valid, cancelled, invalid, unsupported and metadata-failure outcomes without opening a real system dialog

### Requirement: Safe application icon handling

ClipVault SHALL store or reference selected application icons only through a
local representation controlled by ClipVault and SHALL provide a generic
fallback when icon extraction is unavailable.

#### Scenario: Icon reference is safe to render

- **WHEN** the frontend renders an ignored-application row
- **THEN** it uses a validated local icon representation or a generic fallback and never loads an arbitrary user-supplied filesystem path or remote URL

#### Scenario: Icon metadata fails independently

- **WHEN** icon conversion or persistence fails after identifier validation
- **THEN** ClipVault keeps the valid blacklist identifier and display name, reports non-fatal metadata status and does not roll back the privacy rule

### Requirement: Application-picker privacy boundary

The picker flow SHALL remain local and SHALL not couple application metadata
selection with clipboard payloads or capture diagnostics.

#### Scenario: Selection produces no clipboard side effects

- **WHEN** the user selects, cancels or rejects an application
- **THEN** ClipVault does not read, write, log or emit clipboard content, hash, snippet or source-capture payload as part of the picker flow

#### Scenario: Unsupported Linux session is explicit

- **WHEN** Linux cannot provide a stable mapping from an installed application entry to the active-app identifier used by the blacklist
- **THEN** ClipVault reports the unsupported capability without inventing an identifier and leaves existing local settings and capture pipeline unchanged

### Requirement: Selector Linux sólo con asociación estable

El selector visual Linux SHALL ofrecer una aplicación únicamente cuando pueda
devolver el mismo identificador estable que consume el active-app adapter de
la sesión actual. MUST NOT inventar un identificador a partir del nombre
visible, `Exec=`, PID, título o una ruta.

#### Scenario: Sesión Linux con asociación determinista

- GIVEN una aplicación instalada con metadata local y una asociación
  determinista con el identificador activo de la sesión
- WHEN el usuario selecciona la aplicación desde Privacidad
- THEN el picker devuelve el identificador estable, nombre e icono opcional
- AND la blacklist persiste el identificador como clave de matching

#### Scenario: Sesión Linux sin asociación segura

- GIVEN que la sesión no permite demostrar la correspondencia entre la
  aplicación seleccionada y el identificador activo
- WHEN el usuario abre o usa el selector visual
- THEN devuelve `unsupported_session` o un error local tipado
- AND la UI conserva el ingreso manual
- AND no modifica la blacklist ni crea assets

#### Scenario: Cancelación del selector Linux

- GIVEN que el usuario cancela la selección
- WHEN el picker devuelve cancelación
- THEN la blacklist permanece sin cambios
- AND no se crea ni se elimina metadata de aplicación

### Requirement: Los iconos SVG válidos no generan warnings por marker none

El rasterizador Linux SHALL aceptar SVG que expresen el valor estándar
`none` en `marker`, `marker-start`, `marker-mid` o `marker-end` sin emitir los
warnings de compatibilidad de `usvg`. SHALL conservar referencias de marker
`url(#...)`, otras propiedades SVG y la política de no cargar recursos
externos.

#### Scenario: Atributos marker none

- GIVEN un icono SVG con `marker-start="none"`, `marker-mid="none"` y
  `marker-end="none"`
- WHEN el proveedor lo rasteriza
- THEN se genera un PNG válido
- AND no se registra el warning de parseo de esas propiedades

#### Scenario: CSS marker none

- GIVEN un icono SVG con `marker: none` o las propiedades individuales en un
  atributo `style` o en un bloque `<style>`
- WHEN el proveedor lo rasteriza
- THEN se genera un PNG válido sin esos warnings

#### Scenario: Marker referenciado y recursos externos

- GIVEN un SVG con `marker-start="url(#arrow)"` y una referencia externa en
  `<image>`
- WHEN el proveedor lo rasteriza
- THEN conserva el marker local
- AND rechaza la carga externa como antes

### Requirement: La superficie activa de Privacidad ofrece el selector Linux

La superficie de Privacidad montada por `App.svelte` SHALL usar el catálogo
Linux antes del picker legado cuando el comando de catálogo devuelva
`supported`. SHALL mostrar `linux-picker-modal` con los candidatos del catálogo
y SHALL usar `clipvault_ignored_app_linux_add` para confirmar una fila.

#### Scenario: X11 o Wayland con catálogo soportado

- GIVEN `App.svelte` monta `PrivacyModal.svelte`
- AND `clipvault_ignored_app_linux_catalog` devuelve `supported`
- WHEN el usuario pulsa `Seleccionar aplicación`
- THEN se muestra `linux-picker-modal`
- AND se muestran los candidatos recibidos
- AND no se invoca el picker legado

#### Scenario: Sesión Linux sin catálogo visual

- GIVEN `clipvault_ignored_app_linux_catalog` devuelve `unsupported`
- WHEN el usuario pulsa `Seleccionar aplicación`
- THEN la UI comunica que el catálogo visual no está disponible
- AND no se invoca el picker legado
- AND el error no se sustituye por su mensaje genérico `unsupported_session`

#### Scenario: Contrato de respuesta Linux reconocido por la UI

- GIVEN el comando de catálogo Linux devuelve candidatos
- WHEN Tauri serializa la respuesta hacia `PrivacyModal.svelte`
- THEN contiene `kind: "supported"` y los campos del catálogo al mismo nivel
- AND la respuesta de alta contiene `kind: "added"` o `kind: "updated"`
- AND ninguna respuesta usa una variante Serde externamente etiquetada

#### Scenario: Confirmar un candidato

- GIVEN el modal Linux está abierto con un candidato del catálogo
- WHEN el usuario selecciona ese candidato
- THEN se invoca `clipvault_ignored_app_linux_add` con su identificador
- AND la lista de aplicaciones ignoradas se sincroniza con la respuesta

#### Scenario: Cancelar no muta la blacklist

- GIVEN el modal Linux está abierto
- WHEN el usuario pulsa `Cancelar`
- THEN el modal se cierra
- AND no se invoca `clipvault_ignored_app_linux_add`
- AND no cambia la lista de aplicaciones ignoradas

### Requirement: No existe una implementación Linux duplicada en una superficie no montada

El flujo Linux SHALL estar definido en `PrivacyModal.svelte`, que es la
superficie activa, y SHALL NOT permanecer duplicado en `SettingsPanel.svelte`.

#### Scenario: Bundle actualizado

- GIVEN se ejecuta `npm run build` desde el checkout actual
- WHEN se inspecciona el bundle Vite
- THEN contiene `linux-picker-modal`
- AND contiene `ignoredAppLinuxCatalogCommand`

#### Scenario: Flujo validado manualmente en X11 y Wayland

- GIVEN el bundle frontend y el binario Tauri provienen del mismo checkout
- WHEN el usuario abre la superficie activa de Privacidad en X11 y Wayland
- THEN en ambas sesiones se muestra la lista de aplicaciones disponibles para
  agregarlas a la blacklist
- AND las aplicaciones incluidas en la blacklist no generan nuevas capturas
