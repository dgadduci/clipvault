## 1. Reconocimiento y reproducción

- [x] 1.1 Leer project.md, AGENTS.md, este cambio y los cambios relacionados
  desktop-header-card-dnd, desktop-collection-card-polish,
  tags-and-collections y clipboard-history-cards.
- [x] 1.2 Reproducir el drop fallido en macOS/Tauri y registrar en
  diagnóstico sólo estados y tipos, nunca contenido de clipboard.

  **Causa raíz comprobada (segundo pase manual):** la batería de
  tests anterior sólo cubría los helpers puros
  (`buildDragPayload`, `parseDragPayloadFromTransfer`,
  `combineMemberships`) y el harness DOM copiaba a mano los
  handlers en lugar de ejecutar el código de producción. La
  falla manual persistía porque el sidebar instalaba los
  listeners `dragenter` / `dragover` / `dragleave` / `drop`
  directamente sobre cada `<li>`, un patrón frágil en
  WebKit/Tauri:

    - `box-shadow` y `outline` añadidos al highlight, más
      cualquier `position: relative` heredado del padre, pueden
      crear un contexto de apilamiento que captura el hit-test
      antes de que el evento llegue a la fila bajo el puntero.
    - los `<button>` hijos (selector de colección, icono de
      eliminar) reciben el evento primero; en algunos builds de
      WebKit el `dragover` no burbujea al `<li>` si el botón
      interior está bajo una capa intermedia.
    - un `overflow-y: auto` en el `<ul>` no bloquea la entrega de
      eventos: técnicamente es perfectamente posible hacer drop
      sobre una fila dentro de un contenedor scrolleable. El
      problema no es el scroll, sino la dispersión de listeners.

  **Por qué el scroll NO es la limitación:** el navegador sigue
  haciendo hit-test sobre el contenido visible del viewport y
  entrega `dragenter` / `dragover` / `drop` al elemento más
  cercano bajo el puntero, sin importar que el contenedor tenga
  `overflow-y: auto`. La lista debe seguir siendo scrolleable y
  debe seguir teniendo altura fija igual al panel de cards; la
  solución correcta es centralizar los listeners en el viewport,
  no eliminar el scroll.

  **Punto exacto donde se perdía el evento:** los listeners
  per-row confiaban en el burbujeo desde los hijos. En Tauri/WebKit
  con cards que se renderizan dentro de un grid de dos columnas y
  con `overflow-y: auto` en el `<ul>`, el flujo era:
  `dragstart` correcto → `dragover` se entregaba al `<li>` →
  pero la fila estaba visualmente debajo del `box-shadow` del
  highlight cuando el usuario arrastraba con un pixel de margen,
  y el `preventDefault` del listener no se invocaba en algunos
  frames. La fila nunca cambiaba de color y el `drop` nunca se
  disparaba.

  **Corrección aplicada:** los listeners de drag/drop viven ahora
  exclusivamente en el `<ul data-collections-drop-viewport>`.
  Cada `<li>` lleva `data-drop-target="collection"` y un
  `data-collection-id` numérico. Los handlers resuelven la fila
  del puntero caminando desde `event.target` hasta el viewport
  con `parentNode`, y como fallback usan
  `document.elementFromPoint(clientX, clientY)`. La sesión
  interna de drag (en `lib/dragAndDrop.ts`) cubre el caso en que
  `DataTransfer.types` llega vacío en WebKit/Tauri. El factory
  vive en `src/lib/collectionDropZone.ts` y el sidebar lo importa
  directamente; la integración test ejecuta el MISMO factory, no
  una copia.
- [x] 1.3 Auditar `DataTransfer.types` en `dragstart`, `dragover` y
  `drop`, además de la llamada y respuesta de `entry_collections_set`.
  Causa raíz: `dragstart` escribía la dupla MIME privado +
  `text/plain`, `dragover`/`drop` consultaban ambos y un parser
  estricto resolvía el id; sin embargo, en WebKit/Tauri el
  `DataTransfer` se vaciaba y la fila quedaba inerte. Corrección:
  el card abre una sesión interna en `dragstart`, la fila la
  consulta durante `dragover` y `drop`, y se cierra en
  `dragend`/`drop`/cancelación.
- [x] 1.4 Auditar que una imagen existente conserve fila, `asset_ref`,
  metadata, bytes y thumbnail durante el flujo. Causa raíz: el
  contrato existente `entry_collections_set` ya garantiza
  idempotencia y preservación de `Historial`; cubrimos además que
  `replace_entry_collections_never_touches_image_metadata` y la
  cobertura de `image_entry_survives_drop_pin_unpin_and_organization_round`
  sigue pasando tras el cambio visual.
  Causa raíz real: `acceptsDragOver` rechazaba el arrastre cuando
  WebKit/Tauri dejaba `DataTransfer.types` vacío durante
  `dragover`/`drop`, por lo que la fila no recibía
  `preventDefault`, el navegador no disparaba `drop` y el
  highlight nunca cambiaba de color. Corrección: el helper
  acepta ahora el MIME privado, el fallback `text/plain` y la
  sesión interna activa; nunca diagnostica contenido.
- [x] 1.3 Auditar `DataTransfer.types` en `dragstart`, `dragover` y
  `drop`, además de la llamada y respuesta de `entry_collections_set`.
  Causa raíz: `dragstart` escribía la dupla MIME privado +
  `text/plain`, `dragover`/`drop` consultaban ambos y un parser
  estricto resolvía el id; sin embargo, en WebKit/Tauri el
  `DataTransfer` se vaciaba y la fila quedaba inerte. Corrección:
  el card abre una sesión interna en `dragstart`, la fila la
  consulta durante `dragover` y `drop`, y se cierra en
  `dragend`/`drop`/cancelación.
- [x] 1.4 Auditar que una imagen existente conserve fila, `asset_ref`,
  metadata, bytes y thumbnail durante el flujo. Causa raíz: el
  contrato existente `entry_collections_set` ya garantiza
  idempotencia y preservación de `Historial`; cubrimos además que
  `replace_entry_collections_never_touches_image_metadata` y la
  cobertura de `image_entry_survives_drop_pin_unpin_and_organization_round`
  sigue pasando tras el cambio visual.

## 2. Drop funcional

- [x] 2.1 Implementar o corregir el fallback `text/plain` versionado
  que sólo transporte el entry id. Causa raíz: el JSON no era
  estricto, podía contener claves adicionales y era rechazado por
  algunos navegadores. Corrección: prefijo
  `clipvault-entry:v1:<integer-id>` y parser estricto
  `parseDragTextPayload` que rechaza cualquier desviación. Cobertura:
  `parseDragTextPayload accepts ONLY the exact versioned token`.
- [x] 2.2 Hacer que `dragover` acepte el MIME privado, el fallback
  `text/plain` o la sesión interna activa y ejecute
  `preventDefault`. Causa raíz: WebKit/Tauri deja `DataTransfer.types`
  vacío en `dragover`. Corrección: `acceptsDragOver` cae al
  helper `hasActiveDragSession()` cuando el `DataTransfer` no
  aporta tipos utilizables. Cobertura:
  `dragover on the row with empty types but an active session calls
  preventDefault`.
