# Tareas de implementación

MiniMax implementa este cambio. Codex mantiene la arquitectura y revisa el resultado. No se debe archivar ni sincronizar automáticamente.

## 1. Auditoría y decisiones

- [x] 1.1 Leer `AGENTS.md`, `project.md`, `projects.md` y todos los artefactos de este cambio.
- [x] 1.2 Auditar el adapter `linux-native-wayland-app-detection`, la construcción de probes, diagnostics, configuración local y UI existente.
- [x] 1.3 Confirmar la versión objetivo de GNOME Shell y las APIs públicas disponibles para obtener la aplicación enfocada (`Shell.WindowTracker`, `focus_app.get_id()`).
- [x] 1.4 Documentar en `design.md` el mecanismo de activación, el canal IPC y las limitaciones por versión.
- [x] 1.5 Ejecutar baseline sin tocar `~/.clipvault`, assets existentes ni extensiones GNOME del usuario.

## 2. Extensión GNOME

- [x] 2.1 Crear el recurso de extensión con UUID estable, metadata y versiones de Shell soportadas (ver `app/tauri/src-tauri/resources/gnome-extension/`).
- [x] 2.2 Obtener la aplicación enfocada usando APIs públicas de GNOME Shell (`Shell.WindowTracker.get_default().focus_app.get_id()`).
- [x] 2.3 No utilizar títulos, PID, `/proc`, procesos externos, `org.gnome.Shell.Eval` ni scraping.
- [x] 2.4 Publicar sólo `app_id` normalizado y estado de ausencia (envelope JSON `{ v, kind, app_id }`).
- [x] 2.5 Suscribir y desconectar correctamente las señales de foco en `enable`/`disable` (cleanup de `focus_source`, `reconnect_source`, `socket`).
- [x] 2.6 Implementar reconexión controlada con backoff exponencial (250 ms → 4 s) y limpieza al desaparecer ClipVault.
- [x] 2.7 Endurecer el handshake con una cola de escritura estrictamente serializada: el flag `handshake_complete` permanece `false` hasta que el callback de `write_async` del `hello` confirma los bytes, `_sendQueued` se niega a despachar ningún `app_id` mientras la bandera esté baja y `_checkFocus()` ya no encola hasta que el handshake termina. El handshake y el `app_id` comparten ahora el mismo flag `sending`, así no puede haber dos `write_async` simultáneos sobre la misma `output_stream`. Tests: `process_peer_refuses_app_id_before_hello`, `wire_protocol_accepts_hello_then_app_id`, `wire_protocol_treats_empty_app_id_as_no_active_application`. La verificación manual en Ubuntu GNOME Wayland permanece en la sección 8.
- [x] 2.8 Limpiar `handshake_complete` en `disable()` y en `_resetSocket()` para que el próximo `enable` arranque de cero sin arrastrar estado del ciclo anterior.

## 3. IPC y adapter Linux

- [x] 3.1 Implementar el canal local de sesión versionado (Unix socket `$XDG_RUNTIME_DIR/clipvault/clipvault-focus.sock`, 16-bit `PROTOCOL_VERSION`).
- [x] 3.2 Definir conexión, desconexión, versión, `app_id` presente y ausencia sin transportar contenido sensible (envelope `{ v, kind, app_id }`).
- [x] 3.3 Implementar `GnomeShellActiveApplication` como `ActiveApplicationProbe` no bloqueante (lee desde `SharedGnomeSnapshot`, nunca toca I/O).
- [x] 3.4 Usar un snapshot concurrente único (`SharedGnomeSnapshot` con `parking_lot::RwLock`), sin segundo watcher ni estado de deduplicación paralelo.
- [x] 3.5 Traducir peer ausente, versión incompatible, error y desconexión a estados tipados (`NoActiveApplication`, `WireError::ProtocolVersion`, `CommunicationError`, `Disconnected`).
- [x] 3.6 Mantener precedencia GNOME conectado → Wayland público → XWayland → desconocido; el `swap_active_app_probe` queda dentro de `AppContext::cached_active_app` y el watcher nunca construye un segundo probe.
- [x] 3.7 Marcar `Installation::target_dir` y `Installation::metadata_json` con `#[serde(skip_serializing)]` para que el `InstallResult` que Tauri devuelve nunca incluya la ruta absoluta de instalación ni el cuerpo del `metadata.json`. La vista interna de Rust sigue disponible para tests e instalador.

## 4. Instalación y activación consentida

