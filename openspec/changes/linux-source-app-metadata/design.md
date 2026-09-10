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

## Parche funcional post‑publicación: iconos de Linux (`v0.0.6 → v0.0.7`)

### Causa raíz

Las capturas Linux persisten correctamente `source_app` y
`source_app_name` desde los parches anteriores, pero
`source_app_icon_ref` permanece `NULL` y el directorio
`~/.clipvault/assets/application-icons/` ni siquiera se crea.
La inspección de
`crates/clipvault-platform/src/runtime/linux_app_metadata.rs`
muestra tres regresiones:

1. `collect_icon_root_layout()` ya devolvía los data roots
   correctos (`/usr/share/icons`, `/usr/local/share/icons`,
   `$HOME/.local/share/icons`), pero `collect_icon_dirs()`
   volvía a concatenar `/icons` y producía rutas inexistentes
   como `/usr/share/icons/icons/...`, saltándose el layout
   canónico Ubuntu/Debian/Fedora/Arch/openSUSE
   (`/usr/share/icons/hicolor/48x48/apps/<icon>.png` /
   `/usr/share/icons/hicolor/scalable/apps/<icon>.svg`).
2. Cuando `XDG_DATA_HOME` no estaba definido, el helper
   buscaba `$HOME/applications` en lugar de
   `$HOME/.local/share/applications`, así que las
   instalaciones X11/XWayland del usuario se ignoraban.
3. El resolver sólo aceptaba PNG. La mayoría de iconos
   distribuidos por los paquetes oficiales viven como SVG en
   `scalable/`, así que la cobertura real era muy inferior a
   la disponible.

### Corrección

1. **Data roots unificados y deduplicados.** Se introduce
   `data_roots(fs)` en `linux_app_metadata.rs` que devuelve la
   lista canónica de XDG data roots en orden determinista:

   - `XDG_DATA_HOME` si está definido; en caso contrario
     `$HOME/.local/share`.
   - Cada ruta de `XDG_DATA_DIRS`.
   - Si `XDG_DATA_DIRS` está vacío, `/usr/local/share` y
     `/usr/share`.
   - Fallbacks opcionales que se incluyen sólo cuando existen
     en disco y sin desplazar los XDG_DATA_DIRS:
     `~/.local/share/flatpak/exports/share`,
     `/var/lib/flatpak/exports/share`,
     `/var/lib/snapd/desktop`, `/run/current-system/sw/share`.

   A partir de estos `data_roots` se derivan los directorios
   `<root>/applications`, `<root>/icons/<theme>/<size>x<size>/apps/`,
   `<root>/icons/<theme>/scalable/apps/` y
   `<root>/icons/<size>x<size>/apps/` (layout legacy) y
   `<root>/pixmaps`. Cada segmento se concatena exactamente
   una vez: la regresión `<root>/icons/icons/...` deja de ser
   posible.

2. **Coincidencia `.desktop` genérica.** Sin cambios de
   contrato: `StartupWMClass` → `X-GNOME-WMClass` → nombre
   del archivo `.desktop` sin extensión, comparación
   case-insensitive ASCII y desempate lexicográfico. La
   batería de tests ya no se limita a Warp: incluye Firefox,
   GNOME Terminal, Code, una aplicación arbitraria, una
   aplicación con `WM_CLASS` distinto del nombre visible y una
   aplicación sin icono.

3. **Resolución de iconos ampliada.** El resolver recorre, en
   orden determinista, los tamaños `16, 22, 24, 32, 48, 64,
   96, 128, 256` y los formatos `.png` y `.svg`. PNG tiene
   prioridad sobre SVG cuando ambos existen. Para
   `Icon=/ruta/absoluta`, la ruta se canonicaliza, se exige
   que sea un archivo regular y que viva bajo una raíz
   permitida; los symlinks que escapan de las raíces se
   rechazan.

4. **Soporte SVG vía `resvg`.** Cuando sólo existe un SVG, el
   provider lo rasteriza a PNG mediante `resvg`
   (`default-features = false`, sin texto, sin fuentes, sin
   decodificadores externos) detrás de la feature
   `linux-svg-raster`. El rasterizador:

   - Desactiva todos los resolvedores `ImageHrefResolver`
     (datos, paths, URLs): ningún recurso externo puede
     cargarse.
   - Cap el byte length a `MAX_SVG_BYTES` (4 MB).
   - Cap las dimensiones de origen a `MAX_SVG_SOURCE_DIM`
     (1024 × 1024).
   - Escala el resultado a `MAX_ICON_DIM` × `MAX_ICON_DIM`
     (256 × 256) preservando proporción y transparencia.
   - Valida que el PNG producido lleve la firma canónica.
   - Nunca invoca `convert`, `magick`, `gio` ni ningún proceso
     externo; nunca abre la red.

   El PNG se persiste en el namespace existente
   `~/.clipvault/assets/application-icons/<safe-id>.png` y la
   referencia se conserva relativa
   (`application-icons/<safe-id>.png`).

