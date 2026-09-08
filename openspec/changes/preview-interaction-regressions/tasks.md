# Tareas de implementación: preview-interaction-regressions

## 1. Relevamiento y baseline

- [x] 1.1 Leer `project.md`, `AGENTS.md`, este cambio, las specs vigentes de
  `clipboard-rich-text`, `clipboard-history-cards`, `quick-paste`,
  `desktop-header-card-dnd` y los cambios archivados de preview.
- [x] 1.2 Inspeccionar `ClipboardPreview.svelte`, `QuickPaste.svelte`,
  `HistoryCard.svelte`, `HistoryCardRail.svelte`, `App.svelte`, los helpers de
  atajos/orden y el bridge de preview.
- [x] 1.3 Ejecutar `git status --short` y `git diff --check`; preservar todos
  los cambios de usuario y las regresiones protegidas.
- [x] 1.4 Confirmar con tests la causa exacta de cada regresión antes de
  modificar código: whitespace rich, orden Quick Paste, clipping del menú,
  ausencia de selección e hints de atajo.

## 2. Preview rich text

- [x] 2.1 Corregir el renderer compartido para conservar `LF`, `CRLF`, tabs,
  indentación y líneas vacías sin aplicar `trim` o colapsar whitespace.
- [x] 2.2 Mantener el formato rich permitido y la sanitización existente; no
  cargar HTML/RTF original como contenido activo ni persistir HTML generado.
- [x] 2.3 Mantener el fallback plain text con whitespace preservado cuando el
  asset rich está ausente, inválido o cargando.
- [x] 2.4 Verificar que Desktop y Quick Paste consumen la misma implementación
  y que imágenes, código, rich paste y cards no cambian su contrato.

## 3. Orden de Quick Paste

- [x] 3.1 Corregir el helper puro de ids recientes para conservar favoritos
  primero y ordenar cada grupo por `created_at DESC, id DESC`.
- [x] 3.2 Mantener intacto el ranking de resultados de búsqueda.
- [x] 3.3 Reaplicar el orden después de carga, `history-updated`, hidratación,
  pin/unpin y limpieza de query sin mutar timestamps ni crear capturas.
- [x] 3.4 Mantener la selección por id cuando un pin/unpin cambia una entrada
  de grupo.

## 4. Selección de cards Desktop

- [x] 4.1 Agregar un único `selectedEntryId` local al propietario de la rail y
  propagarlo a `HistoryCard` sin persistirlo.
- [x] 4.2 Implementar click de superficie, cambio de card, segundo click para
  deseleccionar y Escape para limpiar.
- [x] 4.3 Mantener aislados pin, menú, título, tags, colecciones, paste,
  delete y drag-and-drop; no llamar al backend para seleccionar.
- [x] 4.4 Exponer estado accesible y estable con `aria-selected`, estilo
  visible y testids; limpiar una selección cuyo entry deja de estar visible.
- [x] 4.5 Mostrar sólo en la card seleccionada `Previsualizar · ⌘↵` o
  `Previsualizar · Ctrl↵` mediante los helpers existentes.
- [x] 4.6 Hacer que `Cmd/Ctrl+Enter` abra el preview de la card seleccionada y
  respete exclusión de inputs, editores y menús.

## 5. Menú completo y atajos

- [x] 5.1 Sacar el popover del contexto que lo recorta o implementar una capa
  equivalente posicionada con `getBoundingClientRect` y límites del viewport.
- [x] 5.2 Reposicionar arriba/abajo y lateralmente según el espacio; aplicar
  scroll interno sólo al menú cuando sea estrictamente necesario.
- [x] 5.3 Mantener una sola instancia de menú, cleanup idempotente, Escape y
  click exterior sin listeners duplicados.
- [x] 5.4 Mostrar el shortcut real de `Previsualizar` y de cualquier otra
  acción que tenga uno, con `aria-keyshortcuts`; no inventar shortcuts.
- [x] 5.5 Confirmar que abrir o desplazar el menú no inicia drag ni cambia la
  selección de texto.

## 6. Tests de regresión

- [x] 6.1 Tests del preview rich para saltos, tabs, líneas vacías,
  indentación, fallback y sanitización.
- [x] 6.2 Tests del orden reciente y del ranking de búsqueda en Quick Paste,
  incluyendo timestamps iguales, refresh, hidratación y pin/unpin.
- [x] 6.3 Tests de click, segundo click, cambio, Escape, accesibilidad y
  desaparición del hint de selección.
- [x] 6.4 Tests de `Cmd/Ctrl+Enter` sólo sobre la card seleccionada y de
  exclusión de controles interactivos.
- [x] 6.5 Tests del menú en bordes del viewport, sin clipping, con scroll
  interno limitado y labels de atajo.
- [x] 6.6 Regresiones de imágenes antiguas, rich text, títulos, tags,
  colecciones, favoritos, búsqueda, source-app filter, copy/paste y
  drag-and-drop pointer/mouse.
- [x] 6.7 Tests de privacidad: selección, hints y menú no llevan contenido,
  snippets, hashes, rutas, bytes ni referencias de assets.

## 7. Verificación automática

- [x] 7.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 7.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 7.3 Ejecutar `cargo test --workspace`.
- [x] 7.4 Ejecutar `npm run check` dentro de `app/tauri/frontend`.
- [x] 7.5 Ejecutar `npm run build` dentro de `app/tauri/frontend`.
- [x] 7.6 Ejecutar `npm test` dentro de `app/tauri/frontend`.
- [x] 7.7 Ejecutar `openspec validate preview-interaction-regressions --strict
  --type change`.
- [x] 7.8 Revisar el diff y confirmar ausencia de red, telemetría, secretos,
  archivos generados y escrituras fuera de los assets permitidos.

## 8. Verificación manual

- [x] 8.1 Abrir un rich preview con tabs, saltos de línea y líneas vacías en
  Desktop y Quick Paste; comparar visualmente con el contenido original.
- [x] 8.2 Abrir Quick Paste con favoritos y entradas de distintas fechas;
  confirmar favoritos primero y orden más reciente → más antiguo dentro de
  cada grupo.
- [x] 8.3 Abrir el menú de una card cerca de cada borde del Desktop y confirmar
  que todas las opciones son visibles y que sólo el menú tiene scroll si hace
  falta.
- [x] 8.4 Seleccionar una card con click, cambiar a otra, repetir click para
  deseleccionar y usar Escape; confirmar que el hint aparece/desaparece.
- [x] 8.5 Probar `Cmd+Enter` en macOS y `Ctrl+Enter` en Linux sobre la card
  seleccionada; confirmar que abre la preview correcta.
- [x] 8.6 Confirmar que `Previsualizar` y cualquier acción con shortcut muestra
  el hint correcto en el menú.
- [x] 8.7 Repetir las regresiones críticas de imágenes persistidas,
  drag-and-drop, títulos, tags, colecciones, favoritos y paste.

La verificación manual de `platform-permission-guidance` es independiente y
no debe marcarse como completada por este cambio.

## 9. Regresión de popover y etiqueta de preview

Esta sección documenta los ajustes manuales posteriores a la
implementación inicial que corrigieron dos regresiones visuales
detectadas durante la revisión manual:

### 9.1 Menú de las cards Desktop pegado al trigger

El menú ya se mostraba como popover flotante, pero quedaba separado
del botón de puntos suspensivos por un hueco de 4 px que rompía la
lectura visual del control.

- [x] 9.1.1 Eliminar el hueco entre el trigger y el popover en
  `cardMenuPositioning.ts` introduciendo la constante
  `CARD_MENU_TRIGGER_GAP = 0` que ancla el popover al borde del
  trigger tanto cuando cae por debajo como cuando se voltea por
  encima. La gutter del viewport (`CARD_MENU_GUTTER`) sigue
  intacta para los márgenes laterales e inferiores contra el
  borde del viewport.
- [x] 9.1.2 Mantener el clamping adaptativo en los cuatro bordes
  del viewport (top, bottom, left, right) sin reintroducir el gap
  vertical cuando hay espacio suficiente.
- [x] 9.1.3 Preservar la invariante de scroll interno sólo del
  menú cuando el viewport es muy bajo: la card y la rail siguen
  con su geometría fija (`--cv-card-size = 240 px`).
- [x] 9.1.4 Tests en `tests/previewInteractionRegressions.test.ts`:
  - [x] "card menu hugs the trigger without an excessive vertical gap"
    (gap = 0 tanto en `trigger.bottom → popover.top` como en
    `popover.bottom → trigger.top` al flipear).
  - [x] "card menu clamps inside the viewport on every edge" (top,
    bottom, left, right con anclas en el viewport inset).

### 9.2 Escape, click exterior y cleanup del menú