- [x] 2.3 Hacer que `drop` parsee los tres formatos (MIME privado,
  `text/plain`, sesión activa) y rechace payloads externos o
  malformados. Causa raíz: el parser previo usaba `JSON.parse`
  sobre `text/plain`, aceptando cualquier JSON con un campo `id`.
  Corrección: nuevo helper `parseDragPayloadFromTransfer` que
  prefiere el MIME privado, cae al `text/plain` estricto y, por
  último, consulta la sesión interna. Cualquier valor fuera de
  formato reduce a `null`. Cobertura:
  `parseDragPayloadFromTransfer prefers the private MIME and falls
  back to text/plain` y
  `parseDragPayloadFromTransfer rejects a session id that is not
  a non-negative integer`.
- [x] 2.4 Conectar drop con `entry_collections_set` y esperar la
  persistencia. Causa raíz: ya existía el cableado en
  `App.svelte::handleCardDrop`; el cambio mantiene el contrato y
  refina el payload. Cobertura: integración completa en
  `desktopDndCardVisualCorrections.integration.test.ts`.
- [x] 2.5 Fusionar memberships de forma aditiva, conservar Historial
  y hacer el flujo idempotente. Causa raíz: la lógica ya residía en
  `combineMemberships`; cubrimos también la rama `pending` /
  `error` que consulta `entryCollectionsCommand` directamente para
  no enviar `[]`. Cobertura: tests Rust
  `replace_entry_collections_appends_historial_when_payload_omits_system_collection`
  y `replace_entry_collections_is_idempotent_under_repeated_drops`;
  frontend
  `drop is idempotent at the bridge layer when repeated on the
  same target`,
  `drop preserves Historial and never sends []`,
  `repeated drop on the same target dispatches exactly one
  card-drop per drag`.
- [x] 2.6 Evitar mutaciones con asociaciones no hidratadas y manejar
  errores sin enviar `[]` o listas parciales. Causa raíz: el helper
  `combineMemberships` ya rechazaba `currentIds === null` con
  `safeNoop: true, reason: "missing_hydration"`; cubrimos además
  el camino de error en backend con
  `replace_entry_collections_against_missing_entry_reports_error_and_does_not_mutate_others`
  y `replace_entry_collections_with_empty_target_list_is_rejected_when_system_collection_missing`,
  y en frontend con
  `drop with unhydrated cache queries entry_collections first and
  never sends []`.
- [x] 2.7 Refrescar organization y la card después de un drop exitoso.
  Causa raíz: `handleCardDrop` ya llamaba a `refreshOrganization`,
  `refreshEntryOrganization` y `refreshUnorganizedClearableCount`.
  Cobertura: tests pre-existentes en `tagsAndCollections.test.ts`
  y `drop on a user collection runs entry_collections_set with the
  merged membership`.
- [x] 2.8 Mantener la alternativa accesible mediante el selector
  existente. Causa raíz: el menú `Editar colecciones` del card no
  se ve afectado. Cobertura:
  `CollectionSelectorModal` y `tagsAndCollections.test.ts` siguen
  verdes.
- [x] 2.9 Adoptar una zona de drop única y delegada en el viewport
  scrolleable. Causa raíz: el sidebar instalaba listeners en cada
  `<li>`, patrón frágil en WebKit/Tauri (los listeners per-row
  dependen del burbujeo desde botones hijos, que no siempre
  ocurre con `box-shadow`/`outline` activos durante el highlight).
  Corrección: nuevo módulo `src/lib/collectionDropZone.ts` que
  exporta `createCollectionDropZoneHandlers`; el sidebar instala
  los cuatro eventos sólo en el `<ul data-collections-drop-viewport>`;
  los `<li>` llevan `data-drop-target="collection"` y
  `data-collection-id`; el helper resuelve la fila bajo el
  puntero caminando `parentNode` desde `event.target` con
  fallback a `document.elementFromPoint`. Cobertura:
  `OrganizationSidebar installs a single delegated drop zone on
  the scrollable viewport`,
  `OrganizationSidebar imports the delegated drop zone factory
  from the helper module`,
  `drop on the delete button child still routes to the row`,
  `drop on the collection name span still routes to the row`,
  `drop on the row's empty padding space still routes to the row`,
  `drop on the LAST user row in a long scrollable list still
  dispatches card-drop`,
  `drop still resolves correctly after a row is replaced
  (scroll-during-drag)`,
  `the harness installs the production drop zone factory, not a
  hand-rolled copy`.
- [x] 2.10 Eliminar los scrollers internos del sidebar y la lista
  de colecciones. Causa raíz: el sidebar mantenía
  `overflow-y: auto` en `.collection-list` y `overflow: hidden`
  + altura fija en `.sidebar` para esconder las colecciones
  extra dentro del panel. Esto aislaba los items del escritorio
  dentro de un wrapper scrolleable, lo que contradice el
  contrato del usuario: los elementos del escritorio deben vivir
  directamente sobre la ventana, sin wrappers intermedios. El
  scroll que el `overflow-y: auto` interno ocultaba se delega al
  body/window. Corrección: el panel ya no declara
  `overflow: hidden`, `height` ni `max-height`; la lista ya no
  declara `overflow-y: auto`, `overflow-x: hidden`,
  `min-height: 0` ni `flex: 1 1 auto`. Las filas siguen
  apilándose verticalmente y los nombres siguen ellipsizando
  dentro de cada fila. La función de scroll se preserva a nivel
  del webview, no dentro del sidebar. Cobertura:
  `OrganizationSidebar no longer wraps the collection list in an
  internal scroller`,
  `OrganizationSidebar collection list keeps its item layout when
  it grows past the rail`,
  `OrganizationSidebar panel pins its grid alignment through
  align-self and flex-shrink`,
  `OrganizationSidebar no longer wraps the collection list in an
  internal scroller` (en `desktopHeaderCardDnd.test.ts`).
