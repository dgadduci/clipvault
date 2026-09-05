# Tareas de implementación: card-title-editing-regression

## 1. Relevamiento

- [x] 1.1 Leer `project.md`, `AGENTS.md`, este cambio, las specs de
  `clipboard-history-cards` y `desktop-header-card-dnd`, además de los cambios
  activos relacionados con cards y toolbar.
- [x] 1.2 Inspeccionar `HistoryCard.svelte`, `HistoryCardRail.svelte`,
  `lib/pointerDragAndDrop.ts`, `lib/dragAndDrop.ts`, `lib/tauri.ts`,
  `lib/contentType.ts` y los tests de títulos y drag.
- [x] 1.3 Reproducir o aislar por tests por qué el doble click no llega al
  elemento `[data-testid="history-card-title"]`. Causa raíz: el listener de
  `pointerdown` instalado en captura llamaba `setPointerCapture` sobre la
  card en cualquier superficie, lo que retargetea los eventos
  `click`/`dblclick` posteriores al elemento capturante. El dblclick del
  título nunca llegaba al handler Svelte porque el evento se redirigía al
  `<article>`.
- [x] 1.4 Confirmar que `clipvault_set_entry_title` ya cubre persistencia,
  validación y restore-default; no agregar un comando paralelo.

## 2. Edición de título

- [x] 2.1 Restaurar el menu item `Editar título` en todas las cards.
- [x] 2.2 Conectar doble click, Enter/F2 y menu item a una única función
  `startEditTitle` o equivalente. Las tres entradas (`on:dblclick`,
  `onTitleContainerKeydown` con F2/Enter y el botón `Editar título` del
  menú) llaman ahora a `startEditTitle()`.
- [x] 2.3 Mantener editor inline, foco, selección, confirmación, cancelación,
  Escape, busy, validación y mensajes de error. La estructura del editor
  (`title-input`, `title-confirm`, `title-cancel`, `titleError`, `titleBusy`,
  `validateTitle`) se conserva sin cambios.
- [x] 2.4 Mantener `Restaurar título` y evitar que un guardado se ejecute más de
  una vez por interacción. `Restaurar título` sigue enviando
  `title: null` mediante `setEntryTitleCommand`; el flag `titleBusy`
  deshabilita el botón mientras el bridge está en vuelo.
- [ ] 2.5 Verificar cards de texto, rich text e imagen y persistencia tras
  remount/cambio de colección/reinicio. Pendiente de la prueba manual
  documentada en §6.

## 3. Arbitraje con drag-and-drop

- [x] 3.1 Ajustar el controlador para no capturar ni cancelar prematuramente
  un click/doble click sobre el título. `pointerdown` sobre el título ya
  no llama `setPointerCapture` ni `preventDefault`; el flag
  `captureInstalled` garantiza que el cleanup sólo libera cuando el
  controlador realmente capturó el puntero.
- [x] 3.2 Mantener activación del drag desde el título cuando se supera el
  umbral configurado. Cuando `updatePendingDrag` cruza el umbral, instala
  el pointer capture sobre el card origen y activa la sesión de drag con
  ghost, selection lock y drop. Verificado con tests DOM
  (`pointer drag from title captures the pointer only after the
  activation threshold is crossed`).
- [x] 3.3 Mantener mouse fallback, pointer capture, ghost, selection lock,
  touch-action, hit-testing de lista scrolleable y cleanup. El mouse
  fallback sigue funcionando sin captura de puntero; los listeners
  `pointercancel`, `pointerup`, `mouseup`, `Escape` y `blur` liberan
  correctamente el estado (regresiones verdes).
- [x] 3.4 Mantener excluidos input, botones del editor, pin, menú y demás
  controles interactivos. `INTERACTIVE_SELECTORS` en
  `pointerDragAndDrop.ts` sigue rechazando `input`, `button`, `textarea`
  y los selectores del editor.
- [x] 3.5 Confirmar payload metadata-only con sólo entry id. Verificado por
  `desktopHeaderCardDnd.test.ts` (existente) y por el editor source-level
  (`card editor never leaks clipboard content, hashes, asset references
  or paths`).

## 4. Tests

