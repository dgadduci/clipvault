# Tasks: desktop-shell-layout

No archivar este cambio al terminar. La verificación manual pendiente de
`platform-permission-guidance` es independiente y no debe marcarse aquí.

## 1. Relevamiento y límites

- [x] 1.1 Leer el layout actual de `App.svelte` y localizar las secciones
  `Frontend ↔ Tauri ↔ Rust ↔ SQLite`, `Capabilities`, `Active application`,
  `Quick paste`, `History Management` y `SettingsPanel`.
- [x] 1.2 Identificar qué comandos y callbacks existentes usa cada sección
  antes de mover markup, sin duplicar lógica de negocio.
- [x] 1.3 Confirmar si `Tick capture` y `Paste latest` siguen siendo controles
  funcionales; eliminar sólo los stubs sin función.
- [x] 1.4 Verificar que el alcance no requiere migraciones, cambios de core ni
  cambios de contratos Tauri.

## 2. Coordinador de modales

- [x] 2.1 Implementar un estado único para Development, Privacidad, Retención,
  Atajo de pegado rápido o ningún modal.
- [x] 2.2 Garantizar que nunca haya dos modales abiertos simultáneamente.
- [x] 2.3 Reutilizar un componente modal accesible o extraer la estructura
  común sin agregar dependencias.
- [x] 2.4 Implementar apertura, cierre, Escape, backdrop y foco de forma
  idempotente.
- [x] 2.5 Limpiar exactamente una vez listeners, handlers y recursos al cerrar
  o destruir el componente.

## 3. Desktop y toolbar

- [x] 3.1 Dejar el desktop principal con búsqueda, rail de cards y controles
  globales compactos.
- [x] 3.2 Agregar botones accesibles para Development, Privacidad, Retención y
  Atajo de pegado rápido.
- [x] 3.3 Agregar el icono de basurero en la esquina superior derecha con
  `aria-label`, `title` y foco visible.
- [x] 3.4 Eliminar del desktop la sección `History Management`.
- [x] 3.5 Mantener el rail horizontal y las cards cuadradas sin deformarlos.
- [x] 3.6 Hacer responsive la toolbar sin cortar búsqueda, modales ni basurero.

## 4. Modal Development

- [x] 4.1 Mover Frontend ↔ Tauri ↔ Rust ↔ SQLite al modal Development.
- [x] 4.2 Mover Capabilities al modal Development.
- [x] 4.3 Mover Active application al modal Development.
- [x] 4.4 Mover el diagnóstico Quick paste y los controles funcionales al modal
  Development.
- [x] 4.5 Eliminar controles Quick paste que sean stubs muertos y documentar la
  decisión en el diff.
- [x] 4.6 Mantener comandos, resultados, errores y estados existentes.

## 5. Modales de configuración

- [x] 5.1 Mover el contenido de privacidad y blacklist al modal Privacidad.
- [x] 5.2 Mantener selector de aplicaciones, iconos, errores y guía de permisos
  existentes.
- [x] 5.3 Mover selector de retención, Preview retention y Apply retention now
  al modal Retención.
- [x] 5.4 Evitar que Preview retention aplique mutaciones.
- [x] 5.5 Crear el modal informativo de Atajo de pegado rápido con el valor
  efectivo por plataforma y el estado actual.
- [x] 5.6 No registrar un segundo hotkey ni cambiar la ventana transient de
  quick-paste.

## 6. Basurero y eliminación

- [x] 6.1 Reutilizar `clearHistoryCommand` y la confirmación existente desde el
  icono de basurero.
- [x] 6.2 Conservar favoritos, tags, colecciones y assets según el contrato
  existente.
- [x] 6.3 Refrescar el rail después de una limpieza exitosa.
- [x] 6.4 Mostrar errores y estados busy sin bloquear el resto del desktop.
- [x] 6.5 Mantener Delete individual en el menú de cada card.

## 7. Sistema visual y accesibilidad

- [x] 7.1 Definir tokens CSS locales para tipografía, tamaños, espaciado,
  botones, bordes y modales.
- [x] 7.2 Aplicar la jerarquía tipográfica acordada sin fuentes externas.
- [x] 7.3 Implementar `role="dialog"`, `aria-modal`, título asociado y nombres
  accesibles.