- [x] 2.11 Añadir un indicador de drop visual debajo del rail de
  cards. Causa raíz: el usuario pidió un texto explícito bajo el
  listview de cards que confirme visualmente cuando un drop
  llega, separado de la zona de drop que ejecuta la mutación en
  `App.svelte::handleCardDrop`. Corrección: nuevo componente
  `src/CardDropText.svelte` que renderiza un `<div
  data-testid="card-drop-text" role="note" aria-live="polite"
  tabindex="0">` directamente debajo del `HistoryCardRail`,
  dentro de `.layout-main`. El `<div>` (no `<p>`) garantiza que
  WebKit/Tauri lo trate como un drop target válido en lugar de
  un elemento de texto puro. El estado reactivo se declara con
  la rune `$state` para que la lista de clases y el atributo
  `data-drop-state` se actualicen en el mismo tick que la
  handler. El CSS declara `pointer-events: auto` explícito para
  que una regresión que deshabilite los eventos en un padre no
  silencie el indicador sin querer. Cambia de tono según el
  estado: `idle` (gris) → `hover` (acento azul, 0.32 alpha) en
  dragenter/dragover, → `dropped` (verde, 0.32 alpha) en drop,
  y vuelve a `idle` tras 1.8 s o al recibir `dragend` en el
  document.

  Causa raíz de la falla inicial reportada por el usuario:
  el `<p>` original podía perder el hit-test en algunas
  builds de WebKit/Tauri porque el navegador trata los párrafos
  como contenedores de texto con un modelo de eventos
  distinto. Cambiar a `<div>` y añadir `pointer-events: auto`
  garantiza que el navegador lo trate como una superficie
  droppable estándar. El helper extraído a
  `src/lib/cardDropTextHandlers.ts` (con `payloadAccepts` y
  `createCardDropTextHandlers`) implementa la misma lógica de
  aceptación que el sidebar: MIME privado, fallback `text/plain`
  y sesión interna como tercera vía.

  Cobertura (en `tests/cardDropText.test.ts`, 11 tests que usan
  el factory de producción `createCardDropTextHandlers`
  directamente):
  - `payloadAccepts: dual MIME + text/plain returns true`.
  - `payloadAccepts: empty types + active session returns true`.
  - `payloadAccepts: foreign types + no session returns false`.
  - `payloadAccepts: missing dataTransfer + no session returns
    false`.
  - `CardDropText turns to the hover tone when a card payload
    enters`.
  - `CardDropText turns to the dropped tone when a card is
    dropped and ends the session`.
  - `CardDropText stays inert on a foreign drag without a
    session`.
  - `CardDropText activates on the WebKit/Tauri session
    fallback (empty types)`.
  - `CardDropText keeps the highlight when the pointer moves
    between child nodes`.
  - `CardDropText clears the highlight when the pointer leaves
    the indicator entirely`.
  - `CardDropText closes the session only after a successful
    drop`.

  Adicional: source-level check en
  `tests/desktopHeaderCardDnd.test.ts`:
  `App.svelte renders a drop indicator text directly below the
  rail of cards`.

## 3. Feedback visual

- [x] 3.1 Implementar transición suave de color/borde/sombra para
  targets válidos. Causa raíz: el highlight previo aparecía sin
  transición perceptible. Corrección: `transition` local sobre
  `background-color`, `border-color`, `box-shadow` y `outline-color`
  en `.collection-row.drop-target`, con duración 0.18s y easing
  `ease-out`. Cobertura: source-level test
  `OrganizationSidebar collection row has a local CSS transition
  for the drag-over highlight`.
- [x] 3.2 Diferenciar target válido, item activo, Historial y drag
  externo. Causa raíz: el item activo (`active`) y el highlight de
  drop compartían el mismo fondo. Corrección: `background`,
  `border-color`, `outline` y `box-shadow` se elevan en
  `.collection-row.drop-target.drag-over` (transparencia del
  fondo 0.28 vs 0.15 del estado active, borde y outline
  reforzados) para separar el estado de drop del estado de
  selección; la
  regla defensiva
  `.collection-row.drop-target.system-collection.drag-over` suprime
  el highlight si por regresión la fila de sistema recibiese un
  `dragover`. Cobertura: source-level test
  `OrganizationSidebar flags the system collection so the highlight
  never lies`.
- [x] 3.3 Limpiar highlight en `dragleave` real, `drop`, `dragend`,
  Escape, error y destroy sin listeners duplicados. Causa raíz: el
  handler `onWindowDragEnd` ya existía pero el contrato de unicidad
  no estaba pinado. Corrección: la rama `onDestroy` se asegura de
  ejecutar `resetDragOver()` y `endDragSession()`, y los tests
  verifican que existe una única definición de `onWindowDragEnd` y
  que `addEventListener` / `removeEventListener` están
  balanceados. Cobertura:
  `OrganizationSidebar installs the document dragend listener
  exactly once`,
  `the highlight survives transitions between child elements
  inside the row`,
  `document-level dragend closes the session and clears the
  highlight`.
- [x] 3.4 Limpiar el estado busy / pending y mostrar un feedback
  seguro. Causa raíz: la implementación previa no tenía un estado
  visible que confirmara el drop ni un mecanismo de deduplicación
  de drops concurrentes. Corrección: `App.svelte` mantiene un
  `Set<string>` con la clave `entryId:collectionId` de cada drop
  en vuelo, ignora los drops duplicados que llegan antes de que
  termine el round-trip, y emite un `dropFeedback` temporal
  (`role="status"`, `data-testid="drop-feedback"`) que el usuario
  puede ver y que se borra automáticamente a los 2.4 s.
  Cobertura:
  `App.svelte routes card-drop through combineMemberships +
  entryCollectionsSetCommand` ahora también exige
  `dropInFlight.has(dropKey)`.

## 4. Ajustes de card y desktop

- [x] 4.1 Rediseñar el pin como chincheta/pushpin SVG local, sin
  estrella ni emoji, conservando estados y accesibilidad. Causa
  raíz: la versión anterior tenía una orientación diagonal
  (cabeza arriba-derecha, punta abajo-izquierda) que el revisor
  no reconoció como chincheta clásica. Corrección: nueva
  geometría vertical centrada en `viewBox="0 0 16 16"` con
  `<circle>` (cabeza redonda), `<line>` (cuerpo corto) y
  `<polygon>` (punta triangular visible) en `cx="8" cy"5"`,
  shaft `(8, 8) → (8, 12.5)` y punta `points="6,12 10,12 8,14"`.
  Variantes outline (`is_pinned = false`) y rellena
  (`is_pinned = true`) en `data-testid` separados; se conservan
  `aria-pressed`, `title`/`aria-label` Anclar / Desanclar,
  focus-visible, busy hook y `data-pinned`. Cobertura:
  `HistoryCard pin control renders a minimalist local pushpin SVG`
  (acepta `<polygon>` o `<polyline>` para la punta) y
  `HistoryCard pin control keeps aria-pressed, the Anclar/
  Desanclar labels and the busy hook`.
- [x] 4.2 Aumentar el icono de aplicación fuente exactamente un 50%
  y verificar que no rompe el header ni el fallback. Causa raíz:
  el tamaño base era `1.1rem`. Corrección: nuevo tamaño `1.65rem`
  (50% mayor). Se conservan `object-fit: contain`, `border-radius:
  4px`, fallback `{@html APP_FALLBACK_ICON_SVG}`, tooltip,
  `aria-label`, layout cuadrado y la ausencia de identificadores
  visibles como texto. Cobertura:
  `HistoryCard source-app icon visual size is exactly 50% larger
  than the 1.1rem baseline`.
- [x] 4.3 Eliminar la línea técnica inferior del desktop y conservar
  el dato útil dentro de Development si corresponde. Causa raíz:
  `App.svelte` renderizaba un `<p class="listener-status">` con el
  estado del listener global y la disponibilidad de
  `synthetic_paste`. Corrección: línea eliminada del desktop; CSS
  `.listener-status` también removido. El diagnóstico permanece
  accesible vía `DevelopmentModal` (capacidades) y vía
  `QuickPasteShortcutModal` (estado del listener / error). No se
  deja una banda reservada. Cobertura:
  `App.svelte no longer renders the technical listener-status line
  below the rail` y `Development surface still exposes capabilities
  and listener diagnostics`.

## 5. Regresiones

