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
- [ ] 5.6 Pendiente en Ubuntu X11 real: pegar en la app activa, leer
  `WM_CLASS` de la app activa, detectar cambios de foco. No se marca
  como completada hasta ejecutar la sesión real.