El rail ya manejaba el cierre del menú con Escape, pero faltaba
documentar y blindar el resto de invariantes (Escape dentro del
popover, click exterior, cleanup idempotente, una sola instancia
abierta).

- [x] 9.2.1 Escape cierra el menú aunque el foco esté dentro del
  popover: `HistoryCardRail` registra el listener en `document`
  con captura, por lo que el keystroke no puede ser interceptado
  por un `menuitem` activo.
- [x] 9.2.2 Click exterior cierra el menú: el listener de
  `document.click` ignora los clics dentro de `[data-testid="history-card-menu"]`
  y dentro de `[data-testid="history-card-menu-trigger"]` y
  cierra el menú activo para cualquier otro blanco.
- [x] 9.2.3 Cleanup idempotente: `onDestroy` invoca
  `detachWindow?.()` y luego `detachWindow = null`, de modo que un
  segundo `destroy` (o un remount) no puede apilar listeners.
- [x] 9.2.4 Una sola instancia abierta: `HistoryCardRail` es el
  dueño del slot canónico `openCardId`; el handler
  `menu-toggle` colapsa cualquier menú activo al fijar
  `openCardId = open ? id : null`, sin exponer un booleano por
  card que pueda generar dos menús simultáneos.
- [x] 9.2.5 Abrir o cerrar el menú no selecciona la card ni
  inicia drag-and-drop: el botón del menú es un `<button>`,
  `isInteractiveTarget` lo reconoce y el helper de drag pointer
  singleton sigue siendo el único origen de drag.
- [x] 9.2.6 Tests en `tests/previewInteractionRegressions.test.ts`:
  - [x] "HistoryCardRail wires Escape to close an open card menu".
  - [x] "HistoryCardRail closes the menu on outside click".
  - [x] "HistoryCardRail cleans up document listeners on destroy".
  - [x] "HistoryCardRail enforces a single card menu instance".
  - [x] "HistoryCard menu never picks the card as a drag source".

### 9.3 Etiqueta de preview en las cards Desktop

La etiqueta `Previsualizar · ⌘↵` / `Previsualizar · Ctrl↵` se
renderizaba en la esquina superior derecha de la card con
`position: absolute`, superponiéndose con la chincheta y el menú.

- [x] 9.3.1 Mover la etiqueta al pie de la card, alineada a la
  izquierda, dentro del footer `.card-actions` con
  `margin-right: auto` para empujarla al borde izquierdo mientras
  pin y menu-trigger siguen anclados a la derecha.
- [x] 9.3.2 No modificar la geometría fija de la card: el footer
  consume su fila `auto` del grid; la huella `--cv-card-size`
  sigue intacta.
- [x] 9.3.3 Renombrar el texto visible a "Preview" (en inglés, en
  consonancia con el resto del menú) y conservar el
  `aria-keyshortcuts` y el `aria-label` accesibles con la misma
  plataforma del matcher `matchesPreviewShortcut`.
- [x] 9.3.4 Mantener la etiqueta oculta para el resto de cards y
  rederiva visible sólo cuando `selected === true`; desaparece al
  deseleccionar o cuando la entrada deja de estar visible
  (rail ya descarta `selectedEntryId` fuera de scope).
- [x] 9.3.5 Tests en `tests/previewInteractionRegressions.test.ts`:
  - [x] "HistoryCard preview hint sits in the footer, not
    absolutely positioned" (inspecciona `.card-actions` y
    verifica la ausencia de `position: absolute` y la presencia
    de `margin-right: auto`).
  - [x] "HistoryCard preview hint uses the English 'Preview'
    label" (lee el span `.preview-hint-label`).

### 9.4 Etiqueta de preview en Quick Paste

Las cards de Quick Paste no mostraban la pista visual de atajo para
el item seleccionado, así que la acción `Previsualizar` del menú
era descubrible sólo a través del menú.

- [x] 9.4.1 Añadir `data-testid="quick-paste-preview-hint"` en la
  meta-línea de cada row, dentro de un bloque
  `{#if index === selectedIndex}` que limita la etiqueta al item
  seleccionado.
- [x] 9.4.2 Resolver plataforma y shortcut reutilizando
  `previewShortcutLabel(shortcutPlatform)` y
  `previewShortcutAccessibleLabel(shortcutPlatform)` (los helpers
  ya importados desde `lib/clipboardPreview.ts`). No se duplica
  la tabla de modificadores ni se codifican glifos literales
  como `⌘Enter` / `Ctrl Enter`.
- [x] 9.4.3 La etiqueta es `pointer-events: none` y `flex: 0 0 auto`,
  de modo que la altura fija de 72 px del row no cambia y el
  título flexiona cuando aparece; el type icon, el pin, el
  source-app icon y el menú trigger mantienen su columna fija.
- [x] 9.4.4 La acción de preview sigue limitada al item
  seleccionado: el helper `matchesPreviewShortcut` sigue
  resolviendo la entrada por `resultIds[selectedIndex]` y
  `openPreviewFor` no cambia.
- [x] 9.4.5 Tests en `tests/previewInteractionRegressions.test.ts`:
  - [x] "QuickPaste exposes a preview shortcut hint on the
    selected row" (verifica el gating por
    `{#if index === selectedIndex}`).
  - [x] "QuickPaste preview hint uses the shared shortcut
    helpers" (comprueba el consumo de los helpers y la ausencia
    de glifos hard-codeados).
  - [x] "QuickPaste preview hint uses the English 'Preview'
    label".
  - [x] "QuickPaste preview hint is non-interactive and never
    breaks the row layout" (extrae el bloque CSS del rule
    `.qp-preview-hint` y verifica `pointer-events: none`,
    `flex: 0 0 auto` y la ausencia de `position: absolute`).

### 9.5 No regresiones obligatorias verificadas

- [x] 9.5.1 `rich-preview` con saltos de línea, tabs e indentación:
  tests de `entryFullPreviewText` siguen pasando sin `trim` ni
  colapso de whitespace.
- [x] 9.5.2 Orden reciente de Quick Paste: tests de
  `quickPasteOrderedIds` y `preserveSelectionAfterReorder` siguen
  pasando.
- [x] 9.5.3 Ranking de búsqueda: tests de `orderWithSearchRanking`
  intactos; el cambio no toca el helper.
- [x] 9.5.4 Selección por click y Escape: tests de
  `HistoryCardRail.wires Escape` y de selección por id siguen
  pasando.
- [x] 9.5.5 Pin / favoritos: `onTogglePin` y `togglePin` no se
  modifican; el chip "pin" sigue en su columna fija del footer de
  la card y en la meta-línea de Quick Paste.
- [x] 9.5.6 Títulos: el editor inline y el doble-click siguen
  funcionando; el cambio sólo afecta a la posición de la
  etiqueta de preview.
- [x] 9.5.7 Tags y colecciones: los modales y los chips no se
  tocan; la etiqueta vive en el footer / meta-línea y nunca en
  las zonas de chips.
- [x] 9.5.8 Filtro por aplicación: `appIconResolver` y
  `sourceAppIconCommand` no se modifican.
- [x] 9.5.9 Imágenes antiguas y thumbnails: el contrato del
  resolver `createClipboardAssetResolver()` y del token
  `thumbnailToken` no cambia.
- [x] 9.5.10 Copy / paste: `runCopyForEntry`, `performCopyFlow` y
  `pasteEntryCommand` no se tocan.
- [x] 9.5.11 Drag-and-drop pointer/mouse y ghost: `pointerDragAndDrop.ts`
  no se modifica; la etiqueta de preview es `pointer-events: none`
  para no robar el pointer al ghost.
- [x] 9.5.12 Focus / blur de Quick Paste: el listener de
  `getCurrentWindow().onFocusChanged` no se modifica; la etiqueta
  sólo se monta dentro del row ya gobernado por el foco.
- [x] 9.5.13 Listeners idempotentes: `HistoryCardRail.onDestroy`
  sigue nuleando `detachWindow` y `QuickPaste.svelte` mantiene
  el flag `unlistenFocusClose`.
- [x] 9.5.14 Privacidad metadata-only: ninguna etiqueta
  introduce `entry.content`, `entry.title`, `asset_ref`,
  `content_hash` ni `preview_ref`; el assertion
  "the preview-shortcut hint never carries clipboard content"
  sigue pasando.

### 9.6 Verificación de la regresión

- [x] 9.6.1 `cargo fmt --all -- --check` sin diferencias.
- [x] 9.6.2 `cargo clippy --workspace --all-targets -- -D warnings`
  sin warnings.
- [x] 9.6.3 `cargo test --workspace` con todos los suites en
  verde (ver `cargo test --workspace | grep "test result:"`).
