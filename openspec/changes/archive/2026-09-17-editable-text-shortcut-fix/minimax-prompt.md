# Prompt para MiniMax: corregir Ctrl+E / Cmd+E de edición

Eres la LLM implementadora de ClipVault. Implementa únicamente el cambio
OpenSpec `editable-text-shortcut-fix` en el checkout actual. Codex dejó
preparados `proposal.md`, `design.md`, la delta spec y `tasks.md`; OpenSpec es
la fuente de verdad.

## Contexto obligatorio

Antes de editar:

1. Lee `project.md` y `AGENTS.md`.
2. Lee todos los archivos de
   `openspec/changes/editable-text-shortcut-fix/` y los specs base referidos.
3. Revisa `git status --short`, `git diff --check` y el diff existente. Hay
   un archivado OpenSpec previo sin mezclar; no lo reviertas ni lo incluyas en
   una reimplementación amplia.
4. Inspecciona `HistoryCard.svelte`, `HistoryCardRail.svelte`, `App.svelte`,
   `EntryTextEditorModal.svelte`, `lib/searchShortcut.ts`,
   `lib/clipboardPreview.ts`, `types.ts` y los tests de atajos y drag.

## Corrección requerida

La implementación actual anuncia `Ctrl+E`, pero el atajo falla en Linux y
macOS. El matcher vive sólo en el `keydown` del `article` de la card y sólo
comprueba `ctrlKey`. Corrige el flujo con estas reglas:

- Linux, tanto Wayland como X11: aceptar `Ctrl+E`.
- macOS: aceptar `Cmd+E` mediante `metaKey`.
- Rechazar `Alt`, `Shift` y el modificador contrario.
- Comparar `event.key` sin depender de mayúsculas/minúsculas.
- Usar la detección de plataforma ya existente; no agregues APIs nativas ni
  dependencias nuevas.
- Conectar el atajo a un único listener compartido de App/rail, o extender el
  listener compartido existente, para que funcione aunque la card esté
  seleccionada pero su `article` no tenga el foco directo.
- Resolver la card por el elemento focalizado o por `selectedEntryId`, validar
  que el entry esté visible y que `isEditableTextEntry(entry)` sea verdadero.
- Abrir exactamente el mismo `EntryTextEditorModal` que abre el menú, una sola
  vez por keydown, preservando ownership, foco y reapertura.
- Ignorar inputs, textarea, select, contenteditable, botones, menús,
  menuitems, editor de título, selectores, chips, diálogos y demás controles
  interactivos. `preventDefault` sólo debe ejecutarse si el atajo fue aceptado.
- En el menú, mostrar `Ctrl+E` / `Control+E` en Linux y `⌘E` / `Meta+E` en
  macOS, derivados del mismo helper que hace el matching.

No agregues listeners globales por card. No cambies backend, SQLite,
`EntryTextEditorModal` salvo el cableado necesario, búsqueda, Quick Paste,
`pointerDragAndDrop.ts`, payloads, geometría ni atributos protegidos de cards.

## Tests obligatorios

Agrega o actualiza tests para:

- `Ctrl+E` en Linux y `Cmd+E` en macOS;
- rechazo de la combinación incorrecta, `Alt` y `Shift`;
- card focalizada y card seleccionada sin foco directo;
- ausencia de selección y entradas de imagen/rich text;
- bloqueo en inputs, textarea, contenteditable, menú, modal y controles;
- etiquetas visibles y `aria-keyshortcuts` acordes a la plataforma;
- listener único, cleanup, apertura única, foco y no regresión de drag-and-drop.

## Verificación

Ejecuta y registra evidencia en `tasks.md`:

```bash
cd app/tauri/frontend
npm run check
npm run build
npm test
cd ../../..
openspec validate editable-text-shortcut-fix --strict --type change
git diff --check
```

Ejecuta también las regresiones frontend protegidas por `AGENTS.md`. Prueba
manualmente en Wayland, X11 y macOS. No modifiques `~/.clipvault` ni
`~/.clipvault/assets`, no agregues red, telemetría o dependencias y no
ejecutes `opsx-sync` ni `opsx-archive`.

## Corrección bloqueante reportada por la prueba manual

La prueba manual confirmó que el shortcut ahora abre el modal, pero detectó
un bug de identidad: después de abrir la edición para la captura A, al
seleccionar otra captura B y volver a usar `Ctrl+E` / `Cmd+E`, el modal sigue
editando A. Implementa únicamente esta corrección incremental; no rehagas el
matcher multiplataforma ni el backend.

### Reproducción obligatoria

1. Selecciona una captura textual elegible A.
2. Abre el editor con el shortcut y confirma que muestra A.
3. Cierra el modal sin perder el foco de forma incorrecta.
4. Selecciona otra captura textual elegible B.
5. Abre el editor nuevamente con el shortcut.
6. Confirma que el modal, su título, `data-entry-id`, draft y baseline
   corresponden a B, no a A.
7. Guarda un cambio en B y verifica que A no se modifica.

Repite también con card enfocada, card seleccionada sin foco directo, cierre y
reapertura, y una solicitud mientras el primer modal todavía está abierto.

### Reglas de implementación

- Cada solicitud debe transportar únicamente `{ entryId }`.
- `HistoryCardRail` debe usar un registro actualizado por ID y despachar el
  evento sólo al `article` cuyo `data-entry-id` coincide.
- `HistoryCard` debe leer el `entryId` del evento, validar que coincide con
  `entry.id` y descartar eventos ausentes, obsoletos, duplicados o dirigidos a
  otra card.
- El modal debe recibir la entrada y `displayTitle` de la card objetivo; no
  debe reutilizar un snapshot, closure o variable global de la primera
  captura.
- Al cambiar el target, reinicializa `draft`, `baselineEntry`, ids accesibles,
  `returnFocusTo` y foco para la nueva entrada. Nunca envíes al backend el
  texto de la captura anterior.
- Si A está abierto y llega una solicitud para B, garantiza que como máximo
  quede un modal activo y que el modal visible edite B. Elige un cierre o
  reemplazo determinístico compatible con el ownership existente de
  `textEditorOpen`.
- No muevas el editor a una variable global del último entry, no agregues
  listeners globales por card y no pongas contenido de captura en eventos,
  logs o payloads.

### Tests y verificación

Agrega regresiones con dos entradas elegibles para cubrir A → B, apertura,
cierre, reapertura, guardado sólo en B, target faltante, evento duplicado y
modal abierto. Verifica que siguen funcionando el matcher multiplataforma, el
foco, la accesibilidad y los baselines protegidos de cards y drag-and-drop.

Ejecuta y registra evidencia en las tareas 6.1–6.5:

```bash
cd app/tauri/frontend
npm run check
npm run build
npm test
cd ../../..
openspec validate editable-text-shortcut-fix --strict --type change
git diff --check
```

No modifiques `~/.clipvault` ni `~/.clipvault/assets`, no agregues
dependencias, red o telemetría y no ejecutes `opsx-sync` ni `opsx-archive`.
