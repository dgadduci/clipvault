# Design: quick-paste-text-editing

## Fronteras de arquitectura

```text
QuickPaste.svelte
  ├── quickPasteActions.ts       → matriz pura de acciones
  ├── editTextShortcut.ts        → matcher y label de Cmd/Ctrl+E
  ├── clipboardPreview.ts        → matcher y label de Cmd/Ctrl+Enter
  ├── EntryTextEditorModal.svelte
  └── updateTextEntryCommand     → comando Tauri ya existente
                                      ↓
                              TextHistoryService
                                      ↓
                                  SQLite
```

Quick Paste sigue siendo un adaptador de UI. La elegibilidad, la validación,
el hash, la clasificación, la detección de duplicados y la transacción siguen
siendo responsabilidad del core existente. No se debe copiar lógica Rust ni
acceder a SQLite desde Svelte.

## Matriz del menú

El helper puro `quickPasteMenuActions` debe conservar la semántica actual de
copia y agregar las acciones en este orden:

| Entrada | Acciones de copia | Edición | Acción común |
| --- | --- | --- | --- |
| Texto plano elegible | `Copiar` | `Editar captura` | `Previsualizar` |
| Texto con rich text | acciones rich/plain existentes | ninguna | `Previsualizar` |
| Imagen | `Copiar` | ninguna | `Previsualizar` |

La acción de edición debe tener un `kind` discriminado propio y no reutilizar
`CopyMode`. Debe incluir label, testid estable, label accesible, tooltip y los
metadatos del atajo que necesite el render. No debe aparecer para una entrada
que `isEditableTextEntry` rechaza, aunque el resultado sea stale o el menú se
construya con datos incompletos.

El item de previsualización conserva su acción actual y sólo amplía su
presentación con la indicación del atajo. No se debe crear una segunda acción
ni alterar la preview read-only.

## Labels y accesibilidad de atajos

Reutilizar los helpers existentes:

- edición: `editTextShortcutLabel`,
  `editTextShortcutAccessibleLabel` y `editTextShortcutKeyAttribute`;
- preview: `previewShortcutLabel`, `previewShortcutAccessibleLabel` y el
  equivalente actual de `Meta+Enter`/`Control+Enter`.

La representación visible debe ser `⌘E` / `Ctrl+E` para edición y `⌘Enter` /
`Ctrl Enter` para preview, según la plataforma diagnosticada. El texto del
atajo debe vivir en un elemento visual separado, por ejemplo
`.qp-menu-item-shortcut`, con `aria-hidden="true"` si el label accesible del
button ya lo incluye. Cada button debe exponer el `aria-keyshortcuts`
correspondiente y conservar su `aria-label` descriptivo.

El menú no debe mostrar un atajo para las acciones de copia ni inventar una
combinación distinta para Linux X11, Linux Wayland o macOS. X11 y Wayland
usan el mismo contrato del WebView Linux; la diferencia de modificador sólo
es macOS frente a los demás hosts.

## Estado y ciclo de edición

`QuickPaste.svelte` mantiene una única fuente de verdad para la edición:

- `quickPasteEditorOpen: boolean`;
- `quickPasteEditorEntryId: number | null`;
- una referencia al trigger del menú o a un elemento estable de la fila para
  devolver el foco.

Al activar la acción:

1. Resolver la entrada actual por ID desde `recent` o `hits`.
2. Comprobar `isEditableTextEntry(entry)` antes de abrir.
3. Cerrar el popover del menú para que no quede detrás del modal.
4. Conservar la selección por `selectedEntryId` y abrir el
   `EntryTextEditorModal` existente con `displayTitle` y `returnFocusTo`.
5. Dejar que el modal gestione draft, foco, Escape, Cancelar, errores,
   doble-submit y `updateTextEntryCommand` mediante su contrato actual.

