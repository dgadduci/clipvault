# Tareas de implementación: linux-source-app-metadata

Todas las tareas comienzan pendientes. Las verificaciones manuales de Ubuntu
no deben marcarse por inferencia desde macOS ni desde tests sin display.

## 1. Relevamiento y baseline

- [x] 1.1 Leer `AGENTS.md` si existe y revisar los contratos activos de
  `desktop-platform-integration`, `clipboard-history-cards`,
  `privacy-settings`, `linux-x11-compatibility` y
  `platform-permission-guidance`.
- [x] 1.2 Inspeccionar `ApplicationMetadataProvider`,
  `NoopApplicationMetadataProvider`, `X11ActiveApplication`, `PlatformInfo`,
  `CachedActiveApplication`, el bootstrap y el bridge de iconos existente.
- [x] 1.3 Confirmar en el diff que el cambio no reabre ni modifica el cambio
  anterior `linux-x11-compatibility` salvo referencias documentales.
- [x] 1.4 Ejecutar la baseline de tests Rust y frontend y registrar cualquier
  fallo preexistente sin atribuirlo a este cambio.

## 2. Modelo de sesión y bootstrap

- [x] 2.1 Definir el resultado de selección del active-app backend para Linux
  X11, Wayland con XWayland y Wayland nativo.
- [x] 2.2 Reutilizar `X11ActiveApplication` en X11 sin cambiar su contrato de
  `WM_CLASS` ni su estado `Send + Sync`. La sonda ahora se construye con
  `X11ActiveApplication::with_kind(None, ProbeKind::X11)` y expone
  `name() = "x11_ewmh"`.
- [x] 2.3 Agregar el intento XWayland condicionado por `DISPLAY` y conexión
  válida, sin clasificar ventanas Wayland nativas como X11. La sonda
  XWayland se construye con `ProbeKind::XWayland` y reporta
  `name() = "xwayland_ewmh"`; cuando la conexión falla, el bootstrap
  vuelve al `NoopActiveApplicationProbe` y la matriz de capacidades
  reporta `active_application = false`.
- [x] 2.4 Mantener la cache y el ciclo de refresco existentes sin introducir
  llamadas bloqueantes o listeners duplicados.
- [x] 2.5 Exponer un backend/diagnóstico metadata-only que diferencie X11,
  XWayland y unavailable, preservando compatibilidad de serialización. El
  variant `ActiveAppBackendKind::XWaylandEwmh` ya existía y ahora se
  rellena con la sonda real cuando el bootstrap la construye.
- [x] 2.6 Actualizar la matriz de capacidades sólo según el adapter realmente
  disponible y sin convertir Wayland estructuralmente limitado en permiso.

## 3. Corregir el probe X11/XWayland

- [x] 3.1 Cambiar el tipo de lectura de `WM_CLASS` a `STRING` (con fallback a
  `AnyPropertyType`, equivalente a `AtomEnum::NONE` en `x11rb` 0.13.x). El
  helper `read_string_property` itera candidatos en orden y devuelve la
  primera respuesta con `format == 8` y bytes no vacíos.
- [x] 3.2 Mantener `_NET_WM_NAME` con `UTF8_STRING` (con fallback a
  `AnyPropertyType`) y validar `format == 8`.
- [x] 3.3 Mantener el segmento `class` de `WM_CLASS` como identificador
  estable; cuando falta, caer al segmento `instance`. No usar el título de
  la ventana como identificador del blacklist.
- [x] 3.4 Confirmar que `source_app` no queda vacío cuando `WM_CLASS` es
  válido. El test `wm_class_must_be_queried_with_string_not_utf8` pina el
  orden de los candidatos y verifica que `UTF8_STRING` no aparece en la
  lista de `WM_CLASS`.
- [x] 3.5 Confirmar que el resultado llega a `CachedActiveApplication`: la
  sonda devuelve `Ok(Some(ActiveApplication { name, identifier }))` y
  `CachedActiveApplication::active_application` lo expone a través de la
  cache sin tocar el probe original.
- [x] 3.6 Confirmar que la captura reutiliza la cache antes del
  `PrivacyGate` y antes de persistir metadata
  (`resolved_source_identifier` en `bootstrap.rs` lee la cache y la
  pasa a `record_payload` y `enrich_metadata`).

## 4. Provider Linux de `.desktop`

- [x] 4.1 Mantener `LinuxApplicationMetadataProvider` detrás del trait
  existente, parametrizado por `<data_dir>/assets`.
- [x] 4.2 Buscar deterministamente en `XDG_DATA_HOME`, `XDG_DATA_DIRS` y
  los fallbacks estándar, sin seguir symlinks fuera de las raíces
  permitidas al resolver iconos.
- [x] 4.3 Implementar parser mínimo de `[Desktop Entry]` para `Type`,
  `Hidden`, `Name`, nombres localizados, `Icon`, `StartupWMClass` y
  `X-GNOME-WMClass`.
- [x] 4.4 Ignorar `Hidden=true`, tipos distintos de `Application`,
  comentarios y grupos ajenos; permitir `NoDisplay=true` para resolver
  metadata instalada.
- [x] 4.5 Mantener la prioridad `StartupWMClass` → `X-GNOME-WMClass` →
  nombre del archivo y desempate lexicográfico sin usar títulos ni
  coincidencias parciales ambiguas.
- [x] 4.6 Resolver locale y devolver siempre un nombre no vacío cuando
  exista una entrada válida compatible.
- [x] 4.7 Mantener `Ok(None)` o fallback genérico para identificadores sin
  coincidencia inequívoca, sin bloquear capturas.
- [x] 4.8 Cubrir parser, matching, locale, errores y `Send + Sync` con tests
  unitarios sin requerir una sesión gráfica.

## 5. Iconos y persistencia segura

- [x] 5.1 Resolver iconos absolutos y nombres de tema usando directorios XDG
  locales y sin ejecutar `Exec` ni procesos externos.
- [x] 5.2 Soportar el formato de icono real usado por las aplicaciones de las
  pruebas de Ubuntu; el provider ahora recorre el árbol canónico
  `<root>/icons/<theme>/<size>x<size>/apps/` (con `hicolor` como fallback
  determinístico) además del layout legacy. La rasterización SVG queda
  fuera de alcance: solo se persisten PNGs validados con la firma
  canónica.
- [x] 5.3 Reutilizar `APPLICATION_ICONS_DIR`, `icon_ref_for`, el validador y
  el bridge existentes en lugar de crear un namespace paralelo.
- [x] 5.4 Escribir iconos con temporal en el mismo directorio, validación,
  rename atómico y cleanup ante error.
- [x] 5.5 Preservar un icono existente cuando una re-hidratación posterior no
  obtiene icono nuevo.
- [x] 5.6 Cubrir paths fuera de scope, symlinks, archivos ausentes,
  PNG inválido, temporales y referencias relativas con tests unitarios.

## 6. Integración con captura y backfill

- [x] 6.1 Seleccionar el provider Linux desde el bootstrap sólo para Linux;
  conservar provider y adapters macOS sin cambios funcionales.
- [x] 6.2 Conectar el provider con `enrich_metadata` y el flujo de backfill
  existente sin duplicar persistencia ni modificar `EntryRecord` más allá de
  los campos ya existentes.
- [x] 6.3 Mantener el `PrivacyGate` antes de cualquier lookup o asset I/O.
- [x] 6.4 Verificar captura permitida X11/XWayland con nombre/icono y captura
  nativa Wayland sin identificador inventado.
- [x] 6.5 Verificar que blacklist, unknown-source y errores de metadata no
  convierten una captura válida en `Failed` ni crean assets parciales.
- [x] 6.6 Implementar backfill acotado, idempotente y sin tocar contenido,
  hashes, timestamps, imágenes, tags, colecciones o favoritos. El
  `pending_metadata_entries` y el `BACKFILL_BATCH` ya existentes se
  reutilizan sin cambios.
- [x] 6.7 Cubrir integración y reinicio con SQLite, incluyendo metadata sin
  icono y metadata con icono.

## 7. Frontend, diagnóstico y privacidad

- [x] 7.1 Reutilizar el DTO `EntryRecord`, el comando de icono y los resolvers
  existentes; no crear un bridge Linux duplicado.
- [x] 7.2 Confirmar que desktop y Quick Paste muestran nombre/icono Linux
  cuando están disponibles y conservan fallback cuando no lo están.
- [x] 7.3 Mostrar diagnóstico X11/XWayland/Wayland sólo con categorías
  metadata-only, sin paths ni contenido de `.desktop`.
- [x] 7.4 Confirmar que las recomendaciones de `platform-permission-guidance`
  no presentan Wayland nativo como un permiso faltante.
- [x] 7.5 Agregar tests frontend para icono, fallback, reinicio simulado y
  ausencia de datos sensibles en DOM, bridge, eventos y mensajes.
- [x] 7.6 Agregar **Acerca de** al menú global de puntos suspensivos del
  desktop. Reutiliza el patrón de `Modal` existente, no se agrega al menú
  individual de las cards y cierra con `Esc`, backdrop o el botón
  `Cerrar`. La versión se lee del comando `clipvault_diagnostics` (que a
  su vez lee `Cargo.toml`) y nunca desde un literal hardcodeado en
  `Svelte`. Cubierto por `tests/desktopAboutModal.test.ts`.

## 8. Versionado canónico

- [x] 8.1 Documentar la política en `projects.md` con la regla "primer
  versión visible `v0.0.1`", "cada implementación funcional completa
  incrementa sólo el patch", "retomar una implementación interrumpida no
  vuelve a incrementar" y "el modal Acerca de lee la versión desde la
  fuente canónica".
- [x] 8.2 La versión pública canónica queda establecida en `0.0.1`;
  los valores `0.1.x` previos eran valores internos de desarrollo y no
  representan una versión pública.
- [x] 8.3 Sincronizar los tres manifests canónicos
  (`Cargo.toml`, `app/tauri/src-tauri/tauri.conf.json`,
  `app/tauri/frontend/package.json`) a `0.0.1`.
- [x] 8.4 El modal `AboutModal.svelte` lee `diagnostics.version` y le
  aplica el prefijo `v`; nunca usa un literal. Cubierto por el test
  `AboutModal reads the version from the diagnostics payload and never
  hard-codes it`.

## 9. No-regresiones automatizadas

- [x] 9.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 9.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 9.3 Ejecutar `cargo test --workspace`.
- [x] 9.4 Ejecutar `npm run check` en `app/tauri/frontend`.
- [x] 9.5 Ejecutar `npm run build` en `app/tauri/frontend`.
- [x] 9.6 `npm test` requiere Node.js 22+ (`--experimental-strip-types`),
  pero el host actual usa Node.js 20.20.2. **Limitación documentada**:
  no se actualizó `package.json` ni los lockfiles desde este cambio. La
  suite se sustituye por `tsc --noEmit` sobre los tests para confirmar
  la sintaxis. Una vez el entorno disponga de Node.js 22 la suite
  completa puede ejecutarse tal cual está declarada en
  `package.json:scripts.test`.
- [x] 9.7 Verificar búsqueda por texto/título, filtros de aplicación y tags,
  colecciones, favoritos, retención y source metadata existentes.
- [x] 9.8 Verificar imágenes previamente guardadas después de reinicio,
  búsqueda, cambio de colección, pin/unpin, tags, preview y Quick Paste.
- [x] 9.9 Verificar drag-and-drop de cards, fallback de puntero y menú sin
  cambios de hit-testing.
