# Design: quick-paste-compact-ui

## Principio de implementación

Este cambio es una evolución de `QuickPaste.svelte` y de sus helpers
existentes. No se debe eliminar ni duplicar la ventana `quick-paste`, el
listener `clipvault://quick-search`, `runSearch`, `performPasteFlow` ni los
comandos Tauri actuales. La lógica de negocio permanece en el core y la UI
consume los `EntryRecord` ya existentes.

## Ventana

La configuración de la ventana transient debe usar:

- ancho: `720` píxeles lógicos;
- alto: `520` píxeles lógicos;
- `resizable: false`;
- decoraciones desactivadas;
- `alwaysOnTop: true`;
- `skipTaskbar: true` cuando la plataforma lo permita;
- oculta al iniciar.

La ventana debe centrarse al activarse. Se debe usar la capacidad de
posicionamiento de Tauri disponible para la ventana y centrarla en el monitor
actual; si el host no permite determinarlo, se acepta el centro del monitor
principal. El centrado no debe introducir un nuevo probe de plataforma ni
alterar la captura del target previo.

El orden funcional existente se conserva:

```text
captureActiveApp → posicionar/centrar → show → focus → emitOpened
```

El posicionamiento puede ocurrir antes de `show`, pero nunca debe romper el
orden `show → focus → emitOpened` que ya protege el flujo de activación.

## Jerarquía visual

La barra de búsqueda es el encabezado principal. Se elimina el encabezado
grande `ClipVault Quick Paste` o se reduce a una identificación visual mínima
que no consuma una fila de contenido. La búsqueda recibe el foco cuando la
ventana se abre y conserva un tamaño cómodo para escribir.

El área de resultados es el único contenedor con scroll vertical. La ventana y
la lista no deben generar overflow horizontal.

## Geometría fija de los items

Cada resultado se representa como un item tipo card de ancho igual al área
interior de la lista y altura fija documentada por el CSS. Como valor inicial
se usará una altura de `72px`, con separación constante. El ancho se calcula
una sola vez a partir del contenedor fijo de `720px`; no se implementará un
layout que crezca según el texto.

Todos los estados deben conservar esa geometría: normal, hover, seleccionado,
focus, loading, error y menú futuro. Ningún cambio de fuente, miniatura o
estado puede desplazar las filas restantes.

La composición interna tendrá dos líneas:

```text
línea 1: icono de tipo · título · icono de aplicación fuente
línea 2: preview/miniatura · tiempo transcurrido y metadata segura
```

Las columnas reservadas para iconos y controles deben tener tamaños fijos.
Los títulos y previews largos deben truncarse con elipsis. El contenido no
debe seleccionar texto accidentalmente durante la navegación o el futuro
arrastre de cards del desktop.

Las imágenes deben usar una miniatura dentro de un rectángulo fijo con
`object-fit: cover`. Mientras `clipvault_clipboard_asset` resuelve, el
placeholder debe ocupar exactamente el mismo espacio; una respuesta obsoleta
no puede reemplazar una miniatura de otra entrada.

## Tipografía y accesibilidad

La tipografía será compacta, sin sacrificar legibilidad ni foco visible. Como
referencia inicial:

- título: `13–14px`;
- preview: `12–13px`;
- metadata: `11–12px`;
- búsqueda: `15–16px`.

Las medidas finales pueden ajustarse visualmente, pero deben permanecer
pequeñas y homogéneas. Los iconos deben conservar `aria-label` o texto
accesible, aunque visualmente sólo se muestre el icono de la aplicación.

La fila seleccionada debe tener un contraste inequívoco y un indicador de
focus visible. Historial vacío, búsqueda sin resultados, carga de imagen y
error de búsqueda deben ocupar estados con dimensiones estables.

## Compatibilidad y no regresiones

- El hotkey global y el evento de activación siguen siendo los actuales.
- La búsqueda continúa reutilizando `recentEntriesCommand`, `runSearch` y
  `searchEntriesCommand`.
- `ArrowUp`, `ArrowDown`, `Home`, `End`, `Enter` y `Escape` mantienen el
  contrato actual hasta que `quick-paste-actions` agregue sus modalidades.
- `Enter` sigue usando el flujo de pegado existente para la selección actual.
- El ocultamiento antes de pegar, la guidance y la restauración del foco no se
  modifican.
- Las imágenes antiguas se cargan mediante el bridge y resolver existentes;
  no se cambian `asset_ref`, namespaces, blobs ni rutas.
- No se emiten query, preview, contenido, hashes, rutas ni identificadores de
  aplicación en eventos o logs.

## Tests

Los tests deben poder verificar sin un display real:

- configuración de ventana `720 × 520`, no redimensionable y centrado;
- búsqueda enfocada al abrir;
- lista vertical con overflow interno y ausencia de overflow horizontal;
- altura fija y layout de dos líneas para texto e imagen;
- truncado sin crecimiento del item;
- placeholder de imagen con las mismas dimensiones que la miniatura;
- estados vacío, sin resultados, loading y error;
- navegación y Escape existentes;
- preservación de `asset_ref`, carga de imágenes y flujo de pegado;
- ausencia de listeners duplicados y ausencia de datos sensibles en eventos.
