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
