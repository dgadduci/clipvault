## Context

`RemoteHistoryRail` obtiene previews y sincroniza en paralelo el estado
`trusted`/`active` del peer con los servicios de historial, importación de
texto e imagen. El rail puede renderizar una fila antes de que esa promesa
termine. `RemotePreviewCard` recibe `peerStateReady`, pero la acción de importar
texto no lo usa; el importador, cuyo cache aún está vacío, rechaza la solicitud
como `PeerUnavailable`. La UI reduce esa respuesta a un mensaje genérico de no
disponibilidad.

## Goals / Non-Goals

**Goals:**

- Hacer que toda acción de importar use el mismo estado de peer que habilitó el
  preview.
- Impedir una solicitud de importación durante la ventana de sincronización.
- Mantener el core, TLS, el contrato metadata-only y los gates de seguridad
  existentes sin cambios de permisos.
- Expresar el estado transitorio con texto localizado y probar la regresión.

**Non-Goals:**

- Reintentar automáticamente una importación fallida.
- Cambiar el protocolo de pares, el formato de previews o la persistencia.
- Convertir errores reales de transporte, revocación o peer inactivo en éxito.

## Decisions

### La card bloquea importación hasta `peerStateReady`

La acción de Importar deshabilitará el control y no llamará los comandos Tauri
mientras la instantánea activa no esté sincronizada. El preview permanece
visible porque ya es metadata-only y no presupone que el importador esté listo.
Esto reutiliza la señal existente en lugar de agregar un nuevo round-trip.

Se descarta importar y reintentar tras `PeerUnavailable`: ese error también
representa revocación o peer inactivo, por lo que reintentar ocultaría un estado
de seguridad real y podría enviar tráfico innecesario.

### Invalidación por generación conserva la seguridad ante cambios de peer

El rail ya incrementa una generación cuando cambia la instantánea de peer. La
preparación sólo será verdadera cuando la promesa de esa generación termine y
el peer siga seleccionado. Al cambiar, bloquear o desvincular, se revoca de
inmediato antes de que una respuesta tardía pueda habilitar otra card.

Se descarta derivar preparación solamente de que existan filas, porque una fila
puede pertenecer a una solicitud previa o llegar antes de sincronizar los
servicios de importación.

### Mensaje localizado para preparación transitoria

El botón y su estado accesible usarán una nueva clave en los cinco catálogos.
Los resultados `peer_unavailable` y `transport_unavailable` reales conservan
su copy existente; la preparación no se presenta como un error.

## Risks / Trade-offs

- [Sincronización lenta] → La acción queda deshabilitada brevemente, pero el
  preview sigue disponible y se evita una falsa indisponibilidad.
- [Regresión de imágenes] → La misma señal debe gatear texto e imágenes; los
  tests cubren ambos caminos de forma estructural.
- [Respuesta tardía] → La generación y la limpieza actual invalidan la
  preparación antes de actualizar la UI.

## Migration Plan

No hay migración de datos ni protocolo. Desplegar junto con tests de frontend;
si se revierte, se restaura la interacción previa sin modificar capturas ni
colecciones locales.

## Open Questions

Ninguna. El estado `peerStateReady` y la generación ya existen en el rail.