- [x] 9.6.4 `npm run check` dentro de `app/tauri/frontend` con
  0 errores (warnings preexistentes de a11y no relacionados).
- [x] 9.6.5 `npm run build` dentro de `app/tauri/frontend`
  completa la generación de los bundles.
- [x] 9.6.6 `npm test` dentro de `app/tauri/frontend` con 1000
  tests, 0 fallos (incluye los 13 nuevos tests del menú, del
  hint de preview, de la etiqueta en Quick Paste y los 18 tests
  adicionales de la regresión 9.7 que cubren las cuatro
  correcciones manuales posteriores).
- [x] 9.6.7 `openspec validate preview-interaction-regressions
  --strict --type change` responde "Change
  'preview-interaction-regressions' is valid".

## 10. Regresión posterior a la implementación inicial

Una segunda vuelta de QA manual detectó cuatro regresiones que
escaparon a las verificaciones de la sección 9. Cada bloque
documenta el defecto observado, el contrato que la implementación
debe cumplir y los tests que lo pinean byte-por-byte.

### 10.1 Menú de las cards pegado al trigger tras el render

El menú ya no debe quedar separado del botón de puntos
suspensivos por un hueco visible. La causa era que el primer
pass usaba `maxHeight` (la cota superior) como altura del
popover; cuando el contenido renderizado era más pequeño que esa
cota, `popover.bottom !== trigger.top`.

- [x] 10.1.1 `HistoryCard.svelte` re-deriva la posición del
  popover con `requestAnimationFrame` + `getBoundingClientRect()`
  midiendo la altura real del popover y delegando el re-anchor en
  `recomputeCardMenuPositionForActualHeight` para que
  `popover.bottom === trigger.top` cuando el menú se voltea
  hacia arriba. El segundo pass sólo re-anchora; los clamps de
  viewport (`CARD_MENU_GUTTER`) y la invariante de scroll interno
  siguen intactos.
- [x] 10.1.2 El primer pass sigue garantizando
  `popover.top === trigger.bottom` cuando hay espacio debajo del
  trigger; el post-render pass sólo se aplica cuando el menú
  flippa hacia arriba para no introducir un crecimiento
  artificial del popover.
- [x] 10.1.3 `cardMenuPositioning.ts` exporta
  `recomputeCardMenuPositionForActualHeight` y mantiene el helper
  puro (sin DOM) para que la suite de regresión lo ejerza sin
  montar Svelte.
- [x] 10.1.4 Tests geométricos en
  `tests/previewInteractionRegressions.test.ts`:
  - [x] "card menu top equals trigger.bottom when the menu drops
    below" — verifica `popover.top === trigger.bottom` con
    `gap === 0`.
  - [x] "card menu bottom equals trigger.top when the menu flips
    above" — verifica `popover.bottom === trigger.top` antes y
    después del post-render pass.
  - [x] "card menu second pass anchors the visible bottom to
    trigger.top" — re-anchora con altura real `< maxHeight` y
    confirma el contrato.
  - [x] "card menu stays inside the viewport when the trigger is
    at the bottom edge" — clamp del viewport en el borde
    inferior.
  - [x] "card menu stays inside the viewport when the trigger is
    at the top edge" — clamp del viewport en el borde superior.
  - [x] "card menu post-render pass preserves the viewport clamp"
    — el segundo pass no rompe los clamps aunque la altura real
    sea menor.

### 10.2 Escape cierra el menú de las cards

`Escape` debe cerrar el menú incluso cuando el foco está dentro
del popover. La regresión venía de tener `menuOpen` como estado
local en `HistoryCard` mientras el rail mantenía
`openCardId`; cerrar el menú en el rail no propagaba al card.

- [x] 10.2.1 `HistoryCard.svelte` recibe `menuOpen` como
  prop derivada de `openCardId === entry.id`; el estado local
  `let menuOpen = false` se elimina para que no pueda divergir
  del rail.
- [x] 10.2.2 Todas las rutas de cierre del card (delete, edit
  title, restore title, toggle, escape, click exterior) delegan
  en `closeMenuAfterAction` que sólo despacha `menu-toggle`;
  ningún path vuelve a escribir `menuOpen = false` localmente.
- [x] 10.2.3 `HistoryCardRail.svelte` propaga
  `menuOpen={openCardId === entry.id}` a cada card. Cuando el
  rail colapsa `openCardId = null` (Escape, outside click,
  refresh, scope change), la prop del card cae a `false` en el
  mismo tick de Svelte y el popover desaparece.
- [x] 10.2.4 Tests estructurales en
  `tests/previewInteractionRegressions.test.ts`:
  - [x] "HistoryCard delegates the menu state to the rail through
    the menuOpen prop" — confirma `export let menuOpen: boolean`
    y la ausencia de un `let menuOpen = false` paralelo.
  - [x] "HistoryCard closes the menu through
    closeMenuAfterAction, not a local write" — barre todas las
    rutas y verifica que ninguna vuelva a escribir `menuOpen =
    false` localmente.
  - [x] "HistoryCardRail forwards menuOpen to every card so
    Escape closes the popover" — confirma el binding
    `menuOpen={openCardId === entry.id}` y la mutación del
    rail.
  - [x] "HistoryCard dispatches select-request through the shared
    helper" — confirma que el flujo de selección pasa por el
    helper compartido.

### 10.3 Hint de preview en Quick Paste después del título

El hint de preview debe aparecer después del título dentro del
row, no entre el icono de tipo y el título. El orden visual
obligatorio es `tipo/icono → título → hint → pin → source-app`.

- [x] 10.3.1 `QuickPaste.svelte` mueve el bloque
  `{#if index === selectedIndex}` con el `qp-preview-hint` para
  que quede inmediatamente después del `<span class="qp-title">`
  y antes del `<button class="qp-pin">`.
- [x] 10.3.2 `grid-template-columns` de `.qp-row-line-meta`
  cambia a `1.5rem minmax(0, 1fr) auto 1.65rem 1.65rem` para
  reservar una columna `auto` que se colapsa a `0` cuando la
  fila no está seleccionada; el tipo, el título, el pin y el
  source-app mantienen sus huellas.
- [x] 10.3.3 El hint sigue siendo `pointer-events: none` con
  `flex: 0 0 auto`, reutiliza `previewShortcutLabel` /
  `previewShortcutAccessibleLabel` (sin glifos hardcodeados),
  conserva el label inglés `Preview` y expone `aria-label` /
  `aria-keyshortcuts` con la plataforma del matcher compartido.
- [x] 10.3.4 Tests estructurales en
  `tests/previewInteractionRegressions.test.ts`:
  - [x] "QuickPaste preview hint renders AFTER the title in the
    meta line" — confirma que el nodo del hint aparece después
    del nodo del título y dentro del bloque
    `{#if index === selectedIndex}`.
  - [x] "QuickPaste preview hint still uses the shared shortcut
    helpers and English label" — revalida los helpers
    compartidos y el label `Preview`.
  - [x] "QuickPaste preview hint never breaks the row layout" —
    confirma la columna `auto`, `pointer-events: none` y
    `flex: 0 0 auto`.

### 10.4 Escape cierra el preview abierto desde una card Desktop

El preview abierto desde una card Desktop debe cerrarse con
`Escape` independientemente del foco. `ClipboardPreview` ya
manejaba `Escape` cuando el foco estaba dentro del overlay; la
regresión era que el container (`App.svelte`) no enrutaba el
evento cuando el foco estaba fuera.

- [x] 10.4.1 `App.svelte` instala un listener de `keydown` en
  `window` en fase de captura mientras el preview está abierto.
  El listener se desinstala cuando el preview se cierra y en el
  `onDestroy` del componente (cleanup idempotente,
  `detachPreviewKeydown` se nulifica tras la primera
  desinstalación).
- [x] 10.4.2 El listener llama a `event.preventDefault()` y
  `event.stopImmediatePropagation()` antes de cerrar el preview
  para que el listener del rail (que cierra el menú y limpia la
  selección) no pueda disparar mientras el preview está abierto;
  el menú y la selección quedan intactos.
- [x] 10.4.3 El listener ignora superficies de escritura
  (`HTMLInputElement`, `HTMLTextAreaElement`,
  `isContentEditable`) para que el input de búsqueda, el editor
  de título y el modal de rename mantengan su comportamiento de
  `Escape` por defecto.
- [x] 10.4.4 El container no introduce un segundo preview ni
  duplica listeners; reutiliza el flujo actual de
  `ClipboardPreview` / Desktop. El listener del overlay sigue
  cubriendo el caso de foco dentro del preview y el nuevo
  listener de `App.svelte` cubre el foco fuera del preview.
