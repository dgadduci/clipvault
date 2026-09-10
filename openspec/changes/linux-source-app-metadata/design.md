# Diseño: linux-source-app-metadata

## Principio arquitectónico

La aplicación origen debe atravesar el mismo flujo que ya funciona en macOS:

```text
active-app probe
    -> source identifier (WM_CLASS class)
    -> ApplicationMetadataProvider::lookup
    -> source_app_name / source_app_icon_ref
    -> SQLite existente
    -> bridge de iconos existente
    -> HistoryCard / Quick Paste existentes
```

El core no debe conocer X11, Wayland, archivos `.desktop` ni rutas del sistema.
La resolución de plataforma vive en `clipvault-platform`; el shell solamente
selecciona el adapter compatible. El provider sigue devolviendo
`ApplicationMetadata` y `ApplicationMetadataError` existentes, salvo que una
extensión tipada del diagnóstico sea estrictamente necesaria.

## Matriz de sesión

| Sesión | Ventana activa | Backend de active app | Resultado |
|---|---|---|---|
| Linux X11 | X11 | EWMH + `WM_CLASS` | nombre/icono cuando existe `.desktop` |
| GNOME Wayland + XWayland | aplicación X11 | EWMH sobre `DISPLAY` | nombre/icono de la ventana X11 |
| GNOME Wayland nativo | aplicación Wayland | no portable en este cambio | indisponibilidad tipada, sin datos inventados |
| Sin display usable | ninguna | no disponible | historial sigue funcionando; origen desconocido |

La presencia de `DISPLAY` en Wayland sólo habilita el intento XWayland. El
probe debe aceptar una respuesta únicamente si obtiene una ventana X11 real y
un identificador no vacío. No se debe presentar una ventana Wayland nativa
como si fuera X11.

## Selección del active-app probe

- En `DisplayServer::X11`, conservar `X11ActiveApplication` existente.
- En `DisplayServer::Wayland`, intentar construir el adapter X11 solamente
  cuando `DISPLAY` esté definido y la conexión sea válida. Si no conecta o no
  hay una ventana X11 activa, conservar el fallback `NoopActiveApplicationProbe`.
- Mantener la cache existente y su contrato de hilo. El watcher nunca debe
  invocar desde un hilo incorrecto un adapter que requiera otro hilo.
- El identificador que alimenta el privacy gate y `source_app` sigue siendo
  el segmento `class` de `WM_CLASS`, con el fallback actual al segmento
  `instance` sólo cuando corresponde. No usar el título de la ventana como
  identificador estable.
- El backend de diagnóstico debe poder diferenciar al menos `x11_ewmh`,
  `xwayland_ewmh` y `unavailable`, sin romper la deserialización existente.

## Proveedor Linux de metadata

Crear un provider detrás del trait existente, por ejemplo
`LinuxApplicationMetadataProvider`, parametrizado por el directorio de
assets y por un lector de filesystem inyectable para tests cuando sea
necesario.

### Búsqueda de archivos `.desktop`

La búsqueda debe ser determinista y no ejecutar el campo `Exec`:

1. `$XDG_DATA_HOME/applications` si está definido; si no, usar
   `$HOME/.local/share/applications`.
2. Cada directorio `applications` bajo las entradas de `$XDG_DATA_DIRS`.
3. Fallbacks estándar `/usr/local/share/applications` y
   `/usr/share/applications` cuando no estén ya incluidos.

El parser sólo necesita las claves del grupo `[Desktop Entry]` que afectan a
este contrato: `Type`, `Hidden`, `Name`, `Name[locale]`, `Icon`,
`StartupWMClass` y `X-GNOME-WMClass`. Debe ignorar comentarios, grupos ajenos,
entradas `Hidden=true` y archivos que no sean `Type=Application`. `NoDisplay`
no implica que el metadata no pueda resolverse.

La prioridad de coincidencia, siempre con comparación normalizada sin
importar mayúsculas, será:

