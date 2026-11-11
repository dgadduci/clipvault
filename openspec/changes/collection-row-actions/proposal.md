# Proposal: collection-row-actions

## Why

Las filas de colecciones muestran controles separados para color, nota y
eliminación. Esto deja poco espacio para los nombres y hace que la lista se
sienta recargada. El color asignado tampoco ayuda a distinguir las filas como
una superficie continua.

## What Changes

- Aplicar a cada fila un fondo con el color persistido de la colección al 18%
  de opacidad, conservando los indicadores de selección y drag-and-drop.
- Reemplazar los controles visibles de color, nota y eliminación por un único
  botón de menú por fila.
- Incluir en el menú las acciones existentes de cambiar color y eliminar
  (sólo para colecciones de usuario), manteniendo la confirmación propiedad
  del padre. Mostrar el icono de nota en la fila, entre el contador y el menú.
- Mantener en la fila el nombre, la etiqueta `system` para `Historial` o
  `imported` para una colección vinculada a un par, el contador, el icono de
  nota y el botón de menú, en ese orden.
- Localizar el nombre accesible del nuevo botón en `en`, `es`, `pt`, `de` y
  `fr`.

## Capabilities

### New Capabilities

### Modified Capabilities

- `tags-and-collections`: define la presentación compacta, el tinte y el menú
  accesible de las filas de colecciones.

## Impact

- Afecta `OrganizationSidebar.svelte`, sus estilos y las traducciones del
  frontend.
- Reutiliza los modales, eventos y comandos existentes; no requiere cambios de
  backend, migraciones, dependencias, red ni almacenamiento nuevo.
- La interacción de selección y el destino de drop siguen perteneciendo a la
  fila y a la lista desplazable existentes.
