## Why

En Linux, el botón de selección visual de aplicaciones de Privacidad informa
que la selección visual todavía no está disponible y deja como alternativa
introducir identificadores manualmente. La blacklist sigue siendo utilizable,
pero la experiencia no permite elegir una aplicación de forma reconocible.

El cambio `linux-wayland-desktop-file-icons` cierra explícitamente esa
superficie como trabajo diferido porque resolver un `.desktop` no basta por sí
solo: el identificador elegido debe coincidir con el valor que el adaptador
activo de X11 o Wayland publica al PrivacyGate.

## What Changes

- Implementar el selector visual Linux detrás del trait existente de
  `ApplicationPicker` mediante un **catálogo** que enumera las entradas
  `.desktop` instaladas y devuelve el identificador determinista que el
  adaptador activo publica, **sin ejecutar** ningún archivo.
- Definir un resolver tipado en [`clipvault_core::linux_picker`] que
  deriva el backend del picker a partir del estado real del runtime:
  - el nombre del probe activo que usa la captura para X11, XWayland y
    Wayland nativo, sin depender de `cache_populated`;
  - el nombre del probe GNOME activo + consentimiento `Accepted` + el
    estado técnico **vivo** de la integración. Sólo `Connected`,
    `Identified` y `NoActiveApplication` habilitan la ruta GNOME;
    `ActivationPending` sigue siendo no operativo.
  - Cualquier otro caso devuelve `Unsupported`.
- Asegurar que la build Linux normal del shell incluya la sonda nativa
  Wayland: la feature se habilita en la dependencia
  `clipvault-platform`, por lo que el wiring de `clipvault-app` no debe
  quedar detrás de una feature homónima del shell que no se activa por
  defecto.
- Mantener sincronizados el bundle frontend y el binario Tauri durante la
  verificación manual: el flujo Linux debe incluir el modal y el comando
  `clipvault_ignored_app_linux_catalog`; un `frontendDist` generado antes del
  cambio no debe degradar silenciosamente a la UI del picker legado.
- El catálogo no deriva la estrategia sólo de `DisplayServer` ni del
  consentimiento persistido; el consentimiento GNOME sin la extensión
  realmente instalada/conectada se trata como `Unsupported`.
- Mantener el ingreso manual como fallback cuando la sesión no admita
  una asociación segura.
- El comando `clipvault_ignored_app_linux_add` **revisa el identificador
  contra el catálogo vivo** y usa la metadata (display_name, icon_ref)
  que el catálogo publica; rechaza identificadores arbitrarios y nunca
  confía en el payload del frontend.
- Cancelación, sesión `Unsupported` o catálogo vacío no modifican la
  blacklist ni crean assets.
- Mostrar el icono resuelto por el catálogo dentro del modal usando el
  `iconResolver` existente; fallback a la inicial cuando no se pueda
  resolver.
- Deduplicación por identificador normalizado en minúsculas; la
  precedencia XDG gana cuando dos raíces declaran el mismo ID.

### Regresión descubierta en la validación manual de GNOME Wayland

La validación real en Ubuntu GNOME 42.9, después de reinstalar el recurso y
correr una sesión nueva, llega a `technical_state = no_active_application`,
con consentimiento `accepted` e identificador vacío. Ese estado demuestra que
el bridge ya completó `hello` y publicó una ausencia de aplicación; no es una
extensión sin cargar ni un fallo de instalación.

Sin embargo, el selector devuelve `Unsupported` y muestra el mensaje de que
el adaptador no está disponible. La tarjeta GNOME lee el
`SharedGnomeSnapshot` vivo, pero `AppContext::linux_picker_backend()` lee el
último estado técnico persistido, guardado como `activation_pending` al iniciar
el listener. Las transiciones posteriores del listener no deben forzar
escrituras SQLite, por lo que ese valor persistido queda obsoleto. El catálogo
debe decidir usando un snapshot runtime sincronizado al momento de invocar
`catalog` o `add`, sin persistir cada cambio de foco.

## Non-Goals

- No ejecutar la aplicación seleccionada.
- No usar títulos, PID, `/proc`, una lista global de ventanas, red, D-Bus ni
  heurísticas de similitud para inventar un identificador.
- No cambiar PrivacyGate, el matcher de blacklist ni el contenido de
  capturas.
- No eliminar el ingreso manual ni hacer obligatorio el selector visual.
- No aceptar identificadores arbitrarios en `clipvault_ignored_app_linux_add`
  aunque el frontend los envíe; el catálogo es la única autoridad.

## Criterio de éxito

El selector visual Linux sólo se ofrece cuando el resolver determina que el
backend activo publica un identificador estable. Si no existe esa garantía,
devuelve `Unsupported` y la UI conserva el ingreso manual. La selección
cancelada o no soportada no modifica la blacklist ni crea assets. El
comando `add` rechaza cualquier identificador que no esté en el catálogo de
la sesión actual.