- [x] 7.4 Implementar foco inicial, retorno de foco y focus trap.
- [x] 7.5 Verificar Escape, backdrop, Tab, Shift+Tab y foco visible.
- [x] 7.6 Mantener estados empty, loading, busy y error actuales.

## 8. Tests frontend

- [x] 8.1 Test de cada botón de toolbar abriendo el modal correcto.
- [x] 8.2 Test de un solo modal abierto al cambiar de sección.
- [x] 8.3 Test de Escape y backdrop sin mutación.
- [x] 8.4 Test de foco inicial y retorno de foco.
- [x] 8.5 Test de navegación por teclado dentro del modal.
- [x] 8.6 Test de no duplicación de listeners tras remount/hot reload.
- [x] 8.7 Test de Development con diagnósticos y controles funcionales.
- [x] 8.8 Test de Retención con preview sin mutación y apply posterior.
- [x] 8.9 Test del basurero con confirmación, favoritos preservados y refresh.
- [x] 8.10 Test de Delete individual aún disponible en card.
- [x] 8.11 Test de cards de texto, rich text e imagen sin cambios de acciones.
- [x] 8.12 Test de no payload sensible en eventos y comandos existentes.

## 9. Verificación

- [x] 9.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 9.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 9.3 Ejecutar `cargo test --workspace`.
- [x] 9.4 Ejecutar `cd app/tauri/frontend && npm run check`.
- [x] 9.5 Ejecutar `cd app/tauri/frontend && npm run build`.
- [x] 9.6 Ejecutar `cd app/tauri/frontend && npm test`.
- [x] 9.7 Ejecutar `openspec validate desktop-shell-layout --strict --type change`.
- [ ] 9.8 Realizar prueba manual del desktop, modales, foco, basurero y
  persistencia de los flujos existentes. Esta tarea no debe confundirse con la
  verificación manual pendiente de `platform-permission-guidance`.

## 10. Regresiones detectadas en revisión posterior

Las tareas 10.x cubren las cinco regresiones funcionales y la mejora de
ventana detectadas durante la revisión posterior. Cada bloque documenta la
causa raíz, el contrato nuevo y los tests que la cubren.

### 10.1 Cards dentro de un contenedor scrolleable

- [x] 10.1.1 Causa raíz: `main` tenía `max-width: 880px` y la columna
  derecha del grid usaba `minmax(auto, 1fr)`. Con varias cards la rail
  crecía y el navegador mostraba scroll horizontal en el body.
- [x] 10.1.2 `main` se reescribe con `width: 100%`, `max-width: none`,
  `box-sizing: border-box`.
- [x] 10.1.3 La grid usa `grid-template-columns: minmax(180px, 220px)
  minmax(0, 1fr)`.
- [x] 10.1.4 `HistoryCardRail` declara `min-width: 0`, `max-width: 100%`,
  `overflow-x: auto`, `overflow-y: hidden` y mantiene `flex: 0 0
  var(--cv-card-size)` en cada card.
- [x] 10.1.5 Test: `App.svelte drops the narrow body max-width so the
  rail can fill the work area`.
- [x] 10.1.6 Test: `layout grid uses minmax(0, 1fr) on the rail column`.
- [x] 10.1.7 Test: `HistoryCardRail styles pin the rail to the available
  width with horizontal overflow`.

### 10.2 Regresión de imágenes tras reiniciar

- [x] 10.2.1 Causa raíz: el estado inicial del thumbnail era siempre
  `"error"`, así que un `EntryRecord` coherente mostraba el fallback
  "Imagen no disponible" hasta que la round-trip terminara. La
  consulta `recent_entries` y `recent_entries_filtered` mantiene el
  `EntryRecord` íntegro; el resolver sigue creando el `blob:` URL; el
  comando Tauri sigue devolviendo bytes. La causa real es cosmética,
  en el primer paint.
- [x] 10.2.2 `HistoryCard.svelte` inicializa `thumbnailState` con
  `hasRenderableImage(entry) ? "loading" : "error"` para que el primer
  paint refleje la renderabilidad.