5. **`IconDiagnostics` extendido.** Se agregan los campos:

   - `kind: IconSourceKind` — `png`, `svg`, `pixmap`,
     `unknown`, `none`.
   - `rasterization_attempted: bool` — true sólo cuando el
     resolver intentó rasterizar un SVG.
   - `rasterization_succeeded: bool` — true sólo cuando la
     rasterización produjo un PNG válido.
   - `failure_kind: IconFailureKind` — categorías estables
     (`not_declared`, `not_found`, `out_of_roots`,
     `invalid_png`, `invalid_svg`, `svg_rejected`,
     `rasterization_failed`, `write_error`, `none`).

   `MetadataSnapshot` (en
   `crates/clipvault-core/src/capture_diagnostic.rs`) añade
   los mismos campos para mantener el contrato
   metadata-only que el sink `capture_debug` ya consume.

6. **Persistencia atómica.** `write_icon_atomic` continúa
   creando un temporal en el mismo directorio, validando la
   firma PNG, haciendo `sync_all`, renombrando atómicamente y
   eliminando el temporal ante cualquier error. Sólo crea
   `application-icons/` cuando el icono es válido y nunca
   reemplaza un icono existente por una respuesta vacía.

7. **Backfill.** Las filas anteriores con `source_app` y
   `source_app_name` pero `source_app_icon_ref = NULL`
   vuelven a invocar el provider en el siguiente arranque y
   persisten el icono cuando ahora puede resolverlo. No se
   borran ni renombran assets existentes.

### Contratos preservados

- `X11ActiveApplication::name()` sigue devolviendo `x11_ewmh`
  / `xwayland_ewmh`; el bootstrap sigue eligiendo
  `ProbeKind::X11` para X11 puro y `ProbeKind::XWayland`
  para Wayland con `$DISPLAY`. `parse_active_window_id` sigue
  leyendo los cuatro bytes del reply X11.
- `ApplicationMetadataProvider::lookup` sigue devolviendo
  `Ok(Some(...))` con `display_name` no vacío cuando hay
  coincidencia, incluso si el icono no puede resolverse.
- El identificador estable sigue siendo el segmento `class`
  de `WM_CLASS`; nunca `_NET_WM_NAME`. `source_app` no se
  sustituye por valores inventados en Wayland nativo.
- `LinuxApplicationMetadataProvider`, el bridge de iconos
  (`application-icons/`) y el ciclo de captura no cambian su
  contrato público.
- macOS, el blacklist, el ciclo de captura, las imágenes,
  tags, colecciones, favoritos, Quick Paste y drag-and-drop
  de cards no se tocan.
- `AboutModal.svelte` sigue leyendo `diagnostics.version` (no
  se hardcodea la versión en Svelte).

### Privacidad

- El rasterizador SVG no carga archivos locales ni recursos
  remotos; el `ImageHrefResolver` se cablea con closures que
  devuelven `None` para datos y paths. `usvg` no ejecuta
  JavaScript ni scripting alguno.
- `IconDiagnostics` no expone rutas absolutas, contenido del
  portapapeles, snippets, hashes, `asset_ref`, títulos de
  ventana ni secretos. `failure_kind` se reduce a una
  categoría tipada estable.
- La superficie JSON del sink `capture_debug metadata_provider`
  añade `icon_kind`, `rasterization_attempted`,
  `rasterization_succeeded` y `icon_failure_kind`; ningún
  campo contiene paths absolutas o contenido sensible.

### Limitaciones que se documentan

- El host actual es macOS, así que la confirmación runtime
  en una sesión Ubuntu real (X11, GNOME Wayland con app
  XWayland, GNOME Wayland nativo) queda como tarea del
  usuario. Las tareas de Ubuntu (10.1–10.6) NO se marcan
  desde macOS ni desde tests sin display; el guard
  anti-`<root>/icons/icons/...>` y los tests determinísticos
  sobre `MemoryFilesystem` validan estructuralmente el
  refactor.
- `cargo check -p clipvault-platform --features linux-svg-raster
  --target x86_64-unknown-linux-gnu --tests` actúa como
  smoke test de la rama Linux sobre el toolchain del dev host.

### Manuales Ubuntu

