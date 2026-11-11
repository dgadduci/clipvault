# Design: search-result-organization-hydration

## Resultados de búsqueda visibles

`performSearch` solicitará la hidratación de tags y memberships para la página
de resultados antes de publicarla en `visibleEntries`, sin depender de que la
colección oculte capturas compartidas de subcolecciones. Los tags y las
colecciones son datos de presentación de cada card y deben estar disponibles
para cualquier resultado mostrado.

## Alcance del caché de organización

`hydrateEntryOrganization` reconciliará su caché contra la lista actualmente
visible cuando una búsqueda esté activa. La página base de `entries` puede ser
distinta de `visibleEntries`; reconciliar solo contra la primera elimina las
claves de los resultados buscados y hace que `HistoryCardRail` los represente
como fallos. Los resultados de búsqueda se mantienen como entradas adicionales
durante la hidratación de la página base.

Al limpiar la búsqueda, la hidratación vuelve a reconciliarse contra la página
base normal. El orden y el ranking de la búsqueda no cambian. No se agregan
dependencias ni textos de interfaz.

## Filtro de colecciones en Historial

`OrganizationSidebarSnapshot` ya entrega las colecciones principales y sus
subcolecciones. El filtro de Historial expondrá todas las colecciones de tipo
usuario. Las opciones hijas mostrarán `Principal / Subcolección` para hacer
visible su jerarquía sin alterar el id que se envía al backend. La selección
de una hija seguirá usando su id para filtrar las capturas asignadas a esa
subcolección.

## Opciones del filtro de tags

Las opciones dejarán de derivarse del caché de memberships de las cards
hidratadas en la página actual. Se agregará una consulta de metadatos en SQLite
que devuelve los ids de tags distintos asociados a capturas del scope activo.
El scope aplica la colección elegida en Historial o la colección activa; para
Historial sin colección elegida incluye todas sus capturas, y el scope sin
asignar excluye las capturas que pertenecen a colecciones de usuario.

La consulta no tendrá `LIMIT` ni `OFFSET` y no aplicará la búsqueda, el tag ya
seleccionado, el filtro de aplicación de origen ni el tipo de contenido. Así,
la lista representa todos los tags de la colección, incluso mientras la lista
visible pagina o busca. La consulta se refresca cuando cambia el scope y ante
actualizaciones de organización; cambiar de página no requiere otra consulta.

## Verificación

- Añadir regresiones para resultados buscados fuera de la página base, opciones
  de colección jerárquicas y opciones de tags agregadas fuera de la página.
- Probar la consulta de tags para Historial, colección, subcolección y scope
  sin asignar.
- Ejecutar las pruebas frontend afectadas, build, regresiones protegidas de
  drag and drop, validación estricta de OpenSpec y `git diff --check`.
