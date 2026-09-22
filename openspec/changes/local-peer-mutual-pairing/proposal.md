# Propuesta: vínculo mutuo seguro entre pares locales

## Problema

El descubrimiento mDNS indica presencia, pero sus records no bastan para
confiar en un equipo ni para habilitar lectura de contenido. ClipVault necesita
que dos personas aprueben la misma relación una sola vez y que todo acceso
posterior se autentique sin repetir esa interacción.

## Objetivo

Permitir que un par descubierto se vincule tras mostrar y aprobar en ambos
equipos el mismo código corto. Persistir la clave pública fijada y habilitar
sólo health autenticado; ningún historial ni contenido se expone aún.

## Alcance

- Reutilizar identidad y known_peers de los cambios previos.
- Añadir listener TCP/TLS local, identidad certificada y mTLS pinning.
- Implementar invitación, código decimal de seis dígitos, doble aprobación,
  cancelación/timeout y trust state, incluida una invitación inbound visible
  aunque el panel de Compartir no esté abierto.
- Permitir N vínculos independientes por identidad.
- Exponer desvincular, bloquear, desbloquear y health autenticado.

## Fuera de alcance

- Listar, previsualizar, obtener o importar capturas.
- Auto-pair, aceptación unilateral, QR, IP manual, CA externa, relay o
  sincronización.
- Borrar capturas/colecciones locales al revocar.

## Criterio de aceptación

Dos pares detectados pueden completar el código recíproco y luego reconocerse
silenciosamente por TLS. Bloquear/revocar uno no altera a los demás pares ni
revela contenido a un cliente no confiado. Una fila en `Revoked` debe seguir
permitiendo iniciar una sesión SAS recíproca nueva; la doble aprobación sigue
siendo el único camino que restablece `Trusted`. `Blocked` sí sigue siendo
terminal hasta `Desbloquear`. La ruta resuelta por mDNS debe ser compatible
con la familia IP del listener TLS para que un anuncio dual-stack no vuelva
indeterminista el re-pairing.
