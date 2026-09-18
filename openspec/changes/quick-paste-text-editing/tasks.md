# Tasks: quick-paste-text-editing

## 1. Baseline y alcance

- [x] 1.1 Leer `project.md`, `AGENTS.md`, el cambio archivado
  `editable-text-captures`, `editable-text-shortcut-fix` y los contratos
  activos/archivados de Quick Paste.
- [x] 1.2 Confirmar que el cambio se limita al menú `...` y la ventana Quick
  Paste, no al menú nativo del tray.
- [x] 1.3 Revisar `git status --short`, `git diff --check` y los baselines
  protegidos antes de modificar cards, listeners o layout.

## 2. Matriz pura del menú

- [x] 2.1 Extender `QuickPasteMenuAction` y `quickPasteMenuActions` con un
  `kind` de edición separado de `CopyMode`.
- [x] 2.2 Mostrar `Editar captura` sólo cuando
  `isEditableTextEntry(entry)` sea verdadero, sin alterar las matrices de
  copia de texto rich, plain e imagen.
- [x] 2.3 Mantener `Previsualizar` en todas las entradas y conservar su acción
  read-only.
- [x] 2.4 Agregar labels, tooltips, testids y casos unitarios para las tres
  clases de entrada y para ambos sistemas de modificadores.

## 3. Integración del editor en Quick Paste

- [x] 3.1 Reutilizar `EntryTextEditorModal.svelte` y
  `updateTextEntryCommand`; no crear un modal ni un comando paralelo.
- [x] 3.2 Agregar estado de apertura y entry ID, resolver siempre la entrada
  actual por ID y cerrar el menú antes de abrir el modal.
- [x] 3.3 Pasar el título resuelto y un destino estable de retorno de foco;
  verificar ciclo abrir, guardar/cancelar, cerrar y reabrir sin estado stale.
- [x] 3.4 Mantener Quick Paste visible, conservar query/selección y refrescar
  recent/search usando el evento metadata-only o el refresh existente.
- [x] 3.5 Cubrir errores, doble submit, Escape, backdrop, duplicados,
  entrada ausente y entrada no editable sin mutar la fila previa.

## 4. Atajos y presentación

- [x] 4.1 Reutilizar `editTextShortcut*` para ejecutar y rotular `Cmd/Ctrl+E`.
- [x] 4.2 Reutilizar `previewShortcut*` para rotular `Cmd/Ctrl+Enter` en el
  item `Previsualizar`, sin alterar el shortcut existente.
- [x] 4.3 Integrar ambos cambios en el listener actual de Quick Paste, con
  gating de input, textarea, select, contenteditable, menú y dialog.
- [x] 4.4 Renderizar label y hint separados con `aria-keyshortcuts`, sin
  cambiar geometría fija ni agregar overflow horizontal.

## 7. Correcciones derivadas de la prueba manual

- [x] 7.1 Permitir que `Cmd/Ctrl+E` abra el editor desde la primera
  activación, sin depender de un ciclo previo de edición. La resolución del
  target usa exclusivamente `selectedEntryId` (cuando sigue en alcance) o
  `resultIds[selectedIndex]` como fallback; nunca el último entry editado,
  `quickPasteEditorEntryId` ni un closure stale. Sincronizar
  `selectedIndex` / `selectedEntryId` con la fila clickeada y la navegación
  con cursores desde la primera carga.
- [x] 7.2 Permitir `Cmd/Ctrl+E` desde el input de búsqueda de Quick Paste
  (porque la ventana coloca el foco inicial ahí) y mantener el bloqueo
  dentro del textarea del editor, otros inputs editables, `select`,
  `contenteditable`, popover `[role="menu"]` y cualquier `[role="dialog"]`.
  El matcher y el handler son los mismos que el menu item `Editar captura`.
  `preventDefault` sólo se ejecuta cuando existe una captura seleccionada
  y se resolvió una acción válida (editor o diálogo informativo).
