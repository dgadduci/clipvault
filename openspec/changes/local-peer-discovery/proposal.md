# Propuesta: descubrimiento persistente de pares locales

## Problema

Una identidad local por sí sola no permite que los equipos se encuentren.
ClipVault necesita descubrir de forma continua las demás instalaciones dentro
del mismo segmento LAN, conservar los equipos vistos y comunicar honestamente
cuando no están disponibles, sin escanear la red ni entregar contenido.

## Objetivo

Agregar opt-in de Compartir en red local y descubrimiento DNS-SD/mDNS para N
instalaciones ClipVault. La UI muestra pares detectados ahora y en sesiones
previas. Esta entrega no vincula, no abre API de contenido y no transfiere
capturas.

## Alcance

- Reutilizar local-peer-identity-foundation.
- Persistir pares observados, identidad pública, nombre, versión y últimos
  avistamientos en known_peers.
- Anunciar y browsear continuamente _clipvault._tcp.local sólo al activar el
  toggle, sin scan, broadcast propio ni IP manual.
- Mostrar estados No verificado, Detectado y No disponible; Activo queda para
  el health autenticado del cambio de vínculo.
- Declarar permisos Bonjour/Local Network de macOS.

## Fuera de alcance

- TCP de aplicación, TLS, health, pairing, trust, bloqueo, historial remoto,
  importación o clipboard.
- Puertos/endpoints persistidos, contenido o previews dentro de mDNS.

## Criterio de aceptación

Varias instalaciones con sharing opt-in en el mismo segmento se ven sin
reiniciar y siguen listadas al desaparecer. Desactivar sharing detiene anuncio
 y browse. Una red sin multicast muestra una degradación segura sin IP manual ni
 escaneo.