1. `StartupWMClass` exacto.
2. `X-GNOME-WMClass` exacto.
3. nombre del archivo `.desktop` sin extensión.
4. identificador normalizado cuando el archivo lo declare de forma
   inequívoca.

Si hay varios candidatos con la misma prioridad, elegir el path lexicográfico
menor. Nunca usar el título de ventana, `Exec` completo o una coincidencia
parcial ambigua.

El nombre visible debe preferir `Name[<locale>]` compatible con la locale del
proceso, después `Name` y finalmente devolver `None` si no hay un nombre no
vacío. No se debe almacenar el contenido del archivo `.desktop` en SQLite.

## Resolución y persistencia del icono

- `Icon=/ruta/icono.png` se resuelve como path absoluto sólo después de
  validarlo como archivo regular local que viva bajo una raíz XDG
  permitida.
- `Icon=nombre` se resuelve recorriendo el árbol canónico de iconos
  XDG en este orden:
  1. `<root>/icons/<theme>/<size>x<size>/apps/<name>.png`
  2. `<root>/icons/<theme>/scalable/apps/<name>.png`
  3. layout legacy `<root>/icons/<size>x<size>/apps/<name>.png`
  Los temas se descubren dinámicamente con `hicolor` añadido al
  final como fallback determinístico. La distribución real de Ubuntu
  instala los iconos en `/usr/share/icons/hicolor/<size>x<size>/apps/`
  y el árbol canónico es la única ruta que los encuentra.
- Solo se aceptan PNGs validados con la firma canónica
  (`\x89PNG\r\n\x1a\n`). El provider no rasteriza SVG ni invoca
  procesos externos (`convert`, `gio`, `magick`, …) — esa limitación
  está documentada en `tasks.md` y se justifica porque las
  aplicaciones objetivo de las pruebas usan PNG.
- El resultado se guarda como PNG bajo
  `<data_dir>/assets/application-icons/<safe-identifier>.png` y se persiste
  como referencia relativa mediante `icon_ref_for` o un helper compartido.
- La escritura debe ser atómica: crear temporal dentro del mismo directorio,
  escribir completamente, validar firma/tamaño/dimensiones y renombrar al
  destino. Limpiar temporales ante error.
- El provider puede devolver el nombre aunque el icono no pueda resolverse.
  La falta del icono nunca debe convertir una captura válida en `Failed`.
- No devolver paths absolutos al core, Tauri, frontend, logs ni eventos. El
  bridge existente sigue siendo el único lector de bytes.
- No sobrescribir con un icono vacío una referencia válida ya persistida.

## Probe X11/XWayland

- `WM_CLASS` se consulta como `STRING` (con fallback a
  `AnyPropertyType`). El ICCCM define `WM_CLASS` como lista de
  cadenas Latin-1; pedirla como `UTF8_STRING` hacía que el servidor X
  no devolviera bytes para ventanas GTK/Qt y la cache quedaba
  vacía.
- `_NET_WM_NAME` se consulta como `UTF8_STRING` (con fallback a
  `AnyPropertyType`); el EWMH define este propiedad como texto
  Unicode.
- Cada respuesta se valida con `format == 8` y `bytes_after == 0`
  antes de devolverla; si no cumple, el helper pasa al siguiente
  candidato.
- El segmento `class` de `WM_CLASS` es el identificador estable que
  alimenta el blacklist y el provider `.desktop`; cuando falta, se
  usa el segmento `instance`. Nunca se usa el título de la ventana.

## Selección del active-app probe

- En `DisplayServer::X11` se construye
  `X11ActiveApplication::with_kind(None, ProbeKind::X11)`. La sonda
  expone `name() = "x11_ewmh"`.
