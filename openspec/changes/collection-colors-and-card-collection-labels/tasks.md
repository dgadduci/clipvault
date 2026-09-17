# Tareas: colores de colecciones y etiquetas en cards

## 1. Contratos y modelo

- [x] 1.1 Leer `project.md`, `AGENTS.md`, los specs base de
  `tags-and-collections` y `clipboard-history-cards`, y confirmar que el
  cambio no modifica otros cambios activos.
- [x] 1.2 Extender `Collection` Rust/TypeScript con `color_hex` normalizado y
  actualizar todos los DTOs, snapshots y fixtures que lo serializan.
- [x] 1.3 Definir la paleta base rojo `#c62828`, amarillo `#a16207`, verde
  `#2e7d32` y azul `#1565c0`, el validador `#rrggbb` opaco y el fallback de
  render seguro.

## 2. Persistencia y servicio de organización

- [x] 2.1 Agregar una migración SQLite aditiva, idempotente y reversible que
  persista `collections.color_hex` y asigne azul a `Historial` y a filas
  existentes sin color.
- [x] 2.2 Probar bootstrap desde una base anterior, migración repetida,
  rollback, foreign keys y conservación de nombres, timestamps y
  memberships.
- [x] 2.3 Implementar la selección aleatoria backend de la paleta para nuevas
  colecciones; permitir inyección/semilla en tests sin usar aleatoriedad de
  frontend.
- [x] 2.4 Implementar validación, normalización y persistencia de color en
  `OrganizationRepository`/`OrganizationService`, incluyendo actualización
  de `updated_at` y errores tipados.
- [x] 2.5 Permitir cambiar el color de `Historial` sin relajar su protección
  contra rename/delete ni alterar sus asociaciones.
- [x] 2.6 Probar que crear, cambiar o rechazar colores no modifica entries,
  tags, memberships, favoritos, source-app metadata ni assets.

## 3. Comandos Tauri y bridge

- [x] 3.1 Agregar un comando Tauri thin para actualizar color por
  `collection_id` y `color_hex`, devolviendo sólo la colección actualizada o
  un error tipado.
- [x] 3.2 Emitir `clipvault://organization-updated` con payload vacío después
  de un guardado exitoso y conservar el refresh idempotente existente.
- [x] 3.3 Actualizar el bridge TypeScript, tipos de comando, snapshot y tests
  metadata-only; no transportar clipboard, hashes, snippets, paths ni bytes.

## 4. Selector y sidebar

- [x] 4.1 Agregar `svelte-awesome-color-picker` v4 como dependencia runtime
  compatible con Svelte 5 y actualizar `package-lock.json` sin introducir
  otro package manager.
- [x] 4.2 Crear el modal de color reutilizando el shell de modales existente,
  con picker sin alpha, swatches de la paleta, HEX visible, Guardar,
  Cancelar, Escape, backdrop, foco inicial y foco de retorno.
- [x] 4.3 Mostrar un cuadrado de color al lado de cada colección de la
  sidebar, con `data-testid`, `data-collection-id`, `aria-label` y `title`.
- [x] 4.4 Implementar doble click, Enter y Space sobre el cuadrado sin que un
  click simple seleccione la fila ni que el control participe en drag/drop.
- [x] 4.5 Refrescar sidebar y cards sólo después de guardar con éxito; cancelar
  o fallar debe conservar el color anterior y no emitir actualización.

## 5. Etiquetas de colecciones en cards

- [x] 5.1 Renderizar las colecciones de usuario asignadas debajo de la fila
  actual de tags y excluir `Historial` de los chips inline. El modal de
  overflow queda especificado para mostrar la membership completa.
- [x] 5.2 Usar el `color_hex` individual como color de texto de cada etiqueta,
  mantener nombres completos en `aria-label` y aplicar fallback seguro sólo
  ante datos legacy inválidos.
- [x] 5.3 Renderizar inicialmente la fila de colecciones debajo de las tags,
  sin cambiar la geometría fija de las cards ni tocar previews, acciones o
  payloads de drag. La corrección del overflow y del estilo visual queda
  pendiente en las tareas 8.1–8.5 tras la validación manual.
- [x] 5.4 Verificar que el cambio de color actualiza todas las cards visibles
  asociadas sin duplicar listeners, requests ni refreshes.

## 6. Pruebas automáticas