- [x] 7.3 Garantizar que Escape dentro del editor sólo cierre el modal y
  mantenga Quick Paste visible. El handler global detecta que la pulsación
  se origina dentro de `[role="dialog"]` y no la deriva a `handleEscape`;
  adicionalmente, `closeQuickPasteEditor` arma el flag
  `suppressNextWindowEscape` como defensa. El foco vuelve al `...` trigger
  (`menuAnchorEls[id]`) cuando el editor se abrió desde el menú, o a la
  fila (`rowRefs.get(id)`) cuando se abrió por shortcut. Mismo contrato
  tras guardar, cancelar, cerrar con backdrop y cerrar con el botón `×`.
- [x] 7.4 Mostrar un diálogo informativo `Captura no editable` cuando el
  shortcut resuelve a una imagen o entrada rich text. El diálogo se monta
  sobre `Modal.svelte` (reutilizando el shell accesible), lleva título
  accesible, mensaje visible y un botón `Aceptar`. Escape, backdrop y
  botón cierran sólo el diálogo y devuelven foco a la fila seleccionada.
  Nunca invoca `updateTextEntryCommand`, nunca muta la entrada y nunca
  crea una nueva fila. El item `Editar captura` del menú sigue oculto
  para image / rich text porque la matriz pura consulta
  `isEditableTextEntry`.

## 5. Tests y no regresiones

- [x] 5.1 Agregar tests de la matriz de acciones, labels visibles y atributos
  accesibles en macOS, Wayland/X11 y fallback.
- [x] 5.2 Agregar tests de routing por entry ID, shortcut, gating de targets,
  modal, focus return y ciclo de reapertura.
- [x] 5.3 Agregar tests de refresh, persistencia por bridge y no exposición de
  contenido en eventos, logs o payloads.
- [x] 5.4 Ejecutar regresiones de Quick Paste para copia, preview, búsqueda,
  favoritos, tags, colecciones, imágenes, foco y listeners idempotentes.
- [x] 5.5 Ejecutar regresiones de cards y drag and drop cuando el cambio toque
  listeners o componentes compartidos; confirmar atributos y payload opaco.

## 6. Verificación y entrega

- [x] 6.1 Ejecutar `npm run check`, `npm run build` y los tests frontend
  relevantes.
- [x] 6.2 Ejecutar `cargo fmt --all -- --check` y los tests Rust relevantes;
  no agregar migraciones si no son necesarias.
- [x] 6.3 Ejecutar `openspec validate quick-paste-text-editing --strict
  --type change` y `git diff --check`.
- [ ] 6.4 Realizar la prueba manual completa en Ubuntu GNOME Wayland, Ubuntu
  X11 y macOS: edición, persistencia, labels de atajos, shortcut, preview,
  cancelación y exclusión de imagen/rich text.
- [x] 6.4-Linux Registrar la aprobación manual reportada por el usuario en el
  entorno Linux probado: edición desde el menú de Quick Paste, persistencia,
  atajos visibles y funcionales, preview, cancelación y exclusión de entradas
  no editables. La validación de macOS permanece pendiente y no se infiere.
- [x] 6.5 Registrar evidencia en esta tarea. MiniMax no debe sincronizar ni
  archivar automáticamente. La evidencia manual disponible corresponde a
  Linux; el cambio no se considera completamente terminado hasta validar
  macOS.

### Evidencia de verificación automatizada (6.1–6.3)

- `cd app/tauri/frontend && npx svelte-check --tsconfig ./tsconfig.json`
  → **0 errores**, 16 warnings preexistentes sobre accesibilidad
  (`aria-selected`, `role='article'`, list items con handlers, etc.)
  que ya estaban antes del cambio.
- `cd app/tauri/frontend && npm run build` → **compila correctamente**;
  Vite emite `dist/index.html`, `dist/quick-paste.html`,
  `dist/assets/ClipboardPreview-*.css/js`, `dist/assets/quick-paste-*.js`
  y `dist/assets/main-*.js` sin errores.
