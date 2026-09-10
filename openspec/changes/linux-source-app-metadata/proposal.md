## Por qué

En macOS ClipVault persiste el nombre visible y el icono de la aplicación
desde la que se obtuvo una captura. En Linux el probe X11 ya puede obtener
`WM_CLASS`, pero el bootstrap instala `NoopApplicationMetadataProvider` para
cualquier host que no sea macOS. Por eso las filas pueden conservar un
identificador de origen, pero no reciben `source_app_name` ni
`source_app_icon_ref`.

En Ubuntu GNOME, además, la sesión habitual es Wayland. La variable
`DISPLAY` puede indicar que XWayland está disponible, pero no convierte una
ventana Wayland nativa en una ventana X11. ClipVault debe aprovechar XWayland
cuando exista una ventana X11 detectable y declarar explícitamente la
limitación cuando la aplicación activa sea Wayland nativa.

## Qué cambia

- Agregar un proveedor Linux de metadata de aplicación que implemente el
  trait `ApplicationMetadataProvider` existente.
- Resolver identificadores `WM_CLASS` contra archivos `.desktop` de los
  directorios de aplicaciones del usuario y del sistema.
- Obtener el nombre localizado y resolver el icono declarado por el archivo
  `.desktop` sin ejecutar comandos ni abrir la aplicación.
- Persistir el icono bajo el namespace existente
  `<data_dir>/assets/application-icons/` mediante una escritura segura y
  devolver solamente una referencia relativa controlada.
- Usar el adapter X11 existente también como fallback XWayland cuando la
  sesión Wayland exponga `DISPLAY` y exista una ventana X11 activa.
- Exponer en diagnósticos qué backend se utilizó y distinguir X11/XWayland de
  Wayland nativo sin filtrar rutas, contenido ni identificadores sensibles en
  logs o eventos.
- Reutilizar la hidratación, el bridge y la presentación de iconos/nombres ya
  usados por las cards de macOS.
- Agregar backfill acotado para filas existentes con `source_app` pero sin
  metadata, reutilizando el flujo actual y sin reescribir las capturas.

## No objetivos

- No implementar una integración genérica o o una extensión específica de GNOME
  para consultar la ventana activa Wayland nativa.
- No afirmar que XWayland identifica aplicaciones Wayland nativas: sólo cubre
  ventanas que realmente estén publicadas en X11.
- No modificar el algoritmo del blacklist ni convertir un origen desconocido
  en una coincidencia inventada.
- No cambiar captura, historial, búsqueda, filtros, pegado, imágenes, rich
  text, tags, colecciones, favoritos o drag-and-drop.
- No modificar adapters modOSOS ni el el de macOS ni el cambio `linux-x11-compatibility`.
- No añadir red, telemetría, embeddings, procesos externos ni dependencias
  innecesarias.

## Capacidades afectadas

### Capacidades modificadas

- `desktop-platform-integration`: Linux X11 y XWayland pueden proporcionar
  metadata de aplicación; Wayland nativo sigue reportando la limitación de
  forma tipada.
- `clipboard-history-cards`: las capturas Linux pueden persistir el nombre e
  icono de la aplicación origen mediante los campos existentes.

### Instrumentación opcional de diagnóstico (temporal, opt-in)

Este cambio también envía una instrumentación **opt-in y
estrictamente metadata-only** del flujo de captura para diagnosticar
por qué `source_app` queda vacío en Ubuntu GNOME Wayland + XWayland
cuando el usuario lo reporta. La instrumentación se activa
exclusivamente con la variable de entorno `CLIPVAULT_DEBUG_CAPTURE=1`
y:

- **No modifica la matriz funcional de Wayland/X11/XWayland.**
  Cuando la variable está ausente o vale `0`, el sink permanece
  inerte y la captura se comporta exactamente como antes.
- **No afecta a la persistencia.** Ningún evento de la
  instrumentación puede convertir una captura válida en `Failed`.