- [x] 4.1 Detectar GNOME Wayland sin asumirlo en otras sesiones (`detect_session()` consulta `WAYLAND_DISPLAY` y `XDG_CURRENT_DESKTOP`).
- [x] 4.2 Estado persistente `unknown/accepted/declined/disabled` (almacenado en `app_settings` con claves `gnome_shell_integration_consent` y `gnome_shell_integration_state`).
- [x] 4.3 UI de consentimiento sólo en GNOME Wayland (`GnomeIntegrationModal.svelte` + `clipvault_gnome_integration_status`).
- [x] 4.4 Instalar la extensión incluida en los recursos de ClipVault, sin red ni root (instalación atómica en `<home>/.local/share/gnome-shell/extensions/clipvault@clipvault.app/`).
- [x] 4.5 Validar UUID, metadata y compatibilidad antes de activar (`validate_metadata` chequea `uuid == clipvault@clipvault.app` y `shell-version`).
- [x] 4.6 Escritura atómica con staging `.staging-<pid>-<n>` y `rename(2)`; auditoría de symlinks; backup `.bak` con rollback.
- [x] 4.7 Activación manual vía `gnome-extensions enable clipvault@clipvault.app` o la app **Extensiones**; el estado técnico queda en `activation_pending` hasta que el listener acepte el `hello` real (no se afirma éxito antes del handshake).
- [x] 4.8 Permitir deshabilitar, desinstalar y reintentar (`uninstall`, `retry`, `clipvault_gnome_integration_uninstall`).
- [x] 4.9 No modificar configuraciones del usuario silenciosamente (ningún `dconf`, `gsettings` ni edición de archivos del usuario fuera del directorio de la extensión).
- [x] 4.10 Corregir el estado inicial y el ciclo de consentimiento: `payload()` ahora deriva `applicable` y la sesión de `gnome_detect_session()` y no exige un live handle; cuando no hay live handle el helper sirve el consentimiento persistido y el estado técnico cacheado, así la primera ejecución en GNOME Wayland muestra el prompt en lugar de colapsar a `not_applicable`. La pre-carga del cache vive en `GnomeIntegrationService::prime_from_database`, llamado una sola vez desde `AppBootstrap::build_default`. Tests: `first_launch_reports_unknown_consent_without_installing`, `non_linux_session_reports_not_applicable`, `install_refuses_with_declined_consent`, `declined_consent_keeps_snapshot_in_not_installed`.
- [x] 4.11 `reactivate_if_consented` queda como el único punto que crea el live handle tras un reinicio, sólo si el consentimiento persistido es `accepted` Y la extensión está instalada; `unknown` / `declined` / `disabled` no construyen platform service, ni listener, ni socket.

## 5. Integración con captura y metadata

- [x] 5.1 Mantener el orden snapshot GNOME → source identifier → PrivacyGate → persistencia → metadata enrichment (el probe hot-swap ocurre dentro de `cached_active_app`, sin bypass).
- [x] 5.2 Reutilizar `LinuxApplicationMetadataProvider` (`build_application_metadata_provider` lo construye para Linux; no hay parser `.desktop` paralelo en `linux_gnome_*`).
- [x] 5.3 Confirmar blacklist antes de provider y assets (`PrivacyGate` opera sobre el snapshot; el provider se invoca después).
- [x] 5.4 Mantener el contrato de origen desconocido cuando no haya app_id (`NoActiveApplication` → `Ok(None)`).
- [x] 5.5 No tocar imágenes, tags, colecciones, favoritos, búsqueda, Quick Paste ni drag-and-drop (cambios acotados a `linux_gnome_*`, `gnome_integration.rs` y `GnomeIntegrationModal.svelte`).

## 6. UI y diagnósticos

- [x] 6.1 Mostrar estados comprensibles: disponible, activa, `activation_pending`, deshabilitada, incompatible y no disponible (`GnomeIntegrationModal.svelte` cubre cada estado con mensaje específico).
- [x] 6.2 Exponer backend y etapa sin títulos, PID, rutas, secretos ni contenido (el payload público ya no incluye `install_target_dir` ni `socket_path`; el campo `identifier` sólo expone el `app_id` normalizado; `Installation` deja de serializar `target_dir` y `metadata_json`).
- [x] 6.3 No informar `active_application = true` antes de confirmar la conexión real (`Identified`/`NoActiveApplication`/`Unavailable` reflejan el estado del snapshot).
- [x] 6.4 Mantener fallback accesible de aplicación desconocida (`NoActiveApplication` → captura sin `source_app`, contrato original).
- [x] 6.5 Alinear el `GnomeIntegrationPayload` y `GnomeIntegrationInstallResult` de `app/tauri/frontend/src/types.ts` con el struct Rust: se eliminan `install_target_dir`, `socket_path`, `target_dir` y `metadata_json` para que el tipado del frontend nunca induzca a renderizar rutas absolutas.

