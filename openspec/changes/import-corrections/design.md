## Context

`RemoteHistoryRail` ya descarta sus filas y muestra un placeholder cuando el
peer seleccionado no es `trusted && present`; por lo tanto, una card visible
no puede provenir de un peer inactivo o no confiable. La causa está después del
preview metadata-only: el host responde `FetchTextUnavailable { reason:
not_available }` cuando no tiene instalado un `FetchTextHostHandler`, algo
propio de una versión anterior. Además, TLS traduce `not_found` y
`not_transferable` a `TransportError::Malformed`, con lo que el core los
presenta como una indisponibilidad genérica en vez de como una captura que ya
no se puede importar.

El rail también intenta sincronizar los caches de servicios al reaccionar a
cambios de `peerId` y de `snapshot`. Durante el primer render el snapshot puede
ser nulo y esa sincronización registra falsamente el peer como no confiable e
inactivo. Cuando luego llega el snapshot correcto se inicia otra sincronización
sin orden de finalización garantizado por el bridge. Una respuesta vieja puede
persistir última y provocar `PeerUnavailable` al importar una card ya visible.

## Goals / Non-Goals

**Goals:**

- Mantener la protección existente que espera el estado sincronizado del peer.
- Conservar las razones de rechazo de un fetch sin inspeccionar contenido.
- Explicar de manera localizada si la instancia origen necesita actualizarse o
  si la captura ya no es transferible.
- Probar el mapeo desde TLS al core y el feedback de la card.
- Hacer monotónica la aplicación local de una instantánea de peer: no enviar
  estados desconocidos y no permitir que una sincronización anterior sobrescriba
  la más reciente.

**Non-Goals:**

- Reintentar automáticamente una importación fallida.
- Cambiar el protocolo de pares, el formato de previews o la persistencia.
- Convertir errores reales de transporte, revocación o peer inactivo en éxito.
- Permitir que un cliente actual importe desde un host que no implementa fetch;
  esa capacidad requiere actualizar el host.

## Decisions

### El preview no implica que el host pueda entregar el cuerpo

Listar una captura y descargar su contenido son capacidades distintas. El host
sin `FetchTextHostHandler` ya usa la razón estable `not_available`; el cliente
la preservará como `HostImportUnavailable` hasta la UI. La card explicará que
el equipo que contiene la captura necesita una versión compatible. No se
reintenta ni se degrada la seguridad: un cliente no puede implementar el
handler ausente en otro equipo.

Se descarta inferir compatibilidad a partir del preview, porque el protocolo
permite explícitamente que una versión anterior liste historial pero responda
`not_available` al fetch.

### Rechazos de captura permanecen tipados

Se agregará una variante específica de error de transporte para
`not_found`/`not_transferable` y otra para `not_available`. TLS las obtiene de
la razón protocolar estable; el adaptador del core las transforma en resultados
de importación explícitos y el bridge no necesita exponer el texto de la razón.

Se descarta usar `Malformed` como contenedor de rechazos esperables del host:
confunde una captura eliminada con una respuesta dañada y fuerza copy erróneo.

### La sincronización de estado requiere una instantánea conocida y ordenada

El rail no llamará los comandos `record_state` mientras el snapshot completo
sea desconocido. Si el snapshot ya fue recibido y no contiene al peer
seleccionado, sí registra el estado inactivo para revocar los caches. Para cada
peer, las escrituras de estado se encadenarán en el orden en que el rail observa
las instantáneas; una finalización tardía no puede adelantar ni reemplazar la
última transición. La señal `peerStateReady` sólo se activa después de completar
la escritura de la instantánea actual.

Se descarta confiar sólo en una generación de UI: ésta descarta actualizaciones
visuales tardías, pero no impone orden en los comandos Tauri que ya fueron
enviados al core.

### Mensajes localizados por causa recuperable

El feedback existente de preparación se conserva. Las nuevas claves en los
cinco catálogos describirán una captura no transferible y un host que requiere
actualización. El contenido, nombres de peers y razones wire no se interpolan.

## Risks / Trade-offs

- [Host antiguo] → La importación no puede completarse hasta actualizar y
  reiniciar el equipo origen; el feedback evita presentar ese caso como fallo
  de red.
- [Compatibilidad] → Las nuevas variantes son internas y se derivan de razones
  wire ya existentes, por lo que no cambia el mensaje ni el límite de payload.
- [Regresión de errores] → Tests de TLS y core fijan que una captura ausente o
  no transferible no termine como transporte no disponible.
- [Snapshot tardío] → La sincronización permanece deshabilitada hasta conocer
  el peer; las pruebas fijan que un estado provisional no llegue al importador.
- [Cambios rápidos de presencia] → La cola por peer conserva cada transición en
  orden y la última determina cuándo se habilita la acción.

## Migration Plan

No hay migración de datos ni protocolo. El cliente y el host deben actualizarse
para habilitar la importación de una captura existente; si se revierte, no se
modifican capturas ni colecciones locales.

## Open Questions

Ninguna.