- En `DisplayServer::Wayland` se construye
  `X11ActiveApplication::with_kind(None, ProbeKind::XWayland)` cuando
  `DISPLAY` está definido y la conexión X11 es válida. La sonda expone
  `name() = "xwayland_ewmh"`. Si la conexión falla, el bootstrap cae
  a `NoopActiveApplicationProbe` y la matriz de capacidades reporta
  `active_application = false`.
- En Wayland nativo sin DISPLAY se omite la sonda por completo.
- La matriz de capacidades (`detect_capabilities_runtime`) no se
  modifica por este cambio — el `active_application` se calcula a
  partir del probe efectivamente instalado, no del display server
  detectado.

## Modal Acerca de

- El desktop agrega el ítem "Acerca de" al menú global de puntos
  suspensivos del toolbar. Las cards no exponen un ítem paralelo.
- El modal reusa el patrón `Modal.svelte` existente: mismo shell,
  mismo `onClose`, misma trampa de foco, mismo comportamiento de
  `Escape` y backdrop.
- La versión se lee siempre desde `diagnostics.version` (canal
  `clipvault_diagnostics`), que a su vez lee
  `Cargo.toml` `[workspace.package].version`. Nunca se usa un
  literal hardcodeado en `Svelte`.

## Captura, backfill y privacidad

- `PrivacyGate` debe ejecutarse antes de cualquier lectura o escritura del
  icono, igual que antes de cualquier otro asset.
- La captura conserva `source_app` como identificador de matching y sólo
  completa `source_app_name` / `source_app_icon_ref` mediante el provider.
- Reutilizar `enrich_metadata` y el backfill existente. El backfill debe ser
  acotado, idempotente y no modificar `content`, hashes, timestamps de captura,
  `asset_ref`, dimensiones, tags, colecciones ni favoritos.
- En Wayland nativo sin identificador, conservar `source_app = NULL` o el
  valor ya existente y aplicar el contrato actual de origen desconocido. No
  convertir `unavailable` en un nombre fijo como "Wayland" o "GNOME".
- Logs y errores sólo pueden contener categorías técnicas estables. No
  registrar clipboard, snippets, hashes de contenido, paths absolutos,
  contenido de `.desktop` ni bytes de iconos.

## Frontend y compatibilidad

No crear un bridge nuevo para Linux. Las cards y Quick Paste deben seguir
recibiendo `source_app_name` y `source_app_icon_ref` por el mismo DTO y el
mismo comando de icono existentes. El fallback visual actual debe continuar
funcionando cuando el provider no encuentra `.desktop` o icono.

Si el diagnóstico necesita exponer el backend, hacerlo con un campo
metadata-only y una etiqueta clara como `X11/XWayland` o `Wayland nativo no
disponible`; no mostrar paths ni identificadores internos innecesarios.

## Tests requeridos

### Unitarios

- Parseo de `.desktop` con comentarios, grupos extra, `Hidden`, `NoDisplay`,
  `Type`, claves localizadas y escapes básicos.
- Coincidencia por `StartupWMClass`, `X-GNOME-WMClass`, nombre del archivo,
  prioridades, comparación de mayúsculas y empate determinista.
- Resolución de icono absoluto, icono por nombre, fallback de tamaños,
  formatos no soportados, archivo ausente y path fuera de los directorios
  permitidos.
- Escritura atómica, validación PNG, referencias relativas y cleanup de
  temporales.
- Provider `Send + Sync`, errores tipados y nombre sin icono.

### Integración y regresión

- Bootstrap Linux X11 instala el provider Linux y el probe X11.
- Bootstrap GNOME Wayland usa XWayland sólo cuando `DISPLAY` y una conexión
  X11 válida lo permiten.
- Wayland nativo sin ventana X11 mantiene `Unavailable` sin nombre/icono
  inventados.
- Una captura permitida persiste nombre/icono; una captura blacklisted no
  crea ni lee assets.
- Backfill de filas antiguas es acotado, idempotente y conserva todos los
  campos de contenido y organización.