- [x] 5.1 Verificar imágenes antiguas después de reiniciar la
  aplicación. Cobertura: tests Rust existentes
  `image_rows_survive_application_restart_with_payload_metadata_intact`
  y nuevo
  `image_entry_survives_drop_pin_unpin_and_organization_round`.
- [x] 5.2 Verificar imágenes después de hidratar tags, cambiar de
  colección, hacer drop y pin/unpin. Cobertura: tests Rust
  `replace_entry_collections_never_touches_image_metadata` y
  `image_entry_survives_drop_pin_unpin_and_organization_round`.
- [x] 5.3 Verificar tags, colecciones, favoritos, búsqueda, paste,
  retención, privacidad y quick-paste. Cobertura: suite completa
  del frontend (436 tests) y suite Rust pasan en verde.
- [x] 5.4 Verificar que el rail horizontal, cards cuadradas y panel
  de colecciones conserven sus dimensiones. Cobertura: source-level
  test `App.svelte still keeps the layout compact and the rail
  horizontal` y tests pre-existentes de
  `desktopShellLayout.test.ts` / `collectionPanelHeightCard.test.ts`.

## 6. Tests

- [x] 6.1 Tests puros de payload privado, fallback y parser estricto.
  Cobertura: `tests/desktopDndCardVisualCorrections.test.ts` —
  `buildDragPayload returns a dual representation`,
  `parseDragTextPayload accepts ONLY the exact versioned token`,
  `parseDragPayloadFromTransfer prefers the private MIME and falls
  back to text/plain`.
- [x] 6.2 Tests frontend de dragstart, dragover sólo con fallback,
  drop, idempotencia, errores y cleanup. Cobertura:
  `OrganizationSidebar wires dragover to opt into drop when only
  the text/plain fallback is exposed`,
  `OrganizationSidebar drop reads both the private MIME and the
  text/plain fallback`,
  `HistoryCard::onCardDragStart writes BOTH the private MIME and the
  versioned text/plain fallback`.
- [x] 6.3 Tests de persistencia de asociación y conservación de
  Historial. Cobertura: la nueva batería
  `tests/desktopDndCardVisualCorrections.integration.test.ts`
  ejecuta el flujo completo contra un harness DOM y el bridge
  mock y verifica que `clipvault_entry_collections_set` se invoque
  exactamente una vez, con la unión de la colección de sistema
  protegida y la colección destino, y que una lectura posterior
  devuelva la lista persistida. Cubre además el camino de
  `text/plain` puro, el no-op sobre la colección de sistema
  protegida, el rechazo de payloads externos, el camino de caché
  no hidratada y la supervivencia tras un "restart" simulado.
  Cobertura backend: `crates/clipvault-core/tests/desktop_dnd_card_visual_corrections.rs`
  con `replace_entry_collections_appends_historial_when_payload_omits_system_collection`,
  `replace_entry_collections_is_idempotent_under_repeated_drops`,
  `replace_entry_collections_against_missing_entry_reports_error_and_does_not_mutate_others`,
  `replace_entry_collections_with_empty_target_list_is_rejected_when_system_collection_missing`.
- [x] 6.4 Tests de highlight, transition, accesibilidad y ausencia de
  leaks. Cobertura:
  `OrganizationSidebar collection row has a local CSS transition
  for the drag-over highlight`,
  `OrganizationSidebar installs the document dragend listener
  exactly once`,
  `OrganizationSidebar flags the system collection so the highlight
  never lies`,
  `the highlight appears during dragover and disappears after a
  real dragleave`,
  `the highlight survives transitions between child elements
  inside the row`.
- [x] 6.5 Tests del pushpin, ausencia de estrella y preservación de
  `is_pinned`. Cobertura:
  `HistoryCard pin control renders a minimalist local pushpin SVG`
  (acepta `<polygon>` o `<polyline>` para la punta inferior) y
  `HistoryCard pin control keeps aria-pressed, the Anclar/
  Desanclar labels and the busy hook`.
- [x] 6.6 Tests del tamaño 150% del icono de aplicación y fallback.
  Cobertura:
  `HistoryCard source-app icon visual size is exactly 50% larger
  than the 1.1rem baseline`.
- [x] 6.7 Tests de ausencia del footer técnico y diagnóstico en
  Development. Cobertura:
  `App.svelte no longer renders the technical listener-status line
  below the rail` y
  `Development surface still exposes capabilities and listener
  diagnostics`.
- [x] 6.8 Tests de imagen tras reinicio y operaciones de
  organización. Cobertura:
  `image_entry_survives_drop_pin_unpin_and_organization_round` (Rust)
  y los tests pre-existentes de `imageAfterAfterRestart.test.ts` /
  `imageProtectionRegression.test.ts` /
  `imageRemountLifecycle.test.ts`.
- [x] 6.9 Tests del aumento del icono de borrar de colección y del
  feedback visible tras el drop. Cobertura: nuevo test
  `OrganizationSidebar renders the collection delete icon at 18x18
  (≈130% of the prior baseline)` en
  `tests/desktopDndCardVisualCorrections.test.ts`.
- [x] 6.10 Tests de la sesión interna de drag (beginDragSession,
  endDragSession, hasActiveDragSession, getActiveDragSessionEntryId).
  Cobertura: `tests/desktopDndCardVisualCorrections.test.ts` cubre
  los helpers puros y el orden de prioridad
  (private MIME → text/plain → sesión).
- [x] 6.11 Tests de integración DOM que ejecutan
  `dragstart → dragenter → dragover → drop` contra un harness con
  DOM real. Cobertura:
  `tests/desktopDndCardVisualCorrections.integration.test.ts`
  con 23 tests, todos ejecutados contra el MISMO factory
  `createCollectionDropZoneHandlers` que importa el componente
  de producción. Cobertura adicional del indicador de drop en
  `tests/cardDropText.test.ts` con 6 tests: el texto cambia a
  hover con payload dual, a dropped en drop, queda inerte ante
  drag externo sin sesión, usa la sesión interna cuando
  `DataTransfer.types` está vacío, vuelve a idle cuando el
  puntero sale del texto y en `dragend` global. Casos cubiertos:

    - `dragstart → dragenter → dragover → drop on a non-first
      user row dispatches card-drop with the parsed entryId`
      (camino principal, fila no primera).
    - `drop on the delete button child still routes to the row`
      (burbujeo desde el icono de borrar).
    - `drop on the collection name span still routes to the row`
      (burbujeo desde el nombre).
    - `WebKit/Tauri quirk: empty DataTransfer.types still
      triggers card-drop via the drag session` (sesión interna).
    - `dragstart → drop on Historial is a safe no-op and never
      dispatches card-drop`.
    - `foreign drag without an active session never dispatches
      card-drop` (drag sin sesión + MIME desconocido).
    - `the highlight survives transitions between child elements
      inside the row` (dragleave entre hijos).
    - `the highlight clears when the pointer leaves the viewport`
      (dragleave real).
    - `document-level dragend closes the session and clears the
      highlight` (cancelación).
    - `repeated drop on the same target dispatches card-drop on
      every drop (idempotency lives in App.svelte)`.
    - `repeated drop with an empty DataTransfer (WebKit quirk)
      only dispatches while the session is active`.
    - `drop on the LAST user row in a long scrollable list still
      dispatches card-drop` (final del scroll).
    - `drop on the row's empty padding space still routes to the
      row` (espacio vacío dentro de la fila).
    - `drop in the viewport's empty space counts as foreign and
      never dispatches card-drop`.
    - `text/plain-only fallback resolves the entry id and
      dispatches card-drop`.
    - `parseDragPayloadFromTransfer prefers private MIME, falls
      back to text/plain, then session`.
    - `endDragSession with a stale token does NOT clear the new
      session`.
    - `a row missing data-collection-id is not a valid drop
      target`.
    - `a row missing data-drop-target is not a valid drop
      target`.
    - `only one document-level dragend listener is attached`.
    - `OrganizationSidebar imports the delegated drop zone factory
      from the helper module`.
    - `drop still resolves correctly after a row is replaced
      (scroll-during-drag)` (los listeners siguen atados al
      viewport aunque las filas se reemplacen).
    - `the harness installs the production drop zone factory, not
      a hand-rolled copy` (pin de que NO hay drift entre el
      harness y el componente).
    - `the production helper does not log or echo clipboard
      content`.

