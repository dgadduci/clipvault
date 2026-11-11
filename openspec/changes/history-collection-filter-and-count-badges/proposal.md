# Proposal: history-collection-filter-and-count-badges

## Why

En `Historial`, el usuario puede acotar las capturas por aplicación y tag, pero
no por colección. Para hacerlo debe abandonar `Historial` y navegar a la
colección, lo que dificulta combinar la colección con los filtros actuales.
Además, la lista lateral no muestra cuántas capturas contiene cada colección.

## What Changes

- Añadir a la barra de filtros un selector de colección disponible al navegar
  `Historial`, con una opción para quitar el filtro y las colecciones locales
  disponibles.
- Aplicar la colección seleccionada inmediatamente y combinarla con la
  búsqueda, el filtro de aplicación y el filtro de tag existentes.
- Mantener las opciones sincronizadas al crear, modificar o eliminar
  colecciones; limpiar la selección si se elimina la colección elegida.
- Mostrar al final de cada fila de colección un badge con el número total de
  capturas asociadas, incluido el valor cero.
- Mantener actualizados los contadores tras cambios de colecciones,
  capturas y membresías.
- Añadir los nombres accesibles y demás textos nuevos a los catálogos `en`,
  `es`, `pt`, `de` y `fr`.

## Capabilities

### New Capabilities

### Modified Capabilities

- `desktop-filtering`: filtrar `Historial` por una colección sin quitar los
  filtros existentes.
- `tags-and-collections`: mostrar y mantener actualizados los contadores de
  capturas por colección.

## Impact

- Afecta el estado y la barra de filtros del desktop, la consulta existente de
  capturas filtradas, el snapshot de colecciones mostrado en el sidebar y sus
  actualizaciones.
- El conteo debe resolverse en la capa de persistencia con una consulta
  agregada de metadata, sin cargar el contenido de las capturas ni hacer una
  consulta individual por colección.
- No requiere migración de esquema ni dependencias nuevas.
- No modifica la búsqueda global de Quick Paste ni el comportamiento de
  navegación entre colecciones.