- **Es estrictamente metadata-only.** Nunca registra contenido del
  portapapeles, snippets, hashes, valores completos de `asset_ref`,
  rutas absolutas, títulos completos de ventana, valores de variables
  de entorno ni secretos. Los mensajes pasan por la redacción
  existente.
- **Es temporal y no permanente.** Si en una versión futuro se
  pudiera quitar sin perder cobertura, esta instrumentación se
  considera candidata a eliminación. Los tests garantizan que la
  salida sigue siendo metadata-only aunque el flag esté activo.

El contrato formal vive en el requisito `Capture debug instrumentation`
de la capacidad `desktop-platform-integration` (este mismo cambio).

## Impacto esperado

- `crates/clipvault-platform`: proveedor Linux de metadata, resolución de
  `.desktop`, resolución segura de iconos y, si hace falta, clasificación
  explícita del backend XWayland.
- `crates/clipvault-core`: sólo cambios de integración mínimos para backfill,
  diagnóstico o propagación de metadata; no se debe duplicar la lógica de
  persistencia.
- `app/tauri/src-tauri`: selección del proveedor Linux y del fallback XWayland
  en el bootstrap; comandos existentes conservados.
- Frontend: reutilización del modelo, bridge y componentes existentes; sólo
  cambios de diagnóstico o etiquetas si son necesarios.
- Tests unitarios, integración y verificación manual en Ubuntu X11 y GNOME
  Wayland.

## Por qué (parche funcional post‑publicación: cfg del shell)

Una vez publicada la versión `0.0.5`, el usuario vuelve a reportar
que en Ubuntu GNOME Wayland + XWayland el identificador
`source_app` queda NULL y la captura nunca llega a invocar el
proveedor de metadata. La causa raíz, confirmada leyendo la salida
de `cargo tauri dev`, es que los brazos Linux de
`build_active_application` y `build_paste_controller` en
`app/tauri/src-tauri/src/bootstrap.rs` están condicionados a la
feature `linux-x11` del propio crate `clipvault-app`:

```rust
#[cfg(all(target_os = "linux", feature = "linux-x11"))]
OsFamily::Linux => { ... }
```

`linux-x11` está declarada en la tabla `[features]` de
`clipvault-app` pero NO forma parte de su conjunto `default`, así
que el `cfg` se evalúa a `false` en tiempo de compilación y la
rama entera se descarta. El resultado es que:

- `build_active_application` cae al `Arc::new(NoopActiveApplicationProbe)`.
- `capabilities.active_application` sigue siendo `true` (la matriz
  de capacidades declara el intento XWayland por sesión Wayland).
- El diagnóstico reporta `Active-app probe unavailable` mientras
  `capabilities.active_application` sigue activo.
- `clipboard_entries.source_app` queda NULL; `source_app_name` y
  `source_app_icon_ref` también.
- El warning `unused import: parse_active_window_id` confirma que la
  rama quedó desacoplada del flujo real.

El adaptador `X11ActiveApplication` y el helper
`parse_active_window_id` viven en `clipvault-platform`, cuya
feature `linux-x11` ya se activa por la dependencia
target-specific existente en `app/tauri/src-tauri/Cargo.toml`:

```toml
[target.'cfg(all(target_os = "linux", not(target_os = "macos")))'.dependencies]
clipvault-platform = { path = "../../../crates/clipvault-platform", features = [
    "clipboard-arboard",
    "hotkey-global",
    "linux-x11",
] }
```

es decir, la plataforma ya tiene `linux-x11` activa en cada build
de Ubuntu. El error estaba exclusivamente en el shell.

### Qué cambia este parche

1. Los brazos Linux de `build_active_application` y
   `build_paste_controller` pasan a `#[cfg(target_os = "linux")]`
   (sin conjunción `feature = "linux-x11"`). El adaptador queda
   cableado en cada build de Linux sin necesidad de tocar la
   feature del shell ni añadirla a `default`.
