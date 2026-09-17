# Prompt para MiniMax: edición persistente de capturas de texto

Eres la LLM implementadora de ClipVault. Implementa únicamente el cambio
OpenSpec `editable-text-captures` en el checkout actual. Codex dejó preparados
`proposal.md`, `design.md`, las delta specs y `tasks.md`; OpenSpec es la fuente
de verdad.

## Contexto obligatorio

Antes de editar:

1. Lee `project.md` y `AGENTS.md`.
2. Lee todos los archivos de
   `openspec/changes/editable-text-captures/` y los specs base de
   `clipboard-text-history` y `clipboard-history-cards`.
3. Revisa `git status --short`, `git diff --check` y el diff existente. Hay
   cambios archivados y modificaciones de specs en el checkout; no los
   reviertas ni los mezcles con una reimplementación.
4. Revisa `ContentType::is_textual`, `EntryRecord`,
   `EntryRepository::set_title`, `TextHistoryService::set_title`, la captura,
   deduplicación, `clipvault://history-updated`, búsqueda, Quick Paste,
   `HistoryCard`, `Modal.svelte` y `pointerDragAndDrop.ts`.

## Decisión aprobada

Usa un `<textarea>` HTML nativo. No agregues CodeMirror, Monaco, Tiptap,
ProseMirror, `contenteditable` ni un wrapper Svelte de editor. El alcance es
texto plano; no implementes edición de imágenes ni de payloads rich text.
Tauri usa WebKitGTK en Linux y WKWebView en macOS, por lo que no debes invocar
APIs específicas de Wayland, X11 o macOS desde el editor.

## Implementación requerida

### Core y SQLite

- Agrega una operación de repositorio y `TextHistoryService::update_text`.
- Edita sólo entradas `ContentType::is_textual()` sin metadata de imagen ni
  referencias rich text; el backend debe validar esto aunque el frontend sea
  stale.
- Rechaza el string vacío, pero conserva whitespace y saltos de línea tal como
  fueron escritos.
- Recalcula con los mismos helpers de captura `content_hash`, `content_size` y
  `content_type`.
- Actualiza sólo `content`, `content_type`, `content_size`, `content_hash` y
  `updated_at`. Conserva ID, `created_at`, `last_seen_at`, título, favoritos,
  tags, colecciones, source-app metadata y assets.
- Un hash ya perteneciente a otra entrada debe producir `duplicate_content`,
  sin merge, delete ni modificación parcial.
- El mismo texto debe producir `noop` sin nueva fila ni mutación innecesaria.
- Missing, empty, non-editable, duplicate y persistence failure deben ser
  resultados/errores tipados y no deben incluir texto, hash, snippet, path ni
  bytes en logs o mensajes.
- No agregues una migración si las columnas actuales alcanzan.

### Tauri y frontend

- Agrega un comando thin, por ejemplo `clipvault_update_text_entry`, y su
  bridge TypeScript tipado. El texto entra únicamente porque el usuario lo
  está editando; no es un nuevo flujo de captura del portapapeles.
- Después de un commit exitoso, emite el evento existente
  `clipvault://history-updated` con payload vacío o usa su mecanismo actual
  equivalente. Nunca pongas el texto en ese evento.
- Crea `EntryTextEditorModal.svelte` o equivalente con el shell Modal común:
  `<textarea>` nativo, foco inicial, label accesible, Guardar, Cancelar,
  Escape, backdrop, cierre explícito, error visible y retorno de foco.
- Usa draft local. Cancelar/Escape/backdrop no persisten. Deshabilita Guardar
  durante la petición y evita doble submit. Si falla el backend, conserva el
  draft para corregirlo.
- Agrega `Editar captura` al menú sólo para entradas elegibles. No lo muestres
  para imágenes o rich text.
- Actualiza la card y las fuentes de historial/search/Quick Paste sólo después
  del resultado exitoso. No reordenes por `updated_at`.

## Baselines protegidos