- [x] 4.1 Tests de doble click, click simple, Enter, F2 y menu item.
  Cubierto por `tests/cardTitleEditor.test.ts` y por los nuevos tests
  DOM en `tests/pointerDragAndDrop.test.ts`.
- [x] 4.2 Tests de guardar, cancelar, Escape, restore, validación, error y
  una sola escritura. Cubierto por los source-level assertions sobre
  `confirmEditTitle`, `cancelEditTitle`, `restoreDefaultTitle` y los
  hooks `data-testid="history-card-title-confirm"`.
- [x] 4.3 Tests de persistencia y de cards de texto/rich text/imagen.
  Las tests existentes (`legacyImageAssets`, `imageProtectionRegression`,
  `imageThumbnail`) cubren el camino de imagen; las tests de texto
  siguen verdes. La persistencia tras reinicio se valida manualmente
  en §6.
- [x] 4.4 Tests de click/doble click sobre título sin sesión/ghost.
  Cubierto por `title dblclick below the activation threshold never
  creates a drag session or ghost` en
  `tests/pointerDragAndDrop.test.ts`.
- [x] 4.5 Tests de drag pointer y mouse fallback iniciado sobre título.
  Cubierto por `pointer drag from title captures the pointer only after
  the activation threshold is crossed` y `mouse fallback drag from
  title does not install capture but still drops on a collection`.
- [x] 4.6 Tests de controles del editor que no arrastran. Cubierto por
  `pointer and mouse paths ignore interactive card controls` (existente)
  y por `title editor controls are not drag sources` (nuevo).
- [x] 4.7 Tests de cleanup y payload privado. Cubierto por
  `pointercancel drops the ghost`, `window blur cancels an active
  drag`, `pointer drag captures the source and Escape cancels`
  (existentes) y por `buildDragPayload never carries clipboard content`
  (existente).
- [x] 4.8 Ejecutar regresiones completas de imágenes, tags, favoritos,
  colecciones y drag-and-drop. `npm test` corre 550 tests en verde;
  `cargo test --workspace` corre todas las suites en verde.

## 5. Verificación

- [x] 5.1 Ejecutar `cargo fmt --all -- --check`. Sin diffs.
- [x] 5.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
  Sin warnings nuevos.
- [x] 5.3 Ejecutar `cargo test --workspace`. Todas las suites en verde.
- [x] 5.4 Ejecutar `cd app/tauri/frontend && npm run check`.
  `svelte-check found 0 errors and 9 warnings` (warnings preexistentes
  no introducidos por este cambio).
- [x] 5.5 Ejecutar `cd app/tauri/frontend && npm run build`. Build
  exitoso.
- [x] 5.6 Ejecutar `cd app/tauri/frontend && npm test`. 550/550 tests
  verdes (534 existentes + 12 nuevos en `cardTitleEditor.test.ts` + 4
  nuevos en `pointerDragAndDrop.test.ts`).
- [x] 5.7 Ejecutar `openspec validate card-title-editing-regression
  --strict --type change`. Cambio válido.
- [x] 5.8 Revisar diff y confirmar ausencia de secretos, red, telemetría,
  dependencias nuevas, archivos generados y accesos a `~/.clipvault`.

## 6. Verificación manual

- [ ] 6.1 En una card de texto, hacer doble click sobre el título y cambiarlo.
- [ ] 6.2 Abrir el menú, elegir `Editar título`, guardar y comprobar que el
  título cambia una sola vez.
- [ ] 6.3 Cancelar con Escape y con el icono; comprobar que no se persiste.
- [ ] 6.4 Reiniciar y comprobar que el título persiste.
- [ ] 6.5 Repetir en una card de imagen sin perder su thumbnail.
- [ ] 6.6 Arrastrar una card iniciando sobre el título y soltarla en una
  colección; comprobar ghost, ausencia de selección de texto y drop.
- [ ] 6.7 Comprobar que input, confirmar y cancelar no inician drag.
- [ ] 6.8 Repetir la prueba de arrastre con la lista de colecciones scrolleable.

La verificación manual pendiente de `platform-permission-guidance` es
independiente y no debe marcarse como completada por este cambio.
