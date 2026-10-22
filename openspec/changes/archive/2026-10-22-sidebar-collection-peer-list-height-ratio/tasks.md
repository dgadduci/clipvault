## Spec and design

- [x] 1.1 Document that the 70/30 percentages are relative to a shared
  container holding the two list regions.
- [x] 1.2 Keep the collection heading and creation controls outside that ratio;
  require independent scrolling and a bounded workspace.

## Implementation

- [x] 2.1 Add a shared sidebar container for the collection-list viewport and
  linked-peers section.
- [x] 2.2 Allocate the container's height with 7fr/3fr tracks and preserve
  independent scrolling within each list.
- [x] 2.3 Add a focused layout regression for the shared container and ratio;
  preserve existing collection drag-and-drop behavior.

## Verification

- [x] 3.1 Run focused layout tests and protected drag-and-drop regressions.
- [x] 3.2 Run frontend checks/build, strict OpenSpec validation and
  `git diff --check`.
- [x] 3.3 Record the user's manual approval of the 70/30 sidebar allocation.

## Resultado de verificación

- `npm run check`: 0 errores y 18 advertencias existentes.
- `npm run build`: completado; validación de los cinco catálogos con 873 claves.
- Regresiones de layout y drag and drop: 7 archivos, 7 aprobados.
- `openspec validate sidebar-collection-peer-list-height-ratio --strict` y
  `git diff --check`: aprobados.
- Prueba manual aprobada por el usuario: proporción 70/30 de la lista de
  colecciones y los equipos vinculados.