- Se conservan imágenes después de reiniciar, búsqueda, cambio de colección,
  tags, pin/unpin, preview, Quick Paste y drag-and-drop.
- macOS y `platform-permission-guidance` no sufren cambios funcionales.

## Parche funcional post‑publicación: cfg del shell (`v0.0.5 → v0.0.6`)

### Causa raíz

El `probe_kind_variants_are_distinct` y los demás tests de
forma de `X11ActiveApplication` validan su contrato pero no su
compilación en cada target del shell: la suite macOS ignora el
archivo (`cfg(target_os = "linux")`), de modo que el bug podía
permanecer invisible mientras Linux no se compilase desde CI.

Una vez que el usuario ejecutó `cargo tauri dev` en Ubuntu, el
log expuso el problema definitivamente. La función de bootstrap
que construye el probe activo en `clipvault-app` está
condicionada a una feature que el propio shell nunca activa:

```rust
#[cfg(all(target_os = "linux", feature = "linux-x11"))]
OsFamily::Linux => { ... }
```

La feature `linux-x11` figura en la tabla `[features]` de
`clipvault-app` pero no está incluida en su `default = [...]`,
así que la conjunción se evalúa a `false` en el shell. El
adaptador `X11ActiveApplication` y el helper
`parse_active_window_id` viven en `clipvault-platform`, donde la
feature `linux-x11` ya se activa por la dependencia
target-specific existente:

```toml
[target.'cfg(all(target_os = "linux", not(target_os = "macos")))'.dependencies]
clipvault-platform = { path = "../../../crates/clipvault-platform", features = [
    "clipboard-arboard",
    "hotkey-global",
    "linux-x11",
] }
```

Por tanto, el adaptador reside en el sitio correcto pero la rama
de construcción está mal cerrada en el shell. El bootstrap
compila su brazo sin adaptadores Linux y devuelve
`NoopActiveApplicationProbe`; mientras tanto, la matriz de
capacidades sigue declarando `active_application = true` para
intentar XWayland y los diagnósticos reportan "probe
no disponible" sin que el usuario pueda saber que la causa es
un cfg mal puesto.

La misma conjunción estaba en `build_paste_controller`, aunque
la regresión funcional observable procedía del probe activo
(la falta del `source_app`); el síntoma era idéntico: todo el
brazo Linux se descarta en tiempo de compilación.

### Corrección

1. Cambiar los atributos del shell:

   ```rust
   #[cfg(target_os = "linux")]
   OsFamily::Linux => { /* build_active_application */ }
   ```

   ```rust
   #[cfg(target_os = "linux")]
   OsFamily::Linux => { /* build_paste_controller */ }
   ```

   La decisión deja el gate en manos de la dependencia
   target-specific de `clipvault-platform` (que ya activa
   `linux-x11` cuando `target_os = "linux"`) y elimina la
   dependencia artificial de la feature del shell.

2. Limpiar el `use` redundante de `parse_active_window_id` en
   `linux_x11_active_app.rs`: el decoder activo usa
   `reply.value32()` y el helper puro se ejerce únicamente desde
   el módulo de tests `active_app::window_id_tests`, así que el
   import del shell-side ya no traía nada a la compilación real
   y contribuyó al warning `unused_import` que el usuario
   reportó.

3. Añadir regresiones estructurales que recorran
   `app/tauri/src-tauri/src/bootstrap.rs` y fallen si vuelve a
   aparecer `#[cfg(all(target_os = "linux", feature =
   "linux-x11"))]` en los brazos shell de
   `build_active_application` o `build_paste_controller`. La
   superficie prose-friendly del parser recorre líneas, salta
   comentarios `/* ... */` y `// ...`, y exige ambas subcadenas
   ("`target_os = \"linux\"`" **y** "`feature = \"linux-x11\"`")
   en una línea que empiece por `#[cfg(all(`). Cualquier intento
   futuro de reintroducir el patrón dispara el guard.

