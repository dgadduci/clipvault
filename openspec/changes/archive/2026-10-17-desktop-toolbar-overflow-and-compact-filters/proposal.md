# Proposal: desktop-toolbar-overflow-and-compact-filters

## Why

La barra superior reúne búsqueda, filtros, creación de capturas y acciones
globales. Cuando su contenido supera el ancho disponible, todos los controles
deben seguir accesibles sin expandir el documento ni interferir con el rail de
cards. Reducir algunos controles libera espacio para las acciones frecuentes.

## What Changes

- Mantener todos los controles en una sola fila desplazable horizontalmente
  cuando superen el ancho disponible.
- Reducir a la mitad el ancho de los filtros de tags y colecciones, de 220 px
  a 110 px; el filtro de aplicación conserva su ancho actual.
- Reducir el ancho base de búsqueda de 240 px a 180 px, equivalente al 75 %
  del ancho base actual, y resumir su placeholder a «Buscar» en cada idioma.
- Presentar el atajo para crear una captura de texto como un badge dentro del
  botón.
- Mostrar mediante hover el nombre completo de cada opción de los filtros de
  tags y colecciones.
- Mantener seleccionado en el modal de una captura el tag que se acaba de
  crear, incluso cuando se actualiza el snapshot de organización.
- Agregar a cada tag del selector de captura una acción con icono para
  eliminarlo globalmente, retirando sus asociaciones de todas las capturas y
  refrescando la interfaz después de guardar.
- Conservar accesibilidad, acciones, navegación por teclado y menú existente.

## Modified Capabilities

- `desktop-shell-layout`: desplazamiento horizontal y dimensiones compactas de
  los controles de la barra superior.
- `tags-and-collections`: mostrar los nombres completos de opciones y permitir
  eliminar una definición de tag desde el selector de una captura.
- `clipboard-history-cards`: conservar la selección recién creada y reflejar
  la eliminación global de un tag en el selector y en las cards.

## Impact

- Afecta `TagFilter`, `CollectionFilter`, `TagSelectorModal` y la
  sincronización de metadata de organización en `App.svelte`.
- La eliminación reutiliza el comando local existente `tags_delete` y su
  evento metadata-only; SQLite ya elimina en cascada las asociaciones sin
  borrar capturas.
- No requiere dependencias ni cambios de esquema.
