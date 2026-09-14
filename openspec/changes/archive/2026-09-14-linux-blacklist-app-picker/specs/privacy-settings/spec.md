# Selector visual de aplicaciones Linux para la blacklist

## ADDED Requirements

### Requirement: La build Linux normal incluye la sonda Wayland

La build Linux normal de `clipvault-app` SHALL compilar la rama de
`build_active_application` que intenta la sonda `linux-wayland-active-app`
cuando la dependencia target-specific de `clipvault-platform` habilita esa
feature. El shell MUST NOT condicionar esa rama únicamente a una feature
homónima de `clipvault-app` que no forme parte de la configuración normal.

#### Scenario: Wayland nativo en la build normal

- GIVEN una build Linux normal con `linux-wayland-active-app` habilitada en
  `clipvault-platform`
- WHEN el host ejecuta una sesión Wayland y el compositor publica un
  protocolo compatible
- THEN el bootstrap intenta construir la sonda Wayland nativa antes del
  fallback XWayland
- AND el selector visual no cae a `Unsupported` únicamente porque la rama
  nativa fue excluida en compilación.

### Requirement: El selector frontend y los comandos Linux deben pertenecer al mismo build

La verificación y distribución del selector Linux SHALL usar un bundle
frontend generado después de la implementación del catálogo y un binario
Tauri que registre `clipvault_ignored_app_linux_catalog` y
`clipvault_ignored_app_linux_add` desde el mismo checkout. Una ejecución que
consuma un `frontendDist` anterior MUST detectarse como artefacto obsoleto y
no contarse como una prueba del runtime X11 o Wayland.

#### Scenario: Bundle actualizado en X11 o Wayland

- GIVEN el bundle frontend contiene `linux-picker-modal` y la invocación a
  `clipvault_ignored_app_linux_catalog`
- AND el binario Tauri registra los dos comandos Linux del catálogo
- WHEN el usuario pulsa `Seleccionar aplicación` en X11 o Wayland
- THEN la UI intenta primero el catálogo Linux
- AND la UI sólo conserva el fallback manual si el catálogo informa una
  sesión realmente `Unsupported` o un catálogo vacío.

#### Scenario: Artefacto frontend obsoleto

- GIVEN `frontendDist` fue generado antes de incorporar el catálogo Linux
- WHEN se ejecuta un binario Tauri que consume ese directorio
- THEN la ejecución se identifica como inválida para la prueba manual
- AND no se interpreta el mensaje del picker legado como evidencia de que
  X11 o Wayland carecen de soporte.

### Requirement: El selector Linux deriva su disponibilidad del runtime real

El selector visual Linux SHALL ofrecer una aplicación únicamente cuando el
backend del picker resuelva `X11OrXWaylandEwmh`, `WaylandNative` o
`GnomeShellExtension` a partir del estado real del runtime. El backend
se deriva del probe activo que el `CachedActiveApplication` envuelve,
del consentimiento y del estado técnico de la `GnomeIntegrationService`.
La presencia o ausencia de una ventana activa (es decir, la flag
`cache_populated`) no participa en la decisión: el catálogo deriva el
identificador de las entradas `.desktop` instaladas, no del cache del
probe. MUST NOT inventar un identificador a partir del nombre visible,
`Exec=`, PID, título o una ruta.

#### Scenario: Sesión Linux con asociación determinista

- GIVEN el probe activo expone uno de los nombres `x11_ewmh`,
  `xwayland_ewmh`, `wayland_foreign_toplevel` o
  `wayland_wlr_foreign_toplevel` (independientemente de si la caché
  ha observado o no una ventana)
- WHEN el usuario abre el selector visual
- THEN el picker devuelve el identificador estable, nombre e icono
  opcional
- AND la blacklist persiste el identificador como clave de matching

#### Scenario: XWayland usa la rama X11/EWMH

- GIVEN el probe activo expone `xwayland_ewmh`
- WHEN el usuario abre el selector visual
- THEN el backend resuelto es `X11OrXWaylandEwmh` y el catálogo usa la
  estrategia `wm_class`

#### Scenario: GNOME Shell extension aceptado y operativo

- GIVEN el probe activo expone `gnome_shell_extension`, el
  consentimiento es `Accepted` y el estado técnico es `Connected`,
  `Identified` o `NoActiveApplication`