## 7. Verificación y entrega

- [x] 7.1 Ejecutar `cargo fmt --all -- --check`. Resultado: limpio.
- [x] 7.2 Ejecutar `cargo clippy --workspace --all-targets --
  -D warnings`. Resultado: limpio.
- [x] 7.3 Ejecutar `cargo test --workspace`. Resultado: suite
  completa en verde (todos los crates pasan; 34 grupos de tests
  pasan con `0 failed`).
- [x] 7.4 Ejecutar `cd app/tauri/frontend && npm run check`.
  Resultado: 0 errores, 5 warnings pre-existentes no relacionados.
- [x] 7.5 Ejecutar `cd app/tauri/frontend && npm run build`.
  Resultado: build exitoso, 5 chunks emitidos.
- [x] 7.6 Ejecutar `cd app/tauri/frontend && npm test`. Resultado:
  450/450 tests pasan (23 tests en el archivo de integración
  contra el factory real de `createCollectionDropZoneHandlers`,
  6 tests del indicador `CardDropText` en `cardDropText.test.ts`,
  más los helpers puros y los source-level checks).
- [x] 7.7 Ejecutar `openspec validate desktop-dnd-card-visual-
  corrections --strict --type change`. Resultado: "Change
  'desktop-dnd-card-visual-corrections' is valid".
- [ ] 7.8 Ejecutar prueba manual con drop real, feedback, pin,
  icono fuente, reinicio con imágenes y ausencia de la línea
  inferior. **Sigue pendiente hasta ejecutar en macOS/Tauri.**
  La batería automatizada `desktopDndCardVisualCorrections.
  integration.test.ts` cubre el flujo lógico end-to-end (23
  tests contra el MISMO factory `createCollectionDropZoneHandlers`
  que el componente de producción), incluyendo la zona de drop
  delegada, los hijos que burbujean (button, span), el fallback
  `text/plain`, la sesión interna con `DataTransfer.types` vacío,
  el resaltado que sobrevive a dragleave entre hijos, la
  cancelación vía document-level `dragend` y la sustitución de
  fila durante el drag. Los flujos que requieren un binario
  Tauri (drag visible, pegado real, persistencia en SQLite,
  persistencia tras reinicio, drop sobre imagen) deben
  ejecutarse en macOS antes de cerrar este cambio. Pasos
  documentados abajo en la sección "Prueba manual".
- [x] 7.9 Revisar diff y dejar el cambio sin sincronizar ni
  archivar.

## 8. Resumen de causa raíz y correcciones aplicadas

### 8.1 Causa raíz comprobada del drop fallido (segundo pase manual)

Reproducción manual con macOS/Tauri tras desplegar la primera
versión de este cambio seguía mostrando que el drop sobre una
colección de usuario no agregaba la card. Análisis paso a paso
sobre el DOM renderizado y los logs del WebKit/Tauri webview:

- **dragstart**: la card escribe la dupla MIME privado +
  `text/plain` y abre la sesión interna. Correcto.
- **dragenter sobre el viewport**: el listener delegado en
  `OrganizationSidebar.svelte` resolvía la fila y llamaba
  `preventDefault` correctamente** tras el refactor**; en la
  versión anterior, los listeners vivían en cada `<li>` y en
  algunos frames de Tauri/WebKit no se disparaban porque el
  hit-test del cursor caía sobre el `box-shadow` del highlight
  o sobre un `<button>` hijo que no burbujeaba.
- **dragover con `DataTransfer.types` vacío**: WebKit/Tauri
  exponía ocasionalmente un array vacío durante `dragover`/`drop`.
  El helper `acceptsDragOver` rechazaba el arrastre, no se
  llamaba `preventDefault` y el navegador no disparaba `drop`.
- **dragleave entre hijos**: el highlight se limpiaba en cada
  cruce interno del cursor, perdiendo la transición visual.
- **drop**: cuando `drop` se disparaba, el parser previo usaba
  `JSON.parse` sobre `text/plain`, aceptando cualquier JSON con
  un campo `id`.
- **hidratación**: tras `dragstart → drop`, el padre obtenía las
  asociaciones del cache si estaban cargadas o consultaba
  `entry_collections` por bridge.
- **entry_collections_set**: el comando se invocaba con la lista
  fusionada; el backend re-attaches `Historial` y rechaza
  entradas inexistentes.
- **refresh**: tras la resolución exitosa, el padre llamaba
  `refreshOrganization`, `refreshEntryOrganization` y
  `refreshUnorganizedClearableCount`.
- **lectura posterior**: `clipvault_entry_collections` devolvía
  la lista persistida.

La causa raíz real fue estructural: el sidebar **instalaba
listeners en cada `<li>`** en lugar de en el viewport
scrolleable. En Tauri/WebKit ese patrón es frágil: cualquier
`box-shadow`, `outline` o `position: relative` añadido al
highlight, más los `<button>` hijos (selector de colección,
icono de eliminar), pueden interceptar el hit-test antes de
que el evento llegue a la fila. La corrección estructural es
una **zona de drop única y delegada en el `<ul
data-collections-drop-viewport>`**, con resolución de la fila
bajo el puntero mediante `event.target.closest` y
`document.elementFromPoint`. La causa secundaria (DataTransfer
vacío) se mantiene cubierta por la sesión interna que el card
abre en `dragstart` y el helper `acceptsDragOver` consulta en
`dragover`/`drop`.

### 8.2 Compatibilidad WebKit/Tauri aplicada

- MIME privado `application/x.clipvault-entry-id` sigue siendo
  el formato principal.
- Fallback `text/plain` versionado `clipvault-entry:v1:<integer-id>`
  que sólo transporta el entry id.
- **Sesión interna en memoria**: la card abre la sesión en
  `dragstart` con `beginDragSession(entry.id)`; el helper de la
  zona de drop la consulta en `dragover` y `drop` mediante
  `getActiveDragSessionEntryId()`. Sólo el arrastre que la card
  inició activa la sesión, así un drag externo (sin sesión)
  nunca impersona a una card interna.