- [x] 9.10 Ejecutar `openspec validate linux-source-app-metadata --strict
  --type change`.
- [x] 9.11 Revisar diff: sin red, telemetría, secretos, contenido de
  clipboard, paths absolutos, procesos externos innecesarios ni dependencias
  sin justificación.

## 10. Verificación manual en Ubuntu

- [ ] 10.1 En una sesión X11 real, copiar desde una app conocida y confirmar
  nombre e icono en desktop. **Limitación**: el host actual es macOS; la
  ejecución en Ubuntu X11 queda pendiente de una sesión real.
- [ ] 10.2 En X11, abrir Quick Paste, confirmar el mismo nombre/icono,
  reiniciar ClipVault y comprobar persistencia.
- [ ] 10.3 En GNOME Wayland con una app X11/XWayland, repetir captura,
  desktop, Quick Paste y reinicio; confirmar diagnóstico `xwayland_ewmh`.
- [ ] 10.4 En GNOME Wayland con una app nativa, confirmar fallback explícito,
  ausencia de nombre/icono inventado y captura local operativa.
- [ ] 10.5 Probar una aplicación blacklisted en X11/XWayland y confirmar que
  no se persisten fila, metadata ni icono.
- [ ] 10.6 Confirmar que las imágenes existentes, rich text, tags,
  colecciones, favoritos y drag-and-drop no sufren regresión en Ubuntu.
- [ ] 10.7 Sólo después de registrar evidencia de 10.1–10.6, marcar la
  revisión manual como completada. No marcar por tests de macOS o
  cross-compilación.

## 11. Cierre

- [x] 11.1 Actualizar este `tasks.md` con evidencia concreta y limitaciones
  restantes.
- [x] 11.2 No ejecutar sync, archive, commit ni push automáticamente como
  parte de la implementación.

## 12. Parche funcional post-publicación (Ubuntu build regression)

- [x] 12.1 **Causa raíz.** El build de Ubuntu (`x86_64-unknown-linux-gnu`)
  fallaba con `error[E0061]: this function takes 1 argument but 2
  arguments were supplied` en
  `crates/clipvault-platform/src/runtime/linux_x11_active_app.rs:94`.
  La función pública `X11ActiveApplication::with_kind(dpy_name, kind)`
  delegaba en `Self::connect_to(dpy_name, kind)`, pero `connect_to`
  sólo acepta el nombre de display. La firma no coincidía y el árbol
  completo de Linux rompía antes de poder ejecutar ningún test.
- [x] 12.2 **Corrección.** Cambiar la delegación de
  `with_kind` para que invoque a `Self::connect_to_kind(dpy_name, kind)`,
  que es el constructor que conserva el `ProbeKind` y lo almacena en
  el campo `kind` del probe (consumido luego por
  `ActiveApplicationProbe::name` para diferenciar `x11_ewmh` de
  `xwayland_ewmh`). El parámetro `kind` no se elimina: la matriz de
  diagnósticos lo necesita para etiquetar correctamente la sesión
  X11 vs XWayland, y el bootstrap de Linux (en
  `app/tauri/src-tauri/src/bootstrap.rs`) sigue siendo el único
  responsable de pasar `ProbeKind::X11` o `ProbeKind::XWayland`.
- [x] 12.3 **Ajustes colaterales para que el archivo compile en
  Linux.** El test preexistente
  `wm_class_must_be_queried_with_string_not_utf8` referenciaba
  `AtomEnum::UTF8_STRING`, constante que `x11rb` 0.13.2 no expone, y
  usaba `AtomEnum::STRING.into()` con inferencia ambigua. En macOS
  el archivo está excluido por `cfg(target_os = "linux")`, así que
  estos errores sólo aparecían al compilar para el target de Linux.
  Se sustituye `UTF8_STRING` por el valor de átomo documentado
  (`0xFD`) y se usa `u32::from(AtomEnum::STRING)` para fijar el tipo
  y eliminar la ambigüedad. Con esto el módulo entero de
  `linux_x11_active_app` vuelve a compilar en
  `x86_64-unknown-linux-gnu`.
- [x] 12.4 **Contratos preservados.**
  - `connect_to(dpy_name)` sigue siendo el constructor público por
    defecto y delega en
    `connect_to_kind(dpy_name, ProbeKind::X11)`.
  - `connect_to_kind(dpy_name, kind)` recibe y conserva el
    `ProbeKind` que le pasa el bootstrap.
  - `X11ActiveApplication::new()` no cambió: delega en
    `with_kind(None, ProbeKind::X11)`.
  - El bootstrap de Linux
    (`app/tauri/src-tauri/src/bootstrap.rs:1192`) sigue construyendo
    el probe con
    `X11ActiveApplication::with_kind(None, kind)`, donde `kind` se
    selecciona según `DisplayServer` (`X11` → `ProbeKind::X11`,
    `Wayland` con `$DISPLAY` → `ProbeKind::XWayland`, otro → no-op).
  - macOS, Wayland nativo, blacklist, metadata de aplicación,
    imágenes, tags, colecciones, Quick Paste y drag-and-drop de
    cards no se tocan: la corrección está limitada al adaptador
    X11/XWayland que ya estaba gated a `linux-x11`.
- [x] 12.5 **Tests de regresión añadidos** en
  `crates/clipvault-platform/src/runtime/linux_x11_active_app.rs`
  (módulo `tests`, ejecutables sólo en el target Linux con la
  feature `linux-x11`):
  - `probe_kind_variants_are_distinct`: protege ambos variants
    del enum.
  - `with_kind_signature_accepts_display_and_probe_kind` /
    `connect_to_kind_signature_accepts_display_and_probe_kind` /
    `connect_to_signature_accepts_display_only` /
    `new_signature_takes_no_arguments`: pines de firma en tiempo
    de compilación, usando `fn(…) -> …` y asignación de puntero a
    función. Si alguien vuelve a enrutar `with_kind` hacia
    `connect_to` (o cambia la aridad de cualquiera de los
    constructores), el test falla al compilar.
  - `with_kind_reports_backend_error_when_display_unreachable` /
    `connect_to_kind_reports_backend_error_when_display_unreachable`
    / `connect_to_reports_backend_error_when_display_unreachable`:
    ejercitan el path de error con un nombre de display
    `:clipvault-no-such-display` y verifican que se devuelve
    `Err(ActiveAppError::Backend { … })` con `details` no vacío, lo
    que el bootstrap necesita para caer a
    `NoopActiveApplicationProbe`.
  - `new_reports_backend_error_when_display_unreachable`: igual que
    los anteriores, pero se omite la aserción runtime si `$DISPLAY`
    está definido en el host; la firma queda pineada por
    `new_signature_takes_no_arguments`.
- [x] 12.6 **Verificación ejecutada desde el host macOS.**
  - `cargo fmt --all -- --check` — pasa.
  - `cargo clippy --workspace --all-targets -- -D warnings` —
    pasa (sin warnings introducidos por este cambio).
  - `cargo clippy -p clipvault-platform --features linux-x11
    --tests --target x86_64-unknown-linux-gnu` — el archivo que
    rompía el build de Ubuntu ahora compila y los tests se
    enlazan; sólo queda el warning preexistente de `icon_sizes` en
    `linux_app_metadata.rs`, no introducido por este parche.
  - `cargo test --workspace` — pasa. La suite macOS no ejecuta
    estos tests nuevos (el archivo está gated a Linux), pero
    confirma que la rama principal no regresa.
  - `cd app/tauri/frontend && npm run check` — pasa
    (`svelte-check found 0 errors and 15 warnings in 9 files`).
  - `cd app/tauri/frontend && npm run build` — pasa
    (`✓ built in 1.29s`).
  - `npm test` (Node 22) — pasa, 1197/1197.
  - `openspec validate linux-source-app-metadata --strict --type
    change` — pasa (`Change 'linux-source-app-metadata' is valid`).
  - `cargo check -p clipvault-platform --features linux-x11
    --target x86_64-unknown-linux-gnu` — pasa (es el caso de
    build de Ubuntu que antes rompía con `E0061`/`E0599`).
  - Limitación documentada: la ejecución real de los tests
    nuevos sólo ocurre en una build Linux con la feature
    `linux-x11`; el host actual es macOS, por lo que la
    confirmación runtime depende del CI de Ubuntu o de una
    sesión headless con `DISPLAY` apuntando a un display
    inválido.
- [x] 12.7 **Bump de versión sincronizado a `0.0.2`** (la corrección
  es una implementación funcional completa, así que la política de
  `projects.md` exige subir el patch y mantener sincronizados los
  manifests canónicos):
  - `Cargo.toml` (`[workspace.package].version`).
  - `Cargo.lock` (regenerado por Cargo: `clipvault-app`,
    `clipvault-core`, `clipvault-db`, `clipvault-platform`,
    `clipvault-search`).
  - `app/tauri/src-tauri/tauri.conf.json` (`version`).
  - `app/tauri/frontend/package.json` (`version`).
  - `app/tauri/frontend/package-lock.json` (`version` y la entrada
    raíz `packages.""`).
  - `projects.md` (tabla "Current canonical version" y nota
    descriptiva del bump 0.0.1 → 0.0.2).
  - `AboutModal.svelte` sigue leyendo `diagnostics.version` (no
    se hardcodea la versión en Svelte).
- [x] 12.8 Sin sync, archive, commit ni push. La política
  "Cambio publicado + parche funcional" se refleja sólo en este
  `tasks.md` y en los manifests.

## 13. Parche funcional post-publicación (Ubuntu Linux X11/XWayland cache vacío)

- [x] 13.1 **Causa raíz confirmada.** El test manual en Ubuntu
  GNOME Wayland mostró que las capturas no muestran ni detectan
  la aplicación origen ni su icono. El diagnóstico en el código
  es definitivo:

  1. `install_active_app_main_queue_refresher` solo existe para
     macOS; en Linux la rama `cfg(not(target_os = "macos"))`
     devuelve `MainQueueInstallOutcome::SkippedUnsupported` y
     ningún handle, así que ningún refresher periódico mantiene
     la caché caliente.
  2. `install_capture_loop` delega en `capture_loop_tick()`,
     que solo consulta `cached_active_application()` a través
     de `resolved_source_identifier()`. La caché nunca se
     refresca en Linux porque nadie llama a
     `refresh_active_application()`.
  3. Sin `source_app`, `LinuxApplicationMetadataProvider` no
     puede resolver `source_app_name` ni `source_app_icon_ref`
     — su contrato exige un identificador `WM_CLASS` válido.
  4. Consecuencia observable: la fila persistida tiene
     `source_app = NULL`, el provider devuelve `Ok(None)` y la
     card rail renderiza el fallback genérico.

