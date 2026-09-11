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

---

## Causa raíz del fallo de compilación Linux

El comando que el usuario ejecutó en su Ubuntu Wayland expone un
defecto que las pruebas macOS no podían detectar: el binario
`clipvault-app` no compila cuando la feature
`linux-gnome-shell-integration` se activa explícitamente. La matriz
de compilación del cambio quedó incompleta y el defecto sólo se
manifiesta cuando el bloque `cfg(all(target_os = "linux", feature =
"linux-gnome-shell-integration"))` se evalúa como verdadero.

**Síntoma exacto:** `ListenerHandle` contiene un campo
`_marker: std::marker::PhantomData<*const ()>`. Un `*const ()` es un
puntero crudo, que por defecto es `!Send + !Sync`. `ListenerHandle`
vive dentro de `LiveGnomeHandle` → `GnomeIntegrationState` →
`AppState` → `SharedState` (la envoltura que el shell registra en
`tauri::Manager::manage`). El extractor `State<'_, SharedState>` de
Tauri requiere que el tipo administrado implemente `Send + Sync`, y
la presencia de `*const ()` rompe ese contrato: el binario `cargo
tauri dev --features ...linux-gnome-shell-integration` falla con
`the trait Send is not implemented for *const ()` / `PhantomData<*const ()>`
antes de alcanzar `main`.

**Por qué existía el marcador:** fue un residuo de un prototipo
anterior donde `ListenerHandle` era genérico sobre el tipo de
transporte (`pub struct ListenerHandle<T: ListenerTransport> { ...
_marker: PhantomData<T> }`). Cuando el tipo dejó de ser genérico el
marcador se quedó y se cambió a `PhantomData<*const ()>` sin
verificar la varianza. El cambio no aporta seguridad: `Arc<AtomicBool>`,
`Option<thread::JoinHandle<()>>` y `Option<PathBuf>` ya son `Send +
Sync` por construcción, así que ningún campo necesita un marcador
para conservar la corrección.

**Por qué no se reemplazó por un `PhantomData<T>` Send-compatible:**
no existe ninguna función de varianza que proteger — el struct no
expone un parámetro `T`. La corrección correcta es eliminar el
marcador; añadir otro `PhantomData<SendSafe>` sería introducir
ruido sin propósito.

## Corrección aplicada

1. **`ListenerHandle` ya no lleva `PhantomData<*const ()>`**: el
   campo se eliminó por completo en
   `crates/clipvault-platform/src/runtime/linux_gnome_shell_integration.rs`.
   El docstring del struct documenta el contrato `Send + Sync` y
   enlaza con la regresión que lo pinnea, para que un futuro
   refactor no reintroduzca el marcador por error.

2. **`spawn_listener_thread` actualiza su construcción** para no
   pasar el marcador inexistente; `from_thread` y
   `from_thread_with_socket` también, ya que ambas rutas se usan
   desde el shell y desde los tests.

3. **`GnomeShellListener<T>` conserva `PhantomData<T>`** porque
   sigue siendo genérico sobre `T: ListenerTransport + 'static` y
   `ListenerTransport: Send + Sync` garantiza que `T: Send + Sync`,
   por lo que `PhantomData<T>` es automáticamente `Send + Sync`. No
   se tocó.

4. **`unsafe impl Send/Sync for ListenerHandle` queda prohibido**:
   la regla arquitectónica del proyecto prohíbe `unsafe` salvo en
   casos justificados (ver `AGENTS.md`), y aquí no se justifica
   porque el struct ya es seguro por construcción. La corrección es
   estructural, no coercitiva.

## Regresión de compilación añadida

- **`listener_handle_is_send_and_sync`** (en
  `crates/clipvault-platform/src/runtime/linux_gnome_shell_integration.rs`,
  dentro de `mod tests`): el cuerpo del test define
  `fn assert_send_sync<T: Send + Sync>() {}` y la invoca con
  `assert_send_sync::<ListenerHandle>()`. La función `assert_send_sync`
  es monomorfizada con `ListenerHandle`, así que el chequeo se
  ejecuta en tiempo de compilación: si alguien reañade el
  `PhantomData<*const ()>` o introduce otro campo `!Send`, el binario
  falla con `the trait Send is not implemented for *const ()`
  exactamente igual que el error original. El test está dentro del
  módulo cfg-gated por
  `#[cfg(all(target_os = "linux", feature = "linux-gnome-shell-integration"))]`,
  así que sólo se compila en el camino Linux y no afecta al binario
  macOS.