- [x] 6.1 Tests de paleta inicial, validación HEX, actualización idempotente,
  valores inválidos y protección de `Historial`.
- [x] 6.2 Tests de migración desde base anterior, persistencia después de
  reiniciar y conservación de asociaciones.
- [x] 6.3 Tests de comandos/bridge para snapshot, set-color, errores y evento
  metadata-only.
- [x] 6.4 Tests frontend del cuadrado, doble click, teclado, modal,
  guardar/cancelar/Escape y actualización del color.
- [x] 6.5 Tests frontend iniciales de cards con cero, una y varias
  colecciones, colores distintos y nombres accesibles. Los tests del nuevo
  overflow/modal quedan en 8.5.
- [x] 6.6 Ejecutar las regresiones protegidas de drag and drop: singleton,
  fallback mouse, pointer capture/liberación, Escape/blur/pointercancel,
  controles interactivos, ghost y drop sobre colecciones scrolleables.

## 7. Verificación y cierre

- [x] 7.1 Ejecutar `cargo fmt --all -- --check`, tests Rust relevantes,
  `cargo clippy` relevante, `npm run check`, `npm run build` y `npm test`
  desde los directorios canónicos.
- [x] 7.2 Ejecutar `openspec validate collection-colors-and-card-collection-labels
  --strict --type change`, `git diff --check` y revisar el diff completo.
- [x] 7.3 Validar manualmente en Ubuntu GNOME Wayland: crear colección,
  cambiar color, reiniciar, revisar sidebar/cards y conservar memberships.
  - Resultado de la prueba manual: aprobado. El selector funciona dentro del
    WebView sin depender de APIs específicas de Wayland.
- [x] 7.4 Validar manualmente en Ubuntu X11 el mismo flujo sin APIs X11 para el
  picker.
  - Resultado de la prueba manual: aprobado. El flujo de color y las cards
    conserva el mismo comportamiento sin introducir dependencia de X11.
- [x] 7.5 Validar manualmente en macOS el mismo flujo en WKWebView.
  - Resultado de la prueba manual: aprobado. El picker, la persistencia y las
    etiquetas de cards funcionan en WKWebView.
- [x] 7.6 Confirmar que no se modifican ni eliminan `~/.clipvault` ni assets
  persistidos.
  - El reporte de implementación confirma que no se tocaron los assets
    persistidos ni `~/.clipvault`. `opsx-sync` y `opsx-archive` se mantienen
    fuera de esta verificación y requieren una orden posterior.

## 8. Correcciones derivadas de la validación manual

- [x] 8.1 Reemplazar el texto plano actual de las colecciones por chips con el
  mismo tratamiento visual que las tags: fondo, borde, radio, padding,
  tipografía compacta, truncamiento seguro y color de texto de la colección.
  - Implementado en `app/tauri/frontend/src/HistoryCard.svelte`: la fila
    `.collection-labels` (texto plano) se sustituye por `.collection-chips`
    con `.collection-chip` reutilizando fondo, borde, radio (`999px`),
    padding, `var(--cv-tag)`, `max-width`, `overflow: hidden`,
    `text-overflow: ellipsis` y `white-space: nowrap`. El color de texto
    sale del helper puro `collectionColor` (`src/lib/collectionColor.ts`).
  - Evidencia: `tests/historyCardCollectionChips.test.ts` ⇒
    *"HistoryCard renders each chip with the same visual treatment as
    tag-chip"*.
- [x] 8.2 Detectar overflow con el ancho real de la card mediante
  `ResizeObserver` o equivalente; no usar un límite fijo de cantidad.
  - `HistoryCard.svelte` instala un `ResizeObserver` durante `onMount`,
    `recomputeCollectionLayout()` mide los anchos reales desde la tira
    `data-collection-chip-measure` y compara
    `scrollWidth > clientWidth + 1`. El subconjunto visible sale del helper
    puro `computeVisibleCollections` (`src/lib/collectionChipLayout.ts`)
    que itera los anchos por chip y reserva `COLLECTION_OVERFLOW_BUTTON_PX`
    para el icono. El observer se desconecta en `onDestroy`.
  - Evidencia: `tests/collectionChipLayout.test.ts` cubre el cómputo puro
    (9 casos); `tests/historyCardCollectionChips.test.ts` ⇒
    *"HistoryCard uses a ResizeObserver to react to layout changes"*,
    *"HistoryCard computes overflow from scrollWidth and clientWidth"*,
    *"HistoryCard consults computeVisibleCollections to trim the chip
    subset"*.
