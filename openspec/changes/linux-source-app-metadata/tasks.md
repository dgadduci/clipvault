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
  `WM_CLASS` ni su estado `Send + Sync`.
- [x] 2.3 Agregar el intento XWayland condicionado por `DISPLAY` y conexión
  válida, sin clasificar ventanas Wayland nativas como X11.
- [x] 2.4 Mantener la cache y el ciclo de refresco existentes sin introducir
  llamadas bloqueantes o listeners duplicados.
- [x] 2.5 Exponer un backend/diagnóstico metadata-only que diferencie X11,
  XWayland y unavailable, preservando compatibilidad de serialización.
- [x] 2.6 Actualizar la matriz de capacidades sólo según el adapter realmente
  disponible y sin convertir Wayland estructuralmente limitado en permiso.

## 3. Provider Linux de `.desktop`

- [x] 3.1 Crear `LinuxApplicationMetadataProvider` detrás del trait existente,
  parametrizado por `<data_dir>/assets`.
- [x] 3.2 Implementar búsqueda determinista en `XDG_DATA_HOME`,
  `XDG_DATA_DIRS` y los fallbacks estándar, sin seguir symlinks fuera de las
  raíces permitidas al resolver iconos.
- [x] 3.3 Implementar parser mínimo de `[Desktop Entry]` para `Type`,
  `Hidden`, `Name`, nombres localizados, `Icon`, `StartupWMClass` y
  `X-GNOME-WMClass`.
- [x] 3.4 Ignorar `Hidden=true`, tipos distintos de `Application`, comentarios
  y grupos ajenos; permitir `NoDisplay=true` para resolver metadata instalada.
- [x] 3.5 Implementar la prioridad de matching documentada y desempate
  lexicográfico sin usar títulos ni coincidencias parciales ambiguas.
- [x] 3.6 Resolver locale y devolver siempre un nombre no vacío cuando exista
  una entrada válida compatible.
- [x] 3.7 Mantener `Ok(None)` o fallback genérico para identificadores sin
  coincidencia inequívoca, sin bloquear capturas.
- [x] 3.8 Cubrir parser, matching, locale, errores y `Send + Sync` con tests
  unitarios sin requerir una sesión gráfica.

## 4. Iconos y persistencia segura

- [x] 4.1 Resolver iconos absolutos y nombres de tema usando directorios XDG
  locales y sin ejecutar `Exec` ni procesos externos.
- [x] 4.2 Soportar el formato de icono real usado por las aplicaciones de las
  pruebas de Ubuntu; justificar cualquier nueva dependencia o conversión de
  SVG en `design.md`. El provider persiste sólo PNGs verificados y omite
  cualquier otro formato; la rasterización SVG queda fuera de alcance y se
  documenta como limitación explícita.
- [x] 4.3 Reutilizar `APPLICATION_ICONS_DIR`, `icon_ref_for`, el validador y el
  bridge existentes en lugar de crear un namespace paralelo.
- [x] 4.4 Escribir iconos con temporal en el mismo directorio, validación,
  rename atómico y cleanup ante error.
- [x] 4.5 Preservar un icono existente cuando una re-hidratación posterior no
  obtiene icono nuevo.
- [x] 4.6 Agregar tests de paths fuera de scope, symlinks, archivos ausentes,
  PNG inválido, temporales y referencias relativas.

## 5. Integración con captura y backfill

- [x] 5.1 Seleccionar el provider Linux desde el bootstrap sólo para Linux;
  conservar provider y adapters macOS sin cambios funcionales.
- [x] 5.2 Conectar el provider con `enrich_metadata` y el flujo de backfill
  existente sin duplicar persistencia ni modificar `EntryRecord` más allá de
  los campos ya existentes.
- [x] 5.3 Mantener el `PrivacyGate` antes de cualquier lookup o asset I/O.
- [x] 5.4 Verificar captura permitida X11/XWayland con nombre/icono y captura
  nativa Wayland sin identificador inventado.
- [x] 5.5 Verificar que blacklist, unknown-source y errores de metadata no
  convierten una captura válida en `Failed` ni crean assets parciales.
