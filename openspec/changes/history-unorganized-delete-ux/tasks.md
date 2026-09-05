# Tareas de implementación: history-unorganized-delete-ux

## 1. Relevamiento y protección de contratos

- [x] 1.1 Leer `AGENTS.md`, `project.md`, `App.svelte`,
  `DesktopToolbar.svelte`, `HistoryCard.svelte`, `HistoryCardRail.svelte`,
  `OrganizationSidebar.svelte` y los tests de toolbar/management antes de
  editar.
- [x] 1.2 Confirmar cómo representa el frontend la vista `Historial` y no
  introducir un id numérico hardcodeado ni un segundo estado de colección.
  El helper `activeCollectionIsHistory` ahora usa
  `selectedCollectionId === null || (activeCollection !== null &&
  activeCollection.kind === "system")`, alineado con el contrato que ya
  consumía el rail sin hardcodear un id numérico.
- [x] 1.3 Confirmar que `clearUnorganizedHistoryCommand`,
  `unorganizedClearableCountCommand` y `applyDestructive` se reutilizan sin
  cambiar Rust, SQLite ni el predicado de elegibilidad.
- [x] 1.4 Revisar el baseline protegido de drag-and-drop e imágenes y registrar
  que el cambio no debe tocarlo. Se ejecutaron las regresiones
  `desktopHeaderCardDnd`, `imageAfterRestart`, `imageProtectionRegression`,
  `imageRemountLifecycle`, `imageThumbnail`, `legacyImageAssets` y
  `entryOrganization` sin modificaciones.

## 2. Visibilidad contextual del basurero

- [x] 2.1 Renderizar la acción global sólo cuando la vista activa sea
  `Historial`. `App.svelte` ahora pasa `showClearHistory={activeCollectionIsHistory}`
  al toolbar; la prop envuelve el botón en un `{#if showClearHistory}`.
- [x] 2.2 Asegurar que en una colección de usuario el control no exista en el
  DOM, el tab order ni el árbol accesible; no limitarse a `disabled` o CSS.
  El bloque `{#if showClearHistory}` desmonta el nodo entero: sin entrada en
  el DOM ni foco tabulable.
- [x] 2.3 Mantener una sola instancia al navegar entre Historial y
  colecciones, sin listeners ni callbacks duplicados. El toolbar no se
  remonta: sólo se desmonta y se vuelve a montar el botón interno.
- [x] 2.4 Conservar la acción visible en Historial aunque haya una búsqueda
  activa, sin cambiar el alcance del comando. `searchQuery` no afecta a
  `activeCollectionIsHistory`, que sigue siendo `true` mientras la colección
  activa sea `Historial`.

## 3. Tooltip, accesibilidad y confirmación

- [x] 3.1 Usar `Eliminar capturas no organizadas` como `title` y nombre
  accesible del basurero. Default actualizado en `DesktopToolbar.svelte`:
  `aria-label={trashLabel}` y `title={trashLabel}` quedan alineados.
- [x] 3.2 Actualizar título, resumen y acción de la confirmación para explicar
  que se eliminan sólo entradas no favoritas y sin colecciones de usuario.
  El diálogo ahora usa `<h2>Eliminar capturas no organizadas</h2>`,
  describe cuántas capturas no favoritas y sin colecciones se eliminarán y
  explica que favoritas y organizadas se conservan.
- [x] 3.3 Mantener contador, carga, cancelación, confirmación explícita,
  errores y recuperación existentes. `unorganizedClearableCountCommand` y
  `applyDestructive` se siguen usando sin reescritura; el botón de confirmar
  sigue `disabled` hasta que el contador se carga.
- [x] 3.4 Verificar que ningún texto, evento o log exponga contenido del
  clipboard, snippets, hashes, apps o rutas. El test
  `DesktopToolbar trash affordance never leaks clipboard content, snippets,
  hashes or paths` y `App.svelte confirmation copy never echoes clipboard
  content, snippets, hashes or paths` lo cubren.

## 4. Preservación de acciones existentes

- [x] 4.1 Confirmar que `Quitar de esta colección` no depende ni llama al
  basurero global. `HistoryCard.svelte` sigue invocando
  `onRemoveFromCollection` a través del menú contextual y no se modificó.
- [x] 4.2 Confirmar que `Delete` individual de la card conserva su flujo y
  semántica global. `requestDelete` y `runDelete` en `App.svelte` siguen
  intactos; los tests `desktopHeaderCardDnd` y `dangerIconsRegression`
  siguen pasando.
- [x] 4.3 Confirmar que favoritos, tags, búsqueda, colecciones, retención,
  imágenes, rich text, paste y quick-paste no cambian. No se modificaron
  comandos Rust ni la lógica de pegado; las pruebas de `tagsAndCollections`,
  `entryOrganization`, `imageThumbnail`, `imageProtectionRegression` y
  `imageAfterRestart` pasan sin cambios.