- [x] 8.3 Cuando no entren todos los chips, mostrar un único botón con icono
  local de colecciones, nombre accesible, `aria-haspopup="dialog"`,
  `data-testid` y exclusión explícita de selección/drag.
  - `HistoryCard.svelte` renderiza el botón dentro de un `{#if
    collectionOverflow}` para que sólo aparezca cuando hay overflow real;
    desaparece cuando todas las caben. El botón declara `type="button"`,
    `:focus-visible { outline: 2px solid var(--cv-focus-ring, …) }`,
    `aria-haspopup="dialog"`, `aria-label` con el total de membresías,
    `title="Ver todas las colecciones"`,
    `data-testid="history-card-collections-overflow"` y `data-entry-id`.
    El icono es un `<svg>` local de tres barras apiladas — nunca un `+N`.
  - Exclusión de selección: `isInteractiveTarget` añade
    `target.closest("[data-testid='history-card-collections-overflow']")`
    y `target.closest(".collection-chips")`.
  - Exclusión de drag: el singleton `pointerDragAndDrop.ts` incorpora
    `"[data-testid='history-card-collections-overflow']"` en
    `INTERACTIVE_SELECTORS`. `tests/pointerDragAndDrop.test.ts` ⇒
    *"pointer and mouse paths ignore the collection overflow icon"*.
  - Evidencia: `tests/historyCardCollectionChips.test.ts` ⇒
    *"HistoryCard renders the overflow icon only when overflow is true"*,
    *"HistoryCard overflow icon declares the documented accessibility
    attributes"*, *"HistoryCard overflow icon carries a focus-visible
    outline"*, *"HistoryCard isInteractiveTarget treats the overflow icon
    as interactive"*, *"HistoryCard overflow icon carries a local
    collections glyph (no fallback)"*, *"pointerDragAndDrop.ts registers
    the overflow icon as an interactive selector"*.
- [x] 8.4 Crear un modal informativo por captura que liste todas las
  colecciones asignadas, incluida `Historial`, con chips completos y colores,
  usando el shell Modal existente, foco de retorno, Escape, backdrop, cierre y
  scroll interno si es necesario. No debe permitir editar memberships.
  - Nuevo componente `app/tauri/frontend/src/CollectionMembershipModal.svelte`
    que reutiliza `Modal.svelte` (focus trap, Escape, backdrop, retorno de
    foco). `orderedCollections` mantiene `Historial` (system) primero y
    ordena las user por nombre. Cada chip pinta el swatch y el texto con
    `collection.color_hex` validado vía `collectionColor`. Lista
    `overflow-y: auto` con `max-height: min(50vh, 22rem)`. El modal es
    read-only: ningún comando Tauri de mutación se invoca.
  - Apertura y retorno de foco: `HistoryCard.svelte` monta el modal con
    `open={membershipModalOpen}`, `returnFocusTo={collectionOverflowEl}` y
    `on:close={() => { membershipModalOpen = false; }}`. El click del icono
    sólo dispara `openMembershipModal()`.
  - Evidencia: `tests/historyCardCollectionChips.test.ts` ⇒
    *"CollectionMembershipModal reuses the shared Modal shell"*,
    *"CollectionMembershipModal lists every assigned collection including
    Historial"*, *"CollectionMembershipModal is read-only — no save or
    delete affordances"*, *"CollectionMembershipModal payload carries no
    clipboard / hash / path / asset data"*, *"CollectionMembershipModal
    wires aria-labelledby through a stable title id"*,
    *"CollectionMembershipModal returns focus to the overflow icon on
    close"*, *"CollectionMembershipModal applies the collection's colour
    to each chip"*.
