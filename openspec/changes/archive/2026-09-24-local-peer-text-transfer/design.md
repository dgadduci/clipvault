# Diseño: transferencia manual de capturas de texto entre pares locales

## 1. Límite de producto y arquitectura

Esta capacidad es una excepción explícita y opt-in a la política general de no
realizar llamadas de red: sólo abre tráfico local cuando la persona activa
Compartir en red local. No hay destino de Internet, cuenta, relay ni
sincronización. La red sólo permite importar una captura concreta desde un
equipo propio o de confianza.

El sistema admite N instalaciones: cada vínculo es siempre una sesión entre
dos pares, pero una identidad puede conservar simultáneamente muchos vínculos
independientes. El estado, la revocación, el bloqueo, la colección vinculada y
la procedencia de un par no afectan a ningún otro.

    Settings / vista Equipos -> Tauri commands/events -> PeerSharingService
                                                            |
                  +-----------------------------------------+------------------+
                  |                                         |                  |
                  v                                         v                  v
           PeerRepository                    TextHistory / Organization  PeerNetworkRuntime
           (SQLite)                           (core, transacción)          (mDNS + TLS local)
                  |                                                             |
                  +-------------------- identidad pública --------------------+
                                                            |
                                              LAN: DNS-SD/mDNS + TCP/TLS

clipvault-db contiene las migraciones y repositorios. clipvault-core contiene
modelos de confianza, validación de datos remotos, proyección de historial
transferible e importación transaccional. Una nueva crate Rust,
provisionalmente clipvault-network, encapsula DNS-SD/mDNS, listener TCP/TLS,
protocolo y timeouts. Puede depender de tipos públicos del core, pero el core
no depende de Tauri, navegador ni red real.

El shell Tauri mantiene el ciclo de vida del runtime y traduce comandos/eventos
metadata-only. El frontend no conoce SQLite, certificados, direcciones IP ni
contenido completo antes de que la persona solicite importarlo.

## 2. Identidad, configuración y consentimiento

Al habilitar compartir por primera vez, se genera una identidad Ed25519. El
peer_id se deriva de su clave pública y no cambia al renombrar el equipo ni al
renovar una dirección IP. El nombre visible se recorta, rechaza vacío o
caracteres de control y queda limitado a 64 caracteres Unicode. Es metadata
pública dentro de la LAN, no una credencial.

La clave privada y el certificado TLS asociado se guardan detrás de un trait
PeerIdentityStore: Keychain de macOS y Secret Service del escritorio Linux en
producción; store en memoria en tests. No se guarda material privado en SQLite,
archivos de configuración, fixtures, logs ni respuestas Tauri. Si el almacén
seguro no está disponible, el toggle no queda habilitado y la UI recibe un
error tipado; no habrá fallback de archivo en texto claro.

app_settings guarda sólo las preferencias no secretas:

- local_peer_sharing_enabled, por defecto false;
- local_peer_display_name.

Al habilitar el toggle, el runtime anuncia y busca pares. En próximos arranques
se vuelve a iniciar si sigue habilitado. Al desactivarlo retira el anuncio,
detiene listener y conexiones, y deja el registro histórico intacto. En macOS
el bundle declara NSLocalNetworkUsageDescription y _clipvault._tcp en
NSBonjourServices antes de activar la capacidad, por lo que el permiso aparece
sólo en el contexto de la acción del usuario.

## 3. Descubrimiento, presencia y compatibilidad

Se usa DNS-SD sobre mDNS con tipo de servicio _clipvault._tcp.local. El browser
es persistente mientras sharing esté activo: eventos de resolución y remoción
actualizan una caché en memoria, sin ciclos de escaneo de puertos ni broadcast
propio. Direcciones y puertos son efímeros: no se persisten como identidad.

El TXT record sólo contiene versión mayor de protocolo, peer_id, clave pública
o fingerprint necesarios para verificar el handshake, puerto y nombre visible
validado. Nunca contiene texto capturado, previews, conteos de historial, tags,
colecciones, aplicaciones de origen, hashes, rutas, secretos ni tokens.

La UI persiste observaciones y deriva estos estados:

- **No verificado**: descubierto, pero no vinculado; sólo permite vínculo.
- **Activo**: vinculado, visto dentro del TTL y con health check TLS
  autenticado reciente.
