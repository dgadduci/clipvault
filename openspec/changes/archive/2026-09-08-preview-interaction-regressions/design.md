# Diseño: preview-interaction-regressions

## 1. Whitespace del preview rich text

`ClipboardPreview.svelte` continúa siendo el único renderer de preview usado
por Desktop y Quick Paste. El flujo de rich text será:

```text
rich_preview_ref
  → bridge local validado
  → sanitización existente
  → representación segura para preview
  → renderer compartido con white-space preservado
```

La representación visible debe conservar:

- saltos `LF` y `CRLF` como separadores de línea;
- tabulaciones como espacios visuales con un `tab-size` estable;
- líneas vacías consecutivas;
- espacios relevantes al comienzo y al final de un bloque cuando formen parte
  del texto capturado.

El renderer puede utilizar CSS `white-space: pre-wrap` o una transformación
equivalente aplicada después de sanitizar, siempre que no convierta el HTML
original en contenido activo. Los elementos de formato permitidos —por
ejemplo párrafos, listas, negrita, cursiva, subrayado y color— deben conservar
su presentación. No se debe aplicar `trim`, colapsar whitespace ni usar el
preview truncado de una fila como fuente del modal.

Si el rich preview no existe o falla, el fallback plain text también debe
preservar whitespace mediante un `<pre>` o una regla equivalente. La corrección
no debe alterar el contenido almacenado ni la acción de paste.

## 2. Orden determinista de Quick Paste

La función pura que produce los ids visibles debe recibir los records ya
cargados y devolver un nuevo orden sin mutar sus entradas.

En modo `recent`:

```text
favoritos:     created_at DESC, id DESC
no favoritos:  created_at DESC, id DESC
resultado:     favoritos + no favoritos
```

En modo `search`, se conserva el orden de ranking recibido por
`SearchService`; el filtro por texto no se reordena por fecha. El orden debe
ser estable después de:

- carga inicial;
- refresh por `history-updated`;
- finalización de hidratación de lenguaje o iconos;
- pin/unpin;
- copiar una entrada sin crear una nueva entrada de historial;
- limpiar la búsqueda y volver a recientes.

El pin/unpin puede cambiar una entrada de grupo, pero no debe cambiar su
posición cronológica dentro del grupo destino. La selección por id debe
conservarse cuando una entrada cambia de grupo.

## 3. Selección local del Desktop

`App.svelte` o el propietario actual de la rail debe mantener un único
`selectedEntryId: number | null`. `HistoryCardRail` pasa a las cards:

- si están seleccionadas;
- una callback de selección;
- la plataforma del atajo si la card necesita mostrar el hint.

La superficie no interactiva de `HistoryCard` debe tener un handler de click
que:

```text
click en otra card       → selectedEntryId = esa id
click en la card activa  → selectedEntryId = null
click en control         → no cambia selección; ejecuta su acción actual
Escape en rail           → selectedEntryId = null
```

El doble click del título debe seguir llegando al editor existente. El gestor
de drag-and-drop sigue siendo el controlador singleton actual: el click de
selección no debe usar HTML5 drag, no debe modificar el payload y no debe
capturar el puntero antes del umbral.

La card seleccionada debe exponer `aria-selected="true"` o un contrato
equivalente sobre el elemento con `role="listitem"`/card, un estilo visible y
un `data-testid` estable. La selección no se envía al backend y desaparece si
la entrada deja de estar visible por colección, búsqueda, borrado o refresh.

## 4. Hint y shortcut de preview

El Desktop reutiliza el matcher existente `matchesPreviewShortcut` y la
resolución de plataforma existente. Sólo la card cuyo id coincide con
`selectedEntryId` muestra el hint visual:

- macOS: `Previsualizar · ⌘↵`;
- Linux: `Previsualizar · Ctrl↵`.

El hint no debe cambiar el tamaño fijo de la card ni desplazar pin, menú,
drag-handle o cualquier control. Se debe ocultar al deseleccionar y al cambiar
de scope. El keydown del preview sólo debe abrir la preview de la card
seleccionada y debe ignorarse dentro de inputs, el editor de título, el menú y
otros controles interactivos.

## 5. Menú de acciones de HistoryCard

El menú debe mantener una sola instancia abierta y reutilizar el estado
`openCardId` de la rail. La superficie visual del popover se debe montar en un
portal lógico fuera del `article.card` o usar una capa equivalente que no
quede limitada por `overflow`, `transform` o la altura fija de la card.

Al abrirse:

1. se mide el trigger;
2. se mide o estima el menú;
3. se elige arriba/abajo y izquierda/derecha según el viewport;
4. se limita la posición a un margen seguro;
5. si la altura disponible no alcanza, se aplica `max-height` y `overflow-y:
   auto` sólo al menú.

Escape, click exterior, cambio de card y refresh deben cerrar el popover sin
dejar listeners duplicados. Abrir el menú no debe seleccionar la card ni
iniciar drag.

Cada menu item con shortcut debe tener:

- label de acción;
- hint visual alineado a la derecha;
- `aria-keyshortcuts` equivalente;
- el matcher o handler existente, no una segunda implementación.

El item `Previsualizar` muestra `⌘↵`/`Ctrl↵`. Los items sin shortcut no
reciben texto decorativo ni una combinación inventada.

## 6. No regresiones y privacidad

- No modificar SQLite, `EntryRecord`, `asset_ref`, hashes ni timestamps.
- No incluir contenido, snippets, bytes, hashes, rutas o referencias opacas en
  atributos DOM, logs, eventos o payloads de selección.
- No tocar `pointerDragAndDrop.ts`, su fallback de mouse, pointer capture,
  ghost, selection lock, touch-action ni drop en colecciones scrolleables salvo
  para añadir una prueba de no-regresión.
- Las imágenes antiguas deben seguir cargando después de refresh/remount y la
  preview debe liberar Blob URLs.

## 7. Tests requeridos

### Rich preview

- HTML/RTF sanitizado conserva saltos, tabs, líneas vacías e indentación.
- fallback plain text conserva los mismos caracteres.
- formatos permitidos siguen visibles y contenido activo sigue bloqueado.
- Desktop y Quick Paste usan el mismo renderer/helper.

### Quick Paste

- recientes se ordenan por `created_at DESC` e `id DESC` dentro de favoritos y
  no favoritos;
- búsqueda conserva ranking;
- refresh, hidratación, pin/unpin y limpieza de query no rompen el orden;
- la selección sigue al id cuando cambia de grupo.

### Desktop selection

- click selecciona y otro click deselecciona;
- click en otra card mueve la selección;
- Escape limpia la selección;
- controles interactivos no seleccionan accidentalmente;
- sólo la card seleccionada muestra el hint y `Cmd/Ctrl+Enter` abre su preview;
- selección no muta el backend.

### Menú

- el menú muestra todas las acciones cuando la card está cerca de cada borde;
- el menú no aumenta card/rail y sólo el popover tiene scroll si hace falta;
- se mantiene una sola instancia y cleanup idempotente;
- `Previsualizar` muestra el shortcut correcto y los items sin shortcut no
  muestran hints falsos.

### Regresiones protegidas

- imágenes persistidas y preview compartido;
- rich text, paste plain/rich y títulos;
- tags, colecciones, favoritos, búsqueda y filtro de aplicación;
- drag-and-drop pointer/mouse, ghost, selección de texto y drop scrolleable;
- privacidad metadata-only.
