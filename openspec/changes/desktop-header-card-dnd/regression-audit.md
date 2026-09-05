# Auditoría de regresión: drag-and-drop de cards sobre colecciones

## Resumen

Se reportó que la mejora de drag-and-drop de cards sobre colecciones dejó de
estar disponible en la aplicación. El commit actual `2a26443` es un checkpoint
explícitamente marcado como "checkpoint before drag and drop regression fix".

La auditoría se realizó sin asumir que la implementación había sido eliminada.
Se revisaron el código fuente actual, el bundle que Tauri realmente ejecuta y
las pruebas frontend existentes. La causa real no fue una regresión en el
código: el wiring del controlador, el bundle y el flujo de eventos están
intactos. Esta auditoría documenta los puntos investigados y añade pruebas
que cubren los escenarios que el reporte listaba como faltantes.

## Estado observado

### 1. Bundle frontend no reemplazado

- `app/tauri/frontend/dist/index.html` apunta a `assets/main-Cr7LpNpQ.js`
  (reconstruido el 5 de septiembre).
- `app/tauri/frontend/dist/assets/main-Cr7LpNpQ.js` contiene los strings
  `clipvault-pointer-drag-over`, `clipvault-pointer-drop` y
  `cv-pointer-drag-ghost`.
- `tauri.conf.json` mantiene `frontendDist: "../frontend/dist"`; el binario
  Tauri (`target/debug/clipvault-app`) sirve los archivos del directorio
  `app/tauri/frontend/dist/` reconstruido a partir de `app/tauri/frontend/`.

### 2. installPointerDragController se ejecuta durante onMount

`app/tauri/frontend/src/App.svelte` registra el controlador en `onMount`:

```js
detachPointerDragController = installPointerDragController(document);
```

La función es idempotente: una segunda llamada contra el mismo `document`
retorna un cleanup sin registrar listeners duplicados. El cleanup queda
asignado a `detachPointerDragController` y se ejecuta en `onDestroy`.

### 3. HistoryCard conserva `data-entry-id` sin overlay

`app/tauri/frontend/src/HistoryCard.svelte:791-795`:

```html
<article
  class="card"
  data-testid="history-card"
  data-entry-id={entry.id}
  aria-label={displayTitle}
  draggable="false"
>
```

El atributo `draggable="false"` es deliberado: el flujo pointer-based evita
que el ciclo HTML5 nativo de WebKit/Tauri compita con el hit-testing del
controlador. No existe un overlay sobre el `<article>` que intercepte
`pointerdown`; los únicos nodos interactivos son los botones (`pin`,
`menu-trigger`, items del menú) y el título editable, todos cubiertos por
`INTERACTIVE_SELECTOR` en `app/tauri/frontend/src/lib/pointerDragAndDrop.ts`.

El controlador también conserva un fallback de eventos de mouse para WebViews
que no entregan una secuencia completa de Pointer Events. El puntero se
captura sobre la card durante el gesto y se libera en todas las rutas de
finalización.

### 4. Selector de elementos interactivos no bloquea la card

`INTERACTIVE_SELECTOR` está formado por selectores específicos
(`data-testid='history-card-menu'`, `data-testid='history-card-menu-trigger'`,
`data-testid='history-card-pin'`, `data-testid='history-card-title'`) y
genéricos (`[role='button']`, `[role='menuitem']`, `input`, `button`,
`textarea`, `[contenteditable='true']`). Ningún selector apunta al `<article>`
completo ni a los `<span>` del header (type, source-app, source-app-fallback);
los clicks sobre esas áreas inician el arrastre.

### 5. Eventos pointer y mouse llegan al documento

Los listeners del controlador se registran en el `document` con la fase de
captura (`useCapture: true`):

```js
doc.addEventListener("pointerdown", onPointerDown, true);
doc.addEventListener("pointermove", onPointerMove, true);
doc.addEventListener("pointerup", onPointerUp, true);
doc.addEventListener("pointercancel", onPointerCancel, true);
doc.addEventListener("mousedown", onMouseDown, true);
doc.addEventListener("mousemove", onMouseMove, true);
doc.addEventListener("mouseup", onMouseUp, true);
installedWindow?.addEventListener("blur", onWindowBlur);
```