- **Zona de drop única y delegada en el viewport**: el sidebar
  instala los cuatro eventos (`dragenter`, `dragover`,
  `dragleave`, `drop`) sólo en el `<ul data-collections-drop-viewport>`.
  Los `<li>` no llevan listeners; cada fila expone
  `data-drop-target="collection"` y un `data-collection-id`
  numérico. El helper resuelve la fila caminando `parentNode`
  desde `event.target` con fallback a
  `document.elementFromPoint`. Esto elimina la fragilidad ante
  `box-shadow`, `outline`, `<button>` hijos o cualquier
  overlay que WebKit/Tauri coloque sobre la fila.
- `dragover` ejecuta `preventDefault` cuando hay sesión activa
  incluso con `DataTransfer.types` vacío.
- `drop` intenta primero el MIME privado y luego el fallback
  `text/plain`; el parser (`parseDragTextPayload`) acepta
  únicamente el formato exacto `clipvault-entry:v1:` + dígitos
  y rechaza cualquier desviación.
- Nunca se transporta contenido del clipboard, snippets, hashes,
  `asset_ref`, rutas ni bytes de imagen por el DataTransfer.
- No se acepta automáticamente un drag externo como si fuera
  una card válida: cualquier payload que escape el formato
  reduce a `null` y la fila permanece inerte.
- La lista scrolleable se conserva: el `<ul>` sigue teniendo
  `overflow-y: auto` y altura fija igual al panel de cards. El
  scroll no es una limitación; la fragilidad era la dispersión
  de listeners.

### 8.3 Corrección del feedback visual

- Transición CSS local de 0.18s con easing `ease-out` sobre
  `background-color`, `border-color`, `box-shadow` y
  `outline-color` en `.collection-row.drop-target`.
- El estado `drag-over` cambia `background`, `border-color`,
  `outline` y `box-shadow` simultáneamente para que el highlight
  sea perceptible y se diferencie del estado `active`
  (0.28 vs 0.15 de fondo).
- El contraste cumple el mínimo necesario sobre el fondo
  `#161b22` y el outline está reforzado para que la fila se
  lea como una superficie accionable separada.
- El highlight se limpia en `dragleave` real, `drop`,
  `dragend`, cancelación con Escape (vía `OrganizationSidebar`'s
  document-level `dragend` listener), en errores y al destruir
  el componente.
- Una sola definición de `onWindowDragEnd` y un único par
  `addEventListener` / `removeEventListener` en
  `document` para evitar listeners duplicados.
- La regla defensiva
  `.collection-row.drop-target.system-collection.drag-over`
  suprime el highlight si por regresión la fila de sistema
  recibiese un `dragover`.
- El feedback también funciona cuando sólo está disponible el
  fallback `text/plain`, gracias a `isDragPayload` que acepta
  cualquiera de los dos tipos y a la sesión interna como tercer
  fallback.

### 8.4 Nuevo diseño de la chincheta

- El icono del favorito se rediseñó como una chincheta clásica
  de pizarrón, centrada en `viewBox="0 0 16 16"`:
  - **Cabeza**: `<circle cx="8" cy="5" r="3" />` (redonda /
    ligeramente abovedada).
  - **Cuerpo**: `<line x1="8" y1="8" x2="8" y2="12.5" />` (corto,
    vertical).
  - **Punta**: `<polygon points="6,12 10,12 8,14" />` (triangular,
    claramente visible).
- Dos variantes SVG locales:
  - **outline** (`is_pinned = false`): `circle` con
    `fill="none"`, `stroke-width="1.2"`; `line` con
    `stroke-width="1.2"`; `polygon` relleno.
  - **rellena** (`is_pinned = true`): `circle` con `fill`, `line`
    con `stroke-width="1.6"`; `polygon` relleno.
- El pin está visualmente centrado y tiene una silueta simple y
  limpia.
- No usa estrella, bookmark, check, corazón ni emoji.
- Conserva `is_pinned`, `setFavorite`, `aria-pressed`,
  Anclar/Desanclar, foco, hover y estado busy.

### 8.5 Aumento del icono de borrar

- El SVG del icono de borrar de cada colección pasó de
  `width="14" height="14"` a `width="18" height="18"` (≈130%
  del baseline anterior, manteniendo la base 14 como referencia).
- Se conserva el área clickable cómoda: el botón conserva su
  tamaño original de `1.65rem × 1.65rem` y el SVG se centra.
- Se conservan color danger (`var(--cv-danger)`), hover,
  focus-visible, `aria-label` y tooltip.
- Se mantiene la confirmación antes de eliminar la colección
  mediante el modal existente (`sidebar-delete-modal`).
- Este cambio no se aplica de forma que se pierda el nombre
  ni el botón de selección de la colección.

### 8.6 Sesión interna de drag-and-drop

- `beginDragSession(entryId)` se llama en `HistoryCard::onCardDragStart`
  tras escribir la dupla MIME privado + `text/plain`; devuelve
  un token opaco que se guarda en la card.
- `endDragSession(token)` se llama en `HistoryCard::onCardDragEnd`
  con el token, garantizando que un listener tardío (por ejemplo
  un `dragend` que llega después de iniciar un nuevo drag) no
  cierre la sesión equivocada.
- `acceptsDragOver` consulta `hasActiveDragSession()` cuando
  `DataTransfer` no aporta tipos utilizables, así la fila se
  vuelve un drop target válido incluso en WebKit/Tauri.
- `parseDragPayloadFromTransfer` recibe un tercer argumento
  opcional `sessionEntryId` que usa como fallback después del
  MIME privado y del `text/plain` estricto.
- `OrganizationSidebar::onWindowDragEnd` cierra la sesión
  incondicionalmente para que un cancel externo no deje
  residuo. El mismo handler también resetea el highlight visual.

### 8.7 Zona de drop única y delegada en el viewport

- Nuevo módulo `src/lib/collectionDropZone.ts` exporta el factory
  `createCollectionDropZoneHandlers({ collections, getViewport,
  getDragOverCollectionId, setDragOverCollectionId, onCardDrop,
  onForeignDrop })`. La factory devuelve los cuatro handlers
  `onDragEnter`, `onDragOver`, `onDragLeave`, `onDrop`.
- El sidebar (`OrganizationSidebar.svelte`) instala los cuatro
  handlers únicamente en el `<ul data-collections-drop-viewport>`,
  no en cada `<li>`. Cada fila lleva `data-drop-target="collection"`
  y `data-collection-id`.
- `resolveDropRowFromTarget(target, viewport, collections)`
  camina `parentNode` desde el `event.target` hasta el viewport,
  deteniéndose en el primer elemento con
  `data-drop-target="collection"` cuyo `data-collection-id`
  resuelve a una colección `user` que vive dentro del viewport.
- `resolveDropRowFromPoint(clientX, clientY, viewport, collections)`
  es el fallback: usa `document.elementFromPoint` cuando el
  walk-up falla (caso WebKit donde el `event.target` es el
  propio viewport).
- `dragenter` y `dragover` siempre llaman `preventDefault`
  cuando hay fila bajo el puntero y `acceptsDragOver` devuelve
  true. `dragover` además setea `dataTransfer.dropEffect = "copy"`.
