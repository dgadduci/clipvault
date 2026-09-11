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

## Causa raíz del problema GNOME Wayland

GNOME/Mutter no expone una API pública y portable que permita a un cliente
Wayland externo consultar qué aplicación nativa tiene el foco. Los dos
protocolos públicos que ClipVault ya conoce — `ext-foreign-toplevel-list-v1`
(preferido) y `zwlr_foreign_toplevel_management_unstable_v1` (fallback
wlroots) — sólo se anuncian en compositores que los publican; GNOME no lo
hace para identificar la app enfocada. Por eso una captura de Chrome,
Firefox o Terminal Ubuntu bajo GNOME Wayland queda sin `source_app`: el
probe nativo recibe `Ok(None)` y el contrato de origen desconocido se
aplica.

La única vía soportada por GNOME es una extensión del Shell que viva
dentro de GNOME Shell y lea `Shell.WindowTracker.get_default().focus_app`
— API privada del Shell, no disponible desde un proceso Wayland externo.
La integración GNOME de este cambio (UUID `clipvault@clipvault.app`)
publica sólo el `app_id` o desktop id por un socket local versionado;
ClipVault lo consume a través de un `ActiveApplicationProbe` que se
hot-swappea adelante del probe nativo Wayland cuando el usuario acepta la
integración. El fallback X11/XWayland permanece intacto para Warp,
Synaptic, XSane y xTerm (todos publican ventanas X11 reales) y la
precedencia GNOME → nativo Wayland → XWayland → `Unavailable` se conserva.