- [x] 8.5 Agregar tests para chips con borde/fondo, medición responsive,
  aparición/ausencia del icono, apertura del modal, listado completo,
  accesibilidad y no regresión de selección/drag-and-drop.
  - `tests/collectionChipLayout.test.ts` (helper puro):
    `computeVisibleCollections` cubre el caso sin overflow, trim por
    overflow, gap entre chips, reserva del botón, filas muy estrechas,
    lista vacía, overflow con fila ancha, arrays cortos y las constantes
    del layout.
  - `tests/historyCardCollectionChips.test.ts` (source-level sobre
    `HistoryCard.svelte`, `CollectionMembershipModal.svelte`,
    `Modal.svelte` y `lib/pointerDragAndDrop.ts`): chips con mismo
    tratamiento que tags, exclusión de `Historial`, fila oculta cuando
    sólo hay `Historial`, overflow sólo cuando hay overflow real,
    atributos del icono, focus-visible, exclusión de selección/drag,
    icono local sin fallback, `ResizeObserver` + `scrollWidth`/
    `clientWidth`, helper `computeVisibleCollections`, modal reutiliza
    `Modal`, lista con `Historial`, modal read-only, modal sin payload
    sensible, `aria-labelledby` vía `titleId` estable por entry id,
    retorno de foco, color por colección, baselines protegidos
    (`data-testid`, `data-entry-id`, `draggable="false"` y singleton de
    `pointerDragAndDrop.ts`).
  - `tests/pointerDragAndDrop.test.ts`: nuevo test *"pointer and mouse
    paths ignore the collection overflow icon"* verifica que `pointerdown`
    y `mousedown` sobre el botón con `data-testid="history-card-collections-overflow"`
    no inician sesión de drag ni cambian `defaultPrevented`.
- [x] 8.6 Ejecutar nuevamente los checks frontend y las regresiones protegidas
  de drag and drop; actualizar esta sección con evidencia antes de cerrar el
  cambio.
  - `cd app/tauri/frontend && npm run check` ⇒
    *"svelte-check found 0 errors and 16 warnings in 10 files"* (los
    warnings son preexistentes en `TagSelectorModal.svelte`,
    `CollectionSelectorModal.svelte`, `PrivacyModal.svelte`, etc. y no
    son introducidos por esta corrección).
  - `cd app/tauri/frontend && npm run build` ⇒ build limpio, `vite build`
    produce `dist/index.html`, `dist/quick-paste.html`,
    `dist/assets/main-qPf49C43.js`, `dist/assets/quick-paste-DdEcA_ex.js`
    y el bundle de `ClipboardPreview` sin warnings nuevos.
  - `cd app/tauri/frontend && npm test` ⇒ 1265 tests, 1265 pass, 0 fail
    (incluye las nuevas suites `tests/collectionChipLayout.test.ts` y
    `tests/historyCardCollectionChips.test.ts` y el test del icono de
    overflow en `tests/pointerDragAndDrop.test.ts`).
  - `cargo fmt --all -- --check` ⇒ limpio (sin diff).
  - `openspec validate collection-colors-and-card-collection-labels
    --strict --type change` ⇒ *"Change
    'collection-colors-and-card-collection-labels' is valid"*.
  - `git diff --check` ⇒ sin warnings de whitespace.
## 9. Ajuste posterior: chip `+N` para el overflow

Las tareas 8.1–8.6 documentan la primera implementación validada con un
icono de overflow. El requisito actual de producto reemplaza únicamente ese
indicador visual por un chip `+N`; el modal, el cálculo responsive y los
baselines se conservan.

- [x] 9.1 Reemplazar el icono visible de overflow por un chip interactivo
  `+N`, siguiendo el patrón visual de `.tag-chip.more`; conservar la apertura
  del `CollectionMembershipModal` y el mismo
  `data-testid="history-card-collections-overflow"` para no romper los
  contratos existentes.
  - `HistoryCard.svelte` sustituye el `<svg>` de tres barras por el texto
    `{collectionOverflowLabel}` derivado de la suma exacta de membresías
    ocultas; el botón declara
    `class="tag-chip more collection-overflow"` para heredar el chrome de
    `.tag-chip.more` y reutiliza los mismos atributos (`type="button"`,
    `data-testid`, `data-entry-id`, `aria-haspopup`, `aria-label`,
    `title`, `bind:this={collectionOverflowEl}`).
  - `openMembershipModal` se conserva como único disparador del
    `CollectionMembershipModal`.
  - Evidencia: `tests/historyCardCollectionChips.test.ts` ⇒
    *"HistoryCard overflow chip declares the documented accessibility
    attributes"*, *"HistoryCard overflow chip reuses the tag-chip.more
    visual treatment"*, *"HistoryCard renders the +N overflow chip only
    when overflow is true"*.