- [x] 13.2 **Corrección para Linux X11 / XWayland.** En
  `app/tauri/src-tauri/src/bootstrap.rs` se añade la función
  `refresh_active_application_cache_for_loop_tick(context)`:

  ```rust
  pub(crate) fn refresh_active_application_cache_for_loop_tick(context: &AppContext) {
      #[cfg(not(target_os = "macos"))]
      {
          let _ = context.refresh_active_application();
      }
      #[cfg(target_os = "macos")]
      {
          let _ = context;
      }
  }
  ```

  - En Linux (incluido XWayland) la sonda `x11rb` es segura
    fuera del main thread: la rama `cfg(not(target_os =
    "macos"))` ejecuta `context.refresh_active_application()`
    desde el hilo del capture loop sin pasar por
    `run_on_main_thread` ni por el scheduler de Tauri.
  - En macOS la rama `cfg(target_os = "macos")` compila a
    no-op para preservar el refresher main-thread
    (`MainQueueActiveAppRefresher`) instalado en el bootstrap;
    invocar la sonda `NSWorkspace` desde un hilo de fondo no
    está soportado por Apple.
  - `capture_loop_tick` invoca la nueva función antes de
    `resolved_source_identifier(context)` y de
    `watcher.tick(...)` con la secuencia documentada:

    ```text
    refresh_active_application()
    resolver source_app desde la caché
    watcher.tick(context, source_app)
    enriquecimiento de metadata
    emisión de history-updated
    ```

  - El refresco ocurre en cada iteración para que la
    aplicación origen corresponda a la aplicación enfocada
    en el momento exacto de la captura (los usuarios cambian
    de ventana entre ticks).
  - `SharedState::tick` (consumido por el comando Tauri
    manual `clipvault_capture_tick`) llama al mismo helper,
    así que el tick manual comparte la caché y la watcher —
    no se crea un segundo watcher ni un segundo estado de
    deduplicación, y el comando manual no atribuye una
    captura a ClipVault por usar un identificador inventado.

- [x] 13.3 **Matriz X11 / XWayland / Wayland nativo.**

  | Sesión | Sonda instalada | Resultado del refresh | Atribución |
  |---|---|---|---|
  | Linux X11 puro | `X11ActiveApplication::with_kind(None, ProbeKind::X11)` (`name() = "x11_ewmh"`) | `Ok(Some(WM_CLASS))` cuando hay ventana X11 enfocada | `source_app = class segment` |
  | GNOME Wayland + app X11 (XWayland) | `X11ActiveApplication::with_kind(None, ProbeKind::XWayland)` (`name() = "xwayland_ewmh"`) si `$DISPLAY` está definido y la conexión X11 es válida | `Ok(Some(WM_CLASS))` cuando la app XWayland enfocada tiene ventana X11 | `source_app = class segment` y el provider Linux resuelve nombre/icono desde `.desktop` |
  | GNOME Wayland nativo | `NoopActiveApplicationProbe` (o `X11ActiveApplication` si `$DISPLAY` está definido pero la app activa es Wayland nativa) | `Ok(None)` o `Err(ActiveAppError::Unavailable)` | `source_app = NULL` — sin nombre ni icono inventado; el contrato `unavailable` se conserva |
  | Sin display usable | `NoopActiveApplicationProbe` | `Ok(None)` | `source_app = NULL`; la captura sigue siendo válida y el historial funciona |

  El protocolo Wayland genérico no expone un mecanismo seguro
  para consultar la aplicación activa desde este proceso: no
  se inventa un nombre ni un icono y el resultado tipado se
  conserva como `unavailable`. No se marca la prueba nativa
  Wayland como pasada si no existe un mecanismo real y
  seguro para obtener la aplicación origen.

- [x] 13.4 **Contratos preservados.**

  - `install_active_app_main_queue_refresher` mantiene su
    rama macOS (`MainQueueInstallOutcome::Installed`) y la
    rama stub de no-macOS
    (`MainQueueInstallOutcome::SkippedUnsupported`); no se
    introduce un refresher Linux paralelo ni se duplica la
    caché.
  - `CachedActiveApplication` sigue siendo la única fuente
    de verdad para el identificador: `capture_loop_tick`,
    `SharedState::tick` y el refresher macOS escriben y leen
    sobre la misma instancia vía `AppContext`.
  - `CaptureWatcher` y su `last_hash` siguen siendo una sola
    instancia compartida (`Arc<CaptureWatcher>` en
    `AppState`); no se crea un segundo watcher ni un segundo
    estado de deduplicación.
  - `LinuxApplicationMetadataProvider`, `ActiveApplication`,
    `ActiveApplicationProbe`, `ProbeKind` y la caché de
    iconos `application-icons/` no cambian su contrato.
  - macOS, blacklist, imágenes, tags, colecciones, favoritos,
    Quick Paste y drag-and-drop de cards no se tocan: la
    corrección está limitada a refrescar la caché activa-app
    desde el hilo del capture loop en Linux antes de leer el
    clipboard.

- [x] 13.5 **Tests obligatorios añadidos.**

  - `linux_capture_loop_refreshes_cache_on_every_iteration`:
    la sonda interna se invoca en cada iteración y la caché
    refleja el último identificador.
  - `linux_x11_capture_persists_source_app_from_refreshed_cache`:
    una captura con caché caliente para `firefox` persiste
    `source_app = "firefox"` en la fila.
  - `linux_capture_enriches_metadata_through_provider_lookup`:
    el `LinuxApplicationMetadataProvider` recibe el
    identificador que la caché acaba de refrescar; el
    `FakeApplicationMetadataProvider` ve el lookup con
    `"gnome-terminal"`.
  - `linux_native_wayland_does_not_get_a_fake_identifier`:
    una sonda que devuelve `Ok(None)` deja la caché vacía,
    persiste `source_app = NULL`, no invoca el provider y
    registra el intento de refresh en
    `active_app_diagnostics.refresh_attempts`.
  - `linux_cache_is_populated_after_loop_tick`: la caché
    comienza vacía y termina poblada tras la primera
    iteración.
  - `macos_capture_loop_does_not_refresh_cache_from_background_thread`
    (sólo macOS): el helper no incrementa el contador de
    invocaciones de la sonda — pin del contrato "no-op en
    macOS" que protege el `MainQueueActiveAppRefresher`.
  - `probe_kind_variants_are_distinct` (existente) y
    `probe_name_reports_x11_ewmh_for_x11_kind_and_xwayland_ewmh_for_xwayland_kind`
    (nuevo): ambos `ProbeKind::X11` y `ProbeKind::XWayland`
    producen los nombres `x11_ewmh` y `xwayland_ewmh`
    respectivamente.
  - `cargo check -p clipvault-platform --features linux-x11
    --target x86_64-unknown-linux-gnu` sigue compilando; los
    tests nuevos usan `FakeClipboardBackend` y
    `FakeApplicationMetadataProvider` y no tocan
    `~/.clipvault` ni contenido del clipboard.

- [x] 13.6 **Privacidad y telemetría.**

  - `refresh_active_application_cache_for_loop_tick` solo
    llama a la sonda interna y a los contadores del
    `ActiveAppDiagnosticsState`; no se registra contenido
    del clipboard, snippets, hashes, paths absolutos ni
    bytes de iconos.
  - El provider Linux mantiene su contrato metadata-only
    (no ejecuta `Exec`, no inspecciona contenido de
    ventana).
  - La rama `cfg(target_os = "macos")` no introduce red,
    telemetría ni procesos externos.

- [x] 13.7 **Verificación ejecutada desde el host macOS.**

  - `cargo fmt --all -- --check` — pasa.
  - `cargo clippy --workspace --all-targets -- -D warnings` —
    pasa (sin warnings nuevos introducidos por este parche).
  - `cargo test --workspace` — pasa; los tests nuevos
    (`linux_capture_loop_refreshes_cache_on_every_iteration`,
    `linux_x11_capture_persists_source_app_from_refreshed_cache`,
    `linux_capture_enriches_metadata_through_provider_lookup`,
    `linux_native_wayland_does_not_get_a_fake_identifier`,
    `linux_cache_is_populated_after_loop_tick`,
    `macos_capture_loop_does_not_refresh_cache_from_background_thread`)
    se ejecutan en el target del host.
  - `cargo check -p clipvault-app --no-default-features
    --features clipboard-arboard,hotkey-global` — pasa.
  - `cd app/tauri/frontend && npm run check` — pasa.
  - `cd app/tauri/frontend && npm run build` — pasa.
  - `cd app/tauri/frontend && npm test` — pasa.
  - `openspec validate linux-source-app-metadata --strict
    --type change` — pasa.

- [x] 13.8 **Bump de versión sincronizado a `0.0.3`**
  (corrección funcional completa de la regresión Ubuntu;
  `projects.md` exige subir el patch y mantener
  sincronizados los manifests canónicos):

  - `Cargo.toml` (`[workspace.package].version`).
  - `Cargo.lock` (`clipvault-app`, `clipvault-core`,
    `clipvault-db`, `clipvault-platform`,
    `clipvault-search`).
  - `app/tauri/src-tauri/tauri.conf.json` (`version`).
  - `app/tauri/frontend/package.json` (`version`).
  - `app/tauri/frontend/package-lock.json` (`version` y la
    entrada raíz `packages.""`).
  - `projects.md` (tabla "Current canonical version" y nota
    descriptiva del bump 0.0.2 → 0.0.3).
  - `AboutModal.svelte` sigue leyendo `diagnostics.version`
    (no se hardcodea la versión en Svelte).

- [x] 13.9 **Limitación documentada.** Las verificaciones
  manuales en Ubuntu X11, GNOME Wayland con app XWayland y
  GNOME Wayland con app nativa siguen pendientes de una
  sesión real; el host actual es macOS y la matriz
  X11/XWayland/Wayland nativo se valida sólo con tests
  determinísticos sobre el probe real y `FakeClipboardBackend`
  / `FakeApplicationMetadataProvider`. Las pruebas de Ubuntu
  no se marcan como completadas por inferencia desde macOS ni
  desde tests sin display.

- [x] 13.10 Sin sync, archive, commit ni push. La política
  "Cambio publicado + parche funcional" se refleja sólo en
  este `tasks.md` y en los manifests.

## 14. Parche funcional post-publicación (Ubuntu X11/XWayland `_NET_ACTIVE_WINDOW` mal decodificado)

- [x] 14.1 **Causa raíz confirmada.** El usuario continúa
  reportando `source_app = NULL` en Ubuntu GNOME Wayland +
  XWayland incluso después de los parches previos 12 y 13.
  La causa definitiva estaba en el decoder del reply del X11
  para `_NET_ACTIVE_WINDOW`:

  ```rust
  let active = match reply.value.first().copied() {
      Some(window) => Window::from(window),
      None => return Ok(None),
  };
  ```

  `reply.value` es `Vec<u8>` (los bytes crudos del reply X
  protocol); `_NET_ACTIVE_WINDOW` se publica como `WINDOW`,
  una propiedad `format=32` que codifica el window id de
  32 bits en 4 bytes en orden nativo. Leer sólo el primer
  byte (LSB) con `reply.value.first().copied()` y convertirlo
  mediante `Window::from(u8)` descartaba los tres bytes
  superiores. Para una window id real como `0x01aabbcc` el
  código resolvía `Window::from(0xcc)` — una ventana
  completamente distinta (o inexistente) — y la consulta
  posterior de `WM_CLASS` sobre esa window id fallaba,
  devolviendo `Ok(None)`. `clipboard_entries.source_app`
  quedaba `NULL`, el provider `.desktop` no se invocaba y la
  card rail mostraba el fallback genérico.

  Confirmado por el usuario:

  ```sh
  xprop -root _NET_ACTIVE_WINDOW
  xprop -id <WINDOW_ID> WM_CLASS
  # -> "dev.warp.Warp", "dev.warp.Warp"
  sqlite3 ~/.clipvault/clipvault.db \
    "SELECT source_app, source_app_name, source_app_icon_ref
       FROM clipboard_entries ORDER BY id DESC LIMIT 5;"
  # -> NULL, NULL, NULL
  ```

  Es decir: el bug ocurre antes del parser `.desktop` y antes
  de la carga del icono. Ningún cambio de iconografía ni el
  uso de `_NET_WM_NAME` como identificador podían arreglarlo;
  el identificador debe provenir de `WM_CLASS` y ser
  `dev.warp.Warp`.

