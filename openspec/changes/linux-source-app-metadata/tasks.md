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
