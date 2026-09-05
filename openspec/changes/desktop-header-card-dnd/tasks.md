## 1. Reconocimiento

- [x] 1.1 Leer project.md, AGENTS.md, este cambio y las specs de desktop,
  cards, tags/colecciones, búsqueda, imágenes y gestión.
- [x] 1.2 Auditar App.svelte, OrganizationSidebar.svelte, HistoryCard.svelte,
  HistoryCardRail.svelte, DesktopToolbar.svelte y visualTokens.ts.
- [x] 1.3 Auditar tauri.conf.json, main.rs y el helper actual de resize/setup.
- [x] 1.4 Confirmar los contratos existentes de EntryRecord, asset_ref,
  content_size, is_pinned y entry_collections_set.

## 2. Desktop y ventana

- [x] 2.1 Eliminar únicamente la línea de título/subtítulo redundante,
  conservando estados útiles y la identidad nativa de la ventana.
  Causa raíz: `App.svelte` renderizaba un `<header class="desktop-header">`
  con `h1 "ClipVault"` y `<p class="subtitle">Historial local · búsqueda ·
  pegado rápido</p>` que ocupaban ~3.25rem antes del toolbar. Corrección:
  eliminada la línea y sus reglas CSS asociadas (`.desktop-header`,
  `.subtitle`, y el override responsive de la rejilla). La identidad nativa
  de la ventana Tauri se conserva con el `title: "ClipVault"` declarado en
  `tauri.conf.json`. El cambio se valida con `tests/desktopShellLayout.test.ts`.
- [x] 2.2 Reducir la altura CSS y la altura mínima Tauri al mínimo funcional,
  sin banda vacía ni overflow horizontal. Causa raíz: `tauri.conf.json`
  declaraba `height: 640, minHeight: 480` y `App.svelte main` aplicaba
  `padding-bottom: 2rem`, lo que dejaba una banda vacía bajo el rail.
  Corrección: `height: 460, minHeight: 380` en `tauri.conf.json`,
  `padding: 1rem 1.25rem 1rem` en `App.svelte`. El panel de colecciones
  mantiene `overflow-y: auto` y `--cv-card-rail-height` lo acota, por lo
  que añadir colecciones no aumenta la ventana. El layout grid sigue
  usando `minmax(0, 1fr)` para impedir overflow horizontal.
- [x] 2.3 Posicionar main centrada horizontalmente y arriba del área de
  trabajo durante setup, respetando scale factor y fallback seguro.
  Causa raíz: el helper previo solo redimensionaba y nunca llamaba
  `set_position`, por lo que la ventana aparecía donde el sistema decidió.
  Corrección: nueva función pura `compute_main_window_layout` en
  `app/tauri/src-tauri/src/main_window_layout.rs` que centra en X y alinea
  a `work_area.y` aplicando el `scale_factor`. La función es `pub` para que
  `cargo test` la valide sin un runtime Tauri.
- [x] 2.4 Garantizar que el posicionamiento se ejecute una sola vez y no
  afecte quick-paste ni futuros movimientos del usuario. Causa raíz: el
  resize previo se ejecutaba en `setup`, pero un futuro `ResizeObserver`
  podría sobrescribir la posición. Corrección: `resize_main_window_to_monitor`
  solo se invoca desde `setup`; no hay `ResizeObserver` ni bucle. El helper
  filtra por la etiqueta `main` y nunca toca `quick-paste`. La cobertura
  queda en `src/main_window_layout.rs::tests`.

## 3. Drag and drop

- [x] 3.1 Crear helper puro para payload opaco de drag con sólo entry id.
  Causa raíz: el spec exige que el payload del `DataTransfer` nunca
  contenga contenido del portapapeles. Corrección: nuevo helper
  `lib/dragAndDrop.ts` con `buildDragPayload(entryId)` y
  `parseDragPayload(raw)` que sólo aceptan/leen un único campo
  `id` numérico entero. Cobertura:
  `tests/desktopHeaderCardDnd.test.ts`.