- [x] 10.4.5 Tests estructurales en
  `tests/previewInteractionRegressions.test.ts`:
  - [x] "App.svelte installs a window-level Escape handler while
    the preview is open" — confirma el listener capture-phase en
    `window`, el `stopImmediatePropagation` y la delegación a
    `closePreview`.
  - [x] "App.svelte tears down the preview Escape listener when
    the preview closes" — verifica el cleanup al cerrar y en el
    `onDestroy`, y que el handle se nulifica para no duplicar la
    desinstalación.
  - [x] "App.svelte preview Escape ignores inputs, / textareas, /
    contenteditable" — verifica que las superficies de escritura
    se salten el cierre.
  - [x] "App.svelte preview Escape must not change the rail
    selection or menu state" — confirma el
    `stopImmediatePropagation` y la guarda sobre `previewEntry`.
  - [x] "HistoryCard preview-hint and QuickPaste preview-hint
    still share the helpers" — revalida que las dos superficies
    siguen consumiendo los helpers compartidos.

Las pruebas manuales de la sección 8 siguen siendo la fuente
autoritativa de los flujos que no pueden automatizarse de forma
confiable (hotkeys nativos, focus real del webview, drop sobre
colecciones scrolleables). No se marcaron como completadas por
inferencia.

## 11. Regresión posterior al QA manual (segunda vuelta)

Una tercera vuelta de QA manual detectó tres regresiones nuevas
que escaparon a las verificaciones de las secciones 9 y 10. Cada
bloque documenta el defecto observado, el contrato que la
implementación debe cumplir y los tests que lo pinean
byte-por-byte.

### 11.1 Card menu no visible al pulsar los puntos suspensivos

El menú de las cards Desktop aparecía completamente invisible al
hacer click en el botón de tres puntos: ni siquiera un flash en
`(0, 0)` antes de saltar a su posición final. La causa era que
`cardMenuStyle` emitía `top / left / width / max-height` pero no
incluía `position: fixed`; la regla CSS `.menu` declaraba
`position: absolute` y, como `.card` lleva `overflow: hidden`,
el popover quedaba recortado por el border-box de la propia
card. Además, la posición se calculaba en un `queueMicrotask`,
lo que dejaba la primera pintura con un `style=""` y al popover
flotando en el origen de la card antes de saltar a su sitio.

- [x] 11.1.1 `cardMenuPositioning.ts`: `cardMenuStyle` ahora
  emite `position: fixed; ... z-index: 999; ...` como parte del
  inline style. La regla CSS `.menu` deja de declarar
  `position: absolute` y `z-index` para que el inline style sea
  la única fuente de verdad del posicionamiento. El popover
  escapa del clipping de `.card` (la card no crea un bloque
  contenedor para descendientes fixed porque no tiene
  transform / filter / perspective / will-change).
- [x] 11.1.2 `HistoryCard.svelte`: el bloque reactivo
  `$: if (menuOpen) { ... }` invoca `recomputeMenuPosition()`
  de forma **sincrona** (sin `queueMicrotask`). El popover se
  monta con el `style` ya calculado en su primera pintura, sin
  parpadeo en `(0, 0)`. El post-render pass vía
  `requestAnimationFrame` sigue refinando la altura real.
- [x] 11.1.3 `HistoryCard.svelte`: el bloque reactivo limpia el
  `menuPositionStyle` cuando el menú se cierra
  (`menuOpen === false`), de modo que un re-open empiece con el
  helper en su estado inicial en lugar de arrastrar estilos
  obsoletos.
- [x] 11.1.4 Tests en `tests/previewInteractionRegressions.test.ts`:
  - [x] "cardMenuStyle escapes the card clipping context with
    position fixed" — el regex valida
    `position: fixed;` en el output del helper.
  - [x] "cardMenuStyle pins z-index so the popover never hides
    behind the card / rail / toolbar" — extrae el número y
    verifica que sea `>= 900`.
  - [x] "HistoryCard computes the menu position synchronously
    when the menu opens" — el regex valida
    `$: if (menuOpen) { ... recomputeMenuPosition() }` y
    rechaza cualquier `queueMicrotask` dentro del mismo bloque.
  - [x] "HistoryCard menu CSS drops `position: absolute` so
    the inline style is authoritative" — la regla CSS `.menu`
    queda libre de `position: absolute` y de `z-index`.
  - [x] "HistoryCard mounts the popover only inside an
    `{#if menuOpen}` block (single visible instance)" — el
    `data-testid="history-card-menu"` aparece exactamente una
    vez en el archivo y dentro del bloque `{#if menuOpen}`.

### 11.2 Selección Desktop no confiable

La selección del rail Desktop no se podía reproducir de forma
confiable durante el QA manual. La auditoría confirmó que el
contrato ya era el correcto (un único `selectedEntryId`,
`===` estricto, binding con `App.svelte`, sin auto-selección
del primer card) pero la batería de tests no pineaba ninguno de
esos invariantes. Esta sección documenta los pins añadidos.

- [x] 11.2.1 `HistoryCardRail.svelte`: mantiene `selectedEntryId`
  como el único estado canónico de selección. La prop
  `selected` se sigue calculando como
  `selectedEntryId === entry.id` (estricto) y se re-deriva
  reactivamente cada vez que cambia `entries` o
  `selectedEntryId`. El bloque reactivo
  `$: if (selectedEntryId !== null && !visibleEntryIds.has(selectedEntryId))`
  sigue reseteando el valor cuando la entrada cae fuera del
  scope visible (borrado, refresh, búsqueda, cambio de
  colección).
- [x] 11.2.2 `App.svelte`: la variable local `railSelectedEntryId`
  se inicializa a `null` (no `entries[0].id` ni
  `visibleEntries[0]?.id`). El handler
  `selectCollectionFromSidebar` reasigna
  `railSelectedEntryId = null` antes de refrescar las opciones
  de filtro y la rail, de modo que un cambio de colección
  nunca hereda una selección obsoleta.
- [x] 11.2.3 `HistoryCard.svelte`: el handler
  `onCardSurfaceClick` sigue delegando en
  `dispatchSelect(selected ? null : entry.id)`. El id usado es
  siempre `entry.id` (no `entries.indexOf(entry)` ni un cursor
  visual); un segundo click sobre la misma card deselecciona.
  La rama de Escape en `onCardKeydown` también delega en
  `dispatchSelect(null)`.
- [x] 11.2.4 Tests en `tests/previewInteractionRegressions.test.ts`:
  - [x] "HistoryCard click handler dispatches the card's entry
    id, not an index" — verifica que el body del handler
    contiene `dispatchSelect(selected ? null : entry.id)` y que
    no contiene `indexOf`.
  - [x] "HistoryCardRail forwards `selectedEntryId === entry.id`
    strictly" — confirma la igualdad estricta y rechaza
    `entries.indexOf` en la rail.
  - [x] "HistoryCardRail binds selectedEntryId through
    `bind:selectedEntryId`" — confirma el binding en
    `App.svelte`.
  - [x] "HistoryCardRail never defaults to a non-null selection
    (no auto-select of the first card)" — valida el default
    `null` en rail y App y rechaza cualquier seeding desde
    `entries[0]`.
  - [x] "HistoryCardRail clears the selection when the visible
    scope changes" — confirma el contrato del bloque reactivo.
  - [x] "App.svelte clears railSelectedEntryId when the
    collection changes" — confirma el reset en
    `selectCollectionFromSidebar`.

### 11.3 Flechas de Quick Paste solo hacen scroll

En Quick Paste, cuando el foco estaba sobre el input de búsqueda,
las flechas Arriba / Abajo solo movían el caret al final del
texto y la selección del row no cambiaba. Además, en el resto
del listado la selección por `ArrowDown` al final de la lista
"teletransportaba" al primer row por culpa de un wrap con
módulo. El nuevo contrato es: clamp dentro de los límites y
delegación a `scrollSelectedRowIntoView`.

- [x] 11.3.1 `QuickPaste.svelte`: nuevo handler
  `onSearchInputKeydown` enlazado al `<input>` de búsqueda vía
  `on:keydown`. Las cuatro teclas
  (`ArrowDown` / `ArrowUp` / `Home` / `End`) llaman
  `preventDefault` + `stopPropagation` y delegan en
  `moveSelection` / `jumpToFirst` / `jumpToLast`. El
  `stopPropagation` evita que el listener de `<svelte:window>`
  se dispare dos veces sobre el mismo evento y avance la
  selección dos filas en una sola pulsación.
- [x] 11.3.2 `QuickPaste.svelte`: `moveSelection` ya no usa
  `(selectedIndex + delta + total) % total`. En su lugar
  delega en el helper puro `clampedSelectedIndex` (en
  `lib/quickPasteActions.ts`) que clamp al rango
  `[0, total - 1]` y devuelve `-1` para listas vacías.
  `moveSelection` corta por lo sano si el helper devuelve
  `-1` o el mismo índice (frontera), de modo que el
  `scrollIntoView` no se re-emite en el borde.
