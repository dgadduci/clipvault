# Diseño: vínculo mutuo seguro entre pares locales

## Dependencia y runtime

Depende de local-peer-identity-foundation y local-peer-discovery. El runtime
reemplaza el anuncio discovery_only/puerto 0 por capability pairing y un puerto
TCP efímero real mientras sharing esté activo. La dirección/puerto viven sólo
en memoria; mDNS es el único origen de ruta.

Se usa rustls y tokio-rustls detrás de PeerTransport. Certificados
autofirmados representan la identidad que ya existe en PeerIdentityStore: la
clave/certificado TLS DEBE estar criptográficamente ligado a la identidad local
persistente y mantenerse estable entre reinicios. No se acepta una clave TLS
aleatoria por arranque ni una identidad TLS independiente. El acceso al
material de firma queda detrás de un proveedor de identidad de plataforma;
bytes privados no entran a core, SQLite, Tauri ni frontend. Tras el vínculo,
el verificador TLS acepta exclusivamente la clave fijada para el peer_id. No
hay CA, cuenta ni servidor externo.

`PeerTransport::start` sólo tiene éxito tras enlazar realmente un listener TCP
efímero no cero, construir el acceptor TLS y poder procesar conexiones. El
puerto publicado por mDNS se actualiza junto a `capability = pairing`; un
placeholder, un puerto `0` o un transport que no acepta handshakes es un
fallo tipado, no una implementación productiva.

### Frontera de identidad, transporte y descubrimiento

La implementación productiva incorpora en `clipvault-platform` un proveedor
interno de identidad TLS. Sólo ese proveedor y `TlsPeerTransport` pueden leer
el seed de 32 bytes ya custodiado por el keychain. El certificado Ed25519 que
emite debe tener como SPKI la misma clave pública de `LocalPeerIdentity`, de
modo que sea estable entre reinicios y verificable contra el `peer_id` y la
huella ya publicados. Ni el seed, ni `PrivateKeyDer`, ni una configuración
rustls salen de esa capa.

También debe ser estable el DER cuyo SHA-256 se persiste como pin, no sólo su
SPKI. El proveedor debe conservar el certificado junto al material seguro o
derivar de modo determinístico todos sus campos firmados (serial, fechas y
subject) desde la identidad; no puede usar el reloj al reconstruirlo en cada
arranque. Una prueba debe construir el material del mismo seed en instantes
distintos y comprobar el mismo DER/fingerprint.

Durante el pairing inicial, la conexión mTLS puede aceptar una identidad aún
no fijada sólo para ejecutar el protocolo limitado: antes de aceptar un
mensaje, el transport verifica que el certificado presentado, la clave pública
declarada, el `peer_id` y las firmas del transcript corresponden entre sí.
Una vez persistido `trusted`, cualquier conexión exige además que el
certificado/SPKI coincida con el pin almacenado. Una CA del sistema, un
certificado aleatorio por inicio o una clave TLS separada de la identidad local
son alternativas descartadas: no prueban continuidad de identidad ni permiten
pinning fiable.

El TLS es mutuamente autenticado incluso antes de que exista un pin: ambos
extremos DEBEN presentar su certificado y probar posesión de la clave privada.
Un verificador de bootstrap puede aceptar un certificado autofirmado sólo para
continuar la sesión limitada, pero debe extraer su SPKI y comprobar después la
relación `SPKI → peer_id/fingerprint → firma del transcript`. `with_no_client_auth`
o un verificador que acepte el certificado sin esa comprobación no implementan
mTLS. Tras el vínculo, health usa el pin persistido además de esa prueba de
posesión.

Una huella es un digest unidireccional y no se puede convertir de vuelta en
una clave Ed25519. El protocol no debe interpretar
`public_key_fingerprint` como una clave: obtiene la clave remota del SPKI del
certificado autenticado, deriva desde allí `peer_id` y la huella de UI, y
verifica la firma contra esa clave. El certificado remoto —no el certificado
local— es el que se persiste como pin del peer remoto.

El puerto efímero y la ruta mDNS son detalles exclusivamente de plataforma.
El adaptador productivo compone listener TLS y registro mDNS: inicia primero el
listener, registra después `capability = pairing` con su puerto real, y al
detenerse retira el anuncio y cierra el listener. `clipvault-core`, SQLite,
Tauri y Svelte no reciben ni persisten endpoint alguno. El adaptador entrega
al runtime únicamente un evento de pairing ya autenticado (identidad pública,
transcript verificado, aprobación remota verificada y huella del certificado).
No entrega envelopes crudos y ningún evento de red atraviesa IPC.

### Frontera del adapter mDNS compartido (descubrimiento + pairing)

