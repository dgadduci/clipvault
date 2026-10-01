# Propuesta: fila desplazable para los controles del toolbar

## Por qué

Cuando el ancho de la ventana no alcanza para mostrar la búsqueda, los filtros
y las acciones del toolbar, los controles pasan a una segunda línea. Esto
aumenta la altura de la barra y altera la distribución del historial.

## Qué cambia

- Mantener la búsqueda, los filtros, la acción «Texto» y las acciones del menú
  en una sola fila sin salto de línea.
- Permitir el desplazamiento horizontal de toda la fila cuando sus controles
  excedan el ancho disponible.
- Limitar el alto de la fila al control más alto.
- Mantener los menús de filtros y acciones visibles fuera del viewport de
  desplazamiento mientras sigan anclados a sus controles.

## Fuera de alcance

- Cambiar qué filtros, botones o acciones ofrece el toolbar.
- Alterar la búsqueda, los resultados, las colecciones o las cards.
- Cambiar el comportamiento del drag-and-drop.
- Agregar dependencias.

## Impacto esperado

- Frontend: `DesktopToolbar.svelte`, los controles de filtro y una utilidad
  pequeña para ubicar los menús desplegables respecto de sus botones.
- Verificación: checks/builds del frontend y regresiones protegidas de
  drag-and-drop.