Conserva `data-testid="history-card"`, `data-entry-id`, `draggable="false"`,
pin, menú, título, tags, colecciones y el singleton de
`app/tauri/frontend/src/lib/pointerDragAndDrop.ts`. Los controles del modal no
pueden seleccionar la card ni iniciar drag. El payload de drag sigue llevando
únicamente el ID opaco de la entrada.

No toques ni limpies `~/.clipvault` o `~/.clipvault/assets`. No agregues red,
telemetría, cuentas, cloud, LLMs ni almacenamiento remoto.

## Tests obligatorios

Agrega tests que cubran:

- actualización persistente, hash, tamaño, tipo, `updated_at` y mismo ID;
- noop, vacío, not-found, no-editable, duplicate_content y rollback;
- conservación de título, favoritos, tags, colecciones, source-app metadata y
  assets;
- comando/bridge, respuesta discriminada y evento vacío;
- modal, foco, Guardar/Cancelar, Escape/backdrop, errores y doble submit;
- refresh de card, búsqueda, Quick Paste y persistencia tras reinicio;
- ausencia de edición para imágenes/rich text;
- regresiones pointer/mouse drag, selección, capture, payload opaco y atributos
  protegidos.

## Verificación y prueba manual

Ejecuta y registra evidencia en `tasks.md`:

```bash
cargo fmt --all -- --check
cargo test -p clipvault-db -p clipvault-core
cargo clippy -p clipvault-db -p clipvault-core --all-targets -- -D warnings
cd app/tauri/frontend && npm run check && npm run build && npm test
cd ../../.. && openspec validate editable-text-captures --strict --type change
git diff --check
```

Ejecuta también las regresiones frontend de drag-and-drop indicadas por
`AGENTS.md`. Luego prueba manualmente en Wayland, X11 y macOS: texto corto,
multilínea y Unicode; guardar, cancelar y Escape; reinicio; búsqueda; Quick
Paste; intento sobre imagen/rich text y conflicto duplicado.

Marca cada tarea como `[x]` sólo después de verificarla y agrega evidencia
concreta. No ejecutes `opsx-sync` ni `opsx-archive`.

## Corrección adicional solicitada por el usuario

La prueba manual de la implementación existente fue aprobada. Implementa
únicamente estos ajustes incrementales de frontend dentro del cambio
OpenSpec `editable-text-captures`; no rehagas el backend ni mezcles refactors:

1. En `EntryTextEditorModal.svelte`, reemplaza el título hardcodeado
   `Editar captura` por el título resuelto de la captura que ya usa
   `HistoryCard` (`displayTitle`: título personalizado o el fallback existente
   para el tipo de contenido). El heading del diálogo debe mostrar ese valor y
   conservar la relación accesible mediante `aria-labelledby`. No uses el
   contenido completo como título ni cambies el título inline de la card.

2. Elimina por completo del modal el texto auxiliar inferior:

   `Guardar actualiza el contenido de la captura. La edición conserva el identificador, los tags, las colecciones, los favoritos, el título y el origen; los espacios y los saltos de línea se mantienen tal como los escribiste.`

   Elimina también `aria-describedby` si sólo referencia ese párrafo y el CSS,
   ids o helpers que queden sin uso por su eliminación. Conserva el label
   accesible del `textarea`, los botones, el foco, los errores visibles y los
   `role="alert"` existentes.

3. Agrega el atajo `Ctrl+E` para editar:

   - En `HistoryCard.svelte`, `Ctrl+E` debe abrir el mismo modal y ejecutar el
     mismo camino que `Editar captura` del menú.
   - Sólo debe funcionar cuando `canEditText` / `isEditableTextEntry(entry)`
     sea verdadero.
   - Debe ignorar el evento si el target es input, textarea, select, botón,
     elemento editable (`contenteditable`), item/elemento del menú, el modal o
     cualquier otro control interactivo protegido por el guard existente.
   - No agregues un listener global al documento o a la ventana. Respeta el
     handler de teclado de la card y no alteres el singleton, pointer capture,
     fallback mouse, cancelación ni payload opaco de drag-and-drop.
   - Evita la acción predeterminada sólo cuando el atajo sea aceptado y abre el
     diálogo sin seleccionar la card ni iniciar drag.
   - En el item `Editar captura` del menú muestra literalmente `Ctrl+E` como
     ayuda visible, siguiendo el patrón visual/accesible del atajo de
     previsualización existente. Agrega `aria-keyshortcuts="Control+E"` y un
     testid estable, por ejemplo `history-card-edit-text-shortcut`, sin cambiar
     el orden ni la geometría de las acciones.

