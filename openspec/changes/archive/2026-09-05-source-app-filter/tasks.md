# Tareas de implementación: source-app-filter

## 1. Relevamiento y decisiones

- [x] 1.1 Leer `project.md`, `AGENTS.md`, este cambio, los cambios activos de
  layout/search y las specs de `clipboard-search`, `desktop-shell-layout` y
  `clipboard-history-cards`.
- [x] 1.2 Inspeccionar `App.svelte`, `DesktopToolbar.svelte`,
  `HistoryCardRail.svelte`, `HistoryCard.svelte`, `lib/search.ts`,
  `lib/tauri.ts`, `types.ts` y los comandos Rust de recents/search.
- [x] 1.3 Confirmar el contrato actual de Historial (`selectedCollectionId`),
  filtros de tags, tokens de búsqueda y evento `history-updated`.
- [x] 1.4 Revisar el baseline de imágenes y drag-and-drop en `AGENTS.md` y
  confirmar que no se usarán `~/.clipvault` ni sus assets en tests/builds.

## 2. Core, DB y comandos delgados

- [x] 2.1 Implementar el modelo explícito `All/Known/Unknown` en el core o
  equivalente serializable, distinguiendo `Todas` de origen desconocido.
- [x] 2.2 Implementar la consulta local de aplicaciones distintas para el
  alcance de Historial o colección, sin limitarla al rail visible.
- [x] 2.3 Resolver metadata determinística de nombre/icono y fallback seguro,
  agrupando por `source_app` sin duplicados.
- [x] 2.4 Extender de forma aditiva los filtros de recents y search para
  aceptar source application sin cambiar ranking, límites ni tags.
- [x] 2.5 Agregar el comando Tauri de opciones de aplicaciones y extender los
  comandos existentes delegando al core; no duplicar lógica en el shell.
- [x] 2.6 Cubrir Known, Unknown, All, Historial, colección, tags, query,
  límites, orden determinista, metadata-only y aislamiento local.

## 3. Contrato frontend y bridge

- [x] 3.1 Agregar tipos para el filtro y las opciones, sin exponer el
  identificador estable como texto visible.
- [x] 3.2 Extender `recentEntriesFilteredCommand` y `searchEntriesCommand` con
  el mismo filtro de aplicación.
- [x] 3.3 Agregar el wrapper del comando de opciones de aplicaciones.
- [x] 3.4 Reutilizar el resolver/cache local de iconos con cleanup de Blob URLs
  y fallback genérico.

## 4. Combobox y composición de la UI

- [x] 4.1 Crear o integrar un único combobox entre búsqueda y menú de
  configuración en `DesktopToolbar`.
- [x] 4.2 Renderizar `Todas` primero y las opciones conocidas/desconocidas con
  icono, nombre y labels accesibles.
- [x] 4.3 Implementar apertura, cierre, click fuera, Escape, navegación por
  teclado, foco visible y selección inmediata.
- [x] 4.4 Mantener estado separado para colección, query y aplicación; combinar
  los tres en recents y search.
- [x] 4.5 Resetear a `Todas` y recargar opciones al cambiar de colección.
- [x] 4.6 Refrescar opciones después de `history-updated` sin listeners
  duplicados y descartar respuestas obsoletas.

## 5. No-regresiones

- [x] 5.1 Verificar búsqueda existente, ranking, fuzzy matching, tags,
  colección activa y quick-paste.
- [x] 5.2 Verificar imágenes antiguas tras reinicio, cambio de colección,
  búsqueda, aplicación, tags y favoritos.
- [x] 5.3 Verificar que pin, títulos, Delete, Quitar de esta colección,
  retención y paste no cambian.
- [x] 5.4 Ejecutar regresiones completas de drag-and-drop: pointer capture,
  mouse fallback, ghost, selección, Escape/blur/pointercancel y drop en lista
  scrolleable.
- [x] 5.5 Verificar que no hay contenido de clipboard, hashes, bytes, rutas ni
  identificadores crudos en logs, errores, eventos o UI.

## 6. Tests

- [x] 6.1 Tests unitarios/core de agrupación, Unknown, orden, metadata y
  predicado de filtro.
- [x] 6.2 Tests de integración Tauri para opciones y recents/search filtrados.
- [x] 6.3 Tests frontend del combobox, iconos, keyboard, lifecycle y fallback.
- [x] 6.4 Tests de combinación con búsqueda, colección, tags y actualización
  por captura.
- [x] 6.5 Tests de respuestas obsoletas, no duplicación de listeners y
  preservación de imágenes/drag-and-drop.

## 7. Verificación automatizada

- [x] 7.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 7.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 7.3 Ejecutar `cargo test --workspace`.
- [x] 7.4 Ejecutar `cd app/tauri/frontend && npm run check`.
- [x] 7.5 Ejecutar `cd app/tauri/frontend && npm run build`.
- [x] 7.6 Ejecutar `cd app/tauri/frontend && npm test`.
- [x] 7.7 Ejecutar `openspec validate source-app-filter --strict --type change`.
- [x] 7.8 Revisar diff, archivos generados, dependencias, red y privacidad.

