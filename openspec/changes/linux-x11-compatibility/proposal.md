# Linux x11 compatibility

Corrección para que los adapters Linux X11 (`X11ActiveApplication` y
`X11PasteController`) compilen y se ejecuten contra `x11rb 0.13.2`, que es la
versión resuelta por el workspace. El frontend Vite ya funciona en Ubuntu y
`x11rb 0.13.2` ya compila; los únicos puntos pendientes son los adapters y la
regresión cruzada del shell que re-referenciaba el tipo macOS-only
`MainQueueActiveAppRefresher` desde sitios sin `cfg` de protección.

## Why

La rama `fix/linux-x11-compatibility` se creó para cerrar la compilación de
`clipvault-platform` con la feature `linux-x11`. El código actual usa APIs que
no existen en `x11rb 0.13.2` o que están mal direccionadas:

- `use x11rb::RustConnection;` falla con `unresolved import`. La ruta real es
  `x11rb::rust_connection::RustConnection`.
- `conn.setup().screens[screen_number]` no existe. `Setup` expone `roots:
  Vec<Screen>` (no `screens`).
- `x11rb::protocol::Error` no existe en `x11rb 0.13.2`. Los errores reales
  viven en `x11rb::errors::{ConnectionError, ReplyError, ReplyOrIdError,
  ConnectError, ParseError}`.
- `Rc<X11State>` no permite que `X11ActiveApplication` y `X11PasteController`
  sean `Send + Sync`. Ambos son wrapeados en `Arc<dyn ActiveApplicationProbe>`
  y `Arc<dyn PasteController>` por el bootstrap, que requieren
  `Send + Sync`.
- `use x11rb::protocol::xtest::ConnectionExt` queda importado sin usar.
- `send_fake_key` ignora los errores de `send_request` y `flush`, y firma su
  tipo de retorno como `Result<(), x11rb::protocol::Error>`, que tampoco
  existe.
- `app/tauri/src-tauri/src/bootstrap.rs` referenciaba
  `clipvault_platform::MainQueueActiveAppRefresher` desde el campo
  `AppState::active_app_refresher`, desde la firma macOS, desde la firma
  Linux y desde el cuerpo de la rama macOS sin un `cfg` común, lo que
  rompe la compilación cruzada del shell en Ubuntu cuando los adapters
  X11 ya compilan.

El comportamiento que sí debe conservarse:

- Detección de la ventana activa mediante `_NET_ACTIVE_WINDOW`.
- Lectura de `WM_CLASS` como identificador estable.
- Fallback de etiqueta a `_NET_WM_NAME` solo cuando `WM_CLASS` falta.
- Pegado sintético `Ctrl+V` mediante `XTEST` (`XTestFakeKeyEvent` / `xtest`
  `FakeInput`).
- Errores tipados `ActiveAppError` y `PasteError` (variantes existentes, sin
  añadir variantes nuevas).
- Features `linux-x11`, `clipboard-arboard` y `hotkey-global`.
- Adapters y comportamiento específicos de macOS.
- En macOS, instalación real del `MainQueueActiveAppRefresher`,
  diagnóstico `refresher_installed`, contador de callbacks, refresco de
  la caché de la app activa y cleanup por ownership del handle.

## What Changes

- Sustituir el import roto de `RustConnection` por la ruta real
  `x11rb::rust_connection::RustConnection`.
- Cambiar `conn.setup().screens[screen_number]` por
  `conn.setup().roots[screen_number]` en ambos adapters.
- Reemplazar `Rc<X11State>` por `Arc<X11State>` en `X11ActiveApplication` y
  `X11PasteController` para que ambos satisfagan `Send + Sync` requerido por
  `ActiveApplicationProbe` y `PasteController`. No se usan implementaciones
  `unsafe impl Send/Sync` manuales.
- Reescribir `send_fake_key` para que use el tipo de error real de `x11rb`
  (`x11rb::errors::ConnectionError`), no ignore los errores de `send_request`
  ni de `flush`, y conserve el envío real de eventos mediante XTEST.
- Eliminar el import sin usar de `ConnectionExt` desde XTEST y cualquier
  campo, parámetro o variable muerta que haya dejado el refactor.
- Agregar o actualizar tests unitarios para cubrir:
  - Uso correcto de `RustConnection` y `Arc`.
  - Uso de `conn.setup().roots[screen_number]`.
  - Propagación de errores de `send_request` y `flush`.
  - Conservación de la lógica `WM_CLASS` y su fallback a `_NET_WM_NAME`.
  - Conservación del flujo XTEST (`FakeInput` con keycodes 37 y 55).
  - No regresión de macOS (los tests existentes deben seguir pasando).
  - No uso de `unsafe` para ocultar problemas de `Send`/`Sync`.
- Introducir el alias neutral `clipvault_platform::ActiveAppRefresherHandle`
  (macOS → `MainQueueActiveAppRefresher`, otros → `()`) y reemplazar las
  referencias directas en el shell para que `AppState`, las dos firmas de
  `install_active_app_main_queue_refresher` y la llamada interna
  `…::install(...)` ya no requieran el símbolo macOS-only.
- Corregir el warning de `build_clipboard(info: &PlatformInfo, ...)` en
  builds no-macOS con `#[cfg_attr(not(target_os = "macos"),
  allow(unused_variables))] info`.

## Capabilities

### Modified Capabilities

- `desktop-platform-integration`: los adapters Linux X11 usan la API correcta
  de `x11rb 0.13.2` y son `Send + Sync` para poder ser consumidos como
  `Arc<dyn ...>` desde el bootstrap; el shell referencia el refresher macOS
  exclusivamente a través del alias neutral `ActiveAppRefresherHandle`.

## Impact

- `crates/clipvault-platform/src/runtime/linux_x11_active_app.rs`.
- `crates/clipvault-platform/src/runtime/linux_x11_paste.rs`.
- `crates/clipvault-platform/src/lib.rs` (alias neutral).
- `app/tauri/src-tauri/src/bootstrap.rs` (uso del alias en el campo y en
  las firmas / llamada del helper de instalación; supresión del warning en
  `build_clipboard`; tests cross-platform).
- No se introducen dependencias nuevas.
- No se cambia el comportamiento de macOS ni de Wayland.
- No se introducen bloques `unsafe` (incluidos `unsafe impl Send` /
  `unsafe impl Sync` manuales) en el shell.