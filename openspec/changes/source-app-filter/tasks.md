# Tareas de implementación: source-app-filter

## 1. Relevamiento y decisiones

- [ ] 1.1 Leer `project.md`, `AGENTS.md`, este cambio, los cambios activos de
  layout/search y las specs de `clipboard-search`, `desktop-shell-layout` y
  `clipboard-history-cards`.
- [ ] 1.2 Inspeccionar `App.svelte`, `DesktopToolbar.svelte`,
  `HistoryCardRail.svelte`, `HistoryCard.svelte`, `lib/search.ts`,
  `lib/tauri.ts`, `types.ts` y los comandos Rust de recents/search.
- [ ] 1.3 Confirmar el contrato actual de Historial (`selectedCollectionId`),
  filtros de tags, tokens de búsqueda y evento `history-updated`.
- [ ] 1.4 Revisar el baseline de imágenes y drag-and-drop en `AGENTS.md` y
  confirmar que no se usarán `~/.clipvault` ni sus assets en tests/builds.

## 2. Core, DB y comandos delgados

- [ ] 2.1 Implementar el modelo explícito `All/Known/Unknown` en el core o
  equivalente serializable, distinguiendo `Todas` de origen desconocido.
- [ ] 2.2 Implementar la consulta local de aplicaciones distintas para el
  alcance de Historial o colección, sin limitarla al rail visible.
- [ ] 2.3 Resolver metadata determinística de nombre/icono y fallback seguro,
  agrupando por `source_app` sin duplicados.
- [ ] 2.4 Extender de forma aditiva los filtros de recents y search para
  aceptar source application sin cambiar ranking, límites ni tags.
- [ ] 2.5 Agregar el comando Tauri de opciones de aplicaciones y extender los
  comandos existentes delegando al core; no duplicar lógica en el shell.
- [ ] 2.6 Cubrir Known, Unknown, All, Historial, colección, tags, query,
  límites, orden determinista, metadata-only y aislamiento local.

## 3. Contrato frontend y bridge

- [ ] 3.1 Agregar tipos para el filtro y las opciones, sin exponer el
  identificador estable como texto visible.
- [ ] 3.2 Extender `recentEntriesFilteredCommand` y `searchEntriesCommand` con
  el mismo filtro de aplicación.
- [ ] 3.3 Agregar el wrapper del comando de opciones de aplicaciones.
- [ ] 3.4 Reutilizar el resolver/cache local de iconos con cleanup de Blob URLs
  y fallback genérico.

## 4. Combobox y composición de la UI

- [ ] 4.1 Crear o integrar un único combobox entre búsqueda y menú de
  configuración en `DesktopToolbar`.
- [ ] 4.2 Renderizar `Todas` primero y las opciones conocidas/desconocidas con
  icono, nombre y labels accesibles.
- [ ] 4.3 Implementar apertura, cierre, click fuera, Escape, navegación por
  teclado, foco visible y selección inmediata.
- [ ] 4.4 Mantener estado separado para colección, query y aplicación; combinar
  los tres en recents y search.
- [ ] 4.5 Resetear a `Todas` y recargar opciones al cambiar de colección.
- [ ] 4.6 Refrescar opciones después de `history-updated` sin listeners
  duplicados y descartar respuestas obsoletas.

## 5. No-regresiones

- [ ] 5.1 Verificar búsqueda existente, ranking, fuzzy matching, tags,
  colección activa y quick-paste.
- [ ] 5.2 Verificar imágenes antiguas tras reinicio, cambio de colección,
  búsqueda, aplicación, tags y favoritos.
- [ ] 5.3 Verificar que pin, títulos, Delete, Quitar de esta colección,
  retención y paste no cambian.
- [ ] 5.4 Ejecutar regresiones completas de drag-and-drop: pointer capture,
  mouse fallback, ghost, selección, Escape/blur/pointercancel y drop en lista
  scrolleable.
- [ ] 5.5 Verificar que no hay contenido de clipboard, hashes, bytes, rutas ni
  identificadores crudos en logs, errores, eventos o UI.

## 6. Tests

- [ ] 6.1 Tests unitarios/core de agrupación, Unknown, orden, metadata y
  predicado de filtro.
- [ ] 6.2 Tests de integración Tauri para opciones y recents/search filtrados.
- [ ] 6.3 Tests frontend del combobox, iconos, keyboard, lifecycle y fallback.
- [ ] 6.4 Tests de combinación con búsqueda, colección, tags y actualización
  por captura.
- [ ] 6.5 Tests de respuestas obsoletas, no duplicación de listeners y
  preservación de imágenes/drag-and-drop.

## 7. Verificación automatizada

- [ ] 7.1 Ejecutar `cargo fmt --all -- --check`.
- [ ] 7.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] 7.3 Ejecutar `cargo test --workspace`.
- [ ] 7.4 Ejecutar `cd app/tauri/frontend && npm run check`.
- [ ] 7.5 Ejecutar `cd app/tauri/frontend && npm run build`.
- [ ] 7.6 Ejecutar `cd app/tauri/frontend && npm test`.
- [ ] 7.7 Ejecutar `openspec validate source-app-filter --strict --type change`.
- [ ] 7.8 Revisar diff, archivos generados, dependencias, red y privacidad.

## 8. Verificación manual

- [ ] 8.1 En Historial, confirmar que el combobox está entre búsqueda y
  configuración y que `Todas` aparece primero con su icono.
- [ ] 8.2 Confirmar que cada aplicación muestra icono y nombre y que no se
  muestra el bundle identifier.
- [ ] 8.3 Seleccionar varias aplicaciones y `Todas`, comprobando que el rail
  cambia inmediatamente sin botón Aplicar.
- [ ] 8.4 Repetir dentro de una colección y confirmar que sólo aparecen las
  aplicaciones de esa colección.
- [ ] 8.5 Combinar el filtro con una búsqueda de texto y cambiar de colección;
  confirmar reset a `Todas` y ausencia de resultados obsoletos.
- [ ] 8.6 Confirmar navegación completa con teclado y cierre con Escape/click
  fuera.
- [ ] 8.7 Reiniciar con cards de texto e imágenes y comprobar que imágenes,
  tags, favoritos y drag-and-drop siguen funcionando.

La verificación manual pendiente de `platform-permission-guidance` es
independiente y no debe marcarse como completada por este cambio.
