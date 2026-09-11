# Diseño: detección de aplicaciones nativas Wayland

## Estado actual

El adapter Linux actual consulta `_NET_ACTIVE_WINDOW` y `WM_CLASS` cuando existe un display X11/XWayland. Una aplicación Wayland nativa no tiene por qué aparecer en esa jerarquía. El shell conserva una caché de la aplicación activa y el pipeline pasa el identificador al PrivacyGate y al enriquecimiento de metadata.

El provider Linux de metadata ya sabe buscar un `.desktop`, elegir el nombre localizado y persistir un icono PNG bajo `assets/application-icons/`. Este cambio debe alimentarlo con un `app_id` Wayland, no reemplazarlo.

## Arquitectura propuesta

### Adapter de plataforma

Crear un adapter Linux detrás de una feature opcional, por ejemplo `linux-wayland-active-app`, en `crates/clipvault-platform/src/runtime/`. El adapter debe:

- conectarse al socket Wayland ordinario (`$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY`);
- respetar el protocolo Wayland y los dos protocolos públicos elegidos,
  en bytes, sin procesos externos en runtime;
- mantener un estado de toplevels actualizado por dispatch asíncrono;
- asociar a cada toplevel sus propiedades `app_id`, activación y cierre;
- publicar únicamente el snapshot comprometido después del evento `done`
  del protocolo;
- implementar `ActiveApplicationProbe` sin bloquear el capture loop;
- distinguir correctamente: socket no disponible, handshake fallido,
  registry sin protocolo, versión incompatible, bind rechazado,
  protocolo conectado, compositor desconectado, snapshot sin confirmar
  y toplevel activo ausente.

El estado puede vivir en un `Arc<RwLock<...>>` o equivalente seguro. El
hilo de protocolo debe tener ownership claro de la conexión y terminar
limpiamente cuando la conexión se cierre. El método de consulta debe leer
el último snapshot comprometido y nunca devolver títulos de ventana como
identidad.

### Selección del toplevel activo

De los dos protocolos soportados, **solo
`zwlr_foreign_toplevel_management_unstable_v1` expone un estado de
activación** mediante el array `state[]` del handle. La sonda usa ese
protocolo como fuente autoritativa del foco y nunca toma el identificador
de un toplevel por orden de handle, por PID, por título o por cualquier
otra heurística.

- Se considera activo el toplevl que el protocolo `zwlr` marque como
  `state[activated] == 2`.
- Si varios toplevels están activados (condición transitoria), se
  aplica una regla determinista que selecciona el handle con el id
  más bajo, nunca el título.
- Al recibir `closed`, se elimina el toplevel y no se lo conserva como
  candidato.
- El `app_id` vacío o compuesto sólo por espacios equivale a `Ok(None)`.

`ext-foreign-toplevel-list-v1`, por contraste, **no expone un estado de
activación**. La sonda puede publicar metadata (`app_id`) del handle
ligado al protocolo ext, pero nunca puede determinar a partir de ext
cuál es la ventana enfocada. Por lo tanto:

- Cuando **ambos** protocolos están anunciados, la sonda usa el
  `activated` de zwlr para elegir el handle y toma el `app_id` del
  handle correspondiente (la sonda también puede consultar el handle
  ext por su `app_id` cuando estén enlazados ambos protocolos y
  representen el mismo toplevel — la metadata queda redundante y la
  fuente de verdad sigue siendo zwlr).
- Cuando **sólo zwlr** está anunciado, la sonda usa zwlr directamente
  para foco y app_id.
- Cuando **sólo ext** está anunciado, la sonda devuelve
  `Unavailable` con la causa `registry_without_protocol`. Inventar un
  "activo" a partir del orden de handle, del orden de creación, del
  título, del PID o de `/proc` viola el contrato del proyecto y
  queda prohibido por este diseño.

### Protocolos y fallback

1. Si el registry anuncia `zwlr_foreign_toplevel_management_unstable_v1`
   en una versión compatible, la sonda lo enlaza y lo usa como fuente
   autoritativa. `ext-foreign-toplevel-list-v1` puede enlazarse
   opcionalmente para metadata adicional.
2. Si el registry sólo anuncia `ext-foreign-toplevel-list-v1`, la sonda
   devuelve `Unavailable` (ext por sí solo no responde la pregunta de
   "qué aplicación está activa").
3. Si ninguno puede usarse, devuelve `Unavailable` con la causa
   correspondiente sin panic ni reintentos agresivos.

