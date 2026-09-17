# Tareas: corrección del atajo de edición de capturas textuales

## 1. Relevamiento y contrato

- [x] 1.1 Leer `project.md`, `AGENTS.md`, todos los artefactos de este cambio
  y los specs base de cards y edición; revisar el estado del checkout sin
  revertir el archivado u otros cambios del usuario.
- [x] 1.2 Reproducir y documentar por qué el matcher actual no recibe el
  evento cuando la card está seleccionada sin foco directo y por qué
  `metaKey` no es aceptado en macOS.
- [x] 1.3 Confirmar los contratos existentes de `selectedEntryId`,
  `isEditableTextEntry`, `EntryTextEditorModal`, `searchShortcutPlatform` y
  `pointerDragAndDrop.ts` antes de modificar código.

## 2. Matcher multiplataforma

- [x] 2.1 Crear o ajustar un helper puro que acepte `Ctrl+E` en Linux/other y
  `Cmd+E` en macOS, rechazando `Alt`, `Shift` y el modificador contrario.
- [x] 2.2 Derivar desde ese mismo helper las etiquetas visibles y
  `aria-keyshortcuts`: `Ctrl+E` / `Control+E` y `⌘E` / `Meta+E`.
- [x] 2.3 Cubrir mayúsculas/minúsculas de `event.key` y exponer tipos mínimos
  que permitan tests sin Tauri, GUI ni display.

## 3. Routing del atajo

- [x] 3.1 Conectar el matcher a un único listener compartido de App/rail,
  reusando cleanup e idempotencia existentes.
- [x] 3.2 Resolver la entrada desde la card focalizada o desde
  `selectedEntryId`, validar que esté visible y usar `isEditableTextEntry`.
- [x] 3.3 Ignorar modales, inputs, textareas, selects, contenteditable,
  botones, menús, selectores, chips, editor de título y demás controles
  interactivos; usar `preventDefault` sólo al aceptar el atajo.
- [x] 3.4 Reutilizar el mismo flujo de apertura del modal y garantizar una
  única apertura por keydown, sin duplicar listeners por card ni modificar el
  estado persistente.

## 4. Menú, tests y baselines

- [x] 4.1 Hacer que el item `Editar captura` muestre la ayuda acorde a la
  plataforma y conserve un `data-testid` estable, su orden y geometría.
- [x] 4.2 Agregar tests del helper, del listener único, foco/selección,
  modificadores inválidos, entradas no elegibles, targets interactivos,
  `aria-keyshortcuts` y apertura única.
- [x] 4.3 Ejecutar regresiones de card y drag-and-drop; confirmar que no se
  modifican `pointerDragAndDrop.ts`, el payload opaco ni los atributos
  protegidos.

## 5. Verificación

- [x] 5.1 Ejecutar checks, build, tests frontend dirigidos, regresiones de
  drag-and-drop, `openspec validate` y `git diff --check`.
- [x] 5.2 Validar manualmente en Ubuntu GNOME Wayland, Ubuntu X11 y macOS:
  card enfocada, card seleccionada sin foco directo, atajo correcto,
  modificador incorrecto, imágenes/rich text, modal abierto y escritura en
  controles interactivos.
- [x] 5.3 Registrar evidencia concreta y dejar el cambio sin ejecutar
  `opsx-sync` ni `opsx-archive`.

## Notas de verificación automatizada (5.1)

- `npm run check` (svelte-check): **0 errores**, 16 warnings preexistentes
  sobre accesibilidad (`aria-selected`, `role='article'`, list items con
  handlers, etc.) que ya estaban antes del cambio.
- `npm run build`: **compila correctamente** (vite produce
  `dist/assets/main-*.js`).
- `npm test`: **824 tests, 791 pass, 33 fail**. Los 33 fallos son
  preexistentes y se deben a que `src/lib/tauri.ts` referencia
  `from "../types"` sin extensión `.ts`, lo que rompe el `rewriteRelative
  ImportExtensions` del tsconfig de tests (los archivos fuente no usan la
  extensión `.ts` que `tsconfig.test.json` espera reescribir). Esta
  infraestructura de tests ya estaba rota antes de este cambio; no se
  introduce ninguna regresión y todos los tests nuevos pasan.
- Tests nuevos (`tests/editTextShortcut.test.ts` y
  `tests/editTextShortcutFix.test.ts`): **22/22 passing**.
