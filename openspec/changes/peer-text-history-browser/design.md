# Diseño: navegación de historial textual de un par

## Dependencia y contrato

Depende de local-peer-mutual-pairing. list_recent_text sólo se ejecuta después
de mTLS y de verificar peer trusted + Activo. El host consulta su historial
local usando una proyección de core, no SQL desde el runtime de red.

Una entrada se lista si ContentType es textual, su `is_textual()` es `true`,
no tiene asset_ref, mime_type ni payload_width/height. Las referencias y
metadatos rich (`rich_text_hash`, `rich_html_ref`, `rich_rtf_ref`) no se
transportan, pero tampoco descartan el texto plano normalizado que la entrada
ya conserva: el host construye únicamente el preview escapado y acotado desde
ese texto plano. `Html` se excluye explícitamente del set transferible aunque
sea textual: el preview escapado en HTML pierde información que la card local
nunca entrega al remoto, así que el wire sólo admite texto plano.

## Camino mTLS productivo

El transporte mTLS existente (`local-peer-pairing-tls`) se extiende con una
ruta `list_recent_text` que vive en `clipvault-platform::peer_transport`:

- El cliente resuelve el peer activo mediante el `RemotePeerResolver` que el
  bootstrap ya instaló, abre una conexión mTLS efímera con el cert
  client-side que `start_with_material` cacheó, presenta el cert del peer
  authenticado contra el pin SHA-256 vigente (`arm_pin` lo armó al promover
  trust).
- El listener acepta el primer envelope; si es `PairingMessage::ListRecentText`,
  valida `version`, valida que el `peer_id` pertenece a un row
  `trusted && is_present`, y delega la proyección en un trait de core
  (`HostHistorySource`) inyectado en el bootstrap. platform no depende
  directamente de SQLite ni de Tauri.
- El host proyecta únicamente su propio historial local, devuelve
  `ListRecentTextAck` con la página bounded, o `ListRecentTextInvalid`
  cuando el cursor no se puede verificar, o `ListRecentTextUnavailable`
  con la razón tipada (`not_trusted`, `not_active`, `cursor_expired`,
  `pin_invalid`).
- Las variantes `Revoked`, `Blocked`, `UnknownPeer`, `KeyMismatch`,
  `peer_ausente`, `cursor_inválido` fallan tipadamente y nunca devuelven filas.
- El comando Tauri `clipvault_peer_history_browse` invoca el transporte
  autenticado a través de la fachada `PeerTextHistoryService` y deja de leer
  la SQLite local como fuente de los previews remotos. El servicio ya no
  invoca una proyección local cuando se solicita un peer remoto: mantiene una
  sola ruta que va por `PeerTransport::list_recent_text`.

## Cursor firmado

`RemoteHistoryCursor` sigue siendo opaco para el renderer y el cliente: deja
de ser `timestamp|id` percent-encoded y pasa a ser una firma HMAC-SHA256
sobre la concatenación canónica `peer_id || "\n" || created_at_rfc3339 ||
"\n" || entry_id` con un secreto de 32 bytes que sólo el host que la
emitió valida. El secreto se genera al promover el peer a `trusted`, se
almacena ligado al peer en `known_peers` (la migración explícita se
documenta en el cambio y se cubre con tests de migración) y se rota al
revocar o re-vincular.

El host:

1. Minta el cursor al final de cada página junto con el secreto vigente y
   firma el payload canónico. La representación wire es
   `<base64url(payload)>::<base64url(hmac)>`.
2. Persiste el secreto cuando promueve el peer; lo rota cuando el peer es
   `Revoked` y se vuelve a generar al re-vincular.
3. Verifica toda firma con el secreto actual antes de proyectar; un cursor
   forjado, con timestamp manipulado o con un secreto rotado retorna
   `ListRecentTextInvalid`. El wire nunca expone el secreto ni el hash
   crudo.

### Backfill de secretos para pares `trusted` heredados

La migración que añadió la columna `known_peers.cursor_secret` la
inicializó como cadena vacía. Un par vinculado antes del despliegue de
`peer-text-history-browser` puede seguir marcado `trusted` sin un
secreto persistido: sin un backfill el host no podría firmar ni validar
cursores para ese par y el dial remoto colapsaría a la razón
tipada `not_trusted` incluso cuando el par está realmente activo.

El bootstrap resuelve el caso durante `AppContext::bootstrap` /
`bootstrap_at`:

- Para cada fila de `known_peers` con `trust_state = Trusted`:
  - Si `cursor_secret` es un hex válido de 64 caracteres, se instala en
    `PeerTextHistoryService` con `install_cursor_secret_hex`.
  - Si está vacío o tiene longitud / formato inválido, se genera un
    nuevo `PeerCursorSecret` CSPRNG de 32 bytes, se persiste con
    `KnownPeerRepository::set_cursor_secret` y, **sólo después de que
    la escritura sea exitosa**, se instala en la caché en memoria del
    servicio.