- [x] 11.3.3 `lib/quickPasteActions.ts`: nuevo helper
  `clampedSelectedIndex(resultIds, currentIndex, delta)` que
  es total, puro y testeable sin DOM. Normaliza índices fuera
  de rango antes de aplicar el delta, lo que evita aterrizar en
  slots obsoletos tras un refresh o un re-rank.
- [x] 11.3.4 Tests en `tests/quickPasteActions.test.ts`:
  - [x] "clampedSelectedIndex advances by the delta inside the
    list bounds" — verifica `+1` y `-1` dentro del rango.
  - [x] "clampedSelectedIndex clamps at the top edge (no
    negative index)" — verifica que `ArrowUp` en la primera
    fila se queda en la primera fila.
  - [x] "clampedSelectedIndex clamps at the bottom edge (no
    wrap-around)" — verifica que `ArrowDown` en la última fila
    se queda en la última fila (sin wrap a la primera).
  - [x] "clampedSelectedIndex returns -1 for an empty list".
  - [x] "clampedSelectedIndex normalises an out-of-range
    current index" — el helper no peta con cierres obsoletos.
  - [x] "clampedSelectedIndex never returns a value outside
    `[0, total-1]`" — barrido paramétrico de
    `(current, delta)`.
- [x] 11.3.5 Tests en `tests/quickPasteSelectionAndScroll.test.ts`:
  - [x] "moveSelection clamps at the bounds of the list (no
    wrap-around)" — confirma que `moveSelection` delega en
    `clampedSelectedIndex`, no usa `% total` y hace
    short-circuit cuando el clamp aterriza en el mismo índice.
  - [x] "search input installs an on:keydown handler so arrow
    keys work while typing" — confirma
    `on:keydown={onSearchInputKeydown}` en el `<input>`.
  - [x] "onSearchInputKeydown routes ArrowUp / ArrowDown /
    Home / End to the moveSelection helpers" — verifica las
    cuatro ramas del handler y exige al menos cuatro
    `preventDefault()`.
  - [x] "onSearchInputKeydown stops propagation so the window
    listener does not double-fire" — exige al menos cuatro
    `stopPropagation()` para evitar el doble avance.

### 11.4 Verificación automática de la regresión 11

- [x] 11.4.1 `cd app/tauri/frontend && npm test` ejecuta
  `1022` tests (los `1000` previos + `22` tests nuevos
  repartidos entre
  `tests/previewInteractionRegressions.test.ts`,
  `tests/quickPasteSelectionAndScroll.test.ts` y
  `tests/quickPasteActions.test.ts`), 0 fallos.
- [x] 11.4.2 `cd app/tauri/frontend && npm run check` ejecuta
  el type-check de Svelte sin errores nuevos (los warnings
  preexistentes no son introducidos por esta regresión).
- [x] 11.4.3 `cd app/tauri/frontend && npm run build` completa
  la generación de los bundles sin warnings nuevos.
- [x] 11.4.4 `cargo fmt --all -- --check`, `cargo clippy
  --workspace --all-targets -- -D warnings` y
  `cargo test --workspace` siguen verdes (ningún archivo de
  Rust se modificó en esta regresión).
- [x] 11.4.5 `openspec validate preview-interaction-regressions
  --strict --type change` responde "Change
  'preview-interaction-regressions' is valid".

## 12. Regresión posterior al QA manual (tercera vuelta)

Una cuarta vuelta de QA manual detectó dos regresiones nuevas
que escaparon a las verificaciones de las secciones 9, 10 y 11.
Ambas son de **comportamiento observable** (no estructurales):
la selección por click nunca pintaba el cue visual y la
navegación con flechas sólo desplazaba la rail. Cada bloque
documenta el defecto, el contrato que la implementación debe
cumplir y los tests que lo pinean byte-por-byte, con una
mitad estructural y otra mitad end-to-end sobre un harness que
reproduce el flujo del rail.

### 12.1 Selección visual real por click

El `class:card-selected={selected}` se aplicaba al `<article>`
pero no existía regla CSS para `.card-selected`, por lo que el
usuario nunca veía un cue visual al pulsar otra card y la
primera card parecía quedar siempre activa. El fix declara la
regla `.card.card-selected` (acento azul + ring suave + fondo
ligeramente más claro) y, simétricamente, `.card.menu-open`
para que la cue del menú sea visible cuando se abra. La regla
combinada `.card.card-selected.menu-open` mantiene el acento
de selección dominante cuando ambos flags están activos.

- [x] 12.1.1 `HistoryCard.svelte` declara el bloque
  `.card.card-selected { … }` con `border-color`, `box-shadow`
  y `background` explícitos para que la fila activa sea
  visiblemente distinta del resto.
- [x] 12.1.2 `HistoryCard.svelte` declara el bloque
  `.card.menu-open { … }` con `border-color` para que el cue
  del menú ya no sea invisible.
- [x] 12.1.3 `HistoryCard.svelte` declara el bloque
  `.card.card-selected.menu-open { … }` para que la selección
  siga ganando cuando la card seleccionada abre su menú.
- [x] 12.1.4 El bind `aria-selected={selected}` se mantiene
  como el único origen accesible (sin string literal
  `aria-selected="false"` que pueda confundir el parser).
- [x] 12.1.5 El bind `data-selected={selected ? "true" :
  "false"}` se mantiene para que las pruebas e2e puedan leer
  el estado sin tener que inspeccionar la clase CSS.
- [x] 12.1.6 Tests en `tests/previewInteractionRegressions.test.ts`:
  - [x] "HistoryCard declares the .card-selected CSS rule so the
    click cue actually paints" — extrae la regla y verifica que
    `border-color` y `box-shadow` están presentes.
  - [x] "HistoryCard declares the .menu-open CSS rule so the
    open menu cue is visible" — mismo patrón para la cue del
    menú.
  - [x] "HistoryCard keeps the selection cue when both selected
    and menu-open are true" — confirma la regla combinada.
  - [x] "HistoryCard mirrors the selected flag onto
    aria-selected as a real boolean attribute" — confirma el
    binding y rechaza un string literal `"false"`.
  - [x] "HistoryCard surfaces data-selected so the click state
    is observable from the DOM" — confirma el binding
    `data-selected`.
- [x] 12.1.7 Tests de comportamiento en
  `tests/desktopRailSelectionBehavior.test.ts` (harness de rail
  + cards que aplica el mismo predicado que la rail real):
  - [x] "click on the second card selects only the second card"
    — confirma que sólo la segunda card queda con
    `classSelected`, `ariaSelected=true`, `dataSelected="true"`
    y `hasPreviewHint=true`.
  - [x] "click on the third card moves the selection from card
    2 to card 3" — la selección se mueve correctamente.
  - [x] "clicking the selected card again clears the selection"
    — segundo click deselecciona.
  - [x] "clicking a control (pin / menu / title / paste /
    delete) never flips the selection" — el guard
    `isInteractiveTarget` corta antes de cambiar selección.
  - [x] "the first card is never auto-selected on mount" —
    `selectedEntryId` arranca en `null`.
  - [x] "the selection survives a refresh that keeps every
    visible id" — refresh sin cambio de scope preserva la
    selección.
  - [x] "the selection is cleared when the visible scope drops
    the selected id" — delete / search / colección limpia la
    selección.

### 12.2 Navegación horizontal con ArrowLeft / ArrowRight

La rail escuchaba `keydown` para `Escape` pero nunca para las
flechas horizontales, así que `ArrowLeft` / `ArrowRight` sólo
disparaban el scroll nativo del overflow sin tocar
`selectedEntryId`. El fix instala un listener de captura que
delega la matemática al helper puro
`horizontalRailNextSelectionId`, escribe el resultado en
`selectedEntryId`, llama a `preventDefault()` y pide al card
recién seleccionado que ejecute
`scrollIntoView({ block: "nearest", inline: "nearest" })`. El
helper es clamp (no wrap-around), cubre los fallbacks `null →
first` / `null → last` y trata los ids obsoletos como
selección vacía. La registry de cards (`onCardRef`) mantiene
el mapeo id → element sincronizado con el DOM para que el
`scrollIntoView` siempre acierte.

- [x] 12.2.1 `HistoryCardRail.svelte` importa
  `horizontalRailNextSelectionId` desde
  `./lib/horizontalRailNavigation.ts` y no reimplementa la
  matemática inline (sin `% entries.length` ni `% visibleIds.length`).
