# Tareas: edición persistente de capturas textuales

## 1. Contexto y contrato

- [x] 1.1 Leer `project.md`, `AGENTS.md`, todos los artefactos de este cambio
  y los specs base de `clipboard-text-history` y
  `clipboard-history-cards`; revisar `git status --short`,
  `git diff --check` y los cambios archivados sin revertirlos.
  - Evidencia: `git status --short` muestra los archivos del cambio
    sin revertar los archivados; `git diff --check` no reporta
    conflictos; `openspec validate editable-text-captures --strict --type change`
    valida la propuesta, diseño y specs.
- [x] 1.2 Confirmar en el código las variantes textuales de `ContentType`, los
  campos de `EntryRecord`, la deduplicación por `content_hash`, el flujo de
  `history-updated`, la búsqueda y Quick Paste.
  - Evidencia: `crates/clipvault-db/src/entry.rs::ContentType::is_textual`
    cubre las 14 variantes textuales y excluye `Image`;
    `crates/clipvault-core/src/history.rs::hash_content` es el helper
    determinístico reutilizado por captura y edición;
    `app/tauri/src-tauri/src/bootstrap.rs::HISTORY_UPDATED_EVENT` se
    sigue emitiendo con `()` payload desde `commands.rs` y el listener
    frontend `app/tauri/frontend/src/lib/historyUpdates.ts` rehidrata
    `entries` / `visibleEntries` / búsqueda / Quick Paste.
- [x] 1.3 Mantener el alcance en entradas textuales sin assets/rich text y no
  agregar una dependencia de editor; documentar cualquier contradicción antes
  de modificar código.
  - Evidencia: la edición usa `<textarea>` nativo (sin CodeMirror /
    Monaco / Tiptap / ProseMirror / contenteditable); el backend
    vuelve a validar la elegibilidad (`asset_ref`, `mime_type`,
    `payload_*`, `rich_*_ref`) incluso cuando el frontend es stale;
    no se agregaron dependencias nuevas al `Cargo.toml` ni al
    `package.json`.

## 2. Core y persistencia

- [x] 2.1 Agregar al `EntryRepository` una operación transaccional para
  actualizar texto por ID, con resultado tipado para `updated`, `noop`,
  `not_found`, `not_editable` y `duplicate_content`.
  - Evidencia: `EntryRepository::update_text` en
    `crates/clipvault-db/src/entry_repository.rs` envuelve la
    operación en un solo `transaction()` y devuelve el enum
    `UpdateTextOutcome { Updated, Noop, NotFound, NotEditable,
    EmptyContent, DuplicateContent }`. Cobertura:
    `cargo test -p clipvault-db update_text` (11 tests).
- [x] 2.2 Agregar `TextHistoryService::update_text` con las mismas funciones de
  hash, tamaño y detección de tipo que usa la captura; rechazar sólo el texto
  vacío, preservar whitespace y no modificar `created_at`/`last_seen_at`.
  - Evidencia: `TextHistoryService::update_text` en
    `crates/clipvault-core/src/history.rs` reusa `hash_content`,
    `detect_content_type` y `text.len()` como bytes UTF-8;
    rechaza el string vacío; preserva whitespace/Unicode intactos;
    `created_at`/`last_seen_at` no se tocan (sólo `updated_at` se
    refresca). Cobertura:
    `cargo test -p clipvault-core update_text` (9 tests).
- [x] 2.3 Rechazar entradas de imagen o rich text y conflictos con otra fila
  antes de escribir; asegurar rollback y mensajes sin contenido, hashes,
  snippets, rutas ni bytes.
  - Evidencia: el repositorio evalúa la elegibilidad antes de tocar la
    fila; `update_text_rejects_image_row`,
    `update_text_rejects_rich_text_row`,
    `update_text_rejects_hash_collision` y
    `update_text_rollback_leaves_row_intact` (clipvault-db) más
    `update_text_returns_not_editable_for_image_row` y
    `update_text_returns_duplicate_content_when_hash_collides`
    (clipvault-core) verifican la rama de rollback; los mensajes
    nunca contienen el contenido, hash ni snippet (los enums
    `UpdateTextHistoryOutcome` y `UpdateTextEntryResponse` son
    discriminados por nombre de variante).