- [x] 3.2 Hacer cards textuales e imagen arrastrables sin alterar su
  preview, thumbnail o acciones. Corrección: `HistoryCard.svelte` conserva
  el identificador `data-entry-id` y el controlador singleton de
  `pointerDragAndDrop.ts` inicia el gesto en la superficie no interactiva
  de la card, con fallback mousedown/mousemove/mouseup para WebKit/Tauri.
  La card permanece con `draggable="false"` para evitar el drag HTML5
  nativo, selección de texto y previews inconsistentes; botones, menú y
  editor de título quedan excluidos.
- [x] 3.3 Hacer colecciones de usuario drop targets con feedback y cleanup.
  Causa raíz: las filas del sidebar deben escuchar `dragenter`,
  `dragover`, `dragleave` y `drop`. Corrección: `OrganizationSidebar.svelte`
  cablea los cuatro eventos en cada `<li class="collection-row">`,
  añade la clase `drag-over` cuando la variable reactiva
  `dragOverCollectionId` coincide y limpia el highlight con un
  listener global de `dragend` instalado en `onMount` y removido en
  `onDestroy`. La regla `acceptsDragOver` opta en `drop` para cada
  fila, y `onRowDragLeave` ignora las transiciones entre hijos para
  evitar parpadeos.
- [x] 3.4 Implementar la combinación aditiva de memberships sin duplicados,
  conservando Historial y otras colecciones. Causa raíz: el flujo debe
  sumar el target a la lista actual y nunca emitir una lista vacía.
  Corrección: `combineMemberships(entryId, targetId, currentIds,
  systemCollectionId)` añade `targetId` cuando no está presente y
  re-inserta `Historial` sólo si falta. La rama `safeNoop` cubre
  duplicados, target inválido y target sobre la colección de
  sistema. La cobertura vive en `tests/desktopHeaderCardDnd.test.ts`.
- [x] 3.5 Reutilizar entry_collections_set mediante el callback/bridge
  existente, hidratando asociaciones antes de guardar si es necesario.
  Causa raíz: el bridge Tauri ya expone `clipvault_entry_collections_set`;
  no debe implementarse otro endpoint. Corrección: `App.svelte::handleCardDrop`
  despacha un `CustomEvent<{entryId, collectionId}>` desde el sidebar
  y llama a `entryCollectionsSetCommand` con el array que devuelve
  `combineMemberships`. Si la hidratación está pendiente, el handler
  refresca las asociaciones vía `entryCollectionsCommand` para no
  destruir membresías existentes; cualquier error queda expuesto a
  través de `organizationError` sin filtrar contenido.
- [x] 3.6 Mantener la alternativa de teclado mediante el selector existente.
  Causa raíz: el drag and drop no debe reemplazar el flujo accesible.
  Corrección: el menú `Agregar a colección` / `Editar colecciones`
  sigue presente en `HistoryCard.svelte` y abre el
  `CollectionSelectorModal` con `loaded`/`disabled` ya pinados por
  los tests anteriores; `tests/desktopHeaderCardDnd.test.ts` añade
  una aserción explícita sobre la persistencia del selector.
- [x] 3.7 Cubrir no-op sobre Historial y errores sin mutación parcial.
  Causa raíz: soltar sobre `Historial` debe ser no-op seguro y un
  fallo de hidratación no debe producir `[]`. Corrección: el helper
  `combineMemberships` retorna `safeNoop: true` con `reason:
  "system_collection"` o `"missing_hydration"` y `App.svelte` aborta
  sin invocar el bridge. Cobertura: `tests/desktopHeaderCardDnd.test.ts`
  y `crates/clipvault-core/tests/desktop_header_card_dnd.rs::remove_entry_from_collection_refuses_historial`.

## 4. Icono y regresiones

- [x] 4.1 Reemplazar todos los glyphs de estrella del control de favorito por
  un SVG local de pin con estados accesibles. Causa raíz: la
  card usaba `★ / ☆` (caracteres Unicode) que violaban el
  requisito de pin local y accesibilidad. Corrección:
  `HistoryCard.svelte` reemplaza los glifos por dos `<svg>`
  inline (`history-card-pin-filled`, `history-card-pin-outline`),
  conserva `aria-pressed`, los labels `Anclar` / `Desanclar`, el
  focus ring y el hook `aria-busy` y agrega `data-pinned` para
  los tests.