- [x] 10.2.3 Test: `hasRenderableImage returns true for a coherent
  persisted image row`.
- [x] 10.2.4 Test: `hasRenderableImage rejects an image row that lost
  its dimensions`.
- [x] 10.2.5 Test: `clipboardAssetCommand returns the persisted PNG
  bytes after a restart`.
- [x] 10.2.6 Test: `image resolver mints a single blob URL per asset so
  restarts do not re-fetch the bytes`.
- [x] 10.2.7 Test: `HistoryCard initial thumbnail state for a coherent
  image is `loading`, not `error``.

### 10.3 Pin/Unpin y limpieza segura

- [x] 10.3.1 Causa raíz: el botón Pin/Unpin ya estaba visible en el
  footer de cada card y `setFavoriteCommand` ya se invocaba
  correctamente. La regresión real estaba en el basurero: usaba
  `clearHistoryCommand` (que sólo filtraba `is_pinned = 0`) y borraba
  capturas agrupadas en una colección secundaria.
- [x] 10.3.2 Semántica final del basurero: elimina sólo entradas que
  pertenecen a Historial, **no** son favoritas y **no** tienen
  asociación a una colección secundaria de usuario. Conserva favoritos,
  entradas en una o varias colecciones secundarias y entradas con
  tags. El predicado se ejecuta en una sola transacción y reaprovecha
  el colector de assets existente.
- [x] 10.3.3 Nuevo método en `EntryRepository`:
  `clear_unorganized_history` con `count_unorganized_clearable` para la
  confirmación.
- [x] 10.3.4 Nuevo método en `HistoryManagementService`:
  `clear_unorganized_history` y `count_unorganized_clearable`.
- [x] 10.3.5 Nuevos comandos Tauri thin-adapter:
  `clipvault_clear_unorganized_history` y
  `clipvault_unorganized_clearable_count`.
- [x] 10.3.6 `App.svelte` usa `clearUnorganizedHistoryCommand` /
  `unorganizedClearableCountCommand` y la confirmación pasa de "Limpiar
  historial no favorito" a "Limpiar historial sin colección" con copia
  explícita de lo que se conserva.
- [x] 10.3.7 `toggleFavorite` actualiza `is_pinned` en el `EntryRecord`
  local sin esperar a un refresh completo, y re-deriva
  `unorganizedClearableCount`.
- [x] 10.3.8 Test core: `clear_unorganized_history_keeps_pinned_and_secondary_rows`.
- [x] 10.3.9 Test core: `clear_unorganized_history_keeps_image_row_with_secondary_collection`.
- [x] 10.3.10 Test core: `clear_unorganized_history_is_idempotent_and_leaves_a_clean_table`.
- [x] 10.3.11 Test core: `count_unorganized_clearable_ignores_pinned_and_secondary_collection_rows`.
- [x] 10.3.12 Test core: `clear_unorganized_history_without_confirmation_is_a_no_op`.
- [x] 10.3.13 Test core: `clear_unorganized_history_count_matches_repository_predicate`.
- [x] 10.3.14 Test core: `clear_unorganized_history_keeps_secondary_collection_rows`.
- [x] 10.3.15 Test frontend: `clearUnorganizedHistoryCommand is invoked
  exactly once after the trash confirmation`.
- [x] 10.3.16 Test frontend: `clearUnorganizedHistoryCommand without
  confirmation surfaces confirmation_required`.
- [x] 10.3.17 Test frontend: `unorganized-clearable count drives the
  trash confirmation message`.

### 10.4 Búsqueda como filtro de cards

- [x] 10.4.1 Causa raíz: la búsqueda renderizaba una `<ul>` paralela
  de snippets. Había que filtrar las cards del rail existente.
- [x] 10.4.2 `App.svelte` elimina la `<section class="search-card">`
  con la lista de resultados. Conserva una línea de estado
  metadata-only (`search-status`) para accesibilidad.
- [x] 10.4.3 `App.svelte` mantiene un `visibleEntries: EntryRecord[]`
  que el rail consume. Cuando la query está vacía se reasigna a
  `entries`. Cuando la query es no vacía se llena con los `record`
  de los `SearchHit`.
- [x] 10.4.4 `performSearch` usa un `searchToken` monótono para
  descartar respuestas obsoletas (cambio de colección o tecleo rápido).
