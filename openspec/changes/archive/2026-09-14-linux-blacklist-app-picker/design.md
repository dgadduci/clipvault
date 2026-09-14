# Diseño: selector visual de blacklist Linux

## Pregunta arquitectónica

El selector debe devolver el mismo identificador que usa el active-app probe.
Sin embargo, una sesión Linux puede ejecutar el binario sin que exista una
asociación determinista entre una entrada `.desktop` y el identificador
publicado por el adaptador activo. El diseño debe:

1. Derivar la disponibilidad del selector del **probe activo** (el
   `CachedActiveApplication` que el capture loop está consultando, no el
   `DisplayServer` estático ni el `ActiveAppDiagnosticsState.backend`
   inicial) cruzado con el consentimiento y el estado técnico **runtime** de
   la integración GNOME. `cache_populated` queda fuera de la decisión: el
   catálogo deriva su identificador de las entradas `.desktop` instaladas,
   por lo que una sesión sin ventana activa no invalida la asociación del
   catálogo.
2. Cuando el resolver determina que la asociación es segura, devolver el
   identificador determinista exacto que el adaptador publica.
3. Limitar el catálogo a entradas que pueden producir un identificador
   determinista bajo la estrategia activa (`StartupWMClass`,
   `X-GNOME-WMClass`, filename stem, `Exec=` basename, o el Desktop File
   ID para la ruta GNOME).
4. Deduplicar por identificador normalizado en minúsculas; la precedencia
   XDG gana.
5. El comando `add` revalida el identificador contra el catálogo vivo y
   usa la metadata del catálogo, no del frontend.
6. Cancelación, sesión `Unsupported` o identificador no presente en el
   catálogo: la blacklist y los assets permanecen inalterados.

## Límites

- El core continúa consumiendo el catálogo a través de comandos Tauri
  delgados; no se introduce un nuevo trait.
- La resolución del backend vive en [`clipvault_core::linux_picker`]
  como función pura testeable; consume snapshots del estado del runtime
  sin tocar el `AppContext`.
- X11 y Wayland se validan por separado; XWayland reutiliza la rama
  X11/EWMH porque XWayland es un servidor X11.
- Un resultado ambiguo debe producir `Unsupported` o un error local
  equivalente, nunca un identificador generado.

## Decisiones arquitectónicas nuevas

### `LinuxPickerBackend` (typed enum en `clipvault-platform`)

Cuatro variantes: `X11OrXWaylandEwmh`, `WaylandNative`,
`GnomeShellExtension`, `Unsupported`. El enum es declarativo: la función
`strategy()` devuelve el `IdentifierStrategy` aplicable o `None` para
`Unsupported`.

### `resolve_linux_picker_backend(LinuxPickerSessionState)` (en
`clipvault-core::linux_picker`)

Función pura, total, testeable sin display real. Recibe un snapshot
del estado (`backend`, `gnome_consent`, `gnome_technical_state`) y
devuelve uno de los cuatro variantes.

La matriz de decisión es pública y está cubierta por tests unitarios
que ejercitan cada rama (12 tests cubriendo X11/XWayland/Wayland
nativo/GNOME con consentimiento presente o ausente, conexión activa
o inactiva, `ActivationPending` rechazado y backends no soportados).

**Decisión revisada:** `cache_populated` deja de participar en la
decisión. La justificación es que el selector deriva el identificador
del catálogo `.desktop`, no del cache del probe: la única condición
necesaria es que el backend activo sea reconocible. La consecuencia
práctica es que una sesión sin ventana activa sigue ofreciendo el
selector.

**GNOME Shell extension** sólo resuelve a `GnomeShellExtension` cuando
el consentimiento es `Accepted` **y** el estado técnico es uno de
`Connected`, `Identified` o `NoActiveApplication`. `ActivationPending`,
`Disconnected`, `NotInstalled`, `Disabled`, `Incompatible`,
`CommunicationError` y `Unavailable` resuelven a `Unsupported`. La
motivación es que `ActivationPending` aún no garantiza un handshake
completo con la extensión, por lo que el picker no debe comprometerse
a una asociación que la sesión puede no terminar cumpliendo.

### Estado técnico GNOME: persistencia frente a runtime

`GnomeTechnicalState` representa dos necesidades distintas:

- el valor persistido permite reconstruir una explicación conservadora al
  arrancar cuando todavía no existe un listener vivo;
- el valor runtime describe el `SharedGnomeSnapshot` actual y es la única
  fuente válida para autorizar el selector durante esta ejecución.