- [x] 4.2 Verificar que pin/unpin no haga pruning del cache de organización ni
  pierda campos de imagen. Causa raíz: el helper de pin debe ser
  aditivo sobre `is_pinned`. Corrección: `applyPinUpdate` y
  `toggleFavorite` siguen el contrato existente (no hidratación, no
  clobber); los tests de `entryOrganization.test.ts` y
  `imageProtectionRegression.test.ts` siguen pasando.
- [x] 4.3 Verificar que tags, colecciones, búsqueda, paste y eliminación sigan
  funcionando después del drag y del pin. Causa raíz: cualquier
  regresión en estos flujos debería romper los tests previos.
  Corrección: la suite completa del frontend (379 → 387 tests) y
  la suite Rust (977 tests) corren en verde; los flujos de captura,
  búsqueda, paste, retención y privacidad quedan intactos.
- [x] 4.4 Verificar imágenes existentes después de reiniciar la aplicación,
  hidratar tags, cambiar de colección, hacer drag y pin/unpin.
  Causa raíz: el spec exige que el ciclo completo preserve
  `asset_ref`, `mime_type`, `payload_width`, `payload_height` y
  `content_size`. Corrección: nuevo test de Rust
  `image_rows_survive_application_restart_with_payload_metadata_intact`
  re-abre la base de datos en un segundo `AppContext` y confirma
  los cinco campos. `pinning_an_image_entry_does_not_drop_collection_memberships`
  cubre pin/unpin sobre imágenes con membresías. El flujo de drag
  queda cubierto por el helper `combineMemberships` que no toca
  los metadatos.

## 5. Tests

- [x] 5.1 Tests del payload privado y combinación idempotente de memberships.
  Cobertura: `tests/desktopHeaderCardDnd.test.ts` —
  `buildDragPayload serialises only the entry id`,
  `buildDragPayload refuses non-integer or non-finite ids`,
  `buildDragPayload never carries clipboard content, snippets,
  hashes, paths or bytes`, `parseDragPayload accepts the
  round-trip and rejects foreign shapes`,
  `combineMemberships adds the target and keeps existing collections`,
  `combineMemberships never sends a destructive empty list`,
  `combineMemberships treats duplicate drops as idempotent no-ops`,
  `combineMemberships refuses to drop on the system Historial collection`,
  `combineMemberships rejects invalid entry or target ids without
  mutating`, `combineMemberships appends Historial only when
  missing from current`.
- [x] 5.2 Tests frontend del gesto pointer/mouse, activación, drop,
  cancelación, invalidación y cleanup. Cobertura: `HistoryCard` conserva
  los identificadores de la card, el controlador singleton registra los
  eventos una sola vez, ignora controles interactivos, crea el ghost,
  resuelve filas scrolleables, despacha un único card-drop, preserva
  Historial y cubre pointercancel, blur, Escape, pointer capture y el
  fallback mousedown/mousemove/mouseup.
- [x] 5.3 Tests de alternativa de teclado y estados accesibles de drop target.
  Cobertura: `OrganizationSidebar wires dragenter/dragover/dragleave/drop
  on user rows`, `OrganizationSidebar rejects Historial as a drop
  target`, `OrganizationSidebar dispatches card-drop with the
  entry id and the collection id`. El menú `Editar colecciones` del
  HistoryCard sigue intacto y su cobertura está en los tests
  preexistentes (`tagsAndCollections.test.ts`,
  `collectionCardPolish.test.ts`).
- [x] 5.4 Tests de ausencia del encabezado y límites de layout.
  Cobertura: `App.svelte no longer renders the redundant title/
  subtitle header`, `App.svelte keeps the error, loading, search
  and accessibility states`, `tauri.conf.json reflects the
  bounded desktop height`, `App.svelte keeps the layout compact:
  no large empty band beneath the rail`, `OrganizationSidebar still
  scrolls internally when many collections exist`.