- [x] 9.2 Calcular `N` como la cantidad exacta de colecciones de usuario
  ocultas por falta de ancho. `Historial` no se muestra inline ni se cuenta en
  `+N`; el modal continúa listando todas las memberships, incluida
  `Historial`.
  - `computeOverflowCount(userCollections, visibleUserCollections)` añadido
    a `lib/collectionChipLayout.ts`; clampado a `Math.max(0, …)` para que
    una combinación stale nunca emita un conteo negativo.
  - El chip se renderiza sólo cuando
    `{#if collectionOverflow && collectionOverflowCount > 0}`.
  - `userCollections` se sigue derivando como
    `assignedCollections.filter((c) => c.kind !== "system")` así que
    `Historial` jamás entra al cómputo inline; el modal sigue iterando
    `orderedCollections` (sistema primero, usuarios ordenados por nombre).
  - Evidencia: `tests/collectionChipLayout.test.ts` ⇒
    *"computeOverflowCount returns the number of hidden user collections"*,
    *"computeOverflowCount returns zero when every collection is visible"*,
    *"computeOverflowCount never reports a negative count"*;
    `tests/historyCardCollectionChips.test.ts` ⇒ *"HistoryCard derives
    the +N count from computeOverflowCount over user collections only"*,
    *"HistoryCard overflow chip text never falls back to a fixed
    membership count"*.
- [x] 9.3 Medir el ancho real del chip `+N` al calcular el subconjunto visible,
  actualizarlo ante `ResizeObserver` y no usar límites fijos de cantidad.
  Mantener foco visible, `aria-haspopup="dialog"`, `aria-label`, `title`,
  exclusión de selección/drag y retorno de foco al chip.
  - El measurement strip añade un span
    `[data-collection-overflow-measure-item]` con el peor caso
    (`+{userCollections.length}`) y `measureCollectionChipWidths` guarda
    su `getBoundingClientRect().width` en `collectionOverflowChipWidth`.
  - `computeVisibleCollections` recibe ese ancho como quinto argumento
    (`overflowButtonWidth`) y cae al
    `COLLECTION_OVERFLOW_BUTTON_PX` documentado cuando la medición es 0
    (primer render antes del `ResizeObserver`).
  - `recomputeCollectionLayout` se sigue invocando desde el
    `ResizeObserver`, así que cualquier cambio de viewport o de `--cv-card-size`
    recalcula el subset visible con la reserva medida.
  - El chip conserva `type="button"`, `:focus-visible { outline: 2px solid
    var(--cv-focus-ring, …) }`, `aria-haspopup="dialog"`, `aria-label`,
    `title` y la entrada al guard `isInteractiveTarget`; el modal sigue
    restaurando foco a `collectionOverflowEl` con
    `returnFocusTo={collectionOverflowEl}`.
  - Evidencia: `tests/historyCardCollectionChips.test.ts` ⇒ *"HistoryCard
    feeds the measured overflow chip width back into the visible subset"*;
    `tests/collectionChipLayout.test.ts` ⇒ *"computeVisibleCollections
    accepts the measured overflow chip width and trims accordingly"*,
    *"computeVisibleCollections falls back to the documented reservation
    when no chip width is supplied"*, *"computeVisibleCollections grows
    the visible subset when the measured chip shrinks"*.
- [x] 9.4 Actualizar los tests de layout, HistoryCard y pointer drag para
  verificar ausencia de overflow, valores `+1`/`+2`, cambio tras resize,
  click que abre el modal y preservación de los baselines protegidos.
  - `tests/collectionChipLayout.test.ts` añade cobertura para el
    parámetro `overflowButtonWidth`, el fallback a
    `COLLECTION_OVERFLOW_BUTTON_PX`, el helper `computeOverflowCount` y
    la sensibilidad del subset ante una reserva medida que cambia.
  - `tests/historyCardCollectionChips.test.ts` reemplaza los tests del
    icono SVG por cobertura del chip `+N`: ausencia de overflow, atributos
    de accesibilidad, estilo `.tag-chip.more`, dependencia del helper
    `computeOverflowCount`, ida y vuelta del ancho medido, ausencia de
    umbral hardcodeado y preservación de los baselines protegidos
    (`data-testid="history-card"`, `data-entry-id`, `draggable="false"`,
    singletons de `pointerDragAndDrop.ts`, payload con sólo el id opaco).
  - `tests/pointerDragAndDrop.test.ts` mantiene el test *"pointer and mouse
    paths ignore the collection overflow icon"* con el mismo selector y
    atributo, así que la sustitución del icono SVG por el chip `+N` no
    introduce regresiones en el guard de selección ni en el flujo de
    drag-and-drop.
