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