- Filas con `trust_state != Trusted` no se tocan: el runtime sólo firma
  cursores para pares `trusted`, así que un secreto huérfano en una
  fila `unverified` / `revoked` / `blocked` seguiría siendo invisible y
  ruidoso.
- Un secreto con formato inválido se rota, no se acepta: dejar un valor
  corrupto en la columna reventaría el HMAC en el primer cursor que el
  par solicitara.
- Un fallo de lectura o de escritura de la base de datos deja el
  bootstrap vivo: el log lleva un mensaje fijo sin `peer_id`,
  `cursor_secret`, hostname, IP, puerto, certificado ni contenido, y el
  resto del proceso continúa.

El comportamiento se cubre con un test representativo sobre una base
temporal con tres filas (`trusted` sin secreto, `trusted` con secreto
válido, `unverified` sin secreto) y tres asserts: persistencia +
instalación, preservación y no-mutación.

## Ciclo de vida del handler de historial

`PeerTextHistoryHostHandlerAdapter` se instala durante `AppContext` para
servir `list_recent_text` desde el primer envelope entrante, pero el
toggle / startup del listener llama a
`PairingRuntime::install_pairing_transport_with_resolver`, que delega en
`start_with_material_and_resolver`. Esa ruta cae en
`install_with_material_and_resolver`, que termina llamando a
`install_with_material_resolver_and_history(..., None)`: el `None`
sobreescribe `state.history_handler = None` y el listener queda mudo.

La corrección pasa la dependencia explícitamente:

- `PairingRuntime` conserva el `HistoryHostHandler` instalado en un slot
  dedicado (`history_handler`) para que un reinicio del listener lo
  reinyecte sin perder la productividad.
- `install_history_handler_inner` registra el handler en ese slot y, si
  el listener está corriendo, sigue llamándolo en el transporte
  subyacente para mantener compatibilidad con tests y rutas heredadas.
- `install_pairing_transport_with_resolver` ahora invoca
  `start_with_material_resolver_and_history` pasando el handler
  registrado (o `None` cuando aún no se instaló).

El handler debe sobrevivir a: arranque normal con compartir activo,
activar compartir, desactivar y volver a activar compartir, y reinicio
del listener. El core ya no depende de que TLS "recuerde" implícitamente
un handler anterior.

Como defensa adicional, `install_with_material_and_resolver` y
`install_with_material` preservan un handler existente cuando una ruta
heredada recibe `None`, pero la corrección principal es que la ruta
productiva ya no envía `None`.

La plataforma se mantiene libre de SQLite y Tauri; el frontend se
mantiene libre de lógica de negocio: el cambio vive en core + platform.

### Restauración de pins mTLS tras reinicio

El fingerprint SHA-256 del certificado de un par `trusted` ya se
persiste en `known_peers.tls_cert_fingerprint`, pero los mapas que el
transporte TLS usa para verificar el certificado son deliberadamente
volátiles. Al reiniciar ClipVault, dejar esos mapas vacíos convertiría
un par válido en `UnknownPeer` antes de abrir la conexión de historial.

Durante el bootstrap y antes de iniciar el listener, el core recorre
únicamente filas `Trusted` con fingerprint SHA-256 canónico (64 hex en
minúsculas) y rearma el pin a través de `PairingRuntime`. Filas no
trusted, vacías o malformadas se omiten. Un fallo al listar o armar un
pin no impide iniciar ClipVault ni revela identificadores, fingerprints,
endpoints, SQL o contenido en logs; el par afectado permanece con el
outcome tipado `not_trusted` hasta volver a vincularse.

Una regresión con un transporte TLS real pero sin red verifica que un
pin trusted restaurado pasa `health_check`, mientras que filas
unverified o con fingerprint malformado no entran al verifier map.

### Outcomes tipados: `not_active` vs `not_trusted`

El runtime debe distinguir con precisión por qué un par remoto rechaza
una página:

- `not_active` — el par local está apagado / sin presencia: el runtime
  no abre la red y el cliente renderiza el estado "No disponible".
- `not_trusted` — el host rechaza la página porque el par no es
  `trusted` o porque el secreto HMAC no está instalado. El wire surface
  es `PeerHistoryOutcome::PeerUnavailable { reason: "not_trusted" }`,
  nunca `not_active`.

