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
- [x] 2.9 Corregir la conexión asíncrona de la extensión para invocar `Gio.SocketClient.connect_async(connectable, cancellable, callback)` con la firma GI de tres argumentos que GNOME Shell 42 expone; no pasar prioridad ni placeholders de otra API. Mantener el handshake posterior a `connect_finish` y el backoff no bloqueante. La regresión `gnomeExtensionSocket.test.ts`, `npm run check`, `npm test`, `gjs --check` y la consulta GI de aridad 3 pasan; la prueba de integración visual sigue pendiente en 8.11.
- [x] 2.10 Crear `Gio.SocketClient` con la propiedad GObject GI `type: Gio.SocketType.STREAM`, no con la inexistente `socket_type`. `gnomeExtensionSocket.test.ts` protege constructor y llamada; `npm run check`, `npm test`, `gjs --check` y la creación directa con `gjs` en GNOME Shell 42 pasan. La prueba manual 8.11 sigue pendiente.

## 3. IPC y adapter Linux

- [x] 3.1 Implementar el canal local de sesión versionado (Unix socket `$XDG_RUNTIME_DIR/clipvault/clipvault-focus.sock`, 16-bit `PROTOCOL_VERSION`).
- [x] 3.2 Definir conexión, desconexión, versión, `app_id` presente y ausencia sin transportar contenido sensible (envelope `{ v, kind, app_id }`).
- [x] 3.3 Implementar `GnomeShellActiveApplication` como `ActiveApplicationProbe` no bloqueante (lee desde `SharedGnomeSnapshot`, nunca toca I/O).
- [x] 3.4 Usar un snapshot concurrente único (`SharedGnomeSnapshot` con `parking_lot::RwLock`), sin segundo watcher ni estado de deduplicación paralelo.
- [x] 3.5 Traducir peer ausente, versión incompatible, error y desconexión a estados tipados (`NoActiveApplication`, `WireError::ProtocolVersion`, `CommunicationError`, `Disconnected`).
- [x] 3.6 Mantener precedencia GNOME conectado → Wayland público → XWayland → desconocido; el `swap_active_app_probe` queda dentro de `AppContext::cached_active_app` y el watcher nunca construye un segundo probe.
- [x] 3.7 Marcar `Installation::target_dir` y `Installation::metadata_json` con `#[serde(skip_serializing)]` para que el `InstallResult` que Tauri devuelve nunca incluya la ruta absoluta de instalación ni el cuerpo del `metadata.json`. La vista interna de Rust sigue disponible para tests e instalador.
- [ ] 3.8 Hacer que `ListenerHandle` sea `Send` por composición (sin `unsafe`) y reutilizar `spawn_listener_thread_with_socket` desde el shell Tauri; cubrir el contrato de compilación `Send` y el shutdown del socket.

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
- [ ] 4.12 Activar `clipvault-app/linux-gnome-shell-integration` en la configuración Linux normal de Tauri, no sólo la feature de `clipvault-platform`. El `DevCommand` y la build empaquetada deben compilar los módulos y comandos GNOME del shell; macOS debe continuar excluyéndolos mediante `cfg(target_os = "linux")`.
- [x] 4.13 Conectar la tarjeta de diagnóstico GNOME aplicable con el modal de consentimiento: ofrece **Configurar integración GNOME** cuando el snapshot es aplicable y le pasa ese snapshot al modal, sin mutar consentimiento ni instalar la extensión desde el botón de entrada. Verificado con `npm run check`, `npm test` y `npm run build`.
- [x] 4.14 Renderizar los `CommandError` tipados del flujo GNOME con mensajes seguros y accionables; nunca mostrar `[object Object]` ni detalles locales crudos de I/O. Verificado con `npm run check`, `npm test` (73 pruebas) y `npm run build`.
- [x] 4.15 Resolver los recursos de extensión desde las dos disposiciones válidas de `resource_dir` de Tauri en Linux (dev con prefijo `resources/` y bundle), sin usar el árbol fuente; cubierta la disposición de desarrollo con `bundled_resource_resolution_supports_tauri_dev_resources_layout`.

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
- [ ] 7.10 En Ubuntu, ejecutar el comando normal `cargo tauri dev` y una build Linux de producción; comprobar que ambos seleccionan `clipvault-app/linux-gnome-shell-integration` (la línea `DevCommand` la incluye) y que el status GNOME no devuelve `feature_disabled`. Repetir los checks Rust/Tauri afectados después de la corrección.
- [ ] 7.11 Ejecutar el check y los tests de `clipvault-app` con `--no-default-features --features clipboard-arboard,hotkey-global,linux-gnome-shell-integration`; esta combinación compila los comandos y el estado Tauri que la build normal Linux selecciona.