- **`from_thread_constructs_idempotent_shutdown_handle`**: ejercita
  `ListenerHandle::from_thread(alive, None)` + `handle.shutdown()` y
  verifica que la bandera `alive` se voltea a `false`. Como
  `shutdown(mut self)` consume el handle por construcción, una
  segunda llamada no puede ejecutarse, así que la idempotencia
  queda demostrada por la firma del método. La rama `None` del
  join cubre el callsite que el shell usa fuera de
  `spawn_listener_thread_with_socket`.

- **`spawn_listener_thread_with_socket_stops_and_cleans_up`**:
  ejercita el flujo producción completo (bind → spawn → shutdown
  con socket) y verifica que el archivo del socket desaparece
  después de `shutdown`, replicando el contrato que
  `shutdown_removes_socket_file_when_handle_owns_path` ya cubría
  para `from_thread_with_socket`. Aquí se valida adicionalmente que
  la combinación `spawn_listener_thread_with_socket` + `shutdown` +
  drop del `Option` permanece libre de pánicos.

- **`spawn_listener_thread_handle_returns_send_sync`**: vuelve a
  afirmar `Send + Sync` después de invocar `spawn_listener_thread`,
  para garantizar que la ruta sin socket también cumple el contrato
  y que un cambio futuro en el helper no rompa la transportabilidad
  entre hilos.

- **`gnome_state_chain_is_send_and_sync`** (en
  `app/tauri/src-tauri/src/gnome_integration.rs`, dentro de
  `mod tests` cfg-gated por Linux + gnome feature): afirma
  `Send + Sync` para la cadena completa
  `ListenerHandle → LiveGnomeHandle → GnomeIntegrationState →
  SharedState → AppState`. Si cualquier eslabón de la cadena
  vuelve a no ser `Send + Sync`, la compilación del shell en Linux
  falla con un error que apunta exactamente al tipo regresionado.

- **`live_handle_is_constructible_through_public_api`**: belt-and-
  braces, fuerza al compilador a materializar un `LiveGnomeHandle`
  con cada uno de los campos públicos que el runtime usa
  (`platform_service`, `snapshot`, `probe`, `listener: None`,
  `socket_path: None`). Si algún campo del shell añade `!Send` /
  `!Sync` accidentalmente, este test lo detecta antes de que el
  binario llegue al usuario.

## Verificación ejecutada en macOS

- `cargo fmt --all -- --check` → limpio.
- `cargo clippy --workspace --all-targets -- -D warnings` → 0
  warnings.
- `cargo test --workspace` → 1329 tests passing, 0 failed
  (los 4 tests nuevos viven en el módulo cfg-gated y sólo se
  ejercitan cuando la feature `linux-gnome-shell-integration` está
  activa, así que en macOS no se cuentan).
- `cargo check -p clipvault-platform --target
  x86_64-unknown-linux-gnu --no-default-features --features
  linux-x11,linux-wayland-active-app,linux-gnome-shell-integration
  --tests` → limpio. Esto valida en macOS que el lado de la
  plataforma del cambio (donde vive `ListenerHandle` y el grueso
  de los nuevos tests) compila para el target Linux, incluidos los
  tests.
- `cargo clippy -p clipvault-platform --target
  x86_64-unknown-linux-gnu --no-default-features --features
  linux-x11,linux-wayland-active-app,linux-gnome-shell-integration
  --all-targets -- -D warnings` → limpio. La regresión se mantiene
  también en `-D warnings`.
- `npm run check` (Node 20.20.2) → 0 errors, 15 warnings
  preexistentes.