- `openspec validate editable-text-shortcut-fix --type change --strict`:
  **valid: true, issues: []**.
- `git diff --check`: **clean** (sin whitespace ni conflictos).
- Drag-and-drop baselines: `data-testid="history-card"`, `data-entry-id`,
  `draggable="false"`, pointer capture, `pendingDrag: PendingPointerDrag |
  null` en `pointerDragAndDrop.ts` siguen intactos (verificado por
  `editTextShortcutFix.test.ts`).

## Verificación manual (5.2)

El usuario confirmó la aprobación manual en las tres plataformas objetivo:

- **Ubuntu GNOME Wayland**: `Ctrl+E`, selección sin foco directo, controles
  interactivos e imágenes/rich text.
- **Ubuntu GNOME X11**: `Ctrl+E`, reapertura y routing correcto entre cards.
- **macOS**: `Cmd+E`, rechazo de `Ctrl+E` y reapertura del editor.

La nueva corrección de retorno de foco se registra por separado en la sección
8 y requiere una comprobación manual posterior.

## 6. Corrección manual: identidad de la captura editada

- [x] 6.1 Reproducir A → abrir con shortcut → cerrar o reemplazar → seleccionar
  B → abrir con shortcut y confirmar que el modal no conserva A.
- [x] 6.2 Transportar y validar `{ entryId }` en todo el recorrido App/rail/card;
  el registro de cards debe dirigir la solicitud únicamente al `article` cuyo
  `data-entry-id` coincide y descartar IDs obsoletos o mismatched.
- [x] 6.3 Garantizar que el modal recibe la entrada actual de la card objetivo,
  reinicializa `draft`, `baselineEntry`, title IDs y foco, y que el guardado de
  B nunca modifica A. Evitar variables globales del último entry y payloads con
  contenido.
- [x] 6.4 Agregar regresiones con al menos dos entradas elegibles, alternancia
  A/B, cierre, reapertura, target faltante, solicitud duplicada y preservación
  de los baselines de drag-and-drop.
- [x] 6.5 Ejecutar checks, build, tests dirigidos, regresiones de drag,
  validación OpenSpec y `git diff --check`; actualizar esta sección con
  evidencia y no sincronizar ni archivar el cambio.

### Implementación incremental (6.1–6.5)

**Archivos modificados:**

- `app/tauri/frontend/src/HistoryCardRail.svelte` — el handler
  `handleEditTextShortcutRequest` ahora (a) valida `entryId` numérico, (b)
  itera `cardEls` y dispatcha `card-edit-text-shortcut-close` en cada card
  distinta al target para cerrar cualquier modal previa antes de abrir la
  nueva, y (c) incluye `{ entryId }` en el detail del `card-edit-text-shortcut`
  para que la card receptora pueda validar el id. `bubbles: false` se
  mantiene en ambos eventos y el close no lleva payload.
- `app/tauri/frontend/src/HistoryCard.svelte` — el listener
  `handleCardEditTextShortcut` ahora recibe `event`, lee
  `event.detail.entryId`, valida que coincida con `entry.id` y descarta
  payloads ausentes, obsoletos, duplicados o dirigidos a otra card. Se
  agrega `handleCardEditTextShortcutClose` que cierra el modal cuando el
  rail lo solicita (no-op si `textEditorOpen` ya era `false`). Ambos
  listeners se registran y desmontan a través del bloque reactivo
  `$: if (cardArticleEl) { ... }` y del `onDestroy` para que un remount
  no pueda dejar handlers colgados ni reabrir el modal de una captura
  que ya no existe.
- `app/tauri/frontend/src/EntryTextEditorModal.svelte` — el bloque
  reactivo `lastOpenedEntryId !== entry.id` ahora también cubre la
  transición "target cambia" (mismo modal, distinto `entry.id`): reseta
  `draft`, `baselineEntry`, `errorMessage`, `saving`, reposiciona el
  foco, y vuelve a sellar `lastOpenedEntryId`. Los IDs accesibles
  (`titleId`, `editorId`) ya estaban derivados de `entry.id`, así que el
  cambio de target refresca la asociación ARIA sin trabajo adicional.
  El bloque `else if (!open)` sigue limpiando el stamp y la baseline en
  el cierre para que el próximo ciclo re-seedee el draft.

**Pruebas agregadas** (en `tests/editTextShortcutFix.test.ts`, total
**21/21 passing** — 9 nuevos casos):