- **No disponible**: conocido pero no visible dentro del TTL o sin health
  check válido; no permite navegar hasta reactivarse.
- **Incompatible**: versión mayor no compatible; no permite vínculo ni acceso.

La primera entrega funciona sólo en el mismo enlace o segmento donde mDNS es
reenviado. Wi-Fi de invitados, VLAN o routers que bloqueen multicast pueden no
mostrar pares. La aplicación comunica esa limitación y no suplanta mDNS con
escaneo, broadcast propio ni entrada manual de IP.

## 4. Vínculo mutuo y autenticación posterior

El transporte es TCP local con TLS mutuo y certificados de identidad
autofirmados. Fuera del emparejamiento, cada lado acepta sólo una clave pública
que ya está fijada en known_peers; una identidad cambiada se rechaza antes de
servir historial.

Flujo inicial acotado y rate-limited:

1. A descubre B y elige Vincular.
2. A y B intercambian identidades efímeras del intento sobre TLS y derivan un
   código decimal de seis dígitos de claves, nonces y versión de protocolo.
3. Ambos muestran ese código, nombre y fingerprint abreviado. Cada persona
   confirma que coincide; una aceptación unilateral o timeout de dos minutos
   no guarda confianza.
4. Al llegar ambas aprobaciones, cada registro local fija el peer_id y la
   clave pública del otro como trusted.

En vínculos posteriores no hay código ni confirmación humana: mTLS con pinning
autoriza silenciosamente. Desvincular cambia a revoked; Bloquear también impide
nuevo vínculo y conexión hasta que se desbloquee. Ninguna acción borra snapshots
importados, colecciones ni procedencia local.

## 5. Protocolo local v1 y límites

El runtime expone un protocolo pequeño, versionado y autenticado. Puede usar
framing HTTP local sobre TCP/TLS, pero sus DTOs residen en Rust y se validan sin
webview. Todas las solicitudes llevan versión mayor 1, timeout, límite de
tamaño y outcome discriminado; ningún error incluye contenido, hashes, rutas,
secretos ni respuesta TLS subyacente.

Después de mTLS exitoso, el host permite exclusivamente:

- health: presencia y compatibilidad, sin contenido;
- list_recent_text(cursor, limit): página descendente transferible;
- fetch_text(remote_entry_id): texto de una entrada tras Importar.

Una entrada es transferible si es textual y no posee asset de imagen, MIME de
asset ni campos rich-text. La lista devuelve hasta 50 filas, cursor opaco del
host, título opcional, tipo, fecha y preview escapado de hasta 300 caracteres o
dos líneas. No devuelve texto completo, tags, colecciones, favoritos, source
app ni hashes. fetch_text admite como máximo 1 MiB UTF-8 y revalida
elegibilidad antes de responder.

El listener limita requests y vínculos pendientes por conexión y par, aplica
timeouts a handshake, health, página e importación, y cierra frames malformados
o sobredimensionados. Sólo Importar pide el texto completo; presencia no inicia
una transferencia de contenido.

## 6. Persistencia e importación

La próxima migración libre, actualmente MIGRATION_0013_LOCAL_PEERS, será
aditiva y reversible:

    known_peers(
      peer_id PK, public_key_fingerprint, display_name, trust_state,
      protocol_major, first_seen_at, last_discovered_at, last_authenticated_at,
      blocked_at?, updated_at
    )
    peer_collection_bindings(
      peer_id PK -> known_peers, collection_id UNIQUE -> collections,
      created_at, updated_at
    )
    remote_imports(
      peer_id -> known_peers, remote_entry_id, imported_content_hash,
      local_entry_id -> entries, imported_at,
      PRIMARY KEY(peer_id, remote_entry_id, imported_content_hash)
    )

FKs están activas. Al eliminar una colección, su binding hace cascade para que
un próximo import cree otro; nunca hay cascade de entradas o assets. No se
persisten IP, puerto, texto remoto separado ni claves privadas. La migración
down elimina sólo estas tablas en orden inverso.

PeerImportService valida DTO, tamaño y título, y recalcula tipo y hash con los
helpers canónicos locales. En una única transacción:

1. Busca contenido local por hash.
2. Crea entrada normal con hora local de importación si falta, sin falsificar
   source_app, y usa título remoto sólo si pasa validación local.