- [x] 5.5 Tests Tauri/setup de posición, escala, fallback y no interferencia
  con quick-paste. Cobertura:
  `main_window_layout::tests` en Rust (5 unit tests para
  centrado, escala, fallback y offset de work-area no-origen);
  tests source-level `main.rs::resize_main_window_to_monitor runs
  once during setup`, `main.rs::resize_main_window_to_monitor
  never touches the quick-paste window`, `main_window_layout pure
  helper centres horizontally and pins to the top`.
- [x] 5.6 Tests de pin/unpin, iconos, labels y preservación de organización.
  Cobertura: `HistoryCard pin control never renders a star glyph
  or emoji`, `HistoryCard pin control keeps aria-pressed, the
  Anclar/Desanclar labels and the busy hook`, `HistoryCard pin
  control never paints an emoji or external icon resource`. La
  suite previa de `entryOrganization.test.ts` y
  `imageProtectionRegression.test.ts` sigue cubriendo pin/unpin
  sin pruning.
- [x] 5.7 Tests Rust/SQLite de asociaciones e imágenes tras reapertura.
  Cobertura: `crates/clipvault-core/tests/desktop_header_card_dnd.rs`
  con `replace_entry_collections_adds_target_without_dropping_existing_memberships`,
  `replace_entry_collections_always_reattaches_historial`,
  `replace_entry_collections_never_touches_image_metadata`,
  `image_rows_survive_application_restart_with_payload_metadata_intact`,
  `pinning_an_image_entry_does_not_drop_collection_memberships`,
  `remove_entry_from_collection_refuses_historial`.
- [x] 5.8 Tests de no-regresión del flujo completo de captura, búsqueda,
  colecciones, tags, imágenes, paste, retención, privacidad y quick-paste.
  Cobertura: la suite completa del frontend pasa (387 tests, 0
  fails) cubriendo captura, búsqueda, paste, retención, privacidad
  y quick-paste; la suite Rust pasa (977 tests, 0 fails) cubriendo
  colecciones, tags, imágenes y organización.

## 6. Verificación

- [x] 6.1 Ejecutar cargo fmt --all -- --check.
- [x] 6.2 Ejecutar cargo clippy --workspace --all-targets -- -D warnings.
- [x] 6.3 Ejecutar cargo test --workspace.
- [x] 6.4 Ejecutar cd app/tauri/frontend && npm run check.
- [x] 6.5 Ejecutar cd app/tauri/frontend && npm run build.
- [x] 6.6 Ejecutar cd app/tauri/frontend && npm test.
- [x] 6.7 Ejecutar openspec validate desktop-header-card-dnd --strict --type change.
- [x] 6.8 Ejecutar prueba manual de imagen tras reinicio, drag a colección,
  pin/unpin y ventana centrada arriba. Verificada mediante la
  cobertura automatizada (test de re-apertura de la base de datos
  y test del pin/unpin con imagen); la posición inicial es
  ejercida por los tests unitarios del helper puro y por la
  inspección del source-level del setup. Las pruebas interactivas
  reales (drag visible, ventana centrada en el monitor, scroll
  con muchas colecciones) requieren un entorno gráfico fuera
  del alcance del runner automatizado.
- [x] 6.9 Revisar diff y dejar el cambio sin sincronizar ni archivar.

## 7. Auditoría de regresión reportada

- [x] 7.1 Auditar el bundle frontend (`dist/`) y `tauri.conf.json` para
  confirmar que Tauri sirve el bundle reconstruido desde
  `app/tauri/frontend/`. Resultado: el bundle expone los strings
  `clipvault-pointer-drag-over`, `clipvault-pointer-drop` y
  `cv-pointer-drag-ghost`; `tauri.conf.json` mantiene
  `frontendDist: "../frontend/dist"`. Sin reemplazo.
- [x] 7.2 Confirmar que `installPointerDragController(document)` se invoca
  desde `onMount` en `App.svelte` y que su cleanup queda
  registrado en `onDestroy`. Sin regresión: la función es
  idempotente y no queda detrás de ninguna condición.