- Sesión X11: copiar desde una aplicación con `.desktop`,
  confirmar nombre e icono en desktop y Quick Paste, reiniciar
  y verificar persistencia. Confirmar la creación de
  `~/.clipvault/assets/application-icons/<safe-id>.png`.
- GNOME Wayland con aplicación X11/XWayland: repetir la
  prueba y revisar el diagnóstico `xwayland_ewmh`. Confirmar
  que `source_app_icon_ref` ya no es `NULL` y que
  `application-icons/` se ha creado.
- GNOME Wayland nativo: confirmar fallback explícito sin
  nombre/icono falso y sin romper captura, historial o
  blacklist.

## Parche funcional post‑publicación: shell sin `linux-svg-raster` (`v0.0.7 → v0.0.8`)

### Causa raíz

El parche de iconos (`v0.0.6 → v0.0.7`) introdujo el rasterizador
`resvg` y la feature `linux-svg-raster` dentro de
`crates/clipvault-platform/src/runtime/linux_svg_raster.rs`, y dejó
la lista de pruebas (`svg_only_icon_is_rasterized_and_persisted`,
`malformed_svg_records_invalid_svg_failure`,
`png_icon_is_preferred_over_svg`, …) gated a
`#[cfg(feature = "linux-svg-raster")]`. Sin embargo, la dependencia
target-specific de Linux en `app/tauri/src-tauri/Cargo.toml`
olvidó habilitar esa feature:

```toml
[target.'cfg(all(target_os = "linux", not(target_os = "macos")))'.dependencies]
clipvault-platform = { path = "../../../crates/clipvault-platform", features = [
    "clipboard-arboard",
    "hotkey-global",
    "linux-x11",
] }
```

El binario Ubuntu que produce `cargo build` /
`cargo tauri dev` enlaza el crate `clipvault-platform` con la
feature `linux-svg-raster` **inactiva**. En
`runtime/linux_app_metadata.rs`, la rama SVG del resolver cae al
fallback explícito:

```rust
#[cfg(not(feature = "linux-svg-raster"))]
{
    let _ = resolved;
    Err(IconFailureKind::SvgRejected)
}
```

Consecuencia observable: cualquier `.desktop` cuyo `Icon=` resuelva
a un SVG (Ubuntu, Debian, Fedora, Arch, openSUSE, GNOME, KDE — la
mayoría de los paquetes oficiales distribuyen iconos en
`scalable/apps/<name>.svg`) produce `IconFailureKind::SvgRejected`,
no escribe PNG en `<data_dir>/assets/application-icons/`,
`source_app_icon_ref` queda `NULL` y la card rail renderiza el
fallback genérico. La regresión es exactamente la que el usuario
reportó después de `v0.0.7`: "los iconos vuelven a estar vacíos en
Ubuntu, Debian y Fedora".

El bug es invisible en el host macOS del dev:

- El módulo `linux_app_metadata` está gated a `cfg(target_os =
  "linux")`, así que la suite macOS nunca compila el resolver.
- `cargo check -p clipvault-platform --features linux-svg-raster
  --target x86_64-unknown-linux-gnu --tests` valida la rama
  `linux-svg-raster` aislada, pero no la configuración real del
  binario `clipvault-app`.
- `cargo check -p clipvault-app --target x86_64-unknown-linux-gnu`
  pasa porque el shell compila; simplemente no enlaza el código
  que el rasterizador aporta.

El `Cargo.lock` del commit `e8db63a` (`fix: resolve Linux
application icons`) ya muestra la característica `resvg`
disponible para el crate `clipvault-platform`, pero sólo porque
los tests con `--features linux-x11,linux-svg-raster` se compilaron
al menos una vez; la feature nunca llegó al binario real.

### Corrección

Una línea en `app/tauri/src-tauri/Cargo.toml`:

```toml
[target.'cfg(all(target_os = "linux", not(target_os = "macos")))'.dependencies]
clipvault-platform = { path = "../../../crates/clipvault-platform", features = [
    "clipboard-arboard",
    "hotkey-global",
    "linux-x11",
    "linux-svg-raster",
] }
```

El rasterizador `resvg` y el módulo `linux_svg_raster.rs` ahora
sí se enlazan en el binario Ubuntu. No se cambia
`default = [...]`, no se añade la feature al shell como
`default` (la convención del repo es que el shell sólo declare
las suyas propias: `clipboard-arboard`, `hotkey-global`,
`linux-x11`); la feature `linux-svg-raster` pertenece a la
plataforma y se activa por dependencia target-specific, igual que
`linux-x11` desde el parche anterior.

### Regresión añadida

