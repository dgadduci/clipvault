# Diseño: renovación de anuncio local

## Decisión

`mdns-sd` usa 120 segundos para los registros host SRV/A. Aunque el browser
intenta refrescar su caché, en la red observada los dos lados pueden perder esa
renovación y emitir `ServiceRemoved` al expirar. El adaptador reanunciará el
registro local cada 45 segundos; eso entrega una respuesta DNS-SD no solicitada
antes del TTL y no puede vaciar la caché de un peer remoto.

## Estado y concurrencia

`MdnsHandle` conserva un `Arc<Mutex<ServiceInfo>>` con el último registro
publicado. El worker toma el lock, clona el registro y llama `register` sin
hacer `unregister`. `reconfigure` toma el mismo lock alrededor del retiro,
registro y reemplazo, por lo que un refresh no puede volver a anunciar un
puerto o capability anterior.

El cierre marca cancelación y une primero el worker de reanuncio. Sólo después
envía el goodbye y apaga el daemon. Errores de reanuncio se registran con una
frase fija: no fullname, host, IP, puerto, peer_id ni contenido.

## Alternativas descartadas

- `verify`: ante timeout vacía caché y convierte una pérdida transitoria de
  multicast en `ServiceRemoved`.
- Ignorar todos los `ServiceRemoved`: impediría reflejar un goodbye real.
- Sonda TCP/TLS: mezcla presencia con pairing y añade tráfico/estado no
  requerido por discovery.