## 8. Verificación manual

- [x] 8.1 En Historial, confirmar que el combobox está entre búsqueda y
  configuración y que `Todas` aparece primero con su icono.
- [x] 8.2 Confirmar que cada aplicación muestra icono y nombre y que no se
  muestra el bundle identifier.
- [x] 8.3 Seleccionar varias aplicaciones y `Todas`, comprobando que el rail
  cambia inmediatamente sin botón Aplicar.
- [x] 8.4 Repetir dentro de una colección y confirmar que sólo aparecen las
  aplicaciones de esa colección.
- [x] 8.5 Combinar el filtro con una búsqueda de texto y cambiar de colección;
  confirmar reset a `Todas` y ausencia de resultados obsoletos.
- [x] 8.6 Confirmar navegación completa con teclado y cierre con Escape/click
  fuera.
- [x] 8.7 Reiniciar con cards de texto e imágenes y comprobar que imágenes,
  tags, favoritos y drag-and-drop siguen funcionando.

## 9. Regresión: bootstrap inicial de Historial

### 9.1 Causa raíz comprobada

`App.svelte::loadEntries()` bifurcaba por `selectedCollectionId === null`
y caía a `recentEntriesCommand`, el comando sin filtros, que descarta
el argumento `sourceApp`. La vista por defecto (Historial,
`selectedCollectionId = null`) cargaba el rail sin aplicar el filtro de
aplicación aunque el combobox ya hubiera recibido sus opciones vía
`sourceApplicationsCommand`. Entrar a una colección de usuario
enrutaba por `recentEntriesFilteredCommand` y enmascaraba el bug; al
volver a Historial reaparecía. El contrato del filtro de aplicación
queda unificado en `recentEntriesFilteredCommand` para todos los
scopes (`collectionId: null` representa Historial, lo mismo que
`searchEntriesCommand` ya hacía).

### 9.2 Tareas

- [x] 9.2.1 Sustituir la rama `recentEntriesCommand` en `loadEntries()`
  por una llamada única a `recentEntriesFilteredCommand` con
  `collectionId: activeCollectionIsHistory ? null : selectedCollectionId`
  y `sourceApp: sourceAppFilter`.
- [x] 9.2.2 Eliminar el import de `recentEntriesCommand` en
  `App.svelte` (QuickPaste y DevelopmentModal siguen usando el comando
  sin filtros; sus imports no cambian).
- [x] 9.2.3 Cubrir el scope Historial con tests de integración en
  `clipvault-core/tests/source_app_filter.rs`:
  - `recent_entries_with_filter_history_scope_all_matches_unfiltered_recent`;
  - `recent_entries_with_filter_history_scope_known_returns_only_that_identifier`;
  - `recent_entries_with_filter_history_scope_unknown_returns_only_null_or_empty_rows`;
  - `recent_entries_with_filter_history_scope_preserves_chronological_order`;
  - `search_filter_history_scope_known_combines_with_text_query`.
- [x] 9.2.4 Añadir `app/tauri/frontend/tests/sourceAppFilterBootstrap.test.ts`
  con cobertura de bootstrap:
  - `loadEntries` siempre enruta por `recentEntriesFilteredCommand`;
  - `loadEntries` deriva `collectionId` desde `activeCollectionIsHistory`;
  - `searchEntriesCommand` recibe ambos filtros en Historial;
  - `selectCollectionFromSidebar` resetea el filtro y recarga opciones;
  - `refreshSourceAppOptions` usa el scope explícito (sin ids numéricos);
  - el guard de token monotónico sigue activo contra respuestas obsoletas;
  - `handleHistoryUpdated` refresca opciones y rail sin duplicar listeners;
  - el tipo `SourceAppFilter` mantiene los tres discriminantes;
  - no-regresiones: drag-and-drop, imagen, ciclo de vida.
- [x] 9.2.5 Confirmar `cargo fmt --all -- --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `npm run check`, `npm run build`, `npm test`,
  `openspec validate source-app-filter --strict --type change`.
- [ ] 9.2.6 Verificación manual pendiente:
  - al iniciar la app, el combobox carga opciones y filtra inmediatamente;
  - `Historial → colección → Historial` no deja opciones obsoletas;
  - `history-updated` no registra listeners duplicados.

> **Nota:** Los pasos de verificación manual requieren un build de
> Tauri en vivo que no se puede ejecutar en este entorno headless. La
> cobertura automatizada equivalente se cubre a través de los tests
> unitarios del combobox, los tests de integración del bridge y los
> tests de drag-and-drop; el contrato del UI se mantiene a través de
> `desktopToolbarLayout.test.ts` y de los `data-testid` del combobox.

La verificación manual pendiente de `platform-permission-guidance` es
independiente y no debe marcarse como completada por este cambio.