No se deben persistir `Connected`, `Identified` o `NoActiveApplication` en
cada transición: la extensión revisa el foco periódicamente y convertir esas
transiciones en escrituras SQLite introduciría I/O innecesario y estado
obsoleto de todos modos. El core conservará un snapshot técnico runtime en
memoria, inicializado conservadoramente desde el estado persistido. El
adaptador GNOME del shell podrá actualizarlo con un enum tipado, sin
identificador, título, PID, ruta ni contenido del clipboard.

Justo antes de ejecutar `clipvault_ignored_app_linux_catalog` y
`clipvault_ignored_app_linux_add`, el adaptador fino Tauri sincroniza ese
snapshot desde el `SharedGnomeSnapshot` vivo si existe un handle GNOME. Luego
`AppContext::linux_picker_backend()` combina el nombre del probe activo,
consentimiento y snapshot runtime. Si no hay handle vivo, conserva el valor
persistido como degradación conservadora. Así, `NoActiveApplication` habilita
el catálogo —la ausencia se debe normalmente a que la propia ventana de
ClipVault tomó el foco—, mientras `ActivationPending` sigue devolviendo
`Unsupported`.

### `AppContext::linux_picker_backend()`

Helper que construye el snapshot desde el
[`CachedActiveApplication`] activo (vía `name()`, que refleja el probe
instalado por `Swap_active_app_probe`) y el `GnomeIntegrationService` y
delega en el resolver. El snapshot ya no consulta el backend del
`ActiveAppDiagnosticsState` porque ese campo se fija en el bootstrap y
no se actualiza tras el swap, lo que hacía que el picker quedase
bloqueado en la rama X11/EWMH después de que el GNOME swap tomara el
control. Para GNOME consume el estado técnico runtime en memoria; el estado
persistido sólo inicializa esa memoria antes de que exista un handle vivo.
Usado tanto por el comando de catálogo como por el comando `add` para que las
dos superficies nunca puedan discrepar sobre qué estrategia aplica.

### Wiring de la sonda Wayland en la build Linux

La feature `linux-wayland-active-app` pertenece al crate
`clipvault-platform` y se habilita desde la dependencia target-specific de
Linux del shell. Las ramas de `build_active_application` que consumen ese
adaptador deben estar condicionadas por `target_os = "linux"`, no por la
feature homónima de `clipvault-app`: Cargo no propaga una feature de una
dependencia a la tabla `[features]` del paquete consumidor. Mantener el
`cfg(feature = "linux-wayland-active-app")` en el shell compila el adaptador
en `clipvault-platform`, pero deja la rama nativa fuera de la build normal y
produce `unavailable` o un fallback XWayland en Wayland.

La corrección conserva la separación de responsabilidades: la feature
continúa controlando qué módulo existe en `clipvault-platform`, mientras el
shell Linux consume ese módulo porque su dependencia target-specific ya lo
habilita. La feature declarada en `clipvault-app` no se elimina sin una
decisión separada, pero no puede ser la condición del wiring de producción.
El test estructural debe inspeccionar tanto la feature de la dependencia
como la ausencia de un gate homónimo en la rama Linux del bootstrap.

### Sincronización del artefacto frontend y el binario Tauri

La ruta común que produce el mensaje observado en X11 y Wayland es la
desincronización de artefactos. `tauri.conf.json` usa
`frontendDist: "../frontend/dist"` para ejecuciones que no usan el servidor
de desarrollo. Si ese directorio fue generado antes de que
`SettingsPanel.svelte` incorporara `ignoredAppLinuxCatalogCommand` y
`linux-picker-modal`, la UI antigua invoca directamente
`ignoredAppPickAndAddCommand`; el backend legado responde
`unsupported_session` en ambas sesiones y se muestra el texto de fallback.

Por lo tanto, la prueba manual debe usar un bundle frontend generado después
del cambio y un binario Tauri recompilado desde el mismo checkout. La
verificación mínima es confirmar que el bundle contiene el selector Linux y
que el binario registra `clipvault_ignored_app_linux_catalog` y
`clipvault_ignored_app_linux_add`. Después se ejecuta el entry point canónico
`cd app/tauri && cargo tauri dev`, o se ejecuta `npm run build` inmediatamente
antes de una ruta que consuma `frontendDist`; se debe cerrar cualquier
instancia anterior para que no sobreviva una combinación vieja de frontend y
backend.

Esta comprobación es independiente de la sesión gráfica. Una vez descartada
la desincronización, X11 debe resolver por `x11_ewmh`/`xwayland_ewmh`, mientras
Wayland además requiere que la rama de la sonda nativa no haya sido excluida
por compilación y que el runtime publique un backend soportado.

### `clipvault_ignored_app_linux_add` revalida siempre

