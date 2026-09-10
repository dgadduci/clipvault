# Diseño: detección de aplicaciones nativas Wayland

## Estado actual

El adapter Linux actual consulta `_NET_ACTIVE_WINDOW` y `WM_CLASS` cuando existe un display X11/XWayland. Una aplicación Wayland nativa no tiene por qué aparecer en esa jerarquía. El shell conserva una caché de la aplicación activa y el pipeline pasa el identificador al PrivacyGate y al enriquecimiento de metadata.

El provider Linux de metadata ya sabe buscar un `.desktop`, elegir el nombre localizado y persistir un icono PNG bajo `assets/application-icons/`. Este cambio debe alimentarlo con un `app_id` Wayland, no reemplazarlo.

## Arquitectura propuesta

### Adapter de plataforma

Crear un adapter Linux detrás de una feature opcional, por ejemplo `linux-wayland-active-app`, en `crates/clipvault-platform/src/runtime/`. El adapter debe:

- conectarse al socket Wayland de la sesión usando una API Rust mantenida y compatible con Rust 1.89;
- enlazar bindings generados o incorporados para los protocolos públicos elegidos, sin depender de ejecutar `wayland-scanner` en runtime ni de comandos del sistema;
- mantener un estado de toplevels actualizado por dispatch asíncrono;
- asociar a cada toplevel sus propiedades `app_id`, activación y cierre;
- publicar únicamente el snapshot comprometido después del evento `done` del protocolo;
- implementar `ActiveApplicationProbe` sin bloquear el capture loop.

El estado puede vivir en un `Arc<RwLock<...>>` o equivalente seguro. El hilo de protocolo debe tener ownership claro de la conexión y terminar limpiamente cuando la conexión se cierre. El método de consulta debe leer el último snapshot comprometido y nunca devolver títulos de ventana como identidad.

### Selección del toplevel activo

- Se considera activo el toplevel que el protocolo marque como activo/activado.
- Si no hay uno activo, devolver `Ok(None)`.
- Si hay más de uno por una condición transitoria, aplicar una regla determinista documentada basada en el orden/generación de eventos del adapter, nunca en el título.
- Al recibir `closed`, eliminar el toplevel y no conservarlo como candidato.
- El `app_id` vacío o compuesto sólo por espacios equivale a `Ok(None)`.

### Protocolos y fallback

1. Intentar `ext-foreign-toplevel-list-v1` cuando el registry lo anuncie y la versión sea compatible.
2. Si no está disponible, intentar `zwlr_foreign_toplevel_management_unstable_v1` cuando corresponda.
3. Si ninguno puede usarse, devolver un error tipado `Unavailable` con backend estable, sin panic ni reintentos agresivos.

El implementador debe comprobar la disponibilidad real de cada protocolo, no asumir que GNOME, KDE, wlroots o una distro específica lo tienen habilitado. En GNOME Wayland sin uno de estos protocolos, el resultado esperado es `Unavailable`; una integración GNOME privada requeriría otro cambio explícito.

### Precedencia de sesiones

- Linux X11: se conserva `X11ActiveApplication`.
- Linux Wayland con adapter nativo operativo: el snapshot nativo es autoritativo. `Ok(None)` significa que no hay aplicación activa y no permite caer a un valor XWayland obsoleto.
- Linux Wayland sin adapter nativo operativo: puede intentarse el adapter XWayland existente si `$DISPLAY` está disponible.
- Wayland nativo sin protocolo y sin XWayland: `Unavailable`/`Ok(None)` tipado, sin identificador falso.

La composición debe estar en una única ruta del shell. No se debe crear una segunda caché ni un segundo watcher con estado de deduplicación independiente.

### Enriquecimiento y persistencia

El pipeline existente debe recibir el `app_id` antes de evaluar la captura. El orden obligatorio es:

`refresh/read snapshot → source identifier → PrivacyGate → persistencia → metadata enrichment → history-updated`.

El `LinuxApplicationMetadataProvider` se reutiliza sin duplicar lógica. Si encuentra el `.desktop`, completa nombre e icono y guarda el PNG en el namespace actual `assets/application-icons/`. Si el icono no es resoluble, el nombre y la regla de blacklist no deben perderse.

Una aplicación blacklistada debe ser rechazada antes de consultar o crear metadata de icono, igual que en X11/XWayland.

## Dependencias y compilación

- La feature Wayland debe ser opcional y habilitarse en el shell Linux de producción.
- Las dependencias deben ser target-specific para Linux y no alterar el binario macOS.
- No agregar una dependencia que requiera una herramienta externa en runtime.
- Registrar en `design.md` la crate elegida, su versión compatible con Rust 1.89 y por qué soporta los bindings necesarios.
- El cambio debe compilar con la combinación Linux usada por el proyecto y conservar la posibilidad de compilar el workspace en macOS.

### Decisión: bindings hand-written, sin `wayland-client`

El adapter Wayland se implementa sin agregar nuevas crates de
Wayland al grafo de dependencias. La capa de transporte se reduce
a `std::os::unix::net::UnixStream` y la capa de protocolo es un
par `Marshal` / `Unmarshal` hand-written que cubre exactamente los
eventos que el probe consume (`toplevel`, `app_id`, `state`,
`closed`, `done`).

#### Por qué no `wayland-client`

`wayland-client` (versión 0.31.x o superior) sería la opción
habitual, pero introduce varios compromisos incompatibles con los
requisitos del cambio:

- La crate se compila con un build script que ejecuta el
  `wayland-scanner` de `libwayland-bin` o de la crate
  `wayland-scanner` como dependencia de build. Ninguno de los dos
  es necesario en runtime, pero arrastra una cadena de
  dependencias (incluyendo bindings para protocolos que el probe no
  consume) que aumenta la superficie de fallo del linker cruzado
  hacia Linux.
- Los bindings generados cubren decenas de interfaces
  (`wl_compositor`, `wl_shm`, `wl_seat`, ...) que no necesitamos;
  el árbol de protocolos crece por encima de lo razonable para un
  cambio que solo necesita dos interfaces muy concretas.
- Para soportar protocolos privados (los dos que el cambio
  requiere) la crate exige distribuir el XML del protocolo a través
  del `build.rs` o de un `include_bytes!` con un parser XML
  adicional. La cantidad de código que termina dependiendo de un
  parser XML excede el tamaño del marshal hand-written que
  necesitamos.

#### Por qué la opción hand-written escala

Los mensajes que el probe consume son estructuralmente pequeños y
estables:

- `ext-foreign-toplevel-list-v1`: 10 eventos; el probe solo
  necesita 5 (`toplevel`, `app_id`, `state`, `closed`, `done`).
- `zwlr_foreign_toplevel_management_unstable_v1`: 6 eventos; el
  probe necesita 4 (`toplevel`, `app_id`, `state`, `closed`,
  `done`).

El `wl_array` y la cadena `length + bytes + NUL + padding` se
codifican en menos de 40 líneas, no requieren una gramática XML y
son totalmente deterministas. La suite de integración ejercita
todos los caminos (handshake, registry `global`, bind,
`done`/`closed`, eventos `app_id` y `state`) con un transport de
memoria; la lógica de producción usa exactamente el mismo código,
solo cambiando `MemoryTransport` por `UnixStreamTransport`.

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

## Diagnósticos y privacidad

Agregar un backend estable para la sonda nativa Wayland, por ejemplo `wayland_foreign_toplevel`, y etapas de diagnóstico suficientes para distinguir `identified`, `no_active_toplevel`, `protocol_unavailable`, `connection_unavailable` y `backend`.

Los diagnósticos pueden transportar el identificador estable necesario para depuración, siguiendo el contrato existente, pero nunca deben incluir contenido del portapapeles, snippets, hashes de payload, `asset_ref`, rutas absolutas, títulos de ventana, PID ni variables de entorno.

### Modelo de confianza del `app_id`

El `app_id` Wayland es metadata publicada por la aplicación a
través de su toolkit (GTK, Qt, Electron, ...) y se trata con el
mismo nivel de confianza que `WM_CLASS` en X11. **No** constituye
una prueba de identidad del proceso: una aplicación puede mentir
sobre su `app_id`, igual que puede mentir sobre su `WM_CLASS`. El
blacklist y el `LinuxApplicationMetadataProvider` lo consumen como
un identificador estable, no como una credencial.

### Backends y etapas de diagnóstico

El nuevo backend expone dos identificadores estables consumidos por
el endpoint de diagnósticos y por el frontend:

- `wayland_foreign_toplevel` cuando el probe se enlaza a
  `ext-foreign-toplevel-list-v1`.
- `wayland_wlr_foreign_toplevel` cuando solo está disponible el
  fallback `zwlr_foreign_toplevel_management_unstable_v1`.

Las etapas granulares que el `ProbeStage` puede mostrar son:

- `Started` (estado inicial, antes del primer handshake).
- `Identified` cuando un round `done` resuelve un `app_id` no vacío.
- `ActiveWindowEmpty` cuando un round `done` no resuelve ningún
  toplevel activo (mapea a `Ok(None)` sin error).
- `Unavailable` cuando no hay protocolo enlazado o la conexión
  cerró antes del primer `done` (mapea a `ActiveAppError::Unavailable`).
- `Backend` cuando el I/O thread termina con un error de
  transporte (mapea a `ActiveAppError::Backend`).

Estos cinco valores son los únicos que el probe Wayland puede
producir: cualquier otra rama del FSM queda dentro del enum
`ProbeStage` existente y el adapter macOS / X11 conserva su
semántica intacta.

## Pruebas automatizadas

El adapter debe exponerse mediante traits/fakes para probarlo sin display real. Cubrir:

- registro con protocolo compatible;
- recepción de `app_id` y publicación sólo después de `done`;
- selección determinista del toplevel activo;
- eliminación al recibir `closed`;
- `Ok(None)` sin activo;
- app_id vacío o inválido;
- protocolo ausente, versión incompatible y desconexión como `Unavailable`;
- precedencia nativa sobre snapshot XWayland obsoleto;
- fallback XWayland sólo cuando el adapter nativo no está operativo;
- reutilización del provider Linux para nombre e icono;
- blacklist antes de metadata y ausencia de assets para entradas rechazadas;
- no filtración de campos sensibles;
- regresiones de imágenes, tags, colecciones, favoritos, búsqueda, Quick Paste y drag-and-drop.

## Verificación manual Ubuntu

La verificación de runtime queda abierta para el usuario en una sesión Ubuntu GNOME Wayland real. Debe probarse la build nueva con aplicaciones nativas Wayland, aplicaciones XWayland y una sesión donde el protocolo no esté disponible. No se puede marcar como pasada desde macOS.