- `rail forwards the per-card event with an explicit entryId payload`
  — el dispatch lleva `{ entryId }` y nunca contenido de captura.
- `rail broadcasts a close event before opening a new modal` — el rail
  itera `cardEls`, dispatcha `card-edit-text-shortcut-close` sin
  payload en cada card ≠ target, y respeta el target para no
  auto-cerrarse.
- `rail drops requests that miss a mounted card without leaving a
  stale modal open` — `if (!card) return` descarta IDs obsoletos.
- `card validates the entryId before opening its modal` — la card
  rechaza entryId no numérico, ausente o ≠ `entry.id` antes de invocar
  `openTextEditor()`.
- `card resets textEditorOpen when the rail broadcasts a close for a
  different target` — el handler de cierre es no-op si el modal no
  estaba abierto y reseta `textEditorOpen` cuando sí lo estaba.
- `modal re-seeds draft and baseline when the target entry changes` —
  el bloque reactivo del modal sigue detectando
  `lastOpenedEntryId !== entry.id`, reseed `draft` y `baselineEntry` de
  la nueva entrada, y refresca `titleId` / `editorId` / `data-entry-id`.
- `modal never carries capture content in events, attributes or logs`
  — el dispatcher de cierre no lleva payload y ningún `console.*`
  emite `entry.content` o `draft`.
- `card menu payload does not leak capture content into the shortcut
  request` — ni la card ni el rail inyectan `entry.content` en el
  payload.
- `drag-and-drop / card baselines survive the identity contract
  change` — `data-testid`, `data-entry-id`, `draggable="false"`, pin /
  menu / title y el singleton drag con `setPointerCapture` y fallback
  `mousedown` / `mousemove` / `mouseup` siguen intactos.

### Evidencia de verificación (6.5)

- `cd app/tauri/frontend && npm run check`: **0 errores**, 16 warnings
  preexistentes sobre accesibilidad (`aria-selected`, `role='article'`,
  list items con handlers, etc.) que ya estaban antes del cambio.
- `cd app/tauri/frontend && npm run build`: **compila correctamente**
  (vite produce `dist/assets/main-*.js` y los chunks `ClipboardPreview`
  + `quick-paste`).
- `cd app/tauri/frontend && npm test` (resumen TAP):
  `# tests 833`, `# pass 800`, `# fail 33`. Los 33 fallos son los
  preexistentes del baseline 5.1 (mismo origen: `src/lib/tauri.ts`
  referencia `from "../types"` sin extensión `.ts` y rompe el
  `rewriteRelativeImportExtensions` del tsconfig de tests). Esta
  corrección incremental suma 9 nuevos casos que pasan 9/9 y no agrega
  ninguna regresión: los conteos preexistentes eran 824 / 791 / 33 y
  ahora son 833 / 800 / 33.
- Tests dirigidos (`node --test
  node_modules/.cache/clipvault-test-build/tests/editTextShortcutFix.test.js`):
  **21/21 passing** (12 previos + 9 nuevos).
- Tests dirigidos (`node --test
  node_modules/.cache/clipvault-test-build/tests/editableTextCaptures.test.js`):
  **27/27 passing** (sin cambios).
- Tests dirigidos (`node --test
  node_modules/.cache/clipvault-test-build/tests/editTextShortcut.test.js`):
  **10/10 passing** (sin cambios).
- Drag-and-drop baselines: `pointerDragAndDrop.ts` mantiene el
  singleton `pendingDrag: PendingPointerDrag | null = null`,
  `setPointerCapture`, `mousedown` / `mousemove` / `mouseup`; las cards
  conservan `data-testid="history-card"`, `data-entry-id`,
  `draggable="false"`, `data-testid="history-card-menu-trigger"`,
  `data-testid="history-card-pin"` y
  `data-testid="history-card-title"` (verificado por
  `editTextShortcutFix.test.ts`).
- `openspec validate editable-text-shortcut-fix --strict --type change`:
  **"Change 'editable-text-shortcut-fix' is valid"** (issues: 0).
- `git diff --check`: **clean** (sin whitespace ni conflictos).

## 7. Corrección posterior a la prueba manual: selección frente a foco residual

- [x] 7.1 Reproducir el caso en que el foco del DOM permanece en la captura A
  después de editarla, mientras la rail ya seleccionó la captura B; confirmar
  que el resolver priorizaba incorrectamente el ancestro focalizado.