La rama del cliente (`browse`) se ajusta para mapear
`TransportError::Revoked` y `TransportError::Blocked` a la razón
`not_trusted` (un par revocado o bloqueado no es `active`), mientras
que un par con `state.active == false` mantiene `not_active`. El
servicio (`serve`) sigue devolviendo
`HostHistoryResponse::Unavailable("not_trusted")` cuando no hay secreto;
la traducción al lado del cliente permanece estable. La distinción
queda cubierta por tests dedicados.

### Endpoint mDNS de pairing tras arranque o reconfiguración

La presencia y la ruta de dial son datos distintos: un `Observed` puede llegar
para el anuncio inicial `discovery_only`, cuyo puerto es el centinela `0` y no
representa un listener TLS. El adaptador nunca guarda ni resuelve un endpoint
con puerto `0`; al pasar a `capability = pairing` retira el registro anterior y
registra el SRV/TXT con el puerto TLS real, aun cuando el fullname no cambie.
Así los browsers remotos reciben una resolución nueva y no conservan una ruta
obsoleta.

El dial de historial conserva un reintento corto y acotado al resolver o a un
fallo de conexión transitorio: vuelve a consultar el `RemotePeerResolver` antes
de cada intento y sólo reintenta `unavailable`. No reintenta un rechazo de pin,
trust, cursor o protocolo. Si tras la ventana no existe endpoint, el outcome
es `transport_unavailable: unavailable`, nunca `not_trusted`; `UnknownPeer`
queda reservado para un pin/identidad desconocidos.

## Snapshot id

El `snapshot_id` deja de ser `created_at|id`. Pasa a ser un fingerprint
SHA-256 del header de la página transferible: `peer_id || "\n" ||
max_created_at || "\n" || max_id || "\n" || count`. El cliente puede
detectar un cambio de historial entre requests sin recibir reversible metadata.

## Paginación y preview

El host ordena por created_at descendente y un tie-breaker estable de id.
Devuelve como máximo 50 filas y un cursor firmado por el host; el cliente no
puede inventar un cursor, offset ni firma. Un cursor inválido retorna typed
`invalid_cursor`.

Cada fila lleva remote_entry_id opaco para la sesión/protocolo, título opcional
validado, content_type, created_at y preview escapado. El preview se construye
en el host, corta a dos líneas o 300 caracteres Unicode y no interpola HTML.
No hay fetch_text en esta etapa, por lo que la red no puede devolver contenido
completo.

## UI

El desktop principal muestra una sección `Equipos vinculados` **dentro** del
sidebar `OrganizationSidebar`, debajo de la lista scrolleable de
colecciones. El sidebar conserva sus dos columnas implícitas en vertical:

- lista de colecciones (scroller vertical propio);
- lista de equipos vinculados (scroller vertical independiente).

El panel principal vuelve a tener exactamente el orden previo:

1. `DesktopToolbar` horizontal completo (búsqueda, selectores de source-app y
   tag, menú de configuración, botón borrar en una sola fila).
2. `HistoryCardRail` horizontal scrolleable, o `RemoteHistoryRail` cuando
   haya un peer seleccionado. Cuando se selecciona una colección local o
   `Historial`, `activePeerId` se limpia y la rail local se restaura sin
   esperar a otra interacción.

Incluye cada peer cuyo trust persistido sea `trusted`, incluso si está no
disponible. Cada ítem muestra nombre visible y un círculo pequeño: verde
sólo para `trusted && is_present`, gris para trusted no disponible, ausente
para el resto (untrusted / discovery_only). La lista se refresca al
aterrizar un vínculo, al recibir cambios de presencia/health y al cargar el
snapshot; un vínculo nuevo aparece sin recargar la aplicación. El refresh
post-pairing dispara `peerSnapshotCommand` explícitamente al cerrarse el
`PeerPairingModal`, sin depender de la siguiente selección del peer.

El shell (`App.svelte`) es el único propietario del snapshot compartido por
la lista y la `RemoteHistoryRail`. La implementación centraliza el refresh
detrás de un helper con guard single-flight (mismo patrón que
`PeerSharingModal.refreshSnapshot`) para que la actualización inicial, la
post-pairing y cualquier otra ruta coaleszan en un único round-trip cuando
coincidan en el tiempo. El shell además ejecuta un polling de metadatos
con cadencia corta y local (2 s, constante del shell, consistente con la del
modal de Compartir) iniciado en `onMount` y detenido en `onDestroy`. La
lista y la rail consumen el snapshot reactivo; no abren
`peerSnapshotCommand` por su cuenta. Una falla del bridge conserva el
snapshot anterior y no interrumpe el desktop ni muestra un error intrusivo.
No se modifica `MdnsPeerDiscoveryAdapter`, `PeerDiscoveryRuntime`, el pareado
ni mTLS: la transición tras un cierre abrupto está anclada en el cambio
archivado `local-peer-presence-liveness`. El adaptador publica `Removed`
(goodbye, expiración de caché o verificación DNS-SD acotada) como
transición autoritativa; `PRESENCE_TTL = 120 s` ya no decide la
disponibilidad.