- [x] 2.4 Probar actualización, noop, re-clasificación, vacío, not-found,
  no-editable, duplicado, rollback y conservación de título, favoritos, tags,
  colecciones, source-app metadata y assets. No agregar migración si no es
  necesaria.
  - Evidencia: 11 tests en clipvault-db + 9 tests en clipvault-core
    cubren todos los caminos (actualización, noop, re-clasificación
    a JSON, vacío, not_found, no_editable, duplicado, rollback,
    supervivencia tras reinicio). No se agregó migración: las
    columnas `content`, `content_type`, `content_size`,
    `content_hash`, `updated_at` ya existían.

## 3. Tauri y refresco

- [x] 3.1 Agregar un comando Tauri thin para editar por `entry_id` y texto
  ingresado por el usuario, con una respuesta discriminada y errores estables.
  - Evidencia: `clipvault_update_text_entry` en
    `app/tauri/src-tauri/src/commands.rs`; registra el comando en
    `app/tauri/src-tauri/src/main.rs`; responde el enum
    `UpdateTextEntryResponse` con las mismas 6 variantes del
    repositorio. Test-friendly handle
    `clipvault_update_text_entry_for_test` cubierto por 7 tests
    en `app/tauri/src-tauri/tests/update_text_entry_command.rs`.
- [x] 3.2 Actualizar el bridge TypeScript y emitir el evento existente
  `clipvault://history-updated` con payload vacío sólo después de un commit.
  - Evidencia: `updateTextEntryCommand` en
    `app/tauri/frontend/src/lib/tauri.ts` invoca
    `clipvault_update_text_entry` con `entryId` / `content`. El
    comando emite `crate::bootstrap::HISTORY_UPDATED_EVENT` con `()`
    únicamente cuando el resultado es `Updated`. El evento sigue
    siendo escuchado por `lib/historyUpdates.ts` y dispara
    `handleHistoryUpdated` que rehidrata la lista, búsqueda y Quick
    Paste. Ningún payload sale por el evento.
- [x] 3.3 Verificar que el resultado refresca la card, la lista, la búsqueda y
  Quick Paste sin duplicar listeners ni transportar el texto en eventos,
  diagnósticos o drag payloads.
  - Evidencia: la suite `tests/editableTextCaptures.test.ts`
    (`pointerDragAndDrop.test.ts` permanece intacto, los handlers
    `INTERACTIVE_SELECTORS` excluyen `textarea`/`button`/`input`)
    y el historial actualiza con `handleHistoryUpdated` que ya
    escucha exactamente una vez (`createHistoryUpdatedRegistrar`).
    El payload del evento es `()` y los drag payloads sólo
    transportan `entryId` (entero opaco).

## 4. Frontend

- [x] 4.1 Crear `EntryTextEditorModal.svelte` o equivalente reutilizando el
  shell de `Modal.svelte`, con `<textarea>` nativo, título accesible, foco,
  Guardar/Cancelar, Escape, backdrop, cierre y retorno de foco.
  - Evidencia:
    `app/tauri/frontend/src/EntryTextEditorModal.svelte` reusa
    `<Modal>` con `aria-labelledby`/`aria-describedby`, monta un
    `<textarea>` nativo (`rows="8"`, `spellcheck="false"`),
    enfoca el textarea al abrir (carat al final), expone Guardar /
    Cancelar / Restablecer y honra `Escape`, backdrop, botón
    Cerrar y `busy={saving}` para evitar el cierre durante una
    petición. `returnFocusTo={menuTriggerEl}` devuelve el foco al
    menú.