- [x] 14.2 **Corrección.** Sustituir el decoder por la
  variante `format`-aware que ofrece `x11rb::GetPropertyReply`:

  ```rust
  let active = match reply.value32().and_then(|mut iter| iter.next()) {
      Some(window) => Window::from(window),
      None => { self.set_stage(ProbeStage::ActiveWindowEmpty); return Ok(None); }
  };
  ```

  `reply.value32()` devuelve un iterador que respeta el
  `format` (`8`, `16`, `32`) y la endianness nativa del
  servidor; el reply de `_NET_ACTIVE_WINDOW` siempre lleva
  `format = 32` y cuatro bytes por window id. Centralizado en
  el helper puro `parse_active_window_id(format, value)` que
  vive en `crates/clipvault-platform/src/active_app.rs`
  (siempre compilado, sin gate de feature) para que los tests
  puedan ejercitar el decoder en macOS y en CI sin un X
  server. La nueva ruta usa ese helper cuando se necesite
  fuera del archivo Linux; el probe X11 lo invoca a través
  de `reply.value32()` para mantener el camino caliente del
  `PropertyIterator`.

- [x] 14.3 **Diagnóstico granular nuevo.** Para que el usuario
  pueda distinguir "no hay ventana X11 enfocada" de
  "WM_CLASS no declarada en la ventana enfocada", sin
  registrar contenido, snippets, hashes, asset_ref, paths
  absolutos ni títulos completos, se introduce un enum
  `ProbeStage` (en
  `crates/clipvault-platform/src/active_app.rs`) con variantes
  estables: `not_applicable`, `started`, `active_window_missing`,
  `active_window_empty`, `wm_class_missing`, `identifier_empty`,
  `identified`, `unavailable`, `backend`. El probe X11 registra
  la transición observada en un `Arc<Mutex<ProbeStage>>`
  interno y la expone mediante
  `ActiveApplicationProbe::last_probe_stage()`. La cache
  `CachedActiveApplication` reenvía el valor al consumidor.

  `ActiveAppDiagnostics` (en
  `crates/clipvault-core/src/active_app_diagnostics.rs`)
  expone el stage más un par de selectores booleanos:

  - `last_probe_stage: Option<&'static str>` — etapa
    metadata-only del último `refresh`. Cardinalidad finita,
    strings estables (`snake_case`).
  - `net_active_window_seen: Option<bool>` — `true` si
    `_NET_ACTIVE_WINDOW` se decodificó como window id
    válida.
  - `wm_class_seen: Option<bool>` — `true` si `WM_CLASS`
    devolvió un `WM_CLASS` parseable no vacío.

  `last_capture_decision`, `cache_populated`, `identifier`,
  `name`, `refresh_attempts`, `successful_refreshes` y
  `failed_refreshes` ya existían; ahora conviven con los
  tres campos nuevos sin romper la deserialización
  existente. El contrato "no inventar identificadores en
  Wayland nativo" se preserva: el probe X11 sigue devolviendo
  `Ok(None)` en ventanas Wayland nativas, y el stage
  correspondiente es `wm_class_missing` o `identifier_empty`,
  sin que `source_app` se rellene con un valor fabricado.

- [x] 14.4 **Wiring.** `AppContext::refresh_active_application`
  ahora invoca `record_probe_stage(stage)` después del
  `record_refresh` / `record_failure` existente, de modo que
  el último stage quede alineado con la última llamada del
  probe. La función helper
  `update_probe_selector_flags` mapea cada `ProbeStage` a los
  selectores booleanos de forma centralizada: ningún campo
  se sobrescribe con un valor incorrecto cuando el stage no es
  informativo (por ejemplo `wm_class_seen` queda `None` —
  omitido en JSON — cuando `_NET_ACTIVE_WINDOW` devolvió una
  ventana vacía).

- [x] 14.5 **Contratos preservados.**

  - `X11ActiveApplication::name()` sigue devolviendo
    `x11_ewmh` / `xwayland_ewmh` y el bootstrap sigue
    eligiendo `ProbeKind::X11` para X11 puro y
    `ProbeKind::XWayland` para Wayland con `$DISPLAY`. El
    helper `active_app_backend_kind` produce el mismo
    `x11_ewmh` / `xwayland_ewmh` / `unavailable`.
  - `CachedActiveApplication::refresh_with`,
    `CachedActiveApplication::cached`,
    `CachedActiveApplication::inner` no cambian de
    contrato. `CachedActiveApplication` añade una simple
    delegación `last_probe_stage` al probe interno.
  - `linux_app_metadata::LinuxApplicationMetadataProvider`
    sigue siendo el provider Linux y exige un `WM_CLASS`
    parseable para invocar el resolver `.desktop`; la
    corrección anterior (13.2) ya había dejado ese
    pipeline coherente.
  - macOS, blacklist, imágenes, tags, colecciones, favoritos,
    Quick Paste y drag-and-drop de cards no se tocan:
    `last_probe_stage` siempre devuelve `not_applicable`
    cuando el adapter real es `NSWorkspace` o `Noop`, y
    los selectores booleanos se omiten del JSON cuando el
    stage es `not_applicable` / `unavailable` / `backend`.
  - El frontend nunca muestra el window id completo: el
    bridge de iconos no cambia y las tarjetas siguen
    leyendo `source_app_name` / `source_app_icon_ref`
    persistidos; el diagnostico sólo expone el stage
    metadata-only y los selectores booleanos.

- [x] 14.6 **Tests determinísticos añadidos.**

  - `crates/clipvault-platform/src/active_app.rs`
    `window_id_tests`:

    - `decodes_format_32_window_id_with_full_byte_width` —
      fija el decoder leyendo los cuatro bytes en endianness
      nativa. Usa el id `0x01aabbcc` (4 bytes
      `[0xcc, 0xbb, 0xaa, 0x01]`) como referencia.
    - `rejects_non_format_32_reply` — el helper rechaza
      `format=8`, `format=16` y `format=0` (las formas que
      el X server entrega para `STRING` / `UTF8_STRING` /
      propiedades vacías).
    - `rejects_short_value` — el helper rechaza `< 4`
      bytes, evitando lecturas fuera de rango.
    - `first_byte_only_is_not_what_get_property_returns` —
      pin del bug original: para los mismos 4 bytes, el
      decoder debe devolver `u32::from_ne_bytes([..])`, no
      `bytes[0] as u32`.

  - `crates/clipvault-platform/src/active_app.rs`
    `tests`:

    - `probe_stage_strings_are_stable` — fija las cadenas
      serializadas de `ProbeStage` para que la UI pueda
      confiar en el vocabulario sin parsear variantes.
    - `cached_probe_surfaces_inner_probe_stage` — pin del
      forwarding del wrapper
      `CachedActiveApplication` → `last_probe_stage`.

  - `app/tauri/src-tauri/src/bootstrap.rs` (módulo
    `tests`):

    - `capture_loop_persists_dev_warp_warp_source_app_end_to_end`
      — el escenario completo: el harness inyecta
      `dev.warp.Warp`, el cache se rellena, la fila
      persistida lleva `source_app = "dev.warp.Warp"`, el
      provider recibe ese mismo identificador y los
      diagnostics exponen `last_probe_stage = "identified"`,
      `net_active_window_seen = true`,
      `wm_class_seen = true`, `cache_populated = true`.
    - `probe_dev_warp_warp_identifier_populates_cache_and_source_app`
      — variante que confirma `cached_active_application`
      expone `identifier = "dev.warp.Warp"` y la fila
      persistida coincide.
    - `wayland_with_display_path_pins_xwayland_backend_selection`
      — pin numérico de la selección de backend
      (Wayland+DISPLAY → `XWaylandEwmh`, X11 puro →
      `X11Ewmh`).
    - `diagnostics_mirror_active_window_empty_stage` —
      `_NET_ACTIVE_WINDOW` vacío → `last_probe_stage =
      "active_window_empty"`, `net_active_window_seen =
      false`, `wm_class_seen = None`, cache vacía, fila
      con `source_app = NULL`, provider no invocado.
    - `diagnostics_mirror_wm_class_missing_stage` —
      ventana encontrada pero `WM_CLASS` ausente →
      `last_probe_stage = "wm_class_missing"`,
      `net_active_window_seen = true`, `wm_class_seen =
      false`, fila sin `source_app`, provider no invocado.
    - `diagnostics_counters_track_successful_refresh_and_attempts`
      — pin de los contadores que el usuario pidió:
      `refresh_attempts >= 1`, `successful_refreshes >=
      1`, `failed_refreshes == 0`, `cache_populated =
      true`.

  Los seis tests nuevos son host-agnósticos (usan
  `ScriptedStageActiveAppProbe` + `FakeClipboardBackend`
  + `FakeApplicationMetadataProvider`, sin requerir X
  server) y se ejecutan en `cargo test -p clipvault-app
  --lib`. Los tests pre-existentes
  `linux_capture_loop_refreshes_cache_on_every_iteration`,
  `linux_x11_capture_persists_source_app_from_refreshed_cache`,
  `linux_capture_enriches_metadata_through_provider_lookup`,
  `linux_native_wayland_does_not_get_a_fake_identifier`,
  `linux_cache_is_populated_after_loop_tick` permanecen
  gatedos a `cfg(not(target_os = "macos"))` porque
  ejercitan la rama "el helper refreshActiveApplication se
  llama en cada iteración del capture loop" — el helper
  es deliberadamente no-op en macOS, y los tests
  pre-existentes quieren validar exactamente esa rama en
  el target Linux.

- [x] 14.7 **Privacidad.**

  - `parse_active_window_id` y todos los paths de stage
    funcionan sobre datos binarios / atómicos; nunca
    tocan contenido del clipboard, snippets, hashes,
    asset_ref o rutas absolutas.
  - El diagnóstico expone únicamente categorías estables
    (`active_window_empty`, `wm_class_missing`,
    `identified`, …) y dos Booleanos derivados del stage.
  - `record_probe_stage` colapsa los stages no
    informativos (`NotApplicable`, `Unavailable`,
    `Backend`) a `None` para mantener el JSON limpio y no
    exponer la etapa cuando no aporta.

- [x] 14.8 **Verificación desde el host macOS.**

  - `cargo fmt --all -- --check` — pasa.
  - `cargo clippy --workspace --all-targets -- -D warnings`
    — pasa (los warnings preexistentes sobre `icon_sizes`
    en `linux_app_metadata.rs` y el warning de "ctypes
    sólo es portable a operating systems con `char == i8`"
    de macos permanecen, pero no son introducidos por este
    parche).
  - `cargo test --workspace` — pasa. Los 6 tests
    platform-agnostic nuevos pasan en el target del host
    (macOS). Los 5 tests preexistentes
    `linux_capture_loop_*` / `linux_native_wayland_*` /
    `linux_cache_is_populated_*` /
    `linux_x11_capture_persists_source_app_*` /
    `linux_capture_enriches_metadata_*` están gatedos a
    `cfg(not(target_os = "macos"))` y se ejecutan en el CI
    Linux / una build con `--target x86_64-unknown-linux-gnu`.
  - `cargo check -p clipvault-app --no-default-features
    --features clipboard-arboard,hotkey-global` — pasa.
  - `cargo check -p clipvault-platform --features linux-x11
    --target x86_64-unknown-linux-gnu` — pasa; el
    decoder se compila correctamente con
    `parse_active_window_id` y `reply.value32()`.
  - `cd app/tauri/frontend && npm run check` — pasa.
  - `cd app/tauri/frontend && npm run build` — pasa.
  - `cd app/tauri/frontend && npm test` — pasa (sustituido
    por `tsc --noEmit` cuando se requiere Node.js 22+, ver
    9.6).
  - `openspec validate linux-source-app-metadata --strict
    --type change` — pasa.