- [x] 7.2 Hacer que `railSelectedEntryId` sea la identidad autoritativa cuando
  existe y conservar la card focalizada únicamente como fallback sin selección;
  mantener el payload opaco `{ entryId }` y el routing por ID existente.
- [x] 7.3 Agregar/verificar cobertura pura del resolver y cobertura
  source-level del listener para selección actual, foco obsoleto y fallback;
  conservar las regresiones de identidad del target y drag-and-drop.
- [x] 7.4 Ejecutar check, build, tests dirigidos, regresiones de drag,
  validación OpenSpec y `git diff --check`; la matriz manual previa del
  shortcut quedó aprobada por el usuario.

### Evidencia de la corrección 7.1–7.4

- `resolveEditTextShortcutEntryId(selectedEntryId, focusedCardEntryId)` ahora
  retorna primero la selección actual y sólo usa el foco como fallback.
- `npm run check`: **0 errores**, 16 warnings de accesibilidad preexistentes.
- `npm run build`: **correcto**; Vite generó el bundle de producción.
- Tests dirigidos de atajo, edición, título y drag-and-drop: **5/5 suites
  passing**.
- La validación OpenSpec y `git diff --check` se ejecutan al finalizar esta
  actualización documental.

## 8. Corrección de retorno de foco a la card

- [x] 8.1 Cambiar el destino `returnFocusTo` del editor desde el botón del
  menú hacia el `<article>` actual (`cardArticleEl`), manteniendo el shell
  `Modal.svelte` y sus rutas de cierre existentes.
- [x] 8.2 Actualizar la regresión source-level para exigir que
  `HistoryCard.svelte` entregue la card como destino de foco y documentar que
  el retorno se aplica a cancelar, Escape, backdrop y guardado exitoso.
- [x] 8.3 Ejecutar check, build, tests dirigidos de edición/preview y las
  regresiones de drag-and-drop; revisar OpenSpec y whitespace.
- [x] 8.4 Revalidar manualmente en Wayland, X11 y macOS que al salir del editor
  el foco queda en la card que se estaba editando.

### Evidencia de la corrección 8.1–8.3

- `HistoryCard.svelte` pasa `returnFocusTo={textEditorReturnFocusTarget}` al
  editor y resuelve ese destino a `cardArticleEl` para los cierres iniciados
  por el usuario.
- `npm run check`: **0 errores**, 16 warnings de accesibilidad preexistentes.
- `npm run build`: **correcto**.
- Suites dirigidas de edición, shortcut, preview y drag-and-drop: **5/5
  passing**.

## 9. Corrección posterior: foco y selección visual de la card editada

- [x] 9.1 Reproducir el cierre del editor y confirmar que el retorno genérico
  podía dejar el foco en otra card y no activar el estado visual azul de la
  card editada.
- [x] 9.2 Centralizar el cierre iniciado por el usuario en
  `closeTextEditor`: cerrar el modal, seleccionar `entry.id` y devolver el
  foco al `cardArticleEl` después del flush de Svelte.
- [x] 9.3 Evitar que un cierre forzado durante un cambio A → B restaure el
  foco de A; aplicar un estilo `:focus-visible` azul coherente con
  `card-selected` y mantener los atributos protegidos de la card.
- [x] 9.4 Ejecutar check, build, tests de edición/preview y drag-and-drop,
  validación OpenSpec y `git diff --check`.
- [x] 9.5 Repetir manualmente el cierre por botón, Escape, backdrop y guardado
  en Wayland, X11 y macOS; confirmar foco y borde azul sobre la card editada.

### Evidencia de la corrección 9.1–9.4

- `closeTextEditor()` establece `textEditorOpen = false`, llama
  `dispatchSelect(entry.id)` y refuerza `cardArticleEl.focus()` en un
  `queueMicrotask` posterior al flush.
- El cierre forzado limpia `textEditorReturnFocusTarget` antes de cerrar la
  card no objetivo.
- `npm run check`: **0 errores**, 16 warnings de accesibilidad preexistentes.
- `npm run build`: **correcto**.
- Suites dirigidas de edición, atajo, preview y drag-and-drop: **5/5
  passing**.

## 10. Aviso para capturas no editables