- `npm run build` (Node 20.20.2) → OK.
- `npm test` (Node 20.20.2) → 1197 passed, 0 failed.
- `openspec validate gnome-wayland-integration --strict --type
  change` → "Change 'gnome-wayland-integration' is valid".

## Limitación de la verificación desde macOS

`cargo check -p clipvault-app --target x86_64-unknown-linux-gnu
--no-default-features --features
custom-protocol,clipboard-arboard,hotkey-global,linux-x11,linux-wayland-active-app,linux-gnome-shell-integration`
no puede ejecutarse desde macOS porque Tauri 2 arrastra la pila
GTK/WebKit2 (`glib-sys`, `gdk-sys`, `gdk-pixbuf-sys`, `cairo-sys-rs`,
`atk-sys`, `pango-sys`, `gobject-sys`, `gio-sys`) y los build
scripts de esas crates requieren `pkg-config` con un sysroot
Linux. macOS no tiene GTK/GLib y `pkg-config --print-errors gdk-3.0`
reporta "not found". Sin `cargo-zigbuild`, `cargo-xwin` o un
sysroot + cross-pkg-config, no hay forma práctica de cross-compilar
el binario final desde macOS.

Por lo tanto la regresión completa del shell
(`gnome_state_chain_is_send_and_sync` y
`live_handle_is_constructible_through_public_api`) sólo puede
verificarse cuando el usuario corra el binario en Ubuntu. Esa
verificación sigue pendiente y debe ejecutarse en la máquina real.

## Verificación pendiente en Ubuntu real

El usuario debe correr, en `/home/diego/Documentos/development/clipvault`:

```
cargo check -p clipvault-app \
  --no-default-features \
  --features custom-protocol,clipboard-arboard,hotkey-global,linux-x11,linux-wayland-active-app,linux-gnome-shell-integration
```

Si la regresión pasa, los 4 tests nuevos del módulo
`linux_gnome_shell_integration` (1329 + 4 = 1333 tests Rust) y los
2 tests nuevos del módulo `gnome_integration` del shell deben
ejecutarse y pasar con `cargo test --workspace`. El resultado se
reporta en este mismo `tasks.md` con la línea:

```
| `cargo test --workspace` en Ubuntu | 1333 tests passing, 0 failed |
```

y la sección 8 (verificación manual GNOME Wayland) puede
reanudar el flujo. MiniMax no marca esa tarea como completada hasta
que el usuario ejecute realmente los comandos y reporte el
resultado.

---

## Causa raíz: errores de compilación Linux en `gnome_integration.rs`

Este pase cierra dos errores de compilación que el shell
`app/tauri/src-tauri/src/gnome_integration.rs` sólo expone cuando la
feature `linux-gnome-shell-integration` se activa explícitamente
sobre un target Linux. Las verificaciones macOS nunca compilaron
el bloque `#[cfg(all(target_os = "linux", feature =
"linux-gnome-shell-integration"))]` que contiene las funciones
afectadas, así que la regresión pasó inadvertida hasta que el
usuario ejecutó `cargo tauri dev --features ...linux-gnome-shell-integration`
en su máquina Ubuntu. Los dos defectos están en funciones que ya
existen y conservan su contrato: ni el protocolo GNOME, ni los
estados `Connected` / `Identified` / `Disconnected` /
`CommunicationError`, ni el consentimiento, ni el intercambio de
`app_id`, ni la privacidad, ni la detección X11/XWayland, ni el
fallback Wayland, ni `~/.clipvault` se ven alterados.

### Error 1 — `ensure_platform_service()`: snapshot prestado en lugar de clonado

**Síntoma exacto.** El método declaraba

```rust
let snapshot = service.snapshot();          // &SharedGnomeSnapshot
let probe = Arc::new(clipvault_platform::GnomeShellActiveApplication::new(
    snapshot.clone(),                       // SharedGnomeSnapshot (clon correcto para el probe)
));
*self.live.lock() = Some(LiveGnomeHandle {
    platform_service: service.clone(),
    snapshot,                                // <-- BUG: &SharedGnomeSnapshot, no SharedGnomeSnapshot
    probe,
    listener: None,
    socket_path: service.socket_path(),
});
```

