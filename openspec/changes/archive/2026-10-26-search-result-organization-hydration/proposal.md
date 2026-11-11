# Proposal: search-result-organization-hydration

## Why

Al buscar dentro de una colección, la búsqueda puede mostrar capturas que no
están en la página reciente cargada como base de la colección. Esas cards
dependen de la hidratación de memberships y tags para dibujar sus etiquetas.
Actualmente se hidratan los resultados solo bajo una condición de visibilidad
de subcolecciones y una hidratación posterior puede descartar resultados que
no estén en la página base. La card entonces interpreta la ausencia del caché
como un error real y muestra “No se pudieron cargar las etiquetas de esta
entrada.”

## What Changes

- Hidratar tags y memberships de todos los resultados visibles de búsqueda,
  independientemente de la configuración de visibilidad de subcolecciones.
- Conservar en el caché los datos de los resultados buscados mientras esas
  capturas sean las que se muestran, aunque no pertenezcan a la página base.
- Mantener el indicador de error únicamente para fallas reales al consultar
  los datos de organización de una captura.
- Incluir las subcolecciones en el filtro de colecciones disponible desde
  Historial, mostrando la ruta de la colección principal para distinguirlas.
- Obtener las opciones del filtro de tags desde todas las capturas del scope
  activo, sin limitarlas a la página visible ni a la búsqueda.

## Fuera de alcance

- No cambiar el ranking ni los filtros de búsqueda.
- No cambiar la paginación ni el orden de las capturas; sus controles siguen
  aplicándose a las capturas visibles, no a los agregados de opciones.
- No cambiar el texto o la presentación de tags y colecciones en las cards.
