# Tareas de implementación

## 1. Relevamiento y diagnóstico

- [x] 1.1 Leer `project.md`, `AGENTS.md`, este cambio y los contratos archivados
  de `clipboard-search`, `card-title-editing-regression`, `desktop-toolbar-layout`
  y `quick-paste` antes de modificar código.
- [x] 1.2 Auditar `LocalSearchEngine`, `SearchService`, repositorio de
  entradas, `clipvault_search_entries`, `App.svelte`, `QuickPaste.svelte`,
  `search.ts` y `tauri.ts` para identificar por qué el título no llega o se
  descarta en cada superficie.
- [x] 1.3 Confirmar con una prueba de regresión mínima si el motor ya soporta
  título pero el servicio sólo lee entradas textuales, y documentar cualquier
  contradicción en `design.md` antes de resolverla.
- [x] 1.4 Revisar `git status --short` y `git diff --check`; no sobrescribir
  cambios pendientes de otros specs.

## 2. Core, búsqueda y scope

- [x] 2.1 Extender el conjunto de candidatos de `SearchService` para incluir
  todos los tipos de entrada actuales, preservando collection, tags, source
  app, límites y orden del repositorio.
- [x] 2.2 Construir `SearchDocument.title` desde el título personalizado
  persistido, tratando `None` y whitespace-only como ausentes.
- [x] 2.3 Mantener `SearchDocument.content` sólo para tipos textuales y evitar
  leer bytes, asset refs o metadata binaria de imágenes durante la búsqueda.
- [x] 2.4 Preservar los niveles y constantes de ranking existentes:
  contenido por encima de título, desempate por recencia e id y fuzzy bounded.
- [x] 2.5 Verificar que title-only image hits devuelven el `EntryRecord` completo
  sin cambiar ni reescribir la persistencia.
- [x] 2.6 Mantener el límite, empty query, no matches, errores y privacidad del
  servicio sin crear índices ni dependencias nuevas.

## 3. Tauri y frontend

- [x] 3.1 Confirmar que `clipvault_search_entries` conserva firma, filtros y
  respuesta; cambiar sólo el wiring necesario para exponer hits de título.
- [x] 3.2 Confirmar que Desktop consume hits del comando común y no restaura
  accidentalmente una lista basada sólo en contenido.
- [x] 3.3 Confirmar que Quick Paste consume hits del mismo comando y que un hit
  title-only conserva título visible, selección, navegación y preview actuales.
- [x] 3.4 Mantener debounce, cancelación, tokens anti-stale y estados de carga,
  error, vacío y sin resultados en ambas barras.
- [x] 3.5 Mantener el alcance de colección activa, tags y filtro de aplicación
  en Desktop y el alcance actual de Historial en Quick Paste.

## 4. Tests de regresión

- [x] 4.1 Agregar test del motor para título exacto, tokens y fuzzy sin título,
  y para título ausente/whitespace-only.
- [x] 4.2 Agregar test de integración del `SearchService` para un título-only
  de texto y otro de imagen cuyo contenido no sea buscable.
- [x] 4.3 Agregar test de ranking mixto: contenido exacto sobre título exacto,
  orden determinista y límites.
- [x] 4.4 Agregar test con collection/tag/source-app scope que confirme que no
  aparecen títulos fuera del alcance activo.
- [x] 4.5 Agregar tests frontend/bridge para Desktop y Quick Paste que
  demuestren que ambos aceptan y renderizan un hit title-only.
- [x] 4.6 Ejecutar regresiones de imágenes después de reinicio, tags,
  colecciones, favoritos, edición de título, source-app filter, Quick Paste y
  drag-and-drop; no cambiar asset refs ni payloads.
- [x] 4.7 Agregar una aserción de privacidad: búsqueda y respuestas no añaden
  logs/eventos con contenido, snippets completos, hashes, bytes, rutas o
  referencias de assets.

## 5. Verificación automática

- [x] 5.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 5.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 5.3 Ejecutar `cargo test --workspace`.
- [x] 5.4 Ejecutar `npm run check` dentro de `app/tauri/frontend`.
- [x] 5.5 Ejecutar `npm run build` dentro de `app/tauri/frontend`.
- [x] 5.6 Ejecutar `npm test` dentro de `app/tauri/frontend`.
- [x] 5.7 Ejecutar `openspec validate search-title-matching --strict --type
  change`.
- [x] 5.8 Revisar el diff y confirmar que no se agregaron red, telemetría,
  dependencias innecesarias, secretos, archivos generados ni acceso a assets
  desde el buscador.

## 6. Verificación manual

- [x] 6.1 En una build actual y sin instancias anteriores abiertas, buscar en
  Desktop el título personalizado de una captura de texto cuyo contenido no
  lo contiene. Verificado manualmente.
- [x] 6.2 Repetir la búsqueda title-only en Quick Paste y confirmar que el item
  aparece, puede seleccionarse y mantiene su preview. Verificado manualmente.
- [x] 6.3 Crear una imagen con título personalizado y confirmar que aparece por
  título en Historial y conserva su miniatura completa. Verificado manualmente.
- [x] 6.4 Repetir en una colección, verificando que el scope activo, tags y
  filtro de aplicación siguen siendo respetados. Verificado manualmente.
- [x] 6.5 Confirmar búsqueda por contenido, limpiar consulta, no matches y
  navegación sin regresiones. Verificado manualmente.
- [x] 6.6 Reiniciar ClipVault y confirmar persistencia de títulos, imágenes,
  tags y favoritos. Verificado manualmente.

La validación manual de `platform-permission-guidance` permanece separada y no
debe marcarse como completada por este cambio. Este cambio tampoco debe
archivarse automáticamente.
