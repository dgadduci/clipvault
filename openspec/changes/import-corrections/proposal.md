## Why

La importación de una captura remota existente puede terminar con el mensaje
genérico “not available” aun cuando el preview sea válido. El protocolo usa
ese motivo cuando el equipo origen no tiene instalado el handler de fetch
(por ejemplo, porque todavía ejecuta una versión anterior), y además reduce
incorrectamente las respuestas `not_found` y `not_transferable` a un error de
transporte genérico. El preview sólo prueba que el host puede listar
metadatos; no prueba que pueda entregar el cuerpo de una captura.

## What Changes

- Conservar la espera actual de sincronización de confianza/actividad antes de
  habilitar la importación.
- Preservar `not_found` y `not_transferable` como un rechazo tipado desde TLS
  hasta el importador, para mostrarlos como una captura que ya no se puede
  transferir, en vez de “not available”.
- Preservar `not_available` como señal de capacidad del host y comunicar que
  el equipo origen debe actualizar ClipVault, sin intentar importar ni mutar
  el historial local.
- Añadir regresiones de protocolo, core y frontend que cubran ambas rutas y
  mantengan el comportamiento independiente del sistema operativo.

## Capabilities

### New Capabilities

- `peer-import-readiness`: Coordina la disponibilidad de la acción de importar
  con la sincronización y elegibilidad de estado del peer sin exponer contenido
  remoto.

### Modified Capabilities

- `peer-text-import`: La importación explícita requiere un estado de peer
  sincronizado antes de abrir el transporte y conserva los rechazos del host.
- `peer-preview-import-feedback`: La card remota debe comunicar la preparación,
  una captura no transferible y un host que requiere actualización.

## Impact

- Frontend: `RemotePreviewCard.svelte`, tipos, catálogos de traducción y
  regresiones frontend.
- Core/Tauri/TLS: se conserva el gate de confianza existente y se amplía la
  taxonomía tipada de rechazos del fetch, sin modificar contenido, permisos ni
  el formato de los mensajes del protocolo.
- No se agregan dependencias, llamadas de red, telemetría ni persistencia nueva.