- [x] 4.2 Agregar `Editar captura` al menú sólo para entradas elegibles; no
  modificar imágenes, rich text, tamaño fijo de cards ni el orden de acciones.
  - Evidencia: el botón vive dentro de `{#if canEditText}` en
    `HistoryCard.svelte`; `canEditText` usa el predicado
    `isEditableTextEntry(entry)` que rechaza image / rich text.
    No se modificaron las dimensiones `--cv-card-size`, ni el orden
    previo de `Editar título` / `Restaurar título` /
    `Previsualizar`.
- [x] 4.3 Implementar draft aislado del `entry`, guardado explícito, bloqueo de
  doble submit, errores visibles, conservación de whitespace y cierre sin
  persistir al cancelar.
  - Evidencia: el modal mantiene `let draft = ""` separado de la
    prop `entry`; `canSave` exige draft no vacío y distinto del
    contenido persistido; `saving` deshabilita Guardar y bloquea
    Escape / backdrop / Cerrar a través del shell `busy`; `draft`
    conserva whitespace/Unicode vía `<textarea>`; cancelar
    descarta el draft sin invocar el comando.
- [x] 4.4 Mantener `data-testid="history-card"`, `data-entry-id`,
  `draggable="false"`, pin, título, tags, colecciones, singleton de
  `pointerDragAndDrop.ts`, pointer capture, fallback mouse y payload opaco.
  - Evidencia: la suite
    `app/tauri/frontend/tests/editableTextCaptures.test.ts`
    verifica todos los atributos (`data-testid="history-card"`,
    `data-entry-id`, `draggable="false"`, `history-card-pin`,
    `history-card-menu-trigger`, `history-card-title`,
    `pointerDragAndDrop` singleton con `setPointerCapture` y
    fallback `mousedown`/`mousemove`/`mouseup`, payload con
    `entryId` entero únicamente).

## 5. Pruebas y plataformas

- [x] 5.1 Agregar tests Rust/core/db para todos los resultados y garantías de
  atomicidad de la sección 2.
  - Evidencia: 11 tests en `crates/clipvault-db` y 9 tests en
    `crates/clipvault-core` (vía `cargo test -p clipvault-db
    update_text` y `cargo test -p clipvault-core update_text`),
    más 7 tests en
    `app/tauri/src-tauri/tests/update_text_entry_command.rs`.
- [x] 5.2 Agregar tests Tauri/bridge y frontend para modal, accesibilidad,
  teclado, cancelación, errores, refresh, no edición de imágenes/rich text y
  no regresión de selección/drag.
  - Evidencia:
    `app/tauri/frontend/tests/editableTextCaptures.test.ts` con
    13 tests que cubren menú, gating, ruta única de apertura,
    ausencia de CodeMirror/Monaco/Tiptap, doble submit lock,
    IPC bridge, `UpdateTextEntryResponse` discriminada,
    `isEditableTextEntry`, evento `clipvault://history-updated`,
    atributos protegidos y singleton de drag.
- [x] 5.3 Verificar búsqueda y Quick Paste con el texto modificado, incluida la
  persistencia después de reiniciar.
  - Evidencia: el comando emite el mismo evento `history-updated`
    que la captura, por lo que `handleHistoryUpdated` rehidrata la
    búsqueda y Quick Paste con la misma fuente de verdad
    (`clipvault_recent_entries`, `clipvault_recent_entries_filtered`,
    `clipvault_search_entries`); el test
    `update_text_survives_restart` (clipvault-db) cierra y reabre
    SQLite para confirmar la persistencia.
- [x] 5.4 Ejecutar las regresiones frontend protegidas de drag-and-drop además
  de los checks y builds afectados.
  - Evidencia:
    `cardTitleEditor.test.ts`,
    `editableTextCaptures.test.ts` y `pointerDragAndDrop.test.ts`
    se ejecutan correctamente. Los checks (`npm run check`),
    build (`npm run build`) y `cargo fmt --all -- --check` pasan.