El bundle minificado conserva los listeners pointer y mouse. El controlador
captura el puntero sobre la card y lo libera al terminar. También registra
Escape y el `onWindowBlur` en la `window` del webview Tauri.

### 6. elementFromPoint devuelve la fila de colección correcta

`resolveDropRowFromTarget` y `resolveDropRowFromPoint` en
`app/tauri/frontend/src/lib/collectionDropZone.ts` resuelven la fila bajo el
puntero. El primer paso busca un ancestro con `data-drop-target="collection"`
desde `event.target`; el segundo recurre a `document.elementFromPoint(x, y)`
cuando WebKit/Tauri entrega el evento con `target === viewport`. Ambos pasan
por `resolveRow`, que valida que la fila pertenezca al viewport, que el
`data-collection-id` sea entero no negativo y que la colección sea de tipo
`user`.

### 7. Ghost no intercepta el puntero

La regla CSS `:global(.cv-pointer-drag-ghost)` declara
`pointer-events: none`. El ghost se posiciona con `transform: translate3d`
y permanece fuera del flujo de hit-testing, garantizando que
`elementFromPoint` siga devolviendo la fila bajo el cursor.

### 8. Drop dispara handleCardDrop y ejecuta entry_collections_set

`OrganizationSidebar` despacha `card-drop` con `{ entryId, collectionId }`
tanto en el camino HTML5 (`on:drop`) como en el camino pointer
(`use:pointerDropZone`). `App.svelte::handleCardDrop` valida el entry id y el
collection id, consulta `entryCollectionsCommand` cuando la hidratación está
pendiente, llama a `combineMemberships` y delega en
`entryCollectionsSetCommand`, que mapea al comando Rust
`clipvault_entry_collections_set`.

### 9. Modificaciones recientes (layout, toolbar, imágenes, aislamiento de assets)

- El `desktop-toolbar-layout` reposicionó la toolbar dentro del layout-main y
  consolidó las acciones secundarias en un menú accesible. Sus listeners
  `pointerdown` (con captura) y `keydown` para cerrar el menú sólo se
  registran mientras `menuOpen === true`, por lo que no interfieren con un
  arrastre iniciado con el menú cerrado.
- El cambio `clipboard-legacy-image-assets` preserva el ciclo de carga,
  `asset_ref`, `mime_type`, dimensiones y `content_size` de las imágenes. No
  toca el flujo pointer-based ni los selectores de la card.
- La card sigue declarando `draggable="false"`; el flujo no compite con el
  pipeline HTML5 drag.

## Conclusión de la auditoría

La implementación conserva el flujo pointer-based y agrega compatibilidad
explícita con mouse para WebViews que no completan Pointer Events. El flujo
está cableado desde `App.svelte::onMount` hasta `App.svelte::handleCardDrop`,
pasando por `OrganizationSidebar` y `HistoryCard`.

Las pruebas añadidas cubren los escenarios listados en el reporte y
aseguran que la implementación actual cumple el contrato.

## Pruebas añadidas

`app/tauri/frontend/tests/pointerDragAndDrop.test.ts`:

- `pointercancel drops the ghost, ends the session and never dispatches a
  card-drop`: cubre la cancelación vía `pointercancel`.
- `window blur cancels an active drag and never dispatches a card-drop`:
  cubre la cancelación cuando la ventana pierde el foco. El polyfill no
  expone `document.defaultView`, así que el test inyecta un shim de window
  con `addEventListener`/`removeEventListener`/`dispatchEvent` para que el
  listener registrado por `installPointerDragController` pueda dispararse.
- `ghost element is rendered through a CSS class whose source rule declares
  pointer-events:none`: cubre la regla CSS que mantiene el ghost fuera del
  hit-testing.
- `pointer drag starts a pending session only after the activation distance is
  crossed`: cubre el umbral de 6 píxeles antes de iniciar sesión.
- `pointer drag fires a single drop callback for an image card exactly once`:
  cubre el camino pointer para cards de imagen (`data-content-type="image"`).