## Tests y verificación de esta corrección

Agrega o actualiza tests frontend source-level cercanos a
`editableTextCaptures.test.ts` para demostrar:

- el modal recibe/renderiza `displayTitle` y ya no contiene el título genérico
  ni el párrafo `entry-text-editor-summary`;
- no queda `aria-describedby` apuntando al resumen eliminado, pero se
  mantienen `aria-labelledby`, label del editor y errores accesibles;
- el menú muestra `Ctrl+E` y `aria-keyshortcuts="Control+E"`;
- `Ctrl+E` abre sólo para una entrada textual elegible;
- imágenes/rich text y targets interactivos/editables no disparan la edición;
- se conserva la ruta única de apertura, el foco y todos los baselines de
  card y drag-and-drop.

Ejecuta y registra la evidencia en las tareas 8.1–8.5:

```bash
cd app/tauri/frontend
npm run check
npm run build
npm test
cd ../../..
openspec validate editable-text-captures --strict --type change
git diff --check
```

Ejecuta también las regresiones frontend protegidas indicadas por `AGENTS.md`.
No modifiques `~/.clipvault` ni `~/.clipvault/assets`, no agregues
dependencias, red, telemetría o migraciones y no ejecutes `opsx-sync` ni
`opsx-archive`.

## Corrección bloqueante reportada por la prueba manual

La edición funciona una vez, pero el usuario no puede volver a abrir el modal
para la misma captura durante la misma sesión. Reproduce este flujo antes de
editar:

1. abrir `Editar captura`;
2. cambiar el texto y guardar;
3. elegir nuevamente `Editar captura` en la misma card.

Repite también después de Cancelar, Escape, backdrop y botón de cierre.

La causa probable ya identificada es que `EntryTextEditorModal.svelte` asigna
`open = false` dentro del hijo, mientras `HistoryCard.svelte` mantiene
`textEditorOpen = true`. El padre escucha `on:close`, pero el hijo no despacha
consistentemente ese evento. El siguiente `textEditorOpen = true` no cambia el
valor y el modal no vuelve a montarse/abrirse.

Corrige el ciclo de vida con estas reglas:

- `HistoryCard` es la única fuente de verdad de `textEditorOpen`;
- el modal debe despachar `close` —o usar un binding explícito— en Guardar
  exitoso, Cancelar, Escape, backdrop y botón de cierre;
- elimina la dependencia de asignar sólo la prop local `open = false`;
- la secuencia `true -> false -> true` debe funcionar para la misma entrada sin
  desmontar toda la card;
- al reabrir, el draft debe inicializarse desde el contenido persistido actual,
  no desde el draft anterior;
- el foco debe volver al disparador y regresar al textarea en la reapertura;
- durante un guardado no se debe permitir doble submit ni cierre accidental.

Agrega una regresión cercana al componente que abra y cierre dos veces el modal
para la misma entrada, cubriendo guardar, Cancelar, Escape y backdrop. Verifica
contenido actualizado, draft limpio, foco, accesibilidad y no regresión de
drag-and-drop.

Ejecuta nuevamente:

```bash
cd app/tauri/frontend
npm run check
npm run build
npm test
cd ../../..
openspec validate editable-text-captures --strict --type change
git diff --check
```

Marca las tareas 7.1–7.4 como completas sólo con evidencia concreta de la
reproducción, la corrección y la reapertura manual del modal. No ejecutes
`opsx-sync` ni `opsx-archive`.
