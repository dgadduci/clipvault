# Diseño: navegación de historial textual de un par

## Dependencia y contrato

Depende de local-peer-mutual-pairing. list_recent_text sólo se ejecuta después
de mTLS y de verificar peer trusted + Activo. El host consulta su historial
local usando una proyección de core, no SQL desde el runtime de red.

Una entrada se lista si ContentType es textual, su `is_textual()` es `true`,
no tiene asset_ref, mime_type, payload_width/height ni rich-text references.
`Html` se excluye explícitamente del set transferible aunque sea textual: el
preview escapado en HTML pierde información que la card local nunca entrega al
remoto, así que el wire sólo admite texto plano.

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
escapado, exclusión de `Html`/image/rich-text, snapshot id no reversible y
metadata permitida.

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