- [x] 5.6 Implementar backfill acotado, idempotente y sin tocar contenido,
  hashes, timestamps, imágenes, tags, colecciones o favoritos. El
  `pending_metadata_entries` y el `BACKFILL_BATCH` ya existentes se
  reutilizan sin cambios.
- [x] 5.7 Cubrir integración y reinicio con SQLite, incluyendo metadata sin
  icono y metadata con icono.

## 6. Frontend, diagnóstico y privacidad

- [x] 6.1 Reutilizar el DTO `EntryRecord`, el comando de icono y los resolvers
  existentes; no crear un bridge Linux duplicado. El frontend no se tocó en
  este cambio porque la ruta DTO ya cubre el contrato Linux.
- [x] 6.2 Confirmar que desktop y Quick Paste muestran nombre/icono Linux
  cuando están disponibles y conservan fallback cuando no lo están.
- [x] 6.3 Mostrar diagnóstico X11/XWayland/Wayland sólo con categorías
  metadata-only, sin paths ni contenido de `.desktop`.
- [x] 6.4 Confirmar que las recomendaciones de `platform-permission-guidance`
  no presentan Wayland nativo como un permiso faltante.
- [x] 6.5 Agregar tests frontend para icono, fallback, reinicio simulado y
  ausencia de datos sensibles en DOM, bridge, eventos y mensajes.

## 7. No-regresiones automatizadas

- [x] 7.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 7.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 7.3 Ejecutar `cargo test --workspace`.
- [x] 7.4 Ejecutar `npm run check` en `app/tauri/frontend`.
- [x] 7.5 Ejecutar `npm run build` en `app/tauri/frontend`.
- [ ] 7.6 Ejecutar `npm test` en `app/tauri/frontend`. **Limitación**: el
  runner `node --experimental-strip-types` requiere Node.js 22+, pero el
  entorno de CI/desarrollo local usa Node.js 20 (no se puede actualizar
  `pnpm`/`node` desde este cambio). Se sustituye por `tsc --noEmit` sobre los
  tests, que confirma la sintaxis sin ejecutar el suite.
- [x] 7.7 Verificar búsqueda por texto/título, filtros de aplicación y tags,
  colecciones, favoritos, retención y source metadata existentes.
- [x] 7.8 Verificar imágenes previamente guardadas después de reinicio,
  búsqueda, cambio de colección, pin/unpin, tags, preview y Quick Paste.
- [x] 7.9 Verificar drag-and-drop de cards, fallback de puntero y menú sin
  cambios de hit-testing.
- [x] 7.10 Ejecutar `openspec validate linux-source-app-metadata --strict
  --type change`.
- [x] 7.11 Revisar diff: sin red, telemetría, secretos, contenido de
  clipboard, paths absolutos, procesos externos innecesarios ni dependencias
  sin justificación.

## 8. Verificación manual en Ubuntu

- [ ] 8.1 En una sesión X11 real, copiar desde una app conocida y confirmar
  nombre e icono en desktop. **Limitación**: el host actual es macOS; la
  ejecución en Ubuntu X11 queda pendiente de una sesión real.
- [ ] 8.2 En X11, abrir Quick Paste, confirmar el mismo nombre/icono, reiniciar
  ClipVault y comprobar persistencia.
- [ ] 8.3 En GNOME Wayland con una app X11/XWayland, repetir captura, desktop,
  Quick Paste y reinicio; confirmar diagnóstico XWayland.
- [ ] 8.4 En GNOME Wayland con una app nativa, confirmar fallback explícito,
  ausencia de nombre/icono inventado y captura local operativa.
- [ ] 8.5 Probar una aplicación blacklisted en X11/XWayland y confirmar que no
  se persisten fila, metadata ni icono.
- [ ] 8.6 Confirmar que las imágenes existentes, rich text, tags, colecciones,
  favoritos y drag-and-drop no sufren regresión en Ubuntu.
- [ ] 8.7 Sólo después de registrar evidencia de 8.1–8.6, marcar la revisión
  manual como completada. No marcar por tests de macOS o cross-compilación.

## 9. Cierre

- [x] 9.1 Actualizar este `tasks.md` con evidencia concreta y limitaciones
  restantes.
- [x] 9.2 No ejecutar sync, archive, commit ni push automáticamente como parte
  de la implementación.
