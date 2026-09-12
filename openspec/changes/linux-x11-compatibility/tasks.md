## 1. Compilación de los adapters Linux X11

- [x] 1.1 Reemplazar `use x11rb::RustConnection;` por
  `use x11rb::rust_connection::RustConnection;` en
  `linux_x11_active_app.rs` y `linux_x11_paste.rs`.
- [x] 1.2 Sustituir `conn.setup().screens[screen_number]` por
  `conn.setup().roots[screen_number]` en ambos adapters.
- [x] 1.3 Reemplazar `Rc<X11State>` por `Arc<X11State>` en
  `X11ActiveApplication` y `X11PasteController`. No usar `unsafe impl Send`
  ni `unsafe impl Sync` manuales.
- [x] 1.4 Eliminar el import sin usar de
  `x11rb::protocol::xtest::ConnectionExt` en `linux_x11_paste.rs`.

## 2. Manejo de errores en el pegado X11

- [x] 2.1 Reescribir `send_fake_key` con tipo de retorno
  `Result<(), x11rb::errors::ConnectionError>`.
- [x] 2.2 Propagar los errores de `send_request_without_reply` (no
  ignorarlos).
- [x] 2.3 Propagar los errores de `flush`.
- [x] 2.4 Conservar el envío real del evento XTEST
    (`xtest::FakeInputRequest`) y los keycodes 37 (Ctrl) y 55 (v).

## 3. Comportamiento conservado

- [x] 3.1 Mantener la detección de la ventana activa mediante
  `_NET_ACTIVE_WINDOW`.
- [x] 3.2 Mantener la lectura de `WM_CLASS` como identificador estable y
  fallback a `_NET_WM_NAME` solo cuando `WM_CLASS` falta.
- [x] 3.3 Mantener las variantes existentes de `ActiveAppError` y
  `PasteError`. No añadir variantes nuevas.
- [x] 3.4 Mantener intactas las features `linux-x11`, `clipboard-arboard`
  y `hotkey-global`.
- [x] 3.5 No modificar adapters ni código específico de macOS.

## 4. Tests

- [x] 4.1 Tests que cubren la lógica `WM_CLASS` (instance vs class, fallback
  a instance cuando class está vacío, fallback a `_NET_WM_NAME` cuando
  `WM_CLASS` falta).
- [x] 4.2 Tests que verifican el tipo `Result<(), ConnectionError>` del
  helper `send_fake_key` y el orden Ctrl-down, V-down, V-up, Ctrl-up.
- [x] 4.3 Tests estáticos sobre la ruta correcta de import
  (`x11rb::rust_connection::RustConnection`) y el campo `roots` del
  `Setup`.
- [x] 4.4 Tests que no regresionen macOS: la suite existente de
  `active_app.rs` y `paste.rs` debe seguir verde sin cambios.
- [x] 4.5 Confirmar que no se introdujo `unsafe` (ni `unsafe impl Send`
  ni `unsafe impl Sync`).

## 5. Verificación

- [x] 5.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 5.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 5.3 Ejecutar `cargo test --workspace`.
- [x] 5.4 Ejecutar `npm run check` y `npm run build` y `npm test` en
  `app/tauri/frontend`.
- [ ] 5.5 Pendiente en Ubuntu: `cargo check -p clipvault-platform
  --features linux-x11`, `cargo check -p clipvault-app
  --no-default-features --features clipboard-arboard,hotkey-global`,
  `cargo test --workspace`. No se marca como completada hasta correr
  realmente en el host Ubuntu.
- [x] 5.6 Validado en Ubuntu X11 real: se leyó `WM_CLASS` de la ventana
  activa, se detectaron cambios de foco y el pegado sintético en la app
  activa fue confirmado manualmente el 2026-09-11. La shell nueva arrancó
  con `display=x11` y las capacidades de clipboard, hotkey, pegado sintético
  y aplicación activa habilitadas.

## 6. Regresión cruzada: `MainQueueActiveAppRefresher` en Ubuntu

El primer build del shell en Ubuntu falló con
`cannot find type MainQueueActiveAppRefresher in crate clipvault_platform`
aunque los adapters X11 compilaban correctamente. La causa raíz fue
que `app/tauri/src-tauri/src/bootstrap.rs` referenciaba el tipo
macOS-only desde tres sitios sin `cfg` de protección:

- el campo `AppState::active_app_refresher`;
- la firma de la rama macOS y la firma de la rama Linux de
  `install_active_app_main_queue_refresher`;
- `MainQueueActiveAppRefresher::install(...)` dentro del cuerpo de la
  rama macOS.

El re-export del crate `clipvault-platform` sigue siendo macOS-only
(`#[cfg(all(target_os = "macos", feature = "macos-native"))]`) y no
se relaja. La corrección introduce un alias neutral
`clipvault_platform::ActiveAppRefresherHandle` que el shell usa en
los tres sitios.

- [x] 6.1 Mantener el re-export de `MainQueueActiveAppRefresher`
  detrás del `cfg(all(target_os = "macos", feature = "macos-native"))`
  existente. No se mueve ni se relaja el gate en
  `crates/clipvault-platform/src/lib.rs`.
- [x] 6.2 Introducir el alias `pub type ActiveAppRefresherHandle`
  en `crates/clipvault-platform/src/lib.rs` con doble `cfg`:
  macOS → `MainQueueActiveAppRefresher`, otros → `()`.
- [x] 6.3 Sustituir `clipvault_platform::MainQueueActiveAppRefresher`
  por `clipvault_platform::ActiveAppRefresherHandle` en el campo
  `AppState::active_app_refresher`.
- [x] 6.4 Sustituir el tipo de retorno en ambas firmas de
  `install_active_app_main_queue_refresher` (macOS y Linux) por
  `Option<clipvault_platform::ActiveAppRefresherHandle>`.
- [x] 6.5 Sustituir la llamada interna
  `MainQueueActiveAppRefresher::install(...)` por
  `clipvault_platform::ActiveAppRefresherHandle::install(...)`.
- [x] 6.6 Mantener intacto el comportamiento macOS: instalación del
  timer, `mark_refresher_installed`, contador de callbacks
  (`record_timer_callback`), refresco de la caché de la app activa y
  cleanup por ownership del handle.
- [x] 6.7 Mantener intacta la rama Linux: sigue devolviendo
  `(MainQueueInstallOutcome::SkippedUnsupported, None)` sin crear ni
  importar el refresher macOS.
- [x] 6.8 Corregir el warning de `build_clipboard(info: &PlatformInfo, _capabilities: Capabilities)`
  en compilaciones no-macOS con
  `#[cfg_attr(not(target_os = "macos"), allow(unused_variables))] info`
  para no introducir un parámetro `_info` que sería ruido en la
  rama macOS donde `info` sí se consulta.
- [x] 6.9 Tests de cobertura del contrato:
  - `app_state_active_app_refresher_uses_platform_neutral_alias`
    demuestra que `AppState` compila en macOS y Linux con el alias
    neutral.
  - `install_active_app_main_queue_refresher_returns_skipped_unsupported_on_linux`
    (cfg no-macOS) verifica que Linux devuelve
    `SkippedUnsupported` y `None`.
  - `_assert_linux_does_not_reference_macos_only_refresher_type`
    (cfg no-macOS, helper puro de compilación) impide un refactor
    futuro que vuelva a referenciar el símbolo macOS-only fuera
    del `cfg`.
  - `macos_active_app_refresher_handle_resolves_to_real_refresher_type`
    (cfg macOS) fija con `TypeId` que el alias resuelve al tipo
    `MainQueueActiveAppRefresher` real, preservando el timer.
  - `bootstrap_active_app_refresher_wiring_stays_safe` parsea
    `bootstrap.rs`, descarta comentarios y strings y falla si
    aparece un `unsafe {` / `unsafe fn` / `unsafe impl` /
    `unsafe trait` nuevo, demostrando que no se introdujeron
    implementaciones `unsafe` (incluido `unsafe impl Send`/`Sync`)
    para enmascarar el alias.
- [x] 6.10 `cargo clippy --workspace --all-targets -- -D warnings`
  sigue limpio en macOS.
- [x] 6.11 `cargo test --workspace` sigue verde en macOS (72 tests
  en `bootstrap::tests`).
- [ ] 6.12 Pendiente en Ubuntu: `cargo check -p clipvault-app
  --no-default-features --features clipboard-arboard,hotkey-global`
  y `cargo test --workspace`. No se marca como completada hasta
  correr realmente en el host Ubuntu.