1. Llama a `linux_picker_backend()` y rechaza con `unsupported_session`
   si devuelve `Unsupported`.
2. Re-corre el catálogo con la estrategia resuelta y busca el
   identificador solicitado vía `catalog.find(...)`.
3. Si el identificador no está, rechaza con `unsupported_session`.
4. Si está, persiste usando el `display_name` y `icon_ref` del catálogo
   (ignora los del frontend).
5. Las cancelaciones del modal nunca llaman al comando; la blacklist
   permanece vacía.
6. El comando `add` consume `add_with_metadata`, que devuelve
   `PickAndAddOutcome::{Added, Updated}` y nunca `Cancelled`. La rama
   `Cancelled` del mapeo se trata como `unreachable!()`: el flujo del
   catálogo no tiene un equivalente del "el usuario cerró el diálogo"
   del picker, así que la única forma de alcanzar esa variante sería
   un bug introducido por un cambio futuro. El pin explícito hace
   visible ese escenario en lugar de fabricar una fila `Added`
   fantasma con identificador vacío.

### `LinuxApplicationCatalog::find(identifier)`

Lookup case-insensitive (normalización ASCII lowercase) que respeta la
misma precedencia que `LinuxApplicationMetadataProvider::lookup`. Es el
único método por el que el backend puede reconocer un identificador
como seguro.

## Plan de investigación

1. Auditar los identificadores reales publicados por los adaptadores X11,
   Wayland y GNOME Shell. *(completado en commits previos)*
2. Definir una tabla de asociaciones permitidas y sus prioridades.
   *(completado en `linux_app_metadata.rs`)*
3. Implementar el resolver tipado y el catálogo con tests sin display
   real.
4. Conectar la superficie Tauri: comandos `catalog` y `add`, modal del
   settings panel con icono resuelto.
5. Cubrir X11, Wayland/XWayland, sesiones no soportadas y cancelación
   sin mutaciones.
6. Corregir la lectura del backend activo: el resolver debe observar el
   probe instalado por `swap_active_app_probe`, no el hint
   `DisplayServer` capturado en el bootstrap. *(completado)*
7. Eliminar `cache_populated` como requisito del resolver y rechazar
   `ActivationPending` como estado GNOME operativo. *(completado)*
8. Reemplazar la rama `Cancelled => Added { entry: empty }` del
   comando `add` por `unreachable!()`. *(completado)*
9. Corregir el gate de compilación del shell para que la sonda nativa
   Wayland forme parte de la build Linux normal y cubrirlo con una regresión
   estructural y un check del binario con las features efectivas.
10. Verificar que `frontendDist` y el binario Tauri provienen del mismo
    checkout antes de atribuir un `unsupported_session` al runtime gráfico.
11. Separar el estado técnico GNOME persistido del snapshot runtime para el
    picker. Sincronizar el enum runtime desde el handle vivo inmediatamente
    antes de `catalog` y `add`, sin escribir SQLite por transiciones de foco.

## Pruebas

- 12 tests unitarios en `linux_picker::tests` ejercitando la matriz del
  resolver (X11/XWayland/Wayland nativo/GNOME con consentimiento
  presente o ausente, conexión activa o inactiva, `ActivationPending`
  rechazado y backends no soportados).
- 2 tests de wiring en `linux_picker::wiring` que validan que
  `AppContext::linux_picker_backend()` sigue al probe instalado por
  `swap_active_app_probe` (incluyendo GNOME con consentimiento y
  técnico sincronizados, y la verificación de que el resolver lee el
  nombre del probe, no el `backend` del `ActiveAppDiagnosticsState`).
- 17 tests de integración en
  `app/tauri/src-tauri/tests/linux_picker_command.rs` cubriendo X11,
  XWayland, Wayland nativo, GNOME operativo, GNOME sin consentimiento,
  GNOME en `ActivationPending`, GNOME sin extensión, GNOME
  desconectado, sesión `Unknown`, catálogo vacío, cancelación sin
  mutaciones, rechazo de identificadores arbitrarios y persistencia con
  metadata del catálogo.
- Tests unitarios en `linux_app_catalog::tests` cubriendo dedup por
  minúsculas, prioridad XDG y el método `find`.
- Tests frontend en `privacySettings.test.ts` cubriendo el wrapper
  IPC y la propagación de errores.
- Regresión core/shell: con probe `gnome_shell_extension`, consentimiento
  `Accepted`, estado persistido `ActivationPending` y snapshot runtime
  `NoActiveApplication`, el catálogo y el comando `add` resuelven
  `GnomeShellExtension`; sin snapshot runtime vivo conservan
  `Unsupported`.