- [x] 12.2.2 `HistoryCardRail.svelte` instala el listener de
  `keydown` en captura para la navegación horizontal y lo
  desmonta en `onDestroy` para que un remount no apile dos
  listeners.
- [x] 12.2.3 El handler `onRailHorizontalKeydown` llama a
  `event.preventDefault()` exactamente una vez por tecla
  consumida (sin tragarse teclas sobre inputs / textareas /
  contenteditable, sin tragarse teclas sobre la rail vacía).
- [x] 12.2.4 El handler delega la resolución del siguiente id al
  helper puro; ningún branch inline reimplementa la clamp ni
  el wrap-around.
- [x] 12.2.5 `scrollSelectedCardIntoView(id)` invoca
  `scrollIntoView({ block: "nearest", inline: "nearest" })`
  sobre el elemento registrado para ese id (no sobre la
  rail, no sobre el card anterior).
- [x] 12.2.6 `HistoryCard.svelte` exporta `onCardRef` y hace
  `bind:this={cardArticleEl}` en el `<article>` para que la
  registry del rail se mantenga sincronizada con el DOM a
  través de remounts.
- [x] 12.2.7 `HistoryCardRail.svelte` declara
  `registerCardRef(id, el)` y limpia la registry de ids que
  cayeron fuera del scope visible (delete / refresh /
  búsqueda / colección).
- [x] 12.2.8 `selectedEntryId` se mantiene inicializado a
  `null` (sin seeding desde `entries[0]`) para que la primera
  `ArrowRight` caiga en la primera card y no se auto-seleccióne
  la primera card.
- [x] 12.2.9 Tests del helper puro en
  `tests/horizontalRailNavigation.test.ts`:
  - [x] "picks the first id when currentId is null and direction
    is right" — fallback `null → first`.
  - [x] "picks the last id when currentId is null and direction
    is left" — fallback `null → last`.
  - [x] "advances by one to the right inside the visible scope".
  - [x] "retreats by one to the left inside the visible scope".
  - [x] "clamps to the right edge without wrapping" —
    `ArrowRight` en la última card se queda.
  - [x] "clamps to the left edge without wrapping" —
    `ArrowLeft` en la primera card se queda.
  - [x] "returns null for an empty visible set".
  - [x] "falls back to the closest neighbour when the
    currentId fell out of scope".
  - [x] "never mutates the entries array" — el helper es puro.
  - [x] "handles a single-entry rail" — caso límite.
  - [x] "respects the visible order for both directions" —
    barrido paramétrico `(current, direction)`.