## 8. Verificación manual Ubuntu GNOME Wayland — pendiente del usuario

> MiniMax corre en macOS y NO puede validar el runtime real de Ubuntu GNOME Wayland. Las tareas de esta sección deben quedarse sin marcar; el usuario las ejecuta tras la próxima build de Codex.

> Bloqueo observado el 2026-09-11: el primer runtime Ubuntu GNOME Wayland
> informó `Integración no disponible (feature_disabled)`. No se puede marcar
> ninguna tarea de esta sección hasta completar 4.12 y 7.10 con la build
> normal.

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
- [ ] 8.11 En GNOME Shell 42.9, con la extensión instalada y `ENABLED` y el listener de ClipVault presente, verificar que tras enfocar Terminal GNOME el diagnóstico pasa de `activation_pending` a `connected` o `identified` y publica el desktop id; registrar sólo versión, estados y resultado.
- [x] 8.12 Registrar el smoke test reportado en Ubuntu GNOME Wayland: sesión `linux_wayland`, consentimiento `accepted`, estado técnico `identified` e identificador publicado `window:6`. Confirma que la extensión y el listener completan el transporte metadata-only; no completa 8.2, 8.3 ni 8.11 porque el icono Wayland continúa ausente y el identificador observado no es un desktop id.

## 9. Cierre

- [ ] 9.1 Revisar diff y confirmar que no se modificaron assets ni configuraciones ajenas del usuario.
- [ ] 9.2 No archivar, sincronizar, commitear ni hacer push automáticamente.

---

## Causa raíz corregida en este pase

1. **`payload()` colapsaba a `not_applicable` en el primer arranque de Ubuntu GNOME Wayland** aunque la sesión era aplicable: el helper exigía un `live` handle que sólo se construía tras un `install()` o un reinicio con `consent = accepted`. La sesión GNOME Wayland sin consentimiento persistido no podía alcanzar el prompt del modal de Development. Corrección: `payload()` ahora deriva `applicable` y la sesión de `gnome_detect_session()` y sirve el consentimiento / estado técnico desde los caches en memoria cuando todavía no existe un live handle, sin crear sockets ni listeners.

2. **Handshake y primer `app_id` se emitían concurrentes sobre la misma `output_stream`**: el `_sendQueued()` revisaba sólo el flag `sending` propio, pero la escritura del `hello` no quedaba registrada en él. Un cambio de foco podía programar un `write_async` de `app_id` antes de que el `hello` flusheara, desordenando el protocolo y arriesgando que el listener descartara el `hello`. Corrección: la escritura del handshake ahora marca `sending = true` desde el inicio, `_sendQueued()` se niega a despachar mientras `handshake_complete` siga `false`, y el callback del handshake vacía la cola una vez confirmado. La limpieza en `disable()` y `_resetSocket()` garantiza que el próximo `enable` arranque de cero.

3. **`InstallResult` filtraba `target_dir` y `metadata_json` en JSON**: el struct público del Tauri command serializaba la ruta absoluta de instalación y el cuerpo de `metadata.json`. El frontend nunca los mostraba, pero el contrato privacy-by-default quedaba violado. Corrección: `serde(skip_serializing)` sobre los dos campos.