El `MdnsPeerDiscoveryAdapter` que el bootstrap construye es **una sola
instancia** compartida por el `PeerDiscoveryRuntime` y por el transporte de
pairing. El adapter expone tres ciclos de vida diferenciados que deben
convivir:

El nombre de instancia DNS-SD y el hostname destino del registro SRV se
derivan del `peer_id` estable (`ClipVault-<peer_id>` y
`clipvault-<peer_id>.local.`). El nombre visible es únicamente metadata TXT.
Un fullname de servicio que contiene `_clipvault._tcp` no es un hostname
válido como destino SRV: algunos resolvedores toleran ese formato, pero otros
sólo emiten `ServiceFound` y nunca `ServiceResolved`, produciendo presencia
unidireccional entre macOS y Linux. Derivar ambos nombres de la identidad
evita además que dos equipos con el mismo nombre visible colisionen en mDNS.

- `start(port=0, sink=RuntimeSink)` instala el daemon, registra
  `capability = discovery_only` con puerto placeholder y arranca el
  loop de browse que reenvía cada `ServiceResolved` / `ServiceRemoved`
  al `RuntimeSink` del runtime. El runtime persiste y actualiza la
  tabla de presencia a partir de esos eventos.
- `reconfigure(advertisement, port)` actualiza el record publicado en
  el daemon activo **sin** reiniciar el loop de browse ni reemplazar
  el sink. Es el camino que el `MdnsPairingAdvertisementSink` usa
  cuando el listener TLS ya reservó un puerto efímero real: cambia
  la capability a `pairing` y publica el puerto real sin perder los
  eventos que el browse loop sigue emitiendo al runtime.
- `stop()` cierra el daemon, retira el record y une el thread. Sólo
  lo dispara el runtime cuando el toggle queda `false`.

En el receptor, `capability` representa disponibilidad dinámica del listener,
no identidad. Por eso la persistencia acepta `discovery_only → pairing` sólo
si peer_id, huella corta, nombre y protocolo permanecen iguales; entonces
actualiza el capability y guarda la huella pública completa de 64 hex. El
camino inverso conserva la huella aprendida, pero la UI no habilita un nuevo
Vincular hasta observar nuevamente `pairing`: una huella previa no convierte
un anuncio sin puerto TLS en un endpoint dialable. Cualquier cambio de esos
campos de identidad sigue siendo conflicto y no reemplaza el registro.

El pairing transport **nunca** invoca `start_with_port` ni instala su
propio sink en el camino productivo: ese contrato provoca (a) un
`AlreadyRunning` en el camino del toggle manual porque el runtime ya
inició el adapter, y (b) en el camino del startup un
`MdnsPairingSink` que descarta todos los `DiscoveryEvent` de browse,
dejando la tabla de presencia del runtime vacía aunque el record
mDNS se publique correctamente. La asimetría que producía eso entre
macOS y Linux (una plataforma detectaba a la otra pero no al revés)
queda cubierta por los tests
`peer_discovery::mdns::tests::reconfigure_refuses_a_stopped_adapter`,
`peer_discovery::mdns::tests::reconfigure_keeps_the_running_browse_loop_alive`,
`peer_discovery::mdns::tests::stop_after_reconfigure_returns_adapter_to_idle_state`
y
`peer_transport::tls::tests::mdns_pairing_advertisement_sink_publishes_via_reconfigure`.

El orden de instalación en cada camino queda entonces así:

- **Toggle manual `false → true`:**
  `sync_runtime_with_settings(true)` llama `runtime.start()` (que
  invoca `adapter.start(port=0, RuntimeSink)`). Después
  `sync_pairing_transport_with_toggle(true)` invoca
  `advertisement.publish(real_port)`, que detecta el adapter en
  curso y llama `adapter.reconfigure(pairing, real_port)`. El
  listener TLS queda bind-eado, el browse loop sigue alimentando al
  `RuntimeSink` y el record mDNS refleja `pairing` con el puerto
  real. `runtime.is_running() == true`,
  `pairing_transport_is_running() == true`.
- **Startup con toggle persistido:** el bootstrap llama primero
  `sync_runtime_with_settings(true)` (idéntico al camino del toggle)
  y después `sync_pairing_transport_on_startup(true)`, que
  reutiliza el mismo `reconfigure`. El estado final es
  idéntico al toggle manual; `clipvault_peer_sharing_toggle_get`
  reporta `active` con el puerto real.
- **Toggle `true → false`:** `sync_runtime_with_settings(false)`
  llama `runtime.stop()` (que invoca `adapter.stop()`). Después
  `sync_pairing_transport_with_toggle(false)` llama
  `context.stop_pairing_transport()`, que cierra el listener TLS y
  en su camino de retirada invoca `advertisement.withdraw()`; el
  adapter ya está parado y el `withdraw` colapsa al no-op
  tipado. El runtime nunca delega la retirada del record al
  transporte de pairing.

