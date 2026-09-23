# Renovación de anuncios mDNS de pares locales

## Problema

Dos equipos macOS/Linux pueden resolverse inicialmente y completar TLS, pero
ambos pasan a `No disponible` aproximadamente al vencer el TTL host de mDNS.
El cambio anterior retiró la verificación `verify`, por lo que la baja ahora
proviene de la expiración normal de caché cuando una renovación de browser no
llega a la red.

## Propuesta

El adaptador mDNS renovará proactivamente su propio `ServiceInfo` cada 45
segundos mediante `ServiceDaemon::register` con el registro actualmente
publicado. La API de `mdns-sd` define esa operación como reanuncio y no exige
`unregister` previo.

La renovación no consulta, no valida ni elimina pares remotos. Conserva el
registro actualizado tras el cambio discovery-only → pairing y se detiene antes
del goodbye ordenado.

## Alcance

- Añadir un worker privado de reanuncio y su estado mínimo.
- Mantener sincronizado el `ServiceInfo` después de `reconfigure`.
- Cubrir el estado de publicación y el cierre sin multicast real.
- Validar con una prueba manual macOS ↔ Linux de más de seis minutos.

## Fuera de alcance

- No restaurar `ServiceDaemon::verify`.
- No hacer health checks TCP/TLS, escaneo de red ni polling de historial.
- No cambiar la persistencia, UI ni el contrato del core.