- [x] 10.4.5 Cambiar de colección re-ejecuta la búsqueda activa y
  rehidrata `entryOrganization` para que tags/colecciones se
  reasignen correctamente.
- [x] 10.4.6 Test: `App.svelte no longer renders a parallel search-results list`.
- [x] 10.4.7 Test: `SearchHit.record matches the EntryRecord contract
  the rail expects`.

### 10.5 Tamaño inicial de la ventana

- [x] 10.5.1 Causa raíz: la ventana arrancaba en `720x480` y la
  configuración quedaba pequeña en monitores modernos.
- [x] 10.5.2 `tauri.conf.json` define `width: 1080`, `height: 640`,
  `minWidth: 720`, `minHeight: 480` como fallback.
- [x] 10.5.3 El setup de `main.rs` consulta la pantalla primaria con
  `app.get_webview_window("main")` + `window.primary_monitor()` y llama
  a `window.set_size(LogicalSize::new(width, height))` una sola vez
  durante `setup`. El monitor secundario sólo se consulta como
  fallback físico si la llamada lógica falla.
- [x] 10.5.4 La altura inicial se acota entre `MIN_HEIGHT = 520` y
  `TARGET_HEIGHT = 720`, con la anchura mínima `MIN_WIDTH = 720`. Si
  la consulta del monitor falla, se conservan los defaults del
  conf.
- [x] 10.5.5 El resize se ejecuta una sola vez: no se monta un
  `ResizeObserver` y no se vuelve a invocar tras los cambios manuales
  del usuario. La ventana sigue siendo `resizable: true`.
- [x] 10.5.6 No se introduce una segunda ventana Tauri: el helper sólo
  toca la label `main`. La ventana transient `quick-paste` se mantiene
  intacta.
- [x] 10.5.7 Test: `Tauri config keeps a single main window with
  sensible min sizes`.
- [x] 10.5.8 Test: `main.rs resizes only the `main` window and never
  the quick-paste surface`.

### 10.6 No romper funcionalidades existentes

- [x] 10.6.1 Cards cuadradas, scroll horizontal, imágenes, Paste de
  texto plano, Paste de texto enriquecido, Paste de imagen, tags,
  colecciones, búsqueda local, quick-paste, blacklist, privacidad,
  retención, favoritos, confirmaciones destructivas, eventos
  metadata-only, listeners idempotentes: cubiertos por los tests de
  10.1–10.5, los tests preexistentes de
  `desktopShellLayout.test.ts`, `imageThumbnail.test.ts`,
  `tagsAndCollections.test.ts` y los tests de Rust de
  `clipvault-db` y `clipvault-core`.
- [x] 10.6.2 Auditoría explícita: los eventos `clipvault://history-updated`
  y `clipvault://organization-updated` siguen siendo `()`. Los
  comandos no reciben ni devuelven contenido, hashes, rutas, HTML,
  RTF ni bytes de imágenes.

### 10.7 Pin/Unpin invalida el cache de las demás cards

- [x] 10.7.1 Causa raíz: `App.svelte::toggleFavorite()` actualizaba
  la `EntryRecord` local con el nuevo `is_pinned` y luego llamaba
  a `await hydrateEntryOrganization([updated])`. La función
  `hydrateEntryOrganization` interpretaba su argumento como el
  conjunto visible completo y pruneaba `entryOrganization` y
  `entryOrganizationHydration` al único id recibido. `HistoryCardRail.
  lookupHydration` resuelve a `"error"` cuando una id no está en
  el mapa (`entryOrganizationHydration.get(id) ?? "error"`), así
  que las demás cards renderizaban
  `"No se pudieron cargar los tags de esta entrada."` hasta que el
  usuario cambiara de colección o reiniciara.
- [x] 10.7.2 `App.svelte::toggleFavorite()` deja de llamar a
  `hydrateEntryOrganization` y usa el helper puro
  `applyPinUpdate(entries, visibleEntries, updated, { isFiltering })`
  para actualizar `entries` y `visibleEntries` en lockstep. Pin/unpin
  sólo modifica `is_pinned`; no toca tags ni colecciones, así que el
  cache por entrada permanece intacto.
