# linux-x11-compatibility

Corrección de los adapters Linux X11 para que compilen y se ejecuten contra
`x11rb 0.13.2`: ruta de import correcta para `RustConnection`, uso del campo
`roots` del handshake, `Arc` en lugar de `Rc` para satisfacer `Send + Sync`,
propagación real de los errores de `send_request` y `flush`, y tests
actualizados.

No introduce dependencias nuevas, no modifica adapters de macOS ni de
Wayland y no toca el frontend.