`service.snapshot()` (en `clipvault_platform::linux_gnome_integration`)
devuelve `&SharedGnomeSnapshot`. El campo `LiveGnomeHandle::snapshot`
espera un valor `SharedGnomeSnapshot` (no una referencia) porque
`SharedGnomeSnapshot: Clone` pero **no** `Copy`. Construir el
`LiveGnomeHandle` con `snapshot` movido intentaba meter un
`&SharedGnomeSnapshot` en un slot `SharedGnomeSnapshot` y el
compilador abortaba con un error de tipo:

```
error[E0308]: mismatched types
   --> app/tauri/src-tauri/src/gnome_integration.rs:136:13
    |
136 |             snapshot,
    |             ^^^^^^^ expected struct `SharedGnomeSnapshot`, found `&SharedGnomeSnapshot`
```

**Por qué se introdujo.** En una iteración anterior del shell la
función envolvía el servicio en un `Ref`/`Mutex` y la snapshot se
movía como referencia prestada para evitar un clon extra. Cuando
la función se simplificó para devolver un `Arc<GnomeIntegrationService>`
y construir `LiveGnomeHandle` por valor, la firma quedó
inconsistente con el campo.

**Por qué la prueba era silenciosa en macOS.** Todo el archivo
lleva `#![cfg(all(target_os = "linux", feature =
"linux-gnome-shell-integration"))]`. En macOS el archivo se
compila vacío, así que ningún `cargo check` / `cargo clippy` /
`cargo test` del workspace veía el cuerpo de la función. El
defecto sólo aparece cuando el bloque cfg-gate se evalúa como
verdadero, es decir, en el binario Ubuntu.

**Corrección.** Se clona explícitamente la snapshot prestada para
que el valor owned alimente tanto el probe como el handle, y la
referencia original del servicio quede intacta:

```rust
let snapshot = service.snapshot().clone();
let probe = Arc::new(clipvault_platform::GnomeShellActiveApplication::new(
    snapshot.clone(),
));
*self.live.lock() = Some(LiveGnomeHandle {
    platform_service: service.clone(),
    snapshot,        // ahora SharedGnomeSnapshot (owned)
    probe,
    listener: None,
    socket_path: service.socket_path(),
});
```

`SharedGnomeSnapshot` envuelve dos `Arc` (`inner: Arc<RwLock<GnomeSnapshot>>`
y `stage: Arc<Mutex<ProbeStage>>`); el `.clone()` incrementa el
refcount sin duplicar el estado. El servicio conserva su
referencia interna, el probe y el handle reciben cada uno su
propia copia del handle de tres campos, y los tres apuntan al
mismo `Arc` subyacente: una mutación realizada por cualquiera de
los tres se observa en los otros dos.

### Error 2 — `start_listener()`: `Arc<AtomicBool>` movido al closure y reutilizado

**Síntoma exacto.** El método declaraba

```rust
let listener = GnomeShellListener::new(handle.snapshot.clone(), transport);
let alive = listener.alive_flag();
let socket_path_for_handle = socket_path.clone();
let join = std::thread::Builder::new()
    .name("clipvault-gnome-listener".into())
    .spawn(move || {                       // <-- captura `alive` por move
        while alive.load(std::sync::atomic::Ordering::Acquire) {
            if let Err(_error) = listener.handle_one() {
                if !alive.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    })
    .ok();
handle.listener = Some(ListenerHandle::from_thread_with_socket(
    alive,                                  // <-- BUG: `alive` ya se movió al closure
    join,
    socket_path_for_handle,
));
```

`alive: Arc<AtomicBool>` se mueve al closure del hilo (porque la
captura por defecto de `move` toma ownership de las variables
externas). Al construir el `ListenerHandle::from_thread_with_socket`
después del `spawn`, el compilador reporta que `alive` ya no
existe en este frame:

```
error[E0382]: use of moved value: `alive`
   --> app/tauri/src-tauri/src/gnome_integration.rs:239:13
    |
223 |         let alive = listener.alive_flag();
    |             ----- move occurs because `alive` has type `Arc<AtomicBool>`...
...
228 |             .spawn(move || {
    |                   ------- value moved into closure here
...
239 |             alive,
    |             ^^^^^ value used here after move
```

**Por qué la prueba era silenciosa en macOS.** Mismo motivo que el
error 1: el archivo entero está cfg-gated al binario Linux.

**Corrección.** Se clona `Arc<AtomicBool>` una vez para el hilo y
se conserva el original para el `ListenerHandle`. Como
`Arc::clone` sólo incrementa el refcount del `AtomicBool`
subyacente, ambos extremos siguen viendo el mismo flag: el
`shutdown` del handle (que recibe el original) lo baja, y el
closure del hilo (que tiene la copia) lo observa inmediatamente.

```rust
let listener = GnomeShellListener::new(handle.snapshot.clone(), transport);
let alive = listener.alive_flag();
let alive_for_thread = alive.clone();
let socket_path_for_handle = socket_path.clone();
let join = std::thread::Builder::new()
    .name("clipvault-gnome-listener".into())
    .spawn(move || {
        while alive_for_thread.load(std::sync::atomic::Ordering::Acquire) {
            if let Err(_error) = listener.handle_one() {
                if !alive_for_thread.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    })
    .ok();
handle.listener = Some(ListenerHandle::from_thread_with_socket(
    alive,                                // original, no la copia del hilo
    join,
    socket_path_for_handle,
));
```

Este patrón coincide con el que ya usa `spawn_listener_thread` en
`crates/clipvault-platform/src/runtime/linux_gnome_shell_integration.rs:538-547`,
así que la corrección no introduce una nueva convención: alinea
el shell con el helper del runtime.

## Regresiones añadidas (mínimas)

Cuatro asserts viven ahora dentro del módulo `tests` del shell
(cfg-gated por Linux + `linux-gnome-shell-integration`, igual que
el resto del archivo), y cubren los cuatro contratos que el pase
promete:

1. **`ensure_platform_service_shares_snapshot_with_probe_and_handle`**.
   Construye un `GnomeIntegrationState` real, llama
   `ensure_platform_service(Accepted)`, muta el snapshot vía
   `service.snapshot().set_state(...)` y verifica que tanto
   `handle.probe.snapshot().state()` como `handle.snapshot.state()`
   observan el nuevo estado. Esto sólo es cierto si las tres
   referencias comparten el mismo `Arc<RwLock<GnomeSnapshot>>`;
   con la versión bugueada del código el archivo ni siquiera
   compila.

2. **`start_listener_keeps_handle_alive_flag_after_thread_creation`**.
   Configura `XDG_RUNTIME_DIR` en un `tempfile::TempDir`, levanta
   el servicio, llama `start_listener`, lee
   `handle.listener.as_ref().unwrap().alive_flag().load(Acquire)`
   y exige que el flag siga siendo `true` después del `spawn`.
   Con la versión bugueada el archivo no compila; con la versión
   correcta el flag queda accesible y se restaura el env var al
   terminar (sea `set_var` con el valor previo o `remove_var`).

3. **`stop_listener_removes_socket_file_after_start`**.
   Mismo escenario que (2) pero, en vez de leer el flag,
   verifica que `socket_path.exists()` es `true` tras el
   `start_listener` y `false` tras `stop_listener`. Pinnea el
   contrato de cleanup que el `shutdown` del `ListenerHandle`
   ya probaba para `from_thread_with_socket`.

4. **`gnome_state_chain_is_send_and_sync`** (pre-existente del
   pase `1720411`). Cubre el cuarto punto que el usuario
   solicita: `ListenerHandle → LiveGnomeHandle →
   GnomeIntegrationState → SharedState → AppState`. Si cualquier
   eslabón pierde `Send + Sync` el binario Linux falla al
   compilar.