La salida normal del shell (`⌘Q`, *tray Salir*, `Ctrl-C`) llama al
helper `stop_network_subsystems(&AppContext)` antes de la pasada
de retention: el orden es primero `stop_pairing_transport()` y
después `PeerDiscoveryRuntime::stop()`. El pairing comparte el
`MdnsPeerDiscoveryAdapter` con la runtime de discovery a través
del `MdnsPairingAdvertisementSink`; detenerlo primero retira el
registro `pairing` antes de que el adapter publique el `goodbye`
final, así un peer remoto observa `ServiceRemoved` en ≤ 5 s y
el desktop refleja `No disponible` gracias al evento autoritativo
`Removed` que el cambio archivado `local-peer-presence-liveness`
introdujo; el adapter mDNS es la única fuente de transición, no
se reintroduce el `PRESENCE_TTL = 120 s`. Ambas paradas son
best-effort: un fallo en una de las dos sólo registra un `warn!`
sin IP, puerto, `peer_id` ni contenido, y nunca impide la salida.
La regresión del orden vive en `app/tauri/src-tauri/src/bootstrap.rs::tests`
con un adapter de discovery y un transporte de pairing
instrumentados, y un segundo test estático verifica que
`main.rs::cleanup` invoca el helper antes de `run_retention`. Una
caída abrupta, una suspensión, Wi-Fi apagado o `kill -9` siguen
dependiendo del goodbye/expiración que publica el adapter, no de
un TTL — el helper solo cambia el cierre normal.

Seleccionar un peer activo reemplaza en el mismo panel principal la lista de
historial o colección que estaba visible. No existe una ruta ni página
`RemoteHistory` independiente. No hay un botón `Volver` como navegación
primaria; volver ocurre al seleccionar `Historial` o una colección local.
Seleccionar un peer no disponible no hace una solicitud de red y muestra el
estado no disponible en el mismo panel.

`RemotePreviewCard` comparte únicamente el esqueleto visual, densidad y
jerarquía tipográfica de las cards locales; no reutiliza `HistoryCard`, su
`data-testid`, `data-entry-id` ni el controlador singleton de drag/drop. Lleva
`draggable="false"`, no inicia listeners de arrastre y no expone pin, editar,
copy/paste ni acciones locales. Conserva un botón de menú remoto que abre sólo
`Importar (próximamente)` deshabilitado: comunica el flujo futuro sin obtener
texto completo ni mutar SQLite antes del cambio `peer-text-import`.

La tira expone Anterior/Siguiente, loading y error en el mismo panel; cada
peer mantiene su cursor/lista independiente. Al cambiar de peer, una respuesta
tardía no puede sobrescribir el panel actual. Un fallo de red conserva la
selección de colección o peer y no marca al peer como bloqueado automáticamente.

Cuando el panel principal muestra historial remoto, los filtros locales
(búsqueda, source-app, tag) no se aplican al rail remoto: el toolbar
correspondiente se desactiva o se oculta, según el componente decida, para
que el usuario no crea que esos filtros están filtrando los previews
remotos.

Las fechas de la card remota se formatean igual que la card local: el bridge
convierte el `created_at` RFC 3339 en el formato que `HistoryCard` ya usa
para que ambas cards compartan la misma tipografía.

## Verificación

Tests core cubren elegibilidad, orden, cursor firmado (firma válida,
firma inválida, secreto rotado, timestamp manipulado), límites, preview
escapado, inclusión del preview plano de una entrada textual rich, exclusión
de `Html`/image, snapshot id no reversible y metadata permitida.

Tests de transporte cubren trust, pin, invalid cursor y reemplazo del
stub por la ruta mTLS productiva.

Tests e2e con dos listeners TLS reales:

- A contiene una captura exclusiva "solo-A"; B contiene "solo-B".
- A navega B y recibe sólo preview de B, nunca "solo-A".
- La navegación no crea ni modifica entradas locales en A.
- Rechazo para `Revoked`, `Blocked`, no `Trusted`, pin inválido y cursor
  fabricado.

Frontend cubre layout (dos columnas, scrollers independientes, toolbar
horizontal, rail local restaurada), lista reactiva de peers, puntos de
estado, reemplazo del panel principal, cards read-only, menú Importar
deshabilitado, páginas, stale response, ausencia de mutación local y
regresión de `pointerDragAndDrop` porque se toca layout, sidebar y cards.

Pruebas manuales en los tres sistemas verifican previews y navegación, no
importación.