El modal debe cerrarse por el evento del padre, nunca mutando sólo una prop
local. Reabrir la misma entrada debe sembrar el draft desde el contenido
persistido actual. Cambiar el resultado seleccionado durante un ciclo no debe
hacer que se edite una entrada anterior.

Después de `updated` o `noop`, el modal se cierra y Quick Paste vuelve a cargar
la fuente activa usando el evento `clipvault://history-updated` o el mecanismo
de refresh existente. El evento no puede llevar texto, hash, snippets, asset
refs ni paths. El query y la selección se conservan por el ID que sigue
existiendo; si la entrada desapareció, se aplica el clamp normal. La ventana
Quick Paste permanece visible: editar desde el menú no ejecuta copia, paste,
hide/show ni crea una nueva captura.

Los errores `not_found`, `not_editable`, `empty_content` y
`duplicate_content` se muestran dentro del modal con el contrato existente.
El draft queda visible para corregirlo y la fila persistida no cambia.

## Shortcut de edición

La ventana Quick Paste debe reutilizar `matchesEditTextShortcut` con el mismo
`shortcutPlatform` que ya alimenta búsqueda y preview. Cuando la ventana está
activa, `Cmd+E` en macOS o `Ctrl+E` en Linux debe:

- operar sobre la entrada **actualmente seleccionada** usando el ID vigente:
  `selectedEntryId` cuando sigue en el alcance (pertenece a `resultIds`) o
  `resultIds[selectedIndex]` como fallback. Nunca debe usar un ID stale del
  último ciclo de edición, ni reabrir una entrada anterior porque el modal
  haya quedado montado con un snapshot obsoleto;
- funcionar desde la primera activación, sin requerir un ciclo previo de
  edición;
- permitir el shortcut cuando el foco está en el **input de búsqueda**
  (única excepción de input permitida, porque la ventana coloca el foco
  ahí al abrir) y bloquear cualquier otro `HTMLInputElement`,
  `HTMLTextAreaElement`, `HTMLSelectElement` y `contenteditable` para no
  robar caracteres tipiables;
- bloquear el shortcut dentro del popover `[role="menu"]` y de cualquier
  `[role="dialog"]` (incluyendo el modal de edición y el diálogo
  informativo) para que la pulsación se dirija a la superficie que tiene
  foco;
- abrir el mismo modal y pasar por el mismo handler que el item `Editar
  captura` del menú;
- mostrar el diálogo informativo `Captura no editable` (no abrir el editor)
  cuando la captura seleccionada es una imagen o una entrada rich text.
  El diálogo nunca invoca `updateTextEntryCommand`, nunca modifica la
  entrada y nunca crea una nueva fila;
- prevenir el comportamiento por defecto sólo cuando el handler va a
  abrir el editor o el diálogo informativo. Sin selección válida la
  pulsación se ignora sin `preventDefault`.

Debe existir un único listener de teclado en Quick Paste, integrado en el
handler actual de la ventana. No se agrega un listener global permanente ni
se modifica el listener del desktop principal. El atajo de preview
`Cmd/Ctrl+Enter` sigue intacto y su item de menú sólo recibe la pista visual y
accesible correspondiente.

## Diálogo informativo para entradas no editables

Quick Paste muestra un diálogo `Captura no editable` cuando el usuario
dispara `Cmd/Ctrl+E` sobre una entrada no elegible (imagen o rich text).
El diálogo:

- reutiliza `Modal.svelte` para mantener el focus trap, el manejo de
  Escape / backdrop y la accesibilidad consistentes con el resto de la
  ventana;
- expone un título accesible (`Captura no editable`) y un cuerpo
  descriptivo (`Esta captura no se puede editar porque no es una captura
  de texto editable.`);
- tiene un único botón visible `Aceptar`; el `×` del header también
  cierra el diálogo;
- cierra por Escape, backdrop o botón y mantiene Quick Paste visible;
- devuelve el foco a la fila cuya `data-entry-id` está almacenada al
  activar el diálogo (no a la última entrada editada);
