# Propuesta: liveness confiable de pares locales

## Problema

La presencia actual vence 120 segundos después del último
`DiscoveryEvent::Observed`. Ese evento es una resolución de DNS-SD, no un
heartbeat de aplicación. En pruebas reales macOS ↔ Linux, dos equipos que
siguen encendidos, con sharing habilitado y sin bloqueo dejan de verse como
activos después de algunos minutos; abrir o usar ClipVault no los recupera.

Esto es un falso negativo de ClipVault, no una desconexión de la LAN. El
estado visible no puede depender de que el backend mDNS repita un evento de
resolución dentro de un TTL que el core eligió por separado.

## Objetivo

Mantener `Detectado`/`Activo` mientras el browser DNS-SD mantenga un servicio
válido, retirar la presencia inmediatamente ante un goodbye o una remoción
confirmada, y detectar una baja abrupta de manera acotada sin escanear la red
ni abrir una conexión de contenido.

## Alcance

- Reemplazar el vencimiento fijo de presencia en el core por eventos de
  liveness autoritativos del adaptador de plataforma.
- Hacer que el adaptador mDNS confirme periódicamente las instancias que
  siguen observadas y emita `Removed` sólo ante goodbye, expiración de cache o
  una verificación mDNS que no recibe respuesta dentro del límite definido.
- Actualizar `mdns-sd` detrás de su feature opcional si es necesario para usar
  su API de verificación DNS-SD; documentar la versión compatible y el motivo.
  Implementación: bump `mdns-sd` de `"0.11"` a `"0.13"` (`cargo update -p
  mdns-sd` resuelve a `0.13.11`). La versión 0.13 sigue declarando
  `rust-version = "1.71"`, dentro del MSRV del workspace (`1.85`). La
  dependencia permanece detrás del feature `local-peer-discovery-mdns` y
  sólo se enlaza desde `clipvault-platform`.
- Conservar el refresh metadata-only ya existente de la UI; no se agrega un
  poller de sockets, health TLS periódico, escaneo de puertos ni tráfico de
  historial.
- Cubrir la regresión de presencia sostenida por encima de 120 segundos y los
  caminos de baja ordenada y abrupta.

## Fuera de alcance

- Cambios en pairing, mTLS, historial remoto, importación, SQLite o el
  contrato IPC metadata-only.
- IP manual, broadcast propio, descubrimiento fuera del enlace local, relay,
  cloud o sincronización.
- Mantener despierto un equipo suspendido, bloquear App Nap, impedir idle
  sleep o elevar permanentemente el consumo de CPU/red.

## Criterio de aceptación

Dos instalaciones con sharing activo permanecen `Detectado`/`Activo` durante
al menos tres intervalos de la anterior ventana de 120 s sin interacción de
usuario. Un cierre o toggle-off ordenado se refleja mediante goodbye; una
caída abrupta cambia a `No disponible` sólo después de una comprobación mDNS
acotada fallida. Ninguno de los caminos expone IP, puerto, contenido, hashes,
secretos o tráfico de la API de historial.