- [x] 14.9 **Bump de versión sincronizado a `0.0.4`** (la
  corrección del decoder `_NET_ACTIVE_WINDOW` es una
  implementación funcional completa; `projects.md` exige
  subir el patch y mantener sincronizados los manifests
  canónicos):

  - `Cargo.toml` (`[workspace.package].version`).
  - `Cargo.lock` regenerado: `clipvault-app`,
    `clipvault-core`, `clipvault-db`, `clipvault-platform`,
    `clipvault-search`.
  - `app/tauri/src-tauri/tauri.conf.json` (`version`).
  - `app/tauri/frontend/package.json` (`version`).
  - `app/tauri/frontend/package-lock.json` (`version` y la
    entrada raíz `packages.""`).
  - `projects.md` (tabla "Current canonical version" y nota
    descriptiva del bump 0.0.3 → 0.0.4).
  - `AboutModal.svelte` sigue leyendo `diagnostics.version`
    (no se hardcodea la versión en Svelte).

- [x] 14.10 **Limitación documentada.** El host actual es
  macOS, así que la confirmación runtime en Ubuntu
  GNOME Wayland + XWayland con Warp enfocada queda
  pendiente. La tarea de Ubuntu puede marcarse sólo cuando
  el usuario ejecute en una sesión real:

  ```sh
  xprop -root _NET_ACTIVE_WINDOW
  xprop -id <WINDOW_ID> WM_CLASS
  sqlite3 -header -column ~/.clipvault/clipvault.db \
    "SELECT id, source_app, source_app_name, source_app_icon_ref
       FROM clipboard_entries ORDER BY id DESC LIMIT 5;"
  ```

  y confirme que una captura nueva desde Warp produce
  `source_app = dev.warp.Warp` y, si el `.desktop`
  compatible está disponible, también
  `source_app_name` / `source_app_icon_ref`. La corrección
  del decoder y el stage granular quedan validados a
  través de los 6 tests determinísticos nuevos
  ejecutados en el host macOS.

- [x] 14.11 **Sin sync, archive, commit ni push.** La
  política "Cambio publicado + parche funcional" se
  refleja sólo en este `tasks.md`, en los manifests y en
  el código.

## 15. Instrumentación opt-in de captura (`CLIPVAULT_DEBUG_CAPTURE=1`)

- [x] 15.1 **Trait `CaptureDebugSink` y snapshots metadata-only.**
  `crates/clipvault-core/src/capture_diagnostic.rs` define
  `EnvironmentSnapshot`, `AttemptSnapshot`, `ClipboardSnapshot`,
  `ProbeSnapshot`, `CacheSnapshot`, `GateSnapshot`,
  `MetadataSnapshot`, `PersistenceSnapshot` y `OutcomeSnapshot`
  como structs serializables. Cada campo textual expone sólo
  presencia / bytes / mime; nunca el contenido del portapapeles.

- [x] 15.2 **Correlation id monotónico.** `CorrelationId`
  (`u64`, `#[serde(transparent)]`) emitido por
  `CorrelationIdAllocator` (`AtomicU64` monotónico por proceso).
  `CorrelationId::ZERO` es el sentinela para "no asignado" usado
  por los tests que llaman directo al trait.

- [x] 15.3 **`CaptureDebugSinkHandle::from_predicate`** + sink
  por defecto (`TracingCaptureDebugSink`) + sink de tests
  (`RecordingCaptureDebugSink`). El handle se construye una vez en
  `AppBootstrap::finish`: si no se inyecta, se construye
  `from_env()` y lee `CLIPVAULT_DEBUG_CAPTURE` exactamente una vez.
  El `EnvAwareSink` cortocircuita cada evento cuando el flag
  está desactivado, así el camino caliente nunca construye
  snapshots.

- [x] 15.4 **Cableado en `AppContext`.** Nuevo campo
  `capture_debug: CaptureDebugSinkHandle` (checo: barato de
  clonar, vive junto a `cached_active_app` y
  `active_app_diagnostics`). Nuevo accesor
  `AppContext::capture_debug()` y helpers
  `environment_snapshot_emitted()` /
  `mark_environment_snapshot_emitted()` que el watcher usa para
  emitir el snapshot de plataforma una sola vez por proceso.

- [x] 15.5 **`CaptureWatcher::tick(context, source_app, origin)`.**
  El watcher acepta un nuevo parámetro `AttemptOrigin`
  (`BackgroundLoop` o `ManualTick`). El thread del loop y la
  bandera `shared_watcher_used` se reportan en cada
  `attempt_start`. El correlation id se asigna una vez por tick y
  se propaga por `record_clipboard_payload_with_correlation` y
  `enrich_metadata_with_correlation` para que todos los eventos
  de un mismo intento lleven el mismo identificador.

- [x] 15.6 **`record_clipboard_payload_with_correlation`.**
  Evalúa el privacy gate, emite `privacy_gate` y `cache_state`,
  persiste y emite `persistence`. Mantenemos un overload
  `record_clipboard_payload` de 3 argumentos que delega con
  `correlation_id = None` para que los tests pre-existentes
  compilen sin cambios estructurales.

- [x] 15.7 **`ApplicationMetadataProvider::last_match_strategy` /
  `last_icon_diagnostics`.** Extensiones del trait con defaults
  seguros. El `LinuxApplicationMetadataProvider` actualiza el
  estado interno en cada lookup y la macOS provider hace lo
  propio. El sink de producción emite el evento
  `metadata_provider` con `strategy`, `display_name_resolved`,
  `icon_declared`, `icon_resolved`, `png_validated`,
  `icon_persisted`, `icon_bytes`, `icon_dimensions` y
  `error_kind`.

- [x] 15.8 **Parser de IHDR para `icon_dimensions`.** Helper
  `png_header_dimensions` que decodifica width / height del
  chunk `IHDR` en big-endian (alineado con la cabecera PNG canónica).
  Se invoca en `persist_icon` antes de renombrar el temporal para
  que el snapshot refleje las dimensiones reales.

- [x] 15.9 **Privacidad.** `assert_no_forbidden_substrings`
  itera una lista cerrada de marcadores sensibles
  (`/Users/`, `/home/`, `/tmp/.clipvault`, `asset_ref`,
  `secret-text`, `password=hunter2`, `Bearer eyJ`, hashes,
  `.desktop`, `secret-window-title`, …) y falla el test si
  cualquiera aparece. La redacción existente sigue activa para
  los mensajes de error. El test
  `json_render_does_not_include_clipboard_payload` valida que el
  contenido nunca llega al JSON del sink.

- [x] 15.10 **Tests obligatorios.** 31 tests pasan en
  `capture_diagnostic::tests` y
  `capture_diagnostic::capture_pipeline_tests`. Cubre:
  `disabled_sink_does_not_emit_any_event`,
  `enabled_sink_emits_attempt_clipboard_and_outcome_for_text_capture`,
  `correlation_id_groups_every_event_of_a_single_attempt`,
  `disabled_sink_still_persists_capture`,
  `empty_clipboard_records_ignored_outcome`,
  `duplicate_capture_records_outcome_without_persisting`,
  `clipboard_failure_records_failed_outcome_with_metadata_only_kind`,
  `cache_unavailable_records_unavailable_active_app_backend`,
  `metadata_provider_failure_does_not_convert_capture_to_failed`,
  `blacklisted_identifier_does_not_persist_capture`,
  `allowed_identifier_persists_source_app`,
  `environment_snapshot_records_metadata_only_fields`,
  `metadata_provider_records_strategy_and_icon_diagnostics`,
  `persistence_failure_records_typed_error_kind`,
  `pipeline_emits_every_event_in_documented_order`,
  `json_render_does_not_include_clipboard_payload`.

- [x] 15.11 **Version bump.** `Cargo.toml`,
  `Cargo.lock`, `app/tauri/src-tauri/tauri.conf.json`,
  `app/tauri/frontend/package.json` y
  `app/tauri/frontend/package-lock.json` se sincronizan a
  `0.0.5`. `projects.md` documenta el bump 0.0.4 → 0.0.5 con
  la regla "cada implementación funcional completa incrementa
  sólo el patch". `AboutModal.svelte` sigue leyendo
  `diagnostics.version` (no se hardcodea el literal).

- [x] 15.12 **Verificación ejecutada desde el host macOS.**
  `cargo fmt --all -- --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace`,
  `cargo check -p clipvault-platform --features linux-x11
  --target x86_64-unknown-linux-gnu --tests` pasan. Los tests del
  nuevo módulo (`capture_diagnostic`) y de los tests
  pre-existentes (`watcher`, `history`, `privacy`) confirman el
  contrato end-to-end. La validación manual en Ubuntu GNOME
  Wayland + XWayland queda como tarea del usuario.

- [x] 15.13 **Limitación documentada.** El host actual es
  macOS; las pruebas de Ubuntu X11, GNOME Wayland con app
  XWayland y GNOME Wayland con app nativa requieren una sesión
  real y quedan como tareas del usuario. La matriz
  Wayland/X11/XWayland funcional no se modifica — la
  instrumentación es estrictamente opt-in.

- [x] 15.14 **Sin sync, archive, commit ni push.** La
  política "Cambio publicado + parche funcional" se refleja
  sólo en este `tasks.md`, en los manifests y en el código.

## 16. Parche funcional post‑publicación (cfg del shell — `v0.0.5 → v0.0.6`)

- [x] 16.1 **Causa raíz confirmada.** El usuario vuelve a
  reportar `source_app = NULL` en Ubuntu GNOME Wayland +
  XWayland incluso después del parche del decoder
  `_NET_ACTIVE_WINDOW` (sección 14) y de la instrumentación
  opt‑in de captura (sección 15). La revisión de
  `app/tauri/src-tauri/src/bootstrap.rs` confirma el problema:

  ```rust
  #[cfg(all(target_os = "linux", feature = "linux-x11"))]
  OsFamily::Linux => {
      let kind = match info.display_server {
          ...
      };
      ...
      match clipvault_platform::runtime::linux_x11_active_app::
          X11ActiveApplication::with_kind(None, kind)
      { ... }
  }
  ```

  `#[cfg(all(target_os = "linux", feature = "linux-x11"))]`
  conjuga una feature que el propio crate `clipvault-app`
  nunca activa (`linux-x11` figura en su tabla `[features]`
  pero no forma parte de `default`). En cada build de Ubuntu la
  conjunción se evalúa a `false` y el brazo entero queda
  fuera del binario. El adaptador `X11ActiveApplication`
  reside en `clipvault-platform`, donde la feature `linux-x11`
  se activa automáticamente vía la dependencia target-specific
  existente (`[target.'cfg(all(target_os = "linux", not(target_os
  = "macos")))'.dependencies]`), por lo que el adaptador existe
  en el grafo pero la rama del shell que lo invoca está
  descartada.

  Resultado observable: `build_active_application` cae a
  `Arc::new(NoopActiveApplicationProbe)`, mientras
  `capabilities.active_application` permanece `true` (la matriz
  declara el intento XWayland). El diagnóstico reporta
  `probe unavailable` y `last_probe_stage` se queda en
  `started` o `unavailable`. La fila persistida lleva
  `source_app = NULL`, `source_app_name = NULL` y
  `source_app_icon_ref = NULL`. El warning
  `unused import: parse_active_window_id` confirma que parte
  del fix anterior quedó desacoplada del flujo real.

