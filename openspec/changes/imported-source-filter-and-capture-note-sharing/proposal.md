# Propuesta: filtros de origen importado y envío opcional de notas

## Why

El filtro de aplicaciones del escritorio obtiene sus opciones de los campos
de aplicación locales de cada captura. La atribución de una captura importada
se guarda por separado en la procedencia del par, así que el filtro no muestra
el nombre ni el icono que la card ya puede presentar. Esto deja incompletas las
opciones en Historial y en las colecciones.

Las notas de capturas son privadas y actualmente no forman parte de las
transferencias entre pares. Se necesita una opción local explícita para que el
dueño de la nota decida si quiere compartirla durante una importación de una
captura.

## What Changes

- Incluir en el filtro de aplicaciones las atribuciones válidas de capturas
  importadas, con el nombre y el icono local disponibles en su procedencia.
- Usar la misma atribución efectiva que presentan las cards y mantener las
  opciones y el filtrado dentro de Historial y de cualquier colección activa.
- Agregar en el modal actual de Configuración general una casilla persistente
  para permitir el envío de notas de capturas durante transferencias entre
  pares. La preferencia nueva queda desactivada por defecto.
- Enviar una nota únicamente durante la solicitud explícita de importar una
  captura y sólo si el equipo dueño de la copia habilitó la opción y el par
  receptor anuncia compatibilidad.
- Mantener las notas fuera de la navegación remota, vistas previas, búsqueda,
  eventos y registros. Una nota local ya existente en una captura duplicada
  no será sobrescrita por la nota recibida.

El item existente `Configuración general` del menú de desbordamiento es la
entrada a la preferencia; no hace falta crear un segundo acceso.

## Capabilities

### Modified

- `desktop-filtering`: las opciones y coincidencias del filtro de aplicaciones
  incorporan atribución importada dentro del alcance activo.
- `capture-notes`: las notas siguen siendo locales por defecto y sólo pueden
  salir mediante la preferencia explícita de exportación.
- `privacy-settings`: persiste y presenta la preferencia local de exportación
  de notas en Configuración general.

### Added

- `peer-capture-note-sharing`: transferencia opcional, acotada y compatible
  con pares antiguos de notas asociadas a capturas de texto e imagen.

## Fuera de alcance

- Compartir notas de colecciones.
- Enviar notas mientras se explora el historial remoto, se buscan capturas o
  se solicitan miniaturas.
- Sincronización automática, nube, transferencia de notas sin una acción
  explícita de importar o modificación de las reglas actuales de captura.
- Cambiar la composición visual o la semántica de búsqueda y filtros que no
  sean los necesarios para incluir atribución importada.