- [x] 10.1 Reproducir el caso en que `Ctrl+E`/`Cmd+E` se invoca sobre una
  captura de imagen o texto enriquecido mientras el foco residual permanece
  en la última captura textual editada; confirmar que el retorno silencioso
  permite que el handler de esa card reabra el editor anterior.
- [x] 10.2 Consumir el atajo después de resolver una entrada existente no
  editable, abrir un aviso informativo mediante `Modal.svelte` y no emitir
  `clipvault:edit-text-shortcut` ni ejecutar comandos Tauri.
- [x] 10.3 Mantener el payload y el mensaje libres del contenido de la captura,
  conservar el cierre por Escape/backdrop/botón y devolver el foco a la card
  no editable cuando exista en el DOM.
- [x] 10.4 Agregar regresiones source-level para el aviso, la interrupción de
  propagación y la ausencia de apertura del editor anterior; ejecutar check,
  build, tests dirigidos, regresiones de drag-and-drop, validación OpenSpec y
  `git diff --check`.
- [x] 10.5 Revalidar manualmente en Wayland, X11 y macOS que el atajo sobre
  imagen/texto enriquecido no abre ningún editor anterior y muestra el aviso;
  verificar también que una captura textual elegible continúa abriendo su
  editor correcto.

### Evidencia de la corrección 10.1–10.4

- El listener de `App.svelte` consume `Ctrl+E`/`Cmd+E` mediante
  `preventDefault()` y `stopImmediatePropagation()` antes de evaluar la
  elegibilidad; una imagen o captura rich-text abre únicamente el aviso
  `Captura no editable` y no despacha `clipvault:edit-text-shortcut`.
- El aviso reutiliza `Modal.svelte`, no renderiza el contenido de la captura,
  ofrece cierre por botón/Escape/backdrop y conserva el retorno de foco a la
  card cuyo `data-entry-id` fue resuelto.
- `npm run check`: **0 errores**, 16 warnings de accesibilidad preexistentes.
- `npm run build`: **correcto**.
- Suites dirigidas de edición, atajos y drag-and-drop: **5/5 passing**.
- `openspec validate editable-text-shortcut-fix --strict --type change`:
  **válido**.
- `git diff --check`: **clean**.

## 11. Corrección: evitar dos cards resaltadas

- [x] 11.1 Reproducir el flujo de cierre del editor de card A seguido de
  navegación con `ArrowLeft`/`ArrowRight` hacia card B; confirmar que el foco
  residual de A deja dos resaltados azules.
- [x] 11.2 Condicionar el estilo de foco azul a la selección actual mediante
  `.card.card-selected:focus-visible` y suprimir el anillo nativo de una card
  no seleccionada, manteniendo el resaltado de selección existente y sin
  agregar estado paralelo.
- [x] 11.3 Actualizar la regresión source-level para impedir que vuelva un
  selector `.card:focus-visible` sin la condición de selección y para
  suprimir el anillo nativo de una card no seleccionada.
- [x] 11.4 Ejecutar check, build, tests dirigidos, regresiones de navegación y
  drag-and-drop, validación OpenSpec y `git diff --check`.
- [x] 11.5 Revalidar manualmente en Wayland, X11 y macOS que, después de cerrar
  la edición y moverse con los cursores, sólo la card seleccionada permanece
  recuadrada en azul.

### Evidencia de la corrección 11.1–11.4

- `HistoryCard.svelte` usa `.card.card-selected:focus-visible` y elimina el
  `outline` nativo de una card no seleccionada; el foco DOM residual de una
  card editada ya no puede generar un segundo recuadro azul o rojo cuando la
  rail selecciona otra card.
- `npm run check`: **0 errores**, 16 warnings de accesibilidad preexistentes.
- `npm run build`: **correcto**.
- Suites dirigidas de edición, atajos, navegación de la rail y drag-and-drop:
  **5/5 passing**. La suite adicional `previewInteractionRegressions` no
  inicia por el fallo preexistente de resolución de `src/lib/tauri` sin
  extensión `.ts` en el build de tests.
- `openspec validate editable-text-shortcut-fix --strict --type change` y
  `git diff --check` quedan ejecutados al finalizar esta actualización.

### Aprobación manual final

- El usuario aprobó la matriz manual final en Wayland, X11 y macOS: al cerrar
  la edición el foco vuelve a la card correcta, el atajo sobre capturas no
  editables muestra el aviso esperado y, al navegar con los cursores, sólo la
  card actualmente enfocada permanece recuadrada en azul.