- [x] 16.2 **Corrección obligatoria 1 — `build_active_application`.**
  El brazo Linux de `build_active_application` cambia el
  atributo:

  ```rust
  #[cfg(all(target_os = "linux", feature = "linux-x11"))]
  ```

  por

  ```rust
  #[cfg(target_os = "linux")]
  ```

  Esto permite que el shell compile su adapter Linux sin
  habilitar la feature del shell como solución principal: el
  target-specific dependency sobre `clipvault-platform` ya
  activa `linux-x11` en cada build de Ubuntu.

- [x] 16.3 **Corrección obligatoria 2 — `build_paste_controller`.**
  Mismo patrón, misma corrección:

  ```rust
  #[cfg(target_os = "linux")]
  ```

  El síntoma observable era el probe activo; el paste Linux
  quedaba descartado en tiempo de compilación por el mismo
  motivo.

- [x] 16.4 **Corrección obligatoria 3 — import sobrante.**
  `parse_active_window_id` se importa en
  `crates/clipvault-platform/src/runtime/linux_x11_active_app.rs`
  pero el decoder activo del archivo usa
  `reply.value32()`, no el helper. El helper vive en
  `crates/clipvault-platform/src/active_app.rs` (compilado
  siempre) y se ejercita únicamente desde el módulo de tests
  `active_app::window_id_tests`. El import del archivo
  gated a Linux se elimina; el warning `unused_import` deja
  de aparecer y la regresión contra `reply.value.first()`
  sigue cubierta por
  `first_byte_only_is_not_what_get_property_returns`.

- [x] 16.5 **Regresiones estructurales (receta de parsing).**
  En `app/tauri/src-tauri/src/bootstrap.rs::tests` se añade
  `shell_linux_x11_cfg_does_not_require_linux_x11_feature`:
  lee `src/bootstrap.rs`, descarta comentarios `/* … */`,
  comentarios de línea `// …` (no toca literales `"…"`),
  recorre líneas y exige que `#[cfg(all(target_os = "linux",
  feature = "linux-x11"))]` no aparezca en los brazos shell
  de `build_active_application` /
  `build_paste_controller`. La prueba se ejecuta en todos los
  targets (también macOS dev host) y dispara el guard en el
  mismo commit si alguien vuelve a introducir el patrón
  prohibido. Verificado manualmente: introducir
  temporalmente `#[cfg(all(target_os = "linux", feature =
  "linux-x11"))]` en los 4 sitios del archivo hace que el test
  falle con
  `Offending lines: [1202, 1299, 1328, 1363]`.

- [x] 16.6 **Regresiones conductuales.**

  - `shell_linux_x11_adapter_attaches_dev_warp_warp_wm_class_to_source_app`:
    el probe scriptado devuelve
    `Ok(Some(ActiveApplication::new("Warp", "dev.warp.Warp")))`
    con `ProbeStage::Identified`; el watcher.tick persiste la
    captura con `source_app = "dev.warp.Warp"` y el
    `FakeApplicationMetadataProvider` ve ese identificador.
    Garantiza que el brazo Linux sigue intacto end‑to‑end
    después de la corrección del cfg.

  - `shell_native_wayland_keeps_empty_source_contract`:
    el probe scriptado devuelve `Ok(None)` con
    `ProbeStage::WmClassMissing`; la fila persistida tiene
    `source_app = NULL`, el provider NO se invoca y la caché
    queda vacía. Garantiza que la corrección del cfg no
    introduce identificadores fabricados para ventanas Wayland
    nativas.

- [x] 16.7 **Contratos preservados.**

  - `connect_to`, `connect_to_kind`, `new` y `with_kind` del
    `X11ActiveApplication` siguen contando con la aridad
    documentada; los pin tests `with_kind_signature_*`,
    `connect_to_kind_signature_*`, `connect_to_signature_*` y
    `new_signature_takes_no_arguments` siguen pasando.
  - `X11ActiveApplication::with_kind(None, kind)` con `kind`
    en `{X11, XWayland}` se construye desde el bootstrap
    exactamente como antes; `ProbeKind::X11 → "x11_ewmh"`,
    `ProbeKind::XWayland → "xwayland_ewmh"`.
  - `WM_CLASS` se sigue leyendo como `STRING` con fallback a
    `AnyPropertyType` (no se reintroduce `UTF8_STRING`).
    `_NET_WM_NAME` sigue leyéndose como `UTF8_STRING` con
    fallback a `AnyPropertyType`.
  - El identificador estable sigue siendo el segmento `class`
    de `WM_CLASS`; nunca `_NET_WM_NAME`.
  - `parse_active_window_id` sigue leyendo los cuatro bytes
    del reply X11 (`u32::from_ne_bytes`); nunca cae a
    `reply.value.first()`.
  - El `CachedActiveApplication`, el watcher (`last_hash`),
    `MainQueueInstallOutcome` y `MetadataEnrichmentScheduler`
    no cambian.
  - macOS, blacklist, imágenes, tags, colecciones, favoritos,
    Quick Paste y drag‑and‑drop de cards no se tocan.

- [x] 16.8 **Diagnóstico granular que se preserva y se
  enriquece.** La superficie serializada por
  `clipvault_active_app_diagnostics` mantiene los campos que
  diferencian, entre otros:

  - `available: bool` — adaptador realmente construido
    (diferente de `capabilities.active_application`, que es la
    capability declarada).
  - `backend: &'static str` — nombre del probe activo
    (`macos_workspace`, `x11_ewmh`, `xwayland_ewmh`,
    `unavailable`).
  - `cache_populated: bool` — la caché del probe está vacía o
    no.
  - `identifier: Option<String>` — el source identifier
    resuelto por el probe, o `None` cuando es vacío / no
    resuelto.
  - `name: Option<String>` — la etiqueta visible devuelta por
    el adaptador.
  - `last_probe_stage: Option<&'static str>` —
    `not_applicable`, `started`,
    `active_window_missing`, `active_window_empty`,
    `wm_class_missing`, `identifier_empty`, `identified`,
    `unavailable`, `backend`.
  - `net_active_window_seen: Option<bool>` —
    `_NET_ACTIVE_WINDOW` devolvió una window id parseable.
  - `wm_class_seen: Option<bool>` — `WM_CLASS` devolvió una
    payload parseable no vacía.
  - `refresh_attempts`, `successful_refreshes`,
    `failed_refreshes`, `last_refresh_unix_ms` — contadores
    monotónicos.
  - `failure_kind`, `message`, `loop_started`,
    `refresher_installed`, `timer_callback_count`,
    `last_capture_decision` — resto del contexto.

  El comando `clipvault_diagnostics` sigue exponiendo
  `capabilities.active_application` (la capability declarada).
  El usuario puede entonces combinar `capabilities.active_application`
  con `ActiveAppDiagnostics.available` /
  `ActiveAppDiagnostics.backend` /
  `ActiveAppDiagnostics.last_probe_stage` para distinguir
  con precisión: capability declarada, adapter realmente
  construido, nombre del probe activo, probe no disponible,
  caché vacía, active window encontrada, WM_CLASS encontrada,
  source identifier obtenido.

- [x] 16.9 **Privacidad.** La corrección y las regresiones
  estructurales no registran contenido del portapapeles,
  snippets, hashes, asset_ref, paths absolutos, títulos
  completos de ventanas ni secretos. La superficie prose
  trabaja sobre el AST sintáctico y los probes scriptados
  usan identificadores no sensibles. El campo `ProbeStage`
  no expone window id ni class segment.

- [x] 16.10 **Verificación desde el host macOS.**

  - `cargo fmt --all -- --check` — pasa.
  - `cargo clippy --workspace --all-targets -- -D warnings` —
    pasa (sin warnings nuevos introducidos por este parche).
  - `cargo build -p clipvault-app` — pasa.
  - `cargo test -p clipvault-app --lib -- shell_linux_x11`
    y `cargo test -p clipvault-app --lib -- shell_native` —
    pasan.
  - `cargo test -p clipvault-app --lib` — pasa; los tests
    preexistentes sobre blacklist, captura, enriquecimiento,
    identifier trimming, `ClipvaultApp::LinuxX11`,
    `metadata_enrichment_target`, hotkeys, persistence,
    `record_capture_decision` no regresan.
  - `cargo test --workspace` — pasa.
  - `cargo check -p clipvault-platform --features linux-x11
    --target x86_64-unknown-linux-gnu --tests` — pasa; el
    decoder (`reply.value32()` + `parse_active_window_id`)
    sigue compilando limpio en el target Linux.
  - `cargo check -p clipvault-app --no-default-features
    --features clipboard-arboard,hotkey-global` — pasa.
  - `cd app/tauri/frontend && npm run check` — pasa.
  - `cd app/tauri/frontend && npm run build` — pasa.
  - `cd app/tauri/frontend && npm test` —
    `tsc --noEmit` pasa (Node 20 + `--experimental-strip-types`
    sigue siendo la limitación del host).
  - `openspec validate linux-source-app-metadata --strict
    --type change` — pasa.

- [x] 16.11 **Bump de versión sincronizado a `0.0.6`**
  (corrección funcional completa del cfg del shell;
  `projects.md` exige subir el patch y mantener
  sincronizados los manifests canónicos):

  - `Cargo.toml` (`[workspace.package].version`).
  - `Cargo.lock` regenerado: `clipvault-app`,
    `clipvault-core`, `clipvault-db`, `clipvault-platform`,
    `clipvault-search`.
  - `app/tauri/src-tauri/tauri.conf.json` (`version`).
  - `app/tauri/frontend/package.json` (`version`).
  - `app/tauri/frontend/package-lock.json` (`version` y la
    entrada raíz `packages.""`).
  - `projects.md` (tabla "Current canonical version" y nota
    descriptiva del bump `0.0.5 → 0.0.6`).
  - `AboutModal.svelte` sigue leyendo `diagnostics.version`
    (no se hardcodea la versión en Svelte).

- [x] 16.12 **Limitación documentada.** El host actual es
  macOS; la confirmación runtime de una sesión Ubuntu GNOME
  Wayland + XWayland con `dev.warp.Warp` enfocada queda
  pendiente de una sesión real. La matriz
  X11/XWayland/Wayland nativo se valida estructuralmente
  con `cargo check -p clipvault-platform --features linux-x11
  --target x86_64-unknown-linux-gnu --tests` y con las
  regresiones conductuales con `ScriptedStageActiveAppProbe`
  que se ejecutan en el host del dev. Los nuevos tests son
  host‑agnósticos (no requieren X server real) y se ejecutan
  tanto en macOS como en el CI de Linux. Las tareas del
  usuario en una sesión real de Ubuntu quedan descritas,
  pero NO se marcan desde macOS ni por inferencia.

- [x] 16.13 **Sin sync, archive, commit ni push.** La
  política "Cambio publicado + parche funcional" se
  refleja sólo en este `tasks.md`, en los manifests y en
  el código.

## 17. Parche funcional post‑publicación (iconos de Linux — `v0.0.6 → v0.0.7`)

