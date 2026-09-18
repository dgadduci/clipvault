# Diseño: descubrimiento persistente de pares locales

## Dependencia y frontera

Depende de local-peer-identity-foundation. Esta entrega incorpora el adaptador
productivo de mDNS en `clipvault-platform`, detrás de `PeerDiscoveryAdapter`;
no se difiere a un crate ni a un cambio futuro. Usa `mdns-sd` como dependencia
opcional, madura y compartida por macOS y Linux, activada sólo por el feature
`local-peer-discovery-mdns` del shell productivo. El core recibe eventos
normalizados y decide validación/persistencia; Tauri sólo inicia/detiene el
runtime y entrega DTOs metadata-only.

El contrato neutral de plataforma define `DiscoveryAdvertisement` y
`DiscoveryEvent` (resuelto/eliminado), ambos con sólo `peer_id`, fingerprint,
nombre, versión y capability. `PeerDiscoveryAdapter::start` recibe el anuncio
local y un sink de esos eventos. `MdnsPeerDiscoveryAdapter` registra y navega
continuamente `_clipvault._tcp.local.`, con puerto `0` y capability
`discovery_only`, y convierte los eventos de `mdns-sd` al contrato neutral.
Puede usar host, IP y puerto sólo dentro del adaptador para operar mDNS: nunca
los transmite al core, frontend, logs ni SQLite.

El shell instala un único worker administrado por el ciclo de vida del runtime:
el sink alimenta al `PeerDiscoveryRuntime`, que procesa/persiste eventos de
forma continua mediante el servicio core. Al apagar o desactivar sharing, el
worker se detiene y el adaptador retira el anuncio y browse; los `known_peers`
persistidos se conservan. Los comandos Tauri siguen siendo adaptadores
delgados, sin pollers o lógica de mDNS propia.

No se agrega servidor TCP en este cambio. El anuncio DNS-SD usa el mismo tipo
de servicio futuro, _clipvault._tcp.local, con puerto 0 y capability
discovery_only. Un cliente no debe intentar conectarse a un descriptor de
discovery_only. local-peer-mutual-pairing sustituirá el registro por un puerto
TLS real y capability pairing sin cambiar peer_id.

## Datos y estados

La migración crea known_peers con peer_id, public_key_fingerprint,
display_name, protocol_major, trust_state, first_seen_at,
last_discovered_at y updated_at. No contiene IP, puerto, texto ni clave. El
estado inicial es unverified; el estado de presencia se deriva en memoria de
los eventos mDNS y un TTL. Un peer visible se presenta como Detectado o No
verificado; uno conocido cuyo TTL vence se presenta No disponible.

Antes de persistir, el core valida peer_id/fingerprint/versión/nombre y
descarta el propio peer_id. Si un anuncio intenta reutilizar un peer_id con una
identidad diferente, no reemplaza el dato previamente persistido.

## Opt-in y privacidad

El cambio agrega local_peer_sharing_enabled, default false. Activarlo requiere
una identidad segura ya disponible; de lo contrario no persiste el toggle.
Al desactivarlo, shutdown retira anuncio y browser, deja known_peers intacto y
no inicia una conexión saliente.

TXT sólo transmite peer_id, fingerprint o clave pública, versión mayor,
capability discovery_only y nombre visible. Nunca transporte contenido,
preview, conteo, tags, collections, hash, source app, ruta o secreto.

macOS declara realmente `NSLocalNetworkUsageDescription` y
`NSBonjourServices` para `_clipvault._tcp` en la configuración o `Info.plist`
que usa el bundle final; comentarios en `build.rs` o `tauri.conf.json` no son
suficientes. Linux Wayland y X11 usan el mismo runtime puro Rust; cualquier
firewall/multicast bloqueado es una limitación visible, no una razón para
escanear.

## UI y pruebas

Settings agrega el toggle. La vista Equipos lista el snapshot persistido y
presente, con nombre, fingerprint abreviado y presencia. Sólo se muestra
información: no hay acción Vincular ni Ver historial todavía.

Tests puros cubren validación, self-filter, merge idempotente, TTL,
desactivación y no persistencia de endpoints. El adaptador productivo se prueba
con dos daemons mDNS de loopback cuando sea estable, además de fakes. La prueba
manual se limita a ver N equipos y a apagar/encender sharing en Wayland, X11 y
macOS.

## Dependencia aprobada

- `mdns-sd`: backend DNS-SD/mDNS local multiplataforma para el adaptador de
  plataforma. Se descartan Bonjour/Avahi directos por duplicar FFI y caminos
  de plataforma, y broadcast/IP manual/escaneo por contradecir el alcance de
  privacidad. Su uso queda restringido al feature del adaptador productivo;
  core y tests puros no dependen de sockets ni de una sesión gráfica.