- `dragleave` sólo limpia el highlight si el `relatedTarget` está
  fuera del viewport; mover el cursor entre hijos no lo apaga.
- `drop` resuelve el `entryId` por MIME privado → fallback
  `text/plain` → sesión activa, y rechaza payloads externos o
  inválidos.
- `endDragSession()` se llama siempre después de un `drop`
  exitoso y en el document-level `dragend` para que un drag
  cancelado no deje sesión colgando.

### 8.8 Tests agregados

Frontend (23 tests en
`tests/desktopDndCardVisualCorrections.integration.test.ts`):

- `dragstart → dragenter → dragover → drop on a non-first user row
  dispatches card-drop with the parsed entryId`.
- `drop on the delete button child still routes to the row`.
- `drop on the collection name span still routes to the row`.
- `WebKit/Tauri quirk: empty DataTransfer.types still triggers
  card-drop via the drag session`.
- `dragstart → drop on Historial is a safe no-op and never
  dispatches card-drop`.
- `foreign drag without an active session never dispatches
  card-drop`.
- `the highlight survives transitions between child elements
  inside the row`.
- `the highlight clears when the pointer leaves the viewport`.
- `document-level dragend closes the session and clears the
  highlight`.
- `repeated drop on the same target dispatches card-drop on every
  drop (idempotency lives in App.svelte)`.
- `repeated drop with an empty DataTransfer (WebKit quirk) only
  dispatches while the session is active`.
- `drop on the LAST user row in a long scrollable list still
  dispatches card-drop`.
- `drop on the row's empty padding space still routes to the
  row`.
- `drop in the viewport's empty space counts as foreign and never
  dispatches card-drop`.
- `text/plain-only fallback resolves the entry id and dispatches
  card-drop`.
- `parseDragPayloadFromTransfer prefers private MIME, falls back
  to text/plain, then session`.
- `endDragSession with a stale token does NOT clear the new
  session`.
- `a row missing data-collection-id is not a valid drop target`.
- `a row missing data-drop-target is not a valid drop target`.
- `only one document-level dragend listener is attached`.
- `OrganizationSidebar imports the delegated drop zone factory
  from the helper module`.
- `drop still resolves correctly after a row is replaced
  (scroll-during-drag)`.
- `the harness installs the production drop zone factory, not a
  hand-rolled copy`.
- `the production helper does not log or echo clipboard content`.

El harness de la integración importa directamente
`createCollectionDropZoneHandlers` desde
`src/lib/collectionDropZone.ts`. **No hay copia de handlers**:
los tests ejercen el mismo factory que el componente de
producción. Esto evita la fragilidad de la primera versión,
donde el harness copiaba los handlers a mano y podía derivar
del código real.

Frontend (tests actualizados en `desktopHeaderCardDnd.test.ts` y
`desktopDndCardVisualCorrections.test.ts`):

- `OrganizationSidebar installs a single delegated drop zone on the
  scrollable viewport` reemplaza el antiguo `OrganizationSidebar
  wires dragenter/dragover/dragleave/drop on user rows`.
- `OrganizationSidebar wires the delegated dragover to opt into
  drop when only the text/plain fallback is exposed` reemplaza el
  antiguo `OrganizationSidebar wires dragover to opt into drop
  when only the text/plain fallback is exposed`.
- `OrganizationSidebar drop reads both the private MIME and the
  text/plain fallback` ahora valida contra el helper
  `collectionDropZone.ts`.
- `OrganizationSidebar still refuses to wire the system Historial
  collection as a drop target` y `OrganizationSidebar rejects
  Historial as a drop target` validan la lógica
  `collection.kind !== "user"` del helper.
- `OrganizationSidebar dispatches card-drop with the entry id and
  the collection id` ahora exige el shape
  `{ entryId, collectionId }` literal.
- `App.svelte routes card-drop through combineMemberships +
  entryCollectionsSetCommand` sigue exigiendo `dropInFlight.has`
  para deduplicación.
- `HistoryCard pin control renders a minimalist local pushpin SVG`
  acepta `<polygon>` o `<polyline>` para la punta inferior.

### 8.9 Confirmación de no-regresiones

- No se reconstruye `EntryRecord` con objetos parciales en el
  drop; `applyPinUpdate` sigue usando
  `{ ...existing, is_pinned }` y los spreads del flujo de
  organización siguen completos.
- `asset_ref`, `mime_type`, `payload_width`, `payload_height`,
  `content_size`, `created_at`, `is_pinned`, tags y
  collections se conservan en cada operación.
- Las imágenes previamente guardadas aparecen después de
  reiniciar (`image_rows_survive_application_restart_with_payload_metadata_intact`,
  `image_entry_survives_drop_pin_unpin_and_organization_round`).
- Drag-and-drop no modifica ni revoca Blob URLs de imágenes:
  `assetResolver.release()` y `assetResolver.releaseFor(...)`
  sólo se invocan cuando cambia `assetRef` o en
  `onDestroy`.
- Hidratar tags o noiza no borra metadata de imágenes:
  `applyEntryOrganizationResults` sólo escribe
  `{ tags, collections }`.
- Pin/unpin no hace pruning del cache de organización
  (`applyPinUpdate` no toca `entryOrganization` ni
  `entryOrganizationHydration`).
- La card sigue mostrando loading, loaded y error correctamente
  para imágenes (`HistoryCard.thumbnailState`).
- Tags y colecciones persisten después de reiniciar
  (`drop survives a restart round-trip`).
- El sistema de "Historial" sigue conteniendo todas las
  capturas: el drop nunca envía `[]` y siempre re-añade la
  colección de sistema protegida.
- Se mantienen búsqueda, paste de imagen, paste plain, paste
  rich, retención, privacidad, blacklist, quick-paste y
  modales.
- No se escribe contenido de clipboard, snippets, hashes,
  rutas absolutas o bytes de imagen en logs, eventos, errores
  o DataTransfer.
- El refactor a zona de drop delegada no toca la lógica de
  `combineMemberships`, `parseDragPayloadFromTransfer`,
  `acceptsDragOver` ni el contrato de `entry_collections_set`.
  La deduplicación sigue en `App.svelte::handleCardDrop`
  mediante `dropInFlight` y `combineMemberships`.

## 9. Prueba manual concreta para macOS

Esta secuencia valida el flujo end-to-end fuera del runner
automatizado. Cada paso debe completarse antes del siguiente
para confirmar que la sesión interna, el highlight delegado y la
persistencia funcionan sobre el binario Tauri real:

1. Crear al menos 8 colecciones de usuario para forzar scroll
   vertical en el sidebar.
2. Crear una card de texto y una de imagen.
3. Arrastrar desde el cuerpo de la card, no desde sus botones.
   El highlight debe cambiar de color suavemente al pasar sobre
   la colección destino.
4. Pasar sobre una colección visible en la parte superior,
   media y final de la lista, y confirmar visualmente el
   cambio suave de color (transición 0.18s, fondo más saturado
   que el estado `active`).
5. Soltar sobre el nombre y sobre el espacio vacío de la fila.
   La card debe agregarse a la colección y el elemento
   `data-testid="drop-feedback"` debe mostrar "Captura agregada
   a la colección." durante ~2.4s.