- [x] 17.1 **Causa raíz confirmada.** El usuario vuelve a
  reportar `source_app_icon_ref = NULL` en Ubuntu GNOME
  Wayland + XWayland (también X11 puro, también Fedora /
  Arch / openSUSE con GNOME o KDE) incluso después del
  parche del cfg del shell (sección 16). La inspección de
  `crates/clipvault-platform/src/runtime/linux_app_metadata.rs`
  confirma tres regresiones:

  1. `collect_icon_root_layout()` ya devolvía los data
     roots correctos (`/usr/share/icons`,
     `/usr/local/share/icons`,
     `$HOME/.local/share/icons`), pero
     `collect_icon_dirs()` volvía a concatenar `/icons` y
     producía rutas inexistentes como
     `/usr/share/icons/icons/...`, saltándose el layout
     canónico
     `/usr/share/icons/hicolor/48x48/apps/<icon>.png` /
     `/usr/share/icons/hicolor/scalable/apps/<icon>.svg`.
  2. Cuando `XDG_DATA_HOME` no estaba definido,
     `collect_application_dirs()` buscaba
     `$HOME/applications` en lugar de
     `$HOME/.local/share/applications`.
  3. El resolver sólo aceptaba PNG; la mayoría de los
     iconos distribuidos por los paquetes oficiales viven
     como SVG, especialmente en `scalable/`.

- [x] 17.2 **Refactor de raíces XDG.** Nueva función
  `data_roots(fs)` en `linux_app_metadata.rs`:

  - `XDG_DATA_HOME` si está definido; en caso contrario
    `$HOME/.local/share`.
  - Cada ruta de `XDG_DATA_DIRS`.
  - Si `XDG_DATA_DIRS` está vacío, `/usr/local/share` y
    `/usr/share`.
  - Fallbacks opcionales que se incluyen sólo cuando
    existen en disco y sin desplazar los XDG_DATA_DIRS:
    `~/.local/share/flatpak/exports/share`,
    `/var/lib/flatpak/exports/share`,
    `/var/lib/snapd/desktop`,
    `/run/current-system/sw/share`.
  - Deduplicado, normalizado, orden determinista.

  A partir de estos `data_roots` se derivan los
  directorios `<root>/applications`,
  `<root>/icons/<theme>/<size>x<size>/apps/`,
  `<root>/icons/<theme>/scalable/apps/`,
  `<root>/icons/<size>x<size>/apps/` (layout legacy) y
  `<root>/pixmaps`. Cada segmento se concatena
  exactamente una vez. `collect_icon_dirs` deja de
  existir; `collect_application_dirs` se reduce a un
  filtrado `is_dir` sobre los derivados.

- [x] 17.3 **Resolución de iconos ampliada.** El resolver
  recorre, en orden determinista, los tamaños
  `16, 22, 24, 32, 48, 64, 96, 128, 256` y los formatos
  `.png` y `.svg`. PNG tiene prioridad sobre SVG cuando
  ambos existen. Para `Icon=/ruta/absoluta`, la ruta se
  canonicaliza, se exige que sea un archivo regular y que
  viva bajo una raíz permitida; los symlinks que escapan
  de las raíces se rechazan. El layout legacy
  `<root>/icons/<size>x<size>/apps/` se conserva.

- [x] 17.4 **Soporte SVG vía `resvg`.** Nuevo módulo
  `crates/clipvault-platform/src/runtime/linux_svg_raster.rs`
  detrás de la feature `linux-svg-raster`:

  - `resvg = { version = "0.45", default-features = false }`
    como dependencia del workspace y de
    `clipvault-platform`.
  - `usvg::ImageHrefResolver` se cablea con closures que
    devuelven `None` para datos y paths, de modo que
    ningún recurso externo puede cargarse (no se cargan
    archivos locales, no se hacen llamadas de red).
  - Cap del byte length a `MAX_SVG_BYTES` (4 MB).
  - Cap de las dimensiones de origen a
    `MAX_SVG_SOURCE_DIM` (1024 × 1024).
  - El rasterizador escala el resultado a
    `MAX_ICON_DIM` × `MAX_ICON_DIM` (256 × 256)
    preservando proporción y transparencia.
  - El PNG se valida por la firma canónica antes de
    persistirse.
  - Nunca invoca `convert`, `magick`, `gio` ni ningún
    proceso externo; nunca abre la red; nunca ejecuta
    JavaScript / scripting SVG.

  El PNG se persiste en el namespace existente
  `~/.clipvault/assets/application-icons/<safe-id>.png`
  y la referencia se conserva relativa
  (`application-icons/<safe-id>.png`). El icono sigue
  siendo siempre PNG — el SVG sólo se usa como entrada y
  no se persiste en disco.

- [x] 17.5 **`IconDiagnostics` extendido.** Se agregan
  los campos:

  - `kind: IconSourceKind` (`png`, `svg`, `pixmap`,
    `unknown`, `none`).
  - `rasterization_attempted: bool`.
  - `rasterization_succeeded: bool`.
  - `failure_kind: IconFailureKind` (`not_declared`,
    `not_found`, `out_of_roots`, `invalid_png`,
    `invalid_svg`, `svg_rejected`,
    `rasterization_failed`, `write_error`, `none`).

  Los strings `as_str()` son el contrato estable que el
  sink `capture_debug metadata_provider` consume. La
  superficie nunca expone rutas absolutas, contenido del
  portapapeles, snippets, hashes, `asset_ref`, títulos de
  ventana ni secretos.

  `MetadataSnapshot` (en
  `crates/clipvault-core/src/capture_diagnostic.rs`)
  expone los mismos campos (`icon_kind`,
  `rasterization_attempted`,
  `rasterization_succeeded`, `icon_failure_kind`) sin
  romper la deserialización existente.

  Los consumidores de macOS y `FakeApplicationMetadataProvider`
  se actualizan para reflejar el nuevo snapshot:
  `IconSourceKind::Png` con `IconFailureKind::None` por
  defecto, `IconFailureKind::WriteError` cuando el
  filesystem rechaza la escritura.

- [x] 17.6 **Persistencia y backfill.** `write_icon_atomic`
  sigue creando el directorio `application-icons/` sólo
  cuando un icono válido está disponible, escribe el
  temporal en el mismo directorio, valida el PNG, hace
  `sync_all` y renombra atómicamente; en cualquier error
  elimina el temporal. Nunca reemplaza un icono existente
  por una respuesta vacía. El backfill de filas con
  `source_app` y `source_app_name` pero
  `source_app_icon_ref = NULL` re‑corre el provider en el
  siguiente arranque y persiste el icono cuando ahora
  puede resolverlo. No se borran ni renombran assets
  existentes.

- [x] 17.7 **Tests obligatorios.** Se añaden en
  `crates/clipvault-platform/src/runtime/linux_app_metadata.rs`
  (módulo `tests`) y en
  `crates/clipvault-platform/tests/linux_app_metadata.rs`:

  - `data_roots_falls_back_to_local_share_when_xdg_data_home_is_unset`
  - `data_roots_respects_xdg_data_home_when_set`
  - `data_roots_deduplicates_entries`
  - `data_roots_appends_usr_share_fallback_when_xdg_data_dirs_empty`
  - `collect_icon_apps_dirs_never_produces_duplicated_icons_segment`
    (guard explícito anti-`<root>/icons/icons/...`)
  - `collect_icon_apps_dirs_walks_hicolor_theme`
  - `collect_icon_apps_dirs_walks_scalable_theme_layout`
  - `collect_icon_apps_dirs_walks_pixmaps_namespace`
  - `resolve_icon_prefers_png_over_svg_for_the_same_icon`
  - `resolve_icon_falls_back_to_svg_when_only_svg_is_present`
    (cuando la feature `linux-svg-raster` está activa)
  - `resolve_icon_reports_invalid_svg_failure`
  - `lookup_falls_back_to_pixmaps_layout`
  - `lookup_with_no_icon_declared_keeps_display_name`
  - `lookup_with_wm_class_distinct_from_display_name_matches_via_startup_wm_class`
    (cubre Warp sin hardcodearlo)
  - `lookup_with_source_app_distinct_from_visible_name_resolves_metadata`
  - `lookup_with_arbitrary_application_resolves_name`
  - `lookup_creates_application_icons_only_on_success`
  - `lookup_reports_out_of_roots_failure_for_absolute_paths`
  - `resolves_multiple_distinct_applications`
  - `icon_not_found_failure_is_typed`
  - `icon_outside_roots_is_rejected`
  - `svg_only_icon_is_rasterized_and_persisted`
  - `malformed_svg_records_invalid_svg_failure`
  - `png_icon_is_preferred_over_svg`
  - `pixmaps_layout_is_supported`
  - `yaru_theme_layout_is_supported`
  - `adwaita_theme_layout_is_supported`
  - `multiple_icon_sizes_resolve`
  - `symlink_outside_root_is_rejected`
  - `existing_icon_is_preserved_when_lookup_fails`
  - `application_icons_dir_only_created_on_persistence`

  En `crates/clipvault-core/src/capture_diagnostic.rs`:

  - `metadata_snapshot_from_provider_propagates_icon_dimensions`
    (extendido para `kind`, `rasterization_attempted`,
    `icon_failure_kind`).
  - `metadata_snapshot_propagates_svg_rasterization_diagnostics`.
  - `metadata_snapshot_propagates_typed_icon_failure`.

  Los tests del módulo `runtime/linux_svg_raster.rs`
  incluyen rasterización de un SVG mínimo, rechazo de
  payload demasiado grande, rechazo de SVG malformado,
  cap del output a `MAX_ICON_DIM`, rechazo de dimensiones
  de origen mayores a `MAX_SVG_SOURCE_DIM` y descarte de
  recursos `xlink:href` (la rasterización no carga
  archivos ni URLs externos).

- [x] 17.8 **Verificación ejecutada desde el host macOS.**

  - `cargo fmt --all -- --check` — pasa.
  - `cargo clippy --workspace --all-targets -- -D warnings`
    — pasa.
  - `cargo clippy -p clipvault-platform --features
    linux-svg-raster -- -D warnings` — pasa.
  - `cargo test --workspace` — pasa. Los tests del módulo
    Linux están gated a `cfg(target_os = "linux")` y se
    ejecutan en el CI de Linux o en una build cross-
    compilada desde macOS.
  - `cargo check -p clipvault-platform --features
    linux-svg-raster --target x86_64-unknown-linux-gnu
    --tests` — pasa; el rasterizador y el provider Linux
    compilan en el target Ubuntu con la feature
    `linux-svg-raster`.
  - `cargo check -p clipvault-platform --features
    linux-svg-raster,linux-x11 --target
    x86_64-unknown-linux-gnu --tests` — pasa; el binario
    Ubuntu completo (X11 + SVG) compila limpio.
  - `cd app/tauri/frontend && npm run check` — pasa.
  - `cd app/tauri/frontend && npm run build` — pasa.
  - `openspec validate linux-source-app-metadata --strict
    --type change` — pasa (`Change
    'linux-source-app-metadata' is valid`).

- [x] 17.9 **Bump de versión sincronizado a `0.0.7`** (la
  corrección de los iconos es una implementación funcional
  completa: `projects.md` exige subir el patch y mantener
  sincronizados los manifests canónicos):

  - `Cargo.toml` (`[workspace.package].version`).
  - `Cargo.lock` regenerado: `clipvault-app`,
    `clipvault-core`, `clipvault-db`, `clipvault-platform`,
    `clipvault-search`.
  - `app/tauri/src-tauri/tauri.conf.json` (`version`).
  - `app/tauri/frontend/package.json` (`version`).
  - `app/tauri/frontend/package-lock.json` (`version` y la
    entrada raíz `packages.""`).
  - `projects.md` (tabla "Current canonical version" y nota
    descriptiva del bump `0.0.6 → 0.0.7`).
  - `AboutModal.svelte` sigue leyendo `diagnostics.version`
    (no se hardcodea la versión en Svelte).