4. Añadir regresiones conductuales que ejecuten el camino del
   `bootstrap` desde un probe `ScriptedStageActiveAppProbe`:

   - `shell_linux_x11_adapter_attaches_dev_warp_warp_wm_class_to_source_app`:
     simula la ventana XWayland con `WM_CLASS = dev.warp.Warp` y
     comprueba que la fila persistida lleva `source_app =
     dev.warp.Warp` y que el provider recibe ese identificador.
   - `shell_native_wayland_keeps_empty_source_contract`: el
     probe devuelve `Ok(None)`, la fila persistida tiene
     `source_app = NULL`, el provider NO se invoca y la caché
     queda vacía.

### Contratos preservados

- `X11ActiveApplication::with_kind(display, kind)` se sigue
  construyendo a través de `Self::connect_to_kind(display,
  kind)`. La firma se mantiene y los tests
  `with_kind_signature_accepts_display_and_probe_kind` y
  `connect_to_kind_signature_accepts_display_and_probe_kind`
  siguen pasando como pin de regresión.
- `connect_to(display)` → `connect_to_kind(display, X11)` se
  conserva como atajo por defecto.
- `X11ActiveApplication::new()` → `with_kind(None, X11)` se
  conserva como constructor sin argumentos.
- `parse_active_window_id(format, value)` se mantiene en el
  módulo compilado-siempre (`crates/clipvault-platform/src/
  active_app.rs`) con la regresión contra el byte único
  (`first_byte_only_is_not_what_get_property_returns`).
- macOS, Wayland nativo, blacklist, metadata, imágenes, tags,
  colecciones, Quick Paste y drag‑and‑drop no cambian.
- `synthetic_paste = false` en Wayland nativo sigue siendo el
  valor por defecto. La matriz de capacidades no se modifica.

### Privacidad

- La corrección del cfg y las regresiones estructurales no
  registran contenido del portapapeles, snippets, hashes,
  asset_ref, paths absolutos, títulos completos de ventanas ni
  secretos: la suite trabaja sobre el AST sintáctico y sobre
  probes scriptados con datos no sensibles.
- El campo `ProbeStage` continúa siendo la única señal del
  `_NET_ACTIVE_WINDOW → WM_CLASS → identifier`, alineado con los
  selectores `net_active_window_seen` y `wm_class_seen` que ya
  expone `ActiveAppDiagnostics`.
- El import depurado de `parse_active_window_id` no participa
  en la superficie diagnostics; la única ruta activa es
  `reply.value32()`.

### Limitaciones que se documentan

- La ejecución runtime en Ubuntu GNOME Wayland + XWayland con
  `dev.warp.Warp` enfocada queda como tarea manual del
  usuario. El host actual es macOS y desde aquí no podemos
  invocar `cargo tauri dev` contra una sesión real.
- `cargo check -p clipvault-platform --features linux-x11
  --target x86_64-unknown-linux-gnu --tests` se ejecuta como
  smoke test de la rama Linux sobre el toolchain del dev host.
- Los tests específicos del cfg Linux se ejecutan en el CI de
  Ubuntu o en una build headed con `DISPLAY` válido.

### Manuales Ubuntu

- Sesión X11: copiar desde una aplicación con `.desktop`, confirmar nombre e
  icono en desktop y Quick Paste, reiniciar y verificar persistencia.
- GNOME Wayland con una aplicación X11/XWayland: repetir la prueba y revisar
  el diagnóstico `xwayland_ewmh`.
- GNOME Wayland con una aplicación nativa: confirmar fallback explícito sin
  nombre/icono falso y sin romper captura, historial o blacklist.

## Verificación