2. El import sobrante de `parse_active_window_id` en
   `linux_x11_active_app.rs` se elimina: el helper sólo se usa en
   el módulo de tests `window_id_tests` del archivo
   `active_app.rs`. La regresión contra el decoder de un solo byte
   sigue cubierta por `first_byte_only_is_not_what_get_property_returns`.
3. Se agregan regresiones estructurales que detectan el patrón
   prohibido `#[cfg(all(target_os = "linux", feature =
   "linux-x11"))]` en los brazos shell de
   `build_active_application` / `build_paste_controller`, además
   de regresiones conductuales para los contratos documentados:
   Wayland + `DISPLAY` selecciona `ProbeKind::XWayland`, una
   captura con `WM_CLASS = "dev.warp.Warp"` persiste `source_app
   = dev.warp.Warp`, el `LinuxApplicationMetadataProvider` recibe
   ese identificador, el ciclo de captura bajo Linux persiste el
   `source_app` actualizado, Wayland nativo continúa devolviendo
   origen desconocido, y no hay regresiones en macOS, X11, imágenes,
   blacklist, tags, colecciones, favoritos, búsqueda, Quick Paste
   ni drag‑and‑drop.

### No objetivos del parche

- No habilitar la feature `linux-x11` del shell como solución
  principal ni añadirla a `default` para ocultar el problema: la
  dependencia target-specific sobre `clipvault-platform` ya la
  activa.
- No modificar macOS, el blacklist, el ciclo de captura, la
  captura, el historial, las imágenes, los tags, las colecciones,
  los favoritos ni el drag‑and‑drop.
- No introducir red, telemetría, llamadas a sistemas externos ni
  dependencias nuevas.

### Contratos que se preservan

- Linux X11 → EWMH y `WM_CLASS` cuando hay ventana X11 enfocada.
- GNOME Wayland + aplicación XWayland → `_NET_ACTIVE_WINDOW` y
  `WM_CLASS` a través de `DISPLAY`, con identificador estable
  siendo el segmento `class` de `WM_CLASS`.
- Wayland nativo → no se inventa identificador; el contrato
  `unavailable` se conserva.
- `synthetic_paste = false` en Wayland nativo se mantiene tal cual.
- `parse_active_window_id` lee los cuatro bytes del reply X11 en
  endianness nativa; nunca cae a `reply.value.first()`.

### Diagnóstico que se preserva y se enriquece

El campo `last_probe_stage` del JSON que sirve el comando
`clipvault_active_app_diagnostics` ya diferencia explícitamente,
entre otras, las siguientes situaciones: `not_applicable`,
`started`, `active_window_missing`, `active_window_empty`,
`wm_class_missing`, `identifier_empty`, `identified`,
`unavailable` y `backend`. La superficie serializada expone
además:

- `available: bool` — el adaptador realmente construido (NO el flag
  de capabilities).
- `backend: &'static str` — el nombre del probe activo
  (`x11_ewmh`, `xwayland_ewmh`, `unavailable`).
- `cache_populated: bool` — la caché está vacía o no.
- `identifier: Option<String>` — el `source identifier` resuelto.
- `net_active_window_seen: Option<bool>` — el `_NET_ACTIVE_WINDOW`
  se decodificó como window id válida.
- `wm_class_seen: Option<bool>` — el `WM_CLASS` devolvió una cadena
  parseable no vacía.
- `refresh_attempts`, `successful_refreshes`, `failed_refreshes`,
  `last_refresh_unix_ms` — contadores monotónicos.
- `failure_kind`, `message`, `loop_started`,
  `refresher_installed`, `timer_callback_count`,
  `last_capture_decision` — resto de metadatos.

El comando `clipvault_diagnostics` sigue exponiendo
`capabilities.active_application` (la capability declarada), y el
usuario puede entonces diferenciar "capability declarada" de
"adapter realmente construido" comparando ambos campos. La
superficie no registra contenido del clipboard, snippets, hashes,
bytes de assets, rutas absolutas, títulos completos de ventanas
ni secretos.