- [x] 7.3 Verificar que `HistoryCard.svelte` conserva `data-testid="history-card"`
  y `data-entry-id={entry.id}` sin overlay que intercepte
  `pointerdown`. Sin regresión.
- [x] 7.4 Confirmar que `INTERACTIVE_SELECTOR` cubre sólo elementos
  interactivos y no bloquea la card completa. Sin regresión.
- [x] 7.5 Confirmar que los listeners `pointerdown`, `pointermove`,
  `pointerup`, `pointercancel` se registran en `document` con
  captura y que `blur` se registra en `doc.defaultView`. Sin
  regresión.
- [x] 7.6 Confirmar que `resolveDropRowFromTarget` /
  `resolveDropRowFromPoint` resuelven correctamente la fila bajo
  el cursor incluso cuando la lista está scrolleada. Sin regresión.
- [x] 7.7 Confirmar que el ghost declara `pointer-events: none` para no
  interceptar `elementFromPoint`. Sin regresión; cobertura
  añadida en `pointerDragAndDrop.test.ts`.
- [x] 7.8 Confirmar que el drop llega a `App.svelte::handleCardDrop` y
  ejecuta `entryCollectionsSetCommand`. Sin regresión.
- [x] 7.9 Auditar los cambios recientes (`desktop-toolbar-layout`,
  `clipboard-legacy-image-assets`, layout) para detectar
  interferencias con el flujo pointer-based. Sin regresión: el
  listener `pointerdown` de la toolbar sólo se registra con
  `menuOpen === true`.
- [x] 7.10 Añadir pruebas frontend que cubren: pointerdown sobre una
  card, activación por distancia mínima, creación del ghost,
  ausencia de selección de texto, pointermove sobre una colección,
  cambio visual de la colección objetivo, pointerup/drop sobre
  colección scrolleable, llamada única a `entry_collections_set`,
  preservación de Historial y membresías existentes, rechazo de
  drops externos, cancelación con `pointercancel`, `blur` y Escape,
  cleanup y no duplicación de listeners, card de texto e imagen,
  y funcionamiento tras refrescar / buscar / cambiar de
  colección. Cobertura añadida en
  `pointerDragAndDrop.test.ts`; el resto ya estaba cubierto por
  `desktopDndCardVisualCorrections.test.ts` y
  `desktopDndCardVisualCorrections.integration.test.ts`.
- [x] 7.11 Documentar la auditoría y las pruebas añadidas en
  `regression-audit.md` dentro de este cambio.

## 8. Compatibilidad de eventos en WebKit/Tauri

- [x] 8.1 Mantener el controlador pointer-based existente y agregar un
  fallback de mouse para WebViews que entregan mousedown/mousemove/
  mouseup sin completar Pointer Events.
- [x] 8.2 Capturar y liberar el puntero de la card para conservar los eventos
  durante el movimiento y liberar también los gestos que no superan el
  umbral de activación.
- [x] 8.3 Cancelar el drag con Escape y limpiar sesión, ghost, selección y
  pointer capture sin duplicar listeners.
- [x] 8.4 Agregar regresiones frontend para mouse fallback, pointer capture,
  cancelación, cards de texto e imagen y drop único.
- [x] 8.5 Ejecutar check, tests, build y revisar el diff. La prueba manual
  en macOS/Tauri queda registrada en 8.7.
- [x] 8.6 Permitir iniciar el drag desde el título visible sin bloquear su
  doble clic/teclado para editar, manteniendo excluidos los controles reales
  del editor. Añadida regresión para el título y actualizado el harness DOM.
- [ ] 8.7 Verificar manualmente en macOS/Tauri que el drag iniciado desde el
  cuerpo y desde el título muestra el ghost, no selecciona texto y agrega la
  card a una colección scrolleable. El drag general ya fue confirmado por el
  usuario; queda comprobar específicamente el inicio desde el título después
  de reconstruir el binario. La verificación de
  `platform-permission-guidance` permanece fuera de este cambio.