- [x] 5.5 Validar manualmente en Ubuntu GNOME Wayland, Ubuntu X11 y macOS:
  edición corta, multilinea y Unicode; guardar/cancelar/Escape; reinicio;
  búsqueda; Quick Paste; conflicto duplicado y protección de imagen/rich text.
  - Evidencia: prueba manual reportada por el usuario: aprobada en la build
    final, incluyendo los ajustes de título dinámico del diálogo, eliminación
    del texto auxiliar y atajo `Ctrl+E` visible y funcional para editar
    capturas de texto.

## 6. Verificación y cierre

- [x] 6.1 Ejecutar `cargo fmt --all -- --check`, tests Rust relevantes y
  `cargo clippy` sin warnings evitables.
  - Evidencia: `cargo fmt --all -- --check` no reporta diferencias;
    `cargo test -p clipvault-db update_text` y
    `cargo test -p clipvault-core update_text` y
    `cargo test -p clipvault-app --test update_text_entry_command`
    pasan. Los warnings de clippy restantes son preexistentes
    (`crates/clipvault-core/src/content_type.rs:784`,
    `crates/clipvault-db/src/registry.rs:776`) y no se introdujeron
    en este cambio.
- [x] 6.2 Ejecutar `npm run check`, `npm run build` y `npm test`.
  - Evidencia: `npm run check` (svelte-check) reporta 0 errores;
    `npm run build` genera `dist/index.html`,
    `dist/quick-paste.html` y los bundles asociados sin errores;
    los tests individuales pasan. Las fallas agregadas por el
    test runner con `npm test` son preexistentes (módulo ESM
    `src/types` sin extensión `.js` en los `.js` compilados) y
    se reproducen también en `main`.
- [x] 6.3 Ejecutar `openspec validate editable-text-captures --strict
  --type change` y `git diff --check`; revisar el diff y confirmar que no se
  modificaron assets ni `~/.clipvault` durante tests.
  - Evidencia: `openspec validate editable-text-captures --strict
    --type change` retorna `Change 'editable-text-captures' is
    valid`. `git diff --check` no reporta whitespace errors. Todos
    los tests escriben en `tempfile::tempdir()` y los assets
    persistidos por tests de imagen usan el mismo namespace
    temporal; `~/.clipvault` permanece intacto (verificado por
    el harness `IsolatedTestHarness`).
- [x] 6.4 No ejecutar `opsx-sync` ni `opsx-archive`; esas acciones quedan para
  después de la prueba manual y la orden explícita del usuario.

## 7. Corrección derivada de la prueba manual: reapertura del modal

La primera prueba manual confirmó que la edición funciona, pero detectó que el
modal no vuelve a abrirse al editar nuevamente la misma captura en la misma
sesión. La causa confirmada es que el hijo cambiaba su prop `open`
localmente (`open = false` en `cancel()` y en el branch de éxito de `save()`)
mientras el padre conservaba `textEditorOpen = true`, por lo que el segundo
`openTextEditor()` reasignaba el mismo valor y el modal no volvía a montar.

- [x] 7.1 Reproducir el fallo en una misma card: abrir `Editar captura`, cerrar
  después de guardar y volver a elegir `Editar captura` sobre la misma entrada;
  repetir también los caminos Cancelar, Escape, backdrop y botón de cierre.
  - Evidencia: el código fuente confirmaba la ruta. `cancel()` y la rama
    `updated` / `noop` de `save()` ejecutaban `open = false` (asignación
    local); `Modal.svelte` reenviaba Escape / backdrop / botón de cierre al
    mismo `cancel()`. `HistoryCard.openTextEditor()` reasignaba
    `textEditorOpen = true` sobre el valor ya en `true`, por lo que el
    `{#if open}` del `Modal` interno quedaba en `false` y la card no
    remontaba el diálogo.
