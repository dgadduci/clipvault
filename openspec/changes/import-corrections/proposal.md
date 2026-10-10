## Why

Una captura remota puede mostrarse correctamente en el preview mientras el
cache de confianza/actividad del importador todavía se sincroniza de forma
asíncrona. En ese intervalo, al confirmar la importación —sobre todo de
texto— el core responde `PeerUnavailable` y la interfaz sólo muestra “not
available”, aunque el peer y la captura siguen disponibles.

## What Changes

- Impedir que una card remota habilite la importación hasta que la misma
  instantánea de confianza/actividad se haya sincronizado con el importador.
- Mantener el preview de sólo metadatos visible durante esa sincronización, sin
  iniciar una petición de cuerpo ni mutar el historial local.
- Distinguir en la interfaz el estado transitorio de sincronización del peer de
  una indisponibilidad real de peer o transporte.
- Añadir una regresión que cubra importaciones de texto habilitadas sólo después
  de completar la sincronización, de manera independiente del sistema operativo.

## Capabilities

### New Capabilities

- `peer-import-readiness`: Coordina la disponibilidad de la acción de importar
  con la sincronización de estado del peer sin exponer contenido remoto.

### Modified Capabilities

- `peer-text-import`: La importación explícita requiere un estado de peer
  sincronizado antes de abrir el transporte.
- `peer-preview-import-feedback`: La card remota debe comunicar la preparación
  transitoria y no presentar un fallo genérico por una carrera local.

## Impact

- Frontend: `RemoteHistoryRail.svelte`, `RemotePreviewCard.svelte`, tipos,
  catálogos de traducción y regresiones frontend.
- Core/Tauri: se conserva el gate de confianza existente; no se modifica el
  contenido que cruza el bridge ni el protocolo de pares.
- No se agregan dependencias, llamadas de red, telemetría ni persistencia nueva.
