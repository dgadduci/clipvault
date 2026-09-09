# Design: linux-x11-compatibility

## Context

`x11rb 0.13.2` reorganizó la API respecto de versiones anteriores:

- `RustConnection` dejó de reexportarse en el crate root. La ruta actual es
  `x11rb::rust_connection::RustConnection`.
- El campo `screens: Vec<Screen>` del handshake pasó a llamarse `roots:
  Vec<Screen>`. Verificado contra
  `x11rb-protocol-0.13.2/src/protocol/xproto.rs` y el ejemplo oficial
  (`x11rb-0.13.2/src/lib.rs:51`: `let screen = &conn.setup().roots[screen_num];`).
- Los errores están agrupados bajo `x11rb::errors::*`. No existe
  `x11rb::protocol::Error`.
- `RustConnection` (versión `DefaultStream`) sigue siendo `Send + Sync`:
  todos sus campos internos (`std::sync::Mutex`, `std::sync::Condvar`,
  `Setup`, `ExtensionManager`, `MaxRequestBytes`, `IdAllocator`) lo son.
  El único `unsafe impl Send/Sync` del crate vive en
  `xcb_ffi::XcbConnectionWrapper`, que no se usa en el código de ClipVault.

## Estrategia de cambio

Cambios acotados, sin reescritura funcional:

### `linux_x11_active_app.rs`

1. Importar `RustConnection` desde `x11rb::rust_connection`.
2. Reemplazar `Rc<X11State>` por `Arc<X11State>`. Como `X11State` solo
   contiene `RustConnection`, `Window` y `u32`/`u8` ya `Send + Sync`, el
   `Arc<X11State>` resultante es automáticamente `Send + Sync`. No se
   necesitan `unsafe impl Send`/`Sync` manuales.
3. Sustituir `conn.setup().screens[screen_number]` por
   `conn.setup().roots[screen_number]`.
4. Eliminar el marcador `_atom_marker` y el `xproto::Atom` re-import que solo
   existían para silenciar un warning previo; al haber un import real de
   `x11rb::protocol::xproto::AtomEnum` y `Window`, ya no es necesario.
5. Mantener `parse_wm_class` y los dos tests existentes
   (`wm_class_class_segment_is_used_as_identifier`,
   `wm_class_falls_back_to_instance_when_class_missing`).
6. Agregar tests nuevos (sin servidor X real) que demuestren el contrato del
   helper `parse_wm_class`, la rama "no hay `WM_CLASS` y tampoco
   `_NET_WM_NAME`" y el uso de la ruta de import correcta.

### `linux_x11_paste.rs`

1. Importar `RustConnection` desde `x11rb::rust_connection`.
2. Reemplazar `Rc<X11State>` por `Arc<X11State>` (mismo razonamiento).
3. Sustituir `conn.setup().screens[screen_number]` por
   `conn.setup().roots[screen_number]`.
4. Eliminar `use x11rb::protocol::xtest::ConnectionExt as XTestConnectionExt`
   (no se usa: `xtest::FakeInputRequest` ya implementa
   `x11rb::x11_utils::VoidRequest` y se envía vía
   `Connection::send_request_without_reply`).
5. Reescribir `send_fake_key`:
   - Tipo de retorno `Result<(), x11rb::errors::ConnectionError>`.
   - `conn.send_request_without_reply(&request)` (request concreto de tipo
     `xtest::FakeInputRequest` con el opcode que devuelve
     `QueryExtension` para `XTEST`) y propagar el error.
   - `conn.flush()?` y propagar el error.
   - El opcode real (no el que devolvía `query_extension`) se usa para
     serializar el request (`FakeInputRequest::serialize(opcode)` recibe el
     major opcode que el servidor asignó a la extensión).
6. Conservar los keycodes 37 (Ctrl) y 55 (v) y el orden Ctrl-down, V-down,
   V-up, Ctrl-up.
7. Agregar tests que cubran:
   - La firma `Result<(), ConnectionError>` y la propagación de errores.
   - El keycode y la dirección (KeyPress/KeyRelease) del request enviado.
   - Que el flujo llama cuatro veces a `send_fake_key` con la secuencia
     correcta.
   - Que el wrapper `from_state` (test-only) sigue permitiendo construir el
     controller para tests unitarios.

## Compatibilidad con macOS y Wayland

Estos adapters son el único punto tocado. El árbol de features y el bootstrap
no cambian:

- macOS sigue usando `macos_active_app` / `macos_paste` (sin cambios).
- Wayland sigue resolviendo a `NoopActiveApplicationProbe` /
  `NoopPasteController`.
- El frontend y los tests TS no se ven afectados.

