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
estado. Ningún comando Tauri acepta `PairingMessage`, firma, certificado,
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
revoke/block/unblock. Sólo trusted+health aparece Activo. El modal no
autoacepta, tiene cancelar, timeout, Escape y foco restaurado.

Tests loopback/fake verifican código coincidente, key mismatch, una aprobación,
timeout, cancelación, reconnect mTLS, bloqueo/revoke y N pares aislados. La
prueba manual cubre dos pares a la vez sobre Wayland, X11 y macOS.