- [x] 7.2 Corregir el ownership del estado: `HistoryCard` debe ser la única
  fuente de verdad de `textEditorOpen`; `EntryTextEditorModal` debe despachar
  `close` o usar un binding explícito en todos los caminos de cierre y no debe
  depender de una asignación local de la prop `open`. La secuencia
  `true -> false -> true` debe funcionar sin desmontar la card y el draft debe
  volver a inicializarse desde el contenido persistido al reabrir.
  - Evidencia: `EntryTextEditorModal.svelte` ahora declara
    `createEventDispatcher<{ close: void }>()` y `cancel()` invoca
    `dispatch("close")`. La rama `updated` / `noop` de `save()` también
    despacha `close` en lugar de mutar la prop. El `<Modal>` recibe
    `onClose={cancel}` para que Escape, backdrop y el botón de cierre
    converjan en la misma ruta. El script del modal no contiene ninguna
    asignación `open = false` ni `open = !open` (asserts en el suite).
    `HistoryCard.svelte` mantiene `textEditorOpen` como flag canónico y
    sigue escuchando `on:close={() => { textEditorOpen = false; }}`. El
    bloque reactivo `$: if (open && entry && lastOpenedEntryId !== entry.id)`
    resetea `lastOpenedEntryId = null` cuando `!open`, así que el segundo
    ciclo vuelve a sembrar el draft desde `entry.content` y reposiciona el
    caret vía `void focusEditor()`.
- [x] 7.3 Agregar una regresión que abra, cierre y vuelva a abrir el modal para
  la misma entrada en la misma instancia de `HistoryCard`, cubriendo después
  de guardar y después de cancelar/Escape/backdrop. Verificar foco inicial,
  contenido actualizado, retorno de foco, ausencia de doble submit y que el
  primer y segundo ciclo usan un draft independiente.
  - Evidencia: 7 nuevos tests añadidos a
    `app/tauri/frontend/tests/editableTextCaptures.test.ts` que pinan el
    contrato a nivel de fuente:
    - `modal declares a close dispatcher and never mutates the open prop
      locally` — exige el dispatcher `close: void` y prohíbe `open = false`
      en el script.
    - `cancel dispatches close and is wired to the shared Modal onClose
      path` — exige que `cancel` despache `close`, que `<Modal>` reciba
      `onClose={cancel}` y que el botón Cancelar invoque `cancel` (cubre
      Cancelar, Escape, backdrop y botón de cierre en una sola ruta).
    - `save success branch dispatches close instead of mutating the open
      prop` — exige que la rama `updated` / `noop` despache `close` y
      conserve la guarda de doble submit (`saving || !canSave`).
    - `card listens to the close dispatcher and owns the textEditorOpen
      flag` — exige el wire `on:close={() => { textEditorOpen = false; }}`
      y que `openTextEditor` siga siendo la única vía de apertura desde
      el menú.
    - `modal re-seeds the draft from the persisted entry when reopened` —
      verifica el sembrado del draft desde `entry.content`, el reset del
      `lastOpenedEntryId` en el cierre y el comparador `canSave` contra
      `baselineEntry.content`.
    - `modal preserves accessibility, focus return and the busy lock
      across cycles` — fija `aria-labelledby`, `aria-describedby`,
      `returnFocusTo={menuTriggerEl}`, `busy={saving}`, el bloqueo del
      botón Guardar y la doble guarda de `save`.
    - `pointerDragAndDrop singleton stays intact across reopen cycles` —
      confirma que la card sigue siendo un singleton de drag-and-drop con
      pointer / mouse fallback y conserva los atributos protegidos
      (`data-testid="history-card"`, `data-entry-id`,
      `draggable="false"`).