Las pruebas usan `clipvault_core::SystemClock` como `Arc<dyn
Clock>` para construir el `GnomeIntegrationService` core, el
mismo reloj que usa `clipvault_core::bootstrap::AppBootstrap` en
arranque. Ningún test introduce dependencias nuevas en
`Cargo.toml` (`tempfile` ya estaba declarado en
`app/tauri/src-tauri/Cargo.toml:78`).

## Limitación de la verificación desde macOS

Este pase se ejecuta desde macOS. El archivo `gnome_integration.rs`
no compila en macOS porque `#![cfg(all(target_os = "linux",
feature = "linux-gnome-shell-integration"))]` lo vacía, así que
los cuatro asserts cfg-gated, las dos correcciones y la
`Send + Sync` de la cadena no se pueden ejercitar localmente. Lo
que sí se pudo verificar en macOS:

| Check | Resultado |
|-------|-----------|
| `cargo fmt --all -- --check` | OK (sin diffs pendientes) |
| `cargo clippy --workspace --all-targets -- -D warnings` | OK (0 warnings) |
| `cargo test --workspace` | OK (1329 tests passing, 0 failed) |
| `cargo check -p clipvault-platform --target x86_64-unknown-linux-gnu --no-default-features --features linux-x11,linux-wayland-active-app,linux-gnome-shell-integration --tests` | OK (el runtime Linux del cambio compila) |
| `cargo clippy -p clipvault-platform --target x86_64-unknown-linux-gnu --no-default-features --features linux-x11,linux-wayland-active-app,linux-gnome-shell-integration --all-targets -- -D warnings` | OK |
| `npm run check` (Node 26.8.1) | OK (0 errores, 15 warnings preexistentes) |
| `npm run build` (Node 26.8.1) | OK |
| Frontend tests con `node --test $(pwd)/node_modules/.cache/clipvault-test-build/tests/` | 1197 passed, 0 failed |
| `openspec validate gnome-wayland-integration --strict --type change` | OK |

Nota ambiental sobre `npm test`: el script del paquete usa el
path relativo `node_modules/.cache/clipvault-test-build/tests/`,
que Node 26.8.1 (instalado en este equipo vía Homebrew) no
resuelve de forma fiable — el runner reporta `tests 0`. El
mismo comando con path absoluto (`$(pwd)/node_modules/...`)
encuentra los 71 archivos `.test.js` y los 1197 tests pasan. El
pase anterior documenta que esa suite se ejecutó con Node
20.20.2; el comportamiento que cambió no es del código
producido, sino del binario `node` disponible. No se modifica
`package.json` porque la corrección de esa regresión queda fuera
del alcance de este pase.

Lo que **no** se pudo verificar desde macOS y queda pendiente:

- `cargo check -p clipvault-app --target x86_64-unknown-linux-gnu
  --no-default-features --features
  custom-protocol,clipboard-arboard,hotkey-global,linux-x11,linux-wayland-active-app,linux-gnome-shell-integration`:
  los build scripts de `glib-sys`, `gdk-sys`, `gdk-pixbuf-sys`,
  `cairo-sys-rs`, `atk-sys`, `pango-sys`, `gobject-sys`, `gio-sys`
  requieren un sysroot Linux con `pkg-config` configurado. macOS
  no lo tiene, por lo que la cross-compilación del binario final
  no es viable desde este equipo. Sin esa verificación no se
  puede afirmar que el binario Ubuntu compila, ni que los 4 tests
  nuevos del módulo `gnome_integration` (más los 4 existentes del
  pase anterior) corren en la máquina del usuario.
- La sección 8 (verificación manual GNOME Wayland) sigue
  pendiente del usuario.

La matriz de verificación para Ubuntu real que el usuario debe
ejecutar queda intacta respecto al pase anterior (mismo
`cargo check`, mismo `cargo test --workspace`). La línea de
resultado sigue siendo `1333 tests passing, 0 failed` (1329
existentes + 4 tests nuevos del módulo `linux_gnome_shell_integration`
del pase anterior; los 3 nuevos tests del módulo
`gnome_integration` del shell sólo se compilan cuando se activa
la feature `linux-gnome-shell-integration`, así que también
contribuyen al conteo una vez que el binario Ubuntu los compile).

