# Propuesta: preview, orden y selección de cards

## Problema

Se detectaron seis problemas relacionados con superficies que comparten la
misma información de captura:

1. el preview de texto enriquecido colapsa saltos de línea y tabulaciones;
2. la lista de Quick Paste no conserva un orden cronológico visible;
3. el menú de acciones de una card queda limitado por el alto de la card y
   algunas opciones quedan fuera de pantalla;
4. una card del Desktop no puede quedar seleccionada mediante click;
5. al no existir selección visual, el usuario no sabe cuándo puede usar el
   atajo de preview;
6. los items del menú que sí tienen atajo no lo comunican visualmente.

Estos problemas deben corregirse sin reimplementar el preview, el pegado, la
persistencia o el drag-and-drop.

## Objetivos

- Mantener intactos los caracteres de whitespace del contenido rich text en el
  preview, incluyendo `\\n`, `\\r\\n`, tabulaciones y líneas vacías.
- Mantener el formato permitido y la sanitización actual del rich preview.
- Hacer determinista el orden del modo reciente de Quick Paste.
- Dar a la card del Desktop un estado de selección local y accesible.
- Mostrar `⌘↵` en macOS o `Ctrl↵` en Linux cuando una card está seleccionada.
- Mostrar el mismo atajo en el item de menú `Previsualizar` y en cualquier
  acción futura que tenga un atajo real.
- Mantener el menú completo visible sin aumentar el tamaño de la card, la rail
  ni la ventana principal.

## Decisiones de producto

### Orden de Quick Paste

Se conserva la decisión existente de mostrar favoritos primero. En el modo
reciente, tanto el grupo de favoritos como el grupo no favorito se ordenan por
`created_at DESC` y luego `id DESC`. Así una captura más nueva nunca aparece
después de una más antigua dentro del mismo grupo.

El modo búsqueda conserva el ranking de `SearchService`; este cambio no
reemplaza relevancia por fecha cuando el usuario está buscando.

### Selección de cards

La selección es visual y local al Desktop; no se persiste en SQLite ni cambia
la captura. Un click sobre la superficie no interactiva selecciona la card,
un click sobre otra mueve la selección y un segundo click sobre la card ya
seleccionada la deselecciona. `Escape` también limpia la selección cuando el
foco está en el rail. Pin, menú, edición de título, tags, colecciones, paste y
drag-and-drop conservan sus acciones independientes.

### Atajo de preview

Se reutiliza el atajo ya definido por el producto: `Cmd+Enter` en macOS y
`Ctrl+Enter` en Linux. La card seleccionada muestra una indicación compacta y
estable, por ejemplo `Previsualizar · ⌘↵` o `Previsualizar · Ctrl↵`, sin
modificar las dimensiones fijas de la card.

### Menú no recortado

El menú de acciones debe renderizarse como un popover posicionado respecto de
la card, pero fuera del contexto que lo recorta. Debe calcular una posición
segura dentro del viewport y usar scroll interno sólo si la altura disponible
no alcanza. No se debe aumentar la altura de la card, de la rail ni del
Desktop para mostrar el menú.

### Preview rich text

El preview seguirá pasando por la representación sanitizada existente. La
corrección se realiza en el renderer compartido y en sus estilos/transformación
segura de whitespace; nunca se debe cargar HTML original sin sanitizar ni
guardar HTML generado adicionalmente.

## Alcance

Incluye:

- `ClipboardPreview.svelte` y sus helpers de whitespace/preview;
- ordenamiento puro de Quick Paste y sus estados de refresh, búsqueda,
  hidratación y pin;
- selección local en `App.svelte`/`HistoryCardRail.svelte`/`HistoryCard.svelte`;
- popover de menú de card, límites del viewport y labels de atajo;
- tests frontend y de regresión de imágenes, rich text, título, tags,
  colecciones, favoritos, búsqueda y drag-and-drop.

No incluye:

- nuevas migraciones, campos SQLite o comandos Tauri;
- cambios en el algoritmo de búsqueda o su ranking;
- cambios en la semántica de paste/copy o pegado sintético;
- cambio del contenido original, hashes, referencias de assets o timestamps;
- reimplementación del controlador singleton de drag-and-drop;
- favoritos, tags, colecciones, edición de títulos o retención;
- red, telemetría, embeddings, LLM o dependencias innecesarias.

## Criterios de aceptación

- Un rich preview con indentación, tabs y líneas vacías se visualiza igual que
  el contenido capturado, conservando el formato permitido.
- La vista reciente de Quick Paste queda ordenada de más reciente a más
  antigua dentro de sus grupos de favoritos/no favoritos.
- El menú de cualquier card muestra todas sus acciones y, si el viewport es
  insuficiente, sólo el menú tiene scroll.
- Click, segundo click, cambio de card y Escape producen un estado de
  selección claro y accesible.
- Sólo la card seleccionada muestra el atajo de preview y el preview se abre
  para esa card.
- Cada acción con atajo muestra el atajo correcto sin inventar atajos para
  acciones que no los tienen.