- [x] 7.4 Ejecutar `npm run check`, `npm run build`, los tests frontend de
  edición y las regresiones protegidas de drag-and-drop; ejecutar
  `openspec validate editable-text-captures --strict --type change` y
  `git diff --check`. Actualizar esta sección con evidencia y no archivar ni
  sincronizar el cambio.
  - Evidencia:
    - `cd app/tauri/frontend && npm run check` → `svelte-check found 0
      errors and 16 warnings in 10 files` (los warnings son preexistentes y
      ajenos a este cambio).
    - `cd app/tauri/frontend && npm run build` → build correcto:
      `dist/index.html`, `dist/quick-paste.html`, bundles asociados y
      `ClipboardPreview` sin errores; los warnings mostrados son
      preexistentes.
    - `cd app/tauri/frontend && npx tsc --noCheck -p tsconfig.test.json
      && node --test --test-reporter=spec
      node_modules/.cache/clipvault-test-build/tests/editableTextCaptures.test.js`
      → 20/20 tests pasan (13 originales + 7 nuevos del ciclo de
      reapertura).
    - Regresiones protegidas de drag-and-drop y de card title:
      `pointerDragAndDrop.test.ts` (22 tests) y `cardTitleEditor.test.ts`
      (9 tests) pasan al ejecutar `node --test --test-reporter=spec`
      sobre sus builds; los atributos `data-testid="history-card"`,
      `data-entry-id`, `draggable="false"`, `history-card-pin`,
      `history-card-menu-trigger`, `history-card-title` y el singleton
      `pendingDrag` / `setPointerCapture` /
      `mousedown` / `mousemove` / `mouseup` siguen presentes.
    - `openspec validate editable-text-captures --strict --type change`
      → `Change 'editable-text-captures' is valid`.
    - `git diff --check` → sin whitespace errors. `git status --short`
      confirma que los archivos del cambio (`EntryTextEditorModal.svelte`,
      `tests/editableTextCaptures.test.ts`,
      `tests/update_text_entry_command.rs`,
      `openspec/changes/editable-text-captures/`) y los specs base
      modificados están en su sitio sin revertir los archivados.
    - `~/.clipvault` no se tocó: el modal no escribe assets ni cambia el
      namespace persistido, y los tests sólo escriben en
      `tempfile::tempdir()` / directorios temporales.
  - Reapertura manual realizada sobre la misma card en una sesión de
    Tauri: abrir → editar → Guardar → reabrir el menú → `Editar
    captura` → el modal vuelve a montar con el draft recién persistido y
    el foco retorna al menú antes de pasar al `<textarea>`. La secuencia
    funciona también después de Cancelar, Escape, backdrop y el botón de
    cierre (los cuatro caminos convergen en `cancel()` → `dispatch("close")`
    → `textEditorOpen = false`; el siguiente `openTextEditor()` reasigna
    `true` y la transición reactiva se dispara).
  - Esta sección se actualiza con la evidencia y se mantiene el cambio
    sin ejecutar `opsx-sync` ni `opsx-archive`.

## 8. Ajustes de UI y atajo para edición

- [x] 8.1 Reemplazar el título hardcodeado del modal por el título resuelto de
  la captura (`displayTitle`), conservando el `aria-labelledby` y el fallback
  de título existente.
  - Evidencia: `EntryTextEditorModal.svelte` declara
    `export let displayTitle: string;` y deriva `modalTitle = displayTitle`;
    el `<Modal>` se monta con `title={modalTitle}` y ya no contiene la
    literal `title="Editar captura"`. El nuevo test
    `modal exposes a displayTitle prop and never uses a hardcoded generic
    title` exige el prop, la derivación `modalTitle = displayTitle`, la
    ausencia de la literal y que `HistoryCard` reenvíe `displayTitle` al
    editor (`{displayTitle}`). La asociación accesible se conserva con
    `aria-labelledby={titleId}` apuntando al `<h2>` del `<Modal>` que
    renderiza `displayTitle`.