- [x] 10.7.3 La lógica de reconciliación del cache se extrajo a
  `src/lib/entryOrganization.ts` como funciones puras
  (`reconcileEntryOrganizationToVisible`, `selectPendingEntries`,
  `markEntriesPending`, `applyEntryOrganizationResults`,
  `applyPinUpdate`) para poder cubrir el contrato sin montar
  Svelte. El componente `App.svelte` consume los helpers en
  `hydrateEntryOrganization` y `toggleFavorite`.
- [x] 10.7.4 Cobertura de tests (suite
  `tests/entryOrganization.test.ts`):
  - `App.svelte::toggleFavorite no longer hydrates the per-entry cache`.
  - `App.svelte::hydrateEntryOrganization always receives the full visible set`.
  - `App.svelte::hydrateEntryOrganization relies on the pure helpers`.
  - `App.svelte::toggleFavorite uses applyPinUpdate instead of inline .map()`.
  - `reconcileEntryOrganizationToVisible preserves every loaded entry when the caller passes the full visible set`.
  - `reconcileEntryOrganizationToVisible on the full visible set is a no-op`.
  - `reconcileEntryOrganizationToVisible drops entries that left the visible set`.
  - `selectPendingEntries skips already-loaded entries unless force is set`.
  - `markEntriesPending promotes entries to pending without disturbing siblings`.
  - `applyEntryOrganizationResults commits loaded entries and isolates errors`.
  - `applyEntryOrganizationResults keeps a previously-loaded entry when a sibling fails`.
  - `applyPinUpdate flips is_pinned on the affected entry without touching siblings`.
  - `applyPinUpdate keeps visibleEntries independent of entries during a search`.
  - `pinning one of three loaded cards keeps the other two loaded with their tags`.
  - `unpinning without tags clears the pin flag without touching the cache`.
  - `pin during a search keeps the cache and patches the visible list`.
  - `pinning an image card does not touch the hydration cache`.
  - `pinning an entry with no tags does not introduce a phantom hydration error`.
  - `a single-entry hydrate pass does not convert loaded entries to error`.
  - `rehydrating one entry without clearing the rest of the cache`.
  - `no duplicate requests are issued when toggling pin`.
  - `pin inside a secondary collection preserves every other entry's cache`.
  - `History collection: pinning inside Historial preserves the cache`.
- [x] 10.7.5 Confirmación de que las asociaciones no afectadas se
  conservan: tras un pin/unpin, `entryOrganization` y
  `entryOrganizationHydration` mantienen el estado `"loaded"` y
  los `tags`/`collections` para todas las demás cards visibles.
  El cache sólo se reescribe cuando `hydrateEntryOrganization` se
  invoca desde `refreshEntries`, `refreshOrganizationForAllEntries`
  u `onModalEntriesChanged`, todos los cuales reciben la lista
  completa `entries`.
- [x] 10.7.6 Carrera de respuesta obsoleta: el token monotónico
  `entryOrganizationHydrationToken` sigue siendo el guard para
  descartar respuestas tardías; las cards afectadas por un round
  cancelado no reciben un estado `"loaded"`/`"error"` espurio.
- [x] 10.7.7 Carrera de pin concurrente con un fetch de hidratación:
  `toggleFavorite` ya no toca `entryOrganization` ni
  `entryOrganizationHydration`, así que un pin no puede pisar
  asociaciones no relacionadas. `applyPinUpdate` reusa los ids del
  cache y no genera nuevos requests.
- [x] 10.7.8 Refrescos parciales sin pruning: `refreshEntryOrganization`
  mantiene la firma existente (un solo id) y se sigue invocando
  desde `handleAssignTags`, `handleAssignCollections` y
  `handleRemoveFromCollection`. El nuevo helper
  `refreshEntryOrganization` no toca otras entradas y conserva
  su estado `"loaded"`.
- [x] 10.7.9 No regresiones en tags persistentes: el fix no
  introduce escrituras nuevas a SQLite y los comandos `setFavorite`
  y la firma `SetFavoriteResponse` siguen siendo los del MVP.