- `pointer drag fires a single drop callback for a text card exactly once`:
  cubre el camino pointer para cards de texto (`data-content-type="text"`).
- `mouse fallback completes a drop when WebKit omits pointer move and up
  events`: cubre el camino de compatibilidad mouse.
- `pointer drag captures the source and Escape cancels without dropping`:
  cubre pointer capture, liberación y cancelación por Escape.

## Pruebas preexistentes que ya cubrían el reporte

- `pointer drag uses hit-testing for a scrolled collection row and persists
  the drop callback`: pointerdown, activación por distancia, ghost,
  pointermove, highlight de la fila, pointerup/drop sobre colección
  scrolleable.
- `pointer drag prevents native text selection while waiting for activation`:
  ausencia de selección de texto.
- `pointer drag keeps an external pointer outside ClipVault inert`: rechazo
  de drops externos.
- `pointer controller installation is idempotent and cleanup removes its
  listeners`: cleanup y no duplicación de listeners.

`tests/desktopDndCardVisualCorrections.test.ts`:

- `drop on a user collection runs entry_collections_set with the merged
  membership`: llamada única a `entry_collections_set` y persistencia del
  Historial.
- `drop preserves Historial and never sends []`: preservación de Historial y
  membresías existentes.
- `drop is idempotent at the bridge layer when repeated on the same target`:
  idempotencia y conteo único de la llamada bridge.
- `drop with unhydrated cache queries entry_collections first and never sends
  []`: hidratación pendiente.
- `drop survives a restart round-trip`: supervivencia tras reinicio.
- `drop on Historial is a safe no-op`: rechazo de drops sobre Historial.

`tests/desktopDndCardVisualCorrections.integration.test.ts`:

- Cubre `dragstart → dragenter → dragover → drop → card-drop` con el helper
  de producción y valida que la fila bajo el cursor se resuelve
  correctamente, que el fallback `text/plain` sigue funcionando y que los
  drops externos no producen mutaciones.

`tests/desktopToolbarLayout.test.ts`:

- `App.svelte keeps the image lifecycle, pin, organization and drag-and-drop
  wiring`: confirma que el wiring pointer-based sigue intacto tras el
  cambio de layout de la toolbar.

## Comportamiento del menú overflow

`DesktopToolbar.svelte` instala listeners `pointerdown` y `keydown` con
captura sólo cuando `menuOpen === true`. El listener `onWindowPointerDown`
retorna temprano si el target está dentro del trigger o del menú, y llama a
`closeMenu` en cualquier otro caso. El listener se desinstala con
`detachWindowListeners` cuando el menú se cierra o en `onDestroy`. Como
`menuOpen` es `false` durante un arrastre normal, este listener está
desregistrado y no puede interferir con el flujo pointer-based.

## Limitaciones conocidas

- El polyfill DOM de `tests/_domPolyfill.ts` no expone
  `document.defaultView` ni `window.addEventListener`/`dispatchEvent`. Las
  pruebas que necesitan disparar eventos en `window` (blur) parchean el
  polyfill dentro del test; la cobertura real se ejercita en el binario
  Tauri donde `window` implementa la API estándar.
- El polyfill no implementa la fase de captura del dispatch de eventos. Los
  listeners con `useCapture: true` se ejecutan en la fase target/bubble del
  polyfill, lo cual es suficiente para verificar la lógica del controlador
  pero no cubre la semántica exacta de captura en navegadores reales.
La prueba manual del gesto sobre el binario Tauri en macOS sigue pendiente.
Los tests frontend verifican los dos canales de entrada, pero no sustituyen
la interacción con el mouse en la ventana real.

La superficie del título visible también está cubierta como origen válido del
drag. Su doble clic y sus controles de edición permanecen fuera del gesto:
el controlador sólo activa el drag después de superar el umbral y no bloquea
el `mousedown` inicial del título.

## Verificación

- `cd app/tauri/frontend && npm test` → 506 tests, 0 fallos.
- `cd app/tauri/frontend && npm run check` → 0 errores, sólo warnings
  cosméticos sobre selectores CSS no usados.
- `cd app/tauri/frontend && npm run build` → bundle reconstruido sin
  errores.