## Apéndice: regresión cruzada del shell y corrección

El primer build del shell en Ubuntu (host real, ya fuera del entorno
de macOS) falló con
`cannot find type MainQueueActiveAppRefresher in crate clipvault_platform`
aunque los adapters X11 ya compilaban correctamente. La causa raíz fue
que `app/tauri/src-tauri/src/bootstrap.rs` referenciaba el tipo
macOS-only desde tres sitios sin `cfg` de protección:

- el campo `AppState::active_app_refresher`;
- la firma de la rama macOS y la firma de la rama Linux de
  `install_active_app_main_queue_refresher`;
- `MainQueueActiveAppRefresher::install(...)` dentro del cuerpo de la
  rama macOS.

El re-export del crate `clipvault-platform` se mantiene estrictamente
detrás de `#[cfg(all(target_os = "macos", feature = "macos-native"))]`
— no se relaja ni se mueve. La corrección introduce un alias neutral
`clipvault_platform::ActiveAppRefresherHandle` con doble `cfg`:

- `#[cfg(all(target_os = "macos", feature = "macos-native"))]` →
  `pub type ActiveAppRefresherHandle = MainQueueActiveAppRefresher;`
- `#[cfg(not(all(target_os = "macos", feature = "macos-native")))]` →
  `pub type ActiveAppRefresherHandle = ();`

El shell usa el alias en los tres sitios. La rama Linux sigue
devolviendo `(MainQueueInstallOutcome::SkippedUnsupported, None)`,
igual que antes, pero ahora `None` envuelve el alias neutral en vez
de referenciar directamente el símbolo macOS-only. La rama macOS
sigue llamando a `ActiveAppRefresherHandle::install(...)` — el alias
resuelve al tipo real, así que el timer, el diagnóstico
`refresher_installed`, el contador de callbacks y el refresco de la
caché de la app activa se conservan sin cambios.

Se añade `#[cfg_attr(not(target_os = "macos"), allow(unused_variables))]`
al parámetro `info: &PlatformInfo` de `build_clipboard` para silenciar
el warning sin alterar el comportamiento: la rama macOS sigue
consultando `info.os_family`, la rama Linux / no-macOS deja el slot
sin usar sin tener que renombrar a `_info` (lo que generaría un
`#[cfg(target_os = "macos")]` interno adicional en el cuerpo).

No se introdujo ningún bloque `unsafe` nuevo (incluido `unsafe impl
Send` / `unsafe impl Sync`); el alias neutral funciona vía reglas
normales de auto-trait deduction. La rama Linux no necesita
construir ni importar el refresher macOS, sino simplemente devolver
`None` y mantener `AppState.active_app_refresher = None`.

## Tests

- Tests unitarios puros (sin servidor X): el helper `parse_wm_class` ya es
  público en el módulo `tests`. Se agregan dos casos:
  - Cadena vacía → instance y class vacíos → el identificador cae al
    instance vacío y `active_application` debe devolver `Ok(None)`. Esta
    lógica ya vive en el adapter y se cubre con un test directo sobre el
    helper.
  - Cadena con un único segmento sin NUL → se usa ese segmento como
    instance y class a la vez.
- Test de contrato para `send_fake_key`: en lugar de levantar un servidor X
  (no disponible en macOS y fuera del alcance del cambio), se expone un
  constructor de tests `from_state` que recibe un estado pre-armado. Como el
  estado real requiere una `RustConnection` conectada, el test se concentra
  en el contrato de error: un tipo `Result<(), ConnectionError>` verificable
  mediante el wrapper expuesto y en el orden de invocación documentado
  (cuatro llamadas, sequence Ctrl/V arriba/abajo).
- Test que verifica que `x11rb::rust_connection::RustConnection` es
  importable y que `Setup::roots` es la ruta correcta (assertions estáticas
  sobre los tipos, sin instancia real).
- Los tests de `active_app.rs` y `paste.rs` (macOS/noop) siguen pasando sin
  cambios; esto cubre la no-regresión macOS.
- Se mantiene el rechazo de `unsafe impl Send/Sync` manuales.

## Verificación

- macOS: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  -- -D warnings`, `cargo test --workspace`, `npm run check`, `npm run
  build`, `npm test`.
- Ubuntu (host real, posterior, fuera de este cambio):
  `cargo check -p clipvault-platform --features linux-x11`,
  `cargo check -p clipvault-app --no-default-features --features
  clipboard-arboard,hotkey-global`,
  `cargo test --workspace`.

Las verificaciones manuales (X11 real, captura, pegado) quedan pendientes
hasta ejecutarse en un host Ubuntu con sesión X11 disponible. No se marcan
como completadas en este cambio.