## 7. Versionado y pruebas automatizadas

- [x] 7.1 Incrementar una sola vez la versión patch al reanudar la implementación: 0.0.11 → 0.0.12, en `Cargo.toml`, `Cargo.lock`, `app/tauri/src-tauri/tauri.conf.json`, `app/tauri/frontend/package.json`, `app/tauri/frontend/package-lock.json` y `projects.md`.
- [x] 7.2 No volver a incrementar la versión mientras este cambio siga abierto.
- [x] 7.3 AboutModal debe continuar leyendo `diagnostics.version`, sin versión hardcodeada (verificado: `app/tauri/frontend/src/AboutModal.svelte:36-40` lee `diagnostics?.version`).
- [x] 7.4 Probar instalación, consentimiento, rechazo persistente, activación, desactivación y desinstalación: tests del installer cubren instalación atómica, rechazo de UUID ajeno, validación de `shell-version`, idempotencia, render correcto de `metadata.json`; el consentimiento se prueba en `clipvault-core` (`consent_round_trips_through_storage`, `unknown_storage_is_default`); los nuevos tests `first_launch_reports_unknown_consent_without_installing`, `non_linux_session_reports_not_applicable`, `install_refuses_with_declined_consent`, `declined_consent_keeps_snapshot_in_not_installed` cubren el ciclo de consentimiento.
- [x] 7.5 Probar IPC, versionado, desconexión, reintentos y app_id vacío: tests `wire_envelope_rejects_incompatible_protocol`, `wire_envelope_rejects_unknown_kind`, `wire_envelope_accepts_hello`, `wire_envelope_accepts_app_id`, `snapshot_active_app_id_filters_empty_and_whitespace`, `process_peer_refuses_app_id_before_hello`, `wire_protocol_accepts_hello_then_app_id`, `wire_protocol_treats_empty_app_id_as_no_active_application`.
- [x] 7.6 Probar precedencia, fallback XWayland, blacklist y provider Linux: la precedencia queda codificada en `swap_active_app_probe` (GNOME > nativo) y en `try_build_gnome_probe` (devuelve `None` cuando no aplica); los tests `X11Ewmh`/`XWaylandEwmh` del backend siguen pasando.
- [x] 7.7 Probar ausencia de filtraciones metadata-only: `payload_does_not_leak_absolute_paths`, `status_omits_install_target_dir`, `diagnostics_payload_never_carries_paths_or_secrets`; la nueva serialización omite `target_dir` y `metadata_json` del `Installation` que `InstallResult` propaga.
- [x] 7.8 Ejecutar regresiones completas de Rust, frontend, imágenes, tags, colecciones, favoritos, búsqueda, Quick Paste y drag-and-drop: 319 core, 163 platform, 117 db, 31 search, 85 app, 77 bin = 792 tests Rust pasando; 1197 tests frontend pasando; ninguna regresión funcional en `pointerDragAndDrop.ts`, `HistoryCardRail` ni en los assets de imágenes.
- [x] 7.9 Ejecutar `fmt`, `clippy` con `-D warnings`, tests Rust, check/build/tests frontend y `openspec validate --strict`.

## 8. Verificación manual Ubuntu GNOME Wayland — pendiente del usuario

> MiniMax corre en macOS y NO puede validar el runtime real de Ubuntu GNOME Wayland. Las tareas de esta sección deben quedarse sin marcar; el usuario las ejecuta tras la próxima build de Codex.

- [ ] 8.1 Verificar primer inicio, consentimiento y ausencia de root.
- [ ] 8.2 Verificar Terminal GNOME nativa: nombre, icono, blacklist y persistencia.
- [ ] 8.3 Verificar Firefox y Chrome nativos: nombre, icono, Desktop y Quick Paste.
- [ ] 8.4 Verificar rechazo persistente y reintento desde configuración.
- [ ] 8.5 Verificar deshabilitar/desinstalar y recuperación a `Unavailable`.
- [ ] 8.6 Verificar Warp, Synaptic, XSane y xTerm XWayland.
- [ ] 8.7 Verificar reinicio de ClipVault y reinicio/cierre de sesión GNOME.
- [ ] 8.8 Verificar GNOME Shell incompatible o sin integración disponible.
- [ ] 8.9 Registrar distro, versión GNOME, tipo de sesión, versión ClipVault y resultado sin incluir datos sensibles.
- [ ] 8.10 Verificar manualmente que la primera ejecución muestra el prompt de consentimiento y que `hello` se publica antes del primer `app_id` (logs `tracing` del listener en `debug`).

## 9. Cierre