**Alternativa descartada:** mantener dos adapters separados (uno
para discovery, otro para pairing) y dejar que el bridge los
coordine. Lo descartamos porque el record mDNS sólo puede
publicar un único `fullname` por instancia (`_clipvault._tcp.local`)
y un peer que ya navegó hacia el record discovery_only recibiría
un `ServiceRemoved` falso al instalar el segundo adapter,
rompiendo la presencia sin justificación. La ruta del
`reconfigure` mantiene una única fuente de verdad del daemon y
del browse loop.

El runtime no inventa un SAS ni envía una aprobación por su cuenta. Al
iniciar un vínculo, el transport resuelve internamente el peer anunciado por
mDNS y abre la sesión; al recibir una invitación, crea el estado remoto. Sólo
después de que cada UI llama a `aprobar localmente`, el transport firma y
envía la aprobación de ese lado. El transport comunica al runtime eventos
metadata-only de sesión autenticada, aprobación remota verificada, desconexión
y health; el runtime conserva el estado, decide la transición `trusted` y
llama al transport mediante un identificador de sesión opaco. Ese identificador
no representa dirección, puerto, certificado ni secreto. Así ambos modales
muestran el SAS de la misma sesión real y ninguna de las partes autoacepta.

Las dependencias aprobadas se limitan a `tokio`, `rustls`, `tokio-rustls` y
`rcgen`, todas bajo el feature de plataforma `local-peer-pairing-tls`. No se
usa `native-tls`, CA externa, HTTP ni protocolo de contenido.

## Vínculo

El endpoint pre-trust sólo entiende mensajes de pairing acotados y rate-limited.
A inicia una invitación hacia B. Ambos aportan nonce de intento y sus
identidades TLS, y calculan el mismo SAS decimal de seis dígitos sobre claves,
nonces, peer_ids y versión. A y B muestran el SAS, nombre y fingerprint
abreviado; cada usuario debe aceptar.

La sesión expira a los dos minutos. Cancelar, cerrar modal, perder conexión o
una sola aceptación revierte a unverified y elimina datos efímeros. Sólo un
mensaje de doble aceptación firmado por ambas identidades y verificado por el
transporte mTLS persiste trust_state trusted y su fingerprint. El SAS incluye
el material público TLS fijable además de ambas identidades, nonces, peer_ids
y versión. No se acepta un peer_id que cambie de clave.

La UI/Tauri sólo puede iniciar, aprobar localmente, cancelar y consultar el
estado. El snapshot metadata-only marca explícitamente si la sesión llegó por
el listener (`is_inbound`), para que el shell pueda elevar una invitación
entrante desde su coordinador global aun cuando el panel de Compartir esté
cerrado; esa marca no incluye endpoint, secreto ni habilita aprobación
automática. Al cerrar, el modal comunica el id opaco al coordinador y ambos
cancelan la sesión de forma idempotente; una respuesta IPC tardía queda
invalidada y se cancela, en vez de reabrir el diálogo o iniciar una segunda
conexión. Los ids de sesión inbound y outbound se reservan de un único contador
por transporte: ambas direcciones comparten la tabla del runtime y dos
contadores que arrancasen en `1` podrían sobrescribir una invitación al pulsar
Vincular en ambos hosts. Ningún comando Tauri acepta `PairingMessage`, firma, certificado,
huella TLS ni una supuesta aprobación remota desde el renderer: esos eventos
entran exclusivamente desde el transport autenticado al runtime.

## Estados y operaciones

known_peers soporta unverified, trusted, revoked y blocked. Presence Activo se
deriva sólo de discovery reciente más health TLS mTLS correcto. Desvincular
pasa a revoked y cierra sesiones; Bloquear pasa a blocked y rechaza pairing y
health. Desbloquear deja unverified, nunca restaura trust automáticamente.

El endpoint tras mTLS permite únicamente health con versión y presencia
metadata-only. Todas las otras rutas se rechazan como not_available; browser e
import entran en cambios posteriores.

## UI y pruebas

Equipos agrega Vincular para unverified, modal de pairing accesible y acciones
revoke/block/unblock. El coordinador global también consulta el snapshot de
sesiones inbound y abre el mismo modal para una invitación entrante; no exige
que el receptor pulse Vincular ni tenga abierto el panel de Compartir. Sólo
trusted+health aparece Activo. El modal no autoacepta, tiene cancelar, timeout,
Escape y foco restaurado.

Tests loopback/fake verifican código coincidente, key mismatch, una aprobación,
timeout, cancelación, reconnect mTLS, bloqueo/revoke y N pares aislados. La
prueba manual cubre dos pares a la vez sobre Wayland, X11 y macOS.