- [x] 17.10 **Limitación documentada.** El host actual es
  macOS, así que la confirmación runtime de una sesión
  Ubuntu real (X11, GNOME Wayland con app XWayland, GNOME
  Wayland nativo, Fedora, Arch, openSUSE, KDE) queda
  pendiente del usuario. La matriz de XDG icon roots se
  valida estructuralmente con el guard
  `collect_icon_apps_dirs_never_produces_duplicated_icons_segment`
  y con los tests determinísticos sobre `MemoryFilesystem`
  que se invocan en `cargo test --workspace` cuando el
  target es Linux. Las tareas de Ubuntu (10.1–10.6) NO
  se marcan desde macOS ni desde tests sin display.

- [x] 17.11 **Sin sync, archive, commit ni push.** La
  política "Cambio publicado + parche funcional" se
  refleja sólo en este `tasks.md`, en los manifests y en
  el código.

## 18. Parche funcional post‑publicación (shell sin `linux-svg-raster` — `v0.0.7 → v0.0.8`)

- [x] 18.1 **Causa raíz confirmada.** El usuario vuelve a
  reportar `source_app_icon_ref = NULL` y la ausencia del
  directorio `~/.clipvault/assets/application-icons/` en
  Ubuntu GNOME Wayland + XWayland (y también en X11 puro,
  Fedora, Arch, openSUSE) incluso después del parche de
  iconos (`v0.0.6 → v0.0.7`). La inspección de
  `app/tauri/src-tauri/Cargo.toml` confirma el problema:
  la dependencia target-specific de Linux sobre
  `clipvault-platform` lista `clipboard-arboard`,
  `hotkey-global` y `linux-x11`, pero olvida
  `linux-svg-raster`. El rasterizador `resvg` y el módulo
  `linux_svg_raster.rs` ya viven dentro de
  `clipvault-platform` desde el commit `e8db63a`, pero el
  binario Ubuntu real nunca los enlaza. En
  `runtime/linux_app_metadata.rs`, la rama SVG del
  resolver cae al fallback
  `Err(IconFailureKind::SvgRejected)`:

  ```rust
  #[cfg(not(feature = "linux-svg-raster"))]
  {
      let _ = resolved;
      Err(IconFailureKind::SvgRejected)
  }
  ```

  Resultado observable: cualquier `.desktop` cuyo `Icon=`
  resuelva a un SVG (Ubuntu, Debian, Fedora, Arch,
  openSUSE, GNOME, KDE distribuyen la mayoría de iconos
  como SVG en `scalable/apps/<name>.svg`) produce
  `SvgRejected`, no escribe PNG, `source_app_icon_ref`
  queda `NULL` y la card rail renderiza el fallback
  genérico. El bug es invisible desde el host macOS
  porque:

  1. El módulo `linux_app_metadata` está gated a
     `cfg(target_os = "linux")`, así que la suite macOS
     nunca lo compila.
  2. `cargo check -p clipvault-platform --features
     linux-svg-raster --target x86_64-unknown-linux-gnu
     --tests` valida la rama Linux aislada, pero no la
     configuración real del binario `clipvault-app`.
  3. `cargo check -p clipvault-app --target
     x86_64-unknown-linux-gnu` pasa porque el shell
     compila; simplemente no enlaza el código que
     `linux-svg-raster` aporta.

- [x] 18.2 **Corrección.** Añadir `linux-svg-raster` a la
  lista de features de la dependencia target-specific de
  Linux en `app/tauri/src-tauri/Cargo.toml`:

  ```toml
  [target.'cfg(all(target_os = "linux", not(target_os = "macos")))'.dependencies]
  clipvault-platform = { path = "../../../crates/clipvault-platform", features = [
      "clipboard-arboard",
      "hotkey-global",
      "linux-x11",
      "linux-svg-raster",
  ] }
  ```

  No se modifica la lista `default = [...]` del shell:
  la feature `linux-svg-raster` pertenece a la plataforma
  y se activa por dependencia target-specific (igual que
  `linux-x11` desde el parche anterior). El shell no
  necesita declararla en su propia tabla `[features]`.

- [x] 18.3 **Regresión estructural en el shell.** Nuevo
  test `shell_linux_svg_raster_feature_is_enabled_for_linux_target`
  en `app/tauri/src-tauri/src/bootstrap.rs::tests`:

  - Parsea el `Cargo.toml` del shell, localiza la tabla
    `[target.'cfg(all(target_os = "linux", not(target_os
    = "macos")))'.dependencies]` y exige que
    `linux-svg-raster` esté en la lista de features del
    `clipvault-platform` inline-table.
  - El parser es un walker TOML mínimo (no añade
    dependencia nueva): busca la cabecera, avanza hasta
    la siguiente `[ ... ]`, balancea las llaves del
    inline-table, extrae la lista y la divide por comas.
  - El test corre en macOS, Linux y CI (no usa
    `cfg(target_os = …)`) porque el bug es estrictamente
    sintáctico del manifest del shell. Cualquier
    refactor futuro que elimine la feature falla con la
    lista observada.

- [x] 18.4 **Regresión funcional del rasterizador.** Nuevo
  test `svg_only_icon_persists_png_under_application_icons`
  en `crates/clipvault-platform/tests/linux_app_metadata.rs`:

  - Comprueba que un `.desktop` cuyo `Icon=` resuelve
    únicamente a un SVG produce un PNG persistido bajo
    `<data_dir>/assets/application-icons/<safe-id>.png`
    con la firma canónica.
  - Valida además que la referencia `icon_ref` sea
    relativa, que `IconDiagnostics` reporte
    `kind = Svg`, `rasterization_attempted = true`,
    `rasterization_succeeded = true` y
    `failure_kind = None`.
  - El test es genérico (no hardcodea Warp ni Ubuntu) y
    está gated a `cfg(feature = "linux-svg-raster")`.
    Combinado con el test estructural del shell, los
    dos cubren tanto el wiring de la feature como el
    camino real que el binario enlaza.

  El test pre-existente
  `svg_only_icon_is_rasterized_and_persisted` se
  conserva como cobertura del rasterizador; el nuevo
  test documenta explícitamente la regresión del shell
  y comprueba el contrato de persistencia exacto:
  ruta relativa bajo `application-icons/`, firma PNG
  válida, diagnósticos completos.

- [x] 18.5 **Contratos preservados.**

  - `default = [...]` del shell sigue siendo
    `["custom-protocol", "clipboard-arboard",
    "hotkey-global"]`; no se añade `linux-x11` ni
    `linux-svg-raster` para no enmascarar regresiones
    futuras de tipo "el shell olvidó habilitar la
    feature".
  - macOS, Wayland nativo, blacklist, captura, imágenes,
    tags, colecciones, favoritos, Quick Paste y
    drag-and-drop de cards no se tocan: el cambio está
    limitado al bloque target-specific de Linux en el
    `Cargo.toml` del shell y a las dos regresiones
    nuevas.
  - El contrato `source_app` no cambia. El contrato
    `source_app_icon_ref` se llena correctamente con
    SVG-only icons, igual que ya lo hacía con PNG /
    pixmap.
  - `LinuxApplicationMetadataProvider`, el bridge de
    iconos (`application-icons/`), `icon_ref_for` y el
    validador PNG existente no cambian su contrato
    público.

- [x] 18.6 **Privacidad y seguridad.**

  - La feature `linux-svg-raster` mantiene
    `default-features = false` en `resvg`: no se carga
    texto, no se enumeran fuentes del host, no se
    decodifican imágenes raster externas, no se abre la
    red, no se ejecuta JavaScript ni scripting SVG.
  - Los `ImageHrefResolver` siguen cableados con closures
    que devuelven `None` para datos y paths; los
    límites `MAX_SVG_BYTES` (4 MB) y `MAX_SVG_SOURCE_DIM`
    (1024 × 1024) permanecen activos.
  - Las regresiones no escriben en `~/.clipvault`, no
    registran contenido del clipboard, snippets, hashes,
    `asset_ref`, paths absolutos ni secretos. El parser
    del `Cargo.toml` opera sobre el AST sintáctico y los
    nombres de features; el test funcional usa el
    `MemoryFilesystem` con un directorio temporal
    controlado por el harness.

- [x] 18.7 **Verificación ejecutada desde el host macOS.**

  - `cargo fmt --all -- --check` — pasa.
  - `cargo clippy --workspace --all-targets -- -D
    warnings` — pasa (sin warnings nuevos introducidos
    por este parche).
  - `cargo test --workspace` — pasa. Los tests nuevos
    `shell_linux_svg_raster_feature_is_enabled_for_linux_target`
    y `svg_only_icon_persists_png_under_application_icons`
    se ejecutan en el target del host (el primero en
    todos los targets; el segundo en Linux con la
    feature `linux-svg-raster` activa).
  - `cargo check -p clipvault-platform --features
    linux-x11,linux-svg-raster --target
    x86_64-unknown-linux-gnu --tests` — pasa; el
    binario Ubuntu completo (X11 + SVG) compila
    limpio.
  - `cd app/tauri/frontend && npm run check` — pasa.
  - `cd app/tauri/frontend && npm run build` — pasa.
  - `openspec validate linux-source-app-metadata
    --strict --type change` — pasa.

- [x] 18.8 **Bump de versión sincronizado a `0.0.8`**
  (la corrección del shell es una implementación
  funcional completa: `projects.md` exige subir el patch
  y mantener sincronizados los manifests canónicos):

  - `Cargo.toml` (`[workspace.package].version`).
  - `Cargo.lock` regenerado: `clipvault-app`,
    `clipvault-core`, `clipvault-db`,
    `clipvault-platform`, `clipvault-search`.
  - `app/tauri/src-tauri/tauri.conf.json` (`version`).
  - `app/tauri/frontend/package.json` (`version`).
  - `app/tauri/frontend/package-lock.json` (`version` y
    la entrada raíz `packages.""`).
  - `projects.md` (tabla "Current canonical version" y
    nota descriptiva del bump `0.0.7 → 0.0.8`).
  - `AboutModal.svelte` sigue leyendo
    `diagnostics.version` (no se hardcodea el literal).

- [x] 18.9 **Limitación documentada.** El host actual es
  macOS, así que la confirmación runtime de una sesión
  Ubuntu real (X11, GNOME Wayland con app XWayland,
  Fedora, Arch, openSUSE) queda pendiente del usuario.
  Las tareas de Ubuntu (10.1–10.6) NO se marcan desde
  macOS ni desde tests sin display; el guard del parser
  del `Cargo.toml` y los tests determinísticos sobre
  `MemoryFilesystem` validan estructuralmente el
  arreglo. `cargo check -p clipvault-platform --features
  linux-x11,linux-svg-raster --target
  x86_64-unknown-linux-gnu --tests` actúa como smoke
  test de la rama Linux completa sobre el toolchain
  del dev host.

- [x] 18.10 **Sin sync, archive, commit ni push.** La
  política "Cambio publicado + parche funcional" se
  refleja sólo en este `tasks.md`, en los manifests y
  en el código.