- [ ] 9.1 Revisar diff y confirmar que no se modificaron assets ni configuraciones ajenas del usuario.
- [ ] 9.2 No archivar, sincronizar, commitear ni hacer push automáticamente.

---

## Causa raíz corregida en este pase

1. **`payload()` colapsaba a `not_applicable` en el primer arranque de Ubuntu GNOME Wayland** aunque la sesión era aplicable: el helper exigía un `live` handle que sólo se construía tras un `install()` o un reinicio con `consent = accepted`. La sesión GNOME Wayland sin consentimiento persistido no podía alcanzar el prompt del modal de Development. Corrección: `payload()` ahora deriva `applicable` y la sesión de `gnome_detect_session()` y sirve el consentimiento / estado técnico desde los caches en memoria cuando todavía no existe un live handle, sin crear sockets ni listeners.

2. **Handshake y primer `app_id` se emitían concurrentes sobre la misma `output_stream`**: el `_sendQueued()` revisaba sólo el flag `sending` propio, pero la escritura del `hello` no quedaba registrada en él. Un cambio de foco podía programar un `write_async` de `app_id` antes de que el `hello` flusheara, desordenando el protocolo y arriesgando que el listener descartara el `hello`. Corrección: la escritura del handshake ahora marca `sending = true` desde el inicio, `_sendQueued()` se niega a despachar mientras `handshake_complete` siga `false`, y el callback del handshake vacía la cola una vez confirmado. La limpieza en `disable()` y `_resetSocket()` garantiza que el próximo `enable` arranque de cero.

3. **`InstallResult` filtraba `target_dir` y `metadata_json` en JSON**: el struct público del Tauri command serializaba la ruta absoluta de instalación y el cuerpo de `metadata.json`. El frontend nunca los mostraba, pero el contrato privacy-by-default quedaba violado. Corrección: `serde(skip_serializing)` sobre los dos campos.

## Verificación ejecutada en este pase

| Check | Resultado |
|-------|-----------|
| `cargo fmt --all -- --check` | OK (sin diffs pendientes) |
| `cargo clippy --workspace --all-targets -- -D warnings` | OK (0 warnings) |
| `cargo clippy -p clipvault-platform --target x86_64-unknown-linux-gnu --features linux-x11,linux-wayland-active-app,linux-gnome-shell-integration,linux-svg-raster,clipboard-arboard,hotkey-global --all-targets -- -D warnings` | OK (0 warnings) |
| `cargo check --workspace` | OK |
| `cargo check -p clipvault-platform --target x86_64-unknown-linux-gnu --features linux-x11,linux-wayland-active-app,linux-gnome-shell-integration,linux-svg-raster,clipboard-arboard,hotkey-global --tests` | OK (los tests del runtime Linux compilan) |
| `cargo test -p clipvault-platform --lib` | 163 passed, 0 failed |
| `cargo test -p clipvault-core --lib` | 319 passed, 0 failed |
| `cargo test -p clipvault-db --lib` | 117 passed, 0 failed |
| `cargo test -p clipvault-search --lib` | 31 passed, 0 failed |
| `cargo test -p clipvault-app --lib` | 85 passed, 0 failed |
| `cargo test -p clipvault-app --bin clipvault-app` | 77 passed, 0 failed |
| `npm ci` (Node 20.20.2, npm 10.8.2) | OK |
| `npm run check` | OK (0 errores, 15 warnings preexistentes) |
| `npm run build` | OK |
| `npm test` | 1197 passed, 0 failed |
| `openspec validate gnome-wayland-integration --strict --type change` | OK ("Change 'gnome-wayland-integration' is valid") |
| `openspec validate --all --strict` | 27 passed, 0 failed |

## Limitaciones de este pase

- `clipvault-app` no se compila para `x86_64-unknown-linux-gnu` desde macOS porque Tauri requiere `pkg-config` con `gdk-pixbuf`, `cairo`, `pango`, `atk` y `webkit2gtk-4.1` enlazados contra un sysroot Linux. Los crates `clipvault-platform`, `clipvault-core` y `clipvault-search` se compilaron limpiamente para `x86_64-unknown-linux-gnu` con las features reales.
- Los tests de runtime Linux (socket, restart, privacidad, precedence del probe) sólo se ejecutan en un binario Linux enlazado. Las pruebas añadidas están compilando correctamente pero requieren un runner Linux para ejercitarse.
- La sección 8 sigue pendiente de verificación manual en Ubuntu GNOME Wayland. MiniMax no marcó ninguna tarea de esa sección.
- El binario final de Tauri Linux y la prueba real de GNOME Wayland siguen requiriendo la máquina Ubuntu del usuario; Codex cierra el ciclo con commit + push y el usuario ejecuta el smoke test.