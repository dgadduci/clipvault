# tags-and-collections

Este cambio agrega organización local al historial de ClipVault sin modificar
el contenido de las capturas.

## Decisiones consensuadas

- Toda captura pertenece siempre a la colección de sistema `Historial`.
- `Historial` es permanente, no puede renombrarse ni eliminarse.
- Una captura puede pertenecer a varias colecciones planas además de
  `Historial`.
- Quitar una captura de una colección secundaria sólo quita esa asociación.
- Quitar una captura de `Historial` equivale a eliminarla definitivamente de
  ClipVault y de todas sus colecciones, con confirmación explícita.
- Las colecciones secundarias pueden eliminarse; sus capturas permanecen en
  `Historial` y en las demás colecciones.
- Los tags son libres, locales, únicos sin distinguir mayúsculas/minúsculas y
  no tienen colores ni jerarquía en esta versión.
- La card muestra como máximo dos tags y un indicador `+N`; una card sin tags
  no reserva espacio vacío.
- La selección de una colección filtra el rail. `Historial` muestra todo.
- Los filtros de varios tags usan lógica AND.
- Tags y colecciones se asignan mediante selectores con búsqueda y checkboxes.
- Quick-paste sigue siendo global y no agrega filtros de colección en este
  cambio.

## Fuera de alcance

No incluye jerarquías de carpetas, tags automáticos, colores de tags,
sincronización, red, telemetría, IA, import/export, edición de contenido,
snippets, transformaciones ni Paste Stack.