El implementador debe comprobar la disponibilidad real de cada
protocolo, no asumir que GNOME, KDE, wlroots o una distro específica lo
tienen habilitado. En GNOME Wayland sin `zwlr` o sin `ext`, el resultado
esperado es `Unavailable`; una integración GNOME privada requeriría
otro cambio explícito.

`ext-foreign-toplevel-list-v1` exige confirmación honesta: muchas
sesiones GNOME, incluyendo la de Ubuntu GNOME Wayland, no anuncian
extensión pública de toplevels. La sonda no debe prometer cobertura
universal ni debe recurrir a extensiones privadas de GNOME.

### Precedencia de sesiones

- Linux X11: se conserva `X11ActiveApplication` (`x11_ewmh`).
- Linux Wayland con adapter nativo operativo: el snapshot nativo es
  autoritativo. `Ok(None)` significa que no hay aplicación activa y no
  permite caer a un valor XWayland obsoleto.
- Linux Wayland sin adapter nativo operativo: puede intentarse el
  adapter XWayland existente si `$DISPLAY` está disponible.
- Linux Wayland sin XWayland: `Unavailable` / `Ok(None)` tipado, sin
  identificador falso.

La composición debe estar en una única ruta del shell. No se debe
crear una segunda caché ni un segundo watcher con estado de
deduplicación independiente.

### `WAYLAND_SOCKET`

La variable de entorno `WAYLAND_SOCKET` (activación al estilo
`systemd`) contiene un descriptor de fichero entero, no una ruta. La
sonda **no** debe transformarla en `/proc/self/fd/<fd>` ni abrir una
nueva conexión: el descriptor pertence al proceso anfitrión y cerrarlo
o reemplazarlo accidentalmente es una regresión grave.

Este cambio **elimina explícitamente** el camino `WAYLAND_SOCKET`.
La sonda sólo usa la ruta `$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY`
ordinaria. Las sesiones que necesitan activación por descriptor deben
configurar el compositor de tal manera que el proceso padre sea dueño
del descriptor; integrar la activación `systemd`-style queda fuera de
alcance de este cambio.

### Enriquecimiento y persistencia

El pipeline existente debe recibir el `app_id` antes de evaluar la
captura. El orden obligatorio es:

`refresh/read snapshot → source identifier → PrivacyGate → persistencia → metadata enrichment → history-updated`.

El `LinuxApplicationMetadataProvider` se reutiliza sin duplicar
lógica. Si encuentra el `.desktop`, completa nombre e icono y guarda el
PNG en el namespace actual `assets/application-icons/`. Si el icono no
es resoluble, el nombre y la regla de blacklist no deben perderse.

Una aplicación blacklistada debe ser rechazada antes de consultar o
crear metadata de icono, igual que en X11/XWayland.

## Dependencias y compilación

- La feature Wayland debe ser opcional y habilitarse en el shell Linux de
  producción.
- Las dependencias deben ser target-specific para Linux y no alterar el
  binario macOS.
- No agregar una dependencia que requiera una herramienta externa en
  runtime.
- Registrar en `design.md` la crate elegida, su versión compatible con
  Rust 1.89 y por qué soporta los bindings necesarios.
- El cambio debe compilar con la combinación Linux usada por el
  proyecto y conservar la posibilidad de compilar el workspace en
  macOS.

### Decisión: marshal / unmarshal hand-written, sin `wayland-client`

La capa de transporte se reduce a `std::os::unix::net::UnixStream`
y la capa de protocolo es un par de helpers `Marshal` / `Unmarshal`
hand-written que cubre exactamente los eventos que la sonda consume.

#### Por qué no `wayland-client`

`wayland-client` (versión 0.31.x o superior) sería la opción
habitual, pero introduce varios compromisos incompatibles con los
requisitos del cambio:

- La crate se compila con un build script que ejecuta el
  `wayland-scanner` de `libwayland-bin` o de la crate
  `wayland-scanner` como dependencia de build. Ninguno de los dos
  es necesario en runtime, pero arrastra una cadena de dependencias
  (incluyendo bindings para protocolos que la sonda no consume)
  que aumenta la superficie de fallo del linker cruzado hacia
  Linux.
- Los bindings generados cubren decenas de interfaces
  (`wl_compositor`, `wl_shm`, `wl_seat`, ...) que no necesitamos;
  el árbol de protocolos crece por encima de lo razonable para un
  cambio que sólo necesita dos interfaces muy concretas.
- Para soportar protocolos privados (los dos que el cambio
  requiere) la crate exige distribuir el XML del protocolo a través
  del `build.rs` o de un `include_bytes!` con un parser XML
  adicional.

#### Por qué la opción hand-written escala — y por qué necesita tests rigurosos

