# Prompt para MiniMax: panel unificado del desktop y lista de colecciones acotada

Implementa únicamente las tareas pendientes `9.1` a `9.6` del cambio
OpenSpec `desktop-toolbar-layout`.

## Contexto

La referencia visual tiene tres zonas que deben pertenecer a una única
superficie del desktop:

1. la lista/panel de colecciones;
2. la fila de búsqueda y acciones;
3. la fila o rail de historial.

Actualmente la zona 1 queda visualmente separada de las zonas 2 y 3. Además,
la lista de colecciones debe mantenerse acotada: al crear muchas colecciones
no puede crecer la ventana ni el body; debe aparecer scroll vertical sólo en
la lista.

## Lectura obligatoria

Antes de editar, lee `project.md`, `AGENTS.md`, todos los artefactos de
`openspec/changes/desktop-toolbar-layout/` y las specs base de
`desktop-shell-layout`, `tags-and-collections`, `clipboard-history-cards` y
`desktop-header-card-dnd`.

Inspecciona como mínimo:

- `app/tauri/frontend/src/App.svelte`;
- `app/tauri/frontend/src/OrganizationSidebar.svelte`;
- `app/tauri/frontend/src/DesktopToolbar.svelte`;
- `app/tauri/frontend/src/HistoryCardRail.svelte`;
- `app/tauri/frontend/tests/desktopToolbarLayout.test.ts`;
- `app/tauri/frontend/tests/collectionPanelHeightRegression.test.ts`.

## Resultado requerido

- Debe existir un único contenedor/panel de workspace que contenga las tres
  zonas. El panel común debe poseer la superficie exterior (fondo, borde, radio
  y padding) y ninguna zona puede quedar como hermano visual externo.
- La jerarquía interna puede seguir usando una grilla de dos columnas y una
  columna de contenido, siempre que la lista de colecciones, la fila de
  búsqueda/acciones y el rail de historial desciendan del mismo workspace.
- La altura visible debe tener una única fuente. El panel no puede crecer por
  la cantidad de colecciones.
- El encabezado y el control de nueva colección deben permanecer visibles.
  Sólo el viewport de la lista debe usar `overflow-y: auto`, con la
  combinación estructural necesaria de `flex: 1` y `min-height: 0`.
- No debe aparecer scroll vertical en el body ni un segundo scrollbar del
  workspace. El rail de cards conserva su scroll horizontal independiente.
- Deben conservarse selección, creación, renombrado, borrado, drop sobre filas
  scrolleadas y el comportamiento visual existente.

## Límites

No cambies backend, SQLite, comandos, búsqueda, ordenamiento, assets,
thumbnail loading, contenido de cards, toolbar actions ni contratos de
drag-and-drop salvo lo estrictamente necesario para que el layout conserve sus
destinos scrolleables. El payload de drag debe seguir transportando únicamente
el identificador opaco de la entrada. No agregues dependencias, red,
telemetría ni recursos externos.

El controlador singleton `lib/pointerDragAndDrop.ts`, el fallback de mouse,
pointer capture, ghost, selection lock, cancelación y exclusión de controles
son baseline protegido. No lo reemplaces ni lo simplifiques.

## Verificación

Agrega sólo tests representativos para demostrar:

- las tres zonas bajo un único workspace/panel;
- altura compartida y ausencia de crecimiento del body;
- scroll vertical únicamente en la lista de colecciones;
- header/control de nueva colección visibles;
- drop sobre una fila scrolleada y payload sólo con entry id.

Ejecuta, según corresponda:

```bash
cargo fmt --all -- --check
npm --prefix app/tauri/frontend run check
npm --prefix app/tauri/frontend test
npm --prefix app/tauri/frontend run build
openspec validate desktop-toolbar-layout --strict --type change
git diff --check
```

Como el cambio toca layout, colecciones y listeners/hit-testing, ejecuta
también las regresiones frontend de drag-and-drop exigidas por `AGENTS.md`.
Si una regresión protegida falla, detén la entrega y corrígela antes de
continuar. No marques pruebas manuales como completadas sin evidencia. No
archives, commitees ni hagas push: esas acciones requieren una instrucción
posterior explícita.