3. Reutiliza una entrada idéntica sin sobrescribir título, timestamps,
   favoritos, tags, colecciones ni assets.
4. Obtiene o crea la colección ligada por peer_id. Su primer nombre es el
   visible del par; ante colisión usa el sufijo local (equipo). Una vez ligado,
   los cambios de nombre remoto nunca renombran la colección.
5. Asegura membership y registra peer_id, remote_entry_id y content_hash.

Reimportar el mismo snapshot no duplica. Si el host editó una entrada, el
contenido nuevo es otro snapshot y se crea o reutiliza por hash. Importar no es
una captura de clipboard: no invoca el watcher, PrivacyGate dependiente de app
activa, clipboard ni pegado sintético; sí respeta validación, tamaño, dedupe y
transacción local.

## 7. Experiencia de usuario

Settings incorpora Red local: toggle, nombre de equipo, estado seguro y
explicación de visibilidad LAN. Una vista Equipos, accesible desde acciones
globales, lista pares actuales e históricos sin mezclarlos con Colecciones. Las
cards locales y sus contratos de drag-and-drop no se reutilizan para pares.

Una fila de par muestra nombre, fingerprint abreviado, estado y acciones según
trust: Vincular, Ver historial, Desvincular, Bloquear o Desbloquear. El vínculo
pendiente usa un modal accesible con código, estado de confirmación, cancelar,
timeout, Escape y retorno de foco.

Ver historial muestra filas remotas sólo de lectura: previews de dos líneas,
paginación, carga/error e Importar. No tiene drag/drop, edición, pin, copy,
paste ni búsqueda remota. Tras commit, se refrescan historial y organización
local por eventos existentes de payload vacío; ningún toast o log incluye el
texto remoto.

## 8. Dependencias y alternativas

Se evaluarán versiones compatibles con Rust 1.85 de mdns-sd, tokio, rustls +
tokio-rustls, rcgen y keyring, siempre detrás de traits testeables. DNS-SD/mDNS
evita port scanning y usa APIs de registro/browse:
https://docs.rs/mdns-sd/latest/mdns_sd/struct.ServiceDaemon.html. RFC 6762 y
6763 definen el alcance local: https://datatracker.ietf.org/doc/html/rfc6762 y
https://datatracker.ietf.org/doc/html/rfc6763. rustls soporta autenticación de
cliente: https://docs.rs/rustls/latest/rustls/struct.ConfigBuilder.html.

Se descartan HTTP plano, Bonjour/Avahi nativo, IP manual, broadcast, libp2p y
QUIC en v1: reducen privacidad, duplican soporte de plataforma o introducen
complejidad destinada a Internet/NAT fuera del alcance.

## 9. Entrega incremental y dependencias

El diseño se implementa mediante cambios OpenSpec independientes; los cambios
posteriores no pueden adelantarse ni reimplementar los anteriores:

1. **local-peer-identity-foundation**: material de identidad seguro, perfil
   local y contracts testables, sin tráfico de red.
2. **local-peer-discovery**: opt-in, mDNS y registro/lista de pares
   descubiertos, sin vínculo ni API de contenido.
3. **local-peer-mutual-pairing**: listener TLS, código corto, confianza,
   revocación/bloqueo y health autenticado, sin historial remoto.
4. **peer-text-history-browser**: endpoint y vista de páginas de metadata y
   preview, sin descarga del texto completo ni mutación local.
5. **peer-text-import**: fetch explícito, dedupe, collection binding y
   procedencia, sin ampliar tipos transferibles.

Cada cambio debe preservar los contratos de sus predecesores, ejecutar sus
tests y dejar un commit funcional antes de abrir el siguiente. El documento
paraguas conserva las decisiones que comparten; no se entrega por sí solo.

## 10. Verificación

Cursores, límites, estados, código de vínculo, pinning, migraciones,
idempotencia y rollback se prueban sin mDNS real ni GUI. El runtime se cubre
con loopback y fakes de discovery, clock y keystore. Pruebas manuales en Ubuntu
GNOME Wayland, Ubuntu X11 y macOS cubren permiso Local Network, vínculo mutuo,
reconexión, desvínculo/bloqueo, importación, reinicio y falta de multicast o
firewall. No se inspeccionan ni modifican ~/.clipvault/assets.