Dos tests cubren el bug:

1. **Regresión estructural en el shell.** Test nuevo en
   `app/tauri/src-tauri/src/bootstrap.rs::tests`:

   - `shell_linux_svg_raster_feature_is_enabled_for_linux_target`:
     parsea el `Cargo.toml` del shell, localiza la tabla
     `[target.'cfg(all(target_os = "linux", not(target_os =
     "macos")))'.dependencies]` y exige que
     `linux-svg-raster` esté en la lista de features del
     `clipvault-platform` inline-table. El parser es un walker
     TOML mínimo (no introduce una dependencia nueva): busca la
     cabecera, avanza hasta la siguiente `[ ... ]`, balancea las
     llaves del inline-table, extrae la lista y la divide por
     comas. Si un futuro refactor elimina la feature del shell,
     el test falla con la lista observada.

   El test corre en macOS, Linux y CI (no usa `cfg(target_os = …)`)
   porque el bug es estrictamente sintáctico del manifest del shell.

2. **Regresión funcional del rasterizador.** Test nuevo en
   `crates/clipvault-platform/tests/linux_app_metadata.rs`:

   - `svg_only_icon_persists_png_under_application_icons`:
     comprueba que un `.desktop` cuyo `Icon=` resuelve únicamente a
     un SVG produce un PNG persistido bajo
     `<data_dir>/assets/application-icons/<safe-id>.png` con la
     firma canónica. Valida además que la referencia `icon_ref`
     sea relativa, que `IconDiagnostics` reporte
     `kind = Svg`, `rasterization_attempted = true`,
     `rasterization_succeeded = true` y `failure_kind = None`.
     El test es genérico (no hardcodea Warp ni Ubuntu) y está
     gated a `cfg(feature = "linux-svg-raster")`. Combinado con
     el test estructural del shell, los dos cubren tanto el
     wiring de la feature como el camino real que el binario
     enlaza.

   El test existente `svg_only_icon_is_rasterized_and_persisted`
   sigue siendo la prueba de cobertura del rasterizador; el nuevo
   test documenta explícitamente la regresión del shell.

### Contratos preservados

- La lista de features del shell (`default`, `custom-protocol`,
  `clipboard-arboard`, `hotkey-global`, `linux-x11`) no cambia.
- `clipvault-platform` no introduce dependencia nueva: `resvg` ya
  figuraba como dependencia opcional (`default-features = false`)
  y se activa exclusivamente a través de `linux-svg-raster`.
- macOS, Wayland nativo, blacklist, captura, imágenes, tags,
  colecciones, favoritos, Quick Paste y drag-and-drop no se
  tocan: el cambio está limitado al bloque target-specific de
  Linux en el `Cargo.toml` del shell y a las dos regresiones
  nuevas.
- `defaults = [...]` del shell sigue siendo
  `["custom-protocol", "clipboard-arboard", "hotkey-global"]`; no
  se añade `linux-x11` ni `linux-svg-raster` para no enmascarar
  regresiones futuras de tipo "el shell olvidó habilitar la
  feature".
- El contrato `source_app` no cambia. El contrato
  `source_app_icon_ref` se llena correctamente con SVG-only
  icons, igual que ya lo hacía con PNG / pixmap.

### Privacidad y seguridad

- La lista de features del shell sigue siendo explícita: ninguna
  feature nueva entra en el `default`. El rasterizador
  `linux-svg-raster` mantiene `default-features = false` y los
  resolvedores `ImageHrefResolver` desactivados (sin red, sin
  archivos externos, sin scripts). Los límites
  `MAX_SVG_BYTES` (4 MB) y `MAX_SVG_SOURCE_DIM` (1024 × 1024)
  siguen activos.
- Los tests del regresión no escriben en `~/.clipvault`, no
  registran contenido del clipboard, snippets, hashes,
  `asset_ref`, paths absolutos ni secretos. El parser del
  `Cargo.toml` opera sobre el AST sintáctico y los nombres de
  features; el test funcional usa el `MemoryFilesystem` con un
  directorio temporal.

### Limitaciones documentadas

- El host del dev sigue siendo macOS, así que la confirmación
  runtime del PNG persistido en una sesión Ubuntu real (X11 y
  GNOME Wayland + XWayland) queda como tarea del usuario. Las
  pruebas de Ubuntu (10.1–10.4) no se marcan desde macOS.
- `cargo check -p clipvault-platform --features
  linux-x11,linux-svg-raster --target x86_64-unknown-linux-gnu
  --tests` sigue siendo el smoke test de la rama Linux sobre el
  toolchain del dev host.