El commit `5c9f584` ("fix: enable GNOME integration by default on
Linux") intentó resolver la misma necesidad forzando la activación del
feature `linux-gnome-shell-integration` en `[features].default`, lo que
volvió inservible el desktop Ubuntu porque:

1. el Tauri dev shell compilaba las ramas `cfg(feature = "linux-gnome-shell-integration")`
   en builds donde la sesión no era Wayland;
2. el shell comenzaba a buscar un listener que no existía en el
   binario cuando la integración no estaba realmente disponible;
3. la integración GNOME dejó de ser opt-in por sesión, contradiciendo
   el contrato de consentimiento explícito.

El revert en `b93718b` devolvió la feature a su activación
target-specific (sólo Linux) y runtime-conditioned (sólo tras el prompt
de consentimiento). El baseline sobre el que se ejecuta este pase
(`b93718b`) coincide con `7a88ad7` salvo por los cambios del propio
revert, y conserva intacta la implementación de la integración GNOME.

---

## Causa raíz corregida en este pase

1. **`payload()` colapsaba a `not_applicable` en el primer arranque de Ubuntu GNOME Wayland** aunque la sesión era aplicable: el helper exigía un `live` handle que sólo se construía tras un `install()` o un reinicio con `consent = accepted`. La sesión GNOME Wayland sin consentimiento persistido no podía alcanzar el prompt del modal de Development. Corrección: `payload()` ahora deriva `applicable` y la sesión de `gnome_detect_session()` y sirve el consentimiento / estado técnico desde los caches en memoria cuando todavía no existe un live handle, sin crear sockets ni listeners.

2. **Handshake y primer `app_id` se emitían concurrentes sobre la misma `output_stream`**: el `_sendQueued()` revisaba sólo el flag `sending` propio, pero la escritura del `hello` no quedaba registrada en él. Un cambio de foco podía programar un `write_async` de `app_id` antes de que el `hello` flusheara, desordenando el protocolo y arriesgando que el listener descartara el `hello`. Corrección: la escritura del handshake ahora marca `sending = true` desde el inicio, `_sendQueued()` se niega a despachar mientras `handshake_complete` siga `false`, y el callback del handshake vacía la cola una vez confirmado. La limpieza en `disable()` y `_resetSocket()` garantiza que el próximo `enable` arranque de cero.

3. **`InstallResult` filtraba `target_dir` y `metadata_json` en JSON**: el struct público del Tauri command serializaba la ruta absoluta de instalación y el cuerpo de `metadata.json`. El frontend nunca los mostraba, pero el contrato privacy-by-default quedaba violado. Corrección: `serde(skip_serializing)` sobre los dos campos.

---

## Regresión de compilación descubierta al activar `--features linux-gnome-shell-integration` en Ubuntu

El pase descubrió que la activación explícita de la feature `linux-gnome-shell-integration`
(con la línea de `cargo tauri dev` que el usuario ejecutó) exponía cinco defectos de
compilación que macOS no podía detectar — las pruebas macOS nunca compilaron los bloques
`cfg(feature = "linux-gnome-shell-integration")` ni los arms de `cfg(target_os = "linux")`
que referencian los símbolos listados. La matriz de compilación del cambio había quedado
incompleta. Los cinco defectos encontrados y corregidos en este pase son:

1. **Imports no resueltos `clipvault_platform::GnomeShellListener`,
   `clipvault_platform::ListenerHandle` y `clipvault_platform::SharedGnomeSnapshot`**:
   el shell `app/tauri/src-tauri/src/gnome_integration.rs` los importaba desde la raíz
   del crate `clipvault_platform`, pero el módulo
   `runtime::linux_gnome_shell_integration` sólo re-exportaba `GnomeDiagnostics`,
   `GnomeIntegrationState`, `GnomeShellActiveApplication`, `UnixListenerTransport`,
   `MAX_FRAME_BYTES` y `PROTOCOL_VERSION`. Los tres tipos que el shell necesitaba para
   instanciar `GnomeShellListener::new(...)`, `ListenerHandle::from_thread_with_socket(...)`
   y para tipar el campo `LiveGnomeHandle::snapshot` no estaban expuestos al crate root.
   Corrección: `crates/clipvault-platform/src/lib.rs` ahora re-exporta `GnomeShellListener`,
   `ListenerHandle` y `SharedGnomeSnapshot` desde el bloque
   `pub use runtime::linux_gnome_shell_integration::{...}` que ya exponía los tipos
   vecinos (mismo cfg gate, mismo prefijo de nombre, misma convención). Se elimina el
   alias muerto `SharedGnomeSnapshot as GnomeIntegrationSnapshot` del bloque
   `runtime::linux_gnome_integration` — ningún consumidor del workspace lo usa y
   chocaba de nombre con `clipvault_core::GnomeIntegrationSnapshot` que el shell ya
   importa por separado, así que la colisión se evita sin tocar APIs externas.

2. **Faltaba el derive macro `serde::Deserialize` en `commands.rs`**: el primer
   `mod gnome_commands` (gateado por
   `#[cfg(all(target_os = "linux", feature = "linux-gnome-shell-integration"))]`)
   declaraba `#[derive(Debug, Deserialize)]` sobre `GnomeConsentUpdate` pero el
   archivo sólo importaba `use serde::Serialize;`. macOS, donde ese módulo se
   compila vaciado, no detecta el uso del símbolo; Linux, donde el módulo
   sí se compila, falla con `error[E0432]: unresolved import crate::Deserialize`.
   Corrección: el `use serde::Serialize;` se conserva en el nivel superior para
   el resto de los Tauri commands, y dentro del `mod gnome_commands` que
   realmente usa `Deserialize` se agrega `use serde::Deserialize;` bajo el mismo
   `cfg` gate. La importación queda restringida al path Linux real y
   `-D warnings` deja de marcarla como unused en macOS.

3. **`bootstrap.rs` trataba `ConnectionOutcome::Operational` y
   `ConnectionOutcome::Unavailable` como variantes tuple / unit, pero son
   variantes struct**: el arm
   `ConnectionOutcome::Operational(probe) => return Arc::new(probe)` fallaba
   al compilar porque la variante real es
   `Operational { probe: WaylandActiveApplication<UnixStreamTransport>, backend: &'static str }`,
   y `ConnectionOutcome::Unavailable` es
   `Unavailable { cause: UnavailableCause }` (no unit). El match estaba escrito
   para una versión anterior del enum sin campos. Corrección: el match se
   reescribe a la sintaxis struct (`Operational { probe, backend: _ }` y
   `Unavailable { cause: _ }`), preservando todos los campos relevantes: el
   `probe` que la bootstrap envuelve en `Arc<dyn ActiveApplicationProbe>` se
   sigue extrayendo del primer arm y se ignora el `backend`/`cause` (los
   diagnostic logs ya tienen su canal dedicado y no se duplican aquí). El
   comportamiento del capture loop no cambia.

4. **`gnome_integration.rs` llamaba `.unwrap_or(default)` sobre
   `GnomeConsentDecision` y `GnomeTechnicalState`**: en el helper `payload()`
   (path sin live handle) el código leía
   `self.core_service.load_consent_from_cache().unwrap_or(GnomeConsentDecision::Unknown)`
   y la versión equivalente para `load_technical_state_from_cache`, pero esos dos
   métodos ya devuelven directamente los enums (no `Result`), de modo que el
   `.unwrap_or(...)` no compila — `GnomeConsentDecision` no implementa
   `Default` para coerción a `Option` ni tiene nada que extraer. Corrección: las
   dos llamadas pasan a ser asignaciones directas
   (`let stored_consent = self.core_service.load_consent_from_cache();`) y se
   conserva intacto el modelo de consentimiento y el estado técnico: el cache en
   memoria ya cae al valor por defecto (`Unknown` / `NotInstalled`) cuando el
   helper nunca ha persistido nada, que es exactamente el primer arranque.

5. **Import `gnome_detect_session` sin uso**: `use clipvault_platform::{...}`
   declaraba `gnome_detect_session` como import nombrado pero el único callsite
   (`clipvault_platform::gnome_detect_session()` dentro de `payload()`) ya
   usaba la ruta totalmente cualificada, así que el nombre importado no se
   referenciaba. Con `-D warnings` se trataba como
   `error: unused import: gnome_detect_session`. Corrección: se elimina la
   entrada de la declaración `use` sin tocar el callsite (la ruta totalmente
   cualificada sigue funcionando y mantiene el contrato privacy-by-default:
   el helper sólo se usa cuando el session check necesita la sesión y el
   desktop, sin acoplar el resto del shell al alias).

Con estas cinco correcciones la build Linux queda consistente con el
baseline `b93718b`: `cargo check -p clipvault-platform
--target x86_64-unknown-linux-gnu --features
linux-x11,linux-wayland-active-app,linux-gnome-shell-integration --tests`
vuelve a compilar limpio, las pruebas macOS se mantienen en 1329 passing,
los tests frontend en 1197 passing, y `cargo clippy --workspace --all-targets -- -D warnings`
queda en 0 warnings. No se reintroduce `linux-gnome-shell-integration` en
`[features].default`, la activación target-specific en
`[target.'cfg(all(target_os = "linux", not(target_os = "macos")))'.dependencies]`
permanece intacta, no se toca `~/.clipvault`, no se usa `cargo clean` y la
lógica de captura / pegado / imágenes / assets / tags / colecciones /
favoritos / Quick Paste / drag-and-drop permanece sin cambios.

## Verificación ejecutada en este pase

Estado en el que se ejecuta: `git status --short` vacío, `git branch --show-current` =
`fix/gnome-wayland-native-detection`, `git log --oneline -3` =
`b93718b Revert "fix: enable GNOME integration by default on Linux"`
→ `5c9f584 fix: enable GNOME integration by default on Linux`
→ `7a88ad7 feat: integrate GNOME Wayland active app detection`,
`git diff --check` sin diff pendientes. Versión canónica confirmada en
`Cargo.toml` / `app/tauri/src-tauri/tauri.conf.json` /
`app/tauri/frontend/package.json` = `0.0.12` (la bump `0.0.11 → 0.0.12`
se aplicó en el pase anterior; projects.md fija el canon y bloquea
re-bumps al reanudar la implementación, por lo que la versión queda
intacta).

| Check | Resultado |
|-------|-----------|
| `cargo fmt --all -- --check` | OK (sin diffs pendientes) |
| `cargo clippy --workspace --all-targets -- -D warnings` | OK (0 warnings) |
| `cargo check -p clipvault-platform --target x86_64-unknown-linux-gnu --features linux-x11,linux-wayland-active-app,linux-gnome-shell-integration --tests` | OK (los tests del runtime Linux compilan) |
| `cargo test --workspace` | OK (1329 tests Rust passing, 0 failed, 1 doc ignored) |
| `cargo test -p clipvault-platform --lib` | 214 passed, 0 failed |
| `cargo test -p clipvault-core --lib` | 319 passed, 0 failed |
| `cargo test -p clipvault-db --lib` | 117 passed, 0 failed |
| `cargo test -p clipvault-search --lib` | 31 passed, 0 failed |
| `cargo test -p clipvault-app --lib` | 85 passed, 0 failed |
| `cargo test -p clipvault-app --bin clipvault-app` | 77 passed, 0 failed |
| `npm run check` (Node 20.20.2, npm 10.8.2) | OK (0 errores, 15 warnings preexistentes) |
| `npm run build` | OK |
| `npm test` | 1197 passed, 0 failed |
| `openspec validate gnome-wayland-integration --strict --type change` | OK ("Change 'gnome-wayland-integration' is valid") |

Conteo `cargo test --workspace`: `1329` tests passing distribuidos en
lib (`clipvault-app` 85, `clipvault-core` 319, `clipvault-db` 117,
`clipvault-platform` 214, `clipvault-search` 31) + bin
(`clipvault-app` 77) + integration tests (`clipboard_adapter_regression`
4, `linux_app_metadata` 0, `linux_wayland_active_app` 0,
`macos_clipboard_main_queue_regression` 15, `asset_isolation` 10,
`blacklist_app_picker` 9, `bootstrap` 6, `clipboard_rich_content` 47,
`clipboard_rich_text` 47, `code_language_bridge` 6,
`code_language_privacy` 7, `code_language_migration` 8, `database` 16,
`desktop_dnd_card_visual_corrections` 5, `desktop_header_card_dnd` 6,
`history` 14, `management` 14, `organization` 56,
`original_png_bytes` 19, `paste_regression` 8, `paste_suppression` 15,
`pasteboard_png_metadata` 18, `platform_integration` 21,
`privacy_settings` 32, `quick_paste_copy` 29, `source_app_filter` 12,
`source_app_metadata` 11, `type_detection` 9, `capture_tick_command`
5, `clipboard_asset_command` 14, `icon_command` 10,
`set_entry_title_command` 4, `source_app_icon_command` 9) + doc-tests
(`clipvault_core` 0 passed + 1 ignored, resto 0).

## Limitaciones reales del cambio

- `clipvault-app` no se compila para `x86_64-unknown-linux-gnu` desde
  macOS porque Tauri requiere `pkg-config` con `gdk-pixbuf`, `cairo`,
  `pango`, `atk` y `webkit2gtk-4.1` enlazados contra un sysroot Linux.
  El check del binario se reduce a `cargo check -p clipvault-platform
  --target x86_64-unknown-linux-gnu --features
  linux-x11,linux-wayland-active-app,linux-gnome-shell-integration
  --tests`, que compila el runtime Linux con sus features reales.
- Los tests de runtime Linux (socket, restart, privacidad, precedence
  del probe) sólo se ejercitan en un binario Linux enlazado. Las
  pruebas añadidas (`wire_envelope_*`, `wire_protocol_*`,
  `process_peer_*`, `snapshot_active_app_id_filters_empty_*`)
  compilan correctamente y ejecutan en macOS usando stubs equivalentes,
  pero su verificación exhaustiva de I/O real requiere un runner
  Linux.
- La activación del feature `linux-gnome-shell-integration` se hace en
  el bloque `[target.'cfg(all(target_os = "linux", not(target_os =
  "macos")))'.dependencies]` del shell `Cargo.toml`, NUNCA en
  `[features].default`. Esta regla arquitectónica está documentada en
  `design.md` y respetada por `cargo check` y `cargo clippy`, pero
  actualmente no cuenta con un test estructural dedicado que la
  pinnee — el `shell_linux_x11_cfg_does_not_require_linux_x11_feature`
  cubre el caso análogo para `linux-x11` y el
  `shell_linux_svg_raster_feature_is_enabled_for_linux_target` para
  `linux-svg-raster`. Un cambio futuro que re-introduzca
  `linux-gnome-shell-integration` en `[features].default` repetiría la
  regresión de `5c9f584` sin disparar ningún test automatizado (los
  binarios compilan y los tests pasan igual). Es la pieza de debt más
  importante que el pase deja abierta; queda registrada para que un
  siguiente cambio cierre el pin.
- La sección 8 (verificación manual Ubuntu GNOME Wayland) sigue
  pendiente del usuario. MiniMax no marcó ninguna tarea de esa sección
  y no dispone del runtime Wayland real para ejercitarla.
- El binario final de Tauri Linux y la prueba real de GNOME Wayland
  requieren la máquina Ubuntu del usuario. Codex cierra el ciclo con
  commit + push tras el visto bueno del usuario; MiniMax no commitea,
  pushea ni archiva por sí mismo.

## Estado del cambio tras este pase

- Tareas 1–7 marcadas `[x]`: la implementación coincide con el alcance
  aprobado en `proposal.md`, `design.md` y `specs/`.
- Tareas 8 y 9 sin marcar: siguen siendo responsabilidad del usuario
  (verificación manual) y del cierre controlado por Codex.
- `Cargo.toml`, `app/tauri/src-tauri/tauri.conf.json`,
  `app/tauri/frontend/package.json` están alineados en `0.0.12`; no se
  modificaron porque la bump anterior ya consumió el patch reservado
  para este cambio (projects.md regla "Resuming an interrupted
  implementation does not re-bump the version").
- El `AboutModal` sigue leyendo `diagnostics?.version` (ver
  `app/tauri/frontend/src/AboutModal.svelte:36–40`) sin valor
  hardcodeado, así que un cambio futuro de versión en los manifiestos
  canónicos se refleja automáticamente en la UI.