### Impacto esperado

- `app/tauri/src-tauri/src/bootstrap.rs`: dos atributos `cfg`
  corregidos, una suite de regresiones estructurales y
  conductuales añadidas, y un nuevo import depurado.
- `crates/clipvault-platform/src/runtime/linux_x11_active_app.rs`:
  un `use` redundante eliminado; el resto del archivo no cambia.
- Manifiestos canónicos (`Cargo.toml`, `Cargo.lock`,
  `tauri.conf.json`, `package.json`, `package-lock.json`) y
  `projects.md`: versión patch `0.0.5 → 0.0.6`.
- Artefactos OpenSpec actualizados (`proposal.md`, `design.md`,
  `tasks.md`, `specs/desktop-platform-integration/spec.md`).

### Limitaciones que se documentan

- La verificación runtime en una sesión real de Ubuntu GNOME
  Wayland + XWayland con `dev.warp.Warp` enfocada queda como
  tarea del usuario (no se puede ejecutar desde macOS).
- Los tests específicos del target Linux (`cfg(target_os =
  "linux")`) se ejecutan en el CI de Ubuntu y/o en builds
  cross-compiladas desde macOS con
  `cargo check -p clipvault-platform --features linux-x11
  --target x86_64-unknown-linux-gnu`.

## Por qué (parche funcional post‑publicación: iconos de Linux)

Después del parche del cfg del shell (v0.0.5 → v0.0.6) el
identificador `source_app` se persiste correctamente y el
`LinuxApplicationMetadataProvider` se invoca con un `WM_CLASS`
válido. Sin embargo el icono (`source_app_icon_ref`) sigue
quedando `NULL` y el directorio `~/.clipvault/assets/application-icons/`
ni siquiera se crea. El usuario confirma que las capturas desde
Ubuntu, Debian, Fedora, Arch, openSUSE, GNOME, KDE y los flujos
X11 + XWayland muestran el icono genérico en lugar del icono
declarado por el `.desktop` correspondiente. La inspección del
diagnóstico confirma que las aplicaciones X11/XWayland se
detectan correctamente y `source_app_name` también se persiste,
así que el cuello de botella está exclusivamente en la
resolución de iconos del provider Linux.

La causa raíz reside en
`crates/clipvault-platform/src/runtime/linux_app_metadata.rs`:

1. `collect_icon_root_layout()` ya devuelve rutas como
   `/usr/share/icons`, `/usr/local/share/icons`,
   `/home/<usuario>/.local/share/icons`; pero
   `collect_icon_dirs()` vuelve a agregar `/icons` y produce
   rutas duplicadas como `/usr/share/icons/icons/...` que no
   existen en ningún host y se saltan el layout canónico
   `/usr/share/icons/hicolor/48x48/apps/<icon>.png` /
   `/usr/share/icons/hicolor/scalable/apps/<icon>.svg`.
2. Cuando `XDG_DATA_HOME` no está definido,
   `collect_application_dirs()` busca
   `$HOME/applications` en lugar del `$HOME/.local/share/applications`
   correcto.
3. El resolver sólo acepta PNG. La mayoría de las aplicaciones
   Linux distribuyen sus iconos como SVG, especialmente en el
   directorio `scalable/`, por lo que la cobertura real queda
   muy por debajo de lo que Ubuntu/Debian/Fedora/Arch/openSUSE
   exponen.

### Qué cambia este parche

1. Se introduce una única colección `data_roots()` que se
   construye a partir de `XDG_DATA_HOME` (o, en su defecto,
   `$HOME/.local/share`), `XDG_DATA_DIRS` y los fallbacks
   estándar `/usr/local/share` y `/usr/share`. Se deduplica,
   se normaliza y se itera exactamente una vez por segmento, lo
   que elimina la regresión `<root>/icons/icons/...`. Los
   directorios derivados (`<root>/applications`,
   `<root>/icons`, `<root>/pixmaps`) se generan a partir de esos
   `data_roots()` y se filtran por `is_dir`, de modo que las
   rutas inexistentes se ignoran sin abortar.