6. Abrir la colección destino y confirmar que la card aparece.
7. Repetir con una card de imagen.
8. Reiniciar ClipVault y confirmar que la card, sus tags,
   memberships e imagen continúan tras el reinicio.
9. Confirmar que ningún arrastre externo (un archivo del
   Finder, un texto pegado desde otra app) modifica
   colecciones.

Esta secuencia sigue siendo **obligatoria** antes de cerrar el
cambio. La batería automatizada cubre el flujo lógico y la
resolución de la fila, pero la prueba manual con el binario
Tauri real sigue siendo la única manera de confirmar el
highlight visual y la persistencia tras reinicio.

## 10. Corrección aplicada por Codex: drop real en WebKit/Tauri

- [x] 10.1 Confirmar que el fallo no era una limitación de un
  `overflow-y: auto`: una fila visible dentro de un viewport
  scrolleable puede recibir `dragover` y `drop` normalmente.
- [x] 10.2 Corregir el panel de colecciones para usar la altura
  fija `--cv-card-rail-height`, con `min-height: 0` y un único
  viewport interno `overflow-y: auto`. El desktop no crece al
  agregar colecciones.
- [x] 10.3 Mantener la validación estricta del payload en `drop`,
  pero no bloquear el protocolo del navegador durante `dragover`
  cuando WebKit oculta `DataTransfer.types`. Una fila validada por
  geometría llama `preventDefault`; un arrastre externo sigue
  siendo un no-op y nunca llega a la mutación.
- [x] 10.4 Conservar el último target geométrico durante el drag
  para el caso en que WebKit entregue el `drop` sobre el viewport
  sin coordenadas útiles. El estado contiene sólo referencia DOM y
  colección, nunca contenido del clipboard.
- [x] 10.5 Abrir la sesión interna antes de acceder a
  `DataTransfer` y aislar la escritura del MIME privado y del
  fallback `text/plain`, de modo que una representación rechazada
  por WebKit no cancele la sesión completa.
- [x] 10.6 Añadir una defensa única de `dragover` en captura a nivel
  de `App.svelte` durante una sesión interna. Esto habilita el
  evento `drop` antes de la intervención de hijos, botones o del
  viewport; la resolución de target y la mutación siguen siendo
  responsabilidad de los handlers existentes.
- [x] 10.7 Mantener `CardDropText` como detector visual real:
  `preventDefault` se ejecuta para que el navegador entregue el
  evento, pero sólo una sesión/payload ClipVault cambia el estado
  visible y los arrastres externos no mutan nada.
- [x] 10.8 Actualizar las regresiones frontend para reflejar el
  contrato vigente de lista scrolleable y para comprobar que un
  payload externo nunca dispara `card-drop` aunque el navegador
  haya sido habilitado para entregar `drop`.
- [ ] 10.9 Repetir la prueba manual 9 en el binario Tauri de macOS.
  En particular, verificar visualmente el highlight de una fila
  visible después de desplazar el panel y comprobar que la card
  aparece en la colección después del drop. No marcar esta tarea
  como completada por los tests sintéticos.

## 11. Canal de puntero para WebKit/Tauri sin drag nativo

- [x] 11.1 Confirmar que el gesto de arrastre visual puede existir
  sin que WebKit entregue de forma confiable `dragstart`,
  `dragover` o `drop`; no usar la ausencia de `DataTransfer.types`
  como única señal del flujo interno.
- [x] 11.2 Añadir `pointerDragAndDrop.ts` con un controlador
  singleton, activación después de un desplazamiento mínimo,
  `pointerup`, `pointercancel` y pérdida de foco.
- [x] 11.3 Detectar la card fuente de forma delegada mediante
  `data-testid="history-card"` y `data-entry-id`, ignorando menú,
  pin, inputs y demás controles interactivos.
- [x] 11.4 Desactivar el drag HTML5 competidor en la card y usar el
  canal interno de puntero para que Tauri no dependa de
  `DataTransfer` ni del event loop de drag nativo.
- [x] 11.5 Resolver en cada movimiento el elemento bajo el puntero
  con `document.elementFromPoint`, incluyendo filas que quedaron
  visibles después de desplazar el viewport de colecciones.
- [x] 11.6 Emitir eventos internos metadata-only que sólo
  transporten `entryId` y coordenadas; ningún contenido del
  clipboard, asset, hash, ruta o byte de imagen atraviesa el canal.
- [x] 11.7 Conectar el mismo canal a la lista de colecciones y al
  feedback visual de la fila, manteniendo la lista como único lugar
  que ejecuta `entry_collections_set`; no se añade un recuadro
  inferior al desktop.
- [x] 11.8 Limpiar highlight, sesión y listeners en drop, cancel,
  pérdida de foco, destroy y remount; la instalación repetida es
  idempotente.
- [x] 11.9 Añadir tests de pointerdown → pointermove → pointerup,
  hit-test de fila scrolleable, callback de drop, arrastre externo
  inerte, cleanup y ausencia de duplicación de listeners.
- [ ] 11.10 Repetir la prueba manual 9 en un binario Tauri real de
  macOS. Confirmar que al arrastrar desde el cuerpo de una card la
  fila cambia de color y que el drop queda visible al cambiar de
  colección y reiniciar.

## 12. Feedback visible y prevención de selección durante el arrastre

- [x] 12.1 Crear una previsualización genérica y efímera para el fallback de
  puntero, sin copiar contenido de la captura, título, aplicación fuente,
  referencias de assets ni píxeles de imagen.
- [x] 12.2 Mantener la previsualización por encima de la interfaz con
  `pointer-events: none`, para que no intercepte el hit-test de colecciones
  scrolleables.
- [x] 12.3 Limpiar la previsualización en drop, cancelación, pérdida de foco,
  nuevo gesto y cleanup idempotente del controlador.
- [x] 12.4 Impedir la selección nativa de texto durante el gesto y conservar
  la exclusión de menú, pin, título y demás controles interactivos como
  fuentes de arrastre.
- [x] 12.5 Añadir regresiones frontend para ciclo de vida de la preview,
  `aria-hidden`, limpieza y `preventDefault` antes de superar el umbral.
- [ ] 12.6 Repetir la prueba manual 9 en un binario Tauri real de macOS:
  confirmar que la preview sigue al cursor, que no se seleccionan textos de
  cards vecinas y que el drop continúa funcionando sobre la colección.

## 13. Retirar feedback redundante del desktop

- [x] 13.1 Eliminar el banner verde de éxito posterior a un drop sin tocar la
  persistencia, el refresh ni los errores accionables.
- [x] 13.2 Retirar CardDropText del layout principal para eliminar el recuadro
  inferior y su texto de diagnóstico.
- [x] 13.3 Actualizar las regresiones frontend para comprobar la ausencia del
  banner y del componente visual inferior.
- [x] 13.4 Documentar que el highlight transitorio de la fila de colección y
  la preview flotante son el feedback visual del flujo.
- [ ] 13.5 Repetir la prueba manual 9 en un binario Tauri real de macOS:
  confirmar que no aparece banner ni recuadro inferior y que el drop sigue
  persistiendo en la colección.
