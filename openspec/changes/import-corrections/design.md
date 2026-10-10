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

## Goals / Non-Goals

**Goals:**

- Mantener la protección existente que espera el estado sincronizado del peer.
- Conservar las razones de rechazo de un fetch sin inspeccionar contenido.
- Explicar de manera localizada si la instancia origen necesita actualizarse o
  si la captura ya no es transferible.
- Probar el mapeo desde TLS al core y el feedback de la card.

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

## Migration Plan

No hay migración de datos ni protocolo. El cliente y el host deben actualizarse
para habilitar la importación de una captura existente; si se revierte, no se
modifican capturas ni colecciones locales.

## Open Questions

Ninguna.