2. Se aceptan fallbacks opcionales sólo cuando existen y sin
   desplazar `XDG_DATA_DIRS`: `~/.local/share/flatpak/exports/share`,
   `/var/lib/flatpak/exports/share`, `/var/lib/snapd/desktop`,
   `/run/current-system/sw/share`. La lista es determinista y
   no introduce dependencias nuevas.
3. La resolución de iconos por nombre recorre, en orden
   determinista:
   `<root>/icons/<theme>/<size>x<size>/apps/<name>.png`,
   `<root>/icons/<theme>/<size>x<size>/apps/<name>.svg`,
   `<root>/icons/<theme>/scalable/apps/<name>.png`,
   `<root>/icons/<theme>/scalable/apps/<name>.svg`,
   `<root>/pixmaps/<name>.png`,
   `<root>/pixmaps/<name>.svg`,
   más el layout legacy equivalente. Los tamaños cubiertos son
   `16, 22, 24, 32, 48, 64, 96, 128 y 256`. El PNG tiene
   prioridad sobre el SVG cuando ambos existen.
4. Las rutas absolutas declaradas en `Icon=` se canonicalizan,
   se acepta únicamente si son archivos regulares, se verifica
   que vivan bajo una raíz permitida y se rechazan los symlinks
   que escapen de las raíces.
5. Se añade soporte para SVG mediante una biblioteca Rust pura
   (`resvg` con `default-features = false`, expuesta por la
   feature `linux-svg-raster`): el rasterizador desactiva todos
   los resolvedores externos, limita dimensiones, complejidad y
   tamaño, produce un PNG RGBA válido con dimensiones
   controladas, conserva proporción y transparencia, y nunca
   invoca `convert`, `magick`, `gio` ni ningún proceso externo.
   El resultado se persiste en el namespace existente
   `~/.clipvault/assets/application-icons/<safe-id>.png` y la
   referencia se conserva relativa (`application-icons/<safe-id>.png`).
6. `IconDiagnostics` se extiende con el formato de origen
   (`png` / `svg` / `pixmap` / `unknown`), las banderas
   `rasterization_attempted` / `rasterization_succeeded`, las
   dimensiones y un motivo tipado de fallo (`not_declared`,
   `not_found`, `out_of_roots`, `invalid_png`, `invalid_svg`,
   `svg_rejected`, `rasterization_failed`, `write_error`). El
   diagnóstico nunca expone rutas absolutas, contenido del
   portapapeles, snippets, hashes, `asset_ref`, títulos de
   ventana ni secretos.
7. Se añade una batería de tests unitarios e integración que
   cubren: `Firefox`, `GNOME Terminal`, una aplicación
   arbitraria, `source_app` distinto del nombre visible,
   aplicación sin icono, `theme` `hicolor`, `Yaru` y `Adwaita`,
   `scalable`, `pixmaps`, PNG directo, SVG rasterizado a PNG,
   PNG preferido sobre SVG, icono inexistente, `.desktop` sin
   `Icon=`, symlink fuera de raíz, SVG malformado,
   rasterización segura, persistencia atómica, creación del
   directorio `application-icons`, backfill de filas anteriores
   y un guard explícito que rompe si el código vuelve a
   construir rutas `<root>/icons/icons/...`.

### No objetivos del parche

- No hardcodear Ubuntu ni Warp: el provider sigue siendo
  genérico para cualquier aplicación freedesktop.
- No introducir LLMs, embeddings, llamadas de red, telemetría
  ni procesos externos.
- No tocar el shell de Tauri, macOS, el blacklist, el ciclo de
  captura, las imágenes, los tags, las colecciones, los
  favoritos, el Quick Paste ni el drag-and-drop de cards.