- `cargo fmt --all -- --check` → **sin diferencias**.
- `openspec validate quick-paste-text-editing --strict --type change`
  → **"Change 'quick-paste-text-editing' is valid"** (issues: 0).
- `git diff --check` → **clean** (sin whitespace errors).
- `git status --short` muestra sólo los archivos del cambio
  (`app/tauri/frontend/src/QuickPaste.svelte`,
  `app/tauri/frontend/src/lib/clipboardPreview.ts`,
  `app/tauri/frontend/src/lib/quickPasteActions.ts`,
  `app/tauri/frontend/tests/clipboardPreview.test.ts`,
  `app/tauri/frontend/tests/quickPasteActions.test.ts`,
  `app/tauri/frontend/tests/quickPasteTextEditing.test.ts` y
  `openspec/changes/quick-paste-text-editing/`).

### Tests dirigidos

- `node --test
  node_modules/.cache/clipvault-test-build/tests/quickPasteTextEditing.test.js`
  → **31/31 passing** (matriz de acciones, labels accesibles,
  matcher `Cmd/Ctrl+E` con gating, integración `EntryTextEditorModal`,
  suscripción `clipvault://history-updated`, hint visible +
  `aria-keyshortcuts`, baselines `pointerDragAndDrop.ts`, contract
  del modal, y los nuevos escenarios de la sección 7: shortcut
  desde la primera activación, dialog informativo, gating del input
  de búsqueda, retorno de foco y Escape sin ocultar Quick Paste).
- `node --test
  node_modules/.cache/clipvault-test-build/tests/pointerDragAndDrop.test.js`
  → **19/19 passing** (baselines de drag-and-drop preservadas).
- `node --test
  node_modules/.cache/clipvault-test-build/tests/cardTitleEditor.test.js`
  → **12/12 passing**.
- `node --test
  node_modules/.cache/clipvault-test-build/tests/editTextShortcut.test.js`
  → **11/11 passing**.
- `node --test
  node_modules/.cache/clipvault-test-build/tests/editTextShortcutFix.test.js`
  → **22/22 passing**.
- `node --test
  node_modules/.cache/clipvault-test-build/tests/editableTextCaptures.test.js`
  → **27/27 passing**.
- `node --test
  node_modules/.cache/clipvault-test-build/tests/historyUpdates.test.js`
  → **9/9 passing**.
- `node --test
  node_modules/.cache/clipvault-test-build/tests/clipboardPreviewComponent.test.js`
  → **16/16 passing**.
- `node --test
  node_modules/.cache/clipvault-test-build/tests/quickPasteController.test.js
  node_modules/.cache/clipvault-test-build/tests/quickPastePreviewShortcuts.test.js
  node_modules/.cache/clipvault-test-build/tests/quickPasteSelectionAndScroll.test.js`
  → **50/50 passing** agregados.
- `quickPasteActions.test.ts` y `quickPasteBridge.test.ts` siguen
  rotas por la infraestructura preexistente (`src/lib/tauri.ts`
  referencia `from "../types"` sin `.ts`, lo que rompe
  `rewriteRelativeImportExtensions`). Esta condición ya estaba antes
  del cambio y no es introducida por esta entrega.

### Privacidad

- Ningún evento, log, payload de drag o diagnóstico lleva contenido
  completo, hashes, snippets, referencias de assets o rutas. El
  payload de `clipvault://history-updated` sigue siendo `()`.
- Los assets persistidos en `~/.clipvault/assets` no se tocan, no se
  renombran y no se eliminan durante los tests ni durante la
  compilación.
- El menú nativo del tray/menu bar queda intacto: el cambio sólo
  toca el menú `...` de cada resultado de la ventana Quick Paste.
- El diálogo informativo `Captura no editable` nunca invoca
  `updateTextEntryCommand`, nunca muta la entrada y nunca crea
  una nueva fila. Sólo expone título, cuerpo y botón de cierre
  sobre `Modal.svelte`.