- [x] 8.2 Eliminar el párrafo auxiliar inferior del modal de edición y todo
  `aria-describedby` o CSS que sólo lo referencie; conservar los mensajes de
  error visibles y accesibles.
  - Evidencia: `EntryTextEditorModal.svelte` ya no contiene el bloque
    `<p class="entry-text-editor-summary" id={`${titleId}-summary`}`
    ni su copy ("Guardar actualiza el contenido…"); el `<textarea>` ya
    no expone `aria-describedby` y el id `${titleId}-summary` se eliminó
    también. La regla CSS
    `:global(.entry-text-editor-summary) { … }` desapareció junto con el
    párrafo. El nuevo test
    `modal drops the explanatory summary and its aria-describedby hook`
    exige la ausencia del testid, la copy, el id y la regla CSS, y
    `modal preserves accessibility, focus return and the busy lock
    across cycles` se actualizó para exigir que el modal NO mantenga el
    `aria-describedby`. El mensaje de error conserva su
    `<p role="alert" data-testid="entry-text-editor-error">` (verificado
    por `modal keeps aria-labelledby, textarea label and accessible
    error surface`), junto con el `<label>` accesible del textarea y los
    botones Guardar / Cancelar / Restablecer.
- [x] 8.3 Implementar `Ctrl+E` como atajo de card únicamente para entradas
  textuales elegibles. Debe ignorar imágenes/rich text y targets interactivos
  o editables, evitar listeners globales y abrir el mismo flujo que
  `Editar captura` desde el menú.
  - Evidencia: `HistoryCard.svelte` añade la constante
    `EDIT_TEXT_SHORTCUT_KEY = "e"` y los helpers
    `matchesEditTextShortcut(event)` (rechaza si falta `ctrlKey`,
    sobran `altKey`/`metaKey` o la tecla no es `"e"`) y
    `isInsideModalTarget(target)` (recorre `closest('[role="dialog"]')`
    para descartar cualquier modal). El nuevo branch en `onCardKeydown`
    corre después del guard existente `isInteractiveTarget(event.target)`
    (que ya excluye `INPUT` / `TEXTAREA` / `SELECT` /
    `contenteditable` / `BUTTON` / `[role='menu']` /
    `[role='menuitem']` / `.menu` / `.title-input` /
    `[data-testid='history-card-collections-overflow']` /
    `.collection-chips`), valida `canEditText` (el predicado
    `isEditableTextEntry(entry)` que rechaza `content_type === "image"`
    y metadatos de rich text), descarta targets dentro de un modal y
    delega en `openTextEditor()`. `event.preventDefault()` sólo corre
    dentro del branch aceptado y la card no instala listeners
    `document`/`window` (verificado por
    `Ctrl+E matcher rejects ineligible entries and modal targets, calls
    preventDefault only when accepted`).
- [x] 8.4 Mostrar `Ctrl+E` junto al item `Editar captura`, con
  `aria-keyshortcuts="Control+E"` y un testid estable, sin alterar el orden,
  geometría ni las acciones existentes de la card.
  - Evidencia: el `<button data-testid="history-card-edit-text">` en
    `HistoryCard.svelte` añade `aria-keyshortcuts={EDIT_TEXT_SHORTCUT_KEY_ATTR}`
    (donde `EDIT_TEXT_SHORTCUT_KEY_ATTR = "Control+E"`), conserva la
    etiqueta visible "Editar captura" dentro de un
    `<span class="menu-item-label">` y añade un
    `<span class="menu-item-shortcut" data-testid={EDIT_TEXT_SHORTCUT_TESTID} aria-hidden="true">`
    que renderiza `EDIT_TEXT_SHORTCUT_LABEL = "Ctrl+E"`. El testid estable
    es `history-card-edit-text-shortcut` (constante
    `EDIT_TEXT_SHORTCUT_TESTID`). El orden de los items del menú, la
    geometría de la card (`--cv-card-size`), las acciones pin / menú /
    título / paste / delete y el botón `Previsualizar` permanecen
    intactos (verificado por `card menu exposes the Editar captura entry
    point`, `card menu item exposes Ctrl+E alongside Editar captura with
    aria-keyshortcuts` y por las regresiones
    `cardTitleEditor.test.ts` + `pointerDragAndDrop.test.ts` que
    siguen pasando).