- No cambiar el contrato `source_app` ni la matriz de
  capacidades.

### Contratos que se preservan

- Linux X11 → EWMH y `WM_CLASS` cuando hay ventana X11
  enfocada; GNOME Wayland + XWayland → `_NET_ACTIVE_WINDOW`
  + `WM_CLASS` cuando hay ventana X11; Wayland nativo →
  `unavailable` sin identificador fabricado.
- El identificador estable sigue siendo el segmento `class`
  de `WM_CLASS`. `parse_active_window_id` sigue leyendo los
  cuatro bytes del reply (`u32::from_ne_bytes`) y nunca cae a
  `reply.value.first()`.
- El bridge de iconos (`application-icons/`), `icon_ref_for`,
  `write_icon_atomic` y el validador PNG existente siguen
  siendo los únicos lectores / escritores de bytes.
- El namespace de assets nunca se contamina con rutas
  absolutas, contenido de `.desktop`, SVG persistido, secretos
  ni snippets.

### Diagnóstico

`IconDiagnostics` se conserva como la única señal que la
captura diagnóstica inspecciona: ahora expone
`kind` (`png` / `svg` / `pixmap` / `unknown`),
`rasterization_attempted`, `rasterization_succeeded`,
`png_validated`, `persisted`, `bytes`, `dimensions` y
`failure_kind` (estable, sin rutas absolutas). La superficie
sigue siendo estrictamente metadata-only: nunca clipboard,
snippets, hashes, `asset_ref`, paths absolutas, títulos de
ventana ni secretos. La superficie JSON del sink
`capture_debug metadata_provider` añade los campos
`icon_kind`, `rasterization_attempted`,
`rasterization_succeeded` y `icon_failure_kind` sin romper la
deserialización existente.

### Impacto esperado

- `crates/clipvault-platform/src/runtime/linux_app_metadata.rs`:
  refactor de `data_roots()` + `collect_directories()`, nuevo
  resolver que prefiere PNG sobre SVG y recorre los tamaños
  `16, 22, 24, 32, 48, 64, 96, 128, 256`.
- `crates/clipvault-platform/src/runtime/linux_svg_raster.rs`
  (nuevo): rasterizador SVG → PNG con `resvg`
  (`default-features = false`), sin red ni procesos externos,
  con límites de tamaño y dimensiones.
- `crates/clipvault-platform/src/app_metadata.rs`:
  `IconSourceKind`, `IconFailureKind` y campos adicionales en
  `IconDiagnostics`; las cadenas `as_str()` son el contrato
  estable del sink `capture_debug`.
- `crates/clipvault-core/src/capture_diagnostic.rs`:
  `MetadataSnapshot` expone `icon_kind`,
  `rasterization_attempted`, `rasterization_succeeded` y
  `icon_failure_kind`.
- Manifiestos canónicos (`Cargo.toml`, `Cargo.lock`,
  `tauri.conf.json`, `package.json`, `package-lock.json`) y
  `projects.md`: versión patch `0.0.6 → 0.0.7`.
- Artefactos OpenSpec actualizados (`proposal.md`, `design.md`,
  `tasks.md`, `specs/desktop-platform-integration/spec.md`).

### Limitaciones que se documentan

- El host actual es macOS, así que la confirmación runtime en
  una sesión Ubuntu real (X11 y Wayland con app XWayland) queda
  pendiente del usuario. Las tareas de Ubuntu (10.1–10.6) NO se
  marcan como completadas desde macOS ni desde tests sin
  display; el guard anti-`<root>/icons/icons/...>` y los tests
  determinísticos sobre `MemoryFilesystem` validan
  estructuralmente el refactor.
- `cargo check -p clipvault-platform --features linux-svg-raster
  --target x86_64-unknown-linux-gnu --tests` actúa como smoke
  test de la rama Linux sobre el toolchain del dev host.
