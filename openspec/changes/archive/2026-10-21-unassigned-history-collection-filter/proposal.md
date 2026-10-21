# Proposal: unassigned-history-collection-filter

## Why

El filtro de colecciones de Historial permite buscar capturas pertenecientes a
una colección, pero no identificar las capturas que todavía no se organizaron
en ninguna colección de usuario. Además, el filtro desaparece al navegar a otra
colección y deja un espacio visual en la barra, aunque sus controles vecinos
siguen visibles.

## What Changes

- Añadir al filtro de Historial la opción `Sin colección`, que incluye capturas
  sin membresía en colecciones de usuario. La pertenencia implícita a
  `Historial` no cuenta como una colección de usuario.
- Mantener el filtro de colecciones en la barra del Desktop fuera de
  `Historial`, con el control deshabilitado mientras esté activa otra
  colección.
- Mantener la selección del filtro limitada a Historial y sin alterar las
  membresías persistidas de las capturas.
- Agregar las etiquetas nuevas a los catálogos `en`, `es`, `pt`, `de` y `fr`.

## Capabilities

### New Capabilities

### Modified Capabilities

- `desktop-filtering`: filtrar capturas sin colección de usuario y conservar el
  filtro visible, deshabilitado, fuera de Historial.

## Impact

- Extiende el filtro de colecciones del Desktop, sus rutas de consultas de
  capturas recientes y búsqueda, y la consulta de tipos de captura disponibles.
- Agrega un predicado metadata-only en SQLite para excluir capturas asociadas a
  colecciones de usuario; no requiere migración ni modifica membresías.
- No altera el alcance de la colección activa ni la búsqueda global de
  QuickPaste.