- WHEN el usuario abre el selector visual
- THEN el backend resuelto es `GnomeShellExtension` y el catálogo usa la
  estrategia `desktop_file_id` (Desktop File ID literal)

#### Scenario: GNOME Shell extension en `ActivationPending`

- GIVEN el probe activo expone `gnome_shell_extension`, el
  consentimiento es `Accepted` y el estado técnico es `ActivationPending`
- WHEN el usuario abre el selector visual
- THEN el backend resuelto es `Unsupported` y la UI conserva el
  ingreso manual
- AND la razón reportada explica que la extensión aún no completó el
  handshake

#### Scenario: GNOME aceptado pero extensión ausente o inactiva

- GIVEN el probe activo expone `gnome_shell_extension` y el estado
  técnico es `NotInstalled`, `Disabled`, `Incompatible`,
  `Disconnected` o `CommunicationError`
- WHEN el usuario abre el selector visual
- THEN el backend resuelto es `Unsupported` y el catálogo devuelve el
  error tipado

#### Scenario: Sesión sin backend estable

- GIVEN `OsFamily == Linux` pero el probe activo expone `unavailable`
  o ningún probe está instalado
- WHEN el usuario abre el selector visual
- THEN el backend resuelto es `Unsupported` y la UI conserva el ingreso
  manual

#### Scenario: Catálogo vacío en sesión soportada

- GIVEN el backend resuelto es uno de los soportados pero no hay
  aplicaciones instaladas con identificador determinista
- WHEN el usuario abre el selector visual
- THEN el picker devuelve `Unsupported` con razón "linux picker catalog
  is empty for this session"

### Requirement: El selector Linux observa el probe activo después de un swap

El backend del picker SHALL consultar el probe actualmente envuelto por
`CachedActiveApplication`, no el `ActiveAppDiagnostics.backend` que el
bootstrap fija al inicio. Tras un `AppContext::swap_active_app_probe`,
la siguiente lectura de `linux_picker_backend()` SHALL reflejar el
backend del probe nuevo sin necesidad de reiniciar el proceso ni
refrescar manualmente las diagnostics.

#### Scenario: Swap de probe X11 → GNOME

- GIVEN un contexto cuyo probe inicial es `x11_ewmh`
- AND el probe se reemplaza por uno que expone `gnome_shell_extension`
  vía `swap_active_app_probe`
- AND el consentimiento GNOME es `Accepted` y el estado técnico es
  `Connected` / `Identified` / `NoActiveApplication`
- WHEN el usuario abre el selector visual
- THEN el backend resuelto es `GnomeShellExtension` y el catálogo usa
  la estrategia `desktop_file_id`

### Requirement: El selector GNOME usa estado técnico runtime vivo

El selector Linux SHALL decidir la disponibilidad de la extensión GNOME con
el snapshot técnico runtime actual, no exclusivamente con el último valor
persistido. El valor persistido sólo puede inicializar una degradación
conservadora antes de que exista un listener vivo. La sincronización runtime
MUST transportar únicamente el enum de estado; no debe transportar ni
persistir contenido del clipboard, identificadores de aplicación, títulos,
PID, rutas o assets.

#### Scenario: Listener conectado sin aplicación identificable

- GIVEN el probe activo expone `gnome_shell_extension`
- AND el consentimiento GNOME es `Accepted`
- AND el valor técnico persistido es `ActivationPending`
- AND el `SharedGnomeSnapshot` vivo informa `NoActiveApplication`
- WHEN el usuario abre el selector visual
- THEN el backend resuelto es `GnomeShellExtension`
- AND el catálogo usa `desktop_file_id`
- AND no se escribe el estado técnico en SQLite como efecto de abrir el
  catálogo

#### Scenario: Aún no existe snapshot vivo

- GIVEN el probe activo expone `gnome_shell_extension`
- AND el consentimiento GNOME es `Accepted`
- AND no existe un handle GNOME vivo
- AND el valor técnico persistido es `ActivationPending`
- WHEN el usuario abre el selector visual
- THEN el backend resuelto es `Unsupported`
- AND la UI conserva el ingreso manual

### Requirement: El comando Linux `add` rechaza identificadores arbitrarios