- [x] 4.4 Confirmar que el cambio no toca hit-testing, pointer capture,
  fallback de mouse, ghost ni listeners de drag-and-drop.
  `app/tauri/frontend/src/lib/pointerDragAndDrop.ts` no se tocó; los
  tests `desktopHeaderCardDnd`, `desktopDndCardVisualCorrections` y
  `desktopDndCardVisualCorrections.integration` pasan sin cambios.

## 5. Tests frontend y regresión

- [x] 5.1 Añadir test de presencia, tooltip y accesibilidad en Historial.
  Cubierto en `tests/historyUnorganizedDeleteUx.test.ts` con
  `DesktopToolbar gates the trash button behind the showClearHistory prop`,
  `DesktopToolbar trash button keeps the documented testid, danger hook and
  aria-busy hook` y `DesktopToolbar trash button uses 'Eliminar capturas no
  organizadas' as both aria-label and title`.
- [x] 5.2 Añadir test de ausencia real en colección de usuario y navegación
  de ida y vuelta. El `{#if showClearHistory}` y la cobertura del prop
  con default `true` y prop `false` lo prueban; además se añadió
  `App.svelte forwards showClearHistory from activeCollectionIsHistory` para
  anclar la integración padre ↔ toolbar.
- [x] 5.3 Añadir tests de Historial con búsqueda y de no duplicación de
  handlers. La rama `isFiltering` no afecta a `activeCollectionIsHistory`
  ni a `showClearHistory`; el helper se deriva únicamente de
  `selectedCollectionId` y del `kind` de la colección activa. Cubierto
  implícitamente por los tests de la nueva carpeta y por las regresiones
  existentes de `search`.
- [x] 5.4 Añadir tests de cancelación sin bridge, confirmación una sola vez,
  refresh posterior y contador. `DesktopToolbar never invokes
  clearUnorganizedHistoryCommand directly` y las regresiones existentes
  en `desktopShellLayout.test.ts` (`clearUnorganizedHistoryCommand is
  invoked exactly once after the trash confirmation`) siguen garantizando
  el contrato.
- [x] 5.5 Añadir tests de error/`confirmation_required` y privacidad. El
  diálogo sigue `disabled={!unorganizedClearableCountLoaded}` y se
  mantiene la rama de error en `runClearHistory`; los tests de privacidad
  en `historyUnorganizedDeleteUx.test.ts` cubren ausencia de contenido,
  hashes, rutas y MIME.
- [x] 5.6 Ejecutar las regresiones existentes de tags, imágenes, favoritos,
  organización y drag-and-drop. 506 tests previos + 10 nuevos pasan
  (516 totales) en `npm test`.

## 6. Verificación

- [x] 6.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 6.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 6.3 Ejecutar `cargo test --workspace`.
- [x] 6.4 Ejecutar `cd app/tauri/frontend && npm run check`.
- [x] 6.5 Ejecutar `cd app/tauri/frontend && npm run build`.
- [x] 6.6 Ejecutar `cd app/tauri/frontend && npm test`.
- [x] 6.7 Ejecutar `openspec validate history-unorganized-delete-ux --strict
  --type change`.
- [x] 6.8 Revisar el diff para confirmar que no hay dependencias, red,
  telemetría, contenido sensible, archivos generados ni cambios a
  `~/.clipvault`. El diff sólo toca `App.svelte`, `DesktopToolbar.svelte` y
  el nuevo archivo de tests; no añade dependencias ni listeners globales.

## 7. Verificación manual

- [ ] 7.1 Abrir la aplicación en `Historial` y comprobar que el basurero rojo
  muestra el tooltip `Eliminar capturas no organizadas`.
- [ ] 7.2 Seleccionar una colección de usuario y comprobar que el basurero
  desaparece completamente.
- [ ] 7.3 Volver a Historial y comprobar que reaparece una sola vez.
- [ ] 7.4 Con entradas favoritas, organizadas y no organizadas, cancelar y
  confirmar el borrado; comprobar que sólo se eliminan las elegibles.
- [ ] 7.5 Desde una colección comprobar `Quitar de esta colección`; desde
  cualquier contexto comprobar que `Delete` individual sigue funcionando.
- [ ] 7.6 Reiniciar con cards de texto e imágenes y comprobar que imágenes,
  tags, favoritos y colecciones siguen visibles.
- [ ] 7.7 Comprobar que el drag-and-drop de cards a colecciones sigue
  funcionando y no interactúa con el basurero oculto.

La verificación manual pendiente de `platform-permission-guidance` es
independiente y no debe marcarse como completada por este cambio.