El cambio debe pasar los comandos habituales del repositorio:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd app/tauri/frontend && npm run check
cd app/tauri/frontend && npm run build
cd app/tauri/frontend && npm test
openspec validate linux-source-app-metadata --strict --type change
```

Las pruebas reales de Ubuntu X11 y GNOME Wayland quedan como tareas manuales
y no pueden marcarse por inferencia desde macOS.

## Instrumentación opt-in de captura (`CLIPVAULT_DEBUG_CAPTURE=1`)

### Principio

El usuario necesita confirmar en producción por qué `_NET_ACTIVE_WINDOW`
se decodifica correctamente pero `WM_CLASS` queda vacío en Ubuntu GNOME
Wayland + XWayland. Para responderlo sin pedir al usuario que adjunte
logs de producción con datos sensibles, el cambio envía un sink de
diagnóstico estrictamente metadata-only que el operador activa con
`CLIPVAULT_DEBUG_CAPTURE=1`.

El sink vive en `crates/clipvault-core/src/capture_diagnostic.rs` y
expone un trait `CaptureDebugSink` con un evento por fase del pipeline.
La instrumentación está desacada por la matriz funcional: la rama
Wayland/X11/XWayland no se modifica, sólo se hace observable.

### Trait y snapshots

```rust
pub trait CaptureDebugSink: Send + Sync {
    fn is_enabled(&self) -> bool;
    fn environment(&self, id: CorrelationId, snapshot: EnvironmentSnapshot);
    fn attempt_start(&self, id: CorrelationId, snapshot: AttemptSnapshot);
    fn clipboard_read(&self, id: CorrelationId, snapshot: ClipboardSnapshot);
    fn active_app_probe(&self, id: CorrelationId, snapshot: ProbeSnapshot);
    fn cache_state(&self, id: CorrelationId, snapshot: CacheSnapshot);
    fn privacy_gate(&self, id: CorrelationId, snapshot: GateSnapshot);
    fn metadata_provider(&self, id: CorrelationId, snapshot: MetadataSnapshot);
    fn persistence(&self, id: CorrelationId, snapshot: PersistenceSnapshot);
    fn outcome(&self, id: CorrelationId, snapshot: OutcomeSnapshot);
}
```

Cada snapshot es metadata-only. Los campos textuales exponen `kind` /
`bytes` / `mime` / `disponible: bool`, nunca el contenido. Los
identificadores (`source_app`) sólo aparecen cuando la cache los tiene
y son siempre la versión normalizada (`WM_CLASS` class segment).

El correlation id es un `u64` monótono por proceso, asignado por el
`CorrelationIdAllocator` que el `AppContext` mantiene. Nunca se
deriva de contenido del portapapeles.

### Sink de producción

`TracingCaptureDebugSink` emite cada evento como `tracing::debug!` con
campos estructurados (sin mensajes libres). Los identificadores del
log stream se mantienen estables: `target = "clipvault_capture_debug"`
y los mensajes (`"capture debug attempt start"`,
`"capture debug clipboard read"`, …) son constantes.

### Sink de tests

`RecordingCaptureDebugSink` acumula `RecordedEvent` para que los tests
del core inspeccionen el flujo sin tocar `tracing::Subscriber` ni
variables de entorno globales. La función
`CaptureDebugSinkHandle::from_predicate` permite inyectar un predicado
sincrónico en lugar del closure que envuelve `std::env::var`, así
los tests concurrentes no compiten por la lectura del entorno.

### Cableado

`CaptureDebugSinkHandle` se construye en
`AppBootstrap::AppBootstrap::finish` exactamente una vez:

```rust
capture_debug: self
    .options
    .capture_debug_sink
    .unwrap_or_else(CaptureDebugSinkHandle::enabled),