El comando `clipvault_ignored_app_linux_add` SHALL revalidar el
identificador recibido contra el catálogo vivo de la sesión actual
antes de persistirlo. MUST NOT aceptar identificadores arbitrarios
que el catálogo no pueda reproducir. La metadata persistida
(`display_name`, `icon_ref`) MUST provenir del catálogo, no del
frontend.

#### Scenario: Identificador presente en el catálogo

- GIVEN el catálogo de la sesión actual contiene un candidato con
  identificador `X`
- WHEN el usuario confirma la selección del candidato `X`
- THEN el comando persiste el identificador normalizado, el
  `display_name` y el `icon_ref` del catálogo
- AND la metadata del frontend es ignorada

#### Scenario: Identificador no presente en el catálogo

- GIVEN el catálogo de la sesión actual NO contiene un candidato con
  identificador `X`
- WHEN el frontend envía la orden `add` con identificador `X`
- THEN el comando rechaza con `unsupported_session`
- AND la blacklist permanece inalterada

#### Scenario: Sesión `Unsupported`

- GIVEN el backend resuelto para la sesión actual es `Unsupported`
- WHEN el frontend envía la orden `add` con cualquier identificador
- THEN el comando rechaza con `unsupported_session`
- AND la blacklist permanece inalterada

### Requirement: Cancelación del modal nunca crea filas vacías

El comando `clipvault_ignored_app_linux_add` SHALL delegar la
persistencia en `IgnoredAppsService::add_with_metadata`, que sólo
devuelve `PickAndAddOutcome::{Added, Updated}`. El brazo `Cancelled`
del mapeo del comando SHALL ser inalcanzable: si una refactorización
futura permitiera que `Cancelled` escapase, el proceso abortaría con
`unreachable!()` antes de fabricar una fila `Added` con identificador
vacío. La cancelación del modal (cierre sin selección) MUST NOT
invocar el comando, y la blacklist MUST permanecer inalterada.

#### Scenario: Cancelación del modal

- GIVEN el catálogo de la sesión actual está disponible y poblado
- WHEN el usuario cierra el modal sin seleccionar ninguna aplicación
- THEN el comando `add` nunca se invoca
- AND la blacklist permanece vacía
- AND no se crean assets nuevos

### Requirement: Modal Linux renderiza icono del catálogo

El modal del selector Linux SHALL resolver cada `icon_ref` del catálogo
a través del `iconResolver` existente y SHALL mostrar el PNG local
cuando la resolución es exitosa. Cuando el icono no se puede resolver
o el catálogo no devuelve `icon_ref`, SHALL mostrar la inicial como
fallback. MUST NOT cargar rutas arbitrarias, URLs remotas ni paths del
sistema: la validación del `iconResolver` es la única vía autorizada.

#### Scenario: Icono resuelto correctamente

- GIVEN el catálogo devuelve un candidato con `icon_ref` válido
- WHEN el modal renderiza la fila del candidato
- THEN se muestra el PNG local obtenido vía `clipvault_ignored_app_icon`
- AND el blob URL se libera al cerrar el modal

#### Scenario: Icono no disponible

- GIVEN el catálogo devuelve un candidato sin `icon_ref` o con un
  `icon_ref` rechazado por el backend
- WHEN el modal renderiza la fila del candidato
- THEN se muestra la inicial del nombre visible como fallback

### Requirement: Flujo de privacidad Linux validado en sesiones X11 y Wayland

El flujo del selector Linux SHALL conservar la misma semántica de blacklist
en X11 y Wayland: el catálogo puede mostrar aplicaciones disponibles para
agregarlas y PrivacyGate debe impedir la persistencia de nuevas capturas
originadas por una aplicación incluida en la blacklist.

#### Scenario: Prueba manual end-to-end en X11 y Wayland

- GIVEN el bundle frontend y el binario Tauri fueron generados desde el mismo
  checkout
- WHEN el usuario abre Privacidad en una sesión X11 y en una sesión Wayland
- THEN en ambas sesiones aparece la lista de aplicaciones disponibles para
  agregarlas a la blacklist
- AND una aplicación incluida en la blacklist no agrega nuevas capturas
- AND la UI conserva la semántica local y no requiere red ni un servicio
  externo