- [x] 12.2.10 Tests estructurales en
  `tests/previewInteractionRegressions.test.ts`:
  - [x] "HistoryCardRail imports the horizontalRailNextSelectionId
    helper".
  - [x] "HistoryCardRail registers a keydown listener for the
    horizontal arrow keys" — confirma el `addEventListener` y
    el `removeEventListener` correspondientes.
  - [x] "HistoryCardRail calls preventDefault on a horizontal
    arrow press the handler consumes".
  - [x] "HistoryCardRail skips horizontal navigation on typing
    surfaces" — confirma `HTMLInputElement`,
    `HTMLTextAreaElement`, `isContentEditable`.
  - [x] "HistoryCardRail delegates the next-id math to
    horizontalRailNextSelectionId (no inline modulo wrap)".
  - [x] "HistoryCardRail calls scrollIntoView on the freshly
    selected card" — confirma `block: "nearest"`, `inline:
    "nearest"`.
  - [x] "HistoryCardRail wires the card registry through the
    onCardRef callback" — confirma el `bind:this` del
    `<article>` y la firma `(el) => registerCardRef(...)`.
  - [x] "HistoryCardRail cleans up the card registry when an
    entry leaves the visible scope".
  - [x] "HistoryCardRail never seeds the initial selection from
    entries[0]".
- [x] 12.2.11 Tests de comportamiento end-to-end en
  `tests/desktopRailSelectionBehavior.test.ts`:
  - [x] "ArrowRight with no current selection selects the first
    visible card" — `null → first` real.
  - [x] "ArrowLeft with no current selection selects the last
    visible card" — `null → last` real.
  - [x] "ArrowRight on the second card advances to the third
    card" — flujo real `102 → 103`.
  - [x] "ArrowLeft on the third card retreats to the second
    card" — flujo real `103 → 102`.
  - [x] "ArrowRight on the last card clamps without wrapping" —
    sin wrap-around en la última card.
  - [x] "ArrowLeft on the first card clamps without wrapping" —
    sin wrap-around en la primera card.
  - [x] "the arrow handler calls preventDefault so the native
    scroll cannot run" — `preventDefault()` exactamente una vez
    por tecla consumida.
  - [x] "the arrow handler calls scrollIntoView on the freshly
    selected card only" — el contador de llamadas discrimina el
    card recién activo del resto.
  - [x] "the arrow handler is a no-op on an empty rail" — rail
    vacía no consume la tecla.
  - [x] "the arrow handler survives a mid-list refresh that
    keeps every id" — refresh / hydration / thumbnails no
    resetean la selección.
  - [x] "the arrow handler drops the selection when the visible
    scope shrinks past the active id" — el siguiente arrow cae
    sobre un vecino visible, no sobre la card borrada.
  - [x] "ArrowLeft / ArrowRight keep the Preview hint in
    lockstep with the active card" — el hint Preview sigue al
    id activo en una secuencia mixta de cinco pulsaciones.

### 12.3 Verificación automática de la regresión 12

- [x] 12.3.1 `cd app/tauri/frontend && npm test` ejecuta
  `1066` tests (los `1047` previos + `11` del helper puro +
  `8` estructurales nuevos del cambio 12), 0 fallos.
- [x] 12.3.2 `cd app/tauri/frontend && npm run check` ejecuta
  el type-check de Svelte sin errores nuevos (los warnings
  preexistentes de a11y no son introducidos por esta
  regresión).
- [x] 12.3.3 `cd app/tauri/frontend && npm run build` completa
  la generación de los bundles sin warnings nuevos.
- [x] 12.3.4 `cargo fmt --all -- --check`, `cargo clippy
  --workspace --all-targets -- -D warnings` y
  `cargo test --workspace` siguen verdes (ningún archivo de
  Rust se modificó en esta regresión).
- [x] 12.3.5 `openspec validate preview-interaction-regressions
  --strict --type change` responde "Change
  'preview-interaction-regressions' is valid".

## 13. Icono grande debajo de la rail Desktop

Una nueva vuelta de QA manual detectó que, debajo de la lista
horizontal de cards del Desktop, seguía existiendo el residuo
visual del antiguo indicador de drop
(`src/CardDropText.svelte` + `src/lib/cardDropTextHandlers.ts`).
El componente ya no estaba importado por `App.svelte`, pero el
archivo seguía presente en el repo con su markup, su factory de
handlers, sus listeners (`dragenter` / `dragover` / `dragleave`
/ `drop` / `dragend` / `clipvault-pointer-drag-over` /
`clipvault-pointer-drop`) y su archivo de tests dedicado. La
causa raíz es que el cleanup de la sección 13 del cambio
archivado `desktop-dnd-card-visual-corrections` sólo borró el
`<CardDropText />` del template y dejó el resto del código
huérfano; el indicador era un elemento de depuración que sólo
servía para confirmar visualmente que un drop llegaba, y la
mutación real siempre la ejecutó la fila de colección del
sidebar. El bloque que sigue documenta el contrato que la
implementación debe cumplir y los tests que lo pinean.

### 13.1 Eliminar el markup del icono grande

- [x] 13.1.1 `src/CardDropText.svelte` se borra del repo. El
  `<div class="card-drop-text">` con el texto "Suelta aquí una
  tarjeta capturada para confirmar que el drop funciona." deja
  de existir; no se reemplaza por otro icono, texto, imagen,
  placeholder ni contenedor visual.
- [x] 13.1.2 `src/lib/cardDropTextHandlers.ts` se borra del
  repo. El factory `createCardDropTextHandlers`, el helper puro
  `payloadAccepts`, el tipo `CardDropTextState` y la interfaz
  `CardDropTextApi` dejan de existir; no tenían ninguna otra
  función real fuera del componente eliminado.
- [x] 13.1.3 `tests/cardDropText.test.ts` se borra del repo.
  Los once tests que ejercían el factory sobre un `<div>`
  armado manualmente se eliminan junto con la dependencia.

### 13.2 Eliminar los listeners del icono grande

- [x] 13.2.1 `tests/pointerDragAndDrop.test.ts` deja de
  importar `createCardDropTextHandlers`. El test que cubría la
  integración entre el controlador pointer y el indicador
  inferior (`pointer drag reaches the bottom drop indicator
  through the same hit-test channel`) se elimina porque su
  única función era verificar el feedback visual eliminado.
- [x] 13.2.2 `src/lib/pointerDragAndDrop.ts` queda intacto. El
  singleton `installPointerDragController`, los listeners
  `pointerdown` / `pointermove` / `pointerup` /
  `pointercancel` / `mousedown` / `mousemove` / `mouseup` /
  `keydown`, la captura de puntero, el ghost
  `cv-pointer-drag-ghost` y el bloqueo de selección nativa se
  conservan tal cual.
- [x] 13.2.3 `src/OrganizationSidebar.svelte` queda intacto. La
  fila de colección sigue siendo el único destino que ejecuta
  `entry_collections_set`; el highlight transitorio sobre la
  fila y el preview flotante del ghost son los dos feedbacks
  visuales que permanecen en el flujo.

### 13.3 Layout del Desktop sin espacio reservado debajo de la rail

- [x] 13.3.1 `App.svelte` no contiene ningún `<CardDropText`,
  ningún elemento con `class="card-drop-text"`, ningún
  `data-testid="card-drop-text"`, ningún `class="drop-feedback"`
  ni ningún `data-testid="drop-feedback"` dentro de
  `.layout-main` ni en ninguna otra posición del template.
- [x] 13.3.2 El último elemento hijo de `<div class="layout-main">`
  es `<HistoryCardRail />`. Entre el cierre del rail y el
  `</div>` que cierra la columna derecha no se renderiza ningún
  `<div>`, `<p>`, `<section>`, `<span>`, `<svg>`, `<aside>`,
  `<footer>`, `<header>`, `<nav>`, `<article>` ni `<main>`.
- [x] 13.3.3 `.layout-main` mantiene `min-height: 0` para que la
  columna pueda encoger por debajo del contenido intrínseco.
  Ningún `min-height` positivo se reintroduce: si una versión
  futura volviera a fijar un `min-height: <valor>` (incluso
  heredado), el rail dejaría de ser el borde inferior del
  Desktop.

### 13.4 Tests estructurales en
      `tests/previewInteractionRegressions.test.ts`

- [x] 13.4.1 "App.svelte does not import or mount the legacy
  bottom drop indicator" — barre `App.svelte` y rechaza el
  import de `CardDropText`, el tag `<CardDropText>`, los
  testids y clases `card-drop-text` / `drop-feedback`; el
  cleanup no puede reintroducir el icono.
- [x] 13.4.2 "HistoryCardRail does not mount any element below
  the rail list" — verifica que el componente sólo expone el
  branch del `<p class="empty">` y el branch del
  `<div class="rail">`; rechaza cualquier `<div>` con la
  clase o el testid del indicador eliminado.
- [x] 13.4.3 "App.svelte layout-main ends with the rail and
  reserves no extra height" — localiza
  `<HistoryCardRail />` dentro de `.layout-main` y exige que
  el slice entre el cierre del rail y el `</div>` de la
  columna no contenga ningún tag de bloque.
- [x] 13.4.4 "App.svelte layout-main has no fixed min-height
  that reserves space below the rail" — exige la presencia de
  `min-height: 0` en la regla `.layout-main` y rechaza
  cualquier `min-height` positivo (numérico o por palabra
  clave); la altura inferior queda atada al contenido real.
- [x] 13.4.5 "HistoryCardRail still owns the fixed
  --cv-card-rail-height token" — la altura de la rail sigue
  saliendo de `var(--cv-card-rail-height)`, el
  `--cv-card-size: 240px` sigue fijado y el `overflow-x: auto`
  del scroll horizontal se conserva.
- [x] 13.4.6 "the CardDropText component, its handler factory
  and its test file are deleted" — `existsSync` confirma que
  `src/CardDropText.svelte`, `src/lib/cardDropTextHandlers.ts`
  y `tests/cardDropText.test.ts` ya no existen en el árbol;
  un copy-paste desde el archivo archivado no puede
  resucitarlos sin tocar un test.

### 13.5 Tests de no-regresión en
      `tests/previewInteractionRegressions.test.ts`

- [x] 13.5.1 "drag-and-drop on collections stays operational
  after the cleanup" — el sidebar sigue declarando
  `COLLECTION_DROP_TARGET_VALUE` y
  `createCollectionDropZoneHandlers`; `pointerDragAndDrop.ts`
  sigue exportando `installPointerDragController` y
  produciendo el `cv-pointer-drag-ghost`; el factory
  eliminado ya no se referencia desde el módulo.
- [x] 13.5.2 "the horizontal arrow navigation stays wired after
  the cleanup" — el rail conserva `onRailHorizontalKeydown`,
  el listener capture-phase sobre `keydown`, la delegación al
  helper puro `horizontalRailNextSelectionId` y la llamada a
  `scrollSelectedCardIntoView`; la navegación horizontal no
  se rompió al borrar el icono.
- [x] 13.5.3 "the click selection stays wired after the
  cleanup" — `HistoryCard` conserva `onCardSurfaceClick` y
  sigue despachando el `entry.id` (no un índice); el rail
  conserva `bind:selectedEntryId` y el bloque reactivo que
  limpia `selectedEntryId` cuando la entrada cae fuera del
  scope visible.
- [x] 13.5.4 "no accessibility marker was removed from the
  rail or the card" — el rail sigue exponiendo `role="list"`
  y `aria-label="Historial reciente"`; la card sigue
  exponiendo `aria-selected={selected}`,
  `data-testid="history-card"` y `draggable="false"`; el
  cleanup no se llevó por delante ninguna pista de
  accesibilidad.

### 13.6 Verificación automática de la regresión 13

- [x] 13.6.1 `cd app/tauri/frontend && npm test` ejecuta
  `1064` tests (los `1054` previos + `10` tests nuevos del
  bloque 13), 0 fallos.
- [x] 13.6.2 `cd app/tauri/frontend && npm run check` ejecuta
  el type-check de Svelte sin errores nuevos (los warnings
  preexistentes de a11y no son introducidos por esta
  regresión; los LSP diagnostics sobre `node:test` /
  `node:fs` / `process` / `String.repeat` ya estaban en el
  archivo antes del cambio).
- [x] 13.6.3 `cd app/tauri/frontend && npm run build` completa
  la generación de los bundles sin warnings nuevos.
- [x] 13.6.4 `cargo fmt --all -- --check`, `cargo clippy
  --workspace --all-targets -- -D warnings` y
  `cargo test --workspace` siguen verdes (ningún archivo de
  Rust se modificó en esta regresión).
- [x] 13.6.5 `openspec validate preview-interaction-regressions
  --strict --type change` responde "Change
  'preview-interaction-regressions' is valid".

## 14. Origen real del icono grande debajo de la rail

Una quinta vuelta de QA manual detectó que la corrección de la
sección 13 no resolvió la regresión visual: el icono grande seguía
visible inmediatamente debajo de la lista horizontal de cards del
Desktop. La causa raíz **no** era `CardDropText.svelte` (ya
eliminado en la sección 13) sino un render incondicional de
`APP_FALLBACK_ICON_SVG` que `ClipboardPreview.svelte` montaba en su
raíz, fuera del bloque `{#if entry != null}`.

`ClipboardPreview.svelte` necesitaba dos sprites al inicio de su
template:

1. `CONTENT_TYPE_ICON_SPRITE` — un `<svg width="0" height="0"
   style="position:absolute">` que registra los
   `<symbol id="cv-icon-…">` consumidos por los `<use>` del
   overlay. Es invisible por construcción.
2. `APP_FALLBACK_ICON_SVG` — un `<svg viewBox="0 0 24 24">`
   pensado para consumo inline dentro de spans de tamaño fijo
   (`.source-app-fallback`, `.qp-source-app-fallback`). **No**
   tiene `width`, `height` ni `position:absolute`, por lo que
   cuando se renderiza suelto en la raíz adopta el tamaño
   intrínseco del agente de usuario (~ 300×150 px en headless
   Chrome) y se pinta como un glyph gigante visible debajo de la
   rail.

El bloque previo añadía ambos sprites en la raíz del componente y
sólo el primero era seguro. El test
`tests/clipboardPreviewComponent.test.ts` ("ClipboardPreview
mounts the icon sprite and fallback glyphs once") incluso
aseguraba explícitamente que ambos se montaran, así que la
regresión estaba blindada por un test que confundía el contrato
correcto con el buggy.

### 14.1 Causa raíz verificada

- [x] 14.1.1 Auditoría DOM con `headless` Chrome contra el dev
  server (`localhost:5173`) reproduce el icono gigante debajo de
  la pantalla de error "No se pudo conectar con el core de
  ClipVault". El glyph visible coincide con la forma de
  `APP_FALLBACK_ICON_SVG` (rectángulo exterior redondeado +
  rectángulo interior).
- [x] 14.1.2 La regresión **no** provenía de `CardDropText.svelte`
  (eliminado en la sección 13), ni de la rail, ni de las cards.
  El render huérfano estaba en `ClipboardPreview.svelte` línea
  408 (`{@html APP_FALLBACK_ICON_SVG}`), que se ejecutaba
  incondicionalmente junto a `{@html CONTENT_TYPE_ICON_SPRITE}`.
- [x] 14.1.3 `App.svelte` monta `<ClipboardPreview />` siempre
  (`<ClipboardPreview entry={previewEntry} … />`), por lo que el
  render se pintaba independientemente del estado de la rail o
  del preview, exactamente debajo del `</main>` (es decir, debajo
  de la columna derecha del Desktop).
- [x] 14.1.4 La suite previa no detectó el problema porque el
  regex se limitaba a buscar `card-drop-text`, `CardDropText` o
  `drop-feedback`, ninguno de los cuales aparecía aquí. La nueva
  regresión obligó a inspeccionar el render final del componente
  compartido, no un nombre de archivo heredado.

### 14.2 Corrección aplicada

- [x] 14.2.1 `ClipboardPreview.svelte` elimina el render
  incondicional `{@html APP_FALLBACK_ICON_SVG}` que vivía al
  final del template, fuera del bloque `{#if entry != null}`.
  El sprite oculto (`{@html CONTENT_TYPE_ICON_SPRITE}`) se
  conserva intacto porque sigue siendo necesario para que los
  `<use href="#cv-icon-…">` del overlay resuelvan.
- [x] 14.2.2 `ClipboardPreview.svelte` elimina el import de
  `APP_FALLBACK_ICON_SVG` desde `./lib/contentTypeIcons.ts`. El
  componente ya no depende del glyph.
- [x] 14.2.3 Los consumidores reales del glyph siguen intactos y
  se verifican con tests estructurales:
  `HistoryCard.svelte` lo usa dentro del
  `<span class="source-app-fallback">` (línea 1267);
  `SourceAppFilter.svelte` lo usa dentro de los iconos del
  trigger y de cada opción (líneas 381 y 423); `QuickPaste.svelte`
  lo usa dentro de `.qp-source-app-fallback` (línea 1893). Cada
  uno de esos wrappers aplica `width` / `height` / `flex` para
  que el SVG ocupe la huella documentada sin desbordar el row.
- [x] 14.2.4 No se reintroduce ningún placeholder, drop target,
  overlay ni contenedor reservado. La altura inferior del Desktop
  sigue terminando inmediatamente después de la rail y su
  scrollbar, exactamente como exige el contrato.
- [x] 14.2.5 `pointerDragAndDrop.ts` y `OrganizationSidebar.svelte`
  quedan intactos. El singleton pointer, el ghost
  `cv-pointer-drag-ghost` y el highlight transitorio sobre la
  fila de colección del sidebar siguen siendo los tres únicos
  feedbacks visuales del drag-and-drop.

### 14.3 Tests de render final

- [x] 14.3.1 Test nuevo en
  `tests/previewInteractionRegressions.test.ts`:
  "ClipboardPreview never renders APP_FALLBACK_ICON_SVG at the
  component root" — confirma que el componente sigue montando
  `CONTENT_TYPE_ICON_SPRITE` (necesario para `<use>`) y que
  rechaza cualquier referencia a `APP_FALLBACK_ICON_SVG` (ni
  import ni render).
- [x] 14.3.2 Test nuevo:
  "APP_FALLBACK_ICON_SVG is still consumed by the inline
  fallback cells" — verifica que los tres consumidores reales
  (`HistoryCard`, `SourceAppFilter`, `QuickPaste`) siguen
  referenciando el glyph dentro de spans con dimensiones, así
  el fix no rompe el fallback por entry.
- [x] 14.3.3 Test nuevo:
  "the Desktop layout-main never mounts a stray visible SVG
  below the rail" — localiza `<HistoryCardRail />` dentro de
  `.layout-main` y exige que el slice entre el cierre del rail y
  el `</div>` de la columna no contenga ningún tag de bloque
  (`div`, `section`, `p`, `span`, `svg`, `aside`, `footer`,
  `header`, `nav`, `article`, `main`, `img`). El test no
  menciona `CardDropText`: comprueba el contrato del render
  final, no un nombre de archivo heredado.
- [x] 14.3.4 Test nuevo:
  "ClipboardPreview mounts only the hidden sprite, never the
  visible glyph" — barre el source del componente en busca de
  cualquier referencia residual a `APP_FALLBACK_ICON_SVG` y
  confirma que `CONTENT_TYPE_ICON_SPRITE` sigue montado.
- [x] 14.3.5 Test actualizado en
  `tests/clipboardPreviewComponent.test.ts`:
  "ClipboardPreview mounts the icon sprite and never the
  fallback glyph" — invierte el contrato previo: el componente
  debe montar el sprite oculto y NO debe montar el glyph
  visible. El mensaje del assert documenta la regresión para
  que un futuro contributor entienda por qué el segundo render
  está prohibido.

### 14.4 Verificación de drag-and-drop y no regresiones

- [x] 14.4.1 `pointerDragAndDrop.ts` sigue exportando
  `installPointerDragController` y produciendo el ghost
  `cv-pointer-drag-ghost` (180×120 px fixed) durante un drag
  real. Ningún cambio estructural toca ese módulo.
- [x] 14.4.2 `OrganizationSidebar.svelte` sigue declarando
  `COLLECTION_DROP_TARGET_VALUE` y
  `createCollectionDropZoneHandlers`; el highlight
  `.collection-row.drop-target.drag-over` y el preview flotante
  del ghost siguen siendo los dos feedbacks visuales del drop.
- [x] 14.4.3 `HistoryCardRail.svelte` sigue exponiendo
  `role="list"` y `aria-label="Historial reciente"`; la rail
  no cambió.
- [x] 14.4.4 `HistoryCard.svelte` mantiene `aria-selected`,
  `data-testid="history-card"`, `draggable="false"`,
  `bind:selectedEntryId` y la navegación con flechas
  (`ArrowLeft` / `ArrowRight`). El fix sólo afecta al sprite
  visible del overlay, no a la card.
- [x] 14.4.5 `App.svelte` sigue montando
  `<HistoryCardRail />` como el último hijo de
  `.layout-main` y sigue instalando los listeners capture-phase
  para `Escape` del preview y para `Cmd/Ctrl+Enter`. La altura
  reservada debajo de la rail sigue siendo `0` (`min-height: 0`
  en la regla `.layout-main`).
- [x] 14.4.6 `QuickPaste.svelte` no se tocó: orden cronológico,
  favoritos, navegación vertical con clamp, `Cmd/Ctrl+Enter`,
  meta-línea y shortcut hint siguen exactamente como en la
  sección 11/12.

### 14.5 Verificación automática de la regresión 14

- [x] 14.5.1 `cd app/tauri/frontend && npm test` ejecuta
  `1068` tests (los `1064` previos + `4` tests nuevos del
  bloque 14), 0 fallos.
- [x] 14.5.2 `cd app/tauri/frontend && npm run check` ejecuta
  el type-check de Svelte sin errores nuevos introducidos por
  este bloque. Los warnings y diagnósticos LSP preexistentes
  (`node:test`, `node:fs`, `process`, `String.repeat`,
  `clipboardAsset.entryFullPreviewText`,
  `clipboardAsset.entryRawContent`,
  `clipboardAsset.escapeForPreview`,
  `EntryRecord.code_language`) ya estaban antes del cambio y
  no son introducidos por esta sección.
- [x] 14.5.3 `cd app/tauri/frontend && npm run build` completa
  la generación de los bundles sin warnings nuevos.
- [x] 14.5.4 `cargo fmt --all -- --check`, `cargo clippy
  --workspace --all-targets -- -D warnings` y
  `cargo test --workspace` siguen verdes (ningún archivo de
  Rust se modificó en esta regresión).
- [x] 14.5.5 `openspec validate preview-interaction-regressions
  --strict --type change` responde "Change
  'preview-interaction-regressions' is valid".

### 14.6 Verificación visual manual

- [x] 14.6.1 Confirmar visualmente con la build de Tauri que el
  icono grande ya no aparece debajo de la lista horizontal de
  cards del Desktop, ni en el estado de error del backend, ni
  en el estado normal con capturas, ni al cambiar entre
  Historial y una colección de usuario, ni al abrir / cerrar el
  preview.

La verificación manual fue confirmada en la aplicación real: el icono grande
ya no aparece debajo de la rail en el estado normal, al cambiar de scope ni al
abrir/cerrar el preview.