4. **La extensión habilitada no completaba la conexión en GNOME Shell 42.9**: durante la verificación manual del 2026-09-12, el diagnóstico quedó en `activation_pending` aunque `gnome-extensions info clipvault@clipvault.app` informó `ENABLED` y el socket del listener existía. `Gio.SocketClient.connect_async` expone una firma de tres argumentos en ese runtime, mientras `extension.js` le pasaba cinco (`connectable`, `cancellable`, prioridad, placeholder y callback). La llamada lanza antes del handshake, la excepción se absorbe y el backoff reintenta sin peer. La tarea 2.9 reemplaza la invocación por la firma GI correcta y añade una regresión; 8.11 conserva la prueba manual que reprodujo el defecto.

5. **El constructor de `Gio.SocketClient` también usaba una propiedad GObject inexistente en GNOME Shell 42.9**: tras reinstalar la corrección 2.9, la copia instalada ya tenía la firma correcta pero el diagnóstico continuó en `activation_pending`. La consulta directa con `gjs` devolvió `Error: No property socket_type on GSocketClient`; la excepción sucede antes de `connect_async` y el `catch` la convierte en un reintento silencioso. La tarea 2.10 reemplaza `socket_type` por la propiedad GI `type`, protege ambas formas y mantiene 8.11 pendiente hasta obtener un `hello` real.

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

## Revalidación en este workspace Linux

| Check | Resultado |
|---|---|
| `cargo fmt --all -- --check` | OK |
| `openspec validate gnome-wayland-integration --strict --type change` | OK |
| `npm run check` | OK (0 errores, 15 warnings existentes) |
| `npm test` | OK (74 archivos de prueba, 0 fallos; incluye GNOME y drag-and-drop) |
| `npm run build` | OK (15 warnings existentes) |
| `cargo test -p clipvault-platform --lib` | No apto como gate en este sandbox: 188 passed y 23 fallos concentrados en `linux_app_metadata` |
| Tests GNOME de `clipvault-platform` | 14 passed y 6 fallos porque el sandbox devuelve `EPERM` al crear/bindear sockets Unix |
| Tests `clipvault-app` con la combinación Linux GNOME | Los binarios de test compilan, pero lib (82/96) y bin (74/88) fallan 14 casos de bootstrap existentes con capturas `Ignored` y un probe Wayland `Unavailable`; 7.11 permanece pendiente |
| `cargo clippy -p clipvault-app --lib --no-default-features --features clipboard-arboard,hotkey-global,linux-gnome-shell-integration -- -D warnings` | Bloqueado por `clippy::overly_complex_bool_expr` ya presente en `crates/clipvault-core/src/content_type.rs:784` |

## Limitaciones de este pase

- `clipvault-app` no se compila para `x86_64-unknown-linux-gnu` desde macOS porque Tauri requiere `pkg-config` con `gdk-pixbuf`, `cairo`, `pango`, `atk` y `webkit2gtk-4.1` enlazados contra un sysroot Linux. Los crates `clipvault-platform`, `clipvault-core` y `clipvault-search` se compilaron limpiamente para `x86_64-unknown-linux-gnu` con las features reales.
- Los tests de runtime Linux (socket, restart, privacidad, precedence del probe) sólo se ejecutan en un binario Linux enlazado. Las pruebas añadidas están compilando correctamente pero requieren un runner Linux para ejercitarse.
- El smoke test reportado de Ubuntu GNOME Wayland confirma `accepted` → `identified` y el identificador opaco `window:6`, por lo que el transporte local está operativo. Las verificaciones de nombre, icono, blacklist, persistencia y el desktop id real siguen pendientes en la sección 8.
- La ausencia de icono de aplicación en cards Wayland queda deliberadamente fuera de este cambio: se tratará como una propuesta OpenSpec posterior para resolver y presentar iconos a partir de metadata ya disponible, sin ampliar el canal de la extensión.
- El binario final de Tauri Linux y la prueba real de GNOME Wayland siguen requiriendo la máquina Ubuntu del usuario; Codex cierra el ciclo con commit + push y el usuario ejecuta el smoke test.