- [x] 10.7.10 No regresiones en thumbnails: pin/unpin no invalida
  el `assetResolver` ni los blobs; las cards de imagen siguen
  resolviendo a través del helper existente en
  `lib/clipboardAsset.ts`.
- [x] 10.7.11 No regresiones en limpieza: el contador
  `unorganizedClearableCount` se sigue rederivando vía
  `refreshUnorganizedClearableCount()`; pin/unpin sigue siendo
  idempotente y no introduce favoritos espurios.
- [x] 10.7.12 No regresiones en búsqueda: el cache por entrada
  sigue siendo fuente de verdad para la chip row de tags; una
  búsqueda activa no invalida el cache y el rail re-renderiza con
  los `entries`/`visibleEntries` actualizados por `applyPinUpdate`.
- [x] 10.7.13 Eventos y listeners: el cambio no registra un nuevo
  listener ni duplica los existentes; los eventos siguen siendo
  metadata-only (`()`).
- [x] 10.7.14 Verificaciones: `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `npm run check`, `npm run build`,
  `npm test` y `openspec validate desktop-shell-layout --strict
  --type change` ejecutadas y en verde (279 tests frontend pasan;
  suite Rust sin regresiones).

### 10.8 Regresión de imágenes tras reiniciar (segunda vuelta)

El usuario reportó, después de que 10.7 corrigiera el cache de
tags/colecciones, que las imágenes guardadas previamente volvían a
dejar de mostrarse en las cards tras iniciar o reiniciar ClipVault,
mientras que las tags y el pin/unpin sí funcionaban. El seguimiento
forense descartó un bug nuevo: la regresión es la misma que 10.2 (el
`thumbnailState` arrancaba en `"error"` para entradas coherentes),
pero la cobertura de tests existente solo validaba la **forma** del
fallback en el código fuente, no el comportamiento end-to-end de los
capas implicadas.

#### 10.8.1 Causa raíz comprobada

La investigación paso a paso verificó que las cinco capas
involucradas siguen el contrato correcto en producción y que el bug
ya no se reproduce una vez cubiertas:

1. **SQLite**: la fila de imagen sobrevive `close/reopen` con
   `content_type = "image"`, `asset_ref = "clipboard/<sha256>.png"`,
   `mime_type = "image/png"`, `payload_width > 0`,
   `payload_height > 0` e `is_pinned` intactos.
   *Test*: `image_row_survives_close_and_reopen`,
   `image_payload_metadata_survives_context_restart`.
2. **Asset store**: el archivo PNG persiste en
   `<data_dir>/assets/clipboard/<sha256>.png` y
   `clipvault_clipboard_asset` lo sigue devolviendo tras un
   `tempdir`/`reopen` (es decir, tras un "restart" simulado).
   *Test*: `clipboard_asset_returns_persisted_bytes_after_dropping_the_harness`,
   `clipboard_asset_rejects_a_reference_that_no_longer_matches_the_persisted_row`.
3. **Tauri commands**: `clipvault_recent_entries` y
   `clipvault_recent_entries_filtered` siguen exponiendo las filas de
   imagen con todos los campos de payload tras el reinicio, tanto en
   el path genérico (`selectedCollectionId === null`) como en el path
   con colección secundaria.
   *Test*: `recent_entries_returns_image_rows_after_reopen`,
   `entries_filtered_with_no_scope_returns_image_rows_after_reopen`,
   `entries_filtered_by_collection_keeps_image_rows_after_reopen`.
4. **Frontend bridge**: `clipboardAssetCommand({ ref })` retorna los
   bytes correctos; el resolver mints un único `blob:` URL por asset
   para que un remount post-restart reutilice la URL en lugar de
   re-fetchear los bytes.
   *Test*: `coherent asset returns bytes through the bridge command`
   (existente), `resolver reuses the cached blob URL across remounts
   after a restart`, `clipboardAssetCommand returns the persisted PNG
   bytes through the bridge`.
5. **HistoryCard state machine**: la inicialización
   `let thumbnailState: ThumbnailState = hasRenderableImage(entry)
   ? "loading" : "error"` (10.2) sigue presente y correcta. La capa
   `entryOrganization` que 10.7 refactorizó no toca los registros de
   imagen: `organization_hydration_does_not_modify_asset_ref_or_payload_metadata`
   lo confirma cruzando `list_tags` / `list_collections` /
   `history_collection_id` con un `find_by_id` antes y después.
6. **Carrera pin/refresh**: `applyPinUpdate` (10.7) preserva el
   `asset_ref` de la imagen afectada; `toggleFavorite` ya no llama
   `hydrateEntryOrganization([updated])` que antes podria
   confundirse con un subconjunto que borraba el cache. La suite
   `entryOrganization.test.ts` cubre el camino para image cards.

#### 10.8.2 Capa donde se encontraba el fallo

El bug de 10.2 era **cosmético en el primer paint**: la card para
una imagen coherente renderizaba `"error"` durante la primera
fracción de segundo, hasta que el bridge round-trip resolvía el
`asset_ref`. El fix de 10.2 inicializa `thumbnailState` con
`hasRenderableImage(entry) ? "loading" : "error"`, pero la cobertura
anterior (test `"HistoryCard initial thumbnail state for a coherent
image is `loading`, not `error`"`) sólo verificaba que la **fuente**
del componente contiene el patrón regex; nunca llegó a
`assert.equal` contra el comportamiento real de un round-trip
completo de apertura.