Los mensajes que la sonda consume son estructuralmente pequeños y
estables:

- `ext-foreign-toplevel-list-v1` admite los siguientes eventos del
  lado del compositor:
  - Lista: `toplevel` (new_id), `finished`, `enter` (object),
    `leave` (object), `update_info`.
  - Handle (`ext_foreign_toplevel_handle_v1`): `closed`, `done`,
    `title` (string), `app_id` (string), `identifier` (string).
  - **No** existe un evento `state` con foco. El evento `toplevel`
    lista lleva sólo el `new_id`; el `app_id` se publica en un
    evento aparte del handle.
- `zwlr_foreign_toplevel_management_unstable_v1` admite:
  - Manager: `toplevel` (new_id), `finished`.
  - Handle: `title`, `app_id`, `output_enter`, `output_leave`,
    `state` (array de u32, longitud **en bytes**), `done`,
    `closed`.

El `wl_array` y la cadena `length + bytes + NUL + padding` se
codifican en menos de 60 líneas, no requieren una gramática XML y
son totalmente deterministas. La suite de integración cubre los
siguientes casos con un transport de memoria:

- handshake: `wl_display` con id `1`, `get_registry` con `new_id`
  correctamente asignado al id `2`;
- registry: `global` y `global_remove` correctos;
- bind: orden exacto de argumentos (`name`, `new_id`, `interface`,
  `version`) y respeto de la versión anunciada por el compositor;
- ext: el evento `toplevel` como `new_id`; los eventos del handle
  (`closed`, `done`, `app_id`) en el orden y opcode correctos;
  comprobación de que ext **no** se usa para inferir foco;
- zwlr: el evento `toplevel` como `new_id`; los eventos del handle
  (`app_id`, `state`, `done`, `closed`); la decodificación del
  `wl_array` cuyo tamaño va **en bytes**, no en elementos; el
  estado `activated` interpretado sólo según el valor 2 definido
  por el protocolo;
- closed: elimina al toplevel antes del `done` de la ronda;
- snapshot sólo después del `done`;
- state global: varios toplevels, snaptshot uncommitted,
  compositor desconectado.

#### Compatibilidad con Rust 1.89 y macOS

El módulo entero está detrás de
`#![cfg(all(target_os = "linux", feature = "linux-wayland-active-app"))]`,
de modo que:

- Los builds de macOS no enlazan ningún símbolo Wayland. La
  ausencia de la feature en el binario del workspace se documenta
  en el `Cargo.toml` del shell (`clipvault-app`).
- La feature `linux-wayland-active-app` se activa desde el bloque
  `[target.'cfg(all(target_os = "linux", not(target_os = "macos")))'.dependencies]`
  del shell y se desactiva por defecto en cualquier otro target.

El proyecto sigue anclado en Rust estable y la nueva capa no
introduce ninguna dependencia que pueda romper ese anclaje.

### `ConnectionOutcome` granular

El constructor distingue las siguientes causas, todas expuestas por
`ActiveAppError::Unavailable` o `Backend` y serializadas como
identificadores snake_case estables para la tarjeta de diagnósticos:

- `socket_unavailable` — `$XDG_RUNTIME_DIR` / `$WAYLAND_DISPLAY`
  no resueltos o `connect(2)` falló;
- `handshake_failed` — la conexión abrió pero el handshake no
  produjo un evento `global`;
- `registry_without_protocol` — el registry no anunció zwlr (y por
  lo tanto la sonda no puede responder);
- `incompatible_version` — el compositor anuncia una versión por
  debajo de la mínima soportada;
- `bind_rejected` — el compositor rechaza el bind;
- `backend` — error de transporte o del backend ya enlazado.

`try_build()` sólo devuelve `Operational` cuando el handshake se ha
completado y la sonda ha enlazado al menos zwlr. Una sesión GNOME
Wayland sin zwlr produce `Unavailable` con causa
`registry_without_protocol`; el shell puede entonces intentar el
fallback XWayland cuando `$DISPLAY` sea utilizable.

## Diagnósticos y privacidad

El probe publica:

- un identificador de backend estable:
  - `wayland_wlr_foreign_toplevel` cuando la sonda está enlazada a
    zwlr;
  - `wayland_wlr_foreign_toplevel_with_ext` cuando la sonda está
    enlazada a ambos protocolos;
- el [`ProbeStage`] granular (`Identified`, `ActiveWindowEmpty`,
  `Unavailable`, `Backend`) que el resto de ClipVault ya consume;