- nunca invoca `updateTextEntryCommand`, nunca muta la entrada, nunca
  crea una nueva fila.

El item `Editar captura` del menú `...` sigue oculto para imágenes y rich
text porque la matriz pura `quickPasteMenuActions` consulta
`isEditableTextEntry` antes de añadirlo. El diálogo es la única ruta
teclado-visible hacia el aviso y nunca se muestra desde el menú.

## Escape en el editor y el diálogo

El modal de edición persistente y el diálogo informativo comparten la
misma superficie `Modal.svelte` con `role="dialog"`. El shell cierra
la superficie al recibir Escape y el handler global de Quick Paste
detecta que la pulsación proviene de un `[role="dialog"]` para no
derivar la misma tecla a `handleEscape` ni a `hideQuickPasteWindow`.
Como defensa adicional, los helpers `closeQuickPasteEditor` y
`closeQuickPasteNotEditableDialog` arman el flag
`suppressNextWindowEscape` que ya consume un único Escape adicional.

El comportamiento se aplica a:

- Cancelar (botón Cancelar del editor);
- Escape dentro del editor o del diálogo;
- backdrop del editor o del diálogo;
- botón de cierre `×` del header `Modal.svelte`;
- guardado exitoso (el dispatcher `close` también cierra el editor).

Después de cualquiera de estos caminos, Quick Paste permanece visible,
el foco vuelve al `menuAnchorEls[id]` cuando el editor se abrió desde
el menú, o a la fila correspondiente (`rowRefs.get(id)`) cuando se
abrió por shortcut o por el botón del menú sin anchor activo.

## Compatibilidad y privacidad

El editor continúa siendo un `<textarea>` nativo dentro del shell Modal. Esto
mantiene IME, selección, Unicode, saltos de línea y copy/paste sin depender de
Wayland, X11 o APIs nativas. En Linux se ejecuta en WebKitGTK tanto sobre
Wayland como sobre X11; en macOS, en WKWebView.

No se agregan dependencias. Los comandos Tauri siguen siendo adaptadores
delgados. No se registran drafts ni contenido completo y no se agregan bytes o
contenido a eventos, payloads de drag o diagnósticos. Los assets de imágenes
existentes no se tocan.

## Verificación

Automática:

- matriz pura de acciones para texto elegible, rich text e imagen;
- labels visibles, `aria-keyshortcuts` y matchers para macOS, Wayland/X11 y
  fallback de plataforma;
- apertura desde menú y shortcut, gating de entradas no editables, target ID,
  ciclo cerrar/abrir y retorno de foco;
- persistencia mediante el bridge existente, refresh metadata-only y
  preservación de query/selección;
- errores, cancelación, Escape, doble submit y no mutación ante fallo;
- regresiones de Quick Paste: copia, preview, búsqueda, favoritos, tags,
  colecciones, filas fijas, imágenes, foco de ventana y listeners idempotentes;
- `npm run check`, `npm run build`, tests frontend dirigidos y validación
  OpenSpec.

Manual en Ubuntu GNOME Wayland, Ubuntu X11 y macOS:

- abrir Quick Paste, seleccionar una captura de texto y editarla desde el
  menú;
- confirmar que se ven `⌘E`/`Ctrl+E` en edición y `⌘Enter`/`Ctrl Enter` en
  preview;
- usar el atajo de edición sobre la entrada seleccionada, guardar y comprobar
  que la ventana queda usable y la selección no salta a otra entrada;
- cancelar, cerrar con Escape, reabrir y comprobar que el texto persistido es
  el correcto;
- reiniciar y verificar búsqueda, preview y copia del contenido editado;
- confirmar que imágenes y rich text no muestran `Editar captura`;
- confirmar que no se crean, renombran ni eliminan assets en
  `~/.clipvault/assets`.