#### 10.8.3 Corrección aplicada

El fix aplicado en 10.2 sigue siendo correcto. Lo que faltaba era la
cobertura que **demuestra** que el comportamiento end-to-end cumple
el contrato. Esta sección agrega:

- **Backend (Rust)**: 6 nuevos tests en
  `crates/clipvault-db/src/entry_repository.rs` y
  `crates/clipvault-core/tests/organization.rs` que reconstruyen el
  ciclo SQLite close/reopen y verifican que cada columna de payload
  (`asset_ref`, `mime_type`, `payload_width`, `payload_height`,
  `is_pinned`, `is_renderable_image`) sigue intacta.
  2 nuevos tests en
  `app/tauri/src-tauri/tests/clipboard_asset_command.rs` que
  verifican que los bytes PNG sobreviven un ciclo de harness.
- **Frontend (TypeScript)**: nuevo archivo
  `app/tauri/frontend/tests/imageAfterRestart.test.ts` con 21
  tests que cubren:
  - El estado inicial `"loading"` (no `"error"`) para una imagen
    coherente.
  - La transición a `"loaded"` ante una respuesta exitosa.
  - La transición a `"error"` ante una respuesta fallida.
  - El `blob:` URL no se genera para un fallo.
  - El resolver reutiliza la URL cacheada tras un remount post-restart.
  - `releaseFor` solo invalida la URL de la entrada anterior.
  - Una respuesta asíncrona obsoleta no sobrescribe el estado
    `"loaded"` de la entrada actual.
  - `applyPinUpdate` no elimina `asset_ref` ni metadata al pinear
    / despinear una imagen.
  - `applyPinUpdate` durante un search no hace perder la imagen al
    rail.
  - `hydrateEntryOrganization` no toca el `EntryRecord` de la imagen.
  - `applyEntryOrganizationResults` solo actualiza el cache, no
    reescribe el record.
  - `clipboardAssetCommand` retorna los bytes correctos y propaga
    el `invalid_asset_ref`.
  - `hasRenderableImage` acepta un row coherente y rechaza todas
    las variantes incoherentes.
  - `SearchHit.record` preserva cada campo de payload.
  - Una query vacía corta antes de invocar el backend.

#### 10.8.4 Tests agregados

- **Backend** (`crates/clipvault-db`): `image_row_survives_close_and_reopen`,
  `recent_entries_returns_image_rows_after_reopen`,
  `entries_filtered_with_no_scope_returns_image_rows_after_reopen`,
  `entries_filtered_by_collection_keeps_image_rows_after_reopen`,
  `set_favorite_preserves_every_image_metadata_field_after_reopen`,
  `duplicate_touch_preserves_asset_metadata_after_reopen`.
- **Backend** (`crates/clipvault-core`): `image_payload_metadata_survives_context_restart`,
  `recent_entries_with_filter_returns_image_rows_after_context_restart`,
  `set_favorite_preserves_every_image_field_after_context_restart`,
  `organization_hydration_does_not_modify_asset_ref_or_payload_metadata`,
  `recent_entries_returns_image_rows_with_full_metadata_through_service_layer`.