```

`CaptureDebugSinkHandle::enabled()` lee `CLIPVAULT_DEBUG_CAPTURE` en
ese mismo momento y construye un `EnvAwareSink` que cortocircuita
todas las llamadas cuando el flag está desactivado.

`CaptureWatcher::tick` recibe el `origin: AttemptOrigin` (background
loop o manual tick) y un `correlation_id: Option<CorrelationId>`
que propaga a `TextHistoryService::record_clipboard_payload_with_correlation`.
Ah se emite el evento `privacy_gate`, el `cache_state` (que
relee `ActiveAppDiagnostics`) y, después de `commit`, el evento
`persistence`. `TextHistoryService::enrich_metadata_with_correlation`
emite el `metadata_provider` con la `MatchStrategy` y la
`IconDiagnostics` que el provider expone. El evento terminal
`outcome` se emite al final del tick para que el operador tenga
una línea por intento.

### Privacidad

Los eventos se redactan antes de salir al log:

- Nunca `plain_text`, `html`, `rtf`, `rgba`, `original_png`,
  `content_hash`, `asset_ref`, `icon_ref` (valor), `path`,
  `home_dir`, `data_dir`, `WAYLAND_DISPLAY=…`, `DISPLAY=…`,
  títulos completos de ventana, secretos.
- Las rutas del filesystem sólo aparecen como flags booleanos
  (`display_env_present`, `wayland_display_env_present`).
- `display_name_resolved` puede aparecer (es el nombre visible de
  la aplicación origen, no el contenido).
- `correlation_id` es numérico, nunca derivado de contenido.
- Los `error_kind` se reducen a una categoría tipada
  (`"backend"`, `"empty"`, `"unavailable"`, `"invalid_image"`,
  `"clipboard"`, `"image_normalize"`, `"asset_store"`,
  `"asset_store_unavailable"`, `"persistence"`).

`assert_no_forbidden_substrings` recorre el JSON del sink y falla
si encuentra uno de los marcadores sensibles: rutas absolutas,
hashes, `asset_ref`, `secret-text`, `password=hunter2`, `Bearer`,
`/Users`, `/home`, `/tmp/.clipvault`, etc.

### Tests obligatorios

Los tests viven en
`crates/clipvault-core/src/capture_diagnostic.rs::capture_pipeline_tests`
y cubren los escenarios del usuario:

- flujo completo `_NET_ACTIVE_WINDOW format=32 → WM_CLASS → dev.warp.Warp`
  (mediante el decoder `parse_active_window_size` y los tests
  determinísticos pre-existentes en `linux_x11_active_app.rs`).
- decodificación del window id completo, no sólo el primer byte
  (`parse_active_window_id`).
- X11 puro, GNOME Wayland + aplicación XWayland, GNOME Wayland
  + aplicación Wayland nativa, DISPLAY ausente, active-app probe
  unavailable — todos modelados por el `ScriptedProbe` con sus
  respuestas controladas.
- caché vacía, caché poblada, origen bloqueado por blacklist
  (inserción previa al bootstrap para que el matcher vea la lista),
  origen permitido.
- captura de texto, captura de imagen (con el fake habilitado),
  captura duplicada.
- fallo del clipboard, fallo del metadata provider (no convierte la
  captura en `Failed`), fallo de persistencia.
- correlación de todos los eventos de un mismo intento
  (`correlation_id_groups_every_event_of_a_single_attempt`).
- logs desactivados por defecto
  (`disabled_sink_does_not_emit_any_event`,
  `disabled_sink_still_persists_capture`).
- logs activados con `CLIPVAULT_DEBUG_CAPTURE=1`
  (`enabled_sink_emits_attempt_clipboard_and_outcome_for_text_capture`).
- ausencia de contenido, hashes, asset_ref, bytes, rutas absolutas y
  títulos en todos los logs (`assert_no_forbidden_substrings`).

### Garantías arquitectónicas

- No se crea un segundo `CaptureWatcher`.
- No se duplica la lógica del probe ni del watcher.
- El contrato `source_app` no cambia.
- No se fabrican identificadores para aplicaciones Wayland nativas.
- `AboutModal.svelte` sigue leyendo la versión de
  `diagnostics.version` (no se hardcodea el literal `v0.0.5`).
