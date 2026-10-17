# Design: history-collection-filter-and-count-badges

## Fronteras de arquitectura

```text
DesktopToolbar.svelte
  └── CollectionFilter.svelte       → selector accesible y presentacional
          ↓ selección por ID
App.svelte                           → estado canónico y composición de filtros
          ↓ collection_id + tags + aplicación
recent_entries_filtered / search    → consulta existente en core y SQLite

OrganizationSidebar.svelte           → badge de conteo por fila
          ↑ snapshot metadata-only
OrganizationSidebarSnapshot         → conteo agregado de membresías
```

La GUI no consulta SQLite ni cuenta las cards actualmente renderizadas. El
backend devuelve únicamente metadata necesaria para el selector y los badges.

## Filtro por colección

- `App.svelte` conserva un estado separado para la colección usada como filtro;
  este estado no reemplaza la colección activa del sidebar.
- El selector se muestra en el ámbito `Historial`. Sus opciones son `Todas`
  primero y luego las colecciones locales de usuario del snapshot, en el mismo
  orden que la lista lateral. La colección de sistema `Historial` no aparece
  como filtro porque ya representa el conjunto sin restricción.
- Se conserva el ID al renombrar una colección y la etiqueta visible toma el
  nombre nuevo del snapshot. Crear una colección agrega su opción. Eliminar la
  colección seleccionada restablece el filtro a `Todas`.
- Al navegar fuera de `Historial`, el filtro de colección se restablece a
  `Todas`; la selección del sidebar sigue controlando el ámbito actual.
- En `Historial`, el ID seleccionado se pasa por la misma ruta de consultas de
  recents y búsqueda como filtro de colección. `Todas` conserva el ámbito
  completo de `Historial`. Los filtros de aplicación, tag y texto se mantienen
  y se combinan con el predicado de colección.
- La selección aplica inmediatamente. El toolbar delega el cambio al estado
  propietario en `App.svelte`, igual que los filtros existentes.
- Las opciones se derivan del snapshot de organización actualizado tras crear,
  modificar o eliminar colecciones. Esto incluye cambios de nombre y otros
  metadatos; una respuesta atrasada no puede reemplazar opciones de un estado
  más reciente. Se reutilizan los eventos/refresh existentes sin registrar
  listeners duplicados.

## Contador de capturas

- El snapshot metadata-only de colecciones incluirá el total de capturas de
  cada colección, calculado en la base de datos mediante una consulta agregada
  sobre las membresías. Se cuentan entradas persistidas distintas, de todos
  los tipos de contenido y tanto favoritas como no favoritas; no se limita el
  total a la búsqueda o al máximo de cards visibles.
- `Historial` representa todas las capturas persistidas. Cada colección de
  usuario cuenta sus propias membresías; una captura asociada a varias
  colecciones aparece en el conteo de cada una.
- El badge muestra el número, también cuando es cero, en el extremo derecho de
  la línea de la colección. Su nombre accesible usa una etiqueta pluralizada y
  localizada. Cada fila conserva una altura compacta fija de `2rem`; el badge
  no puede aumentarla. El nombre, las acciones y el área de drop de la fila
  mantienen su comportamiento; el badge no intercepta eventos de puntero ni
  modifica el contrato de drag and drop.
- El frontend refresca el snapshot ante los eventos existentes de actualización
  de organización y de historial, de forma que renombrado/creación/eliminación,
  asignación/retirada de membresías y altas/bajas de capturas actualicen las
  opciones y los contadores sin reiniciar la aplicación.
- Ninguna respuesta, evento o log nuevo incluye contenido, hashes, snippets,
  rutas o IDs de capturas; el único dato nuevo del resumen es el conteo.

## Contratos de interfaz

- La nueva etiqueta del filtro y el nombre accesible del badge viven en los
  catálogos `en`, `es`, `pt`, `de` y `fr` y mantienen placeholders en paridad.
- El selector usa un `<select>` nativo accesible, con `Todas` como primera
  opción, texto blanco en el menú oscuro, foco visible y navegación por teclado
  y Escape del control nativo. La selección se aplica inmediatamente desde el
  estado propietario en `App.svelte`.
- La barra conserva los filtros de aplicación y tag existentes. En ventanas
  estrechas puede usar el reflow/scroll previsto por el layout actual sin
  cambiar la geometría de las cards ni las interacciones del sidebar.
- Las cards permanecen en una fila horizontal sin salto, con ancho fijo de
  `--cv-card-size`; el propio rail contiene el desplazamiento horizontal.

## Verificación enfocada

- Probar selección, limpieza, composición con búsqueda/aplicación/tag y
  sincronización del selector tras crear, renombrar y borrar una colección.
- Probar conteos de `Historial`, colecciones vacías y colecciones con capturas
  de tipos distintos; comprobar refresco tras cambiar membresías y eliminar o
  agregar capturas.
- Mantener las regresiones de drag and drop de cards y colecciones, la paridad
  de traducciones, los checks frontend/backend afectados y la validación
  estricta de OpenSpec.