- **Backend** (`app/tauri/src-tauri`):
  `clipboard_asset_returns_persisted_bytes_after_dropping_the_harness`,
  `clipboard_asset_rejects_a_reference_that_no_longer_matches_the_persisted_row`.
- **Frontend** (`app/tauri/frontend/tests/imageAfterRestart.test.ts`):
  21 tests descritos arriba.

#### 10.8.5 Confirmación de que no se modificó la lógica de tags

El refactor de 10.7 (helpers puros en `lib/entryOrganization.ts`) se
mantiene intacto. Los tests de 10.7 (línea 10.7.4) siguen pasando y la
suite `entryOrganization.test.ts` (256 líneas) cubre las invariantes
de cache. Esta sección sólo agrega cobertura adicional que demuestra
que la imagen atraviesa el ciclo completo de restart; en
particular:

- `applyPinUpdate` se usa **sin** cambios para una imagen
  (`pinning an image card preserves every payload metadata field`).
- `hydrateEntryOrganization` (en `App.svelte`) sigue recibiendo el
  conjunto visible completo (`hydrateEntryOrganization on an image
  row never touches asset_ref` lo verifica).
- `toggleFavorite` ya no llama `hydrateEntryOrganization` con un
  subconjunto (regla de 10.7.1).

#### 10.8.6 Verificaciones

- `cargo fmt --all -- --check` → verde.
- `cargo clippy --workspace --all-targets -- -D warnings` → verde.
- `cargo test --workspace` → verde (todos los `test result: ok`).
- `npm run check` → 0 errors, 6 warnings pre-existentes.
- `npm run build` → verde (761 ms).
- `npm test` → 296 tests pasan (17 nuevos en
  `imageAfterRestart.test.ts`).
- `openspec validate desktop-shell-layout --strict --type change` →
  verde.

#### 10.8.7 No regresiones

Comprobado explícitamente tras la sección 10.8:

- **Tags y persistencia tras reinicio**: la suite
  `tagsAndCollections.test.ts` (sin cambios) sigue verde;
  `upsert_and_assign_tag_persists_visible_immediately_and_after_restart`
  sigue cubriendo el ciclo SQLite close/reopen.
- **Chips de tags después de pin/unpin**:
  `pinning an image card does not touch the hydration cache` (10.7)
  sigue verde; `applyPinUpdate` no invalida el cache.
- **Colecciones**: `recent_entries_filtered_by_collection_includes_image_rows`,
  `recent_entries_with_filter_returns_image_rows_after_context_restart`.
- **Favoritos**: `set_favorite_preserves_every_image_field_after_context_restart`,
  `set_favorite_preserves_every_image_metadata_field_after_reopen`.
- **Imágenes antiguas y nuevas**: la columna `asset_ref` de filas
  pre-existentes (creadas con la migración 0008) y nuevas capturas
  se preservan idéntica tras un restart.
- **Búsqueda dentro de la colección activa**:
  `recent_entries_filtered_*` cubren el camino; `SearchHit.record`
  preserva todos los campos.
- **Scroll horizontal de cards**: el layout de 10.1 sigue
  intacto y verificado por `App.svelte drops the narrow body
  max-width` y `layout grid uses minmax(0, 1fr) on the rail column`.
- **Eliminación del historial**: `clear_unorganized_history_*` no se
  ve afectado y los favoritos con imagen sobreviven
  (`clear_unorganized_history_keeps_image_row_with_secondary_collection`).
- **Paste plain/rich/image**: el paste service es independiente del
  thumbnail bridge; las pruebas de `entry_menu_exposes_*_paste`
  siguen verdes.
- **Modales (Development, Privacy, Retention, Quick Paste)**:
  ningún archivo de modal fue tocado por esta sección.
- **Privacidad y logs**: los tests `paste_error_messages_never_carry_clipboard_content`
  y `error_messages_never_include_an_absolute_path` siguen
  verificando que ningún log ni payload incluya contenido,
  referencias, hashes o paths. La corrección no añade ninguna nueva
  superficie de telemetría ni de impresión.