## Versión canónica

Este pase no incrementa la versión. `projects.md` fija la
canónica en `0.0.12` y el contrato "Resuming an interrupted
implementation does not re-bump the version" bloquea re-bumps al
reanudar. `Cargo.toml`, `app/tauri/src-tauri/tauri.conf.json` y
`app/tauri/frontend/package.json` quedan en `0.0.12` y el
`AboutModal.svelte:36-40` sigue leyendo `diagnostics?.version`
sin hardcodear. La corrección es estructural (cambio de tipos y
clon de un `Arc`), no funcional, así que la regla se respeta
también por el lado de producto.

---

## Corrección: ventana principal invisible en Ubuntu Wayland

**Causa raíz.** En una sesión Ubuntu GNOME Wayland el proceso iniciaba,
Vite quedaba disponible y el tray se instalaba, pero
`WebviewWindow::primary_monitor()` devolvía `Ok(None)`. El layout
retornaba a los valores de configuración, pero el arranque no ejecutaba
una presentación explícita de la ventana principal y la acción del tray
descartaba silenciosamente los errores de `show()` y `set_focus()`. El
resultado podía ser un proceso visible sólo en la bandeja.

- [x] Añadir selección de monitor `current → primary → available → defaults` en `main_window_layout`, con tests puros de precedencia y fallback.
- [x] Declarar `visible: true` para `main`, preservando `visible: false` para `quick-paste`.
- [x] Centralizar desminimizar, mostrar y enfocar la ventana principal en `tray::present_main_window`, reutilizada por el arranque y por `Open ClipVault`, con logs de errores de plataforma.
- [x] Mantener la visibilidad independiente del resultado de cualquier consulta de monitor; no se toca Quick Paste, capturas, assets, clipboard, tags, colecciones ni drag-and-drop.
- [x] Añadir la regresión de configuración que fija `main.visible = true` y `quick-paste.visible = false`.
- [x] Documentar el contrato en `design.md` y en la delta spec `desktop-platform-integration`.
- [x] Incrementar versión canónica de `0.0.12` a `0.0.13` en manifests, lockfiles y `projects.md`, conforme a la regla de cambio funcional.
- [ ] Verificar manualmente en Ubuntu GNOME Wayland que el primer inicio abre el desktop aun si no existe monitor primario y que el tray lo restaura tras ocultarlo.

**Seguimiento de la prueba Ubuntu.** La inspección inicial con `wmctrl`
devolvió una ventana con `WM_CLASS = dev.warp.Warp`; ese resultado corresponde
al terminal Warp, no a la ventana Tauri de ClipVault. En Wayland `show()` puede
ejecutarse durante `setup` antes de que la superficie sea mapeable, por lo que
el shell también presenta la ventana al recibir `RunEvent::Ready`.

- [x] Reutilizar `present_main_window` en `RunEvent::Ready`, sin crear una
  segunda ruta de visibilidad ni alterar Quick Paste.
- [x] Cubrir que el evento `Ready` activa esta presentación y documentar el
  contrato de timing Wayland en el diseño y la delta spec.
- [ ] Verificar manualmente en Ubuntu GNOME Wayland el arranque posterior al
  evento `Ready`.

**Causa adicional identificada.** Al seleccionar `current_monitor()` en
Wayland, el shell pasó a llamar `set_position(0, 0)` durante `setup`. GNOME
es el propietario de la posición de una ventana toplevel y esa solicitud
previa al mapeo puede dejar la superficie invisible. La corrección conserva
el tamaño inicial, pero omite toda posición absoluta cuando `GDK_BACKEND` o
`XDG_SESSION_TYPE` indican Wayland.

- [x] Extraer y cubrir con tests la política pura que delega la posición al
  compositor Wayland y conserva el comportamiento para X11.
- [x] Evitar `set_position` en Wayland sin modificar el tamaño solicitado,
  Quick Paste, la integración GNOME, captura ni los datos persistidos.
- [ ] Verificar manualmente en Ubuntu GNOME Wayland que la ventana principal
  aparece tras omitir el posicionamiento absoluto.