- [x] 9.5 Ejecutar nuevamente `npm run check`, `npm run build`, `npm test`,
  `openspec validate collection-colors-and-card-collection-labels
  --strict --type change` y `git diff --check`; registrar evidencia y no
  sincronizar/archivear todavía.
  - `cd app/tauri/frontend && npm run check` ⇒
    *"svelte-check found 0 errors and 16 warnings in 10 files"* (los
    warnings son preexistentes en `TagSelectorModal.svelte`,
    `CollectionSelectorModal.svelte`, `PrivacyModal.svelte`, etc. y no
    son introducidos por esta corrección).
  - `cd app/tauri/frontend && npm run build` ⇒ build limpio, `vite build`
    produce `dist/index.html`, `dist/quick-paste.html`,
    `dist/assets/main-BsXoe9C4.js`,
    `dist/assets/quick-paste-DdEcA_ex.js`,
    `dist/assets/ClipboardPreview-DJclYJOz.js` y los CSS asociados sin
    warnings nuevos.
  - `cd app/tauri/frontend && npm test` ⇒ 1275 tests, 1275 pass, 0 fail
    (incluye los nuevos casos de `tests/collectionChipLayout.test.ts`
    para `overflowButtonWidth` y `computeOverflowCount`, las nuevas
    aserciones de `tests/historyCardCollectionChips.test.ts` para el
    chip `+N`, y el test preexistente de
    `tests/pointerDragAndDrop.test.ts` para el guard del botón).
  - `openspec validate collection-colors-and-card-collection-labels
    --strict --type change` ⇒ *"Change
    'collection-colors-and-card-collection-labels' is valid"*.
  - `git diff --check` ⇒ sin warnings de whitespace.
## 10. Corrección bloqueante: overflow real del `+N` en runtime

La prueba manual inicial detectó que una card con colecciones que superaba su
ancho no mostraba `+N`. La causa fue que la tira de medición era hermana de la
fila visible, pero se buscaba desde `.collection-chips`, dejando los anchos en
cero. Esta sección documenta la corrección y la verificación final.

- [x] 10.1 Reproducir el fallo en una card con suficientes colecciones y
  confirmar que el `+N` no aparecía. La reproducción identificó el lookup
  relativo incorrecto y no modificó assets persistidos ni `~/.clipvault`.
- [x] 10.2 Corregir la medición en `HistoryCard.svelte`: se agregó
  `bind:this={collectionMeasureStripEl}` sobre la tira hermana, se eliminó el
  atributo engañoso de la fila visible y los anchos se leen directamente desde
  la tira completa. La detección ahora compara el ancho intrínseco total con
  el ancho disponible y reserva el ancho medido de `+N`.
- [x] 10.3 Garantizar la convergencia después de montar/hidratar el DOM y ante
  cambios de memberships o resize. Se agregó
  `intrinsicCollectionRowWidth`, tolerancia de 1 px y se conserva
  `ResizeObserver`/cleanup; `Historial` queda fuera del conteo.
- [x] 10.4 Agregar regresiones cercanas al layout real en
  `tests/historyCardCollectionLayout.test.ts` (12 casos), además de cuatro
  casos de `intrinsicCollectionRowWidth` y la actualización del test de
  `HistoryCard`. Se cubren tira hermana, primer render, `+1`/`+2`, resize,
  ausencia de overflow y baselines de drag-and-drop.
- [x] 10.5 Verificar el cambio completo:
  - `npm run check`: 0 errores y 16 warnings preexistentes.
  - `npm run build`: limpio, sin warnings nuevos.
  - `npm test`: 1291/1291 pass.
  - Regresiones drag-and-drop: `pointerDragAndDrop` 19/19 y correcciones de
    desktop 54/54.
  - `cargo build --bin clipvault-app`: correcto.
  - `openspec validate collection-colors-and-card-collection-labels --strict
    --type change`: válido.
  - `git diff --check`: limpio.
  - Smoke headless con el DOM/CSS bundleados: entry 348 muestra `+5` y entry
    349 no muestra chip. La prueba manual final fue aprobada. La captura
    nativa del WebView quedó limitada por la ausencia de
    `wlr-screencopy-unstable-v1` en el compositor Wayland; no es un fallo
    funcional.
  - `opsx-sync` y `opsx-archive` no se ejecutaron.