- una [`ConnectionCause`] detallada (`socket_unavailable`,
  `handshake_failed`, `registry_without_protocol`,
  `incompatible_version`, `bind_rejected`, `connected`,
  `compositor_disconnected`, `snapshot_uncommitted`,
  `no_active_toplevel`, `app_id_identified`) consultable a través
  de `Snapshot::cause()`.

Los diagnósticos nunca deben incluir contenido del portapapeles,
snippets, hashes de payload, `asset_ref`, rutas absolutas, títulos de
ventana, PID ni variables de entorno.

### Modelo de confianza del `app_id`

El `app_id` Wayland es metadata publicada por la aplicación a
través de su toolkit (GTK, Qt, Electron, ...) y se trata con el
mismo nivel de confianza que `WM_CLASS` en X11. **No** constituye
una prueba de identidad del proceso: una aplicación puede mentir
sobre su `app_id`, igual que puede mentir sobre su `WM_CLASS`. La
lista de ignorados y el `LinuxApplicationMetadataProvider` lo
consumen como un identificador estable, no como una credencial.

### GNOME Wayland

GNOME / Mutter no publica, de forma estable y pública, ninguno de
los dos protocolos de toplevels extranjeros. La matriz pública de
`ext-foreign-toplevel-list-v1` muestra compatibilidades con KDE
Plasma y wlroots, no con GNOME; `zwlr` es exclusivo de
compositores wlroots.

Como consecuencia:

- En una sesión Ubuntu GNOME Wayland sin una de las dos
  extensiones públicas, la sonda devuelve
  `Unavailable` con causa `registry_without_protocol`. Esto
  **no** es un bug; es el contrato del proyecto.
- La sonda no recurre a `org.gnome.Shell.Eval`, `gdbus`,
  `wmctrl`, `xprop`, `/proc`, scraping de títulos ni procesos
  externos.
- La integración GNOME privada que necesitaría otro protocolo
  queda fuera del alcance de este cambio y requiere un OpenSpec
  específico.

## Pruebas automatizadas

El adapter debe exponerse mediante traits/fakes para probarlo sin display real. Cubrir:

- handshake y wire bytes (sender_id, opcode, new_id, orden de
  argumentos en `bind`);
- registry `global` / `global_remove`, incluyendo payload
  malformado;
- bind con global name correcto, versión anunciada y orden
  exacto de argumentos;
- versiones incompatibles (anuncio por debajo del mínimo);
- rechazo de bind;
- ext: `toplevel` como new_id; eventos del handle (`closed`,
  `done`, `app_id`); garantía de que ext nunca se usa para
  inferir foco;
- zwlr: `toplevel` como new_id; `state` con `wl_array` cuya
  longitud va en bytes; `activated == 2` como única fuente de
  foco; `done` como barrier; `closed` que elimina al handle;
- snapshot: uncommitted antes del `done`; `Identified` cuando
  se resuelve un `app_id`; `ActiveWindowEmpty` cuando ningún
  toplevel está activado;
- estado global: varios toplevels (selección determinista del
  más bajo); compositor desconectado;
- precedencia: Wayland nativo como autoritativo; sin reuso de
  identificador XWayland obsoleto; fallback XWayland cuando el
  nativo no está operativo y `$DISPLAY` es usable;
- integración con `LinuxApplicationMetadataProvider` sin
  duplicar parser;
- blacklist antes del provider, sin assets para entradas
  rechazadas;
- privacidad: ningún log con contenido, snippets, hashes,
  asset_ref, rutas, títulos, PID ni variables de entorno;
- regresiones: imágenes, tags, colecciones, favoritos, búsqueda,
  Quick Paste y drag-and-drop.

## Verificación manual Ubuntu

La verificación de runtime queda abierta para el usuario en una
sesión Ubuntu GNOME Wayland real. Debe probarse la build nueva con
aplicaciones nativas Wayland (cuando el compositor lo soporte),
aplicaciones XWayland y una sesión donde el protocolo no esté
disponible. No se puede marcar como pasada desde macOS.

## Notas sobre el cambio de versión

Esta es una corrección funcional del cambio anterior. La sonda
anterior tenía un wire protocol incorrecto (sender_id `0` para
`wl_display`, falta de `new_id` en `get_registry`, orden inverso en
`bind`, etc.), un manejo roto de `WAYLAND_SOCKET`, una
interpretación incorrecta del evento `toplevel` de
`ext-foreign-toplevel-list-v1` (leía `app_id` del evento de la
lista cuando en realidad llega en un evento aparte del handle), un
`wl_array` cuyo tamaño leía como elemento y una determinación de
foco basada en orden de handle. Esta versión reescribe la sonda
siguiendo los protocolos reales y la convierte en fuente de verdad
del foco cuando zwlr está disponible.