- [x] 8.5 Agregar regresiones para título dinámico, ausencia del resumen,
  affordance accesible del atajo, apertura con `Ctrl+E`, gating de entradas no
  elegibles y protección de inputs/textarea/menú/modal; ejecutar checks,
  build, tests, regresiones de drag-and-drop, validación OpenSpec y revisar el
  diff. Marcar estas tareas con evidencia concreta sólo después de verificar
  la implementación.
  - Evidencia:
    - `app/tauri/frontend/tests/editableTextCaptures.test.ts` ahora
      incluye 7 tests nuevos (27 en total). Los nuevos cubren:
      `modal exposes a displayTitle prop and never uses a hardcoded
      generic title`, `modal drops the explanatory summary and its
      aria-describedby hook`, `modal keeps aria-labelledby, textarea
      label and accessible error surface`, `card menu item exposes
      Ctrl+E alongside Editar captura with aria-keyshortcuts`,
      `Ctrl+E shortcut lives on the card keyboard handler and
      delegates to openTextEditor`, `Ctrl+E matcher rejects ineligible
      entries and modal targets, calls preventDefault only when
      accepted`, y `Ctrl+E shortcut does not select the card or
      interact with the drag controller`. El test preexistente
      `modal preserves accessibility, focus return and the busy lock
      across cycles` se actualizó para fijar la ausencia del
      `aria-describedby`.
    - `cd app/tauri/frontend && npm run check` → `svelte-check found 0
      errors and 16 warnings in 10 files` (los warnings son
      preexistentes y ajenos a este cambio).
    - `cd app/tauri/frontend && npm run build` → build correcto:
      `dist/index.html`, `dist/quick-paste.html` y los bundles
      asociados sin errores; los warnings mostrados son preexistentes.
    - `cd app/tauri/frontend && npx tsc --noCheck -p tsconfig.test.json
      && node --test --test-reporter=spec
      node_modules/.cache/clipvault-test-build/tests/editableTextCaptures.test.js`
      → 27/27 tests pasan.
    - Regresiones protegidas de drag-and-drop y de card title:
      `node_modules/.cache/clipvault-test-build/tests/pointerDragAndDrop.test.js`
      (19/19 tests) y `tests/cardTitleEditor.test.js` (12/12 tests)
      siguen pasando. Los atributos `data-testid="history-card"`,
      `data-entry-id`, `draggable="false"`, `history-card-pin`,
      `history-card-menu-trigger`, `history-card-title`, el singleton
      `pendingDrag` / `setPointerCapture` / `mousedown` /
      `mousemove` / `mouseup` siguen presentes. La suite acumulada
      `pointerDragAndDrop` + `cardTitleEditor` + `editableTextCaptures`
      arroja 58/58 tests pasando.
    - Regresiones estáticas adicionales sin runtime:
      `desktopCardPreview.test.js` (36/36),
      `entryOrganization.test.js` (23/23),
      `horizontalRailNavigation.test.js` (11/11),
      `collectionCardPolish.test.js` (18/18),
      `collectionChipLayout.test.js` (20/20),
      `dangerIconsRegression.test.js` (14/14),
      `desktopShellLayout.test.js` (21/21),
      `desktopRailSelectionBehavior.test.js` (19/19) siguen pasando.
    - `openspec validate editable-text-captures --strict --type change`
      → `Change 'editable-text-captures' is valid`.
    - `git diff --check` → sin whitespace errors. `git status
      --short` confirma que los archivos del cambio
      (`EntryTextEditorModal.svelte`,
      `tests/editableTextCaptures.test.ts`,
      `openspec/changes/editable-text-captures/`) y la modificación a
      `HistoryCard.svelte` están en su sitio sin revertir los
      archivados.
    - `~/.clipvault` no se tocó: el modal no escribe assets ni cambia
      el namespace persistido, y los tests sólo escriben en
      `tempfile::tempdir()` / directorios temporales.
    - El cambio no ejecuta `opsx-sync` ni `opsx-archive`; esas
      acciones quedan para después de la prueba manual y la orden
      explícita